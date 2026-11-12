import fs from 'node:fs';
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const ID = /^[a-zA-Z0-9_-]{1,64}$/;
const object = value => value && typeof value === 'object' && !Array.isArray(value);
export const LOCAL_ASR_MODEL = 'apinex/voice-stt';
export function parseLocalSettings(raw) {
  if (!raw) return null;
  const value = typeof raw === 'string' ? JSON.parse(raw) : raw;
  if (!object(value) || value.version !== 1 || !UUID.test(value.organization_id) || !object(value.providers)) throw Error('local_provider_config_invalid');
  const providers = {};
  for (const [id, provider] of Object.entries(value.providers)) {
    if (!ID.test(id) || !object(provider) || !object(provider.options) || typeof provider.options.apiKey !== 'string' || provider.options.apiKey.length < 8 || /[\r\n\0]/.test(provider.options.apiKey)) throw Error('local_provider_config_invalid');
    const endpoint = new URL(provider.options.baseURL);
    if (!['https:', 'http:'].includes(endpoint.protocol) || endpoint.username || endpoint.password || endpoint.search || endpoint.hash || endpoint.protocol === 'http:' && !['host.docker.internal', '127.0.0.1', 'localhost'].includes(endpoint.hostname)) throw Error('local_provider_config_invalid');
    if (!object(provider.models) || Object.keys(provider.models).length > 256 || Object.keys(provider.models).some(model => model.length > 200 || /[\r\n\0]/.test(model))) throw Error('local_provider_config_invalid');
    providers[id] = { name: typeof provider.name === 'string' ? provider.name.slice(0, 150) : id, npm: '@ai-sdk/openai-compatible', options: { baseURL: endpoint.toString().replace(/\/$/, ''), apiKey: provider.options.apiKey }, models: Object.fromEntries(Object.entries(provider.models).map(([model, definition]) => [model, { name: typeof definition?.name === 'string' ? definition.name.slice(0, 150) : model }])) };
  }
  if (Object.keys(providers).length > 16) throw Error('local_provider_config_invalid');
  return { version: 1, organization_id: value.organization_id.toLowerCase(), providers };
}
export function readLocalSettings(file) {
  if (!fs.existsSync(file)) return null;
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.mode & 0o077 || stat.size > 1024 * 1024) throw Error('local_provider_config_unsafe');
  return parseLocalSettings(fs.readFileSync(file, 'utf8'));
}
export function localAuthorized(settings, organization) { return !!settings && typeof organization === 'string' && UUID.test(organization) && settings.organization_id === organization.toLowerCase(); }
export function localCatalog(settings, organization) {
  if (!localAuthorized(settings, organization)) return { available: false, providers: [], asr_models: [] };
  const asr = settings.providers.apinex?.options.baseURL === 'https://api.apinex.bond/v1';
  return { available: true, providers: Object.entries(settings.providers).map(([id, provider]) => ({ id, name: provider.name, models: Object.entries(provider.models).map(([model, definition]) => ({ id: `${id}/${model}`, name: definition.name })) })), asr_models: asr ? [{ id: LOCAL_ASR_MODEL, name: 'APInex Voice STT', provider: 'apinex' }] : [] };
}
export function redactNative(value, settings) {
  const secrets = Object.values(settings?.providers || {}).map(provider => provider.options.apiKey);
  const walk = current => {
    if (typeof current === 'string') { for (const secret of secrets) current = current.split(secret).join('<configured>'); return current; }
    if (Array.isArray(current)) return current.map(walk);
    if (!object(current)) return current;
    return Object.fromEntries(Object.entries(current).map(([key, item]) => [key, /apikey|token|secret|authorization|password|accesskey|credential|headers/i.test(key.replace(/[^a-z]/ig, '')) ? '<configured>' : walk(item)]));
  };
  return walk(value);
}
export function apinexTranscript(raw) {
  const alternative = raw?.results?.channels?.[0]?.alternatives?.[0];
  if (!object(alternative) || typeof alternative.transcript !== 'string' || alternative.transcript.length > 2 * 1024 * 1024) throw Error('invalid_transcript_response');
  const out = { text: alternative.transcript };
  if (Array.isArray(alternative.words)) {
    out.segments = alternative.words.map((word, index) => {
      if (!object(word) || !Number.isFinite(word.start) || !Number.isFinite(word.end) || word.start < 0 || word.end < word.start || typeof (word.punctuated_word ?? word.word) !== 'string') throw Error('invalid_transcript_response');
      return { id: `s${index + 1}`, start: word.start, end: word.end, text: word.punctuated_word ?? word.word, speaker: Number.isInteger(word.speaker) && word.speaker >= 0 ? `speaker_${word.speaker}` : 'unknown', role: 'unknown' };
    });
  }
  // APInex bills internal tokens, which are not a dollar cost or LLM token usage.
  if (Number.isFinite(raw?.metadata?.duration) && raw.metadata.duration >= 0) out.usage = { seconds: raw.metadata.duration };
  return out;
}
