#!/usr/bin/env python3
"""Read-only HTTPS smoke; credentials stay in a private file and session cookies in memory.

Only login, organization switch and logout mutate this smoke's own sessions.
No provider requests, settings changes, call processing or exports are triggered.
"""
import argparse
import http.cookiejar
import json
from pathlib import Path
import ssl
import sys
import urllib.error
import urllib.parse
import urllib.request


class Client:
    def __init__(self, base):
        self.base = base
        self.cookies = http.cookiejar.CookieJar()
        context = ssl.create_default_context()
        # python.org macOS installations can have no bundled CA store configured.
        # Use the OS trust bundle; certificate/hostname verification stays enabled.
        if sys.platform == 'darwin' and not context.get_ca_certs() and Path('/etc/ssl/cert.pem').is_file():
            context.load_verify_locations('/etc/ssl/cert.pem')
        self.opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}),
            urllib.request.HTTPCookieProcessor(self.cookies),
            urllib.request.HTTPSHandler(context=context),
        )
        self.me = None

    def request(self, path, body=None, headers=None):
        h = {'Origin': self.base, **(headers or {})}
        if body is not None:
            h['Content-Type'] = 'application/json'
            if self.me:
                h['X-CSRF-Token'] = self.me['csrf_token']
        req = urllib.request.Request(self.base + path, data=None if body is None else json.dumps(body).encode(), headers=h)
        try:
            response = self.opener.open(req, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read(4 * 1024 * 1024 + 1)
            if len(raw) > 4 * 1024 * 1024:
                raise RuntimeError('smoke_response_too_large')
            value = json.loads(raw) if 'application/json' in response.headers.get('Content-Type', '') else None
            return response.status, response.headers, value, raw

    def data(self, path, body=None, headers=None):
        code, h, value, raw = self.request(path, body, headers)
        if code != 200 or not isinstance(value, dict) or 'data' not in value:
            raise RuntimeError('smoke_expected_success')
        return value['data']

    def login(self, account):
        code, h, value, _ = self.request('/api/auth/login', {k: account[k] for k in ('email', 'password')})
        if code != 200:
            raise RuntimeError('smoke_login_failed')
        cookie = h.get('Set-Cookie', '')
        if not all(v in cookie for v in ('Secure', 'HttpOnly', 'SameSite=Lax')):
            raise RuntimeError('smoke_cookie_not_secure')
        self.me = value['data']

    def logout(self):
        if self.me:
            self.data('/api/auth/logout', {})
            self.me = None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--access-file', type=Path, default=Path('secrets/production-access.json'))
    args = parser.parse_args()
    path = args.access_file
    if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o077:
        raise RuntimeError('smoke_private_access_file_required')
    access = json.loads(path.read_text())
    base = access['url']
    if base != 'https://phone.bondscan.ru':
        raise RuntimeError('smoke_unexpected_target')
    accounts = [{'email': email, 'password': password} for email, password in access['accounts'].items()]
    report = {'target': base, 'provider_requests': 0, 'configuration_writes': 0}
    client = Client(base)
    if client.request('/api/calls')[0] != 401:
        raise RuntimeError('smoke_anonymous_calls_not_denied')
    report['anonymous_calls_denied'] = True
    for account in accounts:
        if client.request('/api/auth/login', {'email': account['email'], 'password': 'PhoneDemo!2026'})[0] != 401:
            raise RuntimeError('smoke_old_demo_password_still_works')
        isolated = Client(base)
        try:
            isolated.login(account)
        finally:
            isolated.logout()
    report['old_demo_passwords_denied'] = len(accounts)
    report['new_passwords_verified'] = len(accounts)
    owner = next(a for a in accounts if a['email'] == 'owner@demo.local')
    try:
        client.login(owner)
        if client.me.get('demo_mode') is not False:
            raise RuntimeError('smoke_production_demo_mode_enabled')
        report['secure_cookie'] = True
        report['organizations'] = len(client.me['organizations'])
        report['production_mode'] = True
        org = client.me['organization']['id']
        alternative = next(o['id'] for o in client.me['organizations'] if o['id'] != org)
        if client.request('/api/calls', headers={'X-Organization-Id': alternative})[0] != 409:
            raise RuntimeError('smoke_organization_header_not_scoped')
        report['organization_header_guard'] = True
        client.data('/api/dashboard')
        connections = client.data('/api/connections')['items']
        if any('config_encrypted' in d for d in connections):
            raise RuntimeError('smoke_ciphertext_exposed')
        report['connection_count'] = len(connections)
        report['connections_ciphertext_hidden'] = True
        ready_filter = urllib.parse.urlencode({'filter': json.dumps({'field': 'recording_state', 'op': 'eq', 'value': 'ready'}), 'limit': 100})
        calls = client.data('/api/calls?' + ready_filter)
        report['ready_recording_calls'] = calls['total']
        selected = None
        for call in calls['items']:
            detail = client.data('/api/calls/' + call['id'])
            recording = next((r for r in detail['recordings'] if r.get('state') == 'ready' or r.get('ready') is True), None)
            if recording:
                media_path = '/api/calls/' + call['id'] + '/media/' + recording['id']
                code, headers, _, raw = client.request(media_path, headers={'Range': 'bytes=0-63'})
                if code != 206 or not headers.get('Content-Range', '').startswith('bytes 0-63/') or len(raw) != 64:
                    raise RuntimeError('smoke_media_range_failed')
                selected = call['id']
                break
        if selected is None:
            raise RuntimeError('smoke_no_playable_recording')
        report['media_range_verified'] = True
        status = client.data('/api/assistant/status')
        report['assistant_runner_ready'] = status['runner_ready']
        report['assistant_authenticated'] = status['authenticated']
        if not status['runner_ready'] or not status['authenticated']:
            raise RuntimeError('smoke_assistant_login_not_preserved')
        client.me = client.data('/api/auth/organization', {'organization_id': alternative})
        if client.request('/api/calls/' + selected)[0] != 404:
            raise RuntimeError('smoke_cross_organization_call_exposed')
        client.data('/api/calls?limit=1')
        report['cross_organization_call_denied'] = True
    finally:
        client.logout()
    print(json.dumps(report, ensure_ascii=False))


if __name__ == '__main__':
    try:
        main()
    except RuntimeError as error:
        print(json.dumps({'error': str(error)}), file=sys.stderr)
        sys.exit(1)
    except Exception:
        print('{"error":"production_smoke_failed"}', file=sys.stderr)
        sys.exit(1)
