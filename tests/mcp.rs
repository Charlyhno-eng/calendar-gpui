use calendar_gpui::{
    calendar::*,
    mcp::{Server, call_tool},
};
use serde_json::{Value, json};
use std::{io::Cursor, path::Path};

fn input() -> Value {
    json!({"title":"Meeting", "description":"Roadmap", "start":"2026-10-25T02:30:00+02:00", "end":"2026-10-25T02:30:00+01:00", "timezone":"Europe/Paris", "url":"https://example.com"})
}

fn create(path: &Path) -> u64 {
    call_tool(path, "create_event", input()).unwrap()["event"]["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn events_round_trip_and_partial_updates_preserve_other_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let id = create(&path);
    let event = call_tool(&path, "list_events", json!({})).unwrap()["events"][0].clone();
    assert_eq!(event["start"], "2026-10-25T00:30:00Z");
    assert_eq!(event["end"], "2026-10-25T01:30:00Z");
    let updated = call_tool(&path, "update_event", json!({"id":id,"title":"Updated"})).unwrap();
    let mut expected = event;
    expected["title"] = json!("Updated");
    assert_eq!(updated["event"], expected);
    assert!(call_tool(&path, "delete_event", json!({"id":id,"confirm":false})).is_err());
    assert!(call_tool(&path, "delete_event", json!({"id":id})).is_err());
    assert_eq!(
        call_tool(&path, "delete_event", json!({"id":id,"confirm":true})).unwrap()["deleted_id"],
        id
    );
    assert_eq!(
        call_tool(&path, "list_events", json!({})).unwrap()["events"],
        json!([])
    );
    assert!(call_tool(&path, "delete_event", json!({"id":id,"confirm":true})).is_err());
}

#[test]
fn invalid_calls_do_not_change_the_saved_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let id = create(&path);
    let saved = std::fs::read(&path).unwrap();
    for change in [
        json!({"id":id,"title":"\nMeeting"}),
        json!({"id":id,"title":" "}),
        json!({"id":id,"title":null}),
        json!({"id":id,"end":"2026-10-24T00:00:00Z"}),
        json!({"id":id,"start":"2026-10-25T02:30:00"}),
        json!({"id":id,"start":"2101-01-01T00:00:00Z"}),
        json!({"id":id,"timezone":"invalid/zone"}),
        json!({"id":id,"url":"javascript:alert(1)"}),
        json!({"id":id,"typo":"changed"}),
        json!({"id":id}),
        json!({"id":-1,"title":"X"}),
        json!({"id":999,"title":"X"}),
    ] {
        assert!(
            call_tool(&path, "update_event", change.clone()).is_err(),
            "{change}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    for args in [
        json!({}),
        json!({"title":"X","start":"bad","end":"bad"}),
        json!([]),
    ] {
        assert!(call_tool(&path, "create_event", args).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    assert!(call_tool(&path, "list_events", json!({"unexpected":true})).is_err());
}

#[test]
fn tasks_are_never_changed_or_returned_by_event_tools() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let event_id = create(&path);
    let (_, task_id) = CalendarStore::transact(&path, |calendar| {
        let mut draft = calendar.item(event_id)?.draft();
        draft.kind = ItemKind::Task;
        calendar.create(draft)
    })
    .unwrap();
    assert_eq!(
        call_tool(&path, "list_events", json!({})).unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(call_tool(&path, "update_event", json!({"id":task_id,"title":"X"})).is_err());
    assert!(call_tool(&path, "delete_event", json!({"id":task_id,"confirm":true})).is_err());
    assert_eq!(CalendarStore::load(&path).unwrap().items().len(), 2);
}

#[test]
fn transactions_reload_and_failed_changes_leave_disk_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let first = create(&path);
    let stale = CalendarStore::load(&path).unwrap();
    let second = create(&path);
    CalendarStore::transact(&path, |calendar| {
        let mut draft = stale.item(first)?.draft();
        draft.title = "Desktop change".into();
        calendar.update(first, draft)
    })
    .unwrap();
    let latest = CalendarStore::load(&path).unwrap();
    assert!(latest.item(second).is_ok());
    assert_eq!(latest.item(first).unwrap().title(), "Desktop change");
    let saved = std::fs::read(&path).unwrap();
    let result: Result<(Calendar, ())> = CalendarStore::transact(&path, |calendar| {
        calendar.delete(first)?;
        Err(CalendarError("Failure".into()))
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    let (store, _) = CalendarStore::open(&path).unwrap();
    assert!(call_tool(&path, "create_event", input()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    drop(store);
    assert!(call_tool(&path, "create_event", input()).is_ok());
}

fn initialize(server: &mut Server) {
    let response = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap();
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
    assert!(
        server
            .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none()
    );
}

#[test]
fn protocol_lifecycle_errors_and_tool_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = Server::new(dir.path().join("calendar.json"));
    assert_eq!(
        server
            .handle(json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}))
            .unwrap()["error"]["code"],
        -32600
    );
    initialize(&mut server);
    let tools = server
        .handle(json!({"jsonrpc":"2.0","id":"list","method":"tools/list"}))
        .unwrap();
    assert_eq!(tools["id"], "list");
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);
    assert_eq!(
        server
            .handle(json!({"jsonrpc":"2.0","id":2,"method":"unknown"}))
            .unwrap()["error"]["code"],
        -32601
    );
    assert_eq!(
        server
            .handle(
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"unknown"}})
            )
            .unwrap()["error"]["code"],
        -32602
    );
    assert!(server.handle(json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"create_event","arguments":input()}})).is_none());
    let invalid = server.handle(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"create_event","arguments":{}}})).unwrap();
    assert_eq!(invalid["result"]["isError"], true);
    for value in [
        json!([]),
        json!({"id":1,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
    ] {
        assert_eq!(server.handle(value).unwrap()["error"]["code"], -32600);
    }
}

#[test]
fn transport_handles_bad_json_eof_notifications_and_limits() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = Server::new(dir.path().join("calendar.json"));
    let mut output = Vec::new();
    server.run(Cursor::new(b"bad\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\"}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n"), &mut output).unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["result"], json!({}));
    assert!(
        server
            .run(Cursor::new(vec![b' '; 65_537]), Vec::new())
            .is_err()
    );
}

