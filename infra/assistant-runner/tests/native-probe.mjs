import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import crypto from "node:crypto";
import fsSync from "node:fs";
import assert from "node:assert/strict";
import { CodexRpc } from "../src/rpc.mjs";
import { configText, INSTRUCTIONS, MODEL_CATALOG } from "../src/config.mjs";
import { TOOLS } from "../src/common.mjs";
import { writePrivate } from "../src/storage.mjs";
import { createRunner } from "../src/server.mjs";
const advertisedTools = (body) =>
  body.tools ??
  (body.input ?? [])
    .filter((item) => item.type === "additional_tools")
    .flatMap((item) => item.tools);
export async function nativeProbe({
  throughRunner = false,
  searchCapableModel = false,
} = {}) {
  assert.ok(!searchCapableModel || throughRunner);
  if (searchCapableModel) {
    const catalogue = JSON.parse(await fs.readFile(MODEL_CATALOG, "utf8"));
    const model = catalogue.models.find((m) => m.slug === "gpt-6.1-sol");
    assert.equal(model.supports_search_tool, true);
    assert.equal(model.tool_mode, "direct");
  }
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
  let rpc, timer, runner;
  const nativeErrors = [],
    protocol = [];
  let nativeDiagnostics = "";
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
      const namespace = advertisedTools(body).find(
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
    else if (n === 3 && searchCapableModel)
      item = {
        type: "custom_tool_call",
        id: "ctc_forbidden_exec",
        call_id: "call_forbidden_exec",
        name: "exec",
        input:
          "text(await tools.exec_command({cmd:" +
          JSON.stringify("touch " + marker) +
          "}));",
      };
    else if (n === 4 && searchCapableModel)
      item = {
        type: "custom_tool_call",
        id: "ctc_forbidden_patch",
        call_id: "call_forbidden_patch",
        name: "apply_patch",
        input:
          "*** Begin Patch\n*** Add File: " +
          marker +
          "\n+FORBIDDEN_EDIT\n*** End Patch",
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
    if (throughRunner) {
      const token = "synthetic-runner-" + crypto.randomUUID();
      const organization_id = crypto.randomUUID();
      runner = await createRunner({
        token,
        apiUrl: base,
        authRoot: path.join(directory, "auth"),
        tempRoot: path.join(directory, "temp"),
        model: searchCapableModel ? "gpt-6.1-sol" : "offline-probe",
        rpcFactory: (home, cwd) => {
          // Keep every production option; only route the model to this offline fixture.
          const file = path.join(home, "config.toml");
          const production = fsSync.readFileSync(file, "utf8");
          assert.equal(
            production,
            configText({ apiUrl: base, capability: token }),
          );
          fsSync.writeFileSync(
            file,
            'model_provider = "offline_probe"\nmodel = "offline-probe"\n' +
              production.replace(
                "[features]\n",
                "[features]\nenable_request_compression = false\n",
              ) +
              '\n[model_providers.offline_probe]\nname = "Offline fixture"\nbase_url = ' +
              JSON.stringify(base) +
              '\nwire_api = "responses"\nrequires_openai_auth = true\nsupports_websockets = false\n',
            { mode: 0o600 },
          );
          rpc = new CodexRpc(home, cwd);
          rpc.child.stderr.on("data", (chunk) => {
            nativeDiagnostics += chunk.toString();
          });
          const request = rpc.request.bind(rpc);
          rpc.request = (method, params, ...rest) => {
            protocol.push({ method, params });
            return request(method, params, ...rest);
          };
          const message = rpc.message.bind(rpc);
          rpc.message = (value) => {
            if (value.error) nativeErrors.push(value.error);
            message(value);
          };
          return rpc;
        },
      });
      await runner.storage.write(organization_id, {
        generation: 1,
        authenticated: true,
        auth_mode: "apiKey",
      });
      await writePrivate(
        path.join(await runner.storage.org(organization_id), "auth.json"),
        JSON.stringify({
          OPENAI_API_KEY: "sk-synthetic-offline-not-a-real-key",
        }),
      );
      await new Promise((r) => runner.server.listen(0, "127.0.0.1", r));
      const response = await fetch(
        "http://127.0.0.1:" + runner.server.address().port + "/turn",
        {
          method: "POST",
          headers: {
            Authorization: "Bearer " + token,
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            organization_id,
            auth_generation: 1,
            run_id: crypto.randomUUID(),
            mcp_token: token,
            history: [
              {
                role: "user",
                text: "Offline production orchestration boundary fixture only.",
              },
            ],
            credential_refs: [],
          }),
          signal: AbortSignal.timeout(35000),
        },
      );
      assert.equal(response.status, 200);
      const events = (await response.text()).trim().split("\n").map(JSON.parse);
      assert.equal(
        events.filter((x) => ["completed", "error"].includes(x.type)).length,
        1,
      );
      assert.equal(
        events.at(-1).type,
        "completed",
        JSON.stringify({ events, nativeErrors, nativeDiagnostics }),
      );
      assert.equal(events.at(-1).text, "offline probe complete");
      assert.equal(
        protocol.find((x) => x.method === "thread/start").params
          .baseInstructions,
        INSTRUCTIONS,
      );
      assert.equal(
        protocol.find((x) => x.method === "thread/start").params
          .developerInstructions,
        "Работай только с настройками текущей компании через phone_platform MCP.",
      );
      assert.equal(
        protocol.find((x) => x.method === "turn/start").params.summary,
        "none",
      );
      for (const method of ["thread/start", "turn/start"]) {
        assert.deepEqual(
          protocol.find((x) => x.method === method).params.environments,
          [],
        );
        assert.deepEqual(
          protocol.find((x) => x.method === method).params
            .runtimeWorkspaceRoots,
          [],
        );
      }
      assert.deepEqual(await fs.readdir(path.join(directory, "temp")), []);
      assert.equal(runner.turns.active.size, 0);
    } else {
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
      const account = await rpc.request("account/read", {
        refreshToken: false,
      });
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
      timer = setTimeout(
        () => reject(new Error("offline_probe_timeout")),
        30000,
      );
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
    }
    assert.equal(
      toolCalls.length,
      1,
      "Native MCP invocation did not reach the fixed gateway",
    );
    assert.equal(toolCalls[0].name, "get_workspace_configuration");
    if (searchCapableModel) {
      assert.ok(requests.every((request) => request.model === "gpt-6.1-sol"));
      assert.equal(
        protocol.find((entry) => entry.method === "thread/start").params.model,
        "gpt-6.1-sol",
      );
    }
    const exposed = advertisedTools(requests[0]).flatMap((tool) =>
      tool.type === "namespace" && tool.name === "functions"
        ? tool.tools
        : [tool],
    );
    for (const tool of exposed) {
      assert.ok(
        [
          "request_user_input",
          "list_mcp_resources",
          "list_mcp_resource_templates",
          "read_mcp_resource",
          "mcp__phone_platform",
        ].includes(tool.name),
        JSON.stringify({
          unexpected: tool.name,
          advertised: exposed.map((entry) => entry.name),
        }),
      );
      if (tool.name === "mcp__phone_platform") {
        assert.deepEqual(
          tool.tools.map((t) => t.name).sort(),
          [...TOOLS].sort(),
        );
        assert.notEqual(tool.defer_loading, true);
        for (const callable of tool.tools)
          assert.notEqual(callable.defer_loading, true);
      }
    }
    const outputs = requests
      .slice(1)
      .flatMap((body) => body.input ?? [])
      .filter((item) =>
        ["function_call_output", "custom_tool_call_output"].includes(item.type),
      )
      .map((item) => item.output);
    assert.ok(outputs.includes("unsupported call: exec_command"));
    if (searchCapableModel)
      assert.ok(
        outputs.includes("unsupported custom tool call: exec"),
        JSON.stringify(outputs),
      );
    if (searchCapableModel)
      assert.ok(
        outputs.includes("unsupported custom tool call: apply_patch"),
        JSON.stringify(outputs),
      );
    assert.equal(
      await fs.stat(marker).then(
        () => true,
        () => false,
      ),
      false,
    );
    return {
      codex_version: "0.167.2",
      account_unconfigured: !throughRunner,
      synthetic_api_key: throughRunner,
      ephemeral_thread: true,
      environment_capabilities: 0,
      actual_sdk_mcp_call: true,
      forbidden_exec_rejected: true,
      forbidden_tool_advertised: false,
      external_model_requests: 0,
      loopback_responses_requests: requests.length,
      production_orchestrator: throughRunner,
      search_capable_model: searchCapableModel,
      direct_service_tool_count: TOOLS.length,
    };
  } finally {
    clearTimeout(timer);
    await rpc?.dispose();
    await runner?.close();
    server.closeAllConnections();
    await new Promise((r) => server.close(r));
    await fs.rm(directory, { recursive: true, force: true });
  }
}
