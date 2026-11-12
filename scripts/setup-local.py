#!/usr/bin/env python3
"""Create isolated local demo config, never use existing provider credentials."""
from pathlib import Path
import base64, secrets, os
root=Path(__file__).resolve().parent.parent
p=root/'.env.platform'
if p.exists():
    if p.is_symlink() or not p.is_file() or p.stat().st_mode & 0o077:
        raise SystemExit('.env.platform must be a regular private file (chmod 600)')
    print('.env.platform already exists; preserved')
else:
    password=secrets.token_urlsafe(24)
    settings={
      'COMPOSE_PROJECT_NAME':'phone-platform', 'POSTGRES_USER':'phone', 'POSTGRES_PASSWORD':password,
      'POSTGRES_DB':'phone_platform', 'DATABASE_URL':f'postgres://phone:{password}@db:5432/phone_platform',
      'RABBITMQ_USER':'phone', 'RABBITMQ_PASSWORD':password,
      'RABBITMQ_URL':f'amqp://phone:{password}@rabbitmq:5672/%2f',
      'ENCRYPTION_KEY':base64.b64encode(secrets.token_bytes(32)).decode(),
      'DEMO_MODE':'true','COOKIE_SECURE':'false','PUBLIC_URL':'http://localhost:18081',
      'ALLOWED_ORIGINS':'http://localhost:18081,http://127.0.0.1:18081,http://localhost:18082,http://127.0.0.1:18082',
      'API_BIND':'0.0.0.0:8080','MEDIA_ROOT':'/app/media','SMTP_HOST':'mailpit','SMTP_PORT':'1025',
      'MAIL_FROM':'Phone Platform <noreply@phone.local>',
      'BOOTSTRAP_EMAIL':'owner@demo.local','BOOTSTRAP_PASSWORD':'PhoneDemo!2026',
      'MODEL_RUNNER_PASSWORD':secrets.token_urlsafe(32),
      'ASSISTANT_RUNNER_TOKEN':secrets.token_urlsafe(32),
      'ASSISTANT_RUNNER_URL':'http://assistant-runner:8091',
      'BOOTSTRAP_NAME':'Анна Смирнова','BOOTSTRAP_ORGANIZATION':'Атлас · Продажи',
    }
    fd=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,'w') as f:
      for key,value in settings.items():f.write(f'{key}={value}\n')
    print('Created private .env.platform for local demo (independent of Telphin .env)')
# Append only missing runtime keys under a lock; existing secrets/config are preserved.
import fcntl
with p.open('r+') as runtime:
    fcntl.flock(runtime,fcntl.LOCK_EX)
    content=runtime.read()
    present={line.split('=',1)[0] for line in content.splitlines() if '=' in line and not line.startswith('#')}
    missing={'ASSISTANT_RUNNER_TOKEN':lambda:secrets.token_urlsafe(32),
             'ASSISTANT_RUNNER_URL':lambda:'http://assistant-runner:8091',
             'MODEL_RUNNER_PASSWORD':lambda:secrets.token_urlsafe(32)}
    additions=[key+'='+make() for key,make in missing.items() if key not in present]
    if additions:
        runtime.seek(0,2)
        runtime.write(('' if not content or content.endswith('\n') else '\n')+'\n'.join(additions)+'\n')
        runtime.flush();os.fsync(runtime.fileno())
    fcntl.flock(runtime,fcntl.LOCK_UN)
media=root/'data'/'media';media.mkdir(parents=True,exist_ok=True);os.chmod(media,0o700)
