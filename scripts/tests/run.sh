#!/usr/bin/env bash
# Runs every scripts/tests/test-*.sh and fails if any of them fails. `just test-scripts`.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

failed=0
for t in scripts/tests/test-*.sh; do
  if out=$(bash "$t" 2>&1); then
    echo "ok    $t"
  else
    echo "FAIL  $t"
    echo "$out" | sed 's/^/      /'
    failed=1
  fi
  # A test may report that it skipped an opt-in part.
  echo "$out" | grep '^skip:' | sed 's/^/      /' || true
done
exit "$failed"
