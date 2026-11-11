import http from 'node:http';
const token = Buffer.from(`opencode:${process.env.OPENCODE_SERVER_PASSWORD || ''}`).toString('base64');
const request = http.get('http://127.0.0.1:4096/global/health', {
  headers: { Authorization: `Basic ${token}` }, timeout: 3000
}, (response) => {
  let text = '';
  response.on('data', chunk => { text += chunk; if (text.length > 8192) response.destroy(); });
  response.on('end', () => {
    try {
      const body = JSON.parse(text);
      process.exit(response.statusCode === 200 && body.healthy === true && body.version === '1.18.10' ? 0 : 1);
    } catch { process.exit(1); }
  });
});
request.on('timeout', () => request.destroy());
request.on('error', () => process.exit(1));
