import path from "node:path";
import {
  VERSION,
  SafeError,
  keys,
  uuid,
  generation,
  secret,
  requireValue,
  apiPost,
  safeCode,
} from "./common.mjs";
import { configText } from "./config.mjs";
import { CodexRpc } from "./rpc.mjs";
import { writePrivate } from "./storage.mjs";
const mode = (account) =>
  account?.type === "apiKey" || account?.type === "apikey"
    ? "apiKey"
    : account?.type === "chatgpt"
      ? "chatgpt"
      : null;
export class AuthService {
  constructor({
    storage,
    apiUrl,
    rpcFactory = (home, cwd) => new CodexRpc(home, cwd),
    cancelOrg = async () => {},
    model = null,
    loginTtl = 600000,
  }) {
    Object.assign(this, {
      storage,
      apiUrl,
      rpcFactory,
      cancelOrg,
      model,
      loginTtl,
    });
    this.pending = new Map();
  }
  dto(state, pending = null) {
    return {
      generation: state.generation,
      authenticated: state.authenticated === true,
      auth_mode: state.authenticated ? state.auth_mode : null,
      codex_version: VERSION,
      model: this.model,
      pending_login: pending
        ? {
            login_id: pending.loginId,
            verification_url: pending.url,
            user_code: pending.code,
            expires_at: new Date(pending.expires).toISOString(),
          }
        : null,
      error_code: state.error_code ?? null,
    };
  }
  async status(body) {
    keys(body, ["organization_id"]);
    const org = uuid(body.organization_id);
    return this.storage.lock(org, async () => {
      let state = await this.storage.read(org);
      const pending = this.pending.get(org);
      if (pending && pending.expires <= Date.now()) {
        await this.clearPending(org);
        state = {
          generation: state.generation,
          authenticated: false,
          auth_mode: null,
          error_code: "assistant_login_expired",
        };
        await this.storage.write(org, state);
      }
      if (state.authenticated && !(await this.storage.auth(org))) {
        state = {
          ...state,
          authenticated: false,
          auth_mode: null,
          error_code: "assistant_auth_required",
        };
        await this.storage.write(org, state);
      }
      return this.dto(state, this.pending.get(org));
    });
  }
  async clearPending(org) {
    const pending = this.pending.get(org);
    if (!pending) return;
    this.pending.delete(org);
    clearTimeout(pending.timer);
    await pending.rpc.dispose();
    await pending.context.cleanup();
  }
  async start(body) {
    keys(
      body,
      [
        "organization_id",
        "auth_attempt_id",
        "generation",
        "authorization_token",
        "type",
      ],
      ["api_key"],
    );
    const org = uuid(body.organization_id),
      attempt = uuid(body.auth_attempt_id),
      gen = generation(body.generation);
    secret(body.authorization_token);
    requireValue(["apiKey", "chatgptDeviceCode"].includes(body.type));
    if (body.type === "apiKey")
      requireValue(
        typeof body.api_key === "string" &&
          body.api_key.length >= 10 &&
          body.api_key.length <= 8192 &&
          !/[\r\n\0]/.test(body.api_key),
      );
    else requireValue(body.api_key === undefined);
    return this.storage.lock(org, async () => {
      const previous = await this.storage.read(org);
      if (gen === previous.generation && previous.attempt_id === attempt)
        return this.dto(previous, this.pending.get(org));
      requireValue(gen > previous.generation, "stale_auth_generation", 409);
      await this.cancelOrg(org);
      await this.clearPending(org);
      await this.storage.removeAuth(org);
      let state = {
        generation: gen,
        attempt_id: attempt,
        authenticated: false,
        auth_mode: null,
        error_code: null,
      };
      await this.storage.write(org, state);
      const context = await this.storage.context("auth");
      await writePrivate(path.join(context.home, "config.toml"), configText());
      const rpc = this.rpcFactory(context.home, context.work);
      const pending = {
        rpc,
        context,
        generation: gen,
        attempt,
        token: body.authorization_token,
        expires: Date.now() + this.loginTtl,
        loginId: null,
      };
      this.pending.set(org, pending);
      const terminal = (notification) => {
        if (
          notification.method === "account/login/completed" &&
          notification.params?.loginId === pending.loginId
        ) {
          void this.storage
            .lock(org, async () => {
              if (this.pending.get(org) !== pending) return;
              if (notification.params.success)
                await this.complete(org, pending);
              else await this.failure(org, pending, "assistant_login_failed");
            })
            .catch(() => {});
        }
      };
      rpc.on("notification", terminal);
      rpc.on("closed", () => {
        void this.storage
          .lock(org, () =>
            this.failure(org, pending, "assistant_login_interrupted"),
          )
          .catch(() => {});
      });
      rpc.on("fault", () => {
        void this.storage
          .lock(org, () => this.failure(org, pending, "assistant_login_failed"))
          .catch(() => {});
      });
      try {
        await rpc.initialize();
        const result = await rpc.request(
          "account/login/start",
          body.type === "apiKey"
            ? { type: "apiKey", apiKey: body.api_key }
            : { type: "chatgptDeviceCode" },
          25000,
        );
        if (body.type === "apiKey") {
          await this.complete(org, pending);
          return this.dto(await this.storage.read(org));
        }
        requireValue(
          result.type === "chatgptDeviceCode" &&
            typeof result.loginId === "string" &&
            result.loginId.length <= 128 &&
            typeof result.userCode === "string" &&
            /^[A-Z0-9-]{3,64}$/.test(result.userCode),
          "invalid_device_login",
        );
        const url = new URL(result.verificationUrl);
        requireValue(
          url.protocol === "https:" &&
            url.hostname === "auth.openai.com" &&
            !url.username &&
            !url.password &&
            url.pathname === "/codex/device" &&
            !url.hash,
          "invalid_device_login",
        );
        pending.loginId = result.loginId;
        pending.code = result.userCode;
        pending.url = url.toString();
        pending.timer = setTimeout(() => {
          void this.status({ organization_id: org }).catch(() => {});
        }, this.loginTtl);
        pending.timer.unref();
        return this.dto(state, pending);
      } catch (error) {
        await this.failure(
          org,
          pending,
          safeCode(error, "assistant_login_failed"),
        );
        throw error;
      }
    });
  }
  async complete(org, pending) {
    const current = await this.storage.read(org);
    requireValue(
      this.pending.get(org) === pending &&
        current.generation === pending.generation &&
        pending.expires > Date.now(),
      "stale_auth_generation",
      409,
    );
    const account = await pending.rpc.request("account/read", {
      refreshToken: false,
    });
    const authMode = mode(account.account);
    requireValue(authMode, "assistant_auth_required", 409);
    try {
      const approval = await apiPost(
        this.apiUrl,
        "/api/assistant/auth/complete",
        pending.token,
        { auth_attempt_id: pending.attempt, generation: pending.generation },
      );
      requireValue(
        approval?.approved === true &&
          approval.generation === pending.generation,
        "assistant_authorization_revoked",
        403,
      );
    } catch {
      await this.failure(org, pending, "assistant_authorization_revoked");
      throw new SafeError("assistant_authorization_revoked", 403);
    }
    requireValue(
      (await this.storage.read(org)).generation === pending.generation,
      "stale_auth_generation",
      409,
    );
    await this.storage.promote(org, pending.context.home);
    await this.storage.write(org, {
      generation: pending.generation,
      attempt_id: pending.attempt,
      authenticated: true,
      auth_mode: authMode,
      error_code: null,
    });
    await this.clearPending(org);
  }
  async failure(org, pending, code) {
    if (this.pending.get(org) !== pending) return;
    await this.storage.write(org, {
      generation: pending.generation,
      attempt_id: pending.attempt,
      authenticated: false,
      auth_mode: null,
      error_code: code,
    });
    await this.clearPending(org);
  }
  async revoke(body, cancel = false) {
    keys(
      body,
      cancel
        ? ["organization_id", "login_id", "generation"]
        : ["organization_id", "generation"],
    );
    const org = uuid(body.organization_id),
      gen = generation(body.generation);
    return this.storage.lock(org, async () => {
      const current = await this.storage.read(org);
      requireValue(gen >= current.generation, "stale_auth_generation", 409);
      const pending = this.pending.get(org);
      if (cancel) {
        requireValue(
          typeof body.login_id === "string" && body.login_id.length <= 128,
        );
        if (pending)
          requireValue(
            pending.loginId === body.login_id,
            "stale_login_id",
            409,
          );
      }
      await this.storage.write(org, {
        generation: gen,
        authenticated: false,
        auth_mode: null,
        error_code: null,
      });
      await this.cancelOrg(org);
      await this.clearPending(org);
      await this.storage.removeAuth(org);
      return this.dto(await this.storage.read(org));
    });
  }
  async close() {
    for (const org of [...this.pending.keys()])
      await this.storage.lock(org, () => this.clearPending(org));
  }
}
