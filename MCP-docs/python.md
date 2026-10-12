# Python

A local stdio MCP server with four tools:
`list_events`, `create_event`, `update_event`, and `delete_event`.
Tasks are excluded. Updates preserve omitted fields; deletion needs `confirm: true`.

From the repository root (Python 3.10+):

```bash
cargo build --locked --no-default-features --bin calendar-mcp
python3 -m venv .venv
.venv/bin/pip install 'mcp>=1,<2'
```

Save as `client.py`. This example uses a separate `mcp-demo.json` file.

```python
import asyncio
import json
import os
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def main():
    server = StdioServerParameters(
        command=str(Path("target/debug/calendar-mcp").resolve()),
        env={**os.environ, "CALENDAR_GPUI_DATA_FILE": str(Path("mcp-demo.json").resolve())},
    )

    async with stdio_client(server) as (read, write):
        async with ClientSession(read, write) as client:
            await client.initialize()

            async def call(name, **args):
                result = await client.call_tool(name, args)
                if result.isError:
                    raise RuntimeError(result.content[0].text)
                return json.loads(result.content[0].text)

            created = await call(
                "create_event",
                title="Demo meeting",
                start="2026-10-12T09:00:00+02:00",
                end="2026-10-12T10:00:00+02:00",
                timezone="Europe/Paris",
            )
            event_id = created["event"]["id"]

            print(await call("list_events"))
            await call("update_event", id=event_id, title="Updated demo")
            await call("delete_event", id=event_id, confirm=True)


asyncio.run(main())
```

```bash
.venv/bin/python client.py
```

Use the same data path as the desktop app to access your real calendar.
Omit the `env` argument to use the default calendar file.
The app refreshes external changes every second; the server also works when it is closed.
Use RFC 3339 timestamps with an offset or `Z`; dates must be within 1900–2100.
Optional creation fields: `description`, `timezone`, and `url`.
Ask the user before deleting an existing event. Calls wait up to five seconds for a busy calendar,
then return an error; retry the call if the other writer is still active.

SDK reference: [Python MCP client](https://github.com/modelcontextprotocol/python-sdk/blob/v1.x/docs/client.md).
