#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
action=${1:-up}
case "$action" in
  up)
    python3 scripts/setup-local.py
    docker compose --env-file .env.platform -f infra/compose.yml up -d --build --wait
    printf '%s\n' 'Сигнал: http://localhost:18081 · почта: http://localhost:18025'
    ;;
  down|ps|logs)
    shift
    docker compose --env-file .env.platform -f infra/compose.yml "$action" "$@"
    ;;
  ai)
    docker compose --env-file .env.platform -f infra/compose.yml -f infra/compose.ai.yml --profile ai up -d --build --wait model-runner
    ;;
  *) printf '%s\n' 'Usage: scripts/dev.sh up|down|ps|logs|ai' >&2; exit 2 ;;
esac
