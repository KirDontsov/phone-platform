"""Migration safety boundaries; disposable fixtures only, never production credentials."""
import base64
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import uuid

SOURCE=Path(__file__).resolve().parents[1]/'production-migrate.py'
spec=importlib.util.spec_from_file_location('phone_production_migration',SOURCE)
migration=importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)


class MigrationSafetyTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve()
        self.patch=mock.patch.object(migration,'ROOT',self.root);self.patch.start();self.addCleanup(self.patch.stop)
        self.source={'COMPOSE_PROJECT_NAME':'phone-platform','ENCRYPTION_KEY':base64.b64encode(b'f'*32).decode(),
                     'POSTGRES_USER':'phone','POSTGRES_DB':'phone_fixture'}
        (self.root/'secrets').mkdir(mode=0o700)
        migration.write_once(self.root/'secrets/production.env',''.join(k+'='+v+'\n'for k,v in self.source.items()))
        self.snapshot=self.root/'data/backups/production-fixture';self.snapshot.mkdir(parents=True,mode=0o700)
        migration.write_once(self.snapshot/'database.dump',b'fixture database')
        self.archive('media.tar.gz');self.archive('assistant-auth.tar.gz')
        self.manifest()
        self.hashes=[{'email':e,'password_hash':'$argon2id$v=19$m=19456,t=2,p=1$YWJj$ZGVm'}for e in migration.DEMO_EMAILS]
        migration.write_once(self.root/'secrets/production-passwords.json',json.dumps(self.hashes))

    def archive(self,name,member=None):
        with tarfile.open(self.snapshot/name,'w:gz')as archive:
            if member is not None:archive.addfile(member,io.BytesIO(b'x')if member.isfile()else None)
        (self.snapshot/name).chmod(0o600)

    def manifest(self,**changes):
        result={'version':1,'purpose':'first-production-phone-transfer','source_project':'phone-platform',
                'sha256':{n:migration.digest(self.snapshot/n)for n in migration.ARCHIVES},
                'key_fingerprint':hashlib.sha256(self.source['ENCRYPTION_KEY'].encode()).hexdigest(),
                'counts':{'organizations':2},'assistant_auth_control':[],'preserved_documents_digest':'fixture'}
        result.update(changes)
        (self.snapshot/'manifest.json').write_text(json.dumps(result));(self.snapshot/'manifest.json').chmod(0o600)
        return result

    def test_runtime_preserves_only_needed_source_encryption_and_rotates_secrets(self):
        source={**self.source,'BOOTSTRAP_PASSWORD':'public-demo','ASSISTANT_RUNNER_TOKEN':'old-token',
                'POSTGRES_PASSWORD':'old-db','SMTP_HOST':'foreign-mail','ALLOWED_ORIGINS':'http://localhost'}
        result=migration.production_config(source,'15-abcdef0')
        self.assertEqual(result['ENCRYPTION_KEY'],source['ENCRYPTION_KEY'])
        self.assertNotIn('BOOTSTRAP_PASSWORD',result)
        self.assertNotEqual(result['ASSISTANT_RUNNER_TOKEN'],source['ASSISTANT_RUNNER_TOKEN'])
        self.assertNotEqual(result['POSTGRES_PASSWORD'],source['POSTGRES_PASSWORD'])
        self.assertEqual(result['DEMO_MODE'],'false');self.assertEqual(result['COOKIE_SECURE'],'true')
        self.assertEqual(result['ALLOWED_ORIGINS'],'https://phone.bondscan.ru')
        self.assertEqual(result['SMTP_HOST'],'127.0.0.1');self.assertEqual(result['SMTP_PORT'],'1')

    def test_other_source_project_rejected(self):
        with self.assertRaisesRegex(RuntimeError,'source_project_not_phone'):
            migration.production_config({**self.source,'COMPOSE_PROJECT_NAME':'other'},'15-abcdef0')

    def test_pending_jobs_outbox_or_assistant_prevent_capture(self):
        migration.require_idle({'jobs':0,'outbox':0,'assistant_runs':0})
        for field in ('jobs','outbox','assistant_runs'):
            with self.subTest(field=field),self.assertRaisesRegex(RuntimeError,'pending_work'):
                migration.require_idle({k:int(k==field)for k in ('jobs','outbox','assistant_runs')})

    def test_private_file_does_not_follow_links_or_accept_world_read(self):
        env=self.root/'secrets/production.env';env.chmod(0o644)
        with self.assertRaisesRegex(RuntimeError,'private_regular'):migration.read_env(env)
        env.chmod(0o600);link=self.root/'secrets/link';link.symlink_to(env)
        with self.assertRaisesRegex(RuntimeError,'private_regular'):migration.read_env(link)

    def test_tampered_dump_rejected_before_remote_restore(self):
        (self.snapshot/'database.dump').write_bytes(b'corrupt')
        with mock.patch.object(migration,'ssh',side_effect=AssertionError('SSH must not run'))as ssh:
            with self.assertRaisesRegex(RuntimeError,'checksum_mismatch'):migration.restore(self.snapshot)
            ssh.assert_not_called()

    def test_wrong_encryption_key_rejected_before_remote_restore(self):
        self.manifest(key_fingerprint='0'*64)
        with mock.patch.object(migration,'remote_compose',side_effect=AssertionError('Docker must not run'))as compose:
            with self.assertRaisesRegex(RuntimeError,'encryption_key_mismatch'):migration.restore(self.snapshot)
            compose.assert_not_called()

    def test_all_three_archive_checksums_required(self):
        self.manifest(sha256={'database.dump':migration.digest(self.snapshot/'database.dump')})
        with self.assertRaisesRegex(RuntimeError,'incomplete_snapshot'):migration.validate_snapshot(self.snapshot)

    def test_dangerous_archive_members_rejected(self):
        for name,kind in [('../escape',tarfile.REGTYPE),('/absolute',tarfile.REGTYPE),('link',tarfile.SYMTYPE),('fifo',tarfile.FIFOTYPE)]:
            with self.subTest(name=name):
                member=tarfile.TarInfo(name);member.type=kind;member.size=1 if kind==tarfile.REGTYPE else 0
                self.archive('media.tar.gz',member);self.manifest()
                with self.assertRaisesRegex(RuntimeError,'unsafe_snapshot_archive'):migration.validate_snapshot(self.snapshot)

    def test_nonempty_target_refused_before_pg_restore_or_app_start(self):
        commands=[]
        with mock.patch.object(migration,'remote_python'),mock.patch.object(migration,'remote_compose',side_effect=lambda argv,**kw:commands.append(argv)),mock.patch.object(migration,'remote_sql',return_value='5'),mock.patch.object(migration,'ssh',side_effect=AssertionError('must not restore')):
            with self.assertRaisesRegex(RuntimeError,'target_database_not_empty'):migration.restore(self.snapshot)
        self.assertEqual(commands,[['config','--quiet'],['up','-d','--no-build','--wait','--wait-timeout','120','db','rabbitmq'],
                                   ['up','-d','--no-build','media-init'],['wait','media-init']])

    def test_password_mapping_cannot_target_arbitrary_accounts(self):
        self.hashes[0]['email']='someone@real.example'
        with self.assertRaisesRegex(RuntimeError,'invalid_password_hash_mapping'):migration.credential_cleanup_sql(self.hashes)

    def test_existing_private_access_is_preserved(self):
        first=migration.passwords();second=migration.passwords()
        self.assertEqual(first,second);self.assertEqual(len(set(first['accounts'].values())),3)
        self.assertEqual((self.root/'secrets/production-access.json').stat().st_mode&0o777,0o600)

    def test_connection_decrypt_checks_real_gcm_tag_and_outputs_only_count(self):
        fixture="""const c=require('node:crypto');const key=Buffer.alloc(32,102),nonce=Buffer.alloc(12,1);
const cipher=c.createCipheriv('aes-256-gcm',key,nonce);
const body=Buffer.concat([cipher.update(JSON.stringify({app_secret:'synthetic-fixture-secret'})),cipher.final()]);
process.stdout.write('v1.'+Buffer.concat([nonce,body,cipher.getAuthTag()]).toString('base64'));"""
        sealed=migration.run(['node','-e',fixture],text=True)
        result=migration.verify_encrypted_connections(self.source,[sealed])
        self.assertEqual(result,{'decrypted_connections':1})
        wrong={**self.source,'ENCRYPTION_KEY':base64.b64encode(b'g'*32).decode()}
        with self.assertRaisesRegex(RuntimeError,'migration_command_failed'):
            migration.verify_encrypted_connections(wrong,[sealed])

    def capture_with_fake_docker(self,hold,fail=False):
        directory=self.root/'data/backups/production-capture'
        migration.write_once(self.root/'.env.platform',''.join(k+'='+v+'\n'for k,v in self.source.items()))
        calls=[]
        def run(argv,**kwargs):
            calls.append(argv)
            if '--services'in argv:return 'api worker assistant-runner'
            if kwargs.get('stdout')is not None and kwargs['stdout']is not subprocess.PIPE:
                kwargs['stdout'].write(b'synthetic archive')
            return b''
        replies=[json.dumps({'jobs':int(fail),'outbox':0,'assistant_runs':0}),
                 json.dumps({'calls':2}),'[]','fixture-digest',json.dumps(list(migration.DEMO_EMAILS))]
        with mock.patch.object(migration,'run',side_effect=run),mock.patch.object(migration,'local_sql',side_effect=replies),mock.patch.object(migration,'volume',return_value='phone-platform_fixture'),mock.patch.object(migration,'validate_snapshot'),mock.patch('sys.stdout',new_callable=io.StringIO):
            if fail:
                with self.assertRaisesRegex(RuntimeError,'pending_work'):migration.capture(directory,hold_writers=hold)
            else:migration.capture(directory,hold_writers=hold)
        return calls,directory

    def test_capture_hold_keeps_writers_stopped_and_records_resume_set(self):
        calls,directory=self.capture_with_fake_docker(True)
        self.assertTrue(any('stop'in c for c in calls));self.assertFalse(any('start'in c for c in calls))
        manifest=json.loads((directory/'manifest.json').read_text())
        self.assertTrue(manifest['writers_held_for_cutover'])
        self.assertEqual(manifest['quiesced_services'],list(migration.WRITERS))

    def test_failed_capture_hold_resumes_original_writers(self):
        calls,_=self.capture_with_fake_docker(True,fail=True)
        self.assertEqual([c[c.index('start')+1:]for c in calls if 'start'in c],[list(migration.WRITERS)])

    def test_interrupted_upload_never_publishes_partial_and_retry_preserves_existing(self):
        code=migration.REMOTE_WRITE.replace("'/home/dks/phone-platform'",repr(str(self.root)))
        complete=b'complete synthetic bytes';expected=hashlib.sha256(complete).hexdigest()
        def upload(payload,checksum):
            return subprocess.run([sys.executable,'-c',code,'upload.bin',checksum],input=payload,capture_output=True)
        self.assertNotEqual(upload(b'partial',expected).returncode,0)
        self.assertFalse((self.root/'upload.bin').exists())
        self.assertFalse(list(self.root.glob('.upload.bin.upload-*')))
        self.assertEqual(upload(complete,expected).returncode,0)
        self.assertEqual((self.root/'upload.bin').read_bytes(),complete)
        replacement=b'replacement';self.assertNotEqual(upload(replacement,hashlib.sha256(replacement).hexdigest()).returncode,0)
        self.assertEqual((self.root/'upload.bin').read_bytes(),complete)

    def test_parallel_chunks_published_only_after_full_checksum_then_removed(self):
        code=migration.REMOTE_JOIN.replace("'/home/dks/phone-platform'",repr(str(self.root)))
        parts=self.root/'joined.bin.parts';parts.mkdir(mode=0o700)
        for index,value in enumerate((b'first',b'second')):migration.write_once(parts/f'{index:04d}',value)
        def join(expected):return subprocess.run([sys.executable,'-c',code,'joined.bin',expected,'2','11'],capture_output=True)
        self.assertNotEqual(join('0'*64).returncode,0)
        self.assertFalse((self.root/'joined.bin').exists());self.assertTrue(parts.exists())
        self.assertEqual(join(hashlib.sha256(b'firstsecond').hexdigest()).returncode,0)
        self.assertEqual((self.root/'joined.bin').read_bytes(),b'firstsecond');self.assertFalse(parts.exists())

    def test_source_resume_requires_production_writers_stopped(self):
        self.manifest(quiesced_services=list(migration.WRITERS))
        with mock.patch.object(migration,'remote_compose',return_value='worker\n'),mock.patch.object(migration,'run')as run:
            with self.assertRaisesRegex(RuntimeError,'stop_production_writers'):migration.resume_source(self.snapshot)
            run.assert_not_called()
        with mock.patch.object(migration,'remote_compose',return_value='db\nrabbitmq\n'),mock.patch.object(migration,'run')as run,mock.patch('sys.stdout',new_callable=io.StringIO):
            migration.resume_source(self.snapshot)
            self.assertEqual(run.call_args.args[0][-4:],['start',*migration.WRITERS])

    def test_public_key_permissions_allow_rootless_read_only_for_exact_public_path(self):
        code=migration.REMOTE_WRITE.replace("'/home/dks/phone-platform'",repr(str(self.root)))
        content=b'ssh-ed25519 synthetic-fixture-public-key\n';checksum=hashlib.sha256(content).hexdigest()
        path='secrets/analyzer-ssh/id_ed25519.pub'
        argv=[sys.executable,'-c',code,path,checksum,'public']
        self.assertEqual(subprocess.run(argv,input=content,capture_output=True).returncode,0)
        self.assertEqual((self.root/path).stat().st_mode&0o777,0o644)
        self.assertEqual(subprocess.run(argv,input=content,capture_output=True).returncode,0)
        self.assertNotEqual(subprocess.run([sys.executable,'-c',code,'secrets/private.env',checksum,'public'],input=content,capture_output=True).returncode,0)
        self.assertFalse((self.root/'secrets/private.env').exists())

    def test_production_tmpfs_modes_are_single_valid_mount_specs(self):
        # Parse with actual Compose, without any project runtime env or provider credentials.
        runtime=self.root/'empty.env';migration.write_once(runtime,'')
        config=SOURCE.parents[1]/'infra/compose.production.yml'
        result=subprocess.run(['docker','compose','--env-file',str(runtime),'-f',str(config),
                              'config','--no-consistency','--format','json'],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,'fixture Compose parse failed')
        services=json.loads(result.stdout)['services']
        self.assertEqual(services['api']['tmpfs'],['/tmp:size=64m,mode=1777'])
        self.assertEqual(services['worker']['tmpfs'],['/tmp:size=512m,mode=1777'])


