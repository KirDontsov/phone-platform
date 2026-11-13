#!/usr/bin/env python3
"""Connect an existing RouterAI account to this computer's private Phone runner."""
from __future__ import annotations

import argparse
import base64
import copy
import fcntl
import hashlib
import hmac
import importlib.util
import json
import os
from pathlib import Path
import secrets
import ssl
import stat
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import urllib.error
import urllib.parse
import urllib.request
import uuid
import webbrowser

ROOT = Path(__file__).resolve().parents[1]
AUTH_URL = "https://routerai.ru/auth"
EXCHANGE_URL = "https://routerai.ru/api/v1/auth/keys"
ISSUER = "https://routerai.ru"
MAX_RUNTIME = 1024 * 1024


class ConnectError(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def import_tools():
    spec = importlib.util.spec_from_file_location("phone_import_opencode_settings", ROOT / "scripts/import-opencode-settings.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def private_read(path, limit=MAX_RUNTIME):
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    except FileNotFoundError:
        return None
    except OSError:
        raise ConnectError("unsafe_runtime_file") from None
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077 or info.st_size > limit:
            raise ConnectError("unsafe_runtime_file")
        with os.fdopen(fd, "rb", closefd=False) as source:
            raw = source.read(limit + 1)
        if len(raw) > limit:
            raise ConnectError("unsafe_runtime_file")
        try:
            return json.loads(raw)
        except (ValueError, UnicodeError):
            raise ConnectError("invalid_runtime_file") from None
    finally:
        os.close(fd)


def private_write(path, value):
    fd, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as target:
            json.dump(value, target)
            target.write("\n")
            target.flush()
            os.fsync(target.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def checked_directory(directory):
    if directory.is_symlink():
        raise ConnectError("unsafe_runtime_directory")
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    if not directory.is_dir() or directory.is_symlink():
        raise ConnectError("unsafe_runtime_directory")


def merge_routerai_runtime(directory, proposal, installer):
    """Re-read under the same lock as import; installation never sees a stale snapshot."""
    checked_directory(directory)
    target = directory / "local-opencode.json"
    lock_path = directory / "local-opencode.lock"
    try:
        lock_fd = os.open(lock_path, os.O_CREAT | os.O_RDWR | getattr(os, "O_NOFOLLOW", 0), 0o600)
    except OSError:
        raise ConnectError("unsafe_runtime_lock") from None
    try:
        info = os.fstat(lock_fd)
        if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077:
            raise ConnectError("unsafe_runtime_lock")
        fcntl.flock(lock_fd, fcntl.LOCK_EX)
        current = private_read(target)
        org = str(uuid.UUID(proposal["organization_id"]))
        if current is None:
            current = {"version": 1, "organization_id": org, "providers": {}}
        if current.get("version") != 1 or current.get("organization_id") != org or not isinstance(current.get("providers"), dict):
            raise ConnectError("local_runner_bound_to_another_organization")
        routerai = proposal.get("providers", {}).get("routerai")
        if not isinstance(routerai, dict):
            raise ConnectError("routerai_configuration_missing")
        merged = copy.deepcopy(current)
        merged["providers"]["routerai"] = copy.deepcopy(routerai)
        try:
            installer(merged)
            private_write(target, merged)
        except Exception:
            # A failed Docker request can follow the volume's atomic rename. Keep the
            # old host file and attempt to restore that exact volume state, without
            # repeating OAuth exchange or touching another tenant/provider.
            try:
                installer(current)
            except Exception:
                raise ConnectError("runtime_install_uncertain") from None
            raise ConnectError("runtime_install_failed") from None
        return merged
    finally:
        os.close(lock_fd)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def exchange_code(code, verifier, callback_url, opener=None):
    """One HTTP attempt only: replaying the code can revoke the issued RouterAI key."""
    if opener is None:
        context = ssl.create_default_context()
        if not context.get_ca_certs() and Path("/etc/ssl/cert.pem").is_file():
            context.load_verify_locations("/etc/ssl/cert.pem")
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context), NoRedirect())
    body = json.dumps({"code": code, "code_verifier": verifier, "code_challenge_method": "S256", "callback_url": callback_url}).encode()
    request = urllib.request.Request(EXCHANGE_URL, data=body, headers={"Content-Type": "application/json", "Accept": "application/json"}, method="POST")
    try:
        with opener.open(request, timeout=20) as response:
            raw = response.read(131073)
            if response.status != 200 or len(raw) > 131072:
                raise ConnectError("routerai_exchange_rejected")
            value = json.loads(raw)
    except ConnectError:
        raise
    except urllib.error.HTTPError as error:
        error.close()
        raise ConnectError("routerai_exchange_rejected") from None
    except (OSError, ValueError, urllib.error.URLError):
        # The provider may have created the key. Never retry the same code.
        raise ConnectError("routerai_exchange_uncertain") from None
    key = value.get("key") if isinstance(value, dict) else None
    if not isinstance(key, str) or not 8 <= len(key) <= 8192 or any(char.isspace() or ord(char) < 32 for char in key):
        raise ConnectError("routerai_exchange_invalid_response")
    return key


class ConsentFlow:
    def __init__(self, organization, *, timeout=300, exchange=exchange_code, commit=None, clock=time.monotonic):
        self.organization = str(uuid.UUID(organization))
        self.verifier = secrets.token_urlsafe(32)
        self.challenge = base64.urlsafe_b64encode(hashlib.sha256(self.verifier.encode()).digest()).rstrip(b"=").decode()
        self.state = secrets.token_urlsafe(32)
        self.clock = clock
        self.deadline = clock() + timeout
        self.exchange = exchange
        self.commit = commit or (lambda key: None)
        self.callback_url = None
        self.host = None
        self.lock = threading.Lock()
        self.consumed = False
        self.exchange_started = False
        self.done = threading.Event()
        self.result = None

    def authorization_url(self):
        if not self.callback_url:
            raise ConnectError("callback_not_bound")
        params = {"callback_url": self.callback_url, "code_challenge": self.challenge, "code_challenge_method": "S256", "state": self.state, "app_name": "Phone Platform Local / " + self.organization, "key_label": "Phone Platform Local"}
        return AUTH_URL + "?" + urllib.parse.urlencode(params)

    def claim(self, hosts, raw_path):
        if hosts != [self.host] or len(raw_path) > 16384:
            return 400, None
        try:
            parsed = urllib.parse.urlsplit(raw_path)
            if parsed.scheme or parsed.netloc or parsed.fragment or parsed.path != "/callback":
                return 404, None
            entries = urllib.parse.parse_qsl(parsed.query, keep_blank_values=True, strict_parsing=True, max_num_fields=8)
        except ValueError:
            return 400, None
        if any(key not in {"code", "state", "iss", "error"} for key, _ in entries) or len({key for key, _ in entries}) != len(entries):
            return 400, None
        query = dict(entries)
        if not hmac.compare_digest(query.get("state", "").encode("utf-8"), self.state.encode("ascii")):
            return 400, None
        if query.get("iss", ISSUER) != ISSUER:
            return 400, None
        if "error" in query:
            if query.get("error") != "access_denied" or "code" in query:
                return 400, None
            with self.lock:
                if self.consumed:
                    return 409, None
                self.consumed = True
                self.result = {"state": "error", "error": "routerai_consent_denied"}
                self.done.set()
            return 303, None
        code = query.get("code")
        if query.get("iss") != ISSUER or not isinstance(code, str) or not 1 <= len(code) <= 8192 or any(ord(char) < 33 for char in code):
            return 400, None
        with self.lock:
            if self.clock() >= self.deadline:
                return 410, None
            if self.consumed:
                return 409, None
            # Consume before any I/O. All later callbacks are rejected, even when
            # network exchange times out and its outcome cannot be known.
            self.consumed = True
        return 303, code

    def finish(self, code):
        with self.lock:
            if self.exchange_started:
                return
            self.exchange_started = True
        try:
            key = self.exchange(code, self.verifier, self.callback_url)
            self.commit(key)
            self.result = {"state": "connected", "provider": "routerai", "credentials_printed": False}
        except ConnectError as error:
            self.result = {"state": "error", "error": error.code}
        except Exception:
            self.result = {"state": "error", "error": "routerai_connection_failed"}
        finally:
            self.done.set()


def callback_server(flow):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def setup(self):
            super().setup()
            self.connection.settimeout(10)

        def log_message(self, *_):
            pass

        def respond(self, status, body=b"", headers=None):
            self.send_response(status)
            for key, value in {"Content-Length": str(len(body)), "Cache-Control": "no-store", "Referrer-Policy": "no-referrer", "X-Content-Type-Options": "nosniff", "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'", **(headers or {})}.items():
                self.send_header(key, value)
            self.end_headers()
            if body:
                self.wfile.write(body)

        def do_GET(self):
            hosts = self.headers.get_all("Host", [])
            if self.path == "/connected" and hosts == [flow.host]:
                if not flow.done.is_set():
                    content = '<meta http-equiv="refresh" content="2"><p>Подключаем RouterAI…</p>'
                elif flow.result.get("state") == "connected":
                    content = '<p>RouterAI подключён. Ключ сохранён локально. Окно можно закрыть.</p>'
                else:
                    content = '<p>Подключение не завершено. Проверь сообщение в терминале. Ключ в чат присылать не нужно.</p>'
                self.respond(200, ('<!doctype html><meta charset="utf-8">' + content).encode(), {"Content-Type": "text/html; charset=utf-8"})
                return
            status, code = flow.claim(hosts, self.path)
            self.respond(status, headers={"Location": "/connected"} if status == 303 else None)
            if code is not None:
                threading.Thread(target=flow.finish, args=(code,), daemon=True).start()

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    server.timeout = 0.25
    flow.host = "127.0.0.1:" + str(server.server_port)
    flow.callback_url = "http://" + flow.host + "/callback"
    return server


def restart_runner(project):
    if not project.startswith("phone-") or any(char not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-" for char in project):
        raise ConnectError("invalid_phone_project")
    container = project + "-model-runner-1"
    try:
        subprocess.run(["docker", "restart", container], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            status = subprocess.check_output(["docker", "inspect", "--format", "{{.State.Health.Status}}", container], text=True, stderr=subprocess.DEVNULL, timeout=5).strip()
            if status == "healthy":
                return
            time.sleep(1)
    except (OSError, subprocess.SubprocessError):
        pass
    raise ConnectError("routerai_saved_runner_restart_required")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--organization", required=True)
    parser.add_argument("--source", type=Path, default=Path.home() / ".config/opencode/opencode.json")
    parser.add_argument("--project", default="phone-analyzer")
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--timeout", type=int, default=300)
    args = parser.parse_args(argv)
    if not 30 <= args.timeout <= 600:
        raise ConnectError("invalid_consent_timeout")
    org = str(uuid.UUID(args.organization))
    tools = import_tools()
    directory = ROOT / "secrets"
    checked_directory(directory)
    # Fail before requesting consent when this runner is already bound elsewhere.
    current = private_read(directory / "local-opencode.json")
    if current and current.get("organization_id") != org:
        raise ConnectError("local_runner_bound_to_another_organization")

    def commit(key):
        proposal = tools.build_settings(args.source, org, provider_ids={"routerai"}, credentials={"routerai": key})
        merge_routerai_runtime(directory, proposal, lambda value: tools.install(value, args.project))
        restart_runner(args.project)

    flow = ConsentFlow(org, timeout=args.timeout, commit=commit)
    server = callback_server(flow)
    authorization_url = flow.authorization_url()
    print(json.dumps({"state": "awaiting_consent", "provider": "routerai", "authorization_url": authorization_url}), flush=True)
    try:
        if not args.no_browser:
            webbrowser.open(authorization_url)
        while not flow.done.is_set() and (flow.clock() < flow.deadline or flow.consumed):
            server.handle_request()
        if not flow.done.is_set():
            flow.result = {"state": "error", "error": "routerai_consent_expired"}
        print(json.dumps(flow.result), flush=True)
        # Briefly serve the clean callback URL so the browser can show completion.
        grace = time.monotonic() + 3
        while time.monotonic() < grace:
            server.handle_request()
        return 0 if flow.result.get("state") == "connected" else 1
    finally:
        server.server_close()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print(json.dumps({"state": "error", "error": "routerai_consent_cancelled"}), flush=True)
        raise SystemExit(1)
    except ConnectError as error:
        print(json.dumps({"state": "error", "error": error.code}), flush=True)
        raise SystemExit(1)
    except Exception:
        print(json.dumps({"state": "error", "error": "routerai_connection_failed"}), flush=True)
        raise SystemExit(1)
