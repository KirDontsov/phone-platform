import { spawn } from "node:child_process";
import { EventEmitter } from "node:events";
import { StringDecoder } from "node:string_decoder";
import { SafeError, requireValue } from "./common.mjs";
import { CODEX_BIN, childEnvironment } from "./config.mjs";
const METHODS = new Set([
  "initialize",
  "account/read",
  "account/login/start",
  "account/login/cancel",
  "account/logout",
  "config/read",
  "thread/start",
  "turn/start",
  "turn/interrupt",
  "mcpServerStatus/list",
]);
export class CodexRpc extends EventEmitter {
  constructor(
    home,
    cwd,
    {
      command = process.execPath,
      args = [
        CODEX_BIN,
        "app-server",
        "--strict-config",
        "--listen",
        "stdio://",
      ],
    } = {},
  ) {
    super();
    this.next = 0;
    this.pending = new Map();
    this.closed = false;
    this.disposing = false;
    this.buffer = "";
    this.decoder = new StringDecoder("utf8");
    this.stderrBytes = 0;
    this.child = spawn(command, args, {
      cwd,
      env: childEnvironment(home),
      stdio: ["pipe", "pipe", "pipe"],
      detached: true,
      shell: false,
    });
    this.exit = new Promise((resolve) =>
      this.child.once("close", () => resolve()),
    );
    this.child.stdout.on("data", (chunk) => {
      this.buffer += this.decoder.write(chunk);
      if (Buffer.byteLength(this.buffer) > 1048576)
        return this.fail("codex_wire_too_large");
      let split;
      while ((split = this.buffer.indexOf("\n")) >= 0) {
        const line = this.buffer.slice(0, split);
        this.buffer = this.buffer.slice(split + 1);
        if (!line.trim()) continue;
        try {
          this.message(JSON.parse(line));
        } catch {
          this.fail("invalid_codex_wire");
          return;
        }
      }
    });
    this.child.stderr.on("data", (chunk) => {
      this.stderrBytes += chunk.length;
      if (this.stderrBytes > 1048576) this.fail("codex_diagnostics_too_large");
    });
    this.child.on("error", () => this.fail("codex_process_failed"));
    this.child.on("close", () => {
      this.closed = true;
      for (const item of this.pending.values()) {
        clearTimeout(item.timer);
        item.reject(new SafeError("codex_process_closed"));
      }
      this.pending.clear();
      this.emit("closed");
    });
  }
  message(value) {
    requireValue(
      value && typeof value === "object" && !Array.isArray(value),
      "invalid_codex_wire",
    );
    if (typeof value.method === "string") {
      if (Object.hasOwn(value, "id")) {
        this.send({
          id: value.id,
          error: { code: -32601, message: "unsupported_runtime_request" },
        });
        this.emit("blockedRequest", value.method);
        return;
      }
      this.emit("notification", value);
      return;
    }
    const pending = this.pending.get(value.id);
    requireValue(pending, "unexpected_codex_response");
    this.pending.delete(value.id);
    clearTimeout(pending.timer);
    if (value.error) pending.reject(new SafeError("codex_request_failed"));
    else if (Object.hasOwn(value, "result")) pending.resolve(value.result);
    else pending.reject(new SafeError("invalid_codex_wire"));
  }
  send(value) {
    if (this.closed || !this.child.stdin.writable)
      throw new SafeError("codex_process_closed");
    this.child.stdin.write(JSON.stringify(value) + "\n");
  }
  request(method, params, timeout = 15000) {
    if (!METHODS.has(method))
      return Promise.reject(new SafeError("forbidden_codex_method"));
    if (this.closed)
      return Promise.reject(new SafeError("codex_process_closed"));
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new SafeError("codex_request_timeout"));
        void this.dispose();
      }, timeout);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.send({ id, method, params });
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }
  async initialize() {
    const result = await this.request("initialize", {
      clientInfo: { name: "phone_platform_assistant", version: "0.1.0" },
      capabilities: { experimentalApi: true },
    });
    this.send({ method: "initialized", params: {} });
    return result;
  }
  fail(code) {
    if (this.closed) return;
    this.emit("fault", new SafeError(code));
    void this.dispose();
  }
  async dispose() {
    if (this.disposing || this.closed) return this.exit;
    this.disposing = true;
    try {
      process.kill(-this.child.pid, "SIGTERM");
    } catch {}
    await Promise.race([this.exit, new Promise((r) => setTimeout(r, 800))]);
    try {
      process.kill(-this.child.pid, "SIGKILL");
    } catch {}
    await this.exit;
  }
}
