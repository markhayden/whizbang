#!/usr/bin/env bash
# Runs a command, and kills it when one of its processes holds too many threads.
#
#   scripts/thread-guard.sh <command...>
#
# Why: a test that parked threads until the system refused one took a machine down, twice,
# with a kernel panic. Nothing a test does is worth that, so every recipe and script that
# runs tests runs them through this (`just test`, `just test-only`, `scripts/timing.sh`,
# `scripts/tool-record.sh`, the fixture run of `scripts/tests/test-timeout.sh`, and every
# command run through `scripts/heavy.sh`).
#
# While the command runs, about three times a second (0.2 seconds between two counts, and a
# count takes some hundredths of a second more on a busy machine), every process of its tree
# (the command and everything it started, by parent) is counted. When any one process holds more threads
# than the limit, the whole tree is stopped and then killed with SIGKILL, a line says which
# process and how many, and the exit status is 1. Otherwise the exit status is the
# command's.
#
# What this stops, and what it does not. Read this before relying on it.
#
# - It stops a process that grows slowly: one that passes the limit is seen within about a
#   third of a second.
# - It does NOT stop a tight loop that makes threads. Such a loop makes tens of thousands
#   of threads a second, so it is far past the limit, and past what a machine survives,
#   between two counts. What stops that is in the code, before the thread is made: the
#   tests make every thread through `whizbang_testkit::threads`, which refuses the 65th
#   live thread of a process, and `clippy.toml` refuses a test, or anything else, that
#   makes a thread another way; the product bounds the runs it starts (ADR 0008).
# - It does not see a process whose parent has ended: that process is in no tree.
# - On Linux it also lowers, for the command, the number of processes and threads its user
#   may hold (`ulimit -u`) to what the user holds when the guard starts and 4,096 more
#   (WHIZBANG_THREAD_HEADROOM): threads count toward that limit there, and the system then
#   refuses the next one itself, at once. When what the user holds cannot be counted, the
#   limit is left as it was. The limit is the user's, not the tree's, and it
#   does not hold root. macOS has no limit of that kind that counts threads, so there
#   the count is all this script has.
#
# A count that cannot be trusted is a failure, never a pass. A count is good when `ps`
# succeeded, the command is in what it printed, and every process of the tree that is not
# a zombie has at least one thread. Anything else, while the command still runs, is looked
# at once more, and then the tree is killed and the exit status is 1: a guard that cannot
# see is no guard. For the same reason a system on which this cannot count its own
# threads is refused before the command runs, with the exit status 2.
#
# - The limit is 300 threads in one process; WHIZBANG_THREAD_LIMIT changes it. The largest
#   process of a full `just check` holds under 20, and the product starts at most 256 runs.
# - WHIZBANG_THREAD_POLL is the seconds between two counts (default 0.2), and
#   WHIZBANG_THREAD_TEST_HOOK is a command run after each count. The tests use both.
# - Threads are counted by `ps`: `ps -M` on macOS, which prints a line for every thread, and
#   the `nlwp` column on Linux.
# - A guard inside a guard does nothing: the command it is given is told, in
#   WHIZBANG_THREAD_GUARD, which process watches it and with what limit and pause, and a
#   guard that finds itself under that process, which must be running this script, with its
#   own limit and pause, runs its command in its own place. One with another limit or pause
#   watches as well, and so does one whose variable names any other process: setting the
#   variable by hand switches nothing off.
#
# When the tree is killed, the processes of the last good count are killed with those found
# then: a process id from a third of a second ago can, in principle, be another process by
# now. The command is waited for after the kill for five seconds at most.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "thread-guard: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "usage: scripts/thread-guard.sh <command...>" >&2
  exit 2
fi

limit=${WHIZBANG_THREAD_LIMIT:-300}
poll=${WHIZBANG_THREAD_POLL:-0.2}
headroom=${WHIZBANG_THREAD_HEADROOM:-4096}
echo "$limit" | grep -E '^[1-9][0-9]*$' >/dev/null ||
  { echo "thread-guard: the limit must be a whole number of threads, 1 or more: $limit" >&2; exit 2; }
echo "$poll" | grep -E '^[0-9]+(\.[0-9]+)?$' >/dev/null ||
  { echo "thread-guard: the seconds between two counts must be a number: $poll" >&2; exit 2; }
echo "$headroom" | grep -E '^[1-9][0-9]*$' >/dev/null ||
  { echo "thread-guard: the headroom must be a whole number of threads, 1 or more: $headroom" >&2; exit 2; }

# The command is looked at every tenth of a second, so that its end is seen at once, and
# counted every so many looks.
every=$(awk -v p="$poll" 'BEGIN { n = int(p * 10 + 0.5); print (n < 1 ? 1 : n) }')

# processes: one line for every process there is: its id, its parent's id, its threads and
# its state (Z for a zombie where the system says so, - otherwise). Fails when `ps` does.
if [ "$(uname -s)" = "Darwin" ]; then
  processes() {
    local listed
    # With -M every thread has a line, and `-o ppid=,pid=` puts the two ids last on each.
    listed=$(ps -M -A -o ppid=,pid=) || return 1
    printf '%s\n' "$listed" | awk '
      $NF ~ /^[0-9]+$/ && $(NF - 1) ~ /^[0-9]+$/ { parent[$NF] = $(NF - 1); threads[$NF] += 1 }
      END { for (p in parent) print p, parent[p], threads[p], "-" }'
  }
