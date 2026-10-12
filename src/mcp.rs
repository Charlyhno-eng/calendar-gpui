//! Minimal local MCP server: newline-delimited JSON-RPC over stdio.
use std::{
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::calendar::*;

const VERSION: &str = "2025-11-25";
const MAX_MESSAGE: u64 = 65_536;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    title: Option<String>,
    description: Option<String>,
    start: Option<String>,
    end: Option<String>,
    timezone: Option<String>,
    url: Option<String>,
}

impl Fields {
    fn apply(self, draft: &mut ItemDraft) -> Result<()> {
        if let Some(title) = self.title {
            draft.title = title;
        }
        if let Some(description) = self.description {
            draft.description = description;
        }
        if let Some(start) = self.start {
            draft.start = timestamp(&start)?;
        }
        if let Some(end) = self.end {
            draft.end = timestamp(&end)?;
        }
        if let Some(timezone) = self.timezone {
            draft.timezone = parse_zone(&timezone)?;
        }
        if let Some(url) = self.url {
            draft.url = url;
        }
        draft.validate()
    }
}

fn timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| {
            CalendarError("Use an RFC 3339 timestamp with an explicit offset or Z.".into())
        })
}

fn arguments<T: for<'a> Deserialize<'a>>(value: Value) -> Result<T> {
    serde_json::from_value(value)
        .map_err(|error| CalendarError(format!("Invalid arguments: {error}")))
}

fn event(calendar: &Calendar, id: u64) -> Result<&CalendarItem> {
    let item = calendar.item(id)?;
    if item.kind() != ItemKind::Event {
        return Err(CalendarError(
            "This ID belongs to a task, not an event.".into(),
        ));
    }
    Ok(item)
}

/// Every mutation reloads the latest document and holds its lock until saved.
pub fn call_tool(path: &Path, name: &str, mut args: Value) -> Result<Value> {
    if !args.is_object() {
        return Err(CalendarError("Arguments must be an object.".into()));
    }
    // Reject null values as well as wrong types; omitted update fields are preserved.
    if args
        .as_object()
        .is_some_and(|object| object.values().any(Value::is_null))
    {
        return Err(CalendarError(
            "Omit optional fields instead of using null.".into(),
        ));
    }
    match name {
        "list_events" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Empty {}
            let _: Empty = arguments(args)?;
            let calendar = CalendarStore::load(path)?;
            let mut events: Vec<_> = calendar
                .items()
                .iter()
                .filter(|item| item.kind() == ItemKind::Event)
                .collect();
            events.sort_by_key(|item| (item.start(), item.id()));
            Ok(json!({"events": events}))
        }
        "create_event" => {
            for field in ["title", "start", "end"] {
                if args.get(field).is_none() {
                    return Err(CalendarError(format!("Missing {field}.")));
                }
            }
            let fields: Fields = arguments(args)?;
            let (_, value) = CalendarStore::transact(path, |calendar| {
                let mut draft = ItemDraft {
                    title: String::new(),
                    description: String::new(),
                    kind: ItemKind::Event,
                    start: Utc::now(),
                    end: Utc::now(),
                    timezone: calendar.timezone(),
                    url: String::new(),
                    completed: false,
                };
                fields.apply(&mut draft)?;
                let id = calendar.create(draft)?;
                Ok(json!({"event": calendar.item(id)?}))
            })?;
            Ok(value)
        }
        "update_event" => {
            let id: u64 = arguments(
                args.as_object_mut()
                    .expect("checked object")
                    .remove("id")
                    .unwrap_or(Value::Null),
            )?;
            if args.as_object().is_some_and(|object| object.is_empty()) {
                return Err(CalendarError(
                    "Provide at least one field to update.".into(),
                ));
            }
            let fields: Fields = arguments(args)?;
            let (_, value) = CalendarStore::transact(path, |calendar| {
                let mut draft = event(calendar, id)?.draft();
                fields.apply(&mut draft)?;
                calendar.update(id, draft)?;
                Ok(json!({"event": calendar.item(id)?}))
            })?;
            Ok(value)
        }
        "delete_event" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Delete {
                id: u64,
                confirm: bool,
            }
            let args: Delete = arguments(args)?;
            if !args.confirm {
                return Err(CalendarError("Deletion requires confirm: true.".into()));
            }
            CalendarStore::transact(path, |calendar| {
                event(calendar, args.id)?;
                calendar.delete(args.id)
            })?;
            Ok(json!({"deleted_id": args.id}))
        }
        _ => Err(CalendarError(format!("Unknown tool: {name}"))),
    }
}

