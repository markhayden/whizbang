#!/usr/bin/env bash
# A thread is made in two places and nowhere else (clippy.toml): `just check` fails on a
# test, or anything else, that calls `std::thread::spawn`, `std::thread::scope` or a
# builder's `spawn`. Shown on a fixture crate whose one test file does each: it is linted
# under this repo's clippy.toml with the flags `just clippy` uses, and never run. The
# fixture has no dependency, so this builds nothing but it.
source "$(dirname "$0")/lib.sh"

fixture=scripts/tests/fixtures/bare-thread
lint() { # <what to lint: --all-targets, or --lib>
  CLIPPY_CONF_DIR="$root" CARGO_TARGET_DIR="$root/target/script-tests" \
    cargo clippy --quiet --manifest-path "$fixture/Cargo.toml" "$1" -- -D warnings
}

# The lint looks at tests because `just clippy` asks for every target, and fails on a
# warning.
grep -q -x -F '    cargo clippy --workspace --all-targets -- -D warnings' justfile ||
  fail "justfile: clippy must lint every target of the workspace, tests among them, and fail on a warning"

expect_fail "a test file that makes threads itself" "use of a disallowed method" lint --all-targets
for refused in 'std::thread::spawn' 'std::thread::scope' 'std::thread::Scope::spawn' 'std::thread::Builder::spawn'; do
  grep -q -F "use of a disallowed method \`$refused\`" "$tmp/out" ||
    { cat "$tmp/out" >&2; fail "clippy refuses $refused in a test file"; }
done
grep -q -F "whizbang_testkit::threads" "$tmp/out" || { cat "$tmp/out" >&2; fail "the refusal names what to use instead"; }
# The same crate without its test file is clean: what failed above is the test file.
expect_pass "the fixture's library alone" lint --lib

# The lint is allowed in two files, and nothing else in the workspace mentions it: the
# executor's spawner, once, and the test kit's helper, for a thread, a scope and a thread
# of a scope.
allowed=$(grep -r -l -F 'disallowed_methods' crates --include='*.rs' | sort | tr '\n' ' ')
[ "$allowed" = "crates/whizbang-core/src/contract/engine.rs crates/whizbang-testkit/src/threads.rs " ] ||
  fail "clippy::disallowed_methods is allowed in the executor's spawner and the test kit's helper only (found in: $allowed)"
[ "$(grep -c -F 'allow(clippy::disallowed_methods)' crates/whizbang-core/src/contract/engine.rs)" = 1 ] ||
  fail "the executor allows the lint once, on its spawner"
[ "$(grep -c -F 'allow(clippy::disallowed_methods)' crates/whizbang-testkit/src/threads.rs)" = 3 ] ||
  fail "the test kit's helper allows the lint three times: a thread, a scope, a thread of a scope"
grep -B1 -F 'fn spawn_thread(' crates/whizbang-core/src/contract/engine.rs | grep -F 'allow(clippy::disallowed_methods)' >/dev/null ||
  fail "the executor's allow is on spawn_thread"

# Every way a thread is made is named in clippy.toml. The fixture shows four of the six
# being refused; the other two are a builder's scoped and unchecked spawns, of which the
# second needs `unsafe`, which this workspace forbids, so both are held by their lines.
for path in 'std::thread::spawn' 'std::thread::scope' 'std::thread::Scope::spawn' 'std::thread::Builder::spawn' \
  'std::thread::Builder::spawn_scoped' 'std::thread::Builder::spawn_unchecked'; do
  grep -q -F "{ path = \"$path\", reason = " clippy.toml || fail "clippy.toml refuses $path"
done

# An allow of a whole group silences the lint with it, and names no lint: none is allowed or
# expected anywhere in the crates, in any form of attribute.
check_groups() { # <folder>
  local found
  found=$(grep -r -n -E '(allow|expect)[[:space:]]*\([^)]*(warnings|clippy::all|clippy::style)' "$1" --include='*.rs' || true)
  [ -z "$found" ] || { echo "a group of lints is allowed, and the thread lint with it: $found"; return 1; }
}
expect_pass "no group of lints is allowed in the crates" check_groups crates
mkdir -p "$tmp/group/src"
for attribute in '#![allow(clippy::style)]' '#[allow(clippy::all)]' '#[expect(warnings)]' \
  '#![cfg_attr(test, allow(unused, clippy::style))]' '#[allow( warnings )]'; do
  printf '%s\nfn f() {}\n' "$attribute" >"$tmp/group/src/lib.rs"
  expect_fail "a file with $attribute" "a group of lints is allowed" check_groups "$tmp/group"
done
printf '#[allow(clippy::too_many_arguments)]\nfn f() {}\n' >"$tmp/group/src/lib.rs"
expect_pass "an allow of one lint by its name" check_groups "$tmp/group"


finished
