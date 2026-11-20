#!/usr/bin/env bash
# The loop on real models, reported (plan m4, task E1, items 1 and 2): runs `whizbang rascal
# remove-checked` on the 24 entries of `remove_prompt` of the edit set under one judge, or
# once by hand on one picture, and writes what happened as aggregates, read from the runs'
# records by `whizbang runs export` (scripts/loop-evidence.py). The orchestrator runs it
# on the owner's Mac; CI never does. It spends: at most 3 fills and 3 checks an entry.
#
# First, once, the pictures are drawn again, outside the work tree:
#
#   whizbang eval edits draw --pictures <the sets' pictures> --out <folder>
#
# Then, once for each judge, in a store made new for it:
#
#   WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh loop-evidence scripts/loop-evidence.sh \
#       --binary <whizbang> --store <a new store> --set <folder> \
#       --manifest labs/m3-edits/edits.toml --judge <judge> \
#       --config <file> --env-file <file> --out docs/results/m4-loop-<judge>.toml \
#       [--allowance <USD>] [--models-dir <dir>]
#
# And a case by hand, under the same first line:
#
#   ... scripts/loop-evidence.sh --case <name> --image <picture> --prompt <phrase> \
#       --binary <whizbang> --store <store> --config <file> --env-file <file> \
#       [--judge <judge>] [--out <file>] [--allowance <USD>] [--models-dir <dir>]
#
# What it holds itself to, so that it is not left to whoever types the command:
#
# - **The heavy lock.** It refuses to start unless a process above it holds the lock of
#   scripts/heavy.sh, and unless the thread guard's limit is 200. Nothing is read, imported
#   or run before that.
# - **The guards.** Every command that loads a model, which is each `remove-checked`, runs
#   under scripts/memory-watch.sh and scripts/thread-guard.sh at 200 threads. An import
#   and an export load none and run bare.
# - **Which pictures.** The folder also holds clean pictures, two other kinds of defect
#   and the truth masks, so nothing is taken by looking in it. The committed manifest's
#   entries of `remove_prompt` are taken: exactly 24, each picture there under the SHA-256
#   the manifest pins, none a truth mask. On any difference it stops, naming the entry,
#   having imported and run nothing.
# - **The binary is of this tree.** Its `--version` names a commit, which must be HEAD or
#   an ancestor of it, in a checkout with no tracked change: the commit the tally names.
# - **Nothing is paid for twice.** An entry's request id is `m4-loop-<judge>-<item id>`
#   (a case's, `m4-loop-case-<name>`). An entry whose run the store already holds is not
#   started again; one that was interrupted is asked again under its id, which the store
#   answers with the run it made (ADR 0007). So a run of this script that stopped is run
#   again as it was typed.
# - **A call that leaves no run stops the script:** nothing ran, so something is wrong
#   with the config, the keys or the model, and the next 23 would say the same.
#
# The env file is never opened here: its path is handed to the binary. `--allowance` is
# what a store this script makes is given (`whizbang spend reset`); a store that is there
# already keeps its own. What a judge is called is its name in the config's
# `[backends.judge]` tables, which is also the name in the request ids and in `--out`.
#
# The tally holds no item, no picture's name, no path and no word of a judge. A case's
# file holds the record's own sentences of why a run was not checked and why its attempts
# ended, unless one holds a path.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "loop-evidence: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

say() { echo "loop-evidence: $*"; }
stop() {
  echo "loop-evidence: stopped: $*" >&2
  exit 1
}
usage() {
  echo "usage: scripts/loop-evidence.sh --binary <whizbang> --store <dir> --set <folder> --manifest <file> --judge <judge> --config <file> --env-file <file> --out <file> [--allowance <USD>] [--models-dir <dir>]" >&2
  echo "       scripts/loop-evidence.sh --case <name> --image <picture> --prompt <phrase> --binary <whizbang> --store <dir> --config <file> --env-file <file> [--judge <judge>] [--out <file>] [--allowance <USD>] [--models-dir <dir>]" >&2
  exit 2
}

# --- Under the heavy lock, with the guard at 200, or nothing starts.
lock=${WHIZBANG_HEAVY_LOCK:-/tmp/whizbang-heavy-$(id -u).lock}
holder=$(cat "$lock/pid" 2>/dev/null || true)
under=0
if printf '%s\n' "$holder" | grep -E '^[1-9][0-9]*$' >/dev/null; then
  at=$$
  hops=0
  while [ "$hops" -lt 64 ] && [ -n "$at" ] && [ "$at" -gt 1 ]; do
    if [ "$at" = "$holder" ]; then
      under=1
      break
    fi
    at=$(ps -o ppid= -p "$at" 2>/dev/null | tr -d ' ' || true)
    hops=$((hops + 1))
  done
