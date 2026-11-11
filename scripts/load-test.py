#!/usr/bin/env python3
"""Bounded local HTTP loadcheck on disposable synthetic tenant data; no provider calls."""
from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
import http.cookiejar
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent


def private_configuration(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o077:
        raise RuntimeError('private_env_required')
    return dict(line.split('=', 1) for line in path.read_text().splitlines()
                if line and not line.startswith('#'))


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class Client:
    def __init__(self, base_url):
        self.base_url = base_url.rstrip('/')
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar), NoRedirect())
        self.csrf = None
        self.organization = None

    def request(self, path, body=None, expected=200):
        headers = {'Accept': 'application/json'}
        if self.organization:
            headers['X-Organization-Id'] = self.organization
        if body is not None:
            headers['Content-Type'] = 'application/json'
            if self.csrf:
                headers['X-CSRF-Token'] = self.csrf
        request = urllib.request.Request(self.base_url + path,
                                        data=None if body is None else json.dumps(body).encode(), headers=headers)
        started = time.perf_counter()
        try:
            response = self.opener.open(request, timeout=60)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read(64 * 1024 * 1024 + 1)
            if len(raw) > 64 * 1024 * 1024 or response.status != expected:
                raise RuntimeError('unexpected_http_response')
            value = json.loads(raw) if raw else {}
        elapsed = time.perf_counter() - started
        if expected == 200 and ('data' not in value or 'error' in value):
            raise RuntimeError('invalid_api_envelope')
        return value.get('data', value), elapsed

    def session(self, value):
        self.csrf = value['csrf_token']
        self.organization = value['organization']['id']


def database(env_file, config, sql, variables):
    args = ['docker', 'compose', '--env-file', str(env_file), '-f', str(ROOT / 'infra/compose.yml'),
            'exec', '-T', 'db', 'psql', '-X', '-q', '-A', '-t', '-U', config['POSTGRES_USER'],
            '-d', config['POSTGRES_DB'], '-v', 'ON_ERROR_STOP=1']
    for key, value in variables.items():
        args += ['-v', f'{key}={value}']
    result = subprocess.run(args, input=sql, capture_output=True, text=True, cwd=ROOT)
    if result.returncode:
        raise RuntimeError('load_fixture_database_failed')
    return result.stdout.strip()


SETUP_SQL = r'''
BEGIN;
SET LOCAL TIME ZONE 'UTC';
INSERT INTO organizations(id,name,timezone,callback_minutes,data)
VALUES (:'organization'::uuid,'Loadcheck temporary','UTC',120,'{"callback_allow_inbound":true,"callback_min_duration":1}'),
       (:'foreign_org'::uuid,'Loadcheck foreign canary','UTC',120,'{}');
INSERT INTO memberships(organization_id,user_id,role)
SELECT :'organization'::uuid,id,'owner' FROM users WHERE email=:'owner_email';
INSERT INTO documents(organization_id,id,kind,data)
SELECT :'organization'::uuid,md5(:'organization'||':'||i::text)::uuid,'call',
 jsonb_build_object('provider_id','load-'||:'organization'||'-'||i::text,
 'phone','+7900'||lpad((((i-1)/4)*2 + CASE WHEN i%4=0 THEN 1 ELSE 0 END)::text,7,'0'),
 'started_at',to_char(now()-interval '7 days'+i*interval '1 second','YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
 'direction',CASE WHEN i%4 IN (1,0) THEN 'in' ELSE 'out' END,
 'provider_result',CASE WHEN i%4=3 THEN 'bridged' ELSE 'not answered' END,
 'human_connected',i%4=3,'duration',CASE WHEN i%4=3 THEN 90 ELSE 0 END,
 'employee','Synthetic team '||(i%10)::text,'contact_name','Synthetic load contact',
 'coverage_complete',true,'has_recording',false,'tag_ids','[]'::jsonb,
 'contact_kind','unknown','stage','imported','score',null,'manual_lock',false)
FROM generate_series(1,:'calls'::int) i;
INSERT INTO documents(organization_id,id,kind,data)
VALUES(:'foreign_org'::uuid,:'foreign_call'::uuid,'call',jsonb_build_object('provider_id','foreign-canary','phone','+79009999999','direction','in','provider_result','not answered','started_at',now(),'coverage_complete',true));
SELECT json_build_object('calls',(SELECT count(*) FROM documents WHERE organization_id=:'organization'::uuid),
                        'membership',(SELECT count(*) FROM memberships WHERE organization_id=:'organization'::uuid));
COMMIT;
'''
CLEANUP_SQL = r'''
DELETE FROM organizations WHERE (id=:'organization'::uuid AND name='Loadcheck temporary')
 OR (id=:'foreign_org'::uuid AND name='Loadcheck foreign canary');
SELECT count(*) FROM organizations WHERE id IN (:'organization'::uuid,:'foreign_org'::uuid);
'''