@unittest.skipUnless(os.environ.get('PHONE_MIGRATION_TEST_DATABASE'),'explicit disposable PostgreSQL fixture required')
class CleanupPostgresTests(unittest.TestCase):
    def setUp(self):
        self.database=os.environ['PHONE_MIGRATION_TEST_DATABASE']
        if not __import__('re').fullmatch(r'phone_migration_test_[a-f0-9]{8,32}',self.database):
            self.fail('disposable database name required')
        self.db=['docker','exec','-i','phone-platform-db-1','psql','-X','-q','-A','-t','-U','phone','-d',self.database,'-v','ON_ERROR_STOP=1']

    def sql(self,value):
        result=subprocess.run(self.db,input=value,text=True,capture_output=True)
        self.assertEqual(result.returncode,0,'fixture SQL failed')
        return result.stdout.strip()

    def test_cleanup_revokes_capabilities_but_preserves_confirmed_codex_and_connections(self):
        self.sql("""CREATE TABLE users(email TEXT PRIMARY KEY,password_hash TEXT);
CREATE TABLE sessions(id TEXT); CREATE TABLE token_grants(consumed_at TIMESTAMPTZ);
CREATE TABLE documents(id UUID PRIMARY KEY,kind TEXT,data JSONB,version BIGINT DEFAULT 1,updated_at TIMESTAMPTZ DEFAULT now());
INSERT INTO users VALUES('owner@demo.local','old'),('manager@demo.local','old'),('reviewer@demo.local','old'),('real@fixture.test','preserve');
INSERT INTO sessions VALUES('fixture');INSERT INTO token_grants VALUES(NULL);
INSERT INTO documents(id,kind,data) VALUES
('00000000-0000-0000-0000-000000000001','assistant_auth_control','{"authenticated":true,"generation":100,"confirmed_generation":100,"pending_attempt":"fixture"}'),
('00000000-0000-0000-0000-000000000002','assistant_run','{"state":"completed","capability_hash":"fixture","session_hash":"fixture"}'),
('00000000-0000-0000-0000-000000000003','assistant_run','{"state":"running","capability_hash":"fixture"}'),
('00000000-0000-0000-0000-000000000004','assistant_credential','{"encrypted":"fixture"}'),
('00000000-0000-0000-0000-000000000005','connection','{"kind":"telphin","config_encrypted":"fixture-ciphertext"}');""")
        fixture=[{'email':email,'password_hash':'$argon2id$v=19$m=19456,t=2,p=1$YWJj$ZGVm'}for email in migration.DEMO_EMAILS]
        self.sql(migration.credential_cleanup_sql(fixture))
        result=json.loads(self.sql("""SELECT json_build_object(
'auth',(SELECT data FROM documents WHERE kind='assistant_auth_control'),
'connection',(SELECT data FROM documents WHERE kind='connection'),
'credential_count',(SELECT count(*) FROM documents WHERE kind='assistant_credential'),
'sessions',(SELECT count(*) FROM sessions),
'open_grants',(SELECT count(*) FROM token_grants WHERE consumed_at IS NULL),
'caps',(SELECT count(*) FROM documents WHERE data?'capability_hash' OR data?'session_hash'),
'completed',(SELECT data->>'state' FROM documents WHERE id='00000000-0000-0000-0000-000000000002'),
'interrupted',(SELECT data->>'state' FROM documents WHERE id='00000000-0000-0000-0000-000000000003'),
'other_password',(SELECT password_hash FROM users WHERE email='real@fixture.test'))"""))
        self.assertEqual(result['auth'],{'authenticated':True,'generation':100,'confirmed_generation':100,'pending_attempt':None})
        self.assertEqual(result['connection'],{'kind':'telphin','config_encrypted':'fixture-ciphertext'})
        for key in ('credential_count','sessions','open_grants','caps'):self.assertEqual(result[key],0)
        self.assertEqual(result['completed'],'completed');self.assertEqual(result['interrupted'],'interrupted')
        self.assertEqual(result['other_password'],'preserve')


if __name__=='__main__':unittest.main()