fi
[ "$under" = 1 ] || {
  echo "loop-evidence: it is not under the heavy lock; run it as: WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh loop-evidence scripts/loop-evidence.sh ...; nothing was started" >&2
  exit 2
}
[ "${WHIZBANG_THREAD_LIMIT:-}" = 200 ] || {
  echo "loop-evidence: the thread guard's limit must be 200 (WHIZBANG_THREAD_LIMIT=200); nothing was started" >&2
  exit 2
}

binary= store= set= manifest= judge= config= env_file= out= allowance= models= name= image= prompt=
while [ $# -gt 0 ]; do
  [ $# -ge 2 ] || usage
  case "$1" in
    --binary) binary=$2 ;;
    --store) store=$2 ;;
    --set) set=$2 ;;
    --manifest) manifest=$2 ;;
    --judge) judge=$2 ;;
    --config) config=$2 ;;
    --env-file) env_file=$2 ;;
    --out) out=$2 ;;
    --allowance) allowance=$2 ;;
    --models-dir) models=$2 ;;
    --case) name=$2 ;;
    --image) image=$2 ;;
    --prompt) prompt=$2 ;;
    *) usage ;;
  esac
  shift 2
done
[ -n "$binary" ] && [ -n "$store" ] && [ -n "$config" ] && [ -n "$env_file" ] || usage
if [ -n "$name" ]; then
  [ -n "$image" ] && [ -n "$prompt" ] && [ -z "$set" ] && [ -z "$manifest" ] || usage
  printf '%s\n' "$name" | grep -E '^[a-z][a-z0-9-]{0,31}$' >/dev/null || stop "a case's name is lower-case letters, digits and -"
else
  [ -n "$set" ] && [ -n "$manifest" ] && [ -n "$judge" ] && [ -n "$out" ] && [ -z "$image" ] && [ -z "$prompt" ] || usage
fi
[ -z "$judge" ] || printf '%s\n' "$judge" | grep -E '^[a-z][a-z0-9_-]{0,63}$' >/dev/null ||
  stop "the judge is its name in the config's [backends.judge] tables: a lower-case letter, then lower-case letters, digits, _ or -"
[ -z "$allowance" ] || printf '%s\n' "$allowance" | grep -E '^[0-9]+(\.[0-9]{1,6})?$' >/dev/null || stop "--allowance is US dollars, as in 8 or 8.00"
for tool in git python3 ps; do
  command -v "$tool" >/dev/null || stop "$tool is not installed"
