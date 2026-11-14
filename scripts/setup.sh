#!/usr/bin/env bash
# `just setup`: installs the git hooks, then checks the pinned tools. After a change to
# scripts/hook-dispatch.sh it is run again: scripts/worktree.sh refuses to add a worktree
# while the installed copies differ from it.
#
#   scripts/setup.sh           install the hooks and check the tools
#   scripts/setup.sh --check   check the tools only
#
# A tool that is missing, or at another version than scripts/tool-versions.txt pins, is
# named with the command that installs the pinned version, and the script fails. jq and
# python3 (3.9 or later, for scripts/baseline) are checked without a pin. The hooks
# are installed either way. `just check` does not run this: only setup is strict about
# versions.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

status=0
if [ "${1:-}" != "--check" ]; then
  # The hooks go into the repository's own hooks folder, which every worktree shares and
  # where git looks from whatever folder it is run in: each is a copy of
  # scripts/hook-dispatch.sh, which runs scripts/hooks/<name> of the working tree in use.
  # An earlier setup pointed git at scripts/hooks instead, by a path that git reads from
  # the folder it is run in: a push from another folder then found no hook and ran none.
  # That setting is taken away. Any other value of it was not made here and is left, and
  # is a problem: git would not look in the hooks folder at all.
  if [ "$(git config --local --get core.hooksPath || true)" = scripts/hooks ]; then
    git config --local --unset-all core.hooksPath
  fi
  hooks_dir=$(git rev-parse --git-common-dir)/hooks
  mkdir -p "$hooks_dir"
  for hook in pre-commit commit-msg pre-push; do
    cp scripts/hook-dispatch.sh "$hooks_dir/$hook"
    chmod +x "$hooks_dir/$hook"
    cmp -s scripts/hook-dispatch.sh "$hooks_dir/$hook" && [ -x "$hooks_dir/$hook" ] || {
      echo "hook problem: $hook could not be installed in the repository's hooks folder" >&2
      status=1
    }
  done
  if [ -n "$(git config --get core.hooksPath || true)" ]; then
    echo "hook problem: core.hooksPath is set, in this repo's configuration or the user's, so git does not look in the repository's hooks folder and would run none of these hooks; remove the setting (git config --show-origin --get core.hooksPath shows where it is)" >&2
    status=1
  fi
  echo "hooks installed in the repository's hooks folder: pre-commit, commit-msg, pre-push (each runs scripts/hooks/<name> of the working tree in use)"
  echo "set WHIZBANG_WORDLIST to the clean-room word list before your first push"
fi

# The installed version of a tool: the second word of the first line it prints.
installed() {
  case "$1" in
    just) just --version ;;
    cargo-*) cargo "${1#cargo-}" --version ;;
  esac 2>/dev/null | awk 'NR == 1 { print $2 }' || true
}

# Git skips a hook it cannot execute, without a word: a push would then run no scan.
for hook in pre-commit commit-msg pre-push; do
  if [ -f "scripts/hooks/$hook" ] && [ -x "scripts/hooks/$hook" ]; then
    echo "hook ok: $hook"
  else
    echo "hook problem: scripts/hooks/$hook is missing or cannot be executed, so git would skip it; restore it with: git checkout -- scripts/hooks/$hook && chmod +x scripts/hooks/$hook" >&2
    status=1
  fi
done
while read -r tool want; do
  case "$tool" in '' | '#'*) continue ;; esac
  have=$(installed "$tool")
  if [ "$have" = "$want" ]; then
    echo "tool ok: $tool $want"
  elif [ -z "$have" ]; then
    echo "tool problem: $tool is missing; install it with: cargo install $tool@$want --locked" >&2
    status=1
  else
    echo "tool problem: $tool is at another version: found $have, pinned $want; install the pinned one with: cargo install $tool@$want --locked" >&2
    status=1
  fi
done <scripts/tool-versions.txt

if command -v jq >/dev/null; then
  echo "tool ok: jq (any version)"
else
  echo "tool problem: jq is missing; install it with your package manager (brew install jq, apt-get install jq)" >&2
  status=1
fi

# python3 runs scripts/baseline and its tests. Any version from 3.9 on will do.
py=$(python3 -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>/dev/null || true)
case "$py" in
  [0-9]*.[0-9]*)
    if [ "${py%%.*}" -gt 3 ] || { [ "${py%%.*}" -eq 3 ] && [ "${py#*.}" -ge 9 ]; }; then
      echo "tool ok: python3 $py (3.9 or later)"
    else
      echo "tool problem: python3 is too old: found $py, and scripts/baseline needs 3.9 or later; install a newer one with your package manager" >&2
      status=1
    fi
    ;;
  *)
    echo "tool problem: python3 is missing; scripts/baseline needs 3.9 or later; install it with your package manager (brew install python, apt-get install python3)" >&2
    status=1
    ;;
esac
exit "$status"
}
main "$@"; exit
