import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import { once } from 'node:events';
import { createGateway, modelCatalog, ASR_MODEL_IDS } from '../gateway.mjs';
const config = JSON.parse(fs.readFileSync(new URL('../opencode.json', import.meta.url), 'utf8'));
const password = 'synthetic-transport-password-123456789';
const authorization = 'Basic ' + Buffer.from('opencode:' + password).toString('base64');
const audio = Buffer.from('RIFF synthetic isolated fixture').toString('base64');
function payload(model = ASR_MODEL_IDS[0]) { return { model, input_audio: { data: audio, format: 'wav' }, response_format: 'verbose_json' }; }
async function listen(server) { server.listen(0, '127.0.0.1'); await once(server, 'listening'); return 'http://127.0.0.1:' + server.address().port; }
async function close(server) { await new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }); }
async function setup(t, handler, options = {}) {
  const seen = [], native = [];
  const upstream = http.createServer(async (req, res) => {
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const call = { path: req.url, method: req.method, key: req.headers.authorization, body: Buffer.concat(chunks).toString() }; seen.push(call);
    if (handler) return handler(call, req, res);
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(req.url.endsWith('/key') ? { data: { label: 'private account', limit_remaining: 5 } } : { text: 'Синтетический диалог', segments: [{ start: 0, end: 2, text: 'Здравствуйте', speaker: 0 }, { start: 2, end: 4, text: 'Добрый день', speaker: 1 }], usage: { seconds: 4, cost: 0.0001 } }));
  });
  const upstreamBase = await listen(upstream);
  const backend = http.createServer(async (req, res) => { const chunks = []; for await (const chunk of req) chunks.push(chunk); native.push({ path: req.url, headers: req.headers, body: Buffer.concat(chunks).toString() }); if (options.nativeHandler) return options.nativeHandler(req, res); res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(req.url === '/global/health' ? { healthy: true, version: '1.18.10' } : req.method === 'DELETE' ? true : { id: 'ses_fixture', permission: { '*': 'deny' } })); });
  const nativeBase = await listen(backend);
  const gateway = createGateway({ password, config, nativeBase, fetchImpl: async (url, init) => { if (url.startsWith('https://openrouter.ai/')) { assert.ok(['https://openrouter.ai/api/v1/key', 'https://openrouter.ai/api/v1/audio/transcriptions'].includes(url)); assert.equal(init.redirect, 'error'); return fetch(upstreamBase + new URL(url).pathname, init); } return fetch(url, init); }, ...options });
  const base = await listen(gateway.server);
  t.after(async () => { await gateway.close(); await close(upstream); await close(backend); });
  return { base, seen, native, gateway, request(path, body, key = 'synthetic-tenant-key-alpha', extra = {}) { return fetch(base + path, { method: body === undefined ? 'GET' : 'POST', headers: { Authorization: authorization, ...(body === undefined ? {} : { 'Content-Type': 'application/json', 'X-OpenRouter-Api-Key': key }), ...extra }, body: body === undefined ? undefined : JSON.stringify(body) }); } };
}
async function code(response, status, expected) { assert.equal(response.status, status); const value = await response.json(); assert.equal(value.error.code, expected); assert.equal(value.error.message, expected); return value; }

