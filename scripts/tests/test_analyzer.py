import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,path);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
analyzer=module('analyzer',ROOT/'scripts/analyzer.py')
client=module('analyzer_client',ROOT/'infra/analyzer-tunnel/client.py')

class AnalyzerTests(unittest.TestCase):
 def test_init_preserves_own_keys_pairing_and_pins_on_repeat(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);analyzer.initialize(root);files=[root/analyzer.PAIR,root/analyzer.SSH_DIR/'id_ed25519',root/analyzer.SSH_DIR/'known_hosts'];before=[p.read_bytes() for p in files]
   analyzer.initialize(root);self.assertEqual(before,[p.read_bytes() for p in files]);self.assertTrue(all(p.stat().st_mode&0o777==0o600 for p in files));self.assertEqual((root/analyzer.LOCAL).stat().st_mode&0o777,0o600)
 def test_configuration_rejects_injection_missing_target_and_other_projects(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);analyzer.initialize(root)
   with self.assertRaisesRegex(ValueError,'PHONE_REMOTE_SSH_HOST'):analyzer.configuration(root)
   base='PHONE_REMOTE_SSH_HOST=server.example.test\nPHONE_REMOTE_SSH_USER=phone\n'
   for extra in ['PHONE_ANALYZER_PROJECT=bondscan\n','PHONE_ANALYZER_PROJECT=phone-platform\n','PHONE_REMOTE_SSH_PORT=0\n','PHONE_REMOTE_SSH_PORT=22;touch /tmp/x\n','UNKNOWN=1\n']:
    (root/analyzer.LOCAL).write_text(base+extra)
    with self.assertRaises(ValueError):analyzer.configuration(root)
   (root/analyzer.LOCAL).write_text('PHONE_REMOTE_SSH_HOST=example.test\nProxyCommand=bad\nPHONE_REMOTE_SSH_USER=phone\n')
   with self.assertRaises(ValueError):analyzer.configuration(root)
 def test_host_key_change_is_rejected_without_overwriting_pin(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);analyzer.initialize(root);pub=root/analyzer.SSH_DIR/'id_ed25519.pub';analyzer.pin(root,'phone-model-tunnel',pub);pin=root/analyzer.SSH_DIR/'known_hosts';before=pin.read_text()
   other=root/'other';other.mkdir();analyzer.initialize(other)
   with self.assertRaisesRegex(ValueError,'Ключ хоста изменился'):analyzer.pin(root,'phone-model-tunnel',other/analyzer.SSH_DIR/'id_ed25519.pub')
   self.assertEqual(pin.read_text(),before)
 def test_secret_symlinks_and_unprotected_files_are_rejected(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);analyzer.initialize(root);pair=root/analyzer.PAIR;pair.chmod(0o644)
   with self.assertRaises(ValueError):analyzer.read_pair(root)
   pair.unlink();target=root/'target';target.write_text('secret');pair.symlink_to(target)
   with self.assertRaises(ValueError):analyzer.initialize(root)
   self.assertEqual(target.read_text(),'secret')
 def test_symlinked_secret_directory_is_never_followed(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d)/'project';root.mkdir();other=Path(d)/'other';other.mkdir();(root/'secrets').symlink_to(other,target_is_directory=True)
   with self.assertRaises(ValueError):analyzer.initialize(root)
   self.assertEqual(list(other.iterdir()),[])
 def test_client_has_strict_pinning_only_own_credentials_and_reverse_forward(self):
  config=client.render_config({'PHONE_REMOTE_SSH_HOST':'server.example.test','PHONE_REMOTE_SSH_USER':'phone'},Path('/state'))
  for required in ['StrictHostKeyChecking yes','IdentityAgent none','IdentitiesOnly yes','ProxyJump phone-jump','ExitOnForwardFailure yes','HostKeyAlias phone-model-tunnel','IdentityFile /state/id_ed25519']:
   self.assertIn(required,config)
  self.assertNotIn('accept-new',config);self.assertNotIn('~/.ssh',config);self.assertIn('ControlMaster no\n    ControlPath none\nHost phone-model-tunnel',config)
  with self.assertRaises(ValueError):client.render_config({'PHONE_REMOTE_SSH_HOST':'x\nProxyCommand bad','PHONE_REMOTE_SSH_USER':'phone'})
 def test_invalid_installed_key_preserves_previous_credentials_and_connection(self):
  import json, subprocess
  from io import BytesIO
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);analyzer.initialize(root);state=root/'state';state.mkdir();names={'STATE':state,'PRIVATE':state/'id_ed25519','KNOWN':state/'known_hosts','CONFIG':state/'ssh_config','CONTROL':state/'control'}
   pub=analyzer.public_key((root/analyzer.SSH_DIR/'id_ed25519.pub').read_text());known='phone-analyzer-jump '+pub+'\nphone-model-tunnel '+pub+'\n';valid={'private_key':(root/analyzer.SSH_DIR/'id_ed25519').read_text(),'known_hosts':known}
   class Input:
    def __init__(self,value):self.buffer=BytesIO(json.dumps(value).encode())
   env={'PHONE_REMOTE_SSH_HOST':'server.example.test','PHONE_REMOTE_SSH_USER':'phone'}
   from contextlib import ExitStack
   with ExitStack() as stack:
    for name,value in names.items():stack.enter_context(patch.object(client,name,value))
    stack.enter_context(patch.dict(client.os.environ,env));stack.enter_context(patch.object(client.sys,'stdin',Input(valid)));client.install_credentials()
    before={name:path.read_bytes() for name,path in names.items() if name not in ['STATE','CONTROL']}
    with patch.object(client.sys,'stdin',Input({**valid,'private_key':'-----BEGIN OPENSSH PRIVATE KEY-----\ninvalid\n'})):
     with self.assertRaises(subprocess.CalledProcessError):client.install_credentials()
    self.assertEqual(before,{name:path.read_bytes() for name,path in names.items() if name not in ['STATE','CONTROL']})
 def test_valid_credential_reload_closes_only_own_master(self):
  import json,subprocess
  from io import BytesIO
  from contextlib import ExitStack
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);analyzer.initialize(root);state=root/'state';state.mkdir();control=state/'control';control.write_text('fixture socket')
   pub=analyzer.public_key((root/analyzer.SSH_DIR/'id_ed25519.pub').read_text());value={'private_key':(root/analyzer.SSH_DIR/'id_ed25519').read_text(),'known_hosts':'phone-analyzer-jump '+pub+'\nphone-model-tunnel '+pub+'\n'}
   class Input:buffer=BytesIO(json.dumps(value).encode())
   real_run=subprocess.run;calls=[]
   def execute(argv,**kwargs):
    if argv[0]=='ssh':calls.append(argv);return subprocess.CompletedProcess(argv,0)
    return real_run(argv,**kwargs)
   with ExitStack() as stack:
    for name,path in {'STATE':state,'PRIVATE':state/'id_ed25519','KNOWN':state/'known_hosts','CONFIG':state/'ssh_config','CONTROL':control}.items():stack.enter_context(patch.object(client,name,path))
    stack.enter_context(patch.dict(client.os.environ,{'PHONE_REMOTE_SSH_HOST':'server.example.test','PHONE_REMOTE_SSH_USER':'phone'}));stack.enter_context(patch.object(client.sys,'stdin',Input()));stack.enter_context(patch.object(client.subprocess,'run',execute));client.install_credentials()
   self.assertEqual(calls,[['ssh','-F',str(state/'ssh_config'),'-S',str(control),'-O','exit','phone-model-tunnel']])
 def test_pairing_file_overrides_an_inherited_unrelated_runner_password(self):
  import subprocess
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);analyzer.initialize(root);expected=analyzer.read_pair(root)
   with patch.dict(analyzer.os.environ,{'MODEL_RUNNER_PASSWORD':'wrong-inherited-project-password'}):
    with patch.object(analyzer.subprocess,'run',return_value=subprocess.CompletedProcess(['docker'],0)) as called:
     analyzer.execute(['docker','version'],root=root,config={'PHONE_ANALYZER_PROJECT':'phone-analyzer-test'})
   self.assertEqual(called.call_args.kwargs['env']['MODEL_RUNNER_PASSWORD'],expected)
 def test_down_argv_is_dedicated_project_and_never_global_down(self):
  config={'PHONE_ANALYZER_PROJECT':'phone-analyzer-test'};cmd=analyzer.compose(ROOT,config)
  self.assertEqual(cmd[cmd.index('--project-name')+1],'phone-analyzer-test');self.assertIn(str(ROOT/'infra/compose.analyzer-local.yml'),cmd)

if __name__=='__main__':unittest.main()
