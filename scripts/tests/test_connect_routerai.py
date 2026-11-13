import contextlib
import copy
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import io
import json
import os
from pathlib import Path
import ssl
import tempfile
import threading
import time
import unittest
from unittest import mock
import urllib.parse
import urllib.request

MODULE = Path(__file__).resolve().parents[1] / 'connect-routerai.py'
spec = importlib.util.spec_from_file_location('phone_connect_routerai', MODULE)
subject = importlib.util.module_from_spec(spec)
spec.loader.exec_module(subject)
ORG = '11111111-1111-4111-8111-111111111111'
OTHER = '22222222-2222-4222-8222-222222222222'
KEY = 'sk-synthetic-routerai-key-not-real'
CODE = 'synthetic-private-code'


def proposal():
    return {'version': 1, 'organization_id': ORG, 'providers': {'routerai': {'name': 'RouterAI', 'options': {'apiKey': KEY, 'baseURL': 'https://routerai.ru/api/v1'}, 'models': {'synthetic/model': {'name': 'Synthetic'}}}}}


@contextlib.contextmanager
def provider_fixture(mode='success'):
    calls = []
    target_calls = []
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def do_POST(self):
            value = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
            calls.append(value)
            if mode == 'timeout': time.sleep(.15)
            if mode == 'redirect':
                self.send_response(302); self.send_header('Location', '/forbidden'); self.end_headers(); return
            raw = json.dumps({'key': KEY}).encode()
            try:
                self.send_response(200); self.send_header('Content-Type', 'application/json'); self.send_header('Content-Length', str(len(raw))); self.end_headers(); self.wfile.write(raw)
            except (BrokenPipeError, ConnectionResetError): pass
        def do_GET(self):
            target_calls.append(self.path); self.send_response(200); self.end_headers()
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    real_opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), subject.NoRedirect())
    class Remap:
        def open(self, request, timeout):
            if request.full_url != subject.EXCHANGE_URL: raise AssertionError('exchange destination is not fixed')
            mapped = urllib.request.Request('http://127.0.0.1:' + str(server.server_port) + '/exchange', data=request.data, headers=dict(request.headers), method=request.method)
            return real_opener.open(mapped, timeout=.02 if mode == 'timeout' else timeout)
    try: yield Remap(), calls, target_calls
    finally: server.shutdown(); server.server_close(); thread.join(1)


@contextlib.contextmanager
def callback_fixture(exchange, commit=None, clock=time.monotonic):
    flow = subject.ConsentFlow(ORG, exchange=exchange, commit=commit, clock=clock)
    server = subject.callback_server(flow)
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    def request(query=None, path='/callback', hosts=None):
        connection = http.client.HTTPConnection('127.0.0.1', server.server_port, timeout=3)
        connection.putrequest('GET', path + ('?' + urllib.parse.urlencode(query) if query is not None else ''), skip_host=True)
        for host in hosts or [flow.host]: connection.putheader('Host', host)
        connection.endheaders(); response = connection.getresponse(); value = (response.status, dict(response.getheaders()), response.read()); connection.close(); return value
    try: yield flow, request
    finally: server.shutdown(); server.server_close(); thread.join(1)