#[test]
fn stdio_binary_shares_persisted_events_between_processes() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let run = |args: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_calendar-mcp"))
            .env("CALENDAR_GPUI_DATA_FILE", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":args}),
        ] {
            writeln!(stdin, "{message}").unwrap();
        }
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        serde_json::from_str::<Value>(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .last()
                .unwrap(),
        )
        .unwrap()["result"]
            .clone()
    };
    let created = run(json!({"name":"create_event","arguments":input()}));
    assert_eq!(created["isError"], false);
    let id = created["structuredContent"]["event"]["id"].clone();
    assert_eq!(
        run(json!({"name":"list_events","arguments":{}}))["structuredContent"]["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        run(json!({"name":"update_event","arguments":{"id":id,"title":"Changed"}}))["structuredContent"]
            ["event"]["title"],
        "Changed"
    );
    assert_eq!(
        run(json!({"name":"delete_event","arguments":{"id":id,"confirm":true}}))["isError"],
        false
    );
}

#[test]
fn mcp_waits_for_a_slow_writer_without_losing_its_changes() {
    use std::{sync::mpsc, time::Duration};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let (store, mut calendar) = CalendarStore::open(&path).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let client_path = path.clone();
    let client = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        result_tx
            .send(call_tool(&client_path, "create_event", input()))
            .unwrap();
    });
    started_rx.recv().unwrap();
    // A brief slow save must leave the request pending, rather than fail at 500 ms.
    assert!(matches!(
        result_rx.recv_timeout(Duration::from_millis(750)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    calendar.set_show_holidays(false);
    store.save(&calendar).unwrap();
    drop(store);

    let created = result_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    client.join().unwrap();
    let saved = CalendarStore::load(&path).unwrap();
    assert!(!saved.show_holidays());
    assert_eq!(saved.items().len(), 1);
    assert_eq!(
        saved.items()[0].id(),
        created["event"]["id"].as_u64().unwrap()
    );
}

#[test]
fn concurrent_clients_serialize_writes_without_losing_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let clients: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let mut ids = Vec::new();
                for _ in 0..5 {
                    ids.push(create(&path));
                    assert!(
                        !call_tool(&path, "list_events", json!({})).unwrap()["events"]
                            .as_array()
                            .unwrap()
                            .is_empty()
                    );
                }
                ids
            })
        })
        .collect();
    let mut ids: Vec<_> = clients
        .into_iter()
        .flat_map(|client| client.join().unwrap())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 20);
    assert_eq!(CalendarStore::load(&path).unwrap().items().len(), 20);
}