pub fn tools() -> Value {
    let properties = json!({
        "title": {"type":"string", "minLength":1, "maxLength":200},
        "description": {"type":"string", "maxLength":10000},
        "start": {"type":"string", "format":"date-time", "description":"RFC 3339 instant with an explicit offset or Z."},
        "end": {"type":"string", "format":"date-time", "description":"Exclusive end; must be after start."},
        "timezone": {"type":"string", "description":"IANA zone; defaults to the calendar zone on creation."},
        "url": {"type":"string", "description":"HTTP/HTTPS link, or an empty string."}
    });
    let mut update = properties.clone();
    update["id"] = json!({"type":"integer", "minimum":1});
    let schema = |properties: Value, required: Value| {
        json!({
            "type":"object", "properties":properties, "required":required, "additionalProperties":false
        })
    };
    let tool = |name: &str, description: &str, input: Value, readonly: bool, destructive: bool| {
        json!({
            "name":name, "description":description, "inputSchema":input,
            "annotations":{"readOnlyHint":readonly, "destructiveHint":destructive, "openWorldHint":false}
        })
    };
    json!({"tools":[
        tool("list_events", "List all events ordered by start time. Tasks are excluded.", schema(json!({}), json!([])), true, false),
        tool("create_event", "Create one timed event. Returns the saved event and its ID.", schema(properties, json!(["title","start","end"])), false, false),
        tool("update_event", "Update an event by ID. Omitted fields are preserved. Supply at least one changed field.", schema(update, json!(["id"])), false, true),
        tool("delete_event", "Permanently delete an event by ID. Obtain user approval before setting confirm to true.", schema(json!({"id":{"type":"integer","minimum":1},"confirm":{"type":"boolean","const":true}}), json!(["id","confirm"])), false, true)
    ]})
}

pub struct Server {
    path: PathBuf,
    initializing: bool,
    ready: bool,
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

impl Server {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            initializing: false,
            ready: false,
        }
    }

    pub fn handle(&mut self, request: Value) -> Option<Value> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str);
        if request.get("jsonrpc") != Some(&json!("2.0"))
            || method.is_none()
            || (request.get("id").is_some() && !(id.is_string() || id.is_i64() || id.is_u64()))
            || request
                .get("params")
                .is_some_and(|params| !params.is_object())
        {
            return Some(error(Value::Null, -32600, "Invalid JSON-RPC request."));
        }
        let method = method.expect("validated method");
        if request.get("id").is_none() {
            if method == "notifications/initialized" && self.initializing {
                self.ready = true;
            }
            return None;
        }
        let params = request.get("params").cloned().unwrap_or(json!({}));
        let result = match method {
            "initialize" => {
                if self.initializing {
                    return Some(error(id, -32600, "Already initialized."));
                }
                let Some(version) = params.get("protocolVersion").and_then(Value::as_str) else {
                    return Some(error(id, -32602, "Missing protocolVersion."));
                };
                if !params.get("capabilities").is_some_and(Value::is_object)
                    || !params.get("clientInfo").is_some_and(|info| {
                        info.get("name").is_some_and(Value::is_string)
                            && info.get("version").is_some_and(Value::is_string)
                    })
                {
                    return Some(error(id, -32602, "Missing capabilities or clientInfo."));
                }
                self.initializing = true;
                let version =
                    if ["2024-11-05", "2025-03-26", "2025-06-18", VERSION].contains(&version) {
                        version
                    } else {
                        VERSION
                    };
                json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"calendar-gpui","version":env!("CARGO_PKG_VERSION")}})
            }
            "ping" => json!({}),
            _ if !self.ready => return Some(error(id, -32600, "Initialize the session first.")),
            "tools/list" => {
                if params.as_object().is_some_and(|p| !p.is_empty()) {
                    return Some(error(id, -32602, "This server has no pagination."));
                }
                tools()
            }
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(error(id, -32602, "Missing tool name."));
                };
                if ![
                    "list_events",
                    "create_event",
                    "update_event",
                    "delete_event",
                ]
                .contains(&name)
                {
                    return Some(error(id, -32602, "Unknown tool."));
                }
                match call_tool(
                    &self.path,
                    name,
                    params.get("arguments").cloned().unwrap_or(json!({})),
                ) {
                    Ok(value) => {
                        json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                    }
                    Err(error) => {
                        json!({"content":[{"type":"text","text":error.to_string()}],"isError":true})
                    }
                }
            }
            _ => return Some(error(id, -32601, "Method not found.")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }

    pub fn run(&mut self, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
        loop {
            let mut bytes = Vec::new();
            let count = (&mut input)
                .take(MAX_MESSAGE + 1)
                .read_until(b'\n', &mut bytes)?;
            if count == 0 {
                return Ok(());
            }
            let response = if count as u64 > MAX_MESSAGE {
                // Stop on oversized input rather than interpreting its tail as another request.
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MCP message exceeds 64 KiB.",
                ));
            } else {
                match serde_json::from_slice(&bytes) {
                    Ok(request) => self.handle(request),
                    Err(_) => Some(error(Value::Null, -32700, "Invalid JSON.")),
                }
            };
            if let Some(response) = response {
                serde_json::to_writer(&mut output, &response)?;
                writeln!(output)?;
                output.flush()?;
            }
        }
    }
}
