import path from "node:path";
import {
  SafeError,
  keys,
  uuid,
  generation,
  secret,
  requireValue,
  safeCode,
  TOOLS,
} from "./common.mjs";
import { CodexRpc } from "./rpc.mjs";
import { configText, INSTRUCTIONS } from "./config.mjs";
import { writePrivate } from "./storage.mjs";
const IGNORE = new Set([
  // Authenticated ChatGPT startup emits status metadata, not a tool or RPC request.
  "remoteControl/status/changed",
  "thread/started",
  "thread/status/changed",
  "thread/settings/updated",
  "turn/started",
  "thread/tokenUsage/updated",
  "item/reasoning/summaryPartAdded",
  "item/reasoning/summaryTextDelta",
  "item/reasoning/textDelta",
  "item/plan/delta",
  "turn/plan/updated",
  "turn/moderationMetadata",
  "mcpServer/startupStatus/updated",
  "item/mcpToolCall/progress",
  "serverRequest/resolved",
  "account/updated",
  "account/rateLimits/updated",
  "model/safetyBuffering/updated",
  "configWarning",
  "deprecationNotice",
  "warning",
]);
const ITEMS = new Set([
  "userMessage",
  "agentMessage",
  "reasoning",
  "mcpToolCall",
  "contextCompaction",
  "plan",
]);
export function validateTurn(body) {
  keys(body, [
    "organization_id",
    "auth_generation",
    "run_id",
    "mcp_token",
    "history",
    "credential_refs",
  ]);
  uuid(body.organization_id);
  uuid(body.run_id);
  generation(body.auth_generation);
  secret(body.mcp_token);
  requireValue(
    Array.isArray(body.history) &&
      body.history.length >= 1 &&
      body.history.length <= 40,
  );
  for (const message of body.history) {
    keys(message, ["role", "text"]);
    requireValue(
      ["user", "assistant"].includes(message.role) &&
        typeof message.text === "string" &&
        message.text.length <= 65536,
    );
  }
  requireValue(Buffer.byteLength(JSON.stringify(body.history)) <= 131072);
  requireValue(
    Array.isArray(body.credential_refs) && body.credential_refs.length <= 8,
  );
  for (const card of body.credential_refs) {
    keys(card, ["id", "kind", "label", "created_at", "expires_at"]);
    uuid(card.id);
    requireValue(
      card.kind === "telphin" &&
        typeof card.label === "string" &&
        card.label.length <= 120 &&
        typeof card.created_at === "string" &&
        typeof card.expires_at === "string" &&
        Number.isFinite(Date.parse(card.created_at)) &&
        Number.isFinite(Date.parse(card.expires_at)),
    );
  }
  return body;
}
export class TurnService {
  constructor({
    storage,
    apiUrl,
    model = null,
    rpcFactory = (home, cwd) => new CodexRpc(home, cwd),
    deadline = 180000,
  }) {
    Object.assign(this, { storage, apiUrl, model, rpcFactory, deadline });
    this.active = new Map();
  }
  async cancelOrg(org) {
    const jobs = [...this.active.values()].filter((x) => x.org === org);
    for (const job of jobs) job.abort.abort();
    await Promise.all(jobs.map((job) => job.rpc?.dispose()));
  }
  async cancel(body) {
    keys(body, ["organization_id", "run_id"]);
    const org = uuid(body.organization_id),
      id = uuid(body.run_id);
    const job = this.active.get(id);
    if (job && job.org === org) {
      job.abort.abort();
      await job.rpc?.dispose();
    }
    return { cancelled: true };
  }
  async prepare(body) {
    validateTurn(body);
    const org = uuid(body.organization_id),
      id = uuid(body.run_id);
    requireValue(this.active.size < 2, "assistant_runner_busy", 429);
    requireValue(!this.active.has(id), "assistant_run_exists", 409);
    const job = { org, id, abort: new AbortController(), rpc: null };
    this.active.set(id, job);
    try {
      await this.storage.lock(org, async () => {
        const state = await this.storage.read(org);
        requireValue(
          state.generation === body.auth_generation && state.authenticated,
          "assistant_auth_required",
          409,
        );
        const auth = await this.storage.auth(org);
        requireValue(auth, "assistant_auth_required", 409);
        job.context = await this.storage.context("turn");
        await writePrivate(path.join(job.context.home, "auth.json"), auth);
        await writePrivate(
          path.join(job.context.home, "config.toml"),
          configText({ apiUrl: this.apiUrl, capability: body.mcp_token }),
        );
      });
      return job;
    } catch (error) {
      this.active.delete(id);
      await job.context?.cleanup();
      throw error;
    }
  }
  async run(job, body, emit) {
    let terminal = false,
      rpc,
      threadId,
      turnId,
      failure = null,
      stage = "initializing";
    const messages = new Map();
    let outputBytes = 0;
    const send = (value) => {
      if (terminal) return;
      if (value.type === "completed" || value.type === "error") terminal = true;
      emit(value);
    };
    const timer = setTimeout(() => job.abort.abort(), this.deadline);
    timer.unref();
    try {
      requireValue(!job.abort.signal.aborted, "assistant_run_cancelled");
      rpc = this.rpcFactory(job.context.home, job.context.work);
      job.rpc = rpc;
      let resolve, reject;
      const finished = new Promise((ok, fail) => {
        resolve = ok;
        reject = fail;
      });
      finished.catch(() => {});
      const fail = (error) => {
        failure ??= error;
        reject(error);
      };
      const stop = () => {
        fail(new SafeError("assistant_run_cancelled"));
        void rpc.dispose();
      };
      job.abort.signal.addEventListener("abort", stop, { once: true });
      rpc.on("fault", fail);
      rpc.on("closed", () => {
        if (!terminal) fail(new SafeError("codex_stream_interrupted"));
      });
      rpc.on("blockedRequest", () =>
        fail(new SafeError("unsupported_runtime_request")),
      );
      rpc.on("notification", (notification) => {
        try {
          const method = notification.method,
            p = notification.params ?? {};
          if (method === "error") throw new SafeError("codex_turn_failed");
          if (method === "item/agentMessage/delta") {
            requireValue(
              typeof p.delta === "string" && typeof p.itemId === "string",
              "invalid_codex_delta",
            );
            const old = messages.get(p.itemId) || { text: "" };
            old.text += p.delta;
            messages.set(p.itemId, old);
            outputBytes += Buffer.byteLength(p.delta);
            requireValue(outputBytes <= 65536, "assistant_response_too_large");
            send({ type: "delta", text: p.delta });
            return;
          }
          if (method === "item/started" || method === "item/completed") {
            const item = p.item;
            requireValue(item && ITEMS.has(item.type), "forbidden_codex_item");
            if (item.type === "mcpToolCall")
              requireValue(
                item.server === "phone_platform" && TOOLS.includes(item.tool),
                "unsupported_tool",
              );
            if (method === "item/completed" && item.type === "agentMessage") {
              requireValue(
                typeof item.text === "string" &&
                  Buffer.byteLength(item.text) <= 65536,
                "assistant_response_too_large",
              );
              messages.set(item.id, { text: item.text, phase: item.phase });
            }
            return;
          }
          if (method === "turn/completed") {
            requireValue(
              ["completed", "failed", "interrupted"].includes(p.turn?.status),
              "invalid_codex_terminal",
            );
            if (p.turn.status !== "completed")
              throw new SafeError(
                p.turn.status === "interrupted"
                  ? "assistant_run_cancelled"
                  : "codex_turn_failed",
              );
            const list = [...messages.values()];
            const final =
              list.findLast((item) => item.phase === "final_answer") ??
              list.at(-1);
            requireValue(
              final && final.text.length > 0,
              "assistant_empty_response",
            );
            resolve({ text: final.text });
            return;
          }
          requireValue(IGNORE.has(method), "unexpected_codex_event");
        } catch (error) {
          fail(error);
          void rpc.dispose();
        }
      });
      send({ type: "status", message: "Подключение помощника" });
      await rpc.initialize();
      stage = "configuration";
      const configuration = await rpc.request("config/read", {
        includeLayers: false,
      });
      requireValue(
        configuration.config.web_search === "disabled" &&
          configuration.config.features?.shell_tool === false &&
          configuration.config.features?.multi_agent === false &&
          configuration.config.features?.plugins === false,
        "unsafe_codex_config",
      );
      stage = "account";
      const account = await rpc.request("account/read", {
        refreshToken: false,
      });
      requireValue(account.account, "assistant_auth_required", 409);
      stage = "thread";
      const started = await rpc.request("thread/start", {
        ephemeral: true,
        approvalPolicy: "never",
        sandbox: "read-only",
        cwd: job.context.work,
        environments: [],
        runtimeWorkspaceRoots: [],
        baseInstructions: INSTRUCTIONS,
        developerInstructions:
          "Работай только с настройками текущей компании через phone_platform MCP.",
        ...(this.model ? { model: this.model } : {}),
      });
      threadId = started.thread?.id;
      requireValue(
        typeof threadId === "string" &&
          started.thread.ephemeral === true &&
          started.thread.path === null &&
          Array.isArray(started.thread.environments) &&
          started.thread.environments.length === 0 &&
          started.runtimeWorkspaceRoots?.length === 0 &&
          started.sandbox?.type === "readOnly" &&
          started.sandbox.networkAccess === false,
        "unsafe_codex_environment",
      );
      job.model = started.model;
      const context = JSON.stringify({
        conversation: body.history,
        credentials: body.credential_refs,
      });
      stage = "turn";
      const turn = await rpc.request("turn/start", {
        threadId,
        input: [
          {
            type: "text",
            text:
              "Продолжи этот диалог настройки. Доступы — только ссылки на защищённые формы.\n" +
              context,
          },
        ],
        environments: [],
        runtimeWorkspaceRoots: [],
        approvalPolicy: "never",
        sandboxPolicy: { type: "readOnly", networkAccess: false },
        summary: "none",
      });
      turnId = turn.turn?.id;
      stage = "response";
      const result = await finished;
      stage = "credentials_refresh";
      await this.storage.lock(job.org, async () => {
        const state = await this.storage.read(job.org);
        requireValue(
          state.generation === body.auth_generation &&
            state.authenticated &&
            !job.abort.signal.aborted,
          "assistant_authority_revoked",
          403,
        );
        await this.storage.promote(job.org, job.context.home);
      });
      send({
        type: "completed",
        text: result.text,
        ...(job.model ? { model: job.model } : {}),
      });
    } catch (error) {
      const code = job.abort.signal.aborted
        ? "assistant_run_cancelled"
        : safeCode(failure ?? error);
      process.stderr.write(
        JSON.stringify({ event: "assistant_turn_failed", run_id: job.id, stage, code }) + "\n",
      );
      send({ type: "error", code });
    } finally {
      clearTimeout(timer);
      if (job.abort.signal.aborted && threadId && turnId && !rpc?.closed) {
        try {
          await rpc.request("turn/interrupt", { threadId, turnId }, 500);
        } catch {}
      }
      await rpc?.dispose();
      await job.context.cleanup();
      this.active.delete(job.id);
    }
  }
  async close() {
    const jobs = [...this.active.values()];
    for (const job of jobs) job.abort.abort();
    await Promise.all(jobs.map((x) => x.rpc?.dispose()));
    await Promise.allSettled(jobs.map((x) => x.done).filter(Boolean));
  }
}
