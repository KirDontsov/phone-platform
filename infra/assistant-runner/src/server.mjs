import http from "node:http";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  VERSION,
  SafeError,
  secret,
  sameToken,
  safeCode,
  requireValue,
} from "./common.mjs";
import { Storage } from "./storage.mjs";
import { AuthService } from "./auth.mjs";
import { TurnService } from "./turns.mjs";
import { CODEX_BIN, childEnvironment } from "./config.mjs";
export async function createRunner({
  token,
  apiUrl,
  authRoot = "/var/lib/assistant-auth",
  tempRoot = "/tmp/phone-assistant",
  model = null,
  rpcFactory,
  loginTtl,
} = {}) {
  secret(token);
  const storage = new Storage(authRoot, tempRoot);
  await storage.init();
  const turns = new TurnService({
    storage,
    apiUrl,
    model,
    ...(rpcFactory ? { rpcFactory } : {}),
  });
  const auth = new AuthService({
    storage,
    apiUrl,
    model,
    rpcFactory,
    loginTtl,
    cancelOrg: (org) => turns.cancelOrg(org),
  });
  const server = http.createServer(async (req, res) => {
    let streamJob = null;
    try {
      if (req.url === "/health" && req.method === "GET") {
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ healthy: true, codex_version: VERSION }));
        return;
      }
      requireValue(req.method === "POST", "method_not_allowed", 405);
      requireValue(
        sameToken(req.headers.authorization, "Bearer " + token),
        "unauthorized",
        401,
      );
      requireValue(
        req.headers["content-type"]?.split(";")[0] === "application/json",
        "invalid_content_type",
        415,
      );
      const chunks = [];
      let size = 0;
      for await (const chunk of req) {
        size += chunk.length;
        requireValue(size <= 196608, "request_too_large", 413);
        chunks.push(chunk);
      }
      let body;
      try {
        body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      } catch {
        throw new SafeError("invalid_json");
      }
      let data;
      switch (req.url) {
        case "/auth/status":
          data = await auth.status(body);
          break;
        case "/auth/start":
          data = await auth.start(body);
          break;
        case "/auth/cancel":
          data = await auth.revoke(body, true);
          break;
        case "/auth/logout":
          data = await auth.revoke(body);
          break;
        case "/cancel":
          data = await turns.cancel(body);
          break;
        case "/turn": {
          streamJob = await turns.prepare(body);
          if (res.destroyed) streamJob.abort.abort();
          res.writeHead(200, {
            "Content-Type": "application/x-ndjson",
            "Cache-Control": "no-store",
            "X-Accel-Buffering": "no",
          });
          res.on("close", () => {
            if (!res.writableEnded) streamJob.abort.abort();
          });
          streamJob.done = turns.run(streamJob, body, (value) => {
            if (!res.destroyed) {
              if (res.writableLength > 262144) {
                streamJob.abort.abort();
                throw new SafeError("assistant_stream_backpressure");
              }
              res.write(JSON.stringify(value) + "\n");
            }
          });
          await streamJob.done;
          res.end();
          return;
        }
        default:
          throw new SafeError("not_found", 404);
      }
      res.writeHead(200, {
        "Content-Type": "application/json",
        "Cache-Control": "no-store",
      });
      res.end(JSON.stringify(data));
    } catch (error) {
      if (!res.headersSent) {
        res.writeHead(error instanceof SafeError ? error.status : 500, {
          "Content-Type": "application/json",
          "Cache-Control": "no-store",
        });
        res.end(JSON.stringify({ error: { code: safeCode(error) } }));
      } else if (!res.destroyed) {
        res.destroy();
      }
    }
  });
  server.requestTimeout = 30000;
  server.headersTimeout = 10000;
  return {
    server,
    auth,
    turns,
    storage,
    close: async () => {
      await auth.close();
      await turns.close();
      server.closeAllConnections();
      await new Promise((resolve) => server.close(() => resolve()));
    },
  };
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  process.umask(0o077);
  try {
    const versionHome = await fs.mkdtemp(
      path.join(os.tmpdir(), "phone-codex-version-"),
    );
    await fs.chmod(versionHome, 0o700);
    let version;
    try {
      version = (
        await promisify(execFile)(process.execPath, [CODEX_BIN, "--version"], {
          env: childEnvironment(versionHome),
          timeout: 10000,
        })
      ).stdout.trim();
    } finally {
      await fs.rm(versionHome, { recursive: true, force: true });
    }
    requireValue(version === "codex-cli " + VERSION, "codex_version_mismatch");
    const runner = await createRunner({
      token: process.env.ASSISTANT_RUNNER_TOKEN,
      apiUrl: process.env.ASSISTANT_API_URL || "http://api:8080",
      authRoot: process.env.ASSISTANT_AUTH_ROOT || "/var/lib/assistant-auth",
      tempRoot: process.env.ASSISTANT_TEMP_ROOT || "/tmp/phone-assistant",
      model: process.env.ASSISTANT_CODEX_MODEL || null,
    });
    runner.server.listen(8091, "0.0.0.0");
    for (const signal of ["SIGTERM", "SIGINT"])
      process.once(signal, () => {
        void runner.close().then(() => process.exit(0));
      });
  } catch {
    process.stderr.write("assistant_runner_start_failed\n");
    process.exitCode = 1;
  }
}
