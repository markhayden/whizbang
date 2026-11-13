#!/usr/bin/env bash
# Runs some of the tests, through scripts/thread-guard.sh. `just test-only`.
#
#   scripts/test-only.sh [-p <crate>] [--lib] [--test <name>] [-E <filter>]
#                        [--no-capture] [--no-fail-fast] [<filter word>...]
#
# as in `just test-only -p whizbang-core watch::`. The recipe is on the allow list
# (`.claude/settings.json`), so it runs without a prompt, and what it accepts is therefore a
# closed set. nextest takes arguments that name a program to run or a file to read (a
# config with a runner or a compiler wrapper in it, a manifest elsewhere, a config file, an
# archive); handed on, any of them would run what the caller chose. So every argument is
# held to the list below before anything runs, and anything else is refused with the exit
# status 2 and one line that names it.
#
# - `-p <crate>` or `--package <crate>`: a crate of this workspace, by its name.
# - `--lib`: the tests in a crate's library.
# - `--test <name>`: one test binary; letters, digits, `_` and `-`.
# - `-E <filter>`: a nextest filter expression of letters, digits, spaces and
#   `_ : = ( ) & | ! ~ / . -`. No quote, `$`, backtick, `;`, `<`, `>` or line break.
# - `--no-capture`, `--no-fail-fast`.
# - A filter word: letters, digits, `_` and `:`, as in `watch::` or a test's whole name.
#
# An option in one word with its value (`--package=x`) is refused: the two-word form is the
# one that is read here.
#
# The command is run from this repo's folder with this repo's manifest named, whatever
# folder it was started in, so neither the caller's folder nor a config beside it is used.
# These variables are not handed on, since each can name a program to run in a build's
# place, or a config: RUSTC, RUSTDOC, RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER, RUSTFLAGS,
# RUSTDOCFLAGS, CARGO_ENCODED_RUSTFLAGS, CARGO_ENCODED_RUSTDOCFLAGS, every CARGO_BUILD_*,
# every CARGO_TARGET_* but CARGO_TARGET_DIR, and every NEXTEST_*.
#
# WHIZBANG_TEST_ONLY_DRY is for this script's test: set to 1 the command is printed, a word
# a line, and not run; set to `env` or `pwd`, what the command would be handed is printed.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "test-only: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

# refuse <why> <argument>: one line, the argument cut and with nothing in it but plain
# characters, and the exit status 2.
refuse() {
  local shown
  shown=$(printf '%s' "${2:-}" | LC_ALL=C tr -c ' -~' '?' | cut -c1-64)
  echo "test-only: refused: $1: $shown" >&2
  exit 2
}

[ $# -ge 1 ] || refuse "no argument; say which tests, as in -p whizbang-core" ""

crate='^[a-z0-9][a-z0-9_-]*$'
binary='^[A-Za-z0-9_-]+$'
filter='^[A-Za-z0-9_:=()&|!~/. -]+$'
word='^[A-Za-z0-9_:]+$'
line_break='
'

# in_workspace <name>: is that the name of a crate of this workspace?
in_workspace() {
  local manifest
  for manifest in "$root"/crates/*/Cargo.toml; do
    [ -f "$manifest" ] || continue
    # The first `name` of a manifest is its package's.
    [ "$(sed -n 's/^name = "\(.*\)"$/\1/p' "$manifest" | sed -n 1p)" != "$1" ] || return 0
  done
  return 1
}

args=("$@")
count=$#
i=0
while [ "$i" -lt "$count" ]; do
  arg=${args[$i]}
  case "$arg" in *"$line_break"*) refuse "an argument with a line break in it" "$arg" ;; esac
  case "$arg" in
    -p | --package)
      i=$((i + 1))
      [ "$i" -lt "$count" ] || refuse "no crate after" "$arg"
      value=${args[$i]}
      [[ "$value" =~ $crate ]] || refuse "not the name of a crate" "$value"
      in_workspace "$value" || refuse "not a crate of this workspace" "$value"
      ;;
    --lib | --no-capture | --no-fail-fast) ;;
    --test)
      i=$((i + 1))
      [ "$i" -lt "$count" ] || refuse "no test binary after" "$arg"
      value=${args[$i]}
      [[ "$value" =~ $binary ]] || refuse "not the name of a test binary" "$value"
      ;;
    -E)
      i=$((i + 1))
      [ "$i" -lt "$count" ] || refuse "no filter after" "$arg"
      value=${args[$i]}
      case "$value" in *"$line_break"*) refuse "a filter with a line break in it" "$value" ;; esac
      [[ "$value" =~ $filter ]] || refuse "a filter holds letters, digits, spaces and _ : = ( ) & | ! ~ / . - only" "$value"
      ;;
    -*) refuse "not an option this takes (-p, --lib, --test, -E, --no-capture, --no-fail-fast)" "$arg" ;;
    *) [[ "$arg" =~ $word ]] || refuse "a filter word holds letters, digits, _ and : only" "$arg" ;;
  esac
  i=$((i + 1))
done

# What names a program to run in a build's place, or a config, is not handed on.
for name in $(compgen -e); do
  case "$name" in
    CARGO_TARGET_DIR) ;;
    RUSTC | RUSTDOC | RUSTC_WRAPPER | RUSTC_WORKSPACE_WRAPPER | RUSTFLAGS | RUSTDOCFLAGS | \
      CARGO_ENCODED_RUSTFLAGS | CARGO_ENCODED_RUSTDOCFLAGS | CARGO_BUILD_* | CARGO_TARGET_* | NEXTEST_*)
      unset "$name"
      ;;
  esac
done

cd "$root"
case "${WHIZBANG_TEST_ONLY_DRY:-}" in
  1) printf '%s\n' scripts/thread-guard.sh cargo nextest run --manifest-path "$root/Cargo.toml" "$@"; exit 0 ;;
  env) env; exit 0 ;;
  pwd) pwd -P; exit 0 ;;
esac
exec scripts/thread-guard.sh cargo nextest run --manifest-path "$root/Cargo.toml" "$@"
