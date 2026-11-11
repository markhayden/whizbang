# Helpers for the script tests. Sourced, not run.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "failed: $*" >&2
  exit 1
}

# expect_pass <name> <command...>: the command must exit 0.
expect_pass() {
  local name=$1
  shift
  "$@" >"$tmp/out" 2>&1 || { cat "$tmp/out" >&2; fail "$name: expected success"; }
}

# expect_fail <name> <text> <command...>: the command must exit non-zero and print <text>.
expect_fail() {
  local name=$1 text=$2
  shift 2
  if "$@" >"$tmp/out" 2>&1; then
    cat "$tmp/out" >&2
    fail "$name: expected failure"
  fi
  grep -q -F -e "$text" "$tmp/out" || { cat "$tmp/out" >&2; fail "$name: expected the output to say: $text"; }
}
