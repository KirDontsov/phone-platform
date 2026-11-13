#!/usr/bin/env python3
"""One-command LOCAL analyzer startup; remote installation remains an explicit step.

No host SSH agent, Bondscan credential or application database is read by this tool.
"""
from __future__ import annotations
import argparse
import base64
import json
import os
from pathlib import Path
import re
import secrets
import stat
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parent.parent
LOCAL = '.env.analyzer.local'
PAIR = 'secrets/analyzer-pair.env'
SSH_DIR = 'secrets/analyzer-ssh'
ALLOWED = {'PHONE_REMOTE_SSH_HOST', 'PHONE_REMOTE_SSH_USER', 'PHONE_REMOTE_SSH_PORT', 'PHONE_REMOTE_TUNNEL_SSH_PORT', 'PHONE_LOCAL_RUNNER_PORT', 'PHONE_ANALYZER_PROJECT'}


def private_file(path: Path):
    if path.is_symlink() or not path.is_file() or stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise ValueError(f'Файл {path.name} должен быть обычным файлом с правами 600')


def create_once(path: Path, content: str, mode=0o600):
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.exists() or path.is_symlink():
        if mode == 0o600:
            private_file(path)
        elif path.is_symlink() or not path.is_file():
            raise ValueError('Небезопасный путь публичного ключа')
        return False
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), mode)
    with os.fdopen(fd, 'w') as stream:
        stream.write(content)
    return True


def read_env(path: Path):
    private_file(path)
    result = {}
    for line in path.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        key, separator, value = line.partition('=')
        if not separator or key in result:
            raise ValueError(f'Проверь строки в {path.name}')
        result[key] = value
    return result


def initialize(root=ROOT):
    local, pair, ssh = root / LOCAL, root / PAIR, root / SSH_DIR
    if (root/'secrets').is_symlink() or ssh.is_symlink():
        raise ValueError('Каталог ключей не должен быть ссылкой')
    ssh.mkdir(mode=0o700, parents=True, exist_ok=True)
    if ssh.is_symlink():
        raise ValueError('Каталог ключей не должен быть ссылкой')
    os.chmod(ssh, 0o700)
    create_once(local, '# SSH адрес сервера Phone Platform. Ключи и пароли сюда не вставлять.\nPHONE_REMOTE_SSH_HOST=\nPHONE_REMOTE_SSH_USER=\nPHONE_REMOTE_SSH_PORT=22\nPHONE_REMOTE_TUNNEL_SSH_PORT=18422\nPHONE_LOCAL_RUNNER_PORT=18496\nPHONE_ANALYZER_PROJECT=phone-analyzer\n')
    create_once(pair, 'MODEL_RUNNER_PASSWORD=' + secrets.token_urlsafe(36) + '\n')
    key = ssh / 'id_ed25519'
    if not key.exists() and not key.is_symlink():
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-C', 'phone-analyzer', '-f', str(key)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    private_file(key)
    if not key.with_suffix('.pub').is_file():
        raise ValueError('Нет публичной пары ключа; существующий приватный ключ не изменён')
    create_once(ssh / 'known_hosts', '# Добавь проверенные ключи командой pin. Не используй StrictHostKeyChecking=no.\n')
    read_pair(root)


def read_pair(root=ROOT):
    value = read_env(root / PAIR)
    password = value.get('MODEL_RUNNER_PASSWORD', '')
    if set(value) != {'MODEL_RUNNER_PASSWORD'} or not re.fullmatch(r'[A-Za-z0-9_-]{24,128}', password):
        raise ValueError('Проверь MODEL_RUNNER_PASSWORD в secrets/analyzer-pair.env')
    return password


def configuration(root=ROOT, require_remote=True):
    value = read_env(root / LOCAL)
    if set(value) - ALLOWED:
        raise ValueError('В .env.analyzer.local есть неизвестные поля')
    host, user = value.get('PHONE_REMOTE_SSH_HOST', ''), value.get('PHONE_REMOTE_SSH_USER', '')
    if require_remote or host:
        if not re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9.-]{0,251}[A-Za-z0-9])?', host) or host.endswith('.invalid'):
            raise ValueError('Укажи PHONE_REMOTE_SSH_HOST в .env.analyzer.local')
    if require_remote or user:
        if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_-]{0,63}', user):
            raise ValueError('Укажи PHONE_REMOTE_SSH_USER в .env.analyzer.local')
    for key, default in [('PHONE_REMOTE_SSH_PORT',22),('PHONE_REMOTE_TUNNEL_SSH_PORT',18422),('PHONE_LOCAL_RUNNER_PORT',18496)]:
        raw = value.get(key, str(default))
        if not raw.isdigit() or not 1 <= int(raw) <= 65535:
            raise ValueError(f'Неверный порт {key}')
        value[key] = str(int(raw))
    project = value.get('PHONE_ANALYZER_PROJECT', 'phone-analyzer')
    if not re.fullmatch(r'phone-analyzer(?:-[a-z0-9][a-z0-9_-]{0,30})?', project):
        raise ValueError('Отдельный project должен называться phone-analyzer или phone-analyzer-…')
    value['PHONE_ANALYZER_PROJECT'] = project
    return value


