#!/usr/bin/env bash
# Runs a command, and kills it when one of its processes holds too many threads.
#
#   scripts/thread-guard.sh <command...>
#
# Why: a test that parked threads until the system refused one took a machine down, twice,
# with a kernel panic. Nothing a test does is worth that, so every recipe that runs tests
# runs them through this (`just test`, `scripts/timing.sh`, and every command run through
# `scripts/heavy.sh`).
#
# While the command runs, about once a second, every process of its tree (the command and
# everything it started, by parent) is counted. When any one process holds more threads
# than the limit, the whole tree is stopped and then killed with SIGKILL, a line says which
# process and how many, and the exit status is 1. Otherwise the exit status is the
# command's.
#
# - The limit is 1000 threads in one process; WHIZBANG_THREAD_LIMIT changes it.
# - WHIZBANG_THREAD_POLL is the seconds between two counts (default 1), and
#   WHIZBANG_THREAD_TEST_HOOK is a command run after each count. The tests use both.
# - Threads are counted by `ps`: `ps -M` on macOS, which prints a line for every thread, and
#   the `nlwp` column on Linux. A system on which this cannot count its own threads is
#   refused before the command runs: a guard that cannot see is no guard.
#
# What remains: a process whose parent has ended belongs to no tree and is not counted, and
# a process can pass the limit and do its harm between two counts. This is a net, and no
# licence: a test still bounds, with a number written in it, every thread it makes.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "thread-guard: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "usage: scripts/thread-guard.sh <command...>" >&2
  exit 2
fi

limit=${WHIZBANG_THREAD_LIMIT:-1000}
poll=${WHIZBANG_THREAD_POLL:-1}
echo "$limit" | grep -E '^[1-9][0-9]*$' >/dev/null ||
  { echo "thread-guard: the limit must be a whole number of threads, 1 or more: $limit" >&2; exit 2; }
echo "$poll" | grep -E '^[0-9]+(\.[0-9]+)?$' >/dev/null ||
  { echo "thread-guard: the seconds between two counts must be a number: $poll" >&2; exit 2; }
# The command is looked at every tenth of a second, so that its end is seen at once, and
# counted every so many looks.
every=$(awk -v p="$poll" 'BEGIN { n = int(p * 10 + 0.5); print (n < 1 ? 1 : n) }')

# processes: one line for every process there is: its id, its parent's id, its threads.
if [ "$(uname -s)" = "Darwin" ]; then
  processes() {
    # With -M every thread has a line, and `-o pid=` puts the process id last on each.
    { ps -A -o pid=,ppid= | sed 's/^/P /'; ps -M -A -o pid= | sed 's/^/T /'; } | awk '
      $1 == "P" { parent[$2] = $3 }
      $1 == "T" && $NF ~ /^[0-9]+$/ { threads[$NF] += 1 }
      END { for (p in parent) print p, parent[p], threads[p] + 0 }'
  }
else
  processes() { ps -e -o pid=,ppid=,nlwp=; }
fi

# tree <root> [limit]: reads the lines of `processes`. Without a limit, prints the id of
# every process of the tree under <root>, <root> first. With one, prints the id and the
# threads of the first process of the tree that holds more.
tree() {
  awk -v root="$1" -v limit="${2:-}" '
    { parent[$1] = $2; threads[$1] = $3; ids[n++] = $1 }
    END {
      if (!(root in parent)) exit
      inside[root] = 1; found[m++] = root
      do {
        grew = 0
        for (i = 0; i < n; i++) {
          p = ids[i]
          if (!(p in inside) && (parent[p] in inside)) { inside[p] = 1; found[m++] = p; grew = 1 }
        }
      } while (grew)
      for (i = 0; i < m; i++) {
        if (limit == "") print found[i]
        else if (threads[found[i]] + 0 > limit + 0) { print found[i], threads[found[i]]; exit }
      }
    }'
}

own=$(processes | awk -v me="$$" '$1 == me { print $3 }')
if ! echo "$own" | grep -E '^[1-9][0-9]*$' >/dev/null; then
  echo "thread-guard: cannot count the threads of a process on this system; refusing to run the command" >&2
  exit 2
fi

"$@" <&0 &
child=$!

# kill_tree: stops every process under the command, looking again until no new one
# appears (a stopped process starts nothing, and keeps its children), then kills them all.
kill_tree() {
  local ids="" now pass
  for pass in 1 2 3 4 5; do
    now=$(processes | tree "$child")
    [ -n "$now" ] || break
    # shellcheck disable=SC2086
    kill -STOP $now 2>/dev/null || true
    [ "$now" != "$ids" ] || break
    ids=$now
  done
  # shellcheck disable=SC2086
  [ -z "$ids" ] || kill -KILL $ids 2>/dev/null || true
  kill -KILL "$child" 2>/dev/null || true
}
trap 'kill_tree; exit 130' INT
trap 'kill_tree; exit 143' TERM

looks=0
while kill -0 "$child" 2>/dev/null; do
  sleep 0.1
  looks=$((looks + 1))
  [ $((looks % every)) -eq 0 ] || continue
  over=$(processes | tree "$child" "$limit")
  [ -z "${WHIZBANG_THREAD_TEST_HOOK:-}" ] || $WHIZBANG_THREAD_TEST_HOOK
  [ -n "$over" ] || continue
  # The process that is over the limit is stopped before anything else is done.
  kill -STOP "${over%% *}" 2>/dev/null || true
  name=$(ps -o comm= -p "${over%% *}" 2>/dev/null || true)
  kill_tree
  wait "$child" 2>/dev/null || true
  echo "thread-guard: process ${over%% *} (${name:-unknown}) held ${over##* } threads, over the limit of $limit in one process (WHIZBANG_THREAD_LIMIT)" >&2
  echo "thread-guard: killed the command and everything it started: $*" >&2
  exit 1
done

status=0
wait "$child" || status=$?
exit "$status"
