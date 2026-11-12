#!/usr/bin/env python3
"""First planned transfer of THIS Phone Platform only; never an update/restore-overwrite tool.

Passwords/configuration are private files or stdin, never argv or public diagnostics.
This tool deliberately has fixed production paths/SSH target and no arbitrary shell input.
Requires Python3, Node with crypto.argon2Sync, Docker Compose and authorized SSH.
"""
from __future__ import annotations
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shlex
import stat
import subprocess
import sys
import tarfile
import uuid

ROOT = Path(__file__).resolve().parent.parent
HOST = 'dks@81.16.141.170'
REMOTE = '/home/dks/phone-platform'
DEMO_EMAILS = ('owner@demo.local', 'manager@demo.local', 'reviewer@demo.local')
ARCHIVES = ('database.dump', 'media.tar.gz', 'assistant-auth.tar.gz')
WRITERS = ('api', 'worker', 'assistant-runner')
BUSYBOX = 'busybox:1.37@sha256:a7dbc319b1438115a207e11930dc4bb3e643dcb3ec0bdd694bb36cd8fad289ec'
COUNTS_SQL = """SELECT json_build_object(
'organizations',(SELECT count(*) FROM organizations),
'users',(SELECT count(*) FROM users),
'memberships',(SELECT count(*) FROM memberships),
'documents',(SELECT count(*) FROM documents),
'jobs',(SELECT count(*) FROM jobs),
'calls',(SELECT count(*) FROM documents WHERE kind='call'),
'recordings',(SELECT count(*) FROM documents WHERE kind='recording'),
'connections',(SELECT count(*) FROM documents WHERE kind='connection'),
'ready_recordings',(SELECT count(*) FROM documents WHERE kind='recording' AND
 (data->>'ready'='true' OR data->>'state'='ready')));"""
QUEUE_SQL = """SELECT json_build_object(
'jobs',(SELECT count(*) FROM jobs WHERE state IN ('queued','retry','running','uncertain')),
'outbox',(SELECT count(*) FROM outbox_events WHERE published_at IS NULL),
'assistant_runs',(SELECT count(*) FROM documents WHERE kind='assistant_run'
 AND data->>'state' IN ('queued','running')));"""
AUTH_SQL = """SELECT COALESCE(json_agg(json_build_object('organization_id',organization_id,
'authenticated',data->'authenticated','generation',data->'generation',
'confirmed_generation',data->'confirmed_generation') ORDER BY organization_id),'[]'::json)
FROM documents WHERE kind='assistant_auth_control';"""
PRESERVE_SQL = """SELECT md5(COALESCE(string_agg(organization_id::text||id::text||kind||data::text,
'' ORDER BY organization_id,id),'')) FROM documents WHERE kind IN
('connection','call','recording','transcript','analysis');"""


def run(argv, *, input=None, stdin=None, stdout=subprocess.PIPE, text=False):
    result = subprocess.run(argv, input=input, stdin=stdin, stdout=stdout,
                            stderr=subprocess.PIPE, text=text)
    if result.returncode:
        # Never attach stderr/argv: either can contain database/credential material.
        raise RuntimeError('migration_command_failed')
    return result.stdout


def private(path: Path):
    if path.is_symlink() or not path.is_file() or stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise RuntimeError('private_regular_file_required')


def read_env(path: Path):
    private(path)
    result = {}
    for line in path.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        key, sep, value = line.partition('=')
        if not sep or key in result or not re.fullmatch(r'[A-Z_][A-Z0-9_]*', key):
            raise RuntimeError('invalid_private_env')
        result[key] = value
    return result


def write_once(path: Path, content: str | bytes):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.parent.is_symlink():
        raise RuntimeError('private_directory_required')
    raw = content.encode() if isinstance(content, str) else content
    fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw)


