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

finished
