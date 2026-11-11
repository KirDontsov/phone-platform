#!/usr/bin/env python3
"""Smoke the private pinned OpenCode server without sending any model prompt.

Only metadata, authentication, configuration, and empty-session lifecycle are tested.
Provider credentials are neither read nor printed by this script.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent

# Executed inside the container: authentication comes from its runtime environment,
# never a command argument. Returned diagnostics deliberately contain no raw config.
PROBE = r'''
const password=process.env.OPENCODE_SERVER_PASSWORD;
if(!password) throw Error('runner_auth_missing');
const base='http://127.0.0.1:4096';
const auth='Basic '+Buffer.from('opencode:'+password).toString('base64');
async function request(path,method='GET',body=undefined,authenticated=true){
 const response=await fetch(base+path,{method,headers:{...(authenticated?{Authorization:auth}:{}),...(body===undefined?{}:{'Content-Type':'application/json'})},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(12000)});
 return {status:response.status,body:await response.json().catch(()=>null)};
}
const unauthorized=await request('/config','GET',undefined,false);
if(unauthorized.status!==401) throw Error('anonymous_config_not_rejected');
const health=await request('/global/health');
if(health.status!==200||health.body?.healthy!==true||health.body.version!=='1.18.10') throw Error('pinned_health_failed');
const config=await request('/config');
if(config.status!==200) throw Error('config_unavailable');
const value=config.body;
if(!value||value.permission?.['*']!=='deny'||Object.values(value.permission).some(v=>v!=='deny')) throw Error('permission_not_deny_all');
if(Object.keys(value.mcp||{}).length!==0||(value.plugin||[]).length!==0) throw Error('external_plugins_or_mcp_enabled');
if(value.autoupdate!==false||value.snapshot!==false||value.share!=='disabled'||value.lsp!==false||value.formatter!==false) throw Error('unsafe_automation_config');
if(Object.values(value.tools||{}).some(v=>v!==false)) throw Error('enabled_tool_config');
const tools=await request('/experimental/tool/ids');
if(tools.status!==200||!Array.isArray(tools.body)||tools.body.some(v=>typeof v!=='string')) throw Error('tool_api_incompatible');
const providers=await request('/provider');
if(providers.status!==200||!Array.isArray(providers.body?.all)||!Array.isArray(providers.body?.connected)) throw Error('provider_api_incompatible');
const created=await request('/session','POST',{title:'isolation-metadata-smoke'});
if(created.status!==200||typeof created.body?.id!=='string') throw Error('session_api_incompatible');
const session=created.body.id;
if(!/^[A-Za-z0-9_-]{1,128}$/.test(session)) throw Error('unsafe_session_id');
const removed=await request('/session/'+encodeURIComponent(session),'DELETE');
if(removed.status!==200||removed.body!==true) throw Error('session_cleanup_failed');
const missing=await request('/session/'+encodeURIComponent(session));
if(missing.status!==404) throw Error('deleted_session_still_accessible');
const applicationSecrets=['DATABASE_URL','ENCRYPTION_KEY','POSTGRES_PASSWORD','RABBITMQ_PASSWORD','SMTP_PASSWORD'];
if(applicationSecrets.some(key=>process.env[key])) throw Error('application_secret_in_runner');
console.log(JSON.stringify({healthy:true,version:health.body.version,anonymous_rejected:true,deny_all_permission:true,tool_config_disabled:true,mcp_servers:0,plugins:0,session_lifecycle:true,registered_tool_count:tools.body.length,provider_catalogue_count:providers.body.all.length,connected_provider_count:providers.body.connected.length,model_requests:0,application_secrets_absent:true,uid:process.getuid()}));
'''


def execute(command, *, input=None, capture=False):
    result = subprocess.run(command, input=input, text=True, capture_output=capture, cwd=ROOT)
    if result.returncode:
        # Provider/runtime output is not a diagnostic channel for this orchestrator.
        if capture:
            raise RuntimeError('runner_command_failed')
        raise RuntimeError('runner_build_or_start_failed')
    return result.stdout if capture else ''


def inspect_isolation(container):
    info = json.loads(execute(['docker', 'inspect', container], capture=True))[0]
    host = info['HostConfig']
    if info['Config']['User'] != '10001:10001' or not host['ReadonlyRootfs'] or host['Privileged']:
        raise RuntimeError('container_user_or_rootfs_not_isolated')
    if 'ALL' not in host.get('CapDrop', []) or 'no-new-privileges:true' not in host.get('SecurityOpt', []):
        raise RuntimeError('container_capabilities_not_restricted')
    if host.get('Memory', 0) <= 0 or host.get('PidsLimit', 0) <= 0:
        raise RuntimeError('container_limits_missing')
    if host.get('PortBindings'):
        raise RuntimeError('runner_has_host_published_ports')
    mounts = info.get('Mounts', [])
    if any(m['Type'] in ('bind', 'volume') for m in mounts):
        raise RuntimeError('runner_has_persistent_or_host_mount')
    destinations = {m['Destination'] for m in mounts if m['Type'] == 'tmpfs'} | set(host.get('Tmpfs', {}))
    if not {'/tmp', '/work'}.issubset(destinations):
        raise RuntimeError('runner_tmpfs_missing')
    return {'read_only_root': True, 'non_root': True, 'capabilities_dropped': True,
            'no_new_privileges': True, 'memory_limited': True, 'pids_limited': True,
            'host_ports': False, 'host_or_persistent_mounts': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--env-file', type=Path, default=ROOT / '.env.platform')
    parser.add_argument('--project-name', help='Optional isolated Compose project; default matches the platform project.')
    parser.add_argument('--build', action='store_true')
    parser.add_argument('--up', action='store_true', help='Start only model-runner and wait for authenticated health.')
    parser.add_argument('--cleanup', action='store_true', help='Remove only the runner container afterwards; never global compose down.')
    parser.add_argument('--report', type=Path, default=ROOT / 'data/eval/model-runner-smoke.json')
    args = parser.parse_args()
    compose = ['docker', 'compose', '--env-file', str(args.env_file), '-f', 'infra/compose.yml', '-f', 'infra/compose.ai.yml', '--profile', 'ai']
    if args.project_name:
        compose += ['--project-name', args.project_name]
    try:
        execute(compose + ['config', '--quiet'], capture=True)
        if args.build:
            execute(compose + ['build', 'model-runner'])
        if args.up:
            execute(compose + ['up', '-d', '--wait', '--wait-timeout', '180', 'model-runner'])
        container = execute(compose + ['ps', '-q', 'model-runner'], capture=True).strip()
        if not container:
            raise RuntimeError('runner_not_started_use_up')
        isolation = inspect_isolation(container)
        raw = execute(compose + ['exec', '-T', 'model-runner', 'node', '--input-type=module', '-'], input=PROBE, capture=True)
        metadata = json.loads(raw.strip().splitlines()[-1])
        report = {'status': 'passed', 'container': isolation, 'api': metadata,
                  'live_model_quality': 'pending_provider_credentials_and_evaluation',
                  'no_model_prompts_sent': True}
        args.report.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(args.report.parent, 0o700)
        fd = os.open(args.report, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, 'O_NOFOLLOW', 0), 0o600)
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, 'w') as stream:
            json.dump(report, stream, ensure_ascii=False, indent=2)
            stream.write('\n')
        print(json.dumps(report, ensure_ascii=False))
        return 0
    except (OSError, ValueError, KeyError, IndexError, RuntimeError):
        print('{"status":"failed","error":"model_runner_smoke_failed"}', file=sys.stderr)
        return 1
    finally:
        if args.cleanup:
            subprocess.run(compose + ['rm', '--stop', '--force', 'model-runner'], cwd=ROOT,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


if __name__ == '__main__':
    sys.exit(main())
