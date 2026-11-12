#!/usr/bin/env bash
# The scans: what a push would publish is covered commit by commit, against what the remote
# has; nothing is skipped for looking binary, for its name or for being a link; the
# attribution check; the clean-room scan fails closed and prints counts only.
# The cases are in scripts/tests/scan/, one file per subject, each with its own throwaway
# repository. They run side by side, because each scan starts many small processes.
#
#   scripts/tests/test-scan.sh DIR   the parts of another folder (scripts/tests/test-syntax.sh)
#
# A part passes when it exits 0 and the last line it prints is `finished: <its path>`; see
# scripts/tests/run.sh for why exiting 0 is not enough.
source "$(dirname "$0")/lib.sh"

parts=()
pids=()
for part in "${1:-scripts/tests/scan}"/*.sh; do
  [ "${part##*/}" != common.sh ] || continue
  bash "$part" >"$tmp/${part##*/}.log" 2>&1 &
  parts+=("$part")
  pids+=($!)
done
failed=0
i=0
while [ "$i" -lt "${#parts[@]}" ]; do
  log="$tmp/${parts[$i]##*/}.log"
  if ! wait "${pids[$i]}"; then
    echo "in ${parts[$i]}:" >&2
    cat "$log" >&2
    failed=1
  elif [ "$(tail -n 1 "$log")" != "finished: ${parts[$i]}" ]; then
    echo "in ${parts[$i]}:" >&2
    cat "$log" >&2
    echo "failed: ${parts[$i]} exited 0 before its last line" >&2
    failed=1
  fi
  i=$((i + 1))
done
[ "$failed" -eq 0 ] || fail "the scan tests"

finished
