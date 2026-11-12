import path from "node:path";
import fs from "node:fs/promises";
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
      last_generation: state.last_generation ?? state.generation,
      pending_generation: pending?.generation ?? null,
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
      if ((pending && pending.expires <= Date.now()) || (!pending && state.pending_generation != null)) {
        const code = pending ? "assistant_login_expired" : "assistant_login_interrupted";
        if (pending) await this.clearPending(org);
        state = { ...state, pending_generation: null, pending_attempt: null, error_code: code };
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
      const currentPending = this.pending.get(org);
      if ((gen === previous.generation && previous.attempt_id === attempt) || (currentPending?.generation === gen && currentPending.attempt === attempt))
        return this.dto(previous, currentPending);
      requireValue(gen > Math.max(previous.generation, previous.last_generation ?? 0), "stale_auth_generation", 409);
      await this.clearPending(org);
      const state = { ...previous, last_generation: gen, pending_generation: gen, pending_attempt: attempt, error_code: null };
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
              if (notification.params.success) {
                try { await this.complete(org, pending); }
                catch (error) { await this.failure(org,pending,safeCode(error,"assistant_login_failed")); }
              } else await this.failure(org, pending, "assistant_login_failed");
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
    requireValue(this.pending.get(org) === pending && current.pending_generation === pending.generation && current.pending_attempt === pending.attempt && pending.expires > Date.now(), "stale_auth_generation", 409);
    const account = await pending.rpc.request("account/read", { refreshToken: false });
    const authMode = mode(account.account);
    requireValue(authMode, "assistant_auth_required", 409);
    const callback = phase => apiPost(this.apiUrl, "/api/assistant/auth/complete", pending.token,
      { auth_attempt_id: pending.attempt, generation: pending.generation, phase }, {timeout:5000});
    const approval = await callback("prepare");
    requireValue(approval?.approved === true && approval.generation === pending.generation, "assistant_authorization_revoked", 403);
    const previousAuth = await this.storage.auth(org);
    const backup = path.join(await this.storage.org(org), "auth.previous.json");
    if (previousAuth) await writePrivate(backup, previousAuth);
    // Crash-safe boundary: a newly promoted file must never run under the old confirmed generation.
    await this.storage.write(org, { ...current, authenticated:false, commit_uncertain:true, promotion_pending:true, error_code:"assistant_auth_commit_uncertain" });
    await this.storage.promote(org, pending.context.home);
    pending.promoted = true;
    pending.commitStarted = true;
    let committed = false;
    for (let attempt=0; attempt<3 && !committed; attempt++) {
      try { const result = await callback("commit"); committed = result?.approved === true && result.generation === pending.generation; }
      catch { /* A lost response may follow a committed transaction; do not restore blindly. */ }
    }
    if (!committed) {
      let receipt;
      try { receipt = await callback("status"); } catch {}
      committed = receipt?.committed === true;
      if (!committed && receipt?.active_generation === current.generation) {
        if (previousAuth) await writePrivate(path.join(await this.storage.org(org), "auth.json"), previousAuth);
        else await this.storage.removeAuth(org);
        await fs.rm(backup,{force:true});
        pending.promoted = false;
        await this.storage.write(org, current);
        throw new SafeError("assistant_authorization_revoked",403);
      }
      if (!committed) {
        await this.storage.write(org,{...current,authenticated:false,pending_generation:null,pending_attempt:null,commit_uncertain:true,error_code:"assistant_auth_commit_uncertain"});
        throw new SafeError("assistant_auth_commit_uncertain",503);
      }
    }
    await this.cancelOrg(org);
    await this.storage.write(org, { generation: pending.generation, last_generation: current.last_generation ?? pending.generation,
      attempt_id: pending.attempt, pending_generation:null,pending_attempt:null, authenticated: true, auth_mode: authMode, error_code: null });
    await fs.rm(backup,{force:true});
    await this.clearPending(org);
  }
  async failure(org, pending, code) {
    if (this.pending.get(org) !== pending) return;
    const current = await this.storage.read(org);
    const uncertain = current.commit_uncertain || pending.promoted && pending.commitStarted;
    await this.storage.write(org, { ...current, ...(uncertain ? {authenticated:false,commit_uncertain:true}:{}), pending_generation:null, pending_attempt:null, error_code:code });
    if (!uncertain) await fs.rm(path.join(await this.storage.org(org),"auth.previous.json"),{force:true});
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
      requireValue(gen >= Math.max(current.generation, current.last_generation ?? 0), "stale_auth_generation", 409);
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
      await fs.rm(path.join(await this.storage.org(org), "auth.previous.json"), {force:true});
      return this.dto(await this.storage.read(org));
    });
  }
  async close() {
    for (const org of [...this.pending.keys()])
      await this.storage.lock(org, () => this.clearPending(org));
  }
}
