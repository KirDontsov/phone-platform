#!/usr/bin/env python3
"""Read-only Telphin API spike. Python 3.11+, standard library only."""
from __future__ import annotations

import argparse
import collections
import contextlib
import datetime as dt
import email.utils
import fcntl
import gzip
import hashlib
import http.client
import io
import json
import math
import os
from pathlib import Path
import random
import re
import socket
import sqlite3
import ssl
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import zlib
from dataclasses import dataclass, field

UTC = dt.timezone.utc
ROOT = Path(__file__).resolve().parent.parent
ALLOWED_HOSTS = frozenset({'apiproxy.telphin.ru', 'altapi.telphin.ru'})
MAX_BODY = 32 * 1024 * 1024


class SafeError(Exception):
    """Only fixed codes and safe timing/status metadata may leave the process."""
    def __init__(self, code, *, status=None, next_attempt_at=None):
        self.code, self.status, self.next_attempt_at = code, status, next_attempt_at
        super().__init__(code)


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False)


def stamp(value):
    return value.astimezone(UTC).isoformat(timespec='seconds').replace('+00:00', 'Z')


def parse_time(value):
    try:
        parsed = dt.datetime.fromisoformat(value.replace('Z', '+00:00'))
        if len(value) == 10:
            parsed = parsed.replace(tzinfo=UTC)
        if parsed.tzinfo is None:
            raise ValueError
        return parsed.astimezone(UTC).replace(microsecond=0)
    except (ValueError, TypeError, AttributeError):
        raise SafeError('invalid_datetime') from None


def positive_id(value):
    if isinstance(value, bool) or not re.fullmatch(r'[1-9][0-9]*', str(value or '')):
        raise SafeError('invalid_client_id')
    return str(value)


def load_env(path):
    """Deliberately no shell expansion, command execution or credential logging."""
    path = Path(path)
    if not path.exists():
        raise SafeError('env_file_missing')
    if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o077:
        raise SafeError('env_file_permissions')
    result = {}
    try:
        lines = path.read_text(encoding='utf-8').splitlines()
    except (OSError, UnicodeError):
        raise SafeError('env_file_unreadable') from None
    for line in lines:
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        key, sep, value = line.partition('=')
        if not sep or not re.fullmatch(r'[A-Z][A-Z0-9_]*', key):
            raise SafeError('invalid_env_file')
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in '\"\'':
            value = value[1:-1]
        result[key] = value
    return result


@dataclass(frozen=True)
class Config:
    organization_id: str
    host: str
    app_id: str = field(repr=False)
    app_secret: str = field(repr=False)
    client_id: str

    @classmethod
    def from_mapping(cls, env):
        try:
            org = str(uuid.UUID(env.get('PHONE_ORGANIZATION_ID', '')))
        except (ValueError, AttributeError):
            raise SafeError('invalid_organization_id') from None
        host = env.get('TELPHIN_HOST', 'apiproxy.telphin.ru')
        if host not in ALLOWED_HOSTS:
            raise SafeError('host_not_allowed')
        app, secret = env.get('TELPHIN_APP_ID', ''), env.get('TELPHIN_APP_SECRET', '')
        if not app or not secret or any(c in app + secret for c in '\r\n\x00'):
            raise SafeError('credentials_missing')
        return cls(org, host, app, secret, positive_id(env.get('TELPHIN_CLIENT_ID')))

    @property
    def binding(self):
        # App identity is private too; use a fingerprint to forbid accidental credential reuse.
        return {'organization_id': self.organization_id, 'host': self.host,
                'client_id': self.client_id,
                'app_fingerprint': hashlib.sha256(self.app_id.encode()).hexdigest()}


