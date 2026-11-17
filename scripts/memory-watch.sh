#!/usr/bin/env bash
# Runs a command, and kills it when its processes together hold too much resident memory.
#
#   scripts/memory-watch.sh <command...>
#
# Why: a real model loaded in the pack's worker holds gigabytes, and nothing in the standard
# library caps a process's memory in a way macOS is documented to hold to. So the model
# tier runs under this watch, beside scripts/thread-guard.sh: task R1's trial of the
# runtime, `just test-models` and `scripts/model-evidence.sh` (docs/plans/m1c-local-pack.md,
# "Rules", the row on the worker's resident memory).
#
# While the command runs, about five times a second (WHIZBANG_MEMORY_POLL seconds between
# two counts, default 0.2), every process of its tree (the command and everything it
# started, by parent) is read: its resident size and its threads. When the resident sizes of
# the tree add up to more than the limit, the whole tree is stopped and then killed with
# SIGKILL, a line says how much it held, and the exit status is 1. Otherwise the exit
# status is the command's.
#
# - The limit is 24 GiB (25165824 KiB); WHIZBANG_MEMORY_LIMIT_KIB changes it, for this
#   script's test.
# - When the command ends, one line on the error output, and the same as `name=value`
#   lines in the file WHIZBANG_MEMORY_REPORT names (when it is set), say what was seen: the
#   limit; the peak resident size of the tree; the most threads in one process; the most
#   processes in the tree at once; how many processes besides the command were ever seen in
#   it; and how many of those were still running when the command ended (a process the
#   command started that outlived it).
# - WHIZBANG_MEMORY_TEST_HOOK is a command run after each count. The test uses it.
#
# What this does not do. A process that grows faster than a count is seen a fifth of a
# second later; a machine with 24 GiB to spare survives that. It does not see a process
# whose parent has ended: that process is in no tree. A process id counted as still running
# after the command ended may, in principle, be another process by then. A count that
# cannot be made, twice running while the command runs, kills the tree and fails, as the
# thread guard does: a watch that cannot see is no watch.
#
# Resident size is read by `ps` (`rss`, in KiB), threads by `ps -M` on macOS and the `nlwp`
# column on Linux. The command is waited for after a kill for five seconds at most.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "memory-watch: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "usage: scripts/memory-watch.sh <command...>" >&2
  exit 2
fi

limit=${WHIZBANG_MEMORY_LIMIT_KIB:-25165824}
poll=${WHIZBANG_MEMORY_POLL:-0.2}
report=${WHIZBANG_MEMORY_REPORT:-}
echo "$limit" | grep -E '^[1-9][0-9]*$' >/dev/null ||
  { echo "memory-watch: the limit must be a whole number of KiB, 1 or more: $limit" >&2; exit 2; }
echo "$poll" | grep -E '^[0-9]+(\.[0-9]+)?$' >/dev/null ||
  { echo "memory-watch: the seconds between two counts must be a number: $poll" >&2; exit 2; }

# The command is looked at every tenth of a second, so that its end is seen at once, and
# counted every so many looks.
every=$(awk -v p="$poll" 'BEGIN { n = int(p * 10 + 0.5); print (n < 1 ? 1 : n) }')

# processes: one line for every process there is: its id, its parent's id, its threads,
# its resident size in KiB. Fails when `ps` does.
if [ "$(uname -s)" = "Darwin" ]; then
  processes() {
    local listed
    # With -M every thread has a line, ending in the process's parent, id and resident size.
    listed=$(ps -M -A -o ppid=,pid=,rss=) || return 1
    printf '%s\n' "$listed" | awk '
      $NF ~ /^[0-9]+$/ && $(NF - 1) ~ /^[0-9]+$/ && $(NF - 2) ~ /^[0-9]+$/ {
        parent[$(NF - 1)] = $(NF - 2); threads[$(NF - 1)] += 1; rss[$(NF - 1)] = $NF
      }
      END { for (p in parent) print p, parent[p], threads[p], rss[p] }'
  }
else
  processes() {
    local listed
    listed=$(ps -e -o pid=,ppid=,nlwp=,rss=) || return 1
    printf '%s\n' "$listed" |
      awk '$1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ { print $1, $2, $3 + 0, $4 + 0 }'
  }
fi

