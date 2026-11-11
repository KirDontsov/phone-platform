"""Offline contract/failure tests. All identities and payloads are synthetic.

Documentation-derived response shapes; these fixtures are NOT live API evidence.
No network, account credentials, customer numbers or recordings are used.
"""
from __future__ import annotations

import contextlib
import datetime as dt
import gzip
import http.client
import importlib.util
import io
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
from unittest import mock
import urllib.error
import urllib.parse

MODULE_PATH = Path(__file__).resolve().parents[1] / 'telphin-spike.py'
SPEC = importlib.util.spec_from_file_location('telphin_spike_under_test', MODULE_PATH)
spike = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = spike
SPEC.loader.exec_module(spike)

ORG_A = '00000000-0000-0000-0000-000000000001'
ORG_B = '00000000-0000-0000-0000-000000000002'
START = dt.datetime(2026, 1, 31, tzinfo=dt.timezone.utc)
END = START + dt.timedelta(days=1)
SENSITIVE = 'synthetic-secret-NEVER-PRINT'


def configuration(**overrides):
    values = {'PHONE_ORGANIZATION_ID': ORG_A, 'TELPHIN_HOST': 'apiproxy.telphin.ru',
              'TELPHIN_APP_ID': 'synthetic-app', 'TELPHIN_APP_SECRET': SENSITIVE,
              'TELPHIN_CLIENT_ID': '42'}
    values.update(overrides)
    return spike.Config.from_mapping(values)


def response(payload=None, status=200, headers=None, compressed=False):
    body = json.dumps(payload if payload is not None else {}).encode()
    h = dict(headers or {})
    if compressed:
        h['Content-Encoding'] = 'gzip'
        body = gzip.compress(body)
    return spike.Response(status, h, body)


def token(value='synthetic-token', ttl=3600):
    return response({'access_token': value, 'expires_in': ttl,
                     'token_type': 'Bearer', 'scope': 'all'})


def user(**updates):
    data = {'client_id': 42, 'extension_id': None, 'extension_group_id': None,
            'access': 'read_only', 'admin': False, 'login': SENSITIVE}
    data.update(updates)
    return response(data)


def call(number=1, *, result='answered', flow='in', cdr=None, **updates):
    # The same instant is intentional: pagination must not rely on unique timestamps.
    value = {'call_uuid': f'{number:032x}', 'client_owner_id': 42,
             'init_time_gmt': '2026-01-31 23:59:59.31', 'result': result, 'flow': flow,
             'from_username': 'synthetic-caller',
             'cdr': cdr if cdr is not None else [leg()]}
    value.update(updates)
    return value


def leg(*, result='answered', recording=None, **updates):
    value = {'client_owner_id': 42, 'extension_id': 7, 'result': result,
             'record_uuid': recording, 'storage_url': None}
    value.update(updates)
    return value


def page(number, rows, size=2, order='asc'):
    return {'page': number, 'per_page': size, 'order': order, 'call_history': rows}


class ScriptedTransport:
    def __init__(self, replies):
        self.replies = list(replies)
        self.requests = []

    def send(self, method, url, headers, body=None):
        self.requests.append((method, url, dict(headers), body))
        if not self.replies:
            raise AssertionError('Unexpected network request in offline test')
        item = self.replies.pop(0)
        if isinstance(item, BaseException):
            raise item
        return item


class FakeClient:
    """Durable-export boundary: the history endpoint is injected, not the store."""
    def __init__(self, handler, config=None, clock=None):
        self.config = config or configuration()
        self.handler = handler
        self.requests = []
        self.probes = 0
        self.clock = clock or (lambda: 1000.0)

    def probe(self):
        self.probes += 1
        return {'user_scope': 'client'}

    def history(self, lower, upper, number, size):
        self.requests.append((lower, upper, number, size))
        return self.handler(lower, upper, number, size)


