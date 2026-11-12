import crypto from "node:crypto";
export const VERSION = "0.167.2";
export const TOOLS = Object.freeze([
  "get_workspace_configuration",
  "connect_telphin",
  "test_telphin_connection",
  "sync_telphin",
  "upsert_taxonomy",
  "upsert_filter",
  "save_rubric",
  "set_callback_rules",
]);
export class SafeError extends Error {
  constructor(code, status = 422) {
    super(code);
    this.code = code;
    this.status = status;
  }
}
export function requireValue(value, code = "invalid_request", status = 422) {
  if (!value) throw new SafeError(code, status);
}
export function keys(value, required, optional = []) {
  requireValue(value && typeof value === "object" && !Array.isArray(value));
  requireValue(
    required.every((k) => Object.hasOwn(value, k)) &&
      Object.keys(value).every((k) => [...required, ...optional].includes(k)),
  );
}
export function uuid(value) {
  requireValue(
    typeof value === "string" &&
      /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(
        value,
      ),
  );
  return value.toLowerCase();
}
export function generation(value) {
  requireValue(Number.isSafeInteger(value) && value >= 0);
  return value;
}
export function secret(value) {
  requireValue(
    typeof value === "string" &&
      value.length >= 24 &&
      value.length <= 8192 &&
      !/[\r\n\0]/.test(value),
  );
  return value;
}
export function sameToken(a, b) {
  if (typeof a !== "string" || typeof b !== "string") return false;
  const x = crypto.createHash("sha256").update(a).digest(),
    y = crypto.createHash("sha256").update(b).digest();
  return crypto.timingSafeEqual(x, y);
}
export function safeCode(error, fallback = "assistant_runtime_failed") {
  return error instanceof SafeError ? error.code : fallback;
}
export async function readJson(response, max = 262144) {
  let size = 0,
    parts = [];
  for await (const part of response.body) {
    size += part.length;
    requireValue(size <= max, "upstream_response_too_large");
    parts.push(part);
  }
  try {
    return JSON.parse(Buffer.concat(parts).toString("utf8"));
  } catch {
    throw new SafeError("invalid_upstream_json");
  }
}
export function endpoint(base, path) {
  const url = new URL(base);
  requireValue(
    ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password &&
      url.pathname === "/" &&
      !url.search &&
      !url.hash,
    "invalid_api_origin",
  );
  return new URL(path, url);
}
export async function apiPost(
  base,
  path,
  token,
  body,
  { timeout = 15000, signal } = {},
) {
  let response;
  try {
    response = await fetch(endpoint(base, path), {
      method: "POST",
      headers: {
        Authorization: "Bearer " + token,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
      redirect: "error",
      signal: signal
        ? AbortSignal.any([signal, AbortSignal.timeout(timeout)])
        : AbortSignal.timeout(timeout),
    });
  } catch {
    throw new SafeError("assistant_api_unavailable", 503);
  }
  const data = await readJson(response);
  if (!response.ok)
    throw new SafeError(
      response.status === 401 || response.status === 403
        ? "assistant_authority_revoked"
        : response.status === 409
          ? "assistant_conflict"
          : "assistant_tool_failed",
      response.status,
    );
  requireValue(
    data && Object.hasOwn(data, "data"),
    "invalid_upstream_envelope",
  );
  return data.data;
}
