#!/usr/bin/env python3
"""Prove clean-volume startup using local built images and a temporary Compose project."""
import base64
import http.cookiejar
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def main():
    project = 'phone-smoke-' + secrets.token_hex(5)
    api_port, client_port, db_port, broker_port, mail_port, smtp_port = [free_port() for _ in range(6)]
    password = secrets.token_urlsafe(24)
    with tempfile.TemporaryDirectory(prefix=project) as directory:
        private = Path(directory) / 'runtime.env'
        settings = dict(COMPOSE_PROJECT_NAME=project, POSTGRES_USER='phone', POSTGRES_DB='phone_smoke',
            POSTGRES_PASSWORD=password, DATABASE_URL=f'postgres://phone:{password}@db:5432/phone_smoke',
            RABBITMQ_USER='phone', RABBITMQ_PASSWORD=password,
            RABBITMQ_URL=f'amqp://phone:{password}@rabbitmq:5672/%2f',
            ENCRYPTION_KEY=base64.b64encode(secrets.token_bytes(32)).decode(), DEMO_MODE='true',
            COOKIE_SECURE='false', PUBLIC_URL=f'http://127.0.0.1:{client_port}',
            ALLOWED_ORIGINS=f'http://127.0.0.1:{client_port}', API_BIND='0.0.0.0:8080',
            MEDIA_ROOT='/app/media', SMTP_HOST='mailpit', SMTP_PORT='1025',
            MAIL_FROM='Phone <noreply@phone.local>', MODEL_RUNNER_PASSWORD=secrets.token_urlsafe(24),
            BOOTSTRAP_EMAIL='owner@demo.local', BOOTSTRAP_PASSWORD='PhoneDemo!2026',
            BOOTSTRAP_NAME='Владелец', BOOTSTRAP_ORGANIZATION='Атлас · Продажи')
        fd = os.open(private, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, 'w') as stream:
            stream.write(''.join(f'{key}={value}\n' for key, value in settings.items()))
        override = Path(directory) / 'override.yml'
        lines = ['services:']
        ports = dict(api=f'{api_port}:8080', client=f'{client_port}:80', db=f'{db_port}:5432',
                     rabbitmq=f'{broker_port}:5672', mailpit=f'{mail_port}:8025')
        for service, port in ports.items():
            lines += [f'  {service}:', f'    ports: !override ["127.0.0.1:{port}"]']
            if service in ('api', 'worker', 'client'):
                lines += [f'    image: phone-platform-{service}:latest']
            if service == 'api':
                lines += [f'    env_file: !override ["{private}"]']
        lines += ['  worker:', '    image: phone-platform-worker:latest', f'    env_file: !override ["{private}"]']
        override.write_text('\n'.join(lines) + '\n')
        compose = ['docker', 'compose', '--env-file', str(private), '-p', project,
                   '-f', str(ROOT / 'infra/compose.yml'), '-f', str(override)]
        def run(*args):
            return subprocess.run(compose + list(args), check=True, capture_output=True, text=True)
        report = dict(project=project, synthetic=True, live_provider_requests=0, checks=[])
        checks = report['checks']
        try:
            run('up', '-d', '--no-build', '--wait', '--wait-timeout', '180')
            checks.append('clean_db_volumes_migrate_api_worker_client_ready')
            jar = http.cookiejar.CookieJar()
            opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
            origin = settings['PUBLIC_URL']
            csrf = None
            def request(path, body=None, method=None, headers=None):
                data = None if body is None else json.dumps(body).encode()
                h = {'Origin': origin, **(headers or {})}
                if data is not None:
                    h['Content-Type'] = 'application/json'
                if csrf:
                    h['X-CSRF-Token'] = csrf
                req = urllib.request.Request(origin + path, data=data, headers=h, method=method)
                with opener.open(req, timeout=20) as response:
                    payload = response.read()
                    return response.status, json.loads(payload)['data'] if payload.startswith(b'{') else payload
            assert request('/')[0] == 200
            _, me = request('/api/auth/login', dict(email='owner@demo.local', password='PhoneDemo!2026'))
            csrf = me['csrf_token']
            assert len(me['organizations']) == 2
            checks.append('nginx_spa_auth_two_organizations')
            _, calls = request('/api/calls?limit=100')
            call = next(c for c in calls['items'] if c.get('has_recording'))
            _, detail = request('/api/calls/' + call['id'])
            recording = next(r for r in detail['recordings'] if r.get('state') == 'ready')
            status, media = request(f"/api/calls/{call['id']}/media/{recording['id']}", headers={'Range': 'bytes=0-31'})
            assert status == 206 and len(media) == 32
            checks.append('private_media_range_real_named_volume')
            before = len(detail['analyses'])
            request(f"/api/calls/{call['id']}/reprocess", {'stage': 'all'})
            deadline = time.monotonic() + 75
            while time.monotonic() < deadline:
                _, after = request('/api/calls/' + call['id'])
                if len(after['analyses']) > before:
                    break
                time.sleep(1)
            else:
                raise AssertionError('worker did not finish synthetic processing')
            checks.append('api_job_worker_transcript_analysis_pipeline')
            other = next(o for o in me['organizations'] if o['id'] != me['organization']['id'])
            _, switched = request('/api/auth/organization', {'organization_id': other['id']})
            csrf = switched['csrf_token']
            try:
                request('/api/calls/' + call['id'])
                raise AssertionError('cross tenant call exposed')
            except urllib.error.HTTPError as error:
                assert error.code == 404
            checks.append('company_switch_foreign_call_404')
            request('/api/auth/logout', {})
            report['status'] = 'passed'
            destination = ROOT / 'data/eval/compose-smoke.json'
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
            print(json.dumps(report, ensure_ascii=False))
        finally:
            # Only this random project; never delete the main development volumes.
            run('down', '-v', '--remove-orphans')


if __name__ == '__main__':
    main()
