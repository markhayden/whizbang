#!/usr/bin/env bash
# The scans: what a push would publish is covered commit by commit, against what the remote
# has; nothing is skipped for looking binary, for its name or for being a link; the
# attribution check; the clean-room scan fails closed and prints counts only.
# The cases are in scripts/tests/scan/, one file per subject, each with its own throwaway
# repository. They run side by side, because each scan starts many small processes.
source "$(dirname "$0")/lib.sh"

parts=()
pids=()
for part in scripts/tests/scan/*.sh; do
  [ "${part##*/}" != common.sh ] || continue
  bash "$part" >"$tmp/${part##*/}.log" 2>&1 &
  parts+=("$part")
  pids+=($!)
done
failed=0
i=0
while [ "$i" -lt "${#parts[@]}" ]; do
  if ! wait "${pids[$i]}"; then
    echo "in ${parts[$i]}:" >&2
    cat "$tmp/${parts[$i]##*/}.log" >&2
    failed=1
  fi
  i=$((i + 1))
done
[ "$failed" -eq 0 ] || fail "the scan tests"