else
  processes() {
    local listed
    listed=$(ps -e -o pid=,ppid=,nlwp=,s=) || return 1
    printf '%s\n' "$listed" |
      awk '$1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ { print $1, $2, $3 + 0, ($4 == "Z" ? "Z" : "-") }'
  }
fi

# look <root>: one count of the tree under <root>. It sets `ids` to the ids of the tree,
# <root> first, and `over` to the id and the threads of the first process over the limit,
# or to nothing. It fails when the count cannot be trusted, and then sets neither.
ids=""
over=""
look() {
  local table found
  table=$(processes) || return 1
  found=$(printf '%s\n' "$table" | awk -v root="$1" -v limit="$limit" '
    NF == 4 { parent[$1] = $2; threads[$1] = $3; state[$1] = $4; all[n++] = $1 }
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
      list = ""; most = ""
      for (i = 0; i < m; i++) {
        p = found[i]
        if (threads[p] + 0 < 1 && state[p] != "Z") { print "bad"; exit }
        list = list " " p
        if (most == "" && threads[p] + 0 > limit + 0) most = p " " threads[p]
      }
      print "good"; print list; print most
    }') || return 1
  [ "$(printf '%s\n' "$found" | sed -n 1p)" = "good" ] || return 1
  ids=$(printf '%s\n' "$found" | sed -n 2p)
  over=$(printf '%s\n' "$found" | sed -n 3p)
}

if ! look "$$" || [ -z "$ids" ]; then
  echo "thread-guard: cannot count the threads of a process on this system; refusing to run the command" >&2
  exit 2
fi

# Watched already, by a guard above this one with this limit and this pause: nothing to
# add. The variable alone does not say so, since anything can set it: the process it names
# must be one this guard descends from, by a count of its own, and must be running this
# script. Anything else, and this guard watches.
watched_by() { # <process id>: is that process above this one, and a guard?
  local above command
  above=$(processes | awk -v self="$$" -v named="$1" '
    NF == 4 { parent[$1] = $2 }
    END {
      p = self
      for (i = 0; i < 64 && (p in parent) && p != named; i++) p = parent[p]
      print ((p == named && named != self) ? "yes" : "no")
    }') || return 1
  [ "$above" = "yes" ] || return 1
  command=$(ps -o command= -p "$1" 2>/dev/null) || return 1
  printf '%s\n' "$command" |
    grep -E '^([^ ]*/)?(bash|sh)( -[^ ]+)* ([^ ]*/)?thread-guard\.sh( |$)|^([^ ]*/)?thread-guard\.sh( |$)' >/dev/null
}
watcher=${WHIZBANG_THREAD_GUARD:-}
if [ -n "$watcher" ] && [ "${watcher#* }" = "$limit $poll" ] &&
  echo "${watcher%% *}" | grep -E '^[1-9][0-9]*$' >/dev/null && watched_by "${watcher%% *}"; then
  exec "$@"
fi

# On Linux the system is asked to refuse, too: see the top of this file.
allowed=""
if [ "$(uname -s)" = "Linux" ]; then
  # A count that fails, or comes to nothing, lowers nothing: the headroom alone would be
  # under what the user already holds, and every thread of the command would be refused.
  held=$(ps -L -u "$(id -u)" -o lwp= 2>/dev/null | grep -c . || true)
  if echo "${held:-0}" | grep -E '^[1-9][0-9]*$' >/dev/null; then
    allowed=$((held + headroom))
  fi
fi

export WHIZBANG_THREAD_GUARD="$$ $limit $poll"
(
  # A limit that is already lower, or that this user may not set, is left as it is.
  [ -z "$allowed" ] || ulimit -S -u "$allowed" 2>/dev/null || true
  exec "$@"
) <&0 &
child=$!

# kill_tree: stops every process under the command, looking again until no new one
# appears (a stopped process starts nothing, and keeps its children), then kills them all,
# with those of the last good count: the count may be what has failed.
last=""
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
  # The command is gone at once after that, unless something is very wrong: it is waited
  # for, so that it is not left a zombie, for five seconds and no more.
  for pass in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25; do
    kill -0 "$child" 2>/dev/null || break
    sleep 0.2 || true
  done
  if kill -0 "$child" 2>/dev/null; then
    echo "thread-guard: the command (process $child) is still there after it was killed" >&2
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
    # Not a count. The command may have ended since the loop looked; otherwise once more.
    kill -0 "$child" 2>/dev/null || break
    counted=1
    look "$child" || counted=0
  fi
  [ -z "${WHIZBANG_THREAD_TEST_HOOK:-}" ] || $WHIZBANG_THREAD_TEST_HOOK || true
  if [ "$counted" = 0 ]; then
    kill -0 "$child" 2>/dev/null || break
    kill_tree
    echo "thread-guard: the threads of the command could not be counted, twice running, while it ran: a guard that cannot see is no guard" >&2
    echo "thread-guard: killed the command and everything it started: $*" >&2
    exit 1
  fi
  last=$ids
  [ -n "$over" ] || continue
  # The process that is over the limit is stopped before anything else is done.
  kill -STOP "${over%% *}" 2>/dev/null || true
  name=$(ps -o comm= -p "${over%% *}" 2>/dev/null || true)
  kill_tree
  echo "thread-guard: process ${over%% *} (${name:-unknown}) held ${over##* } threads, over the limit of $limit in one process (WHIZBANG_THREAD_LIMIT)" >&2
  echo "thread-guard: killed the command and everything it started: $*" >&2
  exit 1
done

status=0
wait "$child" || status=$?
exit "$status"