@dataclass
class Response:
    status: int
    headers: dict
    body: bytes


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class UrllibTransport:
    def __init__(self, timeout=20):
        self.timeout = timeout
        context = ssl.create_default_context()
        # macOS Python distributions may omit the OS CA file from their default search.
        if not os.environ.get('SSL_CERT_FILE') and Path('/etc/ssl/cert.pem').is_file():
            context.load_verify_locations('/etc/ssl/cert.pem')
        self.opener = urllib.request.build_opener(NoRedirect(), urllib.request.HTTPSHandler(context=context))

    def send(self, method, url, headers, body=None):
        request = urllib.request.Request(url, data=body, headers=headers, method=method)
        try:
            response = self.opener.open(request, timeout=self.timeout)
        except urllib.error.HTTPError as error:
            response = error  # Do not propagate provider error messages or URLs.
        with response:
            payload = response.read(MAX_BODY + 1)
            if len(payload) > MAX_BODY:
                raise SafeError('response_too_large')
            return Response(response.code, dict(response.headers), payload)


def decode_json(response):
    headers = {str(k).lower(): str(v) for k, v in response.headers.items()}
    payload = response.body
    if len(payload) > MAX_BODY:
        raise SafeError('response_too_large')
    try:
        if headers.get('content-encoding', '').lower() == 'gzip':
            with gzip.GzipFile(fileobj=io.BytesIO(payload)) as stream:
                payload = stream.read(MAX_BODY + 1)
        if len(payload) > MAX_BODY:
            raise SafeError('response_too_large')
        def bad_constant(_):
            raise ValueError
        return json.loads(payload.decode('utf-8'), parse_constant=bad_constant)
    except (ValueError, UnicodeError, OSError, EOFError, RecursionError, zlib.error):
        raise SafeError('invalid_json_response') from None


