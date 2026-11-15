#!/usr/bin/env bash
# Holds the example on `Ledger` that must not compile to the one reason it must not
# (ADR 0016: a ledger cannot return from `reserve` without having made its names).
#
#   scripts/compile-fail.sh                       build the core, compile the example, judge
#   scripts/compile-fail.sh --example FILE        print the example taken out of FILE
#   scripts/compile-fail.sh --judge STATUS ERRORS CONTROL
#                                                 judge saved results: the exit status of
#                                                 the example's compilation, the file of
#                                                 what the compiler said, and the exit
#                                                 status of the control's compilation
#
# The last two exist for scripts/tests/test-compile-fail.sh. `just compile-fail`; `just ci`
# runs it. It builds, so it is heavy (AGENTS.md); it runs no test.
#
# Why a script: the fast tests run no documentation examples, and an example marked
# `compile_fail` passes when it fails to compile for any reason at all: a renamed import
# would do. So the example is taken out of the source as it is written there and compiled
# twice against the core as cargo builds it:
#
# - as it is: it must fail, and every error must be E0451 naming `MadeName`, whose fields
#   are private. Any other error fails this check;
# - the control, with the one line that writes a `MadeName` replaced by `todo!()`: it must
#   compile. So nothing else in the example is what stops it.
#
# Needs jq.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "compile-fail: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

source=crates/whizbang-core/src/contract/ledger.rs
# The line of the example that writes the value no ledger can write.
makes='MadeName {'

# example FILE: the lines of the one block marked compile_fail in FILE's documentation,
# without the marks of a documentation comment.
example() {
  awk '
    /^[[:space:]]*\/\/\/ ```compile_fail[[:space:]]*$/ { blocks++; inside = 1; next }
    inside && /^[[:space:]]*\/\/\/ ```[[:space:]]*$/ { inside = 0; next }
    inside { sub(/^[[:space:]]*\/\/\/ ?/, ""); print }
    END { if (blocks != 1 || inside) exit 3 }
  ' "$1" || { echo "compile-fail: $1 does not hold exactly one closed block marked compile_fail" >&2; return 1; }
}

# judge STATUS ERRORS CONTROL
judge() {
  local status=$1 errors=$2 control=$3
  if [ "$status" -eq 0 ]; then
    echo "compile-fail: the example on Ledger compiled: a ledger can return from reserve without having made a name" >&2
    return 1
  fi
  local wanted='^error\[E0451\]: fields? .* of struct `MadeName` (is|are) private'
  local other
  other=$(grep -E '^error' "$errors" | grep -v -E "$wanted" | grep -v -E '^error: aborting due to ' || true)
  if [ -n "$other" ]; then
    echo "compile-fail: the example on Ledger fails for another reason than the one it is there for:" >&2
    printf '%s\n' "$other" | sed 's/^/    /' >&2
    return 1
  fi
  if ! grep -E "$wanted" "$errors" >/dev/null; then
    echo "compile-fail: the example on Ledger failed without the error it is there for (E0451: the fields of MadeName are private)" >&2
    sed 's/^/    /' "$errors" >&2
    return 1
  fi
  if [ "$control" -ne 0 ]; then
    echo "compile-fail: the example on Ledger does not compile without the line that writes a MadeName either: something else in it is wrong" >&2
    return 1
  fi
  echo "compile-fail: ok (the example on Ledger fails with E0451 on the fields of MadeName and with nothing else, and compiles without the line that writes one)"
}

case "${1:-}" in
  --example)
    [ $# -eq 2 ] || { echo "usage: scripts/compile-fail.sh --example FILE" >&2; exit 2; }
    example "$2" || exit 1
    exit 0
    ;;
  --judge)
    [ $# -eq 4 ] || { echo "usage: scripts/compile-fail.sh --judge STATUS ERRORS CONTROL" >&2; exit 2; }
    judge "$2" "$3" "$4" || exit 1
    exit 0
    ;;
  "") ;;
  *) echo "compile-fail: unknown argument: $1" >&2; exit 2 ;;
esac

command -v jq >/dev/null || { echo "compile-fail: jq is not installed" >&2; exit 2; }

tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "compile-fail: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

example "$source" >"$tmp/careless.rs"
[ "$(grep -c -F -e "$makes" "$tmp/careless.rs")" = 1 ] || {
  echo "compile-fail: the example in $source does not write a MadeName on exactly one line" >&2
  exit 1
}
# The control: the same example, with that line replaced.
awk -v makes="$makes" 'index($0, makes) { print "        todo!()"; next } { print }' \
  "$tmp/careless.rs" >"$tmp/control.rs"

# The core as cargo builds it, and where: cargo names the file.
cargo build -p whizbang-core --message-format=json >"$tmp/build.json"
rlib=$(jq -r 'select(.reason == "compiler-artifact" and .target.name == "whizbang_core")
  | .filenames[] | select(endswith(".rlib"))' "$tmp/build.json" | sed -n '1p')
[ -n "$rlib" ] && [ -f "$rlib" ] || { echo "compile-fail: cargo named no library for whizbang-core" >&2; exit 1; }
deps=$(dirname "$rlib")
[ "$(basename "$deps")" = deps ] || deps="$deps/deps"
edition=$(sed -n 's/^edition = "\([0-9]*\)"$/\1/p' Cargo.toml | sed -n '1p')
[ -n "$edition" ] || { echo "compile-fail: Cargo.toml names no edition" >&2; exit 1; }

compile() { # <file>: type-checks it as a crate that uses the core; writes nothing outside $tmp
  rustc --edition "$edition" --crate-type lib --crate-name careless --emit metadata \
    --out-dir "$tmp/out" -L "dependency=$deps" --extern "whizbang_core=$rlib" "$1"
}
mkdir "$tmp/out"
status=0
compile "$tmp/careless.rs" >/dev/null 2>"$tmp/errors" || status=$?
control=0
compile "$tmp/control.rs" >/dev/null 2>"$tmp/control-errors" || control=$?
if ! judge "$status" "$tmp/errors" "$control"; then
  [ "$control" -eq 0 ] || sed 's/^/    /' "$tmp/control-errors" >&2
  exit 1
fi
done_ok=1
exit 0
}
main "$@"; exit
