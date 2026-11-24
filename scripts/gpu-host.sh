#!/usr/bin/env bash
# The GPU host of the kind `remote`, on Modal (docs/plans/m6-http-and-remote-hosting.md, task
# D1, item 3; ADR 0046; docs/guides/gpu-host.md). The orchestrator's alone, on the owner's Mac,
# and a customer's for a host of their own: never in CI, never by a builder. It is not on the
# allowed list, so it asks first.
#
#   scripts/gpu-host.sh deploy <commit> --config <file> --endpoint-file <file>
#   scripts/gpu-host.sh weights --models-dir <dir>
#   scripts/gpu-host.sh probe --config <file> --env-file <file>
#   scripts/gpu-host.sh down
#   scripts/gpu-host.sh lock
#
# - `deploy` deploys deploy/modal/sam3_host.py from this checkout, which must be clean and at
#   <commit> (40 hex digits). It refuses, before anything is deployed, unless the three limits
#   of one attempt in the tree it deploys (`LIMITS`: startup_timeout, timeout and
#   scaledown_window, read back from the file's own table, which the GPU class takes whole)
#   are each there, `HOST_LIMIT_S` is their sum, and the config's one `host_limit_s` is the
#   same sum; and unless Modal's list of apps, read whole, says every app of the name is
#   stopped and Modal lists no container of it, or, for the first deploy, holds no app of the
#   name. An app deployed with no container running is not stopped: run `down` first. So two
#   generations never run at once. The front's endpoint (which names the workspace) is
#   written to the endpoint file, outside the repo, and not printed.
# - `weights` checks SAM 3's installation in the models directory with `whizbang models verify
#   sam3`, every file read whole against the catalogue, then copies that installation's
#   folder into the volume at the same place. No Hugging Face sign-in is used on the host.
# - `probe` is `whizbang backends status --probe` under the config and the env file given.
# - `down` stops the app, so it takes no new input, then asks every WHIZBANG_GPU_HOST_LOOK_S
#   seconds (10) until Modal lists no container of it, at most WHIZBANG_GPU_HOST_DOWN_S
#   seconds (900, past one attempt's 600); it fails when one is left.
# - `lock` writes deploy/gpu-host/requirements.txt again from requirements.in, with `uv`,
#   from pypi.org alone, for Linux on x86-64, every file pinned by its hash.
#
# It prints no token and no account, reads no env file itself (it hands --env-file to
# whizbang), and never reads Modal's token file: Modal's client reads its own. `modal`,
# `whizbang` and `uv` are found on the PATH. Modal's client is at least MODAL_LEAST (its JSON
# keys, `startup_timeout` and `Dict.put(..., skip_if_exists=True)` answering whether it put
# were read from that version's source); an older one is refused before it is asked anything.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "gpu-host: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
# No word this script splits is a pattern: app ids are never globbed.
set -f

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
deployment=deploy/modal/sam3_host.py
app=whizbang-sam3
volume=whizbang-models
model=sam3
# The lock's newest file: a lock made again gives the same files.
exclude_newer=2026-11-16T00:00:00Z
MODAL_LEAST=1.6.1

refuse() {
  echo "gpu-host: $1" >&2
  exit 2
}

# Modal's client, at least MODAL_LEAST: `modal --version` says `modal client version: X.Y.Z`.
modal_recent() {
  local said version
  said=$(modal --version 2>/dev/null) || refuse "Modal's client cannot be asked its version: install it (docs/guides/gpu-host.md)"
  version=$(printf '%s\n' "$said" | sed -n 's/^modal client version: \([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*$/\1/p' | head -n 1)
  [ -n "$version" ] || refuse "Modal's client did not say its version"
  printf '%s\n%s\n' "$version" "$MODAL_LEAST" | python3 -I -c '
import sys
have, least = (tuple(int(p) for p in line.split(".")) for line in sys.stdin.read().split())
sys.exit(0 if have >= least else 1)
' || refuse "Modal's client is $version; this deployment needs $MODAL_LEAST or later"
}