class TelphinClient:
    def __init__(self, config, transport=None, clock=time.time, sleep=time.sleep, attempts=3):
        self.config, self.transport = config, transport or UrllibTransport()
        self.clock, self.sleep, self.attempts = clock, sleep, attempts
        self.token, self.token_expires, self.oauth_scope = None, 0, None
        self.rate_resume_at = 0

    def _exchange(self, method, path, *, query=None, data=None, token=None):
        url = 'https://' + self.config.host + path
        if query:
            url += '?' + urllib.parse.urlencode(query)
        headers = {'Accept': 'application/json', 'Accept-Encoding': 'gzip', 'User-Agent': 'phone-platform-spike/0.1'}
        body = None
        if data is not None:
            headers['Content-Type'] = 'application/x-www-form-urlencoded'
            body = urllib.parse.urlencode(data).encode()
        if token:
            headers['Authorization'] = 'Bearer ' + token
        for attempt in range(self.attempts):
            if self.rate_resume_at > self.clock():
                raise SafeError('rate_limited', status=429, next_attempt_at=self.rate_resume_at)
            try:
                response = self.transport.send(method, url, headers, body)
            except (ssl.SSLError, ssl.CertificateError):
                raise SafeError('tls_error') from None
            except (urllib.error.URLError, TimeoutError, socket.timeout, ConnectionError, OSError, http.client.HTTPException) as error:
                if isinstance(getattr(error, 'reason', None), ssl.SSLError):
                    raise SafeError('tls_error') from None
                if attempt + 1 == self.attempts:
                    raise SafeError('network_error') from None
                self.sleep(min(2 ** attempt, 4) + random.random() * .1)
                continue
            h = {str(k).lower(): str(v) for k, v in response.headers.items()}
            if response.status == 429 or h.get('x-ratelimit-remaining') == '0':
                delays = []
                try:
                    delays.append(max(0.0, float(h['x-ratelimit-reset'])))
                except (KeyError, ValueError):
                    pass
                try:
                    delays.append(max(0.0, float(h['retry-after'])))
                except (KeyError, ValueError):
                    try:
                        delays.append(max(0.0, email.utils.parsedate_to_datetime(h['retry-after']).timestamp() - self.clock()))
                    except (KeyError, TypeError, ValueError, OverflowError):
                        pass
                finite = [v for v in delays if math.isfinite(v) and v > 0]
                self.rate_resume_at = self.clock() + max(finite or [60])
            if response.status == 429:
                raise SafeError('rate_limited', status=429, next_attempt_at=self.rate_resume_at)
            if 300 <= response.status < 400:
                raise SafeError('redirect_blocked', status=response.status)
            if response.status >= 500 and attempt + 1 < self.attempts:
                self.sleep(min(2 ** attempt, 4) + random.random() * .1)
                continue
            return response
        raise SafeError('network_error')

    def authenticate(self):
        r = self._exchange('POST', '/oauth/token', data={'grant_type': 'client_credentials',
                           'client_id': self.config.app_id, 'client_secret': self.config.app_secret})
        if not 200 <= r.status < 300:
            raise SafeError('oauth_rejected', status=r.status)
        data = decode_json(r)
        if not isinstance(data, dict):
            raise SafeError('invalid_oauth_response')
        token, ttl = data.get('access_token'), data.get('expires_in')
        if (not isinstance(token, str) or not token or any(c in token for c in '\r\n\x00')
                or isinstance(ttl, bool) or not isinstance(ttl, (float, int))
                or not math.isfinite(ttl) or ttl <= 0 or str(data.get('token_type', '')).lower() != 'bearer'):
            raise SafeError('invalid_oauth_response')
        self.token, self.token_expires = token, self.clock() + max(ttl * .9, .01)
        # No raw scope strings in output: response is not trusted for diagnostics.
        self.oauth_scope = 'all' if data.get('scope') == 'all' else 'other_or_missing'

    def get(self, path, query=None):
        for grant in range(2):
            if not self.token or self.clock() >= self.token_expires:
                self.authenticate()
            r = self._exchange('GET', '/api/ver1.0/' + path.lstrip('/'), query=query, token=self.token)
            if r.status == 401 and grant == 0:
                self.token = None
                continue
            if r.status == 401:
                raise SafeError('reauthorization_required', status=401)
            if r.status == 403:
                raise SafeError('permission_denied', status=403)
            if not 200 <= r.status < 300:
                raise SafeError('api_request_failed', status=r.status)
            return decode_json(r)
        raise SafeError('reauthorization_required', status=401)

    def probe(self):
        user = self.get('user/')
        if not isinstance(user, dict):
            raise SafeError('invalid_user_response')
        # OAuth scope=all is NOT evidence of access to the entire PBX.
        if user.get('extension_id') is not None or user.get('extension_group_id') is not None:
            raise SafeError('limited_user_scope')
        if positive_id(user.get('client_id')) != self.config.client_id:
            raise SafeError('client_binding_mismatch')
        access = user.get('access')
        if access not in ('full', 'read_only'):
            raise SafeError('unknown_user_access')
        return {'organization_id': self.config.organization_id, 'provider_client_id': self.config.client_id,
                'user_scope': 'client', 'user_access': access, 'oauth_scope': self.oauth_scope,
                'call_history_permission': 'not_yet_tested'}

    def history(self, start, end, page, per_page):
        data = self.get('client/' + self.config.client_id + '/call_history/', {
            'start_datetime': start.strftime('%Y-%m-%d %H:%M:%S'),
            'end_datetime': end.strftime('%Y-%m-%d %H:%M:%S'),
            'page': page, 'per_page': per_page, 'order': 'asc'})
        if (not isinstance(data, dict) or type(data.get('page')) is not int or data['page'] != page
                or type(data.get('per_page')) is not int or data['per_page'] <= 0
                or data.get('order') != 'asc' or not isinstance(data.get('call_history'), list)
                or len(data['call_history']) > data['per_page']):
            raise SafeError('invalid_history_page')
        for call in data['call_history']:
            if not isinstance(call, dict) or not isinstance(call.get('call_uuid'), str) or not call['call_uuid']:
                raise SafeError('missing_call_uuid')
            if str(call.get('client_owner_id')) != self.config.client_id:
                raise SafeError('call_owner_mismatch')
            try:
                occurred = dt.datetime.fromisoformat(call['init_time_gmt'].replace('Z', '+00:00'))
                if occurred.tzinfo is None:
                    occurred = occurred.replace(tzinfo=UTC)
                occurred = occurred.astimezone(UTC)
            except (KeyError, TypeError, AttributeError, ValueError):
                raise SafeError('invalid_history_timestamp') from None
            if not start <= occurred <= end:
                raise SafeError('call_outside_requested_window')
            if not isinstance(call.get('cdr'), list) or any(not isinstance(c, dict) for c in call['cdr']):
                raise SafeError('invalid_cdr')
            for item in [call, *call['cdr']]:
                for key in ('result', 'flow'):
                    if item.get(key) is not None and not isinstance(item[key], str):
                        raise SafeError('invalid_history_field')
                if item.get('record_uuid') is not None and not isinstance(item['record_uuid'], str):
                    raise SafeError('invalid_history_field')
            for leg in call['cdr']:
                if str(leg.get('client_owner_id')) != self.config.client_id:
                    raise SafeError('leg_owner_mismatch')
        return data


