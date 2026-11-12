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
