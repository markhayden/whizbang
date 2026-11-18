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
# Then it builds the tests, and runs the model tier under scripts/memory-watch.sh (24 GiB of
# resident memory for the whole tree, then a kill) and, inside it, scripts/thread-guard.sh at
# the limit it is given (200 from the recipe), one test at a time, in the nextest
# profile `model-tier` (.config/nextest.toml: a test is ended at 5 minutes, so the two take
# 10 at most). The tests read nothing of their environment (scripts/check-binary-start.sh),
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
shared=$target/tmp/model-tier
mkdir -p "$shared/out"
rm -f "$shared/out/phrase.txt" "$shared/out/second.txt" "$shared/out/places.txt" "$shared/out/tiny.txt"
printf '%s\n' "$dir" >"$shared/models-dir"

cd "$root"
scripts/thread-guard.sh cargo nextest run --profile model-tier -p whizbang --test it --no-run
scripts/memory-watch.sh scripts/thread-guard.sh cargo nextest run --profile model-tier -p whizbang --test it \
  --run-ignored only --test-threads 1 --no-fail-fast --no-capture -E 'test(/^model_tier::/)'
}
main "$@"; exit
