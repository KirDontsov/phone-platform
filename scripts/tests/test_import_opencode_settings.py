import importlib.util,json,tempfile,unittest
from pathlib import Path
MODULE=Path(__file__).resolve().parents[1]/'import-opencode-settings.py'
spec=importlib.util.spec_from_file_location('phone_import_opencode',MODULE);subject=importlib.util.module_from_spec(spec);spec.loader.exec_module(subject)
ORG='11111111-1111-4111-8111-111111111111'
class ImportOpenCodeTests(unittest.TestCase):
 def source(self,root,base='https://api.provider.example/v1'):
  p=root/'opencode.json';p.write_text(json.dumps({'mcp':{'forbidden':{'command':'shell'}},'plugin':['forbidden'],'instructions':['secret instructions'],'provider':{'existing':{'npm':'malicious-package','name':'Existing','options':{'baseURL':base,'apiKey':'synthetic-existing-key','headers':{'forbidden':'secret'}},'models':{'model':{'name':'Existing model','extra':'ignored'}}},'unconfigured':{'options':{'baseURL':base},'models':{}}}}));return p
 def test_reuses_only_existing_provider_credentials_without_host_tools_or_plugins(self):
  with tempfile.TemporaryDirectory()as t:
   settings=subject.build_settings(self.source(Path(t)),ORG)
   self.assertEqual(set(settings),{'version','organization_id','providers'});self.assertEqual(list(settings['providers']),['existing']);provider=settings['providers']['existing'];self.assertEqual(provider['npm'],'@ai-sdk/openai-compatible');self.assertEqual(provider['options']['apiKey'],'synthetic-existing-key');self.assertEqual(set(provider['options']),{'baseURL','apiKey'});self.assertEqual(set(provider['models']['model']),{'name'})
 def test_local_endpoint_is_ported_to_docker_host_without_hardcoded_key(self):
  with tempfile.TemporaryDirectory()as t:
   settings=subject.build_settings(self.source(Path(t),'http://localhost:12345/v1'),ORG);self.assertEqual(settings['providers']['existing']['options']['baseURL'],'http://host.docker.internal:12345/v1')
 def test_credentials_in_url_or_unsafe_http_destination_are_rejected(self):
  for base in ['https://user:pass@provider.example/v1','https://provider.example/v1?token=secret','http://other.example/v1']:
   with tempfile.TemporaryDirectory()as t:
    with self.assertRaises(ValueError):subject.build_settings(self.source(Path(t),base),ORG)
 def test_invalid_tenant_is_rejected(self):
  with tempfile.TemporaryDirectory()as t:
   with self.assertRaises(ValueError):subject.build_settings(self.source(Path(t)),'not-a-tenant')
if __name__=='__main__':unittest.main()

class RouterAIImportTests(unittest.TestCase):
 def test_existing_routerai_credential_is_reused_and_original_speech_models_are_added(self):
  with tempfile.TemporaryDirectory()as t:
   p=Path(t)/'opencode.json';p.write_text(json.dumps({'provider':{'routerai':{'name':'RouterAI','options':{'baseURL':'https://routerai.ru/api/v1'},'models':{'text/model':{'name':'Text'}}},'other':{'options':{'baseURL':'https://other.example/v1','apiKey':'synthetic-other-key'},'models':{}}}}))
   auth=Path(t)/'auth.json';auth.write_text(json.dumps({'routerai':{'type':'api','key':'synthetic-existing-routerai-key'},'other':{'type':'api','key':'synthetic-other-key'}}))
   credentials=subject.read_existing_credentials(auth,{'routerai'});settings=subject.build_settings(p,ORG,provider_ids={'routerai'},credentials=credentials)
   self.assertEqual(list(settings['providers']),['routerai']);self.assertEqual(settings['providers']['routerai']['options']['apiKey'],'synthetic-existing-routerai-key');self.assertTrue(set(subject.ROUTERAI_ASR).issubset(settings['providers']['routerai']['models']));self.assertEqual(set(credentials),{'routerai'})
 def test_routerai_credential_cannot_be_sent_to_an_overridden_host(self):
  with tempfile.TemporaryDirectory()as t:
   p=Path(t)/'opencode.json';p.write_text(json.dumps({'provider':{'routerai':{'options':{'baseURL':'https://other.example/v1'},'models':{}}}}))
   with self.assertRaises(ValueError):subject.build_settings(p,ORG,provider_ids={'routerai'},credentials={'routerai':'synthetic-key'})

class RuntimeCommitTests(unittest.TestCase):
 def test_failed_install_preserves_host_file_and_rolls_back_current_volume_snapshot(self):
  from unittest import mock
  import sys,io,contextlib
  with tempfile.TemporaryDirectory()as t:
   root=Path(t);directory=root/'secrets';directory.mkdir(mode=0o700);target=directory/'local-opencode.json'
   original={'version':1,'organization_id':ORG,'providers':{'kept':{'name':'Kept','options':{'baseURL':'https://kept.example/v1','apiKey':'synthetic-kept-key'},'models':{}}}}
   target.write_text(json.dumps(original));target.chmod(0o600)
   source=ImportOpenCodeTests().source(root)
   calls=[]
   def installer(value,project):
    calls.append(json.loads(json.dumps(value)))
    if len(calls)==1:raise RuntimeError('synthetic installer uncertainty')
   args=['import-opencode-settings.py','--source',str(source),'--organization',ORG,'--install']
   with mock.patch.object(subject,'ROOT',root),mock.patch.object(sys,'argv',args),mock.patch.object(subject,'install',installer),contextlib.redirect_stdout(io.StringIO()):
    with self.assertRaises(RuntimeError):subject.main()
   self.assertEqual(json.loads(target.read_text()),original);self.assertEqual(calls[1],original);self.assertIn('existing',calls[0]['providers']);self.assertEqual(target.stat().st_mode&0o777,0o600)
 def test_reimporting_global_providers_keeps_private_routerai_authorization(self):
  from unittest import mock
  import sys,io,contextlib
  with tempfile.TemporaryDirectory()as t:
   root=Path(t);directory=root/'secrets';directory.mkdir(mode=0o700);target=directory/'local-opencode.json'
   target.write_text(json.dumps({'version':1,'organization_id':ORG,'providers':{'routerai':{'options':{'baseURL':'https://routerai.ru/api/v1','apiKey':'synthetic-private-routerai-key'},'models':{}}}}));target.chmod(0o600)
   source=ImportOpenCodeTests().source(root);out=io.StringIO()
   with mock.patch.object(subject,'ROOT',root),mock.patch.object(sys,'argv',['import-opencode-settings.py','--source',str(source),'--organization',ORG]),contextlib.redirect_stdout(out):subject.main()
   self.assertIn('routerai',json.loads(target.read_text())['providers']);self.assertNotIn('synthetic-private-routerai-key',out.getvalue())