class TestConfigurationAndHTTP(unittest.TestCase):
    def make_client(self, replies):
        self.now = 1000.0
        self.sleeps = []
        transport = ScriptedTransport(replies)
        client = spike.TelphinClient(configuration(), transport=transport,
                                    clock=lambda: self.now, sleep=self.sleeps.append)
        return client, transport

    def assert_code(self, expected, operation):
        with self.assertRaises(spike.SafeError) as caught:
            operation()
        self.assertEqual(expected, caught.exception.code)
        return caught.exception

    def test_oauth_form_gzip_and_probe_are_read_only_without_pii(self):
        profile = {'client_id': 42, 'extension_id': None, 'extension_group_id': None,
                   'access': 'read_only', 'login': SENSITIVE}
        client, transport = self.make_client([token(), response(profile, compressed=True)])
        result = client.probe()
        grant, probe = transport.requests
        self.assertEqual('POST', grant[0])
        self.assertEqual('/oauth/token', urllib.parse.urlsplit(grant[1]).path)
        fields = urllib.parse.parse_qs(grant[3].decode())
        self.assertEqual(['client_credentials'], fields['grant_type'])
        self.assertEqual(['synthetic-app'], fields['client_id'])
        self.assertEqual([SENSITIVE], fields['client_secret'])
        self.assertEqual('GET', probe[0])
        self.assertTrue(probe[1].endswith('/api/ver1.0/user/'))
        self.assertEqual('gzip', probe[2]['Accept-Encoding'])
        self.assertEqual('Bearer synthetic-token', probe[2]['Authorization'])
        self.assertEqual('client', result['user_scope'])
        self.assertEqual('read_only', result['user_access'])
        self.assertNotIn(SENSITIVE, json.dumps(result))
        self.assertNotIn(SENSITIVE, repr(client.config))

    def test_token_expiry_regrants_before_next_get(self):
        client, transport = self.make_client([token('token-one', ttl=10), user(), token('token-two'), user()])
        client.probe()
        self.now += 11
        client.probe()
        self.assertEqual(['POST', 'GET', 'POST', 'GET'], [r[0] for r in transport.requests])
        self.assertEqual('Bearer token-two', transport.requests[-1][2]['Authorization'])

    def test_401_regrants_once_then_stops(self):
        client, transport = self.make_client([token(), response(status=401), token(), response(status=401)])
        self.assert_code('reauthorization_required', client.probe)
        self.assertEqual(4, len(transport.requests))
        self.assertEqual([], self.sleeps)

    def test_401_can_recover(self):
        client, transport = self.make_client([token(), response(status=401), token('new'), user()])
        self.assertEqual('client', client.probe()['user_scope'])
        self.assertEqual(4, len(transport.requests))

    def test_403_never_retries_or_regrants(self):
        client, transport = self.make_client([token(), response({'private': SENSITIVE}, status=403)])
        self.assert_code('permission_denied', client.probe)
        self.assertEqual(2, len(transport.requests))

    def test_scope_all_does_not_accept_extension_or_group(self):
        for update in ({'extension_id': 7}, {'extension_group_id': 9}):
            with self.subTest(update=update):
                client, _ = self.make_client([token(), user(**update)])
                self.assert_code('limited_user_scope', client.probe)

    def test_mismatched_client_is_rejected(self):
        client, _ = self.make_client([token(), user(client_id=43)])
        self.assert_code('client_binding_mismatch', client.probe)

    def test_429_records_reset_without_hour_long_sleep(self):
        client, transport = self.make_client([token(), response(status=429, headers={
            'X-RateLimit-Reset': '7200', 'Retry-After': '30'})])
        error = self.assert_code('rate_limited', client.probe)
        self.assertEqual(8200.0, error.next_attempt_at)
        self.assertEqual([], self.sleeps)
        self.assert_code('rate_limited', client.probe)
        self.assertEqual(2, len(transport.requests))

    def test_exhausted_successful_response_defers_next_request(self):
        client, transport = self.make_client([token(), response({
            'client_id': 42, 'extension_id': None, 'extension_group_id': None,
            'access': 'full'}, headers={'X-RateLimit-Remaining': '0', 'X-RateLimit-Reset': '3600'})])
        self.assertEqual('client', client.probe()['user_scope'])
        self.assert_code('rate_limited', client.probe)
        self.assertEqual(2, len(transport.requests))

    def test_network_and_5xx_are_bounded_and_recover(self):
        client, transport = self.make_client([token(), urllib.error.URLError(SENSITIVE),
                                             response(status=503), user()])
        self.assertEqual('client', client.probe()['user_scope'])
        self.assertEqual(4, len(transport.requests))
        self.assertEqual(2, len(self.sleeps))
        self.assertTrue(all(delay < 5 for delay in self.sleeps))

    def test_network_error_never_echoes_provider_exception(self):
        client, transport = self.make_client([token()] + [urllib.error.URLError(SENSITIVE)] * 3)
        error = self.assert_code('network_error', client.probe)
        self.assertNotIn(SENSITIVE, str(error))
        self.assertEqual(4, len(transport.requests))

    def test_redirect_rejected_before_following_credentials(self):
        client, transport = self.make_client([token(), response(status=302, headers={'Location': 'https://invalid.example'})])
        self.assert_code('redirect_blocked', client.probe)
        self.assertEqual(2, len(transport.requests))

    def test_history_validates_ownership_and_pagination_echo(self):
        cases = [({'page': 2}, 'invalid_history_page'),
                 ({'per_page': 0}, 'invalid_history_page'),
                 ({'order': 'desc'}, 'invalid_history_page'),
                 ({'call_history': [call(client_owner_id=43)]}, 'call_owner_mismatch'),
                 ({'call_history': [call(cdr=[leg(client_owner_id=43)])]}, 'leg_owner_mismatch'),
                 ({'call_history': [call(call_uuid='')]}, 'missing_call_uuid')]
        for updates, code in cases:
            with self.subTest(code=code, updates=updates):
                payload = page(1, [call()])
                payload.update(updates)
                client, _ = self.make_client([token(), response(payload)])
                self.assert_code(code, lambda: client.history(START, END, 1, 100))

    def test_history_request_has_no_result_recording_or_direction_filter(self):
        client, transport = self.make_client([token(), response(page(1, [], size=3))])
        result = client.history(START, END, 1, 100)
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(transport.requests[-1][1]).query)
        self.assertEqual({'start_datetime', 'end_datetime', 'page', 'per_page', 'order'}, set(query))
        self.assertEqual(['2026-01-31 00:00:00'], query['start_datetime'])
        self.assertEqual(3, result['per_page'])

    def test_history_timestamp_guards_accept_utc_boundaries_and_fractional_seconds(self):
        for value in ('2026-01-31 00:00:00', '2026-01-31 23:59:59.31', '2026-02-01T00:00:00Z'):
            with self.subTest(timestamp=value):
                client, _ = self.make_client([token(), response(page(1, [call(init_time_gmt=value)]))])
                payload = client.history(START, END, 1, 100)
                self.assertEqual(value, payload['call_history'][0]['init_time_gmt'])

    def test_history_timestamp_missing_invalid_or_outside_window_is_rejected(self):
        for value, expected in ((None, 'invalid_history_timestamp'),
                                ('not-a-time', 'invalid_history_timestamp'),
                                ('2026-01-30 23:59:59', 'call_outside_requested_window'),
                                ('2026-02-01 00:00:00.01', 'call_outside_requested_window')):
            with self.subTest(timestamp=value):
                client, _ = self.make_client([token(), response(page(1, [call(init_time_gmt=value)]))])
                self.assert_code(expected, lambda: client.history(START, END, 1, 100))

    def test_outside_window_page_is_not_committed_by_real_client_export_path(self):
        client, _ = self.make_client([token(), user(), response(page(1, [call(init_time_gmt='2026-03-01 00:00:00')]))])
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve() / 'export'
            self.assert_code('call_outside_requested_window', lambda: spike.export(client, output, start=START, end=END))
            with sqlite3.connect(output / 'checkpoint.sqlite3') as db:
                self.assertEqual(0, db.execute('SELECT COUNT(*) FROM calls').fetchone()[0])
                self.assertEqual(1, db.execute('SELECT next_page FROM windows').fetchone()[0])
            report = json.loads((output / 'manifest.json').read_text())
            self.assertFalse(report['pagination_scan_complete'])
            self.assertEqual('call_outside_requested_window', report['last_error']['code'])

    def test_documentation_derived_fixture_shapes_are_exercised(self):
        fixture_root = Path(__file__).parent / 'fixtures' / 'telphin'
        history_fixture = json.loads((fixture_root / 'history.json').read_text())
        error_fixture = json.loads((fixture_root / 'errors.json').read_text())
        self.assertEqual('documentation-derived/live-unverified', history_fixture['provenance'])
        client, _ = self.make_client([token(), response(history_fixture['response'])])
        actual = client.history(START, END, 1, 100)
        self.assertEqual(2, len(actual['call_history']))
        for case in error_fixture['cases']:
            with self.subTest(status=case['status']):
                client, _ = self.make_client([token(), response(case['body'], status=case['status'], headers=case['headers'])])
                self.assert_code(case['expected_code'], client.probe)

    def test_corrupt_deflate_is_a_safe_protocol_error(self):
        body = bytearray(gzip.compress(b'{"synthetic":"payload"}'))
        body[10] = 255  # gzip envelope, invalid DEFLATE block type.
        self.assert_code('invalid_json_response', lambda: spike.decode_json(
            spike.Response(200, {'Content-Encoding': 'gzip'}, bytes(body))))

    def test_incomplete_http_body_and_protocol_errors_retry_within_budget(self):
        client, transport = self.make_client([token(), http.client.IncompleteRead(b'synthetic-partial', 80),
                                             http.client.HTTPException('synthetic-protocol-error'), user()])
        self.assertEqual('client', client.probe()['user_scope'])
        self.assertEqual(4, len(transport.requests))
        self.assertEqual(2, len(self.sleeps))
        self.assertTrue(all(delay < 5 for delay in self.sleeps))

    def test_nonfinite_json_is_rejected(self):
        self.assert_code('invalid_json_response', lambda: spike.decode_json(spike.Response(200, {}, b'{"x":NaN}')))

    def test_host_and_credential_validation_do_not_echo_values(self):
        for updates, code in [({'TELPHIN_HOST': 'https://evil.example'}, 'host_not_allowed'),
                              ({'TELPHIN_APP_SECRET': ''}, 'credentials_missing'),
                              ({'TELPHIN_CLIENT_ID': '0'}, 'invalid_client_id')]:
            self.assert_code(code, lambda: configuration(**updates))


