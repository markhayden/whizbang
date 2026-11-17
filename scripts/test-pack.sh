#!/usr/bin/env bash
# Runs the pack's Python tests with the Python the pack pins. `just test-pack`, which runs it
# through scripts/thread-guard.sh; started any other way it refuses, before it starts
# anything.
#
#   scripts/thread-guard.sh scripts/test-pack.sh
#
# The pinned Python is the version in pack/.python-version, installed by `uv` and found by
# it here. Finding it contacts nothing: `uv` is started offline, with no user's
# configuration, with downloads of a Python turned off, and with the three bounds of the row
# of "Rules" on `uv` (docs/plans/m1c-local-pack.md). Where it is not installed, this fails
# and names the command that installs it, which downloads it (an origin the owner approved
# for this use; see the plan's "Origins").
#
# `uv` and the Python are each given a cleared environment: PATH, HOME, TMPDIR and
# UV_PYTHON_INSTALL_DIR of the caller's, and the variables named here, nothing else. The Python runs the runner,
# pack/tests/run.py, isolated (`-I`: no user site, no variable of Python's own, not the
# folder it is started in) and writing no bytecode (`-B`). The runner ends itself at
# 60 seconds (its top says how).
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "test-pack: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
guard=$root/scripts/thread-guard.sh

# under_guard: true when a process above this one, at most 32 parents up, is the one
# WHIZBANG_THREAD_GUARD names, and it runs this repo's guard by its whole path (the guard
# always runs so: the top of scripts/thread-guard.sh). A variable set by hand to some other
# process switches nothing off.
under_guard() {
  local named pid parent command script hops=0
  named=$(printf '%s\n' "${WHIZBANG_THREAD_GUARD:-}" | awk 'NF == 3 && $1 ~ /^[1-9][0-9]*$/ { print $1 }')
  [ -n "$named" ] || return 1
  pid=$$
  while [ "$hops" -lt 32 ]; do
    hops=$((hops + 1))
    parent=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ') || return 1
    case "$parent" in '' | 0 | 1) return 1 ;; esac
    if [ "$parent" = "$named" ]; then
      command=$(ps -o command= -p "$parent" 2>/dev/null) || return 1
      script=$(printf '%s\n' "$command" | awk '{
        i = 1
        if ($1 ~ /(^|\/)(bash|sh)$/) { i = 2; while (i <= NF && $i ~ /^-/) i++ }
        print $i
      }')
      [ "$script" = "$guard" ]
      return
    fi
    pid=$parent
  done
  return 1
}
if ! under_guard; then
  echo "test-pack: refused: run it through scripts/thread-guard.sh (just test-pack), which kills a process that holds too many threads" >&2
  exit 2
fi

pinned=$(tr -d ' \n' <"$root/pack/.python-version")
printf '%s\n' "$pinned" | grep -E '^3\.[0-9]+\.[0-9]+$' >/dev/null ||
  { echo "test-pack: pack/.python-version must hold one version, as 3.12.15" >&2; exit 2; }

# Where `uv python install` is told to download from. Without it, `uv` tries a mirror of
# its maker's first and GitHub after; with it, GitHub's releases only. The owner approved
# GitHub's releases, and no other origin, for a Python.
mirror=https://github.com/astral-sh/python-build-standalone/releases/download

# The caller's variables that are handed on, when set.
kept=(PATH="$PATH" HOME="$HOME")
[ -z "${TMPDIR:-}" ] || kept+=(TMPDIR="$TMPDIR")
# Where uv installs its Pythons, when it is not the default: CI sets it to its cached folder.
[ -z "${UV_PYTHON_INSTALL_DIR:-}" ] || kept+=(UV_PYTHON_INSTALL_DIR="$UV_PYTHON_INSTALL_DIR")

python=$(cd "$root/pack" && env -i "${kept[@]}" \
  UV_NO_CONFIG=1 UV_OFFLINE=1 UV_PYTHON_DOWNLOADS=never UV_MANAGED_PYTHON=1 \
  UV_CONCURRENT_DOWNLOADS=4 UV_CONCURRENT_INSTALLS=4 UV_CONCURRENT_BUILDS=1 \
  uv --no-config --offline --no-python-downloads --managed-python python find "$pinned") || python=""
if [ -z "$python" ] || [ ! -x "$python" ]; then
  echo "test-pack: uv found no Python $pinned that it manages, and downloads nothing here; install it once with: UV_PYTHON_INSTALL_MIRROR=$mirror uv python install --no-config --no-bin $pinned (that downloads it from GitHub's releases, and from nowhere else)" >&2
  exit 1
fi

cd "$root/pack"
env -i "${kept[@]}" WHIZBANG_PACK_TEST_LIMIT=60 "$python" -I -B "$root/pack/tests/run.py"
}
main "$@"; exit
