#!/usr/bin/env bash
# Runs every scripts/tests/test-*.sh and fails if any of them fails. `just test-scripts`.
#
#   scripts/tests/run.sh          this repo's script tests
#   scripts/tests/run.sh DIR      the test-*.sh of another folder (scripts/tests/test-syntax.sh)
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
if [ $# -gt 0 ]; then
  dir=$1
  syntax_dir=$1
fi

if ! scripts/syntax.sh "$syntax_dir"; then
  echo "FAIL  the syntax of the scripts under $syntax_dir; no test was run"
  exit 1
fi

failed=0
ran=0
for t in "$dir"/test-*.sh; do
  [ -f "$t" ] || continue
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
[ "$ran" -gt 0 ] || { echo "FAIL  no test under $dir"; exit 1; }
exit "$failed"
}
main "$@"; exit