def public_key(text):
    rows = [line.strip() for line in text.splitlines() if line.strip() and not line.startswith('#')]
    if len(rows) != 1 or not re.fullmatch(r'ssh-ed25519 [A-Za-z0-9+/]+={0,2}(?: [^\r\n]*)?', rows[0]):
        raise ValueError('Нужен один публичный SSH ed25519 ключ, не приватный')
    kind, encoded, *_ = rows[0].split()
    if len(base64.b64decode(encoded, validate=True)) != 51:
        raise ValueError('Некорректный публичный SSH ключ')
    return f'{kind} {encoded}'


def pin(root: Path, alias: str, source: Path):
    key = public_key(source.read_text())
    path = root / SSH_DIR / 'known_hosts'
    private_file(path)
    lines = path.read_text().splitlines()
    existing = [line for line in lines if line.split() and line.split()[0] == alias]
    expected = f'{alias} {key}'
    if existing:
        if len(existing) != 1 or ' '.join(existing[0].split()[:3]) != expected:
            raise ValueError('Ключ хоста изменился; проверь fingerprint. Существующая привязка сохранена')
        return
    with path.open('a') as stream:
        stream.write(expected + '\n')


def credentials(root=ROOT):
    private, known = root / SSH_DIR / 'id_ed25519', root / SSH_DIR / 'known_hosts'
    private_file(private); private_file(known)
    value = known.read_text()
    aliases = {line.split()[0] for line in value.splitlines() if line.strip() and not line.startswith('#')}
    if not {'phone-analyzer-jump', 'phone-model-tunnel'}.issubset(aliases):
        raise ValueError('Сначала закрепи проверенные ключи jump-сервера и relay: ./start-analyzer.sh pin --help')
    return json.dumps({'private_key':private.read_text(),'known_hosts':value})


def compose(root: Path, config: dict):
    # All mutations use this dedicated project, never a repository-global compose down.
    return ['docker','compose','--project-name',config['PHONE_ANALYZER_PROJECT'],'--env-file',str(root/LOCAL),'--env-file',str(root/PAIR),'-f',str(root/'infra/compose.analyzer-local.yml'),'--profile','analyzer']


def execute(argv, *, root=ROOT, config=None, input=None, capture=False, check=True):
    env = os.environ.copy()
    if config:
        env.update(config)
        env['MODEL_RUNNER_PASSWORD'] = read_pair(root)
        # Only needed so Compose can parse unused tunnels during local-only startup.
        env.setdefault('PHONE_REMOTE_SSH_HOST','not-configured.invalid')
        env.setdefault('PHONE_REMOTE_SSH_USER','phonebridge')
        if not env['PHONE_REMOTE_SSH_HOST']:env['PHONE_REMOTE_SSH_HOST']='not-configured.invalid'
        if not env['PHONE_REMOTE_SSH_USER']:env['PHONE_REMOTE_SSH_USER']='phonebridge'
    return subprocess.run(argv, cwd=root, env=env, input=input, text=True, capture_output=capture, check=check)


def status(root, config):
    result = execute(compose(root,config)+['ps','--format','json'],root=root,config=config,capture=True)
    raw = result.stdout.strip()
    try:
        rows = json.loads(raw) if raw.startswith('[') else [json.loads(line) for line in raw.splitlines() if line]
    except ValueError:
        raise ValueError('Docker вернул неизвестный формат состояния')
    return [{'service':r.get('Service'),'state':r.get('State'),'health':r.get('Health')} for r in rows]