done
python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)' || stop "python3 is older than 3.9"
[ -f "$binary" ] && [ -x "$binary" ] || stop "--binary names no program"
[ -f "$config" ] || stop "--config names no file"
[ -f "$env_file" ] || stop "--env-file names no file"
absolute() { case "$1" in /*) printf '%s\n' "$1" ;; *) printf '%s\n' "$PWD/$1" ;; esac; }
binary=$(absolute "$binary")
store=$(absolute "$store")
config=$(absolute "$config")
env_file=$(absolute "$env_file")
[ -z "$out" ] || out=$(absolute "$out")
[ -z "$out" ] || [ -d "$(dirname "$out")" ] || stop "the folder --out is to be written in is not there"
tally() { python3 -B "$root/scripts/loop-evidence.py" "$@"; }

# --- The binary is of this tree: its commit is HEAD's or an ancestor's, and no tracked
# file differs, so the commit a tally names is what ran.
version_line=$("$binary" --version 2>/dev/null) || stop "the binary printed no version line"
version=$(printf '%s\n' "$version_line" | sed -n -E 's/^whizbang ([0-9]+\.[0-9]+\.[0-9]+) \(commit [0-9a-f]{40}\)$/\1/p')
commit=$(printf '%s\n' "$version_line" | sed -n -E 's/^whizbang [0-9.]+ \(commit ([0-9a-f]{40})\)$/\1/p')
[ -n "$version" ] && [ -n "$commit" ] || stop "the binary's version line names no commit: build it from a checkout of this repository"
git -C "$root" merge-base --is-ancestor "$commit" HEAD 2>/dev/null ||
  stop "the binary was built from $commit, which is not a commit of this checkout's history"
git -C "$root" diff --quiet HEAD -- || stop "this checkout has tracked changes not committed; commit them, then run it again"

work=$(mktemp -d)
work=$(cd "$work" && pwd -P)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$work"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "loop-evidence: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

# --- Which pictures: decided by the manifest, checked before anything is imported.
if [ -z "$name" ]; then
  [ -f "$manifest" ] || stop "--manifest names no file"
  [ -d "$set" ] || stop "--set names no folder"
  set=$(cd "$set" && pwd -P)
  tally entries --manifest "$manifest" --set "$set" >"$work/entries" || stop "the set is not the manifest's; nothing was imported or run"
  [ "$(wc -l <"$work/entries" | tr -d ' ')" = 24 ] || stop "the entries are not 24; nothing was imported or run"
  say "the 24 entries of remove_prompt are in the folder, each under the SHA-256 the manifest pins"
else
  [ -f "$image" ] || stop "--image names no file"
fi

under_config=(--config "$config" --env-file "$env_file")
[ -z "$models" ] || under_config+=(--models-dir "$models")
# guarded <arguments...>: the binary, for a command that loads a model, under the watch on
# resident memory and the thread guard at 200. The heavy lock is held above this script.
guarded() {
  WHIZBANG_THREAD_LIMIT=200 "$root/scripts/memory-watch.sh" "$root/scripts/thread-guard.sh" "$binary" "$@"
}
# imported <picture>: brings the picture into the store, which is made, and given its
# allowance, when it is not there; prints the image's identity.
imported() {
  local create=() printed
  [ -d "$store" ] || create=(--create)
  printed=$("$binary" store import --picture "$1" --store "$store" ${create[@]+"${create[@]}"}) || return 1
  if [ ${#create[@]} -gt 0 ] && [ -n "$allowance" ]; then
    "$binary" spend reset --store "$store" --allowance "$allowance" >/dev/null || return 1
    say "made the store and gave it an allowance of $allowance dollars" >&2
  fi
  printf '%s\n' "$printed" | sed -n 's/^imported image \(sha256:[0-9a-f]\{64\}\)$/\1/p' | sed -n '1p'
}
# exported: every record of the store, one a line, in $work/records.
exported() {
  if [ -d "$store" ]; then
    "$binary" runs export --store "$store" >"$work/records" || stop "the store's runs could not be exported"
  else
    : >"$work/records"
  fi
}
# held <request id>: whether the export holds a run of the loop under the id.
held() {
  grep -F "\"request\":\"$1\"" "$work/records" | grep -F '"action":"remove_checked"' >/dev/null
}
# looped <request id> <picture> <phrase>: one run of the loop, unless the store holds it.
looped() {
  local request=$1 picture=$2 phrase=$3 id status=0
  local chosen=()
  [ -z "$judge" ] || chosen=(--judge "$judge")
  if held "$request"; then
    return 0
  fi
  id=$(imported "$picture") || stop "a picture could not be imported"
  [ -n "$id" ] || stop "an import printed no image"
  guarded rascal remove-checked --image "$id" --prompt "$phrase" ${chosen[@]+"${chosen[@]}"} \
    --request-id "$request" --store "$store" "${under_config[@]}" >"$work/out" 2>"$work/err" || status=$?
  # What the loop said of its plan, its attempts and its end; never a picture's name.
  { grep -E '^(plan: |remove_checked: attempt )' "$work/err" || true; } | sed 's/^/    /'
  { grep -E '^(attempts|ended|checked|verdict): ' "$work/out" || true; } | sed 's/^/    /'
  exported
  if ! held "$request"; then
    sed 's/^/    /' "$work/err" >&2
    stop "a call left no run (status $status): nothing ran, so the config, the keys or the model need a look; what the store holds is kept, and the same command goes on from here"
  fi
  ran=$((ran + 1))
}

exported
ran=0
if [ -n "$name" ]; then
  # One case by hand.
  request=m4-loop-case-$name
  looped "$request" "$image" "$prompt"
  tally case --records "$work/records" --request "$request" --name "$name" --commit "$commit" --whizbang "$version" --out "${out:--}"
  say "the case $name: started $ran run"
else
  at=0
  while IFS=$'\t' read -r id picture phrase; do
    at=$((at + 1))
    say "entry $at of 24"
    looped "m4-loop-$judge-$id" "$set/$picture" "$phrase" </dev/null
  done <"$work/entries"
  tally tally --records "$work/records" --manifest "$manifest" --judge "$judge" --commit "$commit" --whizbang "$version" --out "$out"
  say "started $ran runs of 24; wrote $out"
fi
done_ok=1
exit 0
}
main "$@"; exit
