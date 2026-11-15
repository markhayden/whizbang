#!/usr/bin/env bash
# scripts/compile-fail.sh holds the example on `Ledger` that must not compile to the one
# error it is there for. This tests the two parts of it that need no build: taking the
# example out of the source, and judging what the compiler said. The build itself is
# `just compile-fail`, which `just ci` runs.
source "$(dirname "$0")/lib.sh"

check="$root/scripts/compile-fail.sh"

# --- The example is taken out of the source as it is written there.
"$check" --example crates/whizbang-core/src/contract/ledger.rs >"$tmp/example.rs" ||
  fail "the example on Ledger is found"
grep -q -x -F 'struct Careless;' "$tmp/example.rs" || fail "the example is the one on Ledger"
grep -q -F 'Ok(MadeName {' "$tmp/example.rs" || fail "the example writes a MadeName"
grep -q -F '///' "$tmp/example.rs" && fail "the marks of a documentation comment are taken off"
[ "$(grep -c -F 'MadeName {' "$tmp/example.rs")" = 1 ] || fail "one line of the example writes a MadeName"

printf '%s\n' '/// Text.' '///' '/// ```compile_fail' '/// let a = 1;' '///' '/// let b = 2;' '/// ```' \
  'fn f() {}' >"$tmp/one.rs"
"$check" --example "$tmp/one.rs" >"$tmp/out" || fail "one block is taken out"
[ "$(cat "$tmp/out")" = "$(printf 'let a = 1;\n\nlet b = 2;')" ] || { cat "$tmp/out" >&2; fail "the block, as written"; }
# None, two, and one that is never closed: each is refused.
printf '%s\n' '/// ```' '/// let a = 1;' '/// ```' >"$tmp/none.rs"
expect_fail "no block" "does not hold exactly one closed block" "$check" --example "$tmp/none.rs"
cat "$tmp/one.rs" "$tmp/one.rs" >"$tmp/two.rs"
expect_fail "two blocks" "does not hold exactly one closed block" "$check" --example "$tmp/two.rs"
printf '%s\n' '/// ```compile_fail' '/// let a = 1;' >"$tmp/open.rs"
expect_fail "a block never closed" "does not hold exactly one closed block" "$check" --example "$tmp/open.rs"

# --- What the compiler said is held to the one error.
private='error[E0451]: field `step` of struct `MadeName` is private'
both='error[E0451]: fields `step` and `name` of struct `MadeName` are private'
aborting='error: aborting due to 1 previous error'
printf '%s\n' "$private" ' --> careless.rs:6:23' "$aborting" >"$tmp/right"
expect_pass "the error it is there for" "$check" --judge 1 "$tmp/right" 0
grep -q -F "compile-fail: ok" "$tmp/out" || fail "a pass says so"
printf '%s\n' "$both" "$aborting" >"$tmp/right-both"
expect_pass "the same error, said of both fields at once" "$check" --judge 1 "$tmp/right-both" 0

# It compiled.
: >"$tmp/nothing"
expect_fail "an example that compiles" "the example on Ledger compiled" "$check" --judge 0 "$tmp/nothing" 0
# It failed for another reason, alone or beside the right one.
printf '%s\n' 'error[E0432]: unresolved import `whizbang_core::contract::MadeName`' "$aborting" >"$tmp/import"
expect_fail "another error alone" "fails for another reason" "$check" --judge 1 "$tmp/import" 0
grep -q -F "E0432" "$tmp/out" || fail "the other error is shown"
printf '%s\n' "$private" 'error[E0046]: not all trait items implemented, missing: `reserve`' "$aborting" >"$tmp/mixed"
expect_fail "another error beside the right one" "fails for another reason" "$check" --judge 1 "$tmp/mixed" 0
printf '%s\n' 'error: expected one of `!` or `::`, found `;`' >"$tmp/syntax"
expect_fail "an error with no code" "fails for another reason" "$check" --judge 1 "$tmp/syntax" 0
# The same error of another type is not the one.
printf '%s\n' 'error[E0451]: field `id` of struct `Step` is private' "$aborting" >"$tmp/other-type"
expect_fail "a private field of another type" "fails for another reason" "$check" --judge 1 "$tmp/other-type" 0
# It failed, and the compiler named no error.
expect_fail "a failure with no error" "without the error it is there for" "$check" --judge 1 "$tmp/nothing" 0
# The control does not compile.
expect_fail "a control that does not compile" "does not compile without the line" "$check" --judge 1 "$tmp/right" 1

# --- An argument it does not know is refused.
expect_fail "an unknown argument" "unknown argument" "$check" --run-this
status=0
"$check" --judge 1 >"$tmp/out" 2>&1 || status=$?
[ "$status" = 2 ] || fail "a judge without its three values exits 2 (got $status)"

finished