def digest(path: Path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def require_source(config):
    if config.get('COMPOSE_PROJECT_NAME') != 'phone-platform':
        raise RuntimeError('source_project_not_phone_platform')
    try:
        if len(base64.b64decode(config['ENCRYPTION_KEY'], validate=True)) != 32:
            raise ValueError()
    except (KeyError, ValueError):
        raise RuntimeError('source_encryption_key_invalid') from None


def local_compose():
    return ['docker', 'compose', '--env-file', str(ROOT / '.env.platform'),
            '-f', str(ROOT / 'infra/compose.yml')]


def local_sql(sql):
    config = read_env(ROOT / '.env.platform')
    require_source(config)
    return run(local_compose() + ['exec', '-T', 'db', 'psql', '-X', '-q', '-A', '-t',
               '-U', config['POSTGRES_USER'], '-d', config['POSTGRES_DB'],
               '-v', 'ON_ERROR_STOP=1'], input=sql, text=True).strip()


def ssh(command, **kwargs):
    return run(['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', HOST, command], **kwargs)


def remote_python(code, args=(), **kwargs):
    return ssh(shlex.join(['python3', '-c', code, *map(str, args)]), **kwargs)


def remote_compose(args, **kwargs):
    argv = ['env', '-u', 'MODEL_RUNNER_PASSWORD', 'docker', 'compose', '--env-file', '.env.platform', '--env-file',
            'secrets/analyzer-pair.env', '-f', 'infra/compose.yml', '-f',
            'infra/compose.production.yml', '-f', 'infra/compose.analyzer-remote.yml',
            '-f', 'infra/compose.release.yml', *args]
    return ssh('cd ' + shlex.quote(REMOTE) + ' && ' + shlex.join(argv), **kwargs)


def remote_sql(sql):
    return remote_compose(['exec', '-T', 'db', 'psql', '-X', '-q', '-A', '-t', '-U',
                          'phone', '-d', 'phone_platform', '-v', 'ON_ERROR_STOP=1'],
                          input=sql, text=True).strip()


def require_idle(value):
    if set(value) != {'jobs', 'outbox', 'assistant_runs'} or any(type(v) is not int or v != 0 for v in value.values()):
        raise RuntimeError('source_has_pending_work_capture_refused')


def passwords():
    path = ROOT / 'secrets/production-access.json'
    if path.exists():
        private(path)
        data = json.loads(path.read_text())
        if data.get('url') != 'https://phone.bondscan.ru' or set(data.get('accounts', {})) != set(DEMO_EMAILS):
            raise RuntimeError('existing_access_file_invalid')
        values = list(data['accounts'].values())
        if len(set(values)) != 3 or any(not isinstance(v, str) or not re.fullmatch(r'[A-Za-z0-9_-]{32,128}', v) for v in values):
            raise RuntimeError('existing_access_file_invalid')
        return data
    data = {'url': 'https://phone.bondscan.ru', 'accounts': {email: secrets.token_urlsafe(30) for email in DEMO_EMAILS}}
    write_once(path, json.dumps(data, indent=2) + '\n')
    return data


HASH_JS = """const crypto=require('node:crypto');let raw='';
process.stdin.setEncoding('utf8');process.stdin.on('data',c=>raw+=c);
process.stdin.on('end',()=>{const v=JSON.parse(raw);const result=[];
for(const[email,password]of Object.entries(v)){const salt=crypto.randomBytes(16);
 const h=crypto.argon2Sync('argon2id',{message:Buffer.from(password),nonce:salt,
 parallelism:1,tagLength:32,memory:19456,passes:2});
 const b=x=>x.toString('base64').replace(/=+$/,'');
 result.push({email,password_hash:'$argon2id$v=19$m=19456,t=2,p=1$'+b(salt)+'$'+b(h)});}
process.stdout.write(JSON.stringify(result));});"""


def make_hashes(access):
    return json.loads(run(['node', '-e', HASH_JS], input=json.dumps(access['accounts']), text=True))


DECRYPT_JS = """const crypto=require('node:crypto');let raw='';
process.stdin.setEncoding('utf8');process.stdin.on('data',c=>raw+=c);
process.stdin.on('end',()=>{try{const input=JSON.parse(raw),key=Buffer.from(input.key,'base64');
if(key.length!==32||!Array.isArray(input.sealed))throw Error();
for(const value of input.sealed){if(typeof value!=='string'||!value.startsWith('v1.'))throw Error();
 const b=Buffer.from(value.slice(3),'base64');if(b.length<29)throw Error();
 const decipher=crypto.createDecipheriv('aes-256-gcm',key,b.subarray(0,12));
 decipher.setAuthTag(b.subarray(-16));
 const plaintext=Buffer.concat([decipher.update(b.subarray(12,-16)),decipher.final()]);
 const object=JSON.parse(plaintext.toString('utf8'));
 if(!object||Array.isArray(object)||typeof object!=='object')throw Error();}
process.stdout.write(JSON.stringify({decrypted_connections:input.sealed.length}));
}catch{process.exitCode=1;}});"""


def verify_encrypted_connections(config, sealed):
    # Provider secrets remain inside ephemeral child memory; diagnostics contain only a count.
    return json.loads(run(['node','-e',DECRYPT_JS],
                     input=json.dumps({'key':config['ENCRYPTION_KEY'],'sealed':sealed}),text=True))


def production_config(source, release):
    require_source(source)
    if not re.fullmatch(r'15-[0-9a-f]{7,40}', release):
        raise RuntimeError('invalid_release_tag')
    db_password = secrets.token_urlsafe(36)
    broker_password = secrets.token_urlsafe(36)
    return {
        'COMPOSE_PROJECT_NAME': 'phone-platform', 'PHONE_RELEASE_TAG': release,
        'POSTGRES_USER': 'phone', 'POSTGRES_DB': 'phone_platform',
        'POSTGRES_PASSWORD': db_password,
        'DATABASE_URL': f'postgres://phone:{db_password}@db:5432/phone_platform',
        'RABBITMQ_USER': 'phone', 'RABBITMQ_PASSWORD': broker_password,
        'RABBITMQ_URL': f'amqp://phone:{broker_password}@rabbitmq:5672/%2f',
        'ENCRYPTION_KEY': source['ENCRYPTION_KEY'],
        'ASSISTANT_RUNNER_TOKEN': secrets.token_urlsafe(36),
        'ASSISTANT_RUNNER_URL': 'http://assistant-runner:8091',
        'MODEL_RUNNER_URL': 'http://model-runner:4096',
        'DEMO_MODE': 'false', 'COOKIE_SECURE': 'true',
        'PUBLIC_URL': 'https://phone.bondscan.ru',
        'ALLOWED_ORIGINS': 'https://phone.bondscan.ru',
        'API_BIND': '0.0.0.0:8080', 'MEDIA_ROOT': '/app/media',
        # Explicitly unavailable until an operator supplies an independent real SMTP.
        'SMTP_HOST': '127.0.0.1', 'SMTP_PORT': '1', 'SMTP_TLS': 'false',
        'MAIL_FROM': 'Phone Platform <noreply@phone.bondscan.ru>',
    }


REMOTE_WRITE = """import os,pathlib,sys,hashlib,uuid
root=pathlib.Path('/home/dks/phone-platform')
if root.is_symlink() or not root.is_dir():raise SystemExit(1)
relative=pathlib.PurePosixPath(sys.argv[1])
if relative.is_absolute() or '..' in relative.parts:raise SystemExit(1)
public=len(sys.argv)>3 and sys.argv[3]=='public'
if public and str(relative)!='secrets/analyzer-ssh/id_ed25519.pub':raise SystemExit(1)
target=root.joinpath(*relative.parts)
walk=root
for part in relative.parts[:-1]:
 walk=walk/part
 if walk.is_symlink():raise SystemExit(1)
 walk.mkdir(mode=0o700,exist_ok=True)
if target.exists() or target.is_symlink():
 if target.is_symlink() or not target.is_file():raise SystemExit(1)
 expected=sys.argv[2]
 mode=target.stat().st_mode&0o777
 if (mode not in (0o600,0o644) if public else bool(mode&0o077)) or hashlib.sha256(target.read_bytes()).hexdigest()!=expected:raise SystemExit(1)
 if public:os.chmod(target,0o644)
 raise SystemExit(0)
temporary=target.with_name('.'+target.name+'.upload-'+uuid.uuid4().hex)
fd=os.open(temporary,os.O_CREAT|os.O_EXCL|os.O_WRONLY|os.O_NOFOLLOW,0o600)
try:
 h=hashlib.sha256()
 with os.fdopen(fd,'wb')as f:
  while True:
   data=sys.stdin.buffer.read(1024*1024)
   if not data:break
   h.update(data);f.write(data)
  if h.hexdigest()!=sys.argv[2]:raise SystemExit(1)
  f.flush();os.fsync(f.fileno())
 if public:os.chmod(temporary,0o644)
 os.link(temporary,target,follow_symlinks=False)
finally:temporary.unlink(missing_ok=True)
"""


def transfer(path, relative, *, public=False):
    private(path)
    with path.open('rb') as stream:
        remote_python(REMOTE_WRITE, (relative, digest(path), 'public' if public else 'private'), stdin=stream)


REMOTE_JOIN = """import os,pathlib,sys,hashlib,uuid
root=pathlib.Path('/home/dks/phone-platform');relative=pathlib.PurePosixPath(sys.argv[1])
if root.is_symlink() or not root.is_dir() or relative.is_absolute() or '..'in relative.parts:raise SystemExit(1)
target=root.joinpath(*relative.parts);walk=root
for part in relative.parts[:-1]:
 walk=walk/part
 if walk.is_symlink()or not walk.is_dir():raise SystemExit(1)
expected=sys.argv[2];count=int(sys.argv[3]);expected_size=int(sys.argv[4]);parts=target.with_name(target.name+'.parts')
if not 1<=count<=4096 or parts.is_symlink()or not parts.is_dir():raise SystemExit(1)
temporary=target.with_name('.'+target.name+'.join-'+uuid.uuid4().hex)
if target.exists()or target.is_symlink():raise SystemExit(1)
fd=os.open(temporary,os.O_CREAT|os.O_EXCL|os.O_WRONLY|os.O_NOFOLLOW,0o600)
try:
 h=hashlib.sha256();size=0
 with os.fdopen(fd,'wb')as output:
  for i in range(count):
   part=parts/f'{i:04d}'
   if part.is_symlink()or not part.is_file()or part.stat().st_mode&0o077:raise SystemExit(1)
   with part.open('rb')as f:
    for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk);output.write(chunk);size+=len(chunk)
  if h.hexdigest()!=expected or size!=expected_size:raise SystemExit(1)
  output.flush();os.fsync(output.fileno())
 os.link(temporary,target,follow_symlinks=False)
 for i in range(count):(parts/f'{i:04d}').unlink()
 parts.rmdir()
finally:temporary.unlink(missing_ok=True)
"""


def transfer_parallel(path, relative, *, workers=4):
    private(path)
    chunk_size=16*1024*1024
    size=path.stat().st_size
    count=(size+chunk_size-1)//chunk_size
    if not 1<=workers<=6 or count>4096:
        raise RuntimeError('parallel_transfer_limits_exceeded')
    expected=digest(path)
    exists=remote_python("""import pathlib,sys,hashlib
p=pathlib.Path('/home/dks/phone-platform')/sys.argv[1]
if p.exists()or p.is_symlink():
 if p.is_symlink()or not p.is_file()or p.stat().st_mode&0o077:raise SystemExit(1)
 h=hashlib.sha256()
 with p.open('rb')as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 if h.hexdigest()!=sys.argv[2]:raise SystemExit(1)
 print('complete')
else:print('missing')
""",(relative,expected),text=True).strip()
    if exists=='complete':return
    if exists!='missing':raise RuntimeError('parallel_transfer_state_invalid')
    def part(index):
        with path.open('rb')as stream:
            stream.seek(index*chunk_size);data=stream.read(chunk_size)
        remote_python(REMOTE_WRITE,(relative+'.parts/'+f'{index:04d}',hashlib.sha256(data).hexdigest()),input=data)
    with ThreadPoolExecutor(max_workers=workers)as executor:
        list(executor.map(part,range(count)))
    remote_python(REMOTE_JOIN,(relative,expected,count,size))


def prepare(release):
    os.umask(0o077)
    source = read_env(ROOT / '.env.platform')
    require_source(source)
    access = passwords()
    env_path = ROOT / 'secrets/production.env'
    if env_path.exists():
        config = read_env(env_path)
        if config.get('PHONE_RELEASE_TAG') != release or config.get('ENCRYPTION_KEY') != source['ENCRYPTION_KEY']:
            raise RuntimeError('production_runtime_already_prepared_differently')
    else:
        config = production_config(source, release)
        write_once(env_path, ''.join(k+'='+v+'\n' for k, v in config.items()))
    pair = read_env(ROOT / 'secrets/analyzer-pair.env')
    if set(pair) != {'MODEL_RUNNER_PASSWORD'} or not re.fullmatch(r'[A-Za-z0-9_-]{24,128}', pair['MODEL_RUNNER_PASSWORD']):
        raise RuntimeError('invalid_analyzer_pair')
    # Password is supplied by the paired file in Compose; no duplicate source token.
    hashes_path = ROOT / 'secrets/production-passwords.json'
    if not hashes_path.exists():
        write_once(hashes_path, json.dumps(make_hashes(access))+'\n')
    private(hashes_path)
    # Own source repository is staged independently by deploy-build; runtime never enters its archive.
    transfer(env_path, '.env.platform')
    transfer(ROOT / 'secrets/analyzer-pair.env', 'secrets/analyzer-pair.env')
    # ssh-keygen intentionally creates a public644 file. Stage only its PUBLIC bytes privately.
    public=(ROOT/'secrets/analyzer-ssh/id_ed25519.pub').read_text()
    if not re.fullmatch(r'ssh-ed25519 [A-Za-z0-9+/]+={0,2}(?: [^\r\n]+)?\n?',public):
        raise RuntimeError('invalid_public_analyzer_key')
    public_stage=ROOT/'secrets/production-analyzer.pub'
    if public_stage.exists():
        private(public_stage)
        if public_stage.read_text()!=public:raise RuntimeError('public_analyzer_key_changed')
    else:write_once(public_stage,public)
    # Rootless relay UID10001 must read its bind-mounted PUBLIC key; no private file is relaxed.
    transfer(public_stage, 'secrets/analyzer-ssh/id_ed25519.pub', public=True)
    # The release file contains no secret, copy through a protected local staging file.
    release_stage = ROOT / 'secrets/production-release-compose.yml'
    content = (ROOT / 'infra/compose.release.yml').read_bytes()
    if release_stage.exists():
        private(release_stage)
        if release_stage.read_bytes() != content:
            raise RuntimeError('release_compose_changed')
    else:
        write_once(release_stage, content)
    transfer(release_stage, 'infra/compose.release.yml')
    remote_compose(['config', '--quiet'])
    print(json.dumps({'production_runtime_prepared': True, 'release': release,
                      'smtp_available': False, 'access_file': 'secrets/production-access.json'}))


def volume(service, target):
    container = run(local_compose()+['ps', '-a', '-q', service], text=True).strip()
    info = json.loads(run(['docker', 'inspect', container], text=True))[0]
    candidates = [m['Name'] for m in info['Mounts'] if m['Type'] == 'volume' and m['Destination'] == target]
    if len(candidates) != 1 or not candidates[0].startswith('phone-platform_'):
        raise RuntimeError('unexpected_source_volume')
    return candidates[0]


def capture(directory, *, hold_writers=False):
    os.umask(0o077)
    source = read_env(ROOT / '.env.platform')
    require_source(source)
    if directory.is_symlink() or directory.exists() or ROOT not in directory.resolve().parents:
        raise RuntimeError('new_private_snapshot_under_project_required')
    directory.mkdir(parents=True, mode=0o700)
    os.chmod(directory, 0o700)
    running = run(local_compose()+['ps', '--services', '--status', 'running'], text=True).split()
    quiesced = [s for s in WRITERS if s in running]
    complete=False
    try:
        if quiesced:
            run(local_compose()+['stop', *quiesced])
        require_idle(json.loads(local_sql(QUEUE_SQL)))
        counts = json.loads(local_sql(COUNTS_SQL))
        auth = json.loads(local_sql(AUTH_SQL))
        preserved_digest = local_sql(PRESERVE_SQL)
        # Unknown real accounts are never silently reset; expected known seeded users only.
        accounts = json.loads(local_sql("SELECT COALESCE(json_agg(email ORDER BY email),'[]'::json) FROM users WHERE email IN ('owner@demo.local','manager@demo.local','reviewer@demo.local');"))
        if set(accounts) != set(DEMO_EMAILS):
            raise RuntimeError('expected_demo_users_missing')
        fd = os.open(directory/'database.dump', os.O_CREAT|os.O_EXCL|os.O_WRONLY, 0o600)
        with os.fdopen(fd, 'wb') as stream:
            run(local_compose()+['exec','-T','db','pg_dump','-U',source['POSTGRES_USER'],
                '-d',source['POSTGRES_DB'],'-Fc','--no-owner','--no-acl'], stdout=stream)
        for archive, service, target in [('media.tar.gz','api','/app/media'),
                                         ('assistant-auth.tar.gz','assistant-runner','/var/lib/assistant-auth')]:
            source_volume = volume(service, target)
            fd = os.open(directory/archive, os.O_CREAT|os.O_EXCL|os.O_WRONLY, 0o600)
            with os.fdopen(fd, 'wb') as stream:
                run(['docker','run','--rm','--network','none','-v',source_volume+':/source:ro',
                     BUSYBOX,'tar','-czf','-','-C','/source','.'], stdout=stream)
        manifest = {'version': 1, 'purpose': 'first-production-phone-transfer',
                    'captured_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    'sha256': {name:digest(directory/name) for name in ARCHIVES},
                    'counts':counts, 'assistant_auth_control':auth,
                    'preserved_documents_digest':preserved_digest,
                    'key_fingerprint':hashlib.sha256(source['ENCRYPTION_KEY'].encode()).hexdigest(),
                    'quiesced_services':quiesced, 'source_project':'phone-platform',
                    'writers_held_for_cutover':hold_writers}
        write_once(directory/'manifest.json', json.dumps(manifest, indent=2)+'\n')
        validate_snapshot(directory)
        complete=True
    finally:
        if quiesced and (not complete or not hold_writers):
            run(local_compose()+['start', *quiesced])
    print(json.dumps({'snapshot_created': True, 'directory': str(directory.relative_to(ROOT)),
                      'counts': counts, 'source_services_restored':[] if hold_writers else quiesced,
                      'source_writers_held':hold_writers}))


def validate_snapshot(directory):
    private(directory/'manifest.json')
    manifest = json.loads((directory/'manifest.json').read_text())
    source = read_env(ROOT/'secrets/production.env')
    require_source(source)
    if manifest.get('purpose') != 'first-production-phone-transfer' or manifest.get('source_project') != 'phone-platform':
        raise RuntimeError('not_phone_migration_snapshot')
    if manifest.get('key_fingerprint') != hashlib.sha256(source['ENCRYPTION_KEY'].encode()).hexdigest():
        raise RuntimeError('encryption_key_mismatch')
    if set(manifest.get('sha256', {})) != set(ARCHIVES):
        raise RuntimeError('incomplete_snapshot')
    for name in ARCHIVES:
        private(directory/name)
        if digest(directory/name) != manifest['sha256'][name]:
            raise RuntimeError('snapshot_checksum_mismatch')
        if name.endswith('.tar.gz'):
            with tarfile.open(directory/name) as archive:
                for member in archive.getmembers():
                    path = Path(member.name)
                    if not (member.isfile() or member.isdir()) or path.is_absolute() or '..' in path.parts or member.issym() or member.islnk():
                        raise RuntimeError('unsafe_snapshot_archive')
    return manifest


def transfer_snapshot(directory):
    manifest = validate_snapshot(directory)
    name = directory.name
    if not re.fullmatch(r'production-[A-Za-z0-9_-]{1,80}', name):
        raise RuntimeError('invalid_snapshot_name')
    for filename in (*ARCHIVES, 'manifest.json'):
        path=directory/filename;relative='secrets/migration/'+name+'/'+filename
        if path.stat().st_size>8*1024*1024:transfer_parallel(path,relative)
        else:transfer(path,relative)
    print(json.dumps({'snapshot_transferred':True,'counts':manifest['counts']}))


def credential_cleanup_sql(hashes):
    if not isinstance(hashes,list) or {v.get('email') for v in hashes} != set(DEMO_EMAILS) or len(hashes) != 3:
        raise RuntimeError('invalid_password_hash_mapping')
    for row in hashes:
        if set(row) != {'email','password_hash'} or not re.fullmatch(r'\$argon2id\$v=19\$m=19456,t=2,p=1\$[A-Za-z0-9+/]+\$[A-Za-z0-9+/]+',row['password_hash']):
            raise RuntimeError('invalid_password_hash_mapping')
    encoded = base64.b64encode(json.dumps(hashes).encode()).decode()
    return """BEGIN;
CREATE TEMP TABLE migration_password_input(encoded TEXT) ON COMMIT DROP;
COPY migration_password_input FROM STDIN;
"""+encoded+"""
\.
CREATE TEMP TABLE migration_passwords ON COMMIT DROP AS
 SELECT x.* FROM migration_password_input p CROSS JOIN LATERAL
 jsonb_to_recordset(convert_from(decode(p.encoded,'base64'),'UTF8')::jsonb)
 AS x(email TEXT,password_hash TEXT);
DO $$ BEGIN
 IF (SELECT count(*) FROM users u JOIN migration_passwords p ON p.email=u.email)<>3
 THEN RAISE EXCEPTION 'expected_demo_users_missing'; END IF;
END $$;
UPDATE users u SET password_hash=p.password_hash FROM migration_passwords p WHERE p.email=u.email;
DELETE FROM sessions;
UPDATE token_grants SET consumed_at=now() WHERE consumed_at IS NULL;
DELETE FROM documents WHERE kind='assistant_credential';
UPDATE documents SET data=(data-'capability_hash'-'session_hash') || jsonb_build_object(
 'state',CASE WHEN data->>'state' IN ('queued','running') THEN 'interrupted' ELSE data->>'state' END,
 'error_code',CASE WHEN data->>'state' IN ('queued','running') THEN 'assistant_migrated' ELSE data->>'error_code' END,
 'finished_at',CASE WHEN data->>'state' IN ('queued','running') THEN to_jsonb(now()) ELSE data->'finished_at' END),
 version=version+1,updated_at=now() WHERE kind='assistant_run';
UPDATE documents SET data=(data-'capability_hash'-'session_hash') || jsonb_build_object('state','revoked'),
 version=version+1,updated_at=now() WHERE kind='assistant_auth_attempt';
UPDATE documents SET data=data || jsonb_build_object('pending_attempt',NULL),
 version=version+1,updated_at=now() WHERE kind='assistant_auth_control';
UPDATE documents SET data=data || jsonb_build_object('active_run_id',NULL),
 version=version+1,updated_at=now() WHERE kind='assistant_conversation';
COMMIT;
"""


def require_empty_tables(value):
    if value.strip() != '0':
        raise RuntimeError('target_database_not_empty_restore_refused')


EMPTY_VOLUME_SH = 'test -z "$(find /restore -mindepth 1 -maxdepth 1 -print -quit)"'


def restore(directory):
    manifest = validate_snapshot(directory)
    hashes_path=ROOT/'secrets/production-passwords.json'
    private(hashes_path)
    sql = credential_cleanup_sql(json.loads(hashes_path.read_text()))
    if not re.fullmatch(r'production-[A-Za-z0-9_-]{1,80}', directory.name):
        raise RuntimeError('invalid_snapshot_name')
    remote_snapshot = REMOTE+'/secrets/migration/'+directory.name
    # Repeated restore must fail BEFORE any API/worker is created/started.
    remote_python("from pathlib import Path; import sys; p=Path(sys.argv[1]); raise SystemExit(1 if p.exists() or p.is_symlink() else 0)",
                  (REMOTE+'/secrets/production-migrated.json',))
    remote_compose(['config','--quiet'])
    remote_compose(['up','-d','--no-build','--wait','--wait-timeout','120','db','rabbitmq'])
    # Compose --wait requires a running/healthy container and rejects successful oneshots.
    remote_compose(['up','-d','--no-build','media-init'])
    remote_compose(['wait','media-init'])
    require_empty_tables(remote_sql("SELECT count(*) FROM pg_tables WHERE schemaname='public';"))
    for vol in ('phone-platform_media','phone-platform_assistant_auth'):
        ssh(shlex.join(['docker','volume','create',vol]))
        ssh(shlex.join(['docker','run','--rm','--network','none','-v',vol+':/restore',BUSYBOX,'sh','-c',EMPTY_VOLUME_SH]))
    # Confirm transferred archives on the server independently before reading any bytes into DB/volumes.
    remote_python("""import pathlib,hashlib,json,sys
p=pathlib.Path(sys.argv[1]); m=json.loads((p/'manifest.json').read_text())
for name,expected in m['sha256'].items():
 h=hashlib.sha256()
 with(p/name).open('rb')as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 if h.hexdigest()!=expected:raise SystemExit(1)
""", (remote_snapshot,))
    # Both sides are frozen snapshots. No DROP/clean option is ever used.
    restore_cmd=['env','-u','MODEL_RUNNER_PASSWORD','docker','compose','--env-file','.env.platform','--env-file','secrets/analyzer-pair.env',
       '-f','infra/compose.yml','-f','infra/compose.production.yml','-f','infra/compose.analyzer-remote.yml',
       '-f','infra/compose.release.yml','exec','-T','db','pg_restore','-U','phone','-d','phone_platform',
       '--no-owner','--no-acl','--exit-on-error','--single-transaction']
    ssh('cd '+shlex.quote(REMOTE)+' && '+shlex.join(restore_cmd)+' < '+shlex.quote(remote_snapshot+'/database.dump'))
    actual=json.loads(remote_sql(COUNTS_SQL))
    if actual!=manifest['counts']:
        raise RuntimeError('restored_baseline_counts_mismatch')
    if remote_sql(PRESERVE_SQL)!=manifest['preserved_documents_digest']:
        raise RuntimeError('restored_documents_digest_mismatch')
    if json.loads(remote_sql(AUTH_SQL))!=manifest['assistant_auth_control']:
        raise RuntimeError('assistant_auth_control_changed')
    sealed=json.loads(remote_sql("SELECT COALESCE(json_agg(data->>'config_encrypted'),'[]'::json) FROM documents WHERE kind='connection' AND data?'config_encrypted';"))
    decryption=verify_encrypted_connections(read_env(ROOT/'secrets/production.env'),sealed)
    require_idle(json.loads(remote_sql(QUEUE_SQL)))
    for archive, vol in [('media.tar.gz','phone-platform_media'),('assistant-auth.tar.gz','phone-platform_assistant_auth')]:
        extract=['docker','run','--rm','--network','none','-i','-v',vol+':/restore',BUSYBOX,
                 'sh','-c','tar -xzf - -C /restore && chown -R 10001:10001 /restore && chmod 700 /restore']
        ssh(shlex.join(extract)+' < '+shlex.quote(remote_snapshot+'/'+archive))
    # Validate every ready media path through bounded, non-executable stdin data.
    rows=remote_sql("SELECT json_build_object('organization_id',organization_id,'path',data->>'path') FROM documents WHERE kind='recording' AND (data->>'ready'='true' OR data->>'state'='ready');")
    paths=[]
    for raw in rows.splitlines():
        row=json.loads(raw); org=row['organization_id']; path=row['path']
        if str(uuid.UUID(org))!=org or not isinstance(path,str) or len(path)>4096 or any(ord(c)<32 or ord(c)==127 for c in path):
            raise RuntimeError('invalid_media_reference')
        relative=Path(path)
        if relative.is_absolute() or '..'in relative.parts or len(relative.parts)<2 or relative.parts[0]!=org:
            raise RuntimeError('invalid_media_reference')
        paths.append(path)
    check=['docker','run','--rm','--network','none','-i','-v','phone-platform_media:/restore:ro',BUSYBOX,
           'sh','-c','while IFS= read -r relative; do test -f "/restore/$relative" || exit 1; done']
    ssh(shlex.join(check),input=''.join(p+'\n'for p in paths),text=True)
    remote_sql(sql)
    if json.loads(remote_sql(AUTH_SQL))!=manifest['assistant_auth_control']:
        raise RuntimeError('assistant_authorization_not_preserved')
    if remote_sql(PRESERVE_SQL)!=manifest['preserved_documents_digest']:
        raise RuntimeError('persistent_call_or_connection_data_changed')
    result={'migration_restored':True,'baseline_counts':actual,'ready_media_verified':len(paths),
            'assistant_authorization_preserved':True,'web_sessions_revoked':True,
            'demo_passwords_rotated':3,'source_snapshot':directory.name,
            'source_snapshot_sha256':digest(directory/'manifest.json'),**decryption}
    stage=directory/'restore-result.json'
    write_once(stage,json.dumps(result,indent=2)+'\n')
    transfer(stage,'secrets/production-migrated.json')
    print(json.dumps(result))


def finalize(directory):
    manifest=validate_snapshot(directory)
    # Core data must not have changed on local source since the accepted snapshot.
    if local_sql(PRESERVE_SQL)!=manifest['preserved_documents_digest']:
        raise RuntimeError('source_changed_since_snapshot_cutover_refused')
    remote_compose(['exec','-T','api','phone-api','healthcheck'])
    remote_compose(['exec','-T','worker','phone-worker','healthcheck'])
    run(local_compose()+['stop',*WRITERS])
    print(json.dumps({'cutover_finalized':True,'local_writers_stopped':list(WRITERS),
                      'local_database_and_volumes_preserved':True}))


def resume_source(directory):
    manifest=validate_snapshot(directory)
    services=manifest.get('quiesced_services')
    if not isinstance(services,list) or len(set(services))!=len(services) or any(s not in WRITERS for s in services):
        raise RuntimeError('invalid_quiesced_source_services')
    active=remote_compose(['ps','--services','--status','running'],text=True).split()
    if any(s in active for s in WRITERS):
        raise RuntimeError('stop_production_writers_before_source_resume')
    if services:run(local_compose()+['start',*services])
    print(json.dumps({'source_resumed':services,'production_unchanged':True}))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=['prepare','capture','transfer','restore','finalize','resume-source'])
    parser.add_argument('--release-tag',default='15-2e894a4')
    parser.add_argument('--directory',type=Path)
    parser.add_argument('--hold-writers',action='store_true',help='Keep source API/worker/assistant stopped until production cutover; failures resume them')
    args=parser.parse_args();os.umask(0o077)
    if args.action!='prepare'and args.directory is None:parser.error('--directory is required')
    try:
        if args.action=='prepare':prepare(args.release_tag)
        else:
            directory=args.directory.absolute()
            if args.action=='capture':capture(directory,hold_writers=args.hold_writers)
            else:{'transfer':transfer_snapshot,'restore':restore,'finalize':finalize,'resume-source':resume_source}[args.action](directory)
        return 0
    except (OSError,RuntimeError,ValueError,KeyError,TypeError,subprocess.SubprocessError,tarfile.TarError) as error:
        # RuntimeError messages are static codes defined above. No subprocess diagnostics.
        code=str(error) if isinstance(error,RuntimeError) else 'production_migration_failed'
        print(json.dumps({'error':code}),file=sys.stderr)
        return 1


if __name__=='__main__':sys.exit(main())