def secure_directory(path):
    path = Path(path).absolute()
    # Refuse symlinks on all existing components, not just the final file.
    for part in [*reversed(path.parents), path]:
        if part.is_symlink():
            raise SafeError('symlink_not_allowed')
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if not path.is_dir():
        raise SafeError('invalid_output_directory')
    os.chmod(path, 0o700)
    return path


@contextlib.contextmanager
def atomic_writer(path):
    path = Path(path)
    if path.is_symlink():
        raise SafeError('symlink_not_allowed')
    name = None
    try:
        fd, name = tempfile.mkstemp(prefix='.pending-', dir=path.parent)
        with os.fdopen(fd, 'w', encoding='utf-8') as stream:
            os.fchmod(stream.fileno(), 0o600)
            yield stream
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
        directory_fd = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        if name and os.path.exists(name):
            os.unlink(name)


def atomic_write(path, text):
    with atomic_writer(path) as stream:
        stream.write(text)


class Store:
    """SQLite is the checkpoint authority; manifests/NDJSON are derived snapshots."""
    def __init__(self, directory, binding):
        self.directory = secure_directory(directory)
        self.lock = None
        self.db = None
        try:
            flags = os.O_RDWR | os.O_CREAT | getattr(os, 'O_NOFOLLOW', 0)
            fd = os.open(self.directory / '.lock', flags, 0o600)
            self.lock = os.fdopen(fd, 'a+')
            os.fchmod(fd, 0o600)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise SafeError('output_locked') from None
            dbpath = self.directory / 'checkpoint.sqlite3'
            fd = os.open(dbpath, flags, 0o600)
            os.fchmod(fd, 0o600)
            os.close(fd)
            self.db = sqlite3.connect(dbpath)
            self.db.execute('PRAGMA synchronous=FULL')
            self.db.executescript('''
                CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS windows(id INTEGER PRIMARY KEY, start TEXT, end TEXT,
                    next_page INTEGER DEFAULT 1, effective_per_page INTEGER, done INTEGER DEFAULT 0);
                CREATE TABLE IF NOT EXISTS pages(window_id INTEGER,page INTEGER,hash TEXT,rows INTEGER,
                    PRIMARY KEY(window_id,page));
                CREATE TABLE IF NOT EXISTS calls(uuid TEXT PRIMARY KEY,payload TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS variants(uuid TEXT,hash TEXT,PRIMARY KEY(uuid,hash));
            ''')
            old = self.get_meta('binding')
            if old is not None and old != binding:
                raise SafeError('output_binding_mismatch')
            with self.db:
                self.set_meta('binding', binding)
            self.raw = secure_directory(self.directory / 'raw-pages')
        except Exception:
            self.close()
            raise

    def close(self):
        if self.db is not None:
            self.db.close()
            self.db = None
        if self.lock is not None:
            self.lock.close()
            self.lock = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def get_meta(self, key):
        row = self.db.execute('SELECT value FROM meta WHERE key=?', (key,)).fetchone()
        return json.loads(row[0]) if row else None

    def set_meta(self, key, value):
        self.db.execute('INSERT OR REPLACE INTO meta VALUES(?,?)', (key, canonical(value)))

    def initialize(self, spec, windows):
        old = self.get_meta('spec')
        if old is not None:
            if old != spec:
                raise SafeError('resume_parameters_mismatch')
            return
        with self.db:
            self.set_meta('spec', spec)
            for number, (start, end) in enumerate(windows):
                self.db.execute('INSERT INTO windows(id,start,end) VALUES(?,?,?)', (number, stamp(start), stamp(end)))

    def commit_page(self, window, page, payload):
        rows = payload['call_history']
        digest = hashlib.sha256(canonical(payload['call_history']).encode()).hexdigest()
        if rows and self.db.execute('SELECT 1 FROM pages WHERE window_id=? AND hash=?', (window, digest)).fetchone():
            raise SafeError('repeated_page')
        # Raw source is durable before checkpoint. A crash may leave an orphan file, never a skipped page.
        atomic_write(self.raw / f'{window:06d}-{page:06d}.json', canonical(payload) + '\n')
        with self.db:
            for call in rows:
                uuid_ = call['call_uuid']
                serialized = canonical(call)
                self.db.execute('INSERT OR REPLACE INTO calls VALUES(?,?)', (uuid_, serialized))
                self.db.execute('INSERT OR IGNORE INTO variants VALUES(?,?)',
                                (uuid_, hashlib.sha256(serialized.encode()).hexdigest()))
            self.db.execute('INSERT INTO pages VALUES(?,?,?,?)', (window, page, digest, len(rows)))
            self.db.execute('UPDATE windows SET next_page=?,effective_per_page=?,done=? WHERE id=?',
                            (page + 1, payload['per_page'], int(not rows), window))
            self.set_meta('last_error', None)
            self.set_meta('next_attempt_at', None)

    def snapshot(self):
        spec = self.get_meta('spec')
        scanned, unresolved = [], []
        for _, start, end, page, effective, done in self.db.execute('SELECT * FROM windows ORDER BY id'):
            item = {'start': start, 'end': end, 'next_page': page, 'effective_per_page': effective}
            (scanned if done else unresolved).append(item)
        total = self.db.execute('SELECT COUNT(*) FROM calls').fetchone()[0]
        source_rows = self.db.execute('SELECT COALESCE(SUM(rows),0) FROM pages').fetchone()[0]
        variants = self.db.execute('SELECT COUNT(*) FROM (SELECT uuid FROM variants GROUP BY uuid HAVING COUNT(*)>1)').fetchone()[0]
        result_counts, direction_counts, recording_counts = collections.Counter(), collections.Counter(), collections.Counter()
        legs = 0
        binding = self.get_meta('binding')
        with atomic_writer(self.directory / 'calls.ndjson') as calls_stream, atomic_writer(self.directory / 'legs.ndjson') as legs_stream:
            for uuid_, payload in self.db.execute('SELECT uuid,payload FROM calls ORDER BY uuid'):
                call = json.loads(payload)
                cdr = call.pop('cdr')
                result = call.get('result')
                result_counts[result if isinstance(result, str) and result in {'answered', 'bridged', 'missed', 'busy', 'failed', 'no answer', 'cancel'} else 'other_or_unknown'] += 1
                direction = call.get('flow')
                direction_counts[direction if isinstance(direction, str) and direction in {'in', 'out', 'internal'} else 'other_or_unknown'] += 1
                calls_stream.write(canonical({'organization_id': binding['organization_id'], 'client_id': binding['client_id'], 'call': call}) + '\n')
                for ordinal, leg in enumerate(cdr):
                    legs += 1
                    recording_counts['record_id_present' if leg.get('record_uuid') else 'no_record_id'] += 1
                    legs_stream.write(canonical({'organization_id': binding['organization_id'], 'call_uuid': uuid_,
                        'snapshot_ordinal': ordinal, 'leg': leg}) + '\n')
        completed = not unresolved
        manifest = {'schema_version': 1, 'organization_id': binding['organization_id'],
            'provider_client_id': binding['client_id'], 'requested': spec, 'scanned_windows': scanned,
            'unresolved_windows': unresolved, 'pagination_scan_complete': completed,
            'provider_completeness_verified': False, 'lower_bound': 'unknown' if spec['all_available'] else 'operator_requested',
            'status': ('partial_unknown_lower_bound' if spec['all_available'] else 'completed_requested_range') if completed else 'incomplete',
            'calls': total, 'latest_snapshot_legs': legs, 'source_rows': source_rows,
            'duplicate_source_rows': source_rows - total, 'calls_with_multiple_payloads': variants,
            'call_results': dict(result_counts), 'directions': dict(direction_counts),
            'recording_metadata': dict(recording_counts),
            'leg_identity': 'ordinal_in_latest_snapshot_not_provider_id',
            'last_error': self.get_meta('last_error'), 'next_attempt_at': self.get_meta('next_attempt_at'),
            'written_at': stamp(dt.datetime.now(UTC)),
            'limitations': ['live_contract_and_completeness_not_verified', 'late_updates_require_new_scan',
                            'raw_call_and_cdr_results_are_distinct', 'cdr_cardinality_requires_live_validation']}
        atomic_write(self.directory / 'manifest.json', canonical(manifest) + '\n')
        return manifest


