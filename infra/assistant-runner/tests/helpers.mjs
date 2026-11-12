import http from "node:http";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { TOOLS } from "../src/common.mjs";
import { CodexRpc } from "../src/rpc.mjs";
export const token = "test-runtime-only-" + crypto.randomUUID();
export const fakeRpc = (mode) => (home, cwd) =>
  new CodexRpc(home, cwd, {
    args: [
      fileURLToPath(new URL("./fake-codex.mjs", import.meta.url)),
      mode || "normal",
    ],
  });
export async function fixture(t, { approved = true } = {}) {
  const directory = await fs.mkdtemp(
    path.join(os.tmpdir(), "phone-assistant-test-"),
  );
  await fs.chmod(directory, 0o700);
  const calls = [];
  const server = http.createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    const body = JSON.parse(raw);
    calls.push({
      path: req.url,
      body,
      authorization: req.headers.authorization,
    });
    let data;
    if (req.url === "/api/assistant/auth/complete")
      data = { approved, generation: body.generation };
    else if (body.method === "list")
      data = {
        tools: TOOLS.map((name) => ({
          name,
          description: "Synthetic " + name,
          inputSchema: {
            type: "object",
            additionalProperties: false,
            properties: { value: { type: "string" } },
          },
        })),
      };
    else data = { result: { revision: 1, changed: body.name } };
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ data }));
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  t.after(async () => {
    server.closeAllConnections();
    await new Promise((r) => server.close(r));
    await fs.rm(directory, { recursive: true, force: true });
  });
  return {
    directory,
    calls,
    apiUrl: "http://127.0.0.1:" + server.address().port,
  };
}
export const ids = () => ({
  organization_id: crypto.randomUUID(),
  auth_attempt_id: crypto.randomUUID(),
  generation: 1,
  authorization_token: token,
});
export const loginBody = () => ({
  ...ids(),
  type: "apiKey",
  api_key: "sk-test-only-not-real-secret",
});
export const turnBody = (auth) => ({
  organization_id: auth.organization_id,
  auth_generation: auth.generation,
  run_id: crypto.randomUUID(),
  mcp_token: token,
  history: [{ role: "user", text: "Настрой фильтр" }],
  credential_refs: [],
});