test('both exact model IDs come from hardened native configuration without changing permissions', () => {
  assert.deepEqual(modelCatalog(config).map(m => m.id), ASR_MODEL_IDS);
  assert.equal(config.permission['*'], 'deny'); assert.ok(Object.values(config.tools).every(v => v === false)); assert.deepEqual(config.mcp, {}); assert.deepEqual(config.plugin, []);
  for (const id of ASR_MODEL_IDS) { assert.deepEqual(config.provider.openrouter.models[id].modalities, { input: ['audio'], output: ['text'] }); assert.equal(config.provider.openrouter.models[id].tool_call, false); }
  assert.throws(() => modelCatalog({}), /connector_catalog_invalid/);
});
test('catalog requires runner authentication and makes no provider call', async t => {
  const h = await setup(t); await code(await fetch(h.base + '/asr/models'), 401, 'runner_auth_required');
  const r = await h.request('/asr/models'); assert.equal(r.status, 200); assert.deepEqual((await r.json()).models.map(m => m.id), ASR_MODEL_IDS); assert.equal(h.seen.length, 0);
});
test('both models use STT JSON with independent tenant credentials, numeric speakers and real usage', async t => {
  const h = await setup(t);
  for (const [index, id] of ASR_MODEL_IDS.entries()) { const r = await h.request('/asr/transcriptions', payload(id), 'synthetic-tenant-key-' + index); assert.equal(r.status, 200); const result = await r.json(); assert.equal(result.segments[0].speaker, 0); assert.equal(result.segments[1].speaker, 1); assert.equal(result.usage.cost, 0.0001); assert.equal(h.seen[index].path, '/api/v1/audio/transcriptions'); assert.equal(h.seen[index].key, 'Bearer synthetic-tenant-key-' + index); assert.deepEqual(JSON.parse(h.seen[index].body), payload(id)); }
  assert.equal(h.native.length, 0);
});
test('credential test checks upstream key without inference and hides account metadata', async t => {
  const h = await setup(t); const r = await h.request('/asr/test', { model: ASR_MODEL_IDS[0] }); assert.deepEqual(await r.json(), { ok: true, model: ASR_MODEL_IDS[0] }); assert.equal(h.seen.length, 1); assert.equal(h.seen[0].path, '/api/v1/key'); assert.equal(h.seen[0].method, 'GET'); assert.equal(h.seen[0].body, '');
});
test('no environment fallback for missing provider key, unknown model and untrusted controls rejected before upstream', async t => {
  const h = await setup(t);
  await code(await h.request('/asr/transcriptions', payload(), ''), 422, 'asr_provider_key_required');
  await code(await h.request('/asr/transcriptions', payload('invented/model')), 422, 'asr_model_unsupported');
  for (const extra of [{ base_url: 'https://evil.invalid' }, { provider: { only: ['evil'] } }, { headers: { Authorization: 'evil' } }]) await code(await h.request('/asr/transcriptions', { ...payload(), ...extra }), 422, 'asr_invalid_request');
  assert.equal(h.seen.length, 0);
});
test('malformed audio, data URIs, unsupported formats and invalid options fail before upload', async t => {
  const h = await setup(t);
  for (const data of ['', 'bad', 'AA=A', 'data:audio/wav;base64,AAAA']) await code(await h.request('/asr/transcriptions', { ...payload(), input_audio: { data, format: 'wav' } }), 422, 'asr_invalid_request');
  for (const extra of [{ response_format: 'srt' }, { language: 'ru\n' }, { timestamp_granularities: ['invented'] }]) await code(await h.request('/asr/transcriptions', { ...payload(), ...extra }), 422, 'asr_invalid_request');
  await code(await h.request('/asr/transcriptions', { ...payload(), input_audio: { data: audio, format: 'exe' } }), 422, 'asr_invalid_request'); assert.equal(h.seen.length, 0);
});
test('native health/config/session paths remain compatible but cannot receive tenant provider key', async t => {
  const h = await setup(t); const r = await h.request('/global/health'); assert.deepEqual(await r.json(), { healthy: true, version: '1.18.10' });
  await code(await h.request('/config', undefined, undefined, { 'X-OpenRouter-Api-Key': 'synthetic-private-key' }), 422, 'asr_invalid_request'); assert.equal(h.native.length, 1); assert.equal(h.native[0].headers['x-openrouter-api-key'], undefined);
  for (const path of ['/auth/openrouter', '/config?directory=/private', '/%73ession', '/api/session', '/session/id/shell']) await code(await h.request(path), path.includes('?') || path.includes('%') ? 422 : 404, path.includes('?') || path.includes('%') ? 'asr_invalid_request' : 'connector_route_missing');
});
test('unauthorized headers fail before waiting for a declared huge body', async t => {
  const h = await setup(t); const result = await new Promise((resolve, reject) => { const req = http.request(h.base + '/asr/transcriptions', { method: 'POST', headers: { 'Content-Length': 100_000_000, 'Content-Type': 'application/json' } }, res => { res.resume(); res.on('end', () => { req.destroy(); resolve(res.statusCode); }); }); req.on('error', () => {}); req.flushHeaders(); setTimeout(() => { req.destroy(); reject(Error('auth did not reject before body')); }, 1000).unref(); }); assert.equal(result, 401); assert.equal(h.seen.length, 0);
});
test('declared oversized request and decoded audio cap reject before upstream', async t => {
  const h = await setup(t);
  const response = await new Promise((resolve, reject) => { const req = http.request(h.base + '/asr/transcriptions', { method: 'POST', headers: { Authorization: authorization, 'Content-Type': 'application/json', 'X-OpenRouter-Api-Key': 'synthetic-tenant-key', 'Content-Length': 36 * 1024 * 1024 } }, res => { const chunks = []; res.on('data', c => chunks.push(c)); res.on('end', () => { req.destroy(); resolve({ status: res.statusCode, body: JSON.parse(Buffer.concat(chunks).toString()) }); }); }); req.on('error', () => {}); req.flushHeaders(); setTimeout(() => { req.destroy(); reject(Error('oversized declaration was read')); }, 1000).unref(); }); assert.equal(response.status, 413);
  const large = Buffer.alloc(24 * 1024 * 1024 + 1).toString('base64'); await code(await h.request('/asr/transcriptions', { ...payload(), input_audio: { data: large, format: 'wav' } }), 413, 'asr_audio_too_large'); assert.equal(h.seen.length, 0);
});
test('upstream quota/auth/error text never leaks and retry delay stays bounded', async t => {
  let status = 429; const h = await setup(t, (_call, _req, res) => { res.writeHead(status, { 'Content-Type': 'application/json', 'Retry-After': '900000' }); res.end(JSON.stringify({ error: { message: 'synthetic-secret-and-raw-provider-text' } })); });
  const r = await h.request('/asr/transcriptions', payload()); assert.equal(r.headers.get('retry-after'), '3600'); const safe = await code(r, 429, 'provider_rate_limited'); assert.ok(!JSON.stringify(safe).includes('synthetic-secret'));
  for (const [s, expected, error] of [[401, 401, 'provider_unauthorized'], [403, 403, 'provider_forbidden'], [400, 422, 'provider_request_failed'], [503, 502, 'provider_unavailable']]) { status = s; await code(await h.request('/asr/transcriptions', payload()), expected, error); }
});
test('provider redirects never follow and no fallback request is made', async t => {
  const h = await setup(t, (_call, _req, res) => { res.writeHead(302, { Location: 'http://127.0.0.1:1/private' }); res.end('private provider error'); }); await code(await h.request('/asr/transcriptions', payload()), 502, 'provider_unavailable'); assert.equal(h.seen.length, 1);
});
test('oversized and invalid transcript responses are rejected, no raw payload returned', async t => {
  let huge = true; const h = await setup(t, (_call, _req, res) => { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(huge ? JSON.stringify({ text: 'x'.repeat(8 * 1024 * 1024) }) : JSON.stringify({ segments: [], private_key: 'synthetic-secret' })); }); await code(await h.request('/asr/transcriptions', payload()), 502, 'asr_response_too_large'); huge = false; await code(await h.request('/asr/transcriptions', payload()), 502, 'invalid_transcript_response');
});
test('provider metadata and forged roles are stripped, missing cost stays unknown', async t => {
  const h = await setup(t, (_call, _req, res) => { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ text: 'Synthetic text', api_key: 'synthetic-secret', segments: [{ start: 0, end: 1, speaker: 0, text: 'Synthetic text', role: 'manager' }], usage: { seconds: 1, cost: null, private_key: 'synthetic-secret' } })); }); const body = await (await h.request('/asr/transcriptions', payload())).json(); assert.equal(body.api_key, undefined); assert.equal(body.segments[0].role, undefined); assert.equal(body.usage.cost, undefined); assert.equal(body.usage.private_key, undefined);
});
test('concurrency gate runs before reading another body and aborts held uploads cleanly', async t => {
  const h = await setup(t); const held = [];
  for (let i = 0; i < 2; i++) { const req = http.request(h.base + '/asr/transcriptions', { method: 'POST', headers: { Authorization: authorization, 'Content-Type': 'application/json', 'X-OpenRouter-Api-Key': 'synthetic-tenant-key', 'Content-Length': 10000 } }); req.on('error', () => {}); req.flushHeaders(); held.push(req); }
  await new Promise(resolve => setTimeout(resolve, 50));
  const response = await h.request('/asr/transcriptions', payload()); await code(response, 429, 'asr_busy'); assert.equal(h.seen.length, 0); for (const req of held) req.destroy();
});
test('deadline cancels upstream inference without a hidden retry', async t => {
  let closed = false; const h = await setup(t, (_call, _req, res) => { res.on('close', () => { closed = true; }); }, { deadlineMs: 60 }); await code(await h.request('/asr/transcriptions', payload()), 504, 'provider_timeout'); await new Promise(resolve => setTimeout(resolve, 20)); assert.equal(closed, true); assert.equal(h.seen.length, 1);
});
test('client disconnect and shutdown cancel upstream work', async t => {
  let closed = 0; const h = await setup(t, (_call, _req, res) => { res.on('close', () => { closed++; }); });
  const controller = new AbortController(); const call = fetch(h.base + '/asr/transcriptions', { method: 'POST', headers: { Authorization: authorization, 'Content-Type': 'application/json', 'X-OpenRouter-Api-Key': 'synthetic-tenant-key' }, body: JSON.stringify(payload()), signal: controller.signal }).catch(() => {});
  while (!h.seen.length) await new Promise(resolve => setTimeout(resolve, 10)); controller.abort(); await call; await new Promise(resolve => setTimeout(resolve, 30)); assert.equal(closed, 1);
  const again = h.request('/asr/transcriptions', payload()).catch(() => {}); while (h.seen.length < 2) await new Promise(resolve => setTimeout(resolve, 10)); await h.gateway.close(); await again; await new Promise(resolve => setTimeout(resolve, 20)); assert.equal(closed, 2);
});


test('native analysis retains its long deadline independently of the short ASR deadline', async t => {
  const h = await setup(t, undefined, { deadlineMs: 20, nativeDeadlineMs: 600, nativeHandler(_req, res) { setTimeout(() => { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ parts: [{ type: 'text', text: 'Long synthetic analysis finished' }] })); }, 80); } });
  const response = await fetch(h.base + '/session/ses_fixture/message', { method: 'POST', headers: { Authorization: authorization, 'Content-Type': 'application/json' }, body: JSON.stringify({ parts: [{ type: 'text', text: 'Synthetic input' }] }) });
  assert.equal(response.status, 200); assert.equal((await response.json()).parts[0].text, 'Long synthetic analysis finished'); assert.equal(h.seen.length, 0);
});
