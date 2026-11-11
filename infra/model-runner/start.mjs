import fs from 'node:fs';
import { spawn } from 'node:child_process';

const password = process.env.OPENCODE_SERVER_PASSWORD;
if (!password || password.length < 24) {
  process.stderr.write('model_runner_password_missing_or_short\n');
  process.exit(1);
}
const configPath = '/etc/opencode/opencode.json';
const config = fs.readFileSync(configPath, 'utf8');
for (const path of ['/tmp/runner/home', '/tmp/runner/data', '/tmp/runner/cache', '/tmp/runner/config']) {
  fs.mkdirSync(path, { recursive: true, mode: 0o700 });
}
// A narrow environment prevents accidental inheritance of application DB/broker/session secrets.
const env = {
  PATH: process.env.PATH || '/usr/local/bin:/usr/bin:/bin',
  HOME: '/tmp/runner/home',
  XDG_DATA_HOME: '/tmp/runner/data',
  XDG_CACHE_HOME: '/tmp/runner/cache',
  XDG_CONFIG_HOME: '/tmp/runner/config',
  TMPDIR: '/tmp',
  OPENCODE_CONFIG: configPath,
  OPENCODE_CONFIG_CONTENT: config,
  OPENCODE_DISABLE_PROJECT_CONFIG: 'true',
  OPENCODE_DISABLE_DEFAULT_PLUGINS: 'true',
  OPENCODE_DISABLE_MODELS_FETCH: 'true',
  OPENCODE_DISABLE_AUTOUPDATE: 'true',
  OPENCODE_SERVER_PASSWORD: password,
  OPENCODE_SERVER_USERNAME: 'opencode'
};
for (const [key, value] of Object.entries(process.env)) {
  if (/^[A-Z][A-Z0-9_]*(API_KEY|API_TOKEN)$/.test(key) && typeof value === 'string') env[key] = value;
}
const child = spawn('/usr/local/bin/opencode', ['serve', '--hostname', '0.0.0.0', '--port', '4096'], {
  cwd: '/work', env, stdio: ['ignore', 'inherit', 'inherit'], shell: false
});
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => child.kill(signal));
child.on('error', () => { process.stderr.write('model_runner_start_failed\n'); process.exit(1); });
child.on('exit', (code, signal) => { process.exit(code ?? (signal ? 1 : 0)); });