# look <root>: one count of the tree under <root>. It sets `ids` to the ids of the tree,
# <root> first, `held` to the sum of their resident sizes in KiB, and `threads` to the most
# threads in one of them. It fails when the count cannot be trusted.
ids=""
held=0
threads=0
look() {
  local table found
  table=$(processes) || return 1
  found=$(printf '%s\n' "$table" | awk -v root="$1" '
    NF == 4 { parent[$1] = $2; th[$1] = $3; rss[$1] = $4; all[n++] = $1 }
    END {
      if (!(root in parent)) { print "bad"; exit }
      inside[root] = 1; found[m++] = root
      do {
        grew = 0
        for (i = 0; i < n; i++) {
          p = all[i]
          if (!(p in inside) && (parent[p] in inside)) { inside[p] = 1; found[m++] = p; grew = 1 }
        }
      } while (grew)
      list = ""; sum = 0; most = 0
      for (i = 0; i < m; i++) {
        p = found[i]
        list = list " " p; sum += rss[p]
        if (th[p] + 0 > most) most = th[p] + 0
      }
      print "good"; print list; print sum; print most
    }') || return 1
  [ "$(printf '%s\n' "$found" | sed -n 1p)" = "good" ] || return 1
  ids=$(printf '%s\n' "$found" | sed -n 2p)
  held=$(printf '%s\n' "$found" | sed -n 3p)
  threads=$(printf '%s\n' "$found" | sed -n 4p)
}

(exec "$@") <&0 &
child=$!

# What was seen: the peaks, and every process of the tree but the command.
peak=0
most_threads=0
most_processes=0
others=" "
last=""

# kill_tree: stops every process under the command, looking again until no new one
# appears, then kills them all, with those of the last good count.
kill_tree() {
  local known=$last pass
  for pass in 1 2 3 4 5; do
    look "$child" 2>/dev/null || break
    # shellcheck disable=SC2086
    kill -STOP $ids 2>/dev/null || true
    [ "$ids" != "$known" ] || break
    known=$ids
  done
  # shellcheck disable=SC2086
  kill -KILL $known "$child" 2>/dev/null || true
  for pass in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25; do
    kill -0 "$child" 2>/dev/null || break
    sleep 0.2 || true
  done
  if kill -0 "$child" 2>/dev/null; then
    echo "memory-watch: the command (process $child) is still there after it was killed" >&2
  else
    wait "$child" 2>/dev/null || true
  fi
}
trap 'kill_tree; exit 130' INT
trap 'kill_tree; exit 143' TERM

looks=0
while kill -0 "$child" 2>/dev/null; do
  sleep 0.1 || true
  looks=$((looks + 1))
  [ $((looks % every)) -eq 0 ] || continue
  counted=1
  look "$child" || counted=0
  if [ "$counted" = 0 ]; then
    kill -0 "$child" 2>/dev/null || break
    counted=1
    look "$child" || counted=0
  fi
  [ -z "${WHIZBANG_MEMORY_TEST_HOOK:-}" ] || $WHIZBANG_MEMORY_TEST_HOOK || true
  if [ "$counted" = 0 ]; then
    kill -0 "$child" 2>/dev/null || break
    kill_tree
    echo "memory-watch: the processes of the command could not be read, twice running, while it ran: a watch that cannot see is no watch" >&2
    echo "memory-watch: killed the command and everything it started: $*" >&2
    exit 1
  fi
  last=$ids
  [ "$held" -le "$peak" ] || peak=$held
  [ "$threads" -le "$most_threads" ] || most_threads=$threads
  count=0
  for p in $ids; do
    count=$((count + 1))
    [ "$p" = "$child" ] && continue
    case "$others" in *" $p "*) ;; *) others="$others$p " ;; esac
  done
  [ "$count" -le "$most_processes" ] || most_processes=$count
  [ "$held" -gt "$limit" ] || continue
  kill_tree
  echo "memory-watch: the command and what it started held $held KiB resident, over the limit of $limit KiB (WHIZBANG_MEMORY_LIMIT_KIB)" >&2
  echo "memory-watch: killed the command and everything it started: $*" >&2
  exit 1
done

status=0
wait "$child" || status=$?

seen=0
running=0
for p in $others; do
  seen=$((seen + 1))
  if kill -0 "$p" 2>/dev/null; then running=$((running + 1)); fi
done
echo "memory-watch: peak resident $peak KiB of the limit $limit KiB; most threads in one process $most_threads; most processes at once $most_processes; others seen $seen, still running $running" >&2
if [ -n "$report" ]; then
  printf '%s\n' "limit_kib=$limit" "peak_resident_kib=$peak" "most_threads=$most_threads" \
    "most_processes=$most_processes" "others_seen=$seen" "still_running=$running" >"$report"
fi
exit "$status"
}
main "$@"; exit