def make_windows(start, end, days=7):
    # Newest first; overlap boundary by a second, never infer retention from empty windows.
    delta = dt.timedelta(days=days)
    if start >= end:
        raise SafeError('invalid_date_range')
    windows = []
    cursor = end
    while cursor > start:
        lower = max(start, cursor - delta)
        windows.append((lower, cursor))
        cursor = lower if lower == start else lower + dt.timedelta(seconds=1)
    return windows


def export(client, directory, *, start=None, end=None, all_available=False,
           window_days=7, max_windows=52, per_page=100, max_pages=10000):
    if not 1 <= per_page <= 1000 or not 1 <= window_days <= 31 or not 1 <= max_windows <= 10000 or not 1 <= max_pages <= 100000:
        raise SafeError('invalid_export_limits')
    if all_available == (start is not None):
        raise SafeError('choose_range_or_all_available')
    # Bind local storage first; a persisted rate-limit deadline must forbid even probe requests.
    with Store(directory, client.config.binding) as store:
        old = store.get_meta('spec')
        if old and end is None:
            end = parse_time(old['end'])
        end = end or dt.datetime.now(UTC).replace(microsecond=0)
        if all_available:
            start = end - dt.timedelta(days=(window_days * max_windows)) + dt.timedelta(seconds=max_windows - 1)
        spec = {'start': stamp(start), 'end': stamp(end), 'all_available': all_available,
                'window_days': window_days, 'max_windows': max_windows, 'per_page': per_page,
                'max_pages': max_pages, 'order': 'asc'}
        store.initialize(spec, make_windows(start, end, window_days))
        pending_time = store.get_meta('next_attempt_at')
        if pending_time and pending_time > client.clock():
            raise SafeError('rate_limited', status=429, next_attempt_at=pending_time)
        try:
            client.probe()  # No history data is accepted before verifying scope/ownership.
            windows = store.db.execute('SELECT id,start,end,next_page,effective_per_page FROM windows WHERE done=0 ORDER BY id').fetchall()
            for window, lower, upper, page, effective in windows:
                while page <= max_pages:
                    payload = client.history(parse_time(lower), parse_time(upper), page, per_page)
                    if effective is not None and payload['per_page'] != effective:
                        raise SafeError('page_size_changed')
                    store.commit_page(window, page, payload)
                    if not payload['call_history']:
                        break
                    effective, page = payload['per_page'], page + 1
                else:
                    raise SafeError('max_pages_reached')
            return store.snapshot()
        except SafeError as error:
            with store.db:
                store.set_meta('last_error', {'code': error.code, 'status': error.status})
                store.set_meta('next_attempt_at', error.next_attempt_at)
            store.snapshot()
            raise


