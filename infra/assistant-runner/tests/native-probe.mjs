import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import crypto from "node:crypto";
import assert from "node:assert/strict";
import { CodexRpc } from "../src/rpc.mjs";
import { configText } from "../src/config.mjs";
import { TOOLS } from "../src/common.mjs";
import { writePrivate } from "../src/storage.mjs";
export async function nativeProbe() {
  const directory = await fs.mkdtemp(
    path.join(os.tmpdir(), "phone-codex-native-"),
  );
  await fs.chmod(directory, 0o700);
  const home = path.join(directory, "home"),
    work = path.join(directory, "work");
  await fs.mkdir(home, { mode: 0o700 });
  await fs.mkdir(work, { mode: 0o700 });
  const requests = [],
    toolCalls = [];
  const marker = path.join(work, "forbidden-exec-marker");
  let rpc, timer;
  const server = http.createServer(async (req, res) => {
    let raw = "";
    for await (const part of req) raw += part;
    const body = JSON.parse(raw);
    if (req.url === "/api/assistant/tools") {
      let data;
      if (body.method === "list")
        data = {
          tools: TOOLS.map((name) => ({
            name,
            description: "Offline fixture " + name,
            inputSchema: {
              type: "object",
              additionalProperties: false,
              properties: {},
            },
          })),
        };
      else {
        toolCalls.push(body);
        data = { result: { revision: 1, synthetic: true } };
      }
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ data }));
      return;
    }
    requests.push(body);
    const n = requests.length;
    let item;
    if (n === 1) {
      const namespace = body.tools.find(
        (x) => x.type === "namespace" && x.name === "mcp__phone_platform",
      );
      if (
        !namespace?.tools.some((x) => x.name === "get_workspace_configuration")
      ) {
        res.writeHead(500);
        res.end();
        return;
      }
      item = {
        type: "function_call",
        id: "fc_mcp",
        call_id: "call_mcp",
        name: "get_workspace_configuration",
        namespace: "mcp__phone_platform",
        arguments: "{}",
      };
    } else if (n === 2)
      item = {
        type: "function_call",
        id: "fc_forbidden",
        call_id: "call_forbidden",
        name: "exec_command",
        arguments: JSON.stringify({
          cmd: "printf FORBIDDEN_OS_EXEC; touch " + marker,
        }),
      };
    else
      item = {
        type: "message",
        id: "msg_final",
        role: "assistant",
        status: "completed",
        content: [
          {
            type: "output_text",
            text: "offline probe complete",
            annotations: [],
          },
        ],
      };
    const response = {
      id: "resp_" + n,
      object: "response",
      created_at: 0,
      model: "offline-probe",
      status: "completed",
      output: [item],
      usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2 },
    };
    const events = [
      {
        type: "response.created",
        response: { ...response, status: "in_progress", output: [] },
      },
      {
        type: "response.output_item.added",
        output_index: 0,
        item: item.type === "function_call" ? { ...item, arguments: "" } : item,
      },
      { type: "response.output_item.done", output_index: 0, item },
      { type: "response.completed", response },
    ];
    const text = events
      .map(
        (event) =>
          "event: " + event.type + "\ndata: " + JSON.stringify(event) + "\n\n",
      )
      .join("");
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Content-Length": Buffer.byteLength(text),
    });
    res.end(text);
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const base = "http://127.0.0.1:" + server.address().port;
  try {
    await writePrivate(
      path.join(home, "config.toml"),
      configText({
        apiUrl: base,
        capability: "native-probe-only-" + crypto.randomUUID(),
        probeProvider: base,
      }),
    );
    rpc = new CodexRpc(home, work);
    await rpc.initialize();
    const config = await rpc.request("config/read", { includeLayers: false });
    assert.equal(config.config.features.shell_tool, false);
    assert.equal(config.config.web_search, "disabled");
    const account = await rpc.request("account/read", { refreshToken: false });
    assert.equal(account.account, null);
    const thread = await rpc.request("thread/start", {
      model: "offline-probe",
      modelProvider: "offline_probe",
      ephemeral: true,
      sandbox: "read-only",
      approvalPolicy: "never",
      cwd: work,
      environments: [],
      runtimeWorkspaceRoots: [],
    });
    assert.deepEqual(thread.thread.environments, []);
    assert.deepEqual(thread.runtimeWorkspaceRoots, []);
    assert.equal(thread.thread.path, null);
    assert.deepEqual(thread.sandbox, {
      type: "readOnly",
      networkAccess: false,
    });
    let resolve, reject;
    const completed = new Promise((yes, no) => {
      resolve = yes;
      reject = no;
    });
    completed.catch(() => {});
    timer = setTimeout(() => reject(new Error("offline_probe_timeout")), 30000);
    rpc.on("fault", reject);
    rpc.on("notification", (message) => {
      if (message.method === "turn/completed") resolve(message.params.turn);
    });
    await rpc.request("turn/start", {
      threadId: thread.thread.id,
      input: [{ type: "text", text: "Offline boundary fixture only." }],
      environments: [],
      runtimeWorkspaceRoots: [],
      sandboxPolicy: { type: "readOnly", networkAccess: false },
    });
    const turn = await completed;
    clearTimeout(timer);
    assert.equal(turn.status, "completed");
    assert.equal(
      toolCalls.length,
      1,
      "Native MCP invocation did not reach the fixed gateway",
    );
    assert.equal(toolCalls[0].name, "get_workspace_configuration");
    const names = requests[0].tools.map((tool) => tool.name);
    for (const tool of requests[0].tools) {
      assert.ok(
        [
          "request_user_input",
          "list_mcp_resources",
          "list_mcp_resource_templates",
          "read_mcp_resource",
          "mcp__phone_platform",
        ].includes(tool.name),
        String(tool.name),
      );
      if (tool.name === "mcp__phone_platform")
        assert.deepEqual(
          tool.tools.map((t) => t.name).sort(),
          [...TOOLS].sort(),
        );
    }
    const outputs = requests
      .slice(1)
      .flatMap((body) => body.input ?? [])
      .filter((item) => item.type === "function_call_output")
      .map((item) => item.output);
    assert.ok(outputs.includes("unsupported call: exec_command"));
    assert.equal(
      await fs.stat(marker).then(
        () => true,
        () => false,
      ),
      false,
    );
    return {
      codex_version: "0.167.2",
      account_unconfigured: true,
      ephemeral_thread: true,
      environment_capabilities: 0,
      actual_sdk_mcp_call: true,
      forbidden_exec_rejected: true,
      forbidden_tool_advertised: false,
      external_model_requests: 0,
      loopback_responses_requests: requests.length,
    };
  } finally {
    clearTimeout(timer);
    await rpc?.dispose();
    server.closeAllConnections();
    await new Promise((r) => server.close(r));
    await fs.rm(directory, { recursive: true, force: true });
  }
}
