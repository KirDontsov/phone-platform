import net from 'node:net';
import dns from 'node:dns/promises';
import { timingSafeEqual } from 'node:crypto';

const TARGETS = new Set(['auth.openai.com:443', 'api.openai.com:443', 'chatgpt.com:443']);
export function publicAddress(address) {
  if (net.isIP(address) === 4) {
    const [a,b,c] = address.split('.').map(Number);
    return !(a === 0 || a === 10 || a === 127 || a >= 224 ||
      a === 100 && b >= 64 && b <= 127 || a === 169 && b === 254 ||
      a === 172 && b >= 16 && b <= 31 || a === 192 && (b === 168 || b === 0 || b === 2) ||
      a === 198 && (b === 18 || b === 19 || b === 51 && c === 100) || a === 203 && b === 0 && c === 113);
  }
  if (net.isIP(address) === 6) {
    const lower = address.toLowerCase();
    return /^[23][0-9a-f]{0,3}:/.test(lower) && !/^2001:(?:db8|0|10|20):/.test(lower) && !lower.startsWith('2002:');
  }
  return false;
}

/** Exact-host opaque TLS transport. No destination, credential or HTTP body is logged. */
export function attachCodexProxy(server, {
  password, lookup = dns.lookup, connect = net.connect,
  maxConnections = 4, resolveMs = 3000, connectMs = 7000,
  idleMs = 90000, lifetimeMs = 600000,
} = {}) {
  if (typeof password !== 'string' || password.length < 24) throw new Error('codex_proxy_password_required');
  const expected = Buffer.from('Basic ' + Buffer.from('opencode:' + password).toString('base64'));
  const clients = new Set(), upstreams = new Set();
  let closed = false;
  function deny(socket, status) {
    if (socket.destroyed) return;
    socket.end(`HTTP/1.1 ${status}\r\nConnection: close\r\nContent-Length: 0\r\n${status.startsWith('407') ? 'Proxy-Authenticate: Basic realm="phone-codex"\r\n' : ''}\r\n`);
    const timer = setTimeout(() => socket.destroy(), 500); timer.unref(); socket.once('close', () => clearTimeout(timer));
  }
  async function tunnel(req, client, head) {
    const actual = Buffer.from(typeof req.headers['proxy-authorization'] === 'string' ? req.headers['proxy-authorization'] : '');
    if (actual.length !== expected.length || !timingSafeEqual(actual, expected)) return deny(client, '407 Proxy Authentication Required');
    if (!TARGETS.has(req.url) || head.length > 16384) return deny(client, '403 Forbidden');
    if (closed || clients.size >= maxConnections) return deny(client, '503 Service Unavailable');
    clients.add(client);
    let upstream, connected = false, resolveTimer, connectionTimer;
    const lifetime = setTimeout(() => client.destroy(), lifetimeMs); lifetime.unref();
    const cleanup = () => {
      clearTimeout(lifetime); clearTimeout(resolveTimer); clearTimeout(connectionTimer);
      clients.delete(client); if (upstream) { upstreams.delete(upstream); upstream.destroy(); }
    };
    client.once('close', cleanup); client.on('error', () => client.destroy());
    client.setTimeout(idleMs, () => client.destroy());
    try {
      const host = req.url.slice(0, -4);
      const addresses = await Promise.race([
        lookup(host, { all: true, verbatim: true }),
        new Promise((_, reject) => { resolveTimer = setTimeout(() => reject(new Error('dns_timeout')), resolveMs); resolveTimer.unref(); }),
      ]);
      clearTimeout(resolveTimer);
      if (closed || client.destroyed) return;
      if (!Array.isArray(addresses) || addresses.length === 0 || addresses.length > 32 || addresses.some(a => !publicAddress(a.address) || ![4,6].includes(a.family))) {
        deny(client, '403 Forbidden'); return;
      }
      const chosen = addresses.find(a => a.family === 4) ?? addresses[0];
      upstream = connect({ host: chosen.address, family: chosen.family, port: 443 });
      upstreams.add(upstream);
      connectionTimer = setTimeout(() => { upstream.destroy(); deny(client, '504 Gateway Timeout'); }, connectMs); connectionTimer.unref();
      upstream.setTimeout(idleMs, () => upstream.destroy());
      upstream.on('error', () => { if (!connected) deny(client, '502 Bad Gateway'); else client.destroy(); });
      upstream.once('close', () => { upstreams.delete(upstream); if (connected) client.destroy(); });
      upstream.once('connect', () => {
        clearTimeout(connectionTimer);
        if (closed || client.destroyed) { upstream.destroy(); return; }
        connected = true;
        client.write('HTTP/1.1 200 Connection Established\r\n\r\n');
        if (head.length) upstream.write(head);
        client.pipe(upstream); upstream.pipe(client);
      });
    } catch { deny(client, '502 Bad Gateway'); }
  }
  server.on('connect', tunnel);
  const controller = {
    get activeCount() { return clients.size; },
    close() { closed = true; for (const socket of clients) socket.destroy(); for (const socket of upstreams) socket.destroy(); },
  };
  server.once('close', () => controller.close());
  return controller;
}