def reconcile(directory, reference, output):
    """Private JSON list [{call_uuid, cdr_count?}]; report contains counts, never UUIDs."""
    if Path(reference).is_symlink():
        raise SafeError('symlink_not_allowed')
    if Path(reference).stat().st_mode & 0o077:
        raise SafeError('reference_file_permissions')
    try:
        rows = json.loads(Path(reference).read_text(encoding='utf-8'))
        if not isinstance(rows, list) or any(not isinstance(r, dict) or not isinstance(r.get('call_uuid'), str) or not r['call_uuid'] for r in rows):
            raise ValueError
        if len({r['call_uuid'] for r in rows}) != len(rows):
            raise ValueError
        for r in rows:
            if 'cdr_count' in r and (type(r['cdr_count']) is not int or r['cdr_count'] < 0):
                raise ValueError
    except (ValueError, UnicodeError):
        raise SafeError('invalid_reference') from None
    # Binding is read only from the checkpoint; no credentials required for offline audit.
    directory = Path(directory)
    if directory.is_symlink() or (directory / 'checkpoint.sqlite3').is_symlink():
        raise SafeError('symlink_not_allowed')
    if not (directory / 'checkpoint.sqlite3').is_file():
        raise SafeError('checkpoint_missing')
    db = sqlite3.connect((directory / 'checkpoint.sqlite3').resolve().as_uri() + '?mode=ro', uri=True)
    try:
        db.execute('BEGIN')  # Calls, spec and coverage must come from one consistent snapshot.
        binding = json.loads(db.execute("SELECT value FROM meta WHERE key='binding'").fetchone()[0])
        actual = {uid: len(json.loads(payload)['cdr']) for uid, payload in db.execute('SELECT uuid,payload FROM calls')}
        spec_row = db.execute("SELECT value FROM meta WHERE key='spec'").fetchone()
        unfinished = db.execute('SELECT COUNT(*) FROM windows WHERE done=0').fetchone()[0]
    finally:
        db.close()
    expected = {r['call_uuid']: r for r in rows}
    missing, extra = set(expected) - set(actual), set(actual) - set(expected)
    mismatched = [uid for uid in set(expected) & set(actual) if 'cdr_count' in expected[uid] and expected[uid]['cdr_count'] != actual[uid]]
    match = not (missing or extra or mismatched or unfinished)
    report = {'schema_version': 1, 'organization_id': binding['organization_id'],
              'provider_client_id': binding['client_id'], 'comparison': 'matched' if match else 'mismatch_or_incomplete',
              'reference_calls': len(expected), 'imported_calls': len(actual), 'missing_calls': len(missing),
              'extra_calls': len(extra), 'cdr_count_mismatches': len(mismatched), 'unresolved_windows': unfinished,
              'requested': json.loads(spec_row[0]), 'written_at': stamp(dt.datetime.now(UTC)),
              'human_verified_same_period_timezone_filters_and_pbx': False,
              'limitation': 'reference_origin_and_coverage_need_human_confirmation'}
    atomic_write(output, canonical(report) + '\n')
    return report


