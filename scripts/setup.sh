#!/usr/bin/env bash
# `just setup`: installs the git hooks, then checks the pinned tools.
#
#   scripts/setup.sh           install the hooks and check the tools
#   scripts/setup.sh --check   check the tools only
#
# A missing tool, or one at another version, is named with the command that installs the
# pinned version, and the script fails. The hooks are installed either way.
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
while read -r tool want; do
  case "$tool" in '' | '#'*) continue ;; esac
  have=$(installed "$tool")
  if [ "$have" = "$want" ]; then
    echo "tool ok: $tool $want"
  else
    echo "tool missing: $tool $want (found: ${have:-none}); install it with: cargo install $tool@$want --locked" >&2
    status=1
  fi
done <scripts/tool-versions.txt

if command -v jq >/dev/null; then
  echo "tool ok: jq (any version)"
else
  echo "tool missing: jq; install it with your package manager (brew install jq, apt-get install jq)" >&2
  status=1
fi
exit "$status"
