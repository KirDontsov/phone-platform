#!/usr/bin/env python3
"""Real isolated Docker SSH forwarding. No existing server, credential or call is used."""
import argparse
import base64
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import time
import uuid

ROOT=Path(__file__).resolve().parent.parent

def run(args,*,input=None,check=True,timeout=120):
 result=subprocess.run(args,input=input,text=True,capture_output=True,cwd=ROOT,timeout=timeout)
 if check and result.returncode:raise RuntimeError('fixture_command_failed:'+args[0]+':'+str(result.returncode))
 return result

def wait_for(test,timeout=40):
 deadline=time.monotonic()+timeout
 while time.monotonic()<deadline:
  if test():return
  time.sleep(.5)
 raise RuntimeError('fixture_readiness_timeout')

def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--runner-image',default='phone-platform-model-runner:opencode-1.18.10');parser.add_argument('--require-asr',action='store_true');parser.add_argument('--report',type=Path,default=ROOT/'data/eval/phase14-tunnel-smoke.json');args=parser.parse_args()
 prefix='phone14-tunnel-'+uuid.uuid4().hex[:9];local=prefix+'-local';remote=prefix+'-remote';names=[];volumes=[]
 password=secrets.token_urlsafe(36)
 with tempfile.TemporaryDirectory(prefix='phone-tunnel-fixture-') as directory:
  fixture=Path(directory);key=fixture/'id';run(['ssh-keygen','-q','-t','ed25519','-N','','-C','phone-analyzer-fixture','-f',str(key)]);key.with_suffix('.pub').chmod(0o644)
  config=fixture/'jump_config';config.write_text('''Port 2223
ListenAddress 0.0.0.0
HostKey /var/lib/phone-tunnel/ssh_host_ed25519_key
PidFile /tmp/jump-sshd.pid
AuthorizedKeysFile /var/lib/phone-tunnel/authorized_keys
UsePAM no
PasswordAuthentication no
KbdInteractiveAuthentication no
AuthenticationMethods publickey
PermitRootLogin no
AllowUsers phonebridge
PermitTTY no
AllowAgentForwarding no
AllowTcpForwarding local
PermitOpen 127.0.0.1:2222
GatewayPorts no
ForceCommand /bin/false
LogLevel ERROR
''');config.chmod(0o644)
  common=['--read-only','--user','10001:10001','--cap-drop','ALL','--security-opt','no-new-privileges:true','--pids-limit','64','--memory','128m','--tmpfs','/tmp:rw,noexec,nosuid,nodev,size=32m,mode=1777']
  def volume(label):
   name=prefix+'-'+label;run(['docker','volume','create',name]);volumes.append(name);return name
  def start(name,extra,entry):
   names.append(name);return run(['docker','run','-d','--name',name,*common,*extra,'--entrypoint','python3','phone-analyzer-tunnel:phase14',*entry]).stdout.strip()
  try:
   run(['docker','network','create',local]);run(['docker','network','create',remote])
   jump=prefix+'-jump';relay=prefix+'-relay';client=prefix+'-client';model=prefix+'-model'
   jump_state=volume('jump');relay_state=volume('relay');client_state=volume('client')
   start(jump,['--network',local,'--network-alias','phone-jump','-v',f'{jump_state}:/var/lib/phone-tunnel','-v',f'{key.with_suffix(".pub")}:/config/client_key.pub:ro','-v',f'{config}:/fixture/jump_config:ro'],['-c',"import sys,os;sys.path.insert(0,'/opt/phone-tunnel');import server;server.prepare();os.execv('/usr/sbin/sshd',['/usr/sbin/sshd','-D','-e','-f','/fixture/jump_config'])"])
   run(['docker','network','connect','--alias','model-runner',remote,jump])
   start(relay,['--network','container:'+jump,'-v',f'{relay_state}:/var/lib/phone-tunnel','-v',f'{key.with_suffix(".pub")}:/config/client_key.pub:ro','-e','MODEL_RUNNER_PASSWORD='+password],['/opt/phone-tunnel/server.py','serve'])
   names.append(model)
   run(['docker','run','-d','--name',model,'--network',local,'--network-alias','model-runner','--read-only','--user','10001:10001','--cap-drop','ALL','--security-opt','no-new-privileges:true','--pids-limit','96','--memory','768m','--tmpfs','/tmp:rw,noexec,nosuid,nodev,size=256m,mode=1777','--tmpfs','/work:rw,noexec,nosuid,nodev,size=32m,uid=10001,gid=10001,mode=0700','-e','OPENCODE_SERVER_PASSWORD='+password,args.runner_image])
   wait_for(lambda:run(['docker','exec',jump,'test','-f','/var/lib/phone-tunnel/ssh_host_ed25519_key.pub'],check=False).returncode==0)
   wait_for(lambda:run(['docker','exec',relay,'test','-f','/var/lib/phone-tunnel/ssh_host_ed25519_key.pub'],check=False).returncode==0)
   jump_pub=run(['docker','exec',jump,'cat','/var/lib/phone-tunnel/ssh_host_ed25519_key.pub']).stdout.split();relay_pub=run(['docker','exec',relay,'cat','/var/lib/phone-tunnel/ssh_host_ed25519_key.pub']).stdout.split()
   known='phone-analyzer-jump '+' '.join(jump_pub[:2])+'\nphone-model-tunnel '+' '.join(relay_pub[:2])+'\n'
   start(client,['--network',local,'-v',f'{client_state}:/var/lib/phone-tunnel','-e','PHONE_REMOTE_SSH_HOST=phone-jump','-e','PHONE_REMOTE_SSH_USER=phonebridge','-e','PHONE_REMOTE_SSH_PORT=2223','-e','PHONE_REMOTE_TUNNEL_SSH_PORT=2222'],['/opt/phone-tunnel/client.py','serve'])
   payload=json.dumps({'private_key':key.read_text(),'known_hosts':known})
   run(['docker','exec','-i',client,'python3','/opt/phone-tunnel/client.py','install-credentials'],input=payload)
   wait_for(lambda:run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0,timeout=70)
   # This process lives on the REMOTE Docker network, unlike the SSH client/model runner.
   probe="""import urllib.request,urllib.error,json,os,base64
password=os.environ['MODEL_RUNNER_PASSWORD'];headers={'Authorization':'Basic '+base64.b64encode(('opencode:'+password).encode()).decode()}
with urllib.request.urlopen(urllib.request.Request('http://model-runner:4096/global/health',headers=headers),timeout=8) as r:
 data=json.load(r);assert data['healthy'] is True and data['version']=='1.18.10'
try:urllib.request.urlopen('http://model-runner:4096/global/health',timeout=8);raise AssertionError('anonymous_allowed')
except urllib.error.HTTPError as e:assert e.code==401
if os.environ.get('REQUIRE_ASR')=='1':
 with urllib.request.urlopen(urllib.request.Request('http://model-runner:4096/asr/models',headers=headers),timeout=8) as r:
  data=json.load(r);ids={row['id'] for row in data['models']};assert ids=={'moondream/parakeet-ultra','nvidia/nemotron-3.5-transcribe'}
print('{"remote_worker_http_roundtrip":true,"anonymous_rejected":true}')
"""
   worker=run(['docker','run','--rm','--network',remote,'--user','10001:10001','--read-only','--cap-drop','ALL','-e','MODEL_RUNNER_PASSWORD='+password,'-e','REQUIRE_ASR='+('1' if args.require_asr else '0'),'--entrypoint','python3','phone-analyzer-tunnel:phase14','-c',probe])
   # The server refuses arbitrary command sessions; only forced health is available.
   denied=run(['docker','exec',client,'ssh','-F','/var/lib/phone-tunnel/ssh_config','phone-model-tunnel','id'],check=False)
   if denied.returncode==0:raise RuntimeError('arbitrary_remote_command_allowed')
   # Restart the relay. The local SSH process must reconnect without changing keys/config.
   run(['docker','restart',relay]);wait_for(lambda:run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0,timeout=80)
   # A changed relay host key fails closed. Retain credentials but remove the stale control connection first.
   wrong_known='phone-analyzer-jump '+' '.join(jump_pub[:2])+'\nphone-model-tunnel '+' '.join(jump_pub[:2])+'\n'
   run(['docker','exec','-i',client,'python3','/opt/phone-tunnel/client.py','install-credentials'],input=json.dumps({'private_key':key.read_text(),'known_hosts':wrong_known}))
   time.sleep(4)
   if run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0:raise RuntimeError('changed_relay_hostkey_accepted')
   run(['docker','exec','-i',client,'python3','/opt/phone-tunnel/client.py','install-credentials'],input=payload)
   wait_for(lambda:run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0,timeout=50)
   # A different client key is rejected, then the original can reconnect.
   wrong=fixture/'wrong';run(['ssh-keygen','-q','-t','ed25519','-N','','-f',str(wrong)])
   run(['docker','exec','-i',client,'python3','/opt/phone-tunnel/client.py','install-credentials'],input=json.dumps({'private_key':wrong.read_text(),'known_hosts':known}))
   time.sleep(4)
   if run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0:raise RuntimeError('wrong_clientkey_accepted')
   run(['docker','exec','-i',client,'python3','/opt/phone-tunnel/client.py','install-credentials'],input=payload)
   wait_for(lambda:run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0,timeout=50)
   # A mismatched paired password must fail the full SSH health command.
   # sshd's forced health process reads this private token; no token is printed.
   token_update=['docker','exec','-i',relay,'python3','-c',"import pathlib,sys;pathlib.Path('/var/lib/phone-tunnel/runner_password').write_text(sys.stdin.read())"]
   run(token_update,input='wrong-pairing-fixture-password-2026')
   if run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0:raise RuntimeError('mismatched_pairing_claimed_healthy')
   run(token_update,input=password)
   wait_for(lambda:run(['docker','exec',client,'python3','/opt/phone-tunnel/client.py','health'],check=False).returncode==0,timeout=15)
   # Port conflict must terminate another forwarding attempt, not claim success.
   conflict=run(['docker','exec',client,'ssh','-F','/var/lib/phone-tunnel/ssh_config','-o','ControlMaster=no','-o','ControlPath=none','-N','-T','-R','0.0.0.0:4096:model-runner:4096','phone-model-tunnel'],check=False,timeout=10)
   if conflict.returncode==0:raise RuntimeError('forward_conflict_claimed_success')
   report={'status':'passed','actual_reverse_ssh':True,'jump_and_relay':True,'remote_worker_http_roundtrip':True,'pinned_opencode_version':'1.18.10','asr_catalog':args.require_asr,'nonroot_sshd':True,'readonly_rootfs':True,'cap_drop_all':True,'anonymous_rejected':True,'arbitrary_command_denied':True,'relay_restart_reconnected':True,'wrong_hostkey_denied':True,'wrong_clientkey_denied':True,'forward_port_conflict_denied':True,'mismatched_pairing_not_healthy':True,'host_http_ports_published':False,'live_remote_host_checked':False,'provider_inference_requests':0}
   args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2)+'\n');args.report.chmod(0o600);print(json.dumps(report))
  except Exception as error:
   args.report.parent.mkdir(parents=True,exist_ok=True)
   # Private debugging contains only fixture service logs; never any application's runtime env.
   for name in names:
    data=run(['docker','logs',name],check=False)
    (args.report.parent/(name+'.log')).write_text(data.stdout+data.stderr)
   print(json.dumps({'status':'failed','error':str(error)}));return 1
  finally:
   for name in reversed(names):run(['docker','rm','-f',name],check=False)
   for network in [remote,local]:run(['docker','network','rm',network],check=False)
   for name in volumes:run(['docker','volume','rm',name],check=False)
 return 0
if __name__=='__main__':raise SystemExit(main())