class RouterAIConnectTests(unittest.TestCase):
    def callback(self, flow, **extra):
        return {'code': CODE, 'state': flow.state, 'iss': subject.ISSUER, **extra}

    def test_real_localhost_callback_pkce_strip_and_single_exchange_no_secret_output(self):
        stdout, stderr, committed = io.StringIO(), io.StringIO(), []
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr), provider_fixture() as (opener, calls, _):
            with callback_fixture(lambda code, verifier, url: subject.exchange_code(code, verifier, url, opener), committed.append) as (flow, request):
                url = urllib.parse.urlsplit(flow.authorization_url()); query = urllib.parse.parse_qs(url.query)
                self.assertEqual(url.scheme + '://' + url.netloc + url.path, subject.AUTH_URL)
                self.assertEqual(query['code_challenge_method'], ['S256']); self.assertEqual(query['code_challenge'], [flow.challenge]); self.assertNotIn(flow.verifier, flow.authorization_url()); self.assertIn(ORG, query['app_name'][0]); self.assertTrue(query['app_name'][0].startswith('Phone Platform Local / '))
                status, headers, body = request(self.callback(flow))
                self.assertEqual(status, 303); self.assertEqual(headers['Location'], '/connected'); self.assertEqual(body, b''); self.assertEqual(headers['Cache-Control'], 'no-store')
                self.assertTrue(flow.done.wait(2)); self.assertEqual(committed, [KEY]); self.assertEqual(flow.result['state'], 'connected')
                self.assertEqual(request(self.callback(flow))[0], 409); self.assertEqual(len(calls), 1)
                self.assertEqual(calls[0]['code_verifier'], flow.verifier); self.assertEqual(calls[0]['code_challenge_method'], 'S256'); self.assertEqual(calls[0]['callback_url'], flow.callback_url)
                clean = request(path='/connected')[2].decode(); self.assertNotIn(KEY, clean); self.assertNotIn(CODE, clean)
        output = stdout.getvalue() + stderr.getvalue()
        for secret in [KEY, CODE, flow.verifier]: self.assertNotIn(secret, output)

    def test_wrong_host_path_state_issuer_duplicates_do_not_consume_valid_attempt(self):
        calls = []
        with callback_fixture(lambda *args: calls.append(args) or KEY) as (flow, request):
            self.assertEqual(request(self.callback(flow), hosts=['other.example'])[0], 400)
            self.assertEqual(request(self.callback(flow), hosts=[flow.host, flow.host])[0], 400)
            self.assertEqual(request(self.callback(flow), path='/wrong')[0], 404)
            self.assertEqual(request(self.callback(flow, state='incorrect'))[0], 400)
            self.assertEqual(request(self.callback(flow, state='☃'))[0], 400)
            self.assertEqual(request(self.callback(flow, iss='https://other.example'))[0], 400)
            self.assertEqual(request({'code': CODE, 'state': flow.state})[0], 400)
            for duplicate in ['code', 'state', 'iss']:
                query = list(self.callback(flow).items()) + [(duplicate, self.callback(flow)[duplicate])]
                self.assertEqual(request(query)[0], 400)
            self.assertEqual(calls, []); self.assertFalse(flow.consumed)
            self.assertEqual(request(self.callback(flow))[0], 303); self.assertTrue(flow.done.wait(2)); self.assertEqual(len(calls), 1)

    def test_expired_or_denied_consent_does_not_exchange(self):
        now = [100.]
        calls = []
        with callback_fixture(lambda *args: calls.append(args) or KEY, clock=lambda: now[0]) as (flow, request):
            now[0] = 401.
            self.assertEqual(request(self.callback(flow))[0], 410); self.assertEqual(calls, [])
        with callback_fixture(lambda *args: calls.append(args) or KEY) as (flow, request):
            self.assertEqual(request({'state': flow.state, 'error': 'access_denied'})[0], 303)
            self.assertEqual(flow.result['error'], 'routerai_consent_denied'); self.assertEqual(calls, [])

    def test_actual_exchange_timeout_cannot_replay_or_revoke_new_key(self):
        with provider_fixture('timeout') as (opener, calls, _):
            with callback_fixture(lambda code, verifier, url: subject.exchange_code(code, verifier, url, opener)) as (flow, request):
                self.assertEqual(request(self.callback(flow))[0], 303); self.assertTrue(flow.done.wait(2)); self.assertEqual(flow.result['error'], 'routerai_exchange_uncertain')
                self.assertEqual(request(self.callback(flow))[0], 409); flow.finish(CODE)
                self.assertEqual(len(calls), 1)

    def test_exchange_redirect_is_not_followed(self):
        with provider_fixture('redirect') as (opener, calls, target):
            with self.assertRaises(subject.ConnectError) as caught: subject.exchange_code(CODE, 'synthetic-verifier', 'http://127.0.0.1:1/callback', opener)
            self.assertEqual(caught.exception.code, 'routerai_exchange_rejected'); self.assertEqual(len(calls), 1); self.assertEqual(target, [])

    def test_runtime_commit_merges_current_providers_atomically_under_shared_lock(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary); target = directory / 'local-opencode.json'
            initial = {'version': 1, 'organization_id': ORG, 'providers': {'existing': {'marker': 'preserve'}}}
            subject.private_write(target, initial)
            stale_proposal = proposal()
            # A provider added while the browser was open must not be overwritten.
            initial['providers']['parallel'] = {'marker': 'added during consent'}
            subject.private_write(target, initial)
            installed = []
            merged = subject.merge_routerai_runtime(directory, stale_proposal, lambda value: installed.append(copy.deepcopy(value)))
            self.assertEqual(set(merged['providers']), {'existing', 'parallel', 'routerai'}); self.assertEqual(installed, [merged]); self.assertEqual(json.loads(target.read_text()), merged); self.assertEqual(target.stat().st_mode & 0o777, 0o600); self.assertEqual((directory / 'local-opencode.lock').stat().st_mode & 0o777, 0o600)
            self.assertEqual(list(directory.glob('local-opencode.json.*')), [])

    def test_current_tenant_or_symlink_mismatch_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary); target = directory / 'local-opencode.json'; other = {'version':1,'organization_id':OTHER,'providers':{}}
            subject.private_write(target, other); calls=[]
            with self.assertRaises(subject.ConnectError): subject.merge_routerai_runtime(directory, proposal(), calls.append)
            self.assertEqual(json.loads(target.read_text()), other); self.assertEqual(calls, [])
            target.unlink(); destination=directory/'outside.json';subject.private_write(destination, other);target.symlink_to(destination)
            with self.assertRaises(subject.ConnectError): subject.merge_routerai_runtime(directory, proposal(), calls.append)
            self.assertEqual(json.loads(destination.read_text()), other); self.assertEqual(calls, [])

    def test_failed_install_preserves_host_runtime_and_rolls_back_only_current_volume(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary);target=directory/'local-opencode.json';old={'version':1,'organization_id':ORG,'providers':{'existing':{'marker':'preserve'}}};subject.private_write(target,old);calls=[]
            def install(value):
                calls.append(copy.deepcopy(value))
                if len(calls)==1:raise OSError('synthetic-install-failure')
            with self.assertRaises(subject.ConnectError)as caught:subject.merge_routerai_runtime(directory,proposal(),install)
            self.assertEqual(caught.exception.code,'runtime_install_failed');self.assertEqual(json.loads(target.read_text()),old);self.assertEqual(calls[-1],old)
            self.assertEqual(list(directory.glob('local-opencode.json.*')),[])

    def test_successful_callback_reuses_actual_importer_and_persists_only_routerai(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary);source=directory/'source.json'
            source.write_text(json.dumps({'provider':{'routerai':{'name':'RouterAI','options':{'baseURL':'https://routerai.ru/api/v1'},'models':{'synthetic/model':{'name':'Synthetic'}}},'unrelated':{'options':{'baseURL':'https://unrelated.example/v1','apiKey':'synthetic-unrelated-key'},'models':{}}}}))
            tools=subject.import_tools();installed=[]
            def commit(key):
                settings=tools.build_settings(source,ORG,provider_ids={'routerai'},credentials={'routerai':key})
                subject.merge_routerai_runtime(directory,settings,lambda value:installed.append(copy.deepcopy(value)))
            with provider_fixture()as(opener,calls,_):
                with callback_fixture(lambda code,verifier,url:subject.exchange_code(code,verifier,url,opener),commit)as(flow,request):
                    self.assertEqual(request(self.callback(flow))[0],303);self.assertTrue(flow.done.wait(2));self.assertEqual(flow.result['state'],'connected')
            saved=json.loads((directory/'local-opencode.json').read_text());self.assertEqual(set(saved['providers']),{'routerai'});self.assertEqual(saved['providers']['routerai']['options']['apiKey'],KEY);self.assertEqual(installed,[saved]);self.assertNotIn('synthetic-unrelated-key',json.dumps(saved))

    def test_failed_atomic_write_restores_volume_and_leaves_previous_host_file(self):
        with tempfile.TemporaryDirectory()as temporary:
            directory=Path(temporary);target=directory/'local-opencode.json';old={'version':1,'organization_id':ORG,'providers':{}};subject.private_write(target,old);calls=[]
            with mock.patch.object(subject.os,'replace',side_effect=OSError('synthetic-rename-failure')):
                with self.assertRaises(subject.ConnectError):subject.merge_routerai_runtime(directory,proposal(),lambda value:calls.append(copy.deepcopy(value)))
            self.assertEqual(json.loads(target.read_text()),old);self.assertEqual(calls[-1],old);self.assertEqual(list(directory.glob('local-opencode.json.*')),[])

if __name__=='__main__':unittest.main()
