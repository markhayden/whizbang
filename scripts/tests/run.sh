#!/usr/bin/env bash
# Runs every scripts/tests/test-*.sh and fails if any of them fails. `just test-scripts`.
#
#   scripts/tests/run.sh          this repo's script tests
#   scripts/tests/run.sh DIR      the test-*.sh of another folder (scripts/tests/test-syntax.sh)
#   scripts/tests/run.sh --only FILE...
#                                 these tests and no other: the part the local `just check`
#                                 chooses (scripts/script-tests-local.sh); a name that is
#                                 not a file fails
#
# WHIZBANG_SCRIPT_SHARD=<k>/<n> runs only the k-th of n shards of those tests, as CI's jobs
# `scripts (<runner>, shard <k>)` do, so that no one job takes the runner past the budget of
# pull-request CI (SPEC.md section 13): the tests, in their order, are dealt to the shards
# forth and back (1, 2, 2, 1, 1, 2, ...), which spreads the long ones without a list to keep,
# and the n shards together run each test once. The variable is not handed to the tests.
# Unset, as `just test-scripts` runs locally, every test runs.
#
# Before any test: every shell script under scripts/ (or DIR) must parse (scripts/syntax.sh).
# A test passes when it exits 0 and the last line it prints is `finished: <its path>`, which
# the `finished` of lib.sh prints as the last line of the file. Exiting 0 is not enough: the
# bash that ships with macOS (3.2) exits 0 from a file that stopped at a syntax error.
# This script sets no `EXIT` trap, so its own syntax error is a failure.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh. Its last lines are
# reached only after every test has run.
main() {
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2

dir=scripts/tests
syntax_dir=scripts
only=0
if [ "${1:-}" = --only ]; then
  shift
  [ $# -gt 0 ] || { echo "FAIL  --only names no test"; exit 1; }
  only=1
else
  if [ $# -gt 0 ]; then
    dir=$1
    syntax_dir=$1
  fi
  set -- "$dir"/test-*.sh
fi

shard=${WHIZBANG_SCRIPT_SHARD:-}
unset WHIZBANG_SCRIPT_SHARD
if [ -n "$shard" ]; then
  echo "$shard" | grep -E '^[1-9][0-9]*/[1-9][0-9]*$' >/dev/null ||
    { echo "FAIL  WHIZBANG_SCRIPT_SHARD must be <k>/<n>, as 1/2: $shard"; exit 2; }
  k=${shard%/*}
  n=${shard#*/}
  [ "$k" -le "$n" ] || { echo "FAIL  WHIZBANG_SCRIPT_SHARD names a shard past the last: $shard"; exit 2; }
  all=$#
  mine=()
  i=0
  for t in "$@"; do
    # Forth on even rounds of n, back on odd ones.
    if [ $((i / n % 2)) -eq 0 ]; then at=$((i % n + 1)); else at=$((n - i % n)); fi
    [ "$at" -ne "$k" ] || mine+=("$t")
    i=$((i + 1))
  done
  set -- ${mine[@]+"${mine[@]}"}
  echo "shard $shard: $# of $all tests"
fi

if ! scripts/syntax.sh "$syntax_dir"; then
  echo "FAIL  the syntax of the scripts under $syntax_dir; no test was run"
  exit 1
fi

failed=0
ran=0
for t in "$@"; do
  if [ ! -f "$t" ]; then
    [ "$only" = 1 ] || continue
    echo "FAIL  $t: no such test"
    failed=1
    continue
  fi
  ran=$((ran + 1))
  status=0
  out=$(bash "$t" 2>&1) || status=$?
  if [ "$status" -eq 0 ] && [ "$(printf '%s\n' "$out" | tail -n 1)" = "finished: $t" ]; then
    echo "ok    $t"
  else
    echo "FAIL  $t"
    printf '%s\n' "$out" | sed 's/^/      /'
    [ "$status" -ne 0 ] || echo "      failed: $t exited 0 before its last line (it did not print: finished: $t)"
    failed=1
  fi
  # A test may report that it skipped an opt-in part.
  printf '%s\n' "$out" | grep '^skip:' | sed 's/^/      /' || true
done
[ "$ran" -gt 0 ] || { echo "FAIL  no test under $dir${shard:+ in shard $shard}"; exit 1; }
exit "$failed"
}
main "$@"; exit
