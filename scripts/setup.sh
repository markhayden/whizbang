#!/usr/bin/env bash
# `just setup`: installs the git hooks, then checks the pinned tools.
#
#   scripts/setup.sh           install the hooks and check the tools
#   scripts/setup.sh --check   check the tools only
#
# A tool that is missing, or at another version than scripts/tool-versions.txt pins, is
# named with the command that installs the pinned version, and the script fails. jq and
# python3 (3.9 or later, for scripts/baseline) are checked without a pin. The hooks
# are installed either way. `just check` does not run this: only setup is strict about
# versions.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if [ "${1:-}" != "--check" ]; then
  git config core.hooksPath scripts/hooks
  echo "hooks installed from scripts/hooks: pre-commit, commit-msg, pre-push"
  echo "set WHIZBANG_WORDLIST to the clean-room word list before your first push"
fi

# The installed version of a tool: the second word of the first line it prints.
installed() {
  case "$1" in
    just) just --version ;;
    cargo-*) cargo "${1#cargo-}" --version ;;
  esac 2>/dev/null | awk 'NR == 1 { print $2 }' || true
}

status=0
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