def run_profile(client, calls, marker, foreign_call, rounds, concurrency):
    group = calls // 4
    ast = {'op': 'and', 'rules': [{'field': 'direction', 'op': 'eq', 'value': 'out'},
                                 {'field': 'human_connected', 'op': 'eq', 'value': True}]}
    endpoints = {'calls': '/api/calls?limit=100',
                 'filtered_calls': '/api/calls?' + urllib.parse.urlencode({'limit': 100, 'filter': json.dumps(ast)}),
                 'dashboard': '/api/dashboard', 'callbacks': '/api/callbacks'}

    def measure(name):
        value, elapsed = client.request(endpoints[name])
        if name in ('calls', 'filtered_calls'):
            if value['total'] != (calls if name == 'calls' else group) or len(value['items']) != 100:
                raise RuntimeError('list_count_mismatch')
            if any(not row['provider_id'].startswith(marker) for row in value['items']):
                raise RuntimeError('foreign_tenant_data_in_list')
            if name == 'filtered_calls' and any(row['direction'] != 'out' or row['human_connected'] is not True or row['contact_kind'] != 'repeat' for row in value['items']):
                raise RuntimeError('filter_semantics_mismatch')
        elif name == 'dashboard':
            totals = value['totals']
            if any(totals[key] != expected for key, expected in {'calls': calls, 'answered': group,
                   'missed': group * 2, 'unhandled': group, 'overdue': group, 'analyzed': 0}.items()):
                raise RuntimeError('dashboard_count_mismatch')
            if totals['average_score'] is not None or sum(day['calls'] for day in value['daily']) != calls:
                raise RuntimeError('dashboard_aggregate_mismatch')
        else:
            episodes = value['items']
            if len(episodes) != group * 2:
                raise RuntimeError('callback_count_mismatch')
            resolved = [e for e in episodes if e['state'] == 'resolved']
            overdue = [e for e in episodes if e['state'] == 'overdue']
            if len(resolved) != group or len(overdue) != group or any(e['attempts'] != 1 for e in resolved):
                raise RuntimeError('callback_semantics_mismatch')
        return name, elapsed

    # Untimed correctness checks include tenant isolation and stable pagination.
    client.request('/api/calls/' + foreign_call, expected=404)
    first, _ = client.request(endpoints['calls'])
    second, _ = client.request(endpoints['calls'] + '&' + urllib.parse.urlencode({'cursor': first['next_cursor']}))
    if second['total'] != calls or {v['id'] for v in first['items']} & {v['id'] for v in second['items']}:
        raise RuntimeError('pagination_mismatch')
    filtered_dashboard, _ = client.request('/api/dashboard?' + urllib.parse.urlencode({'filter': json.dumps(ast)}))
    if filtered_dashboard['totals']['calls'] != group or filtered_dashboard['totals']['answered'] != group:
        raise RuntimeError('dashboard_filter_mismatch')
    for name in endpoints:
        measure(name)  # warm caches; still validate the result.
    jobs = list(endpoints) * rounds
    started = time.perf_counter()
    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        results = list(executor.map(measure, jobs))
    wall = time.perf_counter() - started
    metrics = {}
    for name in endpoints:
        samples = sorted(elapsed for key, elapsed in results if key == name)
        metrics[name] = {'requests': len(samples), 'p50_ms': round(statistics.median(samples) * 1000, 2),
                         'p95_ms': round(samples[math.ceil(len(samples) * .95) - 1] * 1000, 2),
                         'max_ms': round(max(samples) * 1000, 2)}
    return {'synthetic_calls': calls, 'measured_requests': len(results), 'concurrency': concurrency,
            'rounds': rounds, 'wall_seconds': round(wall, 3),
            'requests_per_second': round(len(results) / wall, 2), 'endpoints': metrics,
            'correctness': {'list_counts': True, 'ast_filter': True, 'dashboard_counts': True,
                            'callback_attempt_vs_success': True, 'tenant_id_access_denied': True,
                            'pagination_disjoint': True, 'unknown_score_not_zero': True}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--env-file', type=Path, default=ROOT / '.env.platform')
    parser.add_argument('--base-url', default='http://127.0.0.1:18080')
    parser.add_argument('--calls', type=int, choices=(5000, 10000), default=5000)
    parser.add_argument('--rounds', type=int, default=8)
    parser.add_argument('--concurrency', type=int, default=4)
    parser.add_argument('--max-p95-ms', type=float, default=5000)
    parser.add_argument('--report', type=Path, default=ROOT / 'data/eval/load-report.json')
    args = parser.parse_args()
    url = urllib.parse.urlsplit(args.base_url)
    if url.scheme != 'http' or url.hostname not in ('127.0.0.1', 'localhost', '::1') or url.username or url.password or url.path not in ('', '/') or url.query or url.fragment:
        parser.error('base-url must be a local HTTP API origin')
    if not 1 <= args.rounds <= 100 or not 1 <= args.concurrency <= 16 or not 0 < args.max_p95_ms <= 60000:
        parser.error('bounded rounds/concurrency/latency required')
    organization, foreign_org, foreign_call = (str(uuid.uuid4()) for _ in range(3))
    variables = {'organization': organization, 'foreign_org': foreign_org, 'foreign_call': foreign_call,
                 'calls': args.calls}
    client = Client(args.base_url)
    setup_attempted = False
    report = None
    failure = None
    config = None
    try:
        config = private_configuration(args.env_file)
        if config.get('DEMO_MODE') != 'true':
            raise RuntimeError('local_demo_mode_required')
        variables['owner_email'] = config['BOOTSTRAP_EMAIL'].strip().lower()
        setup_attempted = True
        fixture = json.loads(database(args.env_file, config, SETUP_SQL, variables))
        if fixture != {'calls': args.calls, 'membership': 1}:
            raise RuntimeError('fixture_count_mismatch')
        login, _ = client.request('/api/auth/login', {'email': config['BOOTSTRAP_EMAIL'], 'password': config['BOOTSTRAP_PASSWORD']})
        client.session(login)
        switched, _ = client.request('/api/auth/organization', {'organization_id': organization})
        client.session(switched)
        report = run_profile(client, args.calls, 'load-' + organization + '-', foreign_call,
                             args.rounds, args.concurrency)
        report['latency_threshold_ms'] = args.max_p95_ms
        report['status'] = 'passed' if all(v['p95_ms'] <= args.max_p95_ms for v in report['endpoints'].values()) else 'failed'
        report['scope'] = 'local synthetic query profile; no production capacity claim'
        report['external_provider_requests'] = 0
    except (OSError, ValueError, KeyError, RuntimeError, urllib.error.URLError):
        failure = 'loadcheck_failed'
    finally:
        if client.csrf:
            try:
                client.request('/api/auth/logout', {})
            except (OSError, ValueError, RuntimeError, urllib.error.URLError):
                failure = 'loadcheck_logout_failed'
        # Cleanup even after partially executed setup: UUIDs were generated only for this run.
        if setup_attempted:
            try:
                remaining = database(args.env_file, config, CLEANUP_SQL, variables)
                if remaining != '0':
                    failure = 'loadcheck_cleanup_failed'
            except (OSError, RuntimeError):
                failure = 'loadcheck_cleanup_failed'
    if failure:
        print(json.dumps({'status': 'failed', 'error': failure}), file=sys.stderr)
        return 1
    report['disposable_tenants_removed'] = True
    args.report.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd = os.open(args.report, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, 'w') as stream:
        json.dump(report, stream, indent=2)
        stream.write('\n')
    print(json.dumps(report))
    return 0 if report['status'] == 'passed' else 1


if __name__ == '__main__':
    os.umask(0o077)
    sys.exit(main())
