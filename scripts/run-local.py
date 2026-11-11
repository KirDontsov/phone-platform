#!/usr/bin/env python3
"""Run native API/worker against this project's localhost-only Compose services."""
from pathlib import Path
import os,subprocess,sys
root=Path(__file__).resolve().parent.parent
if len(sys.argv)!=2 or sys.argv[1] not in ('api','worker'):
    raise SystemExit('Usage: python3 scripts/run-local.py api|worker')
p=root/'.env.platform'
if p.is_symlink() or not p.is_file() or p.stat().st_mode&0o077:
    raise SystemExit('Run setup-local.py; .env.platform must be private')
env=dict(os.environ)
for line in p.read_text().splitlines():
    if line and not line.startswith('#'):
        k,v=line.split('=',1);env[k]=v
for k,old,new in [('DATABASE_URL','@db:5432/','@127.0.0.1:25437/'),('RABBITMQ_URL','@rabbitmq:5672/','@127.0.0.1:25672/')]:
    env[k]=env[k].replace(old,new)
env.update(API_BIND='127.0.0.1:18080',SMTP_HOST='127.0.0.1',SMTP_PORT='11025',MEDIA_ROOT=str(root/'data'/'media'))
os.chdir(root)
os.execvpe('cargo',['cargo','run','--locked','-p','phone-'+sys.argv[1]],env)