class TestMalformedHistory(unittest.TestCase):
    def test_bad_optional_shapes_are_rejected_before_checkpoint(self):
        for key, value in [('result', []), ('flow', {}), ('record_uuid', {})]:
            with self.subTest(key=key), tempfile.TemporaryDirectory() as directory:
                directory = Path(directory).resolve()
                malformed = call(**{key: value})
                transport = ScriptedTransport([token(), user(), response(page(1, [malformed]))])
                client = spike.TelphinClient(configuration(), transport=transport, sleep=lambda _: None)
                with self.assertRaises(spike.SafeError) as error:
                    spike.export(client, directory, start=START, end=END)
                self.assertEqual('invalid_history_field', error.exception.code)
                db = sqlite3.connect(Path(directory) / 'checkpoint.sqlite3')
                self.assertEqual(0, db.execute('SELECT COUNT(*) FROM calls').fetchone()[0])
                self.assertEqual(1, db.execute('SELECT next_page FROM windows LIMIT 1').fetchone()[0])
                db.close()


class TestExportAndDurability(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        # macOS /var is a symlink; tests deliberately use the resolved private path.
        self.directory = Path(self.tmp.name).resolve() / 'export'

    def export(self, client, **updates):
        args = {'start': START, 'end': END, 'per_page': 100}
        args.update(updates)
        return spike.export(client, self.directory, **args)

    def read_manifest(self):
        return json.loads((self.directory / 'manifest.json').read_text())

    def db_value(self, sql):
        with sqlite3.connect(self.directory / 'checkpoint.sqlite3') as db:
            return db.execute(sql).fetchone()[0]

    def assert_code(self, expected, operation):
        with self.assertRaises(spike.SafeError) as caught:
            operation()
        self.assertEqual(expected, caught.exception.code)
        return caught.exception

    def test_three_pages_same_timestamp_server_clamp_and_no_audio_are_preserved(self):
        calls = {1: [call(1, result='bridged', cdr=[leg(result='answered elsewhere'), leg()])],
                 2: [call(2, result='not answered', cdr=[])],
                 3: [call(3, flow='internal', cdr=[leg()])], 4: []}
        client = FakeClient(lambda a, b, n, s: page(n, calls[n], size=1))
        report = self.export(client)
        self.assertEqual([1, 2, 3, 4], [r[2] for r in client.requests])
        self.assertEqual(3, report['calls'])
        self.assertEqual(3, report['latest_snapshot_legs'])
        self.assertEqual({'bridged': 1, 'not answered': 1, 'answered': 1}, report['call_results'])
        self.assertEqual(1, report['directions']['internal'])
        self.assertTrue(report['pagination_scan_complete'])
        self.assertFalse(report['provider_completeness_verified'])
        raw_legs = [json.loads(x) for x in (self.directory / 'legs.ndjson').read_text().splitlines()]
        self.assertIn('answered elsewhere', [x['leg']['result'] for x in raw_legs])
        self.assertNotIn('answered elsewhere', report['call_results'])
        self.assertEqual(3, len(list((self.directory / 'calls.ndjson').read_text().splitlines())))

    def test_every_official_whole_call_result_has_its_own_counter(self):
        official = ['busy', 'answered', 'bridged', 'not answered', 'rejected', 'failed']
        values = [call(index, result=result) for index, result in enumerate(official, 1)]
        client = FakeClient(lambda a, b, n, size: page(n, values if n == 1 else [], size=100))
        report = self.export(client)
        self.assertEqual({value: 1 for value in official}, report['call_results'])
        self.assertEqual(6, report['calls'])
        self.assertNotIn('other_or_unknown', report['call_results'])

    def test_unrecognized_result_is_preserved_raw_and_counted_as_unknown(self):
        report = self.export(FakeClient(lambda a, b, n, size: page(n, [call(result='future-provider-value')] if n == 1 else [])))
        self.assertEqual({'other_or_unknown': 1}, report['call_results'])
        raw = json.loads((self.directory / 'calls.ndjson').read_text().splitlines()[0])
        self.assertEqual('future-provider-value', raw['call']['result'])

    def test_rerun_uses_committed_checkpoint_without_refetching_completed_pages(self):
        first = FakeClient(lambda a, b, n, s: page(n, [call()] if n == 1 else []))
        report = self.export(first)
        second = FakeClient(lambda *args: self.fail('Completed checkpoint must not fetch again'))
        repeated = self.export(second)
        self.assertEqual(report['calls'], repeated['calls'])
        self.assertEqual([], second.requests)

    def test_page_failure_leaves_only_durable_page_and_resume_fetches_next(self):
        def failing(a, b, n, s):
            if n == 1:
                return page(n, [call()])
            raise spike.SafeError('api_request_failed', status=503)
        self.assert_code('api_request_failed', lambda: self.export(FakeClient(failing)))
        manifest = self.read_manifest()
        self.assertEqual('incomplete', manifest['status'])
        self.assertEqual(1, manifest['calls'])
        self.assertEqual(2, manifest['unresolved_windows'][0]['next_page'])
        resumed = FakeClient(lambda a, b, n, s: page(n, []))
        self.assertEqual('completed_requested_range', self.export(resumed)['status'])
        self.assertEqual([2], [r[2] for r in resumed.requests])

    def test_rate_limit_is_persisted_and_resume_does_not_refetch_before_reset(self):
        def limited(a, b, n, s):
            raise spike.SafeError('rate_limited', status=429, next_attempt_at=4600)
        self.assert_code('rate_limited', lambda: self.export(FakeClient(limited)))
        self.assertEqual(4600, self.read_manifest()['next_attempt_at'])
        resumed = FakeClient(lambda *args: self.fail('Must defer until reset'))
        self.assert_code('rate_limited', lambda: self.export(resumed))
        self.assertEqual([], resumed.requests)
        self.assertEqual(0, resumed.probes, 'Persisted rate limit must be checked before OAuth/probe')
        later = FakeClient(lambda a, b, n, s: page(n, []), clock=lambda: 4601)
        self.assertEqual('completed_requested_range', self.export(later)['status'])

    def test_crash_before_page_commit_resumes_same_page(self):
        original = spike.atomic_write
        def broken(path, text):
            if Path(path).parent.name == 'raw-pages':
                raise OSError('synthetic-disk-full')
            return original(path, text)
        first = FakeClient(lambda a, b, n, s: page(n, [call()] if n == 1 else []))
        with mock.patch.object(spike, 'atomic_write', side_effect=broken):
            with self.assertRaises(OSError):
                self.export(first)
        self.assertEqual(0, self.db_value('SELECT COUNT(*) FROM calls'))
        self.assertEqual(1, self.db_value('SELECT next_page FROM windows'))
        resumed = FakeClient(lambda a, b, n, s: page(n, [call()] if n == 1 else []))
        self.assertEqual(1, self.export(resumed)['calls'])
        self.assertEqual([1, 2], [r[2] for r in resumed.requests])

    def test_crash_after_page_commit_before_manifest_does_not_duplicate_or_skip(self):
        first = FakeClient(lambda a, b, n, s: page(n, [call()] if n == 1 else []))
        with mock.patch.object(spike.Store, 'snapshot', side_effect=OSError('synthetic-crash')):
            with self.assertRaises(OSError):
                self.export(first)
        self.assertEqual(1, self.db_value('SELECT COUNT(*) FROM calls'))
        self.assertEqual(1, self.db_value('SELECT done FROM windows'))
        resumed = FakeClient(lambda *args: self.fail('All committed pages must survive manifest crash'))
        report = self.export(resumed)
        self.assertEqual(1, report['calls'])
        self.assertTrue(report['pagination_scan_complete'])

    def test_same_uuid_in_overlapping_windows_is_deduplicated_and_raw_sources_retained(self):
        client = FakeClient(lambda a, b, n, s: page(n, [call()] if n == 1 else []))
        report = self.export(client, end=START + dt.timedelta(days=2), window_days=1)
        self.assertEqual(1, report['calls'])
        self.assertGreater(report['source_rows'], 1)
        self.assertEqual(report['source_rows'] - 1, report['duplicate_source_rows'])
        self.assertGreaterEqual(len(list((self.directory / 'raw-pages').glob('*.json'))), 4)

    def test_updated_call_snapshot_keeps_variants_and_latest_recording(self):
        count = [0]
        def history(a, b, n, size):
            if n > 1:
                return page(n, [])
            count[0] += 1
            return page(n, [call(cdr=[leg(recording=None if count[0] == 1 else 'synthetic-recording')])])
        report = self.export(FakeClient(history), end=START + dt.timedelta(days=2), window_days=1)
        self.assertEqual(1, report['calls_with_multiple_payloads'])
        self.assertEqual(1, report['recording_metadata']['record_id_present'])

    def test_binding_mismatch_rejects_org_client_and_application_reuse(self):
        self.export(FakeClient(lambda a, b, n, s: page(n, [])))
        for overrides in ({'PHONE_ORGANIZATION_ID': ORG_B}, {'TELPHIN_CLIENT_ID': '43'},
                          {'TELPHIN_APP_ID': 'different-synthetic-app'}):
            with self.subTest(overrides=overrides):
                client = FakeClient(lambda *args: self.fail('No history request for wrong binding'),
                                    config=configuration(**overrides))
                self.assert_code('output_binding_mismatch', lambda: self.export(client))
        self.assertEqual(0, self.db_value('SELECT COUNT(*) FROM calls'))

    def test_resume_parameter_change_is_rejected(self):
        self.export(FakeClient(lambda a, b, n, s: page(n, [])))
        self.assert_code('resume_parameters_mismatch', lambda: self.export(
            FakeClient(lambda *args: self.fail('No request after changed resume parameters')), per_page=50))

    def test_repeated_nonempty_page_does_not_loop_or_advance_checkpoint(self):
        self.assert_code('repeated_page', lambda: self.export(FakeClient(lambda a, b, n, s: page(n, [call()]))))
        self.assertEqual(2, self.read_manifest()['unresolved_windows'][0]['next_page'])
        self.assertEqual(1, self.db_value('SELECT COUNT(*) FROM calls'))

    def test_page_size_change_rejects_pagination_drift(self):
        client = FakeClient(lambda a, b, n, s: page(n, [call()] if n == 1 else [], size=2 if n == 1 else 3))
        self.assert_code('page_size_changed', lambda: self.export(client))
        self.assertFalse(self.read_manifest()['pagination_scan_complete'])

    def test_max_pages_is_explicit_failure_not_success(self):
        client = FakeClient(lambda a, b, n, s: page(n, [call(n)]))
        self.assert_code('max_pages_reached', lambda: self.export(client, max_pages=2))
        self.assertEqual(2, len(client.requests))
        self.assertEqual('incomplete', self.read_manifest()['status'])

    def test_increasing_page_budget_resumes_only_uncommitted_pages(self):
        first = FakeClient(lambda a, b, n, size: page(n, [call(n)]))
        self.assert_code('max_pages_reached', lambda: self.export(first, max_pages=2))
        self.assertEqual([1, 2], [request[2] for request in first.requests])
        resumed = FakeClient(lambda a, b, n, size: page(n, []))
        report = self.export(resumed, max_pages=3)
        self.assertEqual([3], [request[2] for request in resumed.requests])
        self.assertTrue(report['pagination_scan_complete'])
        self.assertEqual(2, report['calls'])
        self.assertEqual({'max_pages': 3}, report['execution_budget'])
        self.assertNotIn('max_pages', report['requested'])

    def test_legacy_checkpoint_page_budget_migrates_without_reimport(self):
        self.assert_code('max_pages_reached', lambda: self.export(
            FakeClient(lambda a, b, n, size: page(n, [call(n)])), max_pages=1))
        with sqlite3.connect(self.directory / 'checkpoint.sqlite3') as db:
            original = json.loads(db.execute("SELECT value FROM meta WHERE key='spec'").fetchone()[0])
            original['max_pages'] = 1
            db.execute("UPDATE meta SET value=? WHERE key='spec'", (json.dumps(original),))
            db.execute("DELETE FROM meta WHERE key='execution_budget'")
        resumed = FakeClient(lambda a, b, n, size: page(n, []))
        report = self.export(resumed, max_pages=2)
        self.assertEqual([2], [request[2] for request in resumed.requests])
        self.assertEqual(1, report['calls'])
        self.assertNotIn('max_pages', report['requested'])
        self.assertEqual({'max_pages': 2}, report['execution_budget'])

    def test_resume_keeps_data_parameters_fixed_when_budget_increases(self):
        self.assert_code('max_pages_reached', lambda: self.export(
            FakeClient(lambda a, b, n, size: page(n, [call(n)])), max_pages=1))
        for updates in ({'per_page': 50}, {'start': START - dt.timedelta(days=1)},
                        {'end': END + dt.timedelta(days=1)}, {'window_days': 1}):
            with self.subTest(updates=updates):
                client = FakeClient(lambda *args: self.fail('Changed data identity must not request history'))
                self.assert_code('resume_parameters_mismatch', lambda: self.export(client, max_pages=2, **updates))
        self.assertEqual(1, self.db_value('SELECT COUNT(*) FROM calls'))

    def test_resume_rejects_decreasing_page_budget_without_network(self):
        self.assert_code('max_pages_reached', lambda: self.export(
            FakeClient(lambda a, b, n, size: page(n, [call(n)])), max_pages=2))
        client = FakeClient(lambda *args: self.fail('Decreased budget must not request history'))
        self.assert_code('resume_budget_cannot_decrease', lambda: self.export(client, max_pages=1))
        self.assertEqual(0, client.probes)

    def test_all_available_honors_window_budget_and_does_not_infer_retention_from_empty(self):
        client = FakeClient(lambda a, b, n, s: page(n, []))
        report = self.export(client, start=None, all_available=True, max_windows=2, window_days=7)
        self.assertEqual(2, len(client.requests), 'max_windows is a hard API window budget')
        self.assertEqual('unknown', report['lower_bound'])
        self.assertEqual('partial_unknown_lower_bound', report['status'])
        self.assertFalse(report['provider_completeness_verified'])

    def test_private_output_permissions_and_exclusive_lock(self):
        self.export(FakeClient(lambda a, b, n, s: page(n, [call()] if n == 1 else [])))
        self.assertEqual(0o700, self.directory.stat().st_mode & 0o777)
        for path in self.directory.rglob('*'):
            self.assertEqual(0, path.stat().st_mode & 0o077, str(path.relative_to(self.directory)))
        with spike.Store(self.directory, configuration().binding):
            self.assert_code('output_locked', lambda: spike.Store(self.directory, configuration().binding))

    def test_output_directory_symlink_is_rejected(self):
        real = self.directory.parent / 'real'
        real.mkdir()
        self.directory.symlink_to(real, target_is_directory=True)
        self.assert_code('symlink_not_allowed', lambda: self.export(FakeClient(lambda *a: page(1, []))))

    def test_atomic_write_refuses_existing_symlink(self):
        target = self.directory.parent / 'private'
        target.write_text('original')
        link = self.directory.parent / 'link'
        link.symlink_to(target)
        self.assert_code('symlink_not_allowed', lambda: spike.atomic_write(link, 'replacement'))
        self.assertEqual('original', target.read_text())


class TestReconciliationAndCLI(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.directory = self.root / 'data' / 'export'
        self.reference = self.root / 'reference.json'
        self.output = self.root / 'report.json'

    def create_export(self, fail=False):
        def history(a, b, number, size):
            if number == 1:
                return page(number, [call(1), call(2, cdr=[])])
            if fail:
                raise spike.SafeError('api_request_failed', status=503)
            return page(number, [])
        try:
            return spike.export(FakeClient(history), self.directory, start=START, end=END)
        except spike.SafeError:
            if not fail:
                raise

    def write_reference(self, payload):
        self.reference.write_text(json.dumps(payload))
        self.reference.chmod(0o600)

    def test_matched_reference_is_not_automatic_human_or_live_attestation(self):
        self.create_export()
        self.write_reference([{'call_uuid': call(1)['call_uuid'], 'cdr_count': 1},
                              {'call_uuid': call(2)['call_uuid'], 'cdr_count': 0}])
        report = spike.reconcile(self.directory, self.reference, self.output)
        self.assertEqual('matched', report['comparison'])
        self.assertFalse(report['human_verified_same_period_timezone_filters_and_pbx'])
        self.assertEqual(ORG_A, report['organization_id'])
        self.assertEqual('42', report['provider_client_id'])
        self.assertNotIn(call(1)['call_uuid'], json.dumps(report))
        self.assertEqual(0o600, self.output.stat().st_mode & 0o777)

    def test_mismatches_missing_extra_and_cdr_counts_are_all_reported(self):
        self.create_export()
        self.write_reference([{'call_uuid': call(1)['call_uuid'], 'cdr_count': 3},
                              {'call_uuid': call(3)['call_uuid'], 'cdr_count': 0}])
        report = spike.reconcile(self.directory, self.reference, self.output)
        self.assertEqual('mismatch_or_incomplete', report['comparison'])
        self.assertEqual((1, 1, 1), (report['missing_calls'], report['extra_calls'], report['cdr_count_mismatches']))

    def test_equal_ids_in_incomplete_export_do_not_report_matched(self):
        self.create_export(fail=True)
        self.write_reference([{'call_uuid': call(1)['call_uuid']}, {'call_uuid': call(2)['call_uuid']}])
        report = spike.reconcile(self.directory, self.reference, self.output)
        self.assertEqual('mismatch_or_incomplete', report['comparison'])
        self.assertEqual(1, report['unresolved_windows'])

    def test_duplicate_or_invalid_reference_is_rejected(self):
        self.create_export()
        for payload in ([{'call_uuid': 'synthetic'}, {'call_uuid': 'synthetic'}],
                        [{'call_uuid': 'synthetic', 'cdr_count': -1}], {'bad': 'shape'}):
            self.write_reference(payload)
            with self.assertRaises(spike.SafeError) as caught:
                spike.reconcile(self.directory, self.reference, self.output)
            self.assertEqual('invalid_reference', caught.exception.code)

    def test_env_file_is_private_literal_data_not_shell_code(self):
        path = self.root / 'credentials.env'
        path.write_text('TELPHIN_APP_SECRET="$(do-not-execute)"\n')
        path.chmod(0o600)
        self.assertEqual('$(do-not-execute)', spike.load_env(path)['TELPHIN_APP_SECRET'])
        path.chmod(0o644)
        with self.assertRaises(spike.SafeError) as caught:
            spike.load_env(path)
        self.assertEqual('env_file_permissions', caught.exception.code)

    def invoke_main(self, argv, client=None):
        stdout, stderr = io.StringIO(), io.StringIO()
        env = {'PHONE_ORGANIZATION_ID': ORG_A, 'TELPHIN_APP_ID': 'synthetic-app',
               'TELPHIN_APP_SECRET': SENSITIVE, 'TELPHIN_CLIENT_ID': '42'}
        old_umask = os.umask(0o077)
        try:
            with mock.patch.object(spike, 'ROOT', self.root), mock.patch.dict(os.environ, env, clear=True), \
                 contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                if client is None:
                    result = spike.main(argv)
                else:
                    with mock.patch.object(spike, 'TelphinClient', return_value=client):
                        result = spike.main(argv)
        finally:
            os.umask(old_umask)
        return result, stdout.getvalue(), stderr.getvalue()

    def test_cli_safe_error_no_secret_traceback_or_provider_body(self):
        class FailedProbe:
            def probe(self):
                raise spike.SafeError('permission_denied', status=403)
        code, stdout, stderr = self.invoke_main(['probe'], FailedProbe())
        self.assertEqual(1, code)
        self.assertEqual('', stdout)
        self.assertNotIn(SENSITIVE, stdout + stderr)
        self.assertNotIn('Traceback', stderr)
        self.assertEqual('permission_denied', json.loads(stderr)['error'])

    def test_cli_partial_all_available_returns_nonzero_with_safe_counts(self):
        client = FakeClient(lambda a, b, n, s: page(n, []))
        code, stdout, stderr = self.invoke_main(['export', '--all-available', '--max-windows', '1',
                                                '--to', '2026-02-01', '--output', str(self.directory)], client)
        self.assertEqual(2, code)
        self.assertEqual('', stderr)
        self.assertEqual('partial_unknown_lower_bound', json.loads(stdout)['status'])
        self.assertNotIn(SENSITIVE, stdout)

    def test_cli_rate_limit_exit_code_is_temporary_failure(self):
        class LimitedProbe:
            def probe(self):
                raise spike.SafeError('rate_limited', status=429, next_attempt_at=4600)
        code, _, stderr = self.invoke_main(['probe'], LimitedProbe())
        self.assertEqual(75, code)
        self.assertEqual(4600, json.loads(stderr)['next_attempt_at'])

    def test_cli_refuses_export_outside_ignored_data(self):
        code, _, stderr = self.invoke_main(['export', '--from', '2026-01-31', '--output', str(self.root / 'docs')])
        self.assertEqual(1, code)
        self.assertEqual('output_must_be_inside_data', json.loads(stderr)['error'])

    def test_cli_help_works_without_credentials_or_network(self):
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout), self.assertRaises(SystemExit) as caught:
            spike.main(['--help'])
        self.assertEqual(0, caught.exception.code)
        self.assertIn('reconcile', stdout.getvalue())
        self.assertNotIn(SENSITIVE, stdout.getvalue())


if __name__ == '__main__':
    unittest.main()