def main():
    parser=argparse.ArgumentParser(description='Phone Platform: локальный Docker OpenCode и SSH-туннель к своему серверу')
    sub=parser.add_subparsers(dest='command')
    for name in ['init','up','local','down','status','doctor','public-key','remote-instructions','connect-routerai']:
        sub.add_parser(name)
    pins=sub.add_parser('pin',help='Закрепить публичные ключи, полученные по доверенному каналу')
    pins.add_argument('--jump-public-key',type=Path)
    pins.add_argument('--relay-public-key',type=Path)
    args=parser.parse_args()
    command=args.command or 'up'
    if command in ['init','up','local']:
        initialize()
    if command=='init':
        print('Созданы отдельные Phone ключи и pairing. Заполни .env.analyzer.local; ./start-analyzer.sh remote-instructions')
        return 0
    if command=='public-key':
        print(public_key((ROOT/SSH_DIR/'id_ed25519.pub').read_text())+' phone-analyzer')
        return 0
    if command=='pin':
        if not args.jump_public_key and not args.relay_public_key:
            parser.error('Укажи --jump-public-key и/или --relay-public-key')
        for alias,path in [('phone-analyzer-jump',args.jump_public_key),('phone-model-tunnel',args.relay_public_key)]:
            if path:pin(ROOT,alias,path)
        print('Публичные ключи закреплены; секреты не выводились')
        return 0
    if command=='remote-instructions':
        print('''Одноразовая настройка сервера Phone Platform:
1. Добавь публичный secrets/analyzer-ssh/id_ed25519.pub отдельному SSH-пользователю Phone. Рекомендуемая строка authorized_keys: restrict,port-forwarding,permitopen="127.0.0.1:18422" <публичный ключ>.
2. Передай на сервер только публичный id_ed25519.pub и защищённый secrets/analyzer-pair.env. Приватный id_ed25519 остаётся здесь.
3. На сервере из репозитория: env -u MODEL_RUNNER_PASSWORD docker compose --env-file .env.platform --env-file secrets/analyzer-pair.env -f infra/compose.yml -f infra/compose.production.yml -f infra/compose.analyzer-remote.yml up -d --build model-tunnel api worker
4. Получи публичный relay ключ: env -u MODEL_RUNNER_PASSWORD docker compose --env-file .env.platform --env-file secrets/analyzer-pair.env -f infra/compose.yml -f infra/compose.production.yml -f infra/compose.analyzer-remote.yml exec -T model-tunnel python3 /opt/phone-tunnel/server.py public-key
5. Сверь fingerprint ключа SSH-сервера по доверенному каналу; сохрани jump/relay публичные ключи в отдельных файлах.
6. Здесь: ./start-analyzer.sh pin --jump-public-key ПУБЛИЧНЫЙ_КЛЮЧ_СЕРВЕРА --relay-public-key ПУБЛИЧНЫЙ_КЛЮЧ_RELAY
7. Заполни SSH host/user в .env.analyzer.local. Повседневный запуск: ./start-analyzer.sh
Не включай compose.ai.yml на сервере одновременно с relay: DNS model-runner должен указывать только на relay.
Компьютер должен оставаться включённым. Можно использовать уже подключённого локального провайдера; OpenRouter настраивается отдельно.''')
        return 0
    config=configuration(require_remote=command in ['up','doctor'])
    read_pair()
    cmd=compose(ROOT,config)
    if command=='connect-routerai':
        runtime=ROOT/'secrets/local-opencode.json'
        private_file(runtime)
        organization=str(uuid.UUID(json.loads(runtime.read_text())['organization_id']))
        execute([sys.executable,str(ROOT/'scripts/connect-routerai.py'),'--organization',organization,'--project',config['PHONE_ANALYZER_PROJECT']],config=config)
        return 0
    if command=='local':
        execute(cmd+['up','-d','--build','--wait','model-runner'],config=config)
        print('Локальный connector запущен; SSH-туннель не запускался')
    elif command=='up':
        payload=credentials()
        execute(cmd+['up','-d','--build','model-runner','tunnels'],config=config)
        execute(cmd+['exec','-T','tunnels','python3','/opt/phone-tunnel/client.py','install-credentials'],config=config,input=payload,capture=True)
        execute(cmd+['up','-d','--no-recreate','--wait','--wait-timeout','90','model-runner','tunnels'],config=config)
        print('Локальный OpenCode и сквозной SSH-туннель готовы; данные и секреты не выводились')
    elif command=='down':
        execute(cmd+['stop','tunnels','model-runner'],config=config)
        execute(cmd+['rm','-f','tunnels','model-runner'],config=config)
        print('Остановлены только контейнеры этого локального analyzer; pairing сохранён')
    elif command=='status':
        print(json.dumps(status(ROOT,config),ensure_ascii=False))
    elif command=='doctor':
        credentials()
        result=execute(cmd+['exec','-T','tunnels','python3','/opt/phone-tunnel/client.py','health'],config=config,capture=True,check=False)
        print(json.dumps({'configuration':True,'ssh_http_roundtrip_and_pairing':result.returncode==0,'containers':status(ROOT,config)},ensure_ascii=False))
        return 0 if result.returncode==0 else 1
    return 0

if __name__=='__main__':
    try:
        raise SystemExit(main())
    except (OSError,ValueError,subprocess.SubprocessError) as error:
        # Never include subprocess output or an environment/key value in public diagnostics.
        message=str(error) if isinstance(error,ValueError) else 'Не удалось выполнить локальную команду Docker/SSH; проверь Docker и настройки Phone analyzer'
        print(message,file=sys.stderr)
        raise SystemExit(1)