def parser():
    p = argparse.ArgumentParser(description='Телфин: read-only probe, выгрузка истории и сверка. Секреты только .env или environment.')
    p.add_argument('--env-file', type=Path, help='Файл mode 600; без shell expansion. По умолчанию .env, если он есть.')
    sub = p.add_subparsers(dest='command', required=True)
    sub.add_parser('probe', help='Проверить OAuth, принадлежность АТС и уровень пользователя без вывода PII.')
    e = sub.add_parser('export', help='Все исходы/направления, включая без записи; повтор той же команды возобновляет checkpoint.')
    scope = e.add_mutually_exclusive_group(required=True)
    scope.add_argument('--from', dest='start', help='Дата UTC YYYY-MM-DD либо ISO с timezone.')
    scope.add_argument('--all-available', action='store_true', help='Сканировать назад max-windows; нижняя граница остаётся unknown.')
    e.add_argument('--to', dest='end', help='Верхняя граница; по умолчанию now, при resume сохранённая.')
    e.add_argument('--output', type=Path, required=True, help='Каталог внутри data/; никогда не использовать каталог другой компании.')
    e.add_argument('--window-days', type=int, default=7)
    e.add_argument('--max-windows', type=int, default=52, help='Бюджет обратного поиска; пустые окна не останавливают поиск.')
    e.add_argument('--per-page', type=int, default=100)
    e.add_argument('--max-pages', type=int, default=10000)
    a = sub.add_parser('reconcile', help='Сверить приватный reference JSON с выгрузкой; никаких API-запросов.')
    a.add_argument('--output', type=Path, required=True, help='Каталог ранее выполненного экспорта внутри data/.')
    a.add_argument('--reference', type=Path, required=True)
    return p


