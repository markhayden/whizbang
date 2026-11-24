#!/usr/bin/env bash
# The model tier (docs/plans/m1c-local-pack.md, task R2b; decision D15): the tests that load
# a real model, `model_tier::` in crates/whizbang/tests/it/model_tier.rs. Never in CI.
#
#   just test-models
#
# which runs, under the heavy lock with the thread guard's limit at 200:
#
#   WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh test-models scripts/test-models.sh
#
# It refuses, before it builds or starts anything, unless the local environment names the
# models directory: WHIZBANG_MODELS_DIR, an absolute path to a folder that holds the model
# installed by `whizbang models get sam3` and the pack installed by `whizbang setup --extra
# torch`, and `tiny` by `whizbang models get tiny` (plan 1D, task T1). Set it where it
# lasts, as AGENTS.md says of the word list.
#
# One argument may name a part of the tier: `model_tier` (the tests of SAM 3's and Tiny's
# requests, which scripts/model-evidence.sh runs), `parity` (the parity fixtures on the GPU and
# the CPU, plan 1D, task V1) or `coverage` (the timings of plan 1D, task V1); with none, all
# three run. Any other argument is refused before anything runs.
#
# `parity-remote` (plan m6, task D1; run by task L1) selects SAM 3's parity fixtures asked of a
# remote worker, `remote_parity::`, and nothing else; no other selection names it, the local
# leg of a release's among them. It needs no models directory: it refuses unless
# WHIZBANG_REMOTE_CONFIG names, by an absolute path, a config whose segment is of the kind
# `remote`; WHIZBANG_REMOTE_ENV_FILE, when set, names the env file that holds its token. Both
# reach the test as files of cargo's folder for tests' scratch, as the models directory does.
#
# `--candidate` as the first argument measures the staged tree, not the checkout (plan m6,
# task P1, item 5): scripts/candidate.sh prepares it under `<target>/tmp/candidate` (the
# index exported, its binary built, its pack installed into a models directory of its own
# with SAM 3 and Tiny linked in from WHIZBANG_MODELS_DIR, and that pack's sources held to the
# tree's), and the exported tree's own copy of this script runs the tier against it. Heavy.
#
# `--list` as the first argument runs no test: it prints the names of the tests the filter
# names, one a line, as `cargo nextest list` gives them, and needs no models directory.
# The local leg of a release (scripts/prerelease-local.sh) lists them before the run and
# holds the run's report to that list. The run writes its report, as the fast tests do:
# `<target>/nextest/model-tier/junit.xml` (.config/nextest.toml).
#
# Then it builds the tests, and runs the model tier under scripts/memory-watch.sh (24 GiB of
# resident memory for the whole tree, then a kill) and, inside it, scripts/thread-guard.sh at
# the limit it is given (200 from the recipe), one test at a time, in the nextest
# profile `model-tier` (.config/nextest.toml: a test is ended at 5 minutes, and the parity test at
# 10). The tests read nothing of their environment (scripts/check-binary-start.sh),
# so the models directory reaches them in one file of cargo's folder for tests' scratch,
# `<target>/tmp/model-tier/models-dir`, written here; each writes what it measured beside
# it, in `model-tier/out/`, which scripts/model-evidence.sh reads. WHIZBANG_MEMORY_REPORT,
# when set, is handed on to the watch.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "test-models: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

refuse() {
  echo "test-models: $1" >&2
  exit 2
}

candidate=0
if [ "${1-}" = --candidate ]; then
  candidate=1
  shift
fi
list=0
if [ "${1-}" = --list ]; then
  list=1
  shift
fi
remote=0
case "${1-}" in
  "") filter='test(/^(model_tier|parity|coverage)::/)' ;;
  model_tier | parity | coverage) filter="test(/^$1::/)" ;;
  parity-remote) filter='test(/^remote_parity::/)' remote=1 ;;
  *) refuse "the one argument names a part of the tier: model_tier, parity, coverage or parity-remote" ;;
esac
[ "$#" -le 1 ] || refuse "the one argument names a part of the tier: model_tier, parity, coverage or parity-remote"
[ "$candidate" = 0 ] || [ "$list" = 0 ] || refuse "--candidate and --list are not given together"
[ "$candidate" = 0 ] || [ "$remote" = 0 ] || refuse "--candidate and parity-remote are not given together"

if [ "$list" = 1 ]; then
  cd "$root"
  scripts/thread-guard.sh cargo nextest list --profile model-tier -p whizbang --test it --run-ignored only \
    --message-format oneline -E "$filter"
  exit
