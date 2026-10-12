# TypeScript

A local stdio MCP server with four tools:
`list_events`, `create_event`, `update_event`, and `delete_event`.
Tasks are excluded. Updates preserve omitted fields; deletion needs `confirm: true`.

From the repository root (Node.js 20+):

```bash
cargo build --locked --no-default-features --bin calendar-mcp
npm install @modelcontextprotocol/sdk@1 zod
npm install --save-dev tsx typescript @types/node
```

Save as `client.ts`. This example uses a separate `mcp-demo.json` file.

```typescript
import { resolve } from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

async function main() {
    const client = new Client({ name: "calendar-demo", version: "1.0.0" });
    const transport = new StdioClientTransport({
        command: resolve("target/debug/calendar-mcp"),
        env: { CALENDAR_GPUI_DATA_FILE: resolve("mcp-demo.json") },
    });

    try {
        await client.connect(transport);

        async function call(name: string, args: Record<string, unknown> = {}) {
            const result = await client.callTool({ name, arguments: args });
            if (result.isError) throw new Error(JSON.stringify(result.content));
            return result.structuredContent as Record<string, unknown>;
        }

        const created = await call("create_event", {
            title: "Demo meeting",
            start: "2026-10-12T09:00:00+02:00",
            end: "2026-10-12T10:00:00+02:00",
            timezone: "Europe/Paris",
        });
        const { id } = created.event as { id: number };

        console.log(await call("list_events"));
        await call("update_event", { id, title: "Updated demo" });
        await call("delete_event", { id, confirm: true });
    } finally {
        await client.close();
    }
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
```

```bash
node --import tsx client.ts
```

Use the same data path as the desktop app to access your real calendar.
Omit the `env` property to use the default calendar file.
The app refreshes external changes every second; the server also works when it is closed.
Use RFC 3339 timestamps with an offset or `Z`; dates must be within 1900–2100.
Optional creation fields: `description`, `timezone`, and `url`.
Ask the user before deleting an existing event. Calls wait up to five seconds for a busy calendar,
then return an error; retry the call if the other writer is still active.

SDK reference: [TypeScript MCP client](https://github.com/modelcontextprotocol/typescript-sdk/blob/v1.x/docs/client.md).
