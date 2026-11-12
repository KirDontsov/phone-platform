#!/usr/bin/env python3
"""Rootless SSH client with explicit Phone credentials and strict host-key pinning."""
import base64
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

STATE = Path('/var/lib/phone-tunnel')
CONFIG = STATE / 'ssh_config'
CONTROL = STATE / 'control'
PRIVATE = STATE / 'id_ed25519'
KNOWN = STATE / 'known_hosts'


def config_values(env):
    host, user = env.get('PHONE_REMOTE_SSH_HOST', ''), env.get('PHONE_REMOTE_SSH_USER', '')
    if not re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9.-]{0,251}[A-Za-z0-9])?', host) or host.endswith('.invalid'):
        raise ValueError('phone_remote_host_required')
    if not re.fullmatch(r'[a-zA-Z_][a-zA-Z0-9_-]{0,63}', user):
        raise ValueError('phone_remote_user_required')
    def port(name, default):
        value = env.get(name, str(default))
        if not str(value).isdigit() or not 1 <= int(value) <= 65535:
            raise ValueError('phone_remote_port_invalid')
        return int(value)
    return host, user, port('PHONE_REMOTE_SSH_PORT', 22), port('PHONE_REMOTE_TUNNEL_SSH_PORT', 18422)


def render_config(env, state=STATE):
    host, user, port, relay = config_values(env)
    return f'''Host phone-jump
    HostName {host}
    User {user}
    Port {port}
    HostKeyAlias phone-analyzer-jump
    ControlMaster no
    ControlPath none
Host phone-model-tunnel
    HostName 127.0.0.1
    User phonebridge
    Port {relay}
    HostKeyAlias phone-model-tunnel
    ProxyJump phone-jump
Host *
    IdentityFile {state}/id_ed25519
    IdentitiesOnly yes
    IdentityAgent none
    UserKnownHostsFile {state}/known_hosts
    GlobalKnownHostsFile /dev/null
    StrictHostKeyChecking yes
    UpdateHostKeys no
    BatchMode yes
    PasswordAuthentication no
    KbdInteractiveAuthentication no
    ForwardAgent no
    RequestTTY no
    ConnectTimeout 10
    ConnectionAttempts 1
    ServerAliveInterval 15
    ServerAliveCountMax 3
    ExitOnForwardFailure yes
    ControlMaster auto
    ControlPath {state}/control
    ControlPersist no
'''


def secure_write(path, text):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'w') as stream:
        stream.write(text)
    os.chmod(path, 0o600)


def install_credentials():
    data = sys.stdin.buffer.read(128 * 1024 + 1)
    if len(data) > 128 * 1024:
        raise ValueError('credentials_too_large')
    value = json.loads(data)
    private, known = value['private_key'], value['known_hosts']
    if not isinstance(private, str) or not private.startswith('-----BEGIN OPENSSH PRIVATE KEY-----') or len(private) > 16 * 1024:
        raise ValueError('phone_private_key_invalid')
    if not isinstance(known, str) or len(known) > 64 * 1024:
        raise ValueError('phone_known_hosts_invalid')
    aliases = {line.split()[0] for line in known.splitlines() if line.strip() and not line.startswith('#')}
    if not {'phone-analyzer-jump', 'phone-model-tunnel'}.issubset(aliases):
        raise ValueError('phone_host_keys_not_pinned')
    rendered = render_config(os.environ)
    STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(STATE, 0o700)
    candidate = STATE / f'.candidate-{os.getpid()}'
    marker = STATE / '.installing'
    try:
        secure_write(candidate, private)
        # Validate before touching a live connection or any existing credential.
        subprocess.run(['ssh-keygen', '-y', '-P', '', '-f', str(candidate)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        secure_write(marker, 'installing')
        for destination, text in [(PRIVATE, private), (KNOWN, known), (CONFIG, rendered)]:
            staging = STATE / ('.' + destination.name + '-next')
            secure_write(staging, text)
            os.replace(staging, destination)
        # Only this project's own multiplexed master is closed. The serving loop
        # reconnects using the new key, pinned host keys and validated destination.
        if CONTROL.exists():
            subprocess.run(['ssh', '-F', str(CONFIG), '-S', str(CONTROL), '-O', 'exit', 'phone-model-tunnel'], timeout=5, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    finally:
        candidate.unlink(missing_ok=True)
        marker.unlink(missing_ok=True)


def health():
    if (STATE / '.installing').exists() or not CONFIG.is_file() or not CONTROL.exists():
        return 1
    result = subprocess.run(['ssh', '-F', str(CONFIG), '-S', str(CONTROL), '-o', 'ConnectTimeout=4', 'phone-model-tunnel', 'health'], timeout=6, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    if result.returncode:
        return 1
    try:
        value = json.loads(result.stdout)
        return 0 if value == {'ssh_forward': True, 'connector': True, 'pairing': True} else 1
    except (ValueError, TypeError):
        return 1


def serve():
    stopped = False
    child = None
    def stop(*_):
        nonlocal stopped
        stopped = True
        if child and child.poll() is None:
            child.terminate()
    for sig in [signal.SIGTERM, signal.SIGINT]:
        signal.signal(sig, stop)
    backoff = 2
    # A recreated container must not reconnect to an old persisted destination.
    if CONFIG.is_file() and CONFIG.read_text() != render_config(os.environ):
        CONFIG.unlink()
    while not stopped:
        if (STATE / '.installing').exists() or not all(p.is_file() for p in [CONFIG, PRIVATE, KNOWN]):
            time.sleep(1)
            continue
        CONTROL.unlink(missing_ok=True)
        child = subprocess.Popen(['ssh', '-F', str(CONFIG), '-N', '-T', '-R', '0.0.0.0:4096:model-runner:4096', 'phone-model-tunnel'], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        started = time.monotonic()
        while child.poll() is None and not stopped:
            time.sleep(.25)
        if stopped:
            if child.poll() is None:
                child.terminate()
            try:
                child.wait(timeout=8)
            except subprocess.TimeoutExpired:
                child.kill(); child.wait()
            break
        print('phone_tunnel_disconnected_retrying', flush=True)
        if time.monotonic() - started > 45:
            backoff = 2
        deadline = time.monotonic() + backoff
        while not stopped and time.monotonic() < deadline:
            time.sleep(.25)
        backoff = min(backoff * 2, 30)
    CONTROL.unlink(missing_ok=True)
    return 0


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else 'serve'
    if command == 'install-credentials':
        install_credentials(); print('phone_tunnel_credentials_installed'); return 0
    if command == 'health':
        return health()
    if command != 'serve':
        return 2
    config_values(os.environ)
    return serve()

if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
        print('phone_tunnel_client_configuration_failed', file=sys.stderr)
        raise SystemExit(1)