usage="deploy <commit> --config <file> --endpoint-file <file>, weights --models-dir <dir>, probe --config <file> --env-file <file>, down, or lock"
[ "$#" -ge 1 ] || refuse "say what to do: $usage"
command=$1
shift

config="" env_file="" endpoint_file="" models_dir="" commit=""
if [ "$command" = deploy ]; then
  [ "$#" -ge 1 ] || refuse "deploy names the commit to deploy"
  commit=$1
  shift
fi
while [ "$#" -gt 0 ]; do
  [ "$#" -ge 2 ] || refuse "$1 needs a value"
  case "$1" in
    --config) config=$2 ;;
    --env-file) env_file=$2 ;;
    --endpoint-file) endpoint_file=$2 ;;
    --models-dir) models_dir=$2 ;;
    *) refuse "an option it does not take: $usage" ;;
  esac
  shift 2
done

absolute_file() { # <what> <path>
  case "$2" in
    /*) ;;
    *) refuse "$1 must be an absolute path" ;;
  esac
  [ -f "$2" ] || refuse "$1 names no file"
}

# The apps of the name, as Modal lists them, read whole: one line `<app id> <state>` each.
# Anything it cannot read whole refuses.
apps_of_the_name() {
  local listed
  listed=$(modal app list --json) || refuse "Modal's list of apps cannot be read"
  printf '%s' "$listed" | python3 -I -c '
import json, sys
name = sys.argv[1]
try:
    apps = json.load(sys.stdin)
    assert isinstance(apps, list)
    for app in apps:
        assert isinstance(app, dict)
        ident, said, state = app["app_id"], app["description"], app["state"]
        assert all(isinstance(v, str) for v in (ident, said, state))
        if said == name:
            # The state is printed with its spaces made underscores: "stopping..." and
            # "ephemeral (detached)" are states too.
            assert ident and " " not in ident and state
            print(ident, state.replace(" ", "_"))
except (ValueError, AssertionError, KeyError, TypeError):
    sys.exit(1)
' "$app" || refuse "Modal's list of apps cannot be read whole: deploy nothing"
}

# How many containers Modal lists of these app ids; anything it cannot read whole refuses.
containers_of() { # <app id>...
  local listed
  listed=$(modal container list --json) || refuse "Modal's list of containers cannot be read"
  printf '%s' "$listed" | python3 -I -c '
import json, sys
ids = set(sys.argv[1:])
try:
    found = json.load(sys.stdin)
    assert isinstance(found, list)
    count = 0
    for container in found:
        assert isinstance(container, dict) and isinstance(container["app_id"], str)
        count += container["app_id"] in ids
    print(count)
except (ValueError, AssertionError, KeyError, TypeError):
    sys.exit(1)
' "$@" || refuse "Modal's list of containers cannot be read whole"
}

# The three limits of one attempt, read back from the deployment's own table: their sum,
# or a refusal naming what is wrong.
limits_sum() { # <file>
  python3 -I -c '
import ast, sys
tree = ast.parse(open(sys.argv[1], encoding="utf-8").read())
values = {}
for node in tree.body:
    if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
        try:
            values[node.targets[0].id] = ast.literal_eval(node.value)
        except ValueError:
            pass
limits = values.get("LIMITS")
if not isinstance(limits, dict):
    sys.exit("the deployment has no table LIMITS")
for name in ("startup_timeout", "timeout", "scaledown_window"):
    value = limits.get(name)
    if not (isinstance(value, int) and not isinstance(value, bool) and value > 0):
        sys.exit("the limit %s is missing from LIMITS" % name)
if set(limits) != {"startup_timeout", "timeout", "scaledown_window"}:
    sys.exit("LIMITS holds something other than the three limits")
total = sum(limits.values())
if values.get("HOST_LIMIT_S") != total:
    sys.exit("HOST_LIMIT_S is not the sum of LIMITS")
# The GPU class takes the table whole, and nothing asks for a retry.
whole = False
for node in ast.walk(tree):
    if isinstance(node, ast.Call):
        for keyword in node.keywords:
            if keyword.arg is None and isinstance(keyword.value, ast.Name) and keyword.value.id == "LIMITS":
                whole = whole or (isinstance(node.func, ast.Attribute) and node.func.attr == "cls")
            on_the_class = isinstance(node.func, ast.Attribute) and node.func.attr == "cls"
            if keyword.arg == "retries" and not (isinstance(keyword.value, ast.Constant) and keyword.value.value == 0):
                sys.exit("the deployment asks for a retry")
            if on_the_class and keyword.arg in ("startup_timeout", "timeout", "scaledown_window"):
                sys.exit("the deployment sets %s outside LIMITS" % keyword.arg)
if not whole:
    sys.exit("the GPU class does not take LIMITS whole")
print(total)
' "$1"
}

# The config's one host_limit_s.
config_limit() { # <file>
  local found
  found=$(grep -E '^[[:space:]]*host_limit_s[[:space:]]*=' "$1" || true)
  [ -n "$found" ] || refuse "the config names no host_limit_s"
  [ "$(printf '%s\n' "$found" | wc -l | tr -d ' ')" = 1 ] || refuse "the config names host_limit_s more than once"
  printf '%s\n' "$found" | sed -E -n 's/^[[:space:]]*host_limit_s[[:space:]]*=[[:space:]]*([0-9]+)[[:space:]]*(#.*)?$/\1/p'
}

case "$command" in
  deploy)
    case "$commit" in
      *[!0-9a-f]* | "") refuse "the commit is 40 lower-case hex digits" ;;
    esac
    [ "${#commit}" = 40 ] || refuse "the commit is 40 lower-case hex digits"
    [ -n "$config" ] || refuse "deploy needs --config, the config whose host_limit_s the deployment must match"
    absolute_file "--config" "$config"
    [ -n "$endpoint_file" ] || refuse "deploy needs --endpoint-file, where the front's endpoint is written"
    case "$endpoint_file" in
      /*) ;;
      *) refuse "--endpoint-file must be an absolute path" ;;
    esac
    [ ! -L "$endpoint_file" ] || refuse "--endpoint-file is a link: name the file itself"
    at=$(cd "$(dirname "$endpoint_file")" 2>/dev/null && pwd -P) || refuse "the folder of --endpoint-file is not there"
    case "$at/" in
      "$root"/*) refuse "--endpoint-file must be outside the repo: the endpoint names the workspace" ;;
    esac
    cd "$root"
    [ -z "$(git status --porcelain)" ] || refuse "the checkout has changes: deploy only from a clean checkout"
    [ "$(git rev-parse HEAD)" = "$commit" ] || refuse "the checkout is not at $commit"
    # What the deployment uploads holds nothing git ignores: no cache, no file of a test.
    ignored=$(git status --porcelain --ignored -- pack/src/whizbang_pack deploy | grep -c '^!!' || true)
    [ "$ignored" = 0 ] ||
      refuse "$ignored path(s) git ignores are under pack/src/whizbang_pack or deploy: remove them (git clean -fdX -- pack/src deploy)"
    modal_recent
    total=$(limits_sum "$deployment" 2>&1) || refuse "in the deployment: ${total:-its limits cannot be read}"
    wanted=$(config_limit "$config")
    [ -n "$wanted" ] || refuse "the config's host_limit_s is not a whole number"
    [ "$wanted" = "$total" ] ||
      refuse "the config's host_limit_s is $wanted, and the deployment's limits of one attempt come to $total"
    listed=$(apps_of_the_name)
    if [ -n "$listed" ]; then
      ids=$(printf '%s\n' "$listed" | awk '$2 != "stopped" { print "not" } $2 == "stopped" { print $1 }')
      case "$ids" in
        *not*) refuse "the app $app is not stopped (an app scaled to no container is not): run scripts/gpu-host.sh down first" ;;
      esac
      # shellcheck disable=SC2086
      left=$(containers_of $ids)
      [ "$left" = 0 ] || refuse "the app $app is stopped and $left container(s) of it are left: wait, and run down again"
    fi
    said=$(modal deploy "$deployment" 2>&1) || {
      # Its last lines, with every address that names the workspace masked.
      printf '%s\n' "$said" | tail -n 20 |
        sed -E -e 's#https://[A-Za-z0-9.-]+\.modal\.run#<endpoint>#g' -e 's#https://modal\.com/(apps|id|settings)/[^ ]*#<modal page>#g' >&2
      refuse "the deploy failed"
    }
    endpoint=$(printf '%s\n' "$said" | grep -o -E "https://[A-Za-z0-9-]+--$app-front[A-Za-z0-9-]*\.modal\.run" | head -n 1 || true)
    [ -n "$endpoint" ] || refuse "the deploy named no endpoint of the front"
    umask 077
    printf '%s\n' "$endpoint" >"$endpoint_file"
    echo "gpu-host: deployed $commit as $app; limits of one attempt $total seconds, as the config's host_limit_s"
    echo "gpu-host: the front's endpoint is in $endpoint_file"
    ;;
  weights)
    [ -n "$models_dir" ] || refuse "weights needs --models-dir, the models directory that holds sam3"
    case "$models_dir" in
      /*) ;;
      *) refuse "--models-dir must be an absolute path" ;;
    esac
    modal_recent
    said=$(whizbang models verify "$model" --models-dir "$models_dir") || refuse "whizbang models verify $model refused: copy nothing"
    identity=$(printf '%s\n' "$said" | sed -E -n "s/^whole $model: sha256:([0-9a-f]{64})\$/\\1/p")
    [ -n "$identity" ] || refuse "whizbang models verify $model did not say the installation is whole"
    folder=models/$model/installations/$identity
    [ -f "$models_dir/$folder/RECORD" ] || refuse "the installation has no record beside its files"
    modal volume put "$volume" "$models_dir/$folder" "/$folder"
    echo "gpu-host: copied the installation $identity of $model into the volume $volume"
    ;;
  probe)
    [ -n "$config" ] && [ -n "$env_file" ] || refuse "probe needs --config and --env-file"
    absolute_file "--config" "$config"
    absolute_file "--env-file" "$env_file"
    whizbang backends status --probe --config "$config" --env-file "$env_file"
    ;;
  down)
    look=${WHIZBANG_GPU_HOST_LOOK_S:-10}
    within=${WHIZBANG_GPU_HOST_DOWN_S:-900}
    case "$look$within" in
      *[!0-9]* | "") refuse "WHIZBANG_GPU_HOST_LOOK_S and WHIZBANG_GPU_HOST_DOWN_S are whole seconds" ;;
    esac
    modal_recent
    listed=$(apps_of_the_name)
    [ -n "$listed" ] || refuse "Modal lists no app $app"
    if printf '%s\n' "$listed" | awk '$2 != "stopped"' | grep . >/dev/null; then
      modal app stop -y "$app" || refuse "modal app stop failed"
    fi
    ids=$(apps_of_the_name | awk '{ print $1 }')
    waited=0
    while true; do
      # shellcheck disable=SC2086
      left=$(containers_of $ids)
      [ "$left" != 0 ] || break
      [ "$waited" -lt "$within" ] || refuse "$left container(s) of $app are left after $within seconds"
      sleep "$look"
      waited=$((waited + look))
      [ "$look" != 0 ] || waited=$((waited + 1))
    done
    echo "gpu-host: $app is stopped, and Modal lists no container of it"
    ;;
  lock)
    cd "$root"
    uv pip compile --no-config --index-url https://pypi.org/simple --emit-index-url \
      --exclude-newer "$exclude_newer" --python-platform x86_64-unknown-linux-gnu --python-version 3.12 \
      --generate-hashes --no-header deploy/gpu-host/requirements.in -o deploy/gpu-host/requirements.txt
    ;;
  *) refuse "say what to do: $usage" ;;
esac
}
main "$@"; exit
