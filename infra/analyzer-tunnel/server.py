#!/usr/bin/env python3
"""Dedicated reverse-forward relay. No application DB or model-provider credentials."""
import base64
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.request

STATE = Path('/var/lib/phone-tunnel')
PUBLIC = Path('/config/client_key.pub')
KEY = STATE / 'ssh_host_ed25519_key'


def public_key(text):
    rows = [line.strip() for line in text.splitlines() if line.strip() and not line.startswith('#')]
    if len(rows) != 1 or not re.fullmatch(r'ssh-ed25519 [A-Za-z0-9+/]+={0,2}(?: [^\r\n]*)?', rows[0]):
        raise ValueError('invalid_phone_public_key')
    kind, key, *_ = rows[0].split()
    raw = base64.b64decode(key, validate=True)
    if len(raw) != 51:
        raise ValueError('invalid_phone_public_key')
    return f'{kind} {key} phone-analyzer'


def private_write(path, text):
    flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, 'O_NOFOLLOW', 0)
    fd = os.open(path, flags, 0o600)
    with os.fdopen(fd, 'w') as stream:
        stream.write(text)
    os.chmod(path, 0o600)


def prepare():
    STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(STATE, 0o700)
    if not KEY.exists():
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-C', 'phone-model-tunnel', '-f', str(KEY)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if KEY.is_symlink() or not KEY.is_file():
        raise ValueError('unsafe_phone_host_key')
    os.chmod(KEY, 0o600)
    key = public_key(PUBLIC.read_text())
    private_write(STATE / 'authorized_keys', f'restrict,port-forwarding,permitlisten="0.0.0.0:4096" {key}\n')


def health():
    # This forced command is the only permitted SSH session. It probes the actual
    # reverse-forward target, so a healthy SSH control socket alone is insufficient.
    if os.environ.get('SSH_ORIGINAL_COMMAND') not in (None, '', 'health'):
        return 1
    password = os.environ.get('MODEL_RUNNER_PASSWORD', '')
    # sshd sanitizes session environment: read the server-owned private health token.
    if not password:
        try:
            password = (STATE / 'runner_password').read_text().strip()
        except OSError:
            return 1
    if len(password) < 24:
        return 1
    token = base64.b64encode(('opencode:' + password).encode()).decode()
    request = urllib.request.Request('http://127.0.0.1:4096/global/health', headers={'Authorization': 'Basic ' + token})
    try:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with opener.open(request, timeout=4) as response:
            data = response.read(8193)
            if response.status != 200 or len(data) > 8192:
                return 1
            result = json.loads(data)
            if result.get('healthy') is not True or result.get('version') != '1.18.10':
                return 1
        print('{"ssh_forward":true,"connector":true,"pairing":true}')
        return 0
    except Exception:
        return 1


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else 'serve'
    if command == 'public-key':
        print(public_key((KEY.with_suffix('.pub')).read_text()))
        return 0
    if command == 'health':
        return health()
    if command != 'serve':
        return 2
    prepare()
    password = os.environ.get('MODEL_RUNNER_PASSWORD', '')
    if len(password) < 24 or any(c in password for c in '\r\n\0'):
        raise ValueError('phone_pairing_password_missing')
    private_write(STATE / 'runner_password', password)
    os.execv('/usr/sbin/sshd', ['/usr/sbin/sshd', '-D', '-e', '-f', '/etc/phone-sshd_config'])

if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError):
        print('phone_tunnel_server_configuration_failed', file=sys.stderr)
        raise SystemExit(1)
