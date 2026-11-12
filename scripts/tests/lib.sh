# Helpers for the script tests. Sourced, not run.
#
# First, parse the test file and every file between it and this one: the bash that ships
# with macOS (3.2) runs a script up to its first syntax error and then, because of the
# `EXIT` trap below, exits 0 (see the top of scripts/scan.sh).
for _file in "${BASH_SOURCE[@]}"; do
  "$BASH" -n "$_file" || { echo "test: $_file has a syntax error; refusing to run" >&2; exit 2; }
done
unset _file
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "failed: $*" >&2
  exit 1
}

# finished: the last line of every test file, and of every part that scripts/tests/run.sh
# or a test runs as a file of its own. It prints the line they require as the last of the
# output: a file that exits 0 without reaching its end has not passed.
finished() {
  echo "finished: $0"
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
