import test from "node:test";
import assert from "node:assert/strict";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { BRIDGE } from "../src/config.mjs";
import { TOOLS } from "../src/common.mjs";
import { fixture, token } from "./helpers.mjs";
test("real MCP SDK stdio list/call, capability-only HTTP, bounded allowlist/schema denial", async (t) => {
  const f = await fixture(t);
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [BRIDGE],
    env: {
      PATH: process.env.PATH,
      PHONE_ASSISTANT_API_URL: f.apiUrl,
      PHONE_ASSISTANT_CAPABILITY: token,
    },
    stderr: "pipe",
  });
  const client = new Client(
    { name: "phone-test", version: "1" },
    { capabilities: {} },
  );
  await client.connect(transport);
  t.after(() => client.close());
  const catalogue = await client.listTools();
  assert.deepEqual(
    catalogue.tools.map((t) => t.name),
    TOOLS,
  );
  const result = await client.callTool({
    name: "upsert_filter",
    arguments: { value: "synthetic" },
  });
  assert.equal(JSON.parse(result.content[0].text).changed, "upsert_filter");
  const count = f.calls.length;
  assert.equal(
    (
      await client.callTool({
        name: "exec_command",
        arguments: { cmd: "echo nope" },
      })
    ).isError,
    true,
  );
  assert.equal(
    (
      await client.callTool({
        name: "upsert_filter",
        arguments: { organization_id: "foreign" },
      })
    ).isError,
    true,
  );
  assert.equal(f.calls.length, count);
  const write = f.calls.find((c) => c.body.method === "call");
  assert.match(write.body.call_id, /^[0-9a-f-]{36}$/);
  assert.equal(write.authorization, "Bearer " + token);
  assert.ok(!JSON.stringify(result).includes(token));
  for (let i = 3; i < 24; i++)
    await client.callTool({ name: "exec_command", arguments: { cmd: "nope" } });
  const exceeded = await client.callTool({
    name: "upsert_filter",
    arguments: { value: "synthetic" },
  });
  assert.equal(
    JSON.parse(exceeded.content[0].text).error.code,
    "assistant_tool_budget_reached",
  );
  assert.equal(f.calls.length, count);
});
