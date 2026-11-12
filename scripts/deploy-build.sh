#!/usr/bin/env bash
# Native production image builds with an isolated BuildKit cache on the server HDD.
set -euo pipefail

BUILDKIT_IMAGE='moby/buildkit:buildx-stable-1@sha256:c5f8862f52499773af85bc07695b2379ef08e8002352b0df2c4864c91d78dabe'

validate_path() {
  [[ "$1" =~ ^/[a-zA-Z0-9_./-]+$ && "$1" != *'/../'* && "$1" != *'/./'* && "$1" != */.. && "$1" != */. ]]
}

require_space() {
  local available_kib
  available_kib=$(df -Pk "$1" | awk 'NR == 2 {print $4}')
  if [[ ! "$available_kib" =~ ^[0-9]+$ ]] || (( available_kib < $2 * 1024 * 1024 )); then
    echo "Insufficient free space on $1: need at least $2 GiB; shared resources will not be pruned." >&2
    exit 1
  fi
}

if [[ "${1:-}" != --remote ]]; then
  if [[ $# -lt 1 || $# -gt 4 ]]; then
    echo 'Usage: scripts/deploy-build.sh user@host [remote-source-dir] [remote-build-dir] [git-ref]' >&2
    exit 2
  fi
  server=$1
  source_dir=${2:-/home/dks/phone-platform}
  build_dir=${3:-/var/www/phone-platform-build}
  git_ref=${4:-HEAD}
  [[ "$server" =~ ^[a-zA-Z0-9_][a-zA-Z0-9_.-]*@[a-zA-Z0-9][a-zA-Z0-9_.-]*$ ]] || exit 2
  validate_path "$source_dir" && validate_path "$build_dir" || exit 2
  [[ "$source_dir" == */phone-platform && "$build_dir" == */phone-platform-build ]] || exit 2
  repo_dir=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
  revision=$(git -C "$repo_dir" rev-parse --verify --end-of-options "${git_ref}^{commit}")
  # Runtime credentials and customer data must never be staged as source.
  if git -C "$repo_dir" ls-tree -r --name-only "$revision" | awk '
    !/(^|\/)\.env\.example$/ && /(^|\/)(\.env([.\/]|$)|auth\.json$|secrets\/|data\/)/ { found=1 }
    END { exit !found }
  '; then
    echo 'Refusing source archive containing runtime credentials or data.' >&2
    exit 1
  fi
  stage_script=$(cat <<'STAGE'
set -euo pipefail
umask 077
project_dir=$1
build_dir=$2
revision=$3
mkdir -p "$project_dir" "$build_dir"
chmod 700 "$project_dir" "$build_dir"
exec 9>"$build_dir/build.lock"
flock 9
context_dir="$build_dir/source-$revision"
stage_dir=''
trap '[[ -z "$stage_dir" ]] || rm -rf -- "$stage_dir"' EXIT
if [[ -L "$context_dir" ]]; then
  echo 'Refusing symlink build context.' >&2
  exit 1
elif [[ -d "$context_dir" ]]; then
  # Drain the incoming archive; a completed context for this Git revision is immutable.
  cat >/dev/null
else
  stage_dir=$(mktemp -d "$build_dir/source-$revision.tmp.XXXXXX")
  tar -xpf - -C "$stage_dir"
  mv -- "$stage_dir" "$context_dir"
  stage_dir=''
fi
# The working tree supplies operational scripts/Compose files. Runtime files survive.
# Image builds exclusively use the immutable revision context above.
tar -C "$context_dir" -cf - . | tar -C "$project_dir" -xpf -
STAGE
  )
  printf -v prepare '%q ' bash -c "$stage_script" -- "$source_dir" "$build_dir" "$revision"
  git -C "$repo_dir" archive "$revision" | ssh -o BatchMode=yes "$server" "$prepare"
  scp -q -o BatchMode=yes "$repo_dir/scripts/deploy-build.sh" "$server:$source_dir/scripts/deploy-build.sh"
  printf -v run 'bash %q --remote %q %q %q' "$source_dir/scripts/deploy-build.sh" "$source_dir" "$build_dir" "$revision"
  exec ssh -o BatchMode=yes "$server" "$run"
fi

[[ $# == 4 ]] || exit 2
source_dir=$2
build_dir=$3
revision=$4
validate_path "$source_dir" && validate_path "$build_dir" || exit 2
[[ "$source_dir" == */phone-platform && "$build_dir" == */phone-platform-build && "$revision" =~ ^[a-f0-9]{40}$ ]] || exit 2
context_dir="$build_dir/source-$revision"
[[ -d "$context_dir" && ! -L "$context_dir" ]] || { echo 'Immutable source context is missing.' >&2; exit 1; }
umask 077
mkdir -p "$build_dir/state" "$build_dir/output"
exec 9>"$build_dir/build.lock"
flock -n 9 || { echo 'Another Phone Platform build is running.' >&2; exit 1; }
tag="15-${revision:0:7}"
report="$build_dir/images-$tag.txt"
printf 'revision=%s\nbuildkit=%s\n' "$revision" "$BUILDKIT_IMAGE" > "$report"
docker pull "$BUILDKIT_IMAGE"
docker run --rm --entrypoint buildkitd "$BUILDKIT_IMAGE" --version >> "$report"
for service in api worker client assistant-runner model-tunnel; do
  case "$service" in
    api|worker|client) dockerfile="phone-$service/Dockerfile" ;;
    assistant-runner) dockerfile=infra/assistant-runner/Dockerfile ;;
    model-tunnel) dockerfile=infra/analyzer-tunnel/Dockerfile ;;
  esac
  image="phone-platform-$service:$tag"
  artifact="$build_dir/output/$service-$tag.tar"
  require_space "$build_dir" 10
  echo "Building $image with isolated HDD cache"
  docker run --rm --name phone-platform-image-build --privileged --cpus 4 --memory 12g \
    --mount "type=bind,src=$context_dir,dst=/source,readonly" \
    --mount "type=bind,src=$build_dir/state,dst=/var/lib/buildkit" \
    --mount "type=bind,src=$build_dir/output,dst=/output" \
    --entrypoint buildctl-daemonless.sh "$BUILDKIT_IMAGE" build \
    --frontend dockerfile.v0 --local context=/source --local dockerfile=/source \
    --opt "filename=$dockerfile" --opt platform=linux/amd64 \
    --output "type=docker,name=$image,dest=/output/$service-$tag.tar"
  require_space / 4
  docker load --input "$artifact"
  docker image inspect "$image" --format '{{.RepoTags}} {{.Id}} {{.Os}}/{{.Architecture}}' >> "$report"
  rm -f -- "$artifact"
done
df -h / "$build_dir" >> "$report"
cat "$report"
