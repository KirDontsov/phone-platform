#!/usr/bin/env python3
"""Copy only existing OpenCode providers into private, tenant-bound Phone runtime."""
import argparse,json,os,re,subprocess,tempfile,uuid
from pathlib import Path
from urllib.parse import urlsplit,urlunsplit
ROOT=Path(__file__).resolve().parents[1]
def build_settings(source, organization):
    org=str(uuid.UUID(organization));data=json.loads(source.read_text());providers={}
    for name, definition in data.get('provider',{}).items():
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,64}',name) or not isinstance(definition,dict):continue
        options=definition.get('options',{});key=options.get('apiKey');base=options.get('baseURL')
        if not isinstance(key,str) or len(key)<8 or any(c in key for c in '\r\n\0') or not isinstance(base,str):continue
        url=urlsplit(base)
        if url.username or url.password or url.query or url.fragment or url.scheme not in ['http','https']:raise ValueError('unsafe_provider_endpoint')
        if url.scheme=='http':
            if url.hostname not in ['localhost','127.0.0.1']:raise ValueError('unsafe_provider_endpoint')
            url=url._replace(netloc='host.docker.internal'+(':'+str(url.port)if url.port else ''))
        models=definition.get('models',{})
        if not isinstance(models,dict) or len(models)>256:raise ValueError('invalid_provider_models')
        providers[name]={'name':str(definition.get('name',name))[:150],'npm':'@ai-sdk/openai-compatible','options':{'baseURL':urlunsplit(url).rstrip('/'),'apiKey':key},'models':{m:{'name':str(v.get('name',m))[:150]if isinstance(v,dict)else m}for m,v in models.items()if isinstance(m,str)and len(m)<=200 and not any(c in m for c in '\r\n\0')}}
    if not providers:raise ValueError('no_configured_local_provider')
    return {'version':1,'organization_id':org,'providers':providers}
def install(settings, project):
    if not re.fullmatch(r'phone-[a-zA-Z0-9_-]{1,50}',project):raise ValueError('invalid_phone_project')
    volume=project+'_local-providers'
    subprocess.run(['docker','volume','create',volume],check=True,stdout=subprocess.DEVNULL)
    code="const fs=require('fs');let s='';process.stdin.on('data',c=>s+=c);process.stdin.on('end',()=>{JSON.parse(s);fs.mkdirSync('/config',{recursive:true,mode:448});fs.writeFileSync('/config/providers.json.tmp',s,{mode:384});fs.chownSync('/config/providers.json.tmp',10001,10001);fs.renameSync('/config/providers.json.tmp','/config/providers.json');fs.chownSync('/config',10001,10001);fs.chmodSync('/config',448);});"
    subprocess.run(['docker','run','--rm','-i','--network','none','--read-only','--user','0:0','--mount','type=volume,src='+volume+',dst=/config','--entrypoint','node','phone-platform-model-runner:opencode-1.18.10','-e',code],input=json.dumps(settings),text=True,check=True,stdout=subprocess.DEVNULL)
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--organization',required=True);parser.add_argument('--source',type=Path,default=Path.home()/'.config/opencode/opencode.json');parser.add_argument('--project',default='phone-analyzer');parser.add_argument('--install',action='store_true');args=parser.parse_args()
    settings=build_settings(args.source,args.organization);directory=ROOT/'secrets';directory.mkdir(mode=0o700,exist_ok=True);target=directory/'local-opencode.json'
    if target.is_symlink():raise ValueError('unsafe_runtime_file')
    if target.exists() and json.loads(target.read_text()).get('organization_id')!=settings['organization_id']:raise ValueError('local_runner_already_bound_to_another_organization')
    fd,tmp=tempfile.mkstemp(prefix='local-opencode.',dir=directory)
    try:
        with os.fdopen(fd,'w')as file:json.dump(settings,file);file.write('\n')
        os.chmod(tmp,0o600);os.replace(tmp,target)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)
    if args.install:install(settings,args.project)
    print(json.dumps({'imported':True,'providers':list(settings['providers']),'credentials_printed':False,'installed':args.install}))
if __name__=='__main__':
    try:main()
    except Exception as error:
        print(json.dumps({'error':'opencode_import_failed','type':type(error).__name__}));raise SystemExit(1)