fi

# A remote config: an absolute file, one of whose tables of segment is of the kind remote.
if [ "$remote" = 1 ]; then
  config=${WHIZBANG_REMOTE_CONFIG:-}
  [ -n "$config" ] || refuse "parity-remote needs WHIZBANG_REMOTE_CONFIG, the config whose segment is remote"
  case "$config" in
    /*) ;;
    *) refuse "WHIZBANG_REMOTE_CONFIG must be an absolute path" ;;
  esac
  case "$config" in
    *$'\n'*) refuse "WHIZBANG_REMOTE_CONFIG holds a line break" ;;
  esac
  [ -f "$config" ] || refuse "WHIZBANG_REMOTE_CONFIG names no file"
  awk '
    /^[[:space:]]*\[/ { segment = ($0 ~ /^[[:space:]]*\[backends\.segment(\.[A-Za-z0-9_-]+)?\][[:space:]]*(#.*)?$/) }
    segment && /^[[:space:]]*kind[[:space:]]*=[[:space:]]*"remote"[[:space:]]*(#.*)?$/ { found = 1 }
    END { exit found ? 0 : 1 }
  ' "$config" || refuse "WHIZBANG_REMOTE_CONFIG names a config whose segment is not of the kind remote"
  env_file=${WHIZBANG_REMOTE_ENV_FILE:-}
  if [ -n "$env_file" ]; then
    case "$env_file" in
      /*) ;;
      *) refuse "WHIZBANG_REMOTE_ENV_FILE must be an absolute path" ;;
    esac
    case "$env_file" in
      *$'\n'*) refuse "WHIZBANG_REMOTE_ENV_FILE holds a line break" ;;
    esac
    [ -f "$env_file" ] || refuse "WHIZBANG_REMOTE_ENV_FILE names no file"
  fi
  target=${CARGO_TARGET_DIR:-$root/target}
  case "$target" in
    /*) ;;
    *) target=$root/$target ;;
  esac
  shared=$target/tmp/model-tier
  mkdir -p "$shared/out"
  rm -f "$shared/out/remote-parity.txt" "$shared/remote-env-file"
  printf '%s\n' "$config" >"$shared/remote-config"
  [ -z "$env_file" ] || printf '%s\n' "$env_file" >"$shared/remote-env-file"
  cd "$root"
  scripts/thread-guard.sh cargo nextest run --profile model-tier -p whizbang --test it --no-run
  scripts/thread-guard.sh cargo nextest run --profile model-tier -p whizbang --test it \
    --run-ignored only --test-threads 1 --no-fail-fast --no-capture -E "$filter"
  exit
fi

dir=${WHIZBANG_MODELS_DIR:-}
[ -n "$dir" ] ||
  refuse "no models directory is named: set WHIZBANG_MODELS_DIR in the local environment to the folder that holds sam3 and the pack"
case "$dir" in
  /*) ;;
  *) refuse "WHIZBANG_MODELS_DIR must be an absolute path" ;;
esac
[ -d "$dir/models" ] && [ -d "$dir/pack" ] ||
  refuse "WHIZBANG_MODELS_DIR names no models directory with a model and the pack installed in it"
case "$dir" in
  *$'\n'*) refuse "WHIZBANG_MODELS_DIR holds a line break" ;;
esac

target=${CARGO_TARGET_DIR:-$root/target}
case "$target" in
  /*) ;;
  *) target=$root/$target ;;
esac
if [ "$candidate" = 1 ]; then
  work=$target/tmp/candidate
  rm -rf "$work"
  mkdir -p "$work"
  "$root/scripts/candidate.sh" prepare "$work"
  exec env CARGO_TARGET_DIR="$work/target" WHIZBANG_MODELS_DIR="$work/models" "$work/tree/scripts/test-models.sh" "$@"
fi
shared=$target/tmp/model-tier
mkdir -p "$shared/out"
rm -f "$shared/out/phrase.txt" "$shared/out/second.txt" "$shared/out/places.txt" "$shared/out/tiny.txt" \
  "$shared/out/parity.txt" "$shared/out/coverage.txt"
printf '%s\n' "$dir" >"$shared/models-dir"

cd "$root"
scripts/thread-guard.sh cargo nextest run --profile model-tier -p whizbang --test it --no-run
scripts/memory-watch.sh scripts/thread-guard.sh cargo nextest run --profile model-tier -p whizbang --test it \
  --run-ignored only --test-threads 1 --no-fail-fast --no-capture -E "$filter"
}
main "$@"; exit
