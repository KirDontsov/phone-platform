#!/bin/sh
# Local OpenCode and private reverse SSH tunnel for Phone Platform.
set -eu
cd "$(dirname "$0")"
exec python3 scripts/analyzer.py "$@"
