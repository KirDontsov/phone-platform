import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
const mode = process.argv[2] || "normal";
let login = "login-test",
  thread = "thread-test";
const send = (value) => process.stdout.write(JSON.stringify(value) + "\n");
for await (const line of readline.createInterface({ input: process.stdin })) {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    continue;
  }
  if (!message.id) continue;
  const { id, method, params: p } = message;
  let result = {};
  if (method === "initialize") result = { userAgent: "fake/0.167.2" };
  else if (method === "account/login/start") {
    if (p.type === "apiKey") {
      fs.writeFileSync(
        path.join(process.env.CODEX_HOME, "auth.json"),
        JSON.stringify({ OPENAI_API_KEY: p.apiKey }),
        { mode: 0o600 },
      );
      result = { type: "apiKey" };
    } else {
      result = {
        type: "chatgptDeviceCode",
        loginId: login,
        verificationUrl: "https://auth.openai.com/codex/device",
        userCode: "TEST-1234",
      };
      if (mode === "device")
        setTimeout(() => {
          fs.writeFileSync(
            path.join(process.env.CODEX_HOME, "auth.json"),
            JSON.stringify({ tokens: { synthetic: true } }),
            { mode: 0o600 },
          );
          send({
            method: "account/login/completed",
            params: { loginId: login, success: true, error: null },
          });
        }, 80);
    }
  } else if (method === "account/read")
    result = {
      account: fs.existsSync(path.join(process.env.CODEX_HOME, "auth.json"))
        ? {
            type: JSON.parse(
              fs.readFileSync(
                path.join(process.env.CODEX_HOME, "auth.json"),
                "utf8",
              ),
            ).tokens
              ? "chatgpt"
              : "apiKey",
          }
        : null,
      requiresOpenaiAuth: true,
    };
  else if (method === "config/read")
    result = {
      config: {
        web_search: "disabled",
        features: { shell_tool: false, multi_agent: false, plugins: false },
      },
    };
  else if (method === "thread/start")
    result = {
      thread: { id: thread, ephemeral: true, path: null, environments: [] },
      runtimeWorkspaceRoots: [],
      sandbox: { type: "readOnly", networkAccess: false },
      model: "fake-model",
    };
  else if (method === "turn/start") {
    result = { turn: { id: "turn-test", status: "inProgress" } };
    send({ id, result });
    if (mode === "eof") process.exit(0);
    if (mode === "slow") continue;
    if (mode === "forbidden") {
      send({
        method: "item/started",
        params: {
          threadId: thread,
          item: {
            id: "bad",
            type: "commandExecution",
            command: "secret-command",
          },
        },
      });
      continue;
    }
    if (mode === "unknown") {
      send({ method: "new/untrusted/event", params: { secret: "hidden" } });
      continue;
    }
    if (mode === "request") {
      send({
        id: 900,
        method: "item/tool/requestUserInput",
        params: { questions: [] },
      });
      continue;
    }
    process.stderr.write("private stderr APIKEY-MUST-NOT-ECHO\n");
    send({
      method: "item/agentMessage/delta",
      params: {
        threadId: thread,
        turnId: "turn-test",
        itemId: "reply",
        delta: "Готово",
      },
    });
    send({
      method: "item/completed",
      params: {
        threadId: thread,
        item: {
          id: "reply",
          type: "agentMessage",
          text: "Готово",
          phase: "final_answer",
        },
      },
    });
    send({
      method: "turn/completed",
      params: {
        threadId: thread,
        turn: { id: "turn-test", status: "completed", items: [] },
      },
    });
    continue;
  } else if (method === "turn/interrupt") result = {};
  else if (method === "account/logout")
    fs.rmSync(path.join(process.env.CODEX_HOME, "auth.json"), { force: true });
  else {
    send({ id, error: { code: -32601, message: "unsupported" } });
    continue;
  }
  send({ id, result });
}