def main(argv=None):
    args = parser().parse_args(argv)
    os.umask(0o077)
    try:
        if args.command in ('export', 'reconcile'):
            root = (ROOT / 'data').absolute()
            dest = args.output.absolute()
            if not dest.is_relative_to(root) or dest == root:
                raise SafeError('output_must_be_inside_data')
            # Lexical '..' and symlinks must not escape the gitignored root.
            if dest.resolve().is_relative_to(root.resolve()) is False:
                raise SafeError('output_must_be_inside_data')
            secure_directory(dest)
        if args.command == 'reconcile':
            report = reconcile(dest, args.reference, dest / 'completeness-report.json')
            print(canonical(report))
            return 0 if report['comparison'] == 'matched' else 2
        env = {}
        env_path = args.env_file or (ROOT / '.env' if (ROOT / '.env').exists() else None)
        if env_path:
            env.update(load_env(env_path))
        env.update({k: v for k, v in os.environ.items() if k.startswith('TELPHIN_') or k == 'PHONE_ORGANIZATION_ID'})
        client = TelphinClient(Config.from_mapping(env))
        if args.command == 'probe':
            print(canonical(client.probe()))
            return 0
        manifest = export(client, dest, start=parse_time(args.start) if args.start else None,
            end=parse_time(args.end) if args.end else None, all_available=args.all_available,
            window_days=args.window_days, max_windows=args.max_windows, per_page=args.per_page, max_pages=args.max_pages)
        # Avoid provider payloads, UUIDs, numbers, local secret paths in stdout.
        print(canonical({k: manifest[k] for k in ('status', 'calls', 'latest_snapshot_legs', 'pagination_scan_complete', 'provider_completeness_verified', 'lower_bound')}))
        return 2 if manifest['status'] != 'completed_requested_range' else 0
    except SafeError as error:
        print(canonical({'error': error.code, 'http_status': error.status, 'next_attempt_at': error.next_attempt_at}), file=sys.stderr)
        return 75 if error.code == 'rate_limited' else 1
    except KeyboardInterrupt:
        print('{"error":"interrupted_resume_same_command"}', file=sys.stderr)
        return 130
    except (OSError, sqlite3.Error, ValueError, TypeError, OverflowError):
        # Never echo exception strings: provider output, paths or credentials may be present.
        print('{"error":"local_io_or_data_error"}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
