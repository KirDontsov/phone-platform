import http from 'node:http';
import { timingSafeEqual } from 'node:crypto';
import { Readable } from 'node:stream';
import { attachCodexProxy } from './codex-proxy.mjs';
import { LOCAL_ASR_MODEL, localAuthorized, localCatalog, redactNative, apinexTranscript } from './local-providers.mjs';

export const ASR_MODEL_IDS = Object.freeze(['moondream/parakeet-ultra', 'nvidia/nemotron-3.5-transcribe']);
const UPSTREAM = 'https://openrouter.ai/api/v1';
const AUDIO_LIMIT = 24 * 1024 * 1024;
const REQUEST_LIMIT = 34 * 1024 * 1024;
const RESPONSE_LIMIT = 8 * 1024 * 1024;
const FORMATS = new Set(['wav', 'mp3', 'flac', 'm4a', 'ogg', 'webm', 'aac', 'mp4']);
class GatewayError extends Error {
  constructor(status, code, retryAfter) { super(code); this.status = status; this.code = code; this.retryAfter = retryAfter; }
}
const failure = (status, code, retryAfter) => new GatewayError(status, code, retryAfter);
function object(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function strictKeys(value, keys) { return object(value) && Object.keys(value).every(key => keys.includes(key)); }
export function modelCatalog(config) {
  const definitions = config?.provider?.openrouter?.models;
  if (!object(definitions) || ASR_MODEL_IDS.some(id => !object(definitions[id]) || typeof definitions[id].name !== 'string')) throw failure(503, 'connector_catalog_invalid');
  return ASR_MODEL_IDS.map(id => ({ id, name: definitions[id].name }));
}
function send(res, status, data, retryAfter) {
  if (res.destroyed || res.writableEnded) return;
  res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', ...(retryAfter ? { 'Retry-After': String(retryAfter) } : {}) });
  res.end(JSON.stringify(data));
}
function authenticated(req, expected) {
  const auth = req.headers.authorization;
  if (typeof auth !== 'string' || auth.length > 4096) return false;
  const actual = Buffer.from(auth);
  return actual.length === expected.length && timingSafeEqual(actual, expected);
}
function providerKey(req) {
  const key = req.headers['x-openrouter-api-key'];
  if (typeof key !== 'string' || key.length < 8 || key.length > 4096 || /[\s\x00-\x1f\x7f]/.test(key)) throw failure(422, 'asr_provider_key_required');
  return key;
}
function incoming(req, limit, signal) {
  const length = req.headers['content-length'];
  if (length !== undefined && (!/^\d+$/.test(length) || Number(length) > limit)) { req.pause(); throw failure(413, 'asr_audio_too_large'); }
  return new Promise((resolve, reject) => {
    let size = 0; const chunks = [];
    const cleanup = () => { req.off('data', data); req.off('end', end); req.off('error', error); signal.removeEventListener('abort', aborted); };
    const error = () => { cleanup(); reject(failure(400, 'asr_invalid_request')); };
    const aborted = () => { cleanup(); req.pause(); reject(signal.reason || failure(504, 'provider_timeout')); };
    const data = chunk => { size += chunk.length; if (size > limit) { cleanup(); req.pause(); reject(failure(413, 'asr_audio_too_large')); } else chunks.push(chunk); };
    const end = () => { cleanup(); resolve(Buffer.concat(chunks, size)); };
    req.on('data', data); req.on('end', end); req.on('error', error); signal.addEventListener('abort', aborted, { once: true });
    if (signal.aborted) aborted();
  });
}
async function jsonRequest(req, limit, signal) {
  if (!/^application\/json(?:\s*;|$)/i.test(req.headers['content-type'] || '')) throw failure(415, 'asr_invalid_request');
  const bytes = await incoming(req, limit, signal);
  try { return JSON.parse(bytes.toString('utf8')); } catch { throw failure(422, 'asr_invalid_request'); }
}
async function boundedJson(response, signal) {
  const length = response.headers.get('content-length');
  if (length && Number(length) > RESPONSE_LIMIT) { await response.body?.cancel(); throw failure(502, 'asr_response_too_large'); }
  const reader = response.body?.getReader();
  if (!reader) throw failure(502, 'invalid_transcript_response');
  const chunks = []; let size = 0;
  try {
    for (;;) {
      if (signal.aborted) throw signal.reason;
      const { done, value } = await reader.read(); if (done) break;
      size += value.length; if (size > RESPONSE_LIMIT) throw failure(502, 'asr_response_too_large');
      chunks.push(value);
    }
    try { return JSON.parse(Buffer.concat(chunks, size).toString('utf8')); } catch { throw failure(502, 'invalid_transcript_response'); }
  } finally { await reader.cancel().catch(() => {}); }
}
async function checked(response) {
  if (response.ok) return;
  const retry = Number(response.headers.get('retry-after'));
  await response.body?.cancel().catch(() => {});
  const status = response.status;
  if (status === 401) throw failure(401, 'provider_unauthorized');
  if (status === 403) throw failure(403, 'provider_forbidden');
  if (status === 429) throw failure(429, 'provider_rate_limited', Number.isFinite(retry) && retry > 0 ? Math.min(3600, Math.ceil(retry)) : 60);
  if (status === 408 || status === 504) throw failure(504, 'provider_timeout');
  if (status === 413) throw failure(413, 'asr_audio_too_large');
  if (status >= 500 || status >= 300 && status < 400) throw failure(502, 'provider_unavailable');
  throw failure(422, 'provider_request_failed');
}
function selectedModel(body, catalog) {
  if (!catalog.some(model => model.id === body?.model)) throw failure(422, 'asr_model_unsupported');
  return body.model;
}
function transcriptionRequest(body, catalog) {
  if (!strictKeys(body, ['model', 'input_audio', 'response_format', 'language', 'timestamp_granularities'])) throw failure(422, 'asr_invalid_request');
  const model = selectedModel(body, catalog), audio = body.input_audio;
  if (!strictKeys(audio, ['data', 'format']) || typeof audio.data !== 'string' || !FORMATS.has(audio.format)) throw failure(422, 'asr_invalid_request');
  if (audio.data.length > Math.ceil(AUDIO_LIMIT / 3) * 4) throw failure(413, 'asr_audio_too_large');
  if (!audio.data.length || audio.data.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(audio.data)) throw failure(422, 'asr_invalid_request');
  const decoded = Buffer.from(audio.data, 'base64');
  if (decoded.length > AUDIO_LIMIT) throw failure(413, 'asr_audio_too_large');
  if (!decoded.length || decoded.toString('base64') !== audio.data) throw failure(422, 'asr_invalid_request');
  const format = body.response_format ?? 'verbose_json';
  if (!['json', 'verbose_json'].includes(format)) throw failure(422, 'asr_invalid_request');
  if (body.language !== undefined && (typeof body.language !== 'string' || !/^[a-z]{2,3}(?:-[A-Z]{2})?$/.test(body.language))) throw failure(422, 'asr_invalid_request');
  if (body.timestamp_granularities !== undefined && (!Array.isArray(body.timestamp_granularities) || !body.timestamp_granularities.length || body.timestamp_granularities.length > 2 || body.timestamp_granularities.some(v => !['segment', 'word'].includes(v)) || format !== 'verbose_json')) throw failure(422, 'asr_invalid_request');
  return { model, input_audio: audio, response_format: format, ...(body.language === undefined ? {} : { language: body.language }), ...(body.timestamp_granularities === undefined ? {} : { timestamp_granularities: body.timestamp_granularities }) };
}
function transcriptResponse(raw) {
  if (!object(raw) || typeof raw.text !== 'string' || !raw.text.trim()) throw failure(502, 'invalid_transcript_response');
  const out = { text: raw.text };
  if (typeof raw.language === 'string') out.language = raw.language;
  if (Number.isFinite(raw.duration) && raw.duration >= 0) out.duration = raw.duration;
  for (const [field, allowed] of [['segments', ['id', 'start', 'end', 'text', 'speaker', 'confidence']], ['words', ['word', 'start', 'end', 'speaker', 'confidence']]]) {
    if (raw[field] === undefined) continue;
    if (!Array.isArray(raw[field])) throw failure(502, 'invalid_transcript_response');
    out[field] = raw[field].map(row => { if (!object(row)) throw failure(502, 'invalid_transcript_response'); return Object.fromEntries(allowed.filter(key => row[key] !== undefined).map(key => [key, row[key]])); });
  }
  if (object(raw.usage)) {
    out.usage = Object.fromEntries(['seconds', 'input_tokens', 'output_tokens', 'total_tokens', 'cost'].filter(key => Number.isFinite(raw.usage[key]) && raw.usage[key] >= 0).map(key => [key, raw.usage[key]]));
  }
  return out;
}
function nativeRoute(method, path) {
  if (method === 'GET' && ['/doc', '/global/health', '/config', '/provider', '/config/providers', '/experimental/tool/ids'].includes(path)) return true;
  if (method === 'POST' && path === '/session') return true;
  if (/^\/session\/[A-Za-z0-9_-]{1,128}$/.test(path) && ['GET', 'DELETE'].includes(method)) return true;
  return method === 'POST' && /^\/session\/[A-Za-z0-9_-]{1,128}\/message$/.test(path);
}
// Native analysis may legitimately run for 600s. Node fetch's implicit 300s header
// timeout would truncate it, so use a signal-bounded loopback-only HTTP transport.
function nativeRequest(url, init) {
  return new Promise((resolve, reject) => {
    const request = http.request(url, { method: init.method, headers: init.headers, signal: init.signal }, response => {
      resolve(new Response(Readable.toWeb(response), { status: response.statusCode, headers: response.headers }));
    });
    request.once('error', reject);
    request.end(init.body);
  });
}
/** No provider URL or provider secret comes from configuration/env. fetchImpl is for isolated HTTP fixtures. */
export function createGateway({ password, config, fetchImpl = fetch, nativeBase = 'http://127.0.0.1:4097', deadlineMs = 55_000, nativeDeadlineMs = 610_000, localSettings = null, maxSessions = 128 } = {}) {
  if (typeof password !== 'string' || password.length < 24) throw failure(503, 'runner_auth_required');
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(nativeBase)) throw failure(503, 'connector_native_invalid');
  const catalog = modelCatalog(config), expected = Buffer.from('Basic ' + Buffer.from('opencode:' + password).toString('base64'));
  let inFlight = 0, sessionReservations = 0, closing = false;
  const controllers = new Set(), sessions = new Map();
  const server = http.createServer({ maxHeaderSize: 12 * 1024 }, async (req, res) => {
    if (!authenticated(req, expected)) { req.resume(); send(res, 401, { error: { code: 'runner_auth_required', message: 'runner_auth_required' } }); return; }
    const controller = new AbortController(); controllers.add(controller);
    const abortClient = () => controller.abort(failure(499, 'client_disconnected'));
    req.once('aborted', abortClient); res.once('close', () => { if (!res.writableEnded) abortClient(); });
    const nativeMessage = req.method === 'POST' && /^\/session\/[A-Za-z0-9_-]{1,128}\/message$/.test(req.url);
    const timer = setTimeout(() => controller.abort(failure(504, 'provider_timeout')), nativeMessage ? nativeDeadlineMs : deadlineMs); timer.unref();
    let counted = false, reservedSession = false;
    try {
      if (closing) throw failure(503, 'connector_unavailable');
      const url = new URL(req.url, 'http://connector');
      if (url.search || url.pathname !== req.url || /%/.test(req.url)) throw failure(422, 'asr_invalid_request');
      const organization = req.headers['x-phone-organization'];
      const scoped = localAuthorized(localSettings, organization);
      const localModels = localCatalog(localSettings, organization).asr_models;
      if (url.pathname === '/local/providers' && req.method === 'GET') { send(res, 200, localCatalog(localSettings, organization)); return; }
      if (url.pathname === '/local/test' && req.method === 'POST') {
        if (!scoped) throw failure(403, 'local_provider_not_authorized');
        const body = await jsonRequest(req, 4096, controller.signal);
        if (!strictKeys(body, ['model']) || typeof body.model !== 'string') throw failure(422, 'local_model_unavailable');
        const slash = body.model.indexOf('/'), provider = localSettings.providers[body.model.slice(0, slash)], model = body.model.slice(slash + 1);
        if (slash < 1 || !provider?.models[model]) throw failure(422, 'local_model_unavailable');
        const probe = await fetchImpl(provider.options.baseURL + '/models', { method: 'GET', headers: { Authorization: 'Bearer ' + provider.options.apiKey }, redirect: 'error', signal: controller.signal });
        await checked(probe); const metadata = await boundedJson(probe, controller.signal);
        if (!Array.isArray(metadata.data) || !metadata.data.some(item => item.id === model)) throw failure(422, 'local_model_unavailable');
        send(res, 200, { ok: true, model: body.model }); return;
      }
      if (url.pathname === '/asr/models' && req.method === 'GET') { send(res, 200, { models: [...catalog, ...localModels] }); return; }
      if (['/asr/test', '/asr/transcriptions'].includes(url.pathname) && req.method === 'POST') {
        if (inFlight >= 2) { req.resume(); throw failure(429, 'asr_busy', 5); }
        inFlight++; counted = true;
        const body = await jsonRequest(req, url.pathname === '/asr/test' ? 4096 : REQUEST_LIMIT, controller.signal);
        const local = body.model === LOCAL_ASR_MODEL;
        if (local && (!scoped || !localModels.length)) throw failure(403, 'local_provider_not_authorized');
        const model = selectedModel(body, [...catalog, ...localModels]);
        if (local) {
          if (req.headers['x-openrouter-api-key'] !== undefined) throw failure(422, 'asr_invalid_request');
          const provider = localSettings.providers.apinex, headers = { Authorization: 'Bearer ' + provider.options.apiKey };
          if (url.pathname === '/asr/test') {
            if (!strictKeys(body, ['model'])) throw failure(422, 'asr_invalid_request');
            const probe = await fetchImpl(provider.options.baseURL + '/models', { method: 'GET', headers, redirect: 'error', signal: controller.signal });
            await checked(probe); const metadata = await boundedJson(probe, controller.signal);
            if (!Array.isArray(metadata.data)) throw failure(502, 'invalid_transcript_response');
            send(res, 200, { ok: true, model, provider: 'apinex', message: 'Existing local provider authenticated; no audio was submitted.' }); return;
          }
          const payload = transcriptionRequest(body, [...catalog, ...localModels]);
          const mime = { wav: 'audio/wav', mp3: 'audio/mpeg', flac: 'audio/flac', m4a: 'audio/mp4', ogg: 'audio/ogg', webm: 'audio/webm', aac: 'audio/aac', mp4: 'audio/mp4' }[payload.input_audio.format];
          const query = new URLSearchParams({ detect_language: 'true', punctuate: 'true', smart_format: 'true', diarize: 'true', utterances: 'true', ...(payload.language ? { language: payload.language } : {}) });
          const response = await fetchImpl(provider.options.baseURL + '/tools/voice/stt?' + query.toString(), { method: 'POST', headers: { ...headers, 'Content-Type': mime }, body: Buffer.from(payload.input_audio.data, 'base64'), redirect: 'error', signal: controller.signal });
          await checked(response);
          let transcript;
          try { transcript = apinexTranscript(await boundedJson(response, controller.signal)); } catch { throw failure(502, 'invalid_transcript_response'); }
          send(res, 200, transcriptResponse(transcript)); return;
        }
        const key = providerKey(req);
        if (url.pathname === '/asr/test') {
          if (!strictKeys(body, ['model'])) throw failure(422, 'asr_invalid_request');
          const r = await fetchImpl(UPSTREAM + '/key', { method: 'GET', headers: { Authorization: 'Bearer ' + key }, redirect: 'error', signal: controller.signal });
          await checked(r); const metadata = await boundedJson(r, controller.signal);
          if (!object(metadata.data)) throw failure(502, 'invalid_transcript_response');
          send(res, 200, { ok: true, model }); return;
        }
        const payload = transcriptionRequest(body, catalog);
        const r = await fetchImpl(UPSTREAM + '/audio/transcriptions', { method: 'POST', headers: { Authorization: 'Bearer ' + key, 'Content-Type': 'application/json' }, body: JSON.stringify(payload), redirect: 'error', signal: controller.signal });
        await checked(r); send(res, 200, transcriptResponse(await boundedJson(r, controller.signal))); return;
      }
      if (!nativeRoute(req.method, url.pathname)) throw failure(404, 'connector_route_missing');
      const publicNative = req.method === 'GET' && ['/doc', '/global/health'].includes(url.pathname);
      if (localSettings && !publicNative && !scoped) throw failure(403, 'local_provider_not_authorized');
      const sessionId = /^\/session\/([A-Za-z0-9_-]{1,128})(?:\/message)?$/.exec(url.pathname)?.[1];
      if (localSettings && sessionId && sessions.get(sessionId) !== organization) throw failure(403, 'local_session_not_authorized');
      if (req.headers['x-openrouter-api-key'] !== undefined) throw failure(422, 'asr_invalid_request');
      const body = ['POST', 'DELETE'].includes(req.method) && (req.headers['content-length'] || req.headers['transfer-encoding']) ? await jsonRequest(req, 2 * 1024 * 1024, controller.signal) : undefined;
      if (localSettings && req.method === 'POST' && url.pathname === '/session') {
        if (sessions.size + sessionReservations >= maxSessions) throw failure(429, 'connector_session_limit', 5);
        sessionReservations++; reservedSession = true;
      }
      if (localSettings && nativeMessage) {
        const provider = localSettings.providers[body?.model?.providerID];
        if (!provider?.models[body?.model?.modelID]) throw failure(422, 'local_model_unavailable');
      }
      const r = await nativeRequest(nativeBase + url.pathname, { method: req.method, headers: { Authorization: expected.toString(), ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) }, body: body === undefined ? undefined : JSON.stringify(body), redirect: 'error', signal: controller.signal });
      if (!r.ok) { await r.body?.cancel(); throw failure(r.status, 'connector_native_request_failed'); }
      const result = await boundedJson(r, controller.signal);
      if (localSettings && req.method === 'POST' && url.pathname === '/session') {
        if (typeof result.id !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(result.id)) throw failure(502, 'connector_invalid_session');
        sessions.set(result.id, organization);
      }
      if (req.method === 'DELETE' && sessionId) sessions.delete(sessionId);
      send(res, r.status, redactNative(result, localSettings));
    } catch (error) {
      const safe = controller.signal.aborted ? controller.signal.reason : error instanceof GatewayError ? error : failure(502, 'provider_unavailable');
      send(res, safe.status || 502, { error: { code: safe.code || 'provider_unavailable', message: safe.code || 'provider_unavailable' } }, safe.retryAfter);
    } finally { if (reservedSession) sessionReservations--; if (counted) inFlight--; clearTimeout(timer); controllers.delete(controller); req.off('aborted', abortClient); if (!req.complete && !req.destroyed) { req.resume(); res.once('finish', () => req.destroy()); } }
  });
  const codexProxy = attachCodexProxy(server, { password });
  server.requestTimeout = 60_000; server.headersTimeout = 10_000; server.keepAliveTimeout = 5000; server.maxRequestsPerSocket = 100;
  return { server, async close() { closing = true; codexProxy.close(); sessions.clear(); for (const c of controllers) c.abort(failure(503, 'connector_unavailable')); await new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }); } };
}
