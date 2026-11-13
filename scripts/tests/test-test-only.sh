#!/usr/bin/env bash
# `just test-only` (scripts/test-only.sh) runs some of the tests and takes a closed set of
# arguments. It is on the allow list (`.claude/settings.json`), so what it accepts is what
# anyone may run without a prompt: an argument that could name a program to run, a config,
# or a manifest elsewhere is refused before anything runs.
#
# Nothing here runs a test or builds: the script is asked, with WHIZBANG_TEST_ONLY_DRY=1, to
# print the command it would run. A refused argument is shown to have run nothing by a
# runner script that would write a file, which is never written.
source "$(dirname "$0")/lib.sh"

script=scripts/test-only.sh

# The recipe hands its arguments to the script and to nothing else.
grep -q -x -F '    scripts/test-only.sh "$@"' justfile ||
  fail "justfile: the recipe test-only must hand its arguments to scripts/test-only.sh"
[ -x "$script" ] || fail "$script is there and may be run"
grep -q -x -F 'exec scripts/thread-guard.sh cargo nextest run --manifest-path "$root/Cargo.toml" "$@"' "$script" ||
  fail "$script runs the tests through scripts/thread-guard.sh, with this repo's manifest"

export WHIZBANG_TEST_ONLY_DRY=1
marker="$tmp/ran"
printf '#!/bin/sh\ntouch "%s"\n' "$marker" >"$tmp/runner.sh"
chmod +x "$tmp/runner.sh"

# accepted <name> <arguments...>: the script exits 0 and prints the guarded command, with
# this repo's manifest before the arguments as they were given.
accepted() {
  local name=$1 want
  shift
  expect_pass "$name" "$script" "$@"
  want=$(printf '%s\n' scripts/thread-guard.sh cargo nextest run --manifest-path "$root/Cargo.toml" "$@")
  [ "$(cat "$tmp/out")" = "$want" ] || { cat "$tmp/out" >&2; fail "$name: the command is the guarded one, with the arguments as given"; }
}
accepted "one crate" -p whizbang-core
accepted "one crate, by the long word" --package whizbang-testkit
accepted "one crate and a filter word" -p whizbang-core watch::
accepted "a filter word alone" contract::admit::two_simultaneous_requests
accepted "the library's tests" -p whizbang-core --lib
accepted "one test binary" -p whizbang-core --test it
accepted "a filter expression" -E 'test(=contract::watch::a_name) & package(whizbang-core)'
accepted "a filter expression with or, not and a path" -E 'test(/store::/) | !binary(it) ~ a.b-c'
accepted "output shown and every failure" -p whizbang --no-capture --no-fail-fast
# Through the recipe, as it is typed.
expect_pass "the recipe" just test-only -p whizbang-core watch::
grep -q -x -F 'watch::' "$tmp/out" || { cat "$tmp/out" >&2; fail "the recipe hands its arguments on"; }

# refused <name> <arguments...>: exit 2, one line that says so, and nothing ran.
refused() {
  local name=$1 status=0
  shift
  "$script" "$@" >"$tmp/out" 2>&1 || status=$?
  [ "$status" = 2 ] || { cat "$tmp/out" >&2; fail "$name: refused with exit 2 (got $status)"; }
  [ "$(grep -c . "$tmp/out")" = 1 ] || { cat "$tmp/out" >&2; fail "$name: one line says why"; }
  grep -q -F "test-only: refused:" "$tmp/out" || { cat "$tmp/out" >&2; fail "$name: the line says it was refused"; }
  grep -q -F "thread-guard.sh" "$tmp/out" && { cat "$tmp/out" >&2; fail "$name: no command was made"; }
  [ ! -e "$marker" ] || fail "$name: something ran"
}
# What the review ran: a config that names a runner.
refused "a config naming a runner" --config "target.'cfg(all())'.runner='$tmp/runner.sh'" -p whizbang-core
refused "a config in one word" "--config=target.'cfg(all())'.runner='$tmp/runner.sh'"
refused "a config naming a compiler wrapper" --config "build.rustc-wrapper='$tmp/runner.sh'"
refused "a manifest elsewhere" --manifest-path "$tmp/Cargo.toml"
refused "a manifest in one word" "--manifest-path=$tmp/Cargo.toml"
refused "a config file" --config-file "$tmp/nextest.toml"
refused "a tool's config file" --tool-config-file "tool:$tmp/nextest.toml"
refused "an archive" --archive-file "$tmp/archive.tar.zst"
refused "a workspace moved" --workspace-remap "$tmp"
refused "a profile" --profile ci
refused "a target folder" --target-dir "$tmp"
refused "an option nextest has and this does not" --workspace
refused "the end of options" -- --ignored
refused "a crate that is not in the workspace" -p serde
refused "a crate by a path" -p ../x
refused "a crate with no name" -p
refused "a crate's name in one word" --package=whizbang-core
refused "a test binary by a path" --test ../it
refused "a filter word with a path" crates/whizbang-core
refused "a word with a semicolon" 'watch;touch'
refused "a word with a command in it" "\$(touch $marker)"
refused "a word with a line break" "$(printf 'watch\n--config')"
refused "a filter with a backtick" -E "test(\`touch $marker\`)"
refused "a filter with a dollar" -E 'test($HOME)'
refused "a filter with a quote" -E "test('x')"
refused "a filter with a redirect" -E 'test(x) > y'
refused "a filter with a line break" -E "$(printf 'test(x)\n--config')"
refused "a filter with nothing" -E
refused "no arguments"
# What is refused is named, with nothing in it that is not a plain character.
"$script" "$(printf 'bad\033[31mword')" >"$tmp/out" 2>&1 || true
LC_ALL=C grep -q '[^ -~]' "$tmp/out" && { od -c "$tmp/out" >&2; fail "the refusal prints its argument escaped"; }
grep -q -F 'bad' "$tmp/out" || { cat "$tmp/out" >&2; fail "the refusal names the argument"; }
"$script" "$(printf 'x%.0s' $(seq 1 300))/" >"$tmp/out" 2>&1 || true
[ "$(wc -c <"$tmp/out")" -lt 200 ] || fail "the refusal cuts a long argument"

# The environment cannot say what the command line may not: the variables that name a
# program to run in a build's place, or a config, are not handed on.
handed=$(RUSTC_WRAPPER="$tmp/runner.sh" CARGO_BUILD_RUSTC_WRAPPER="$tmp/runner.sh" RUSTC_WORKSPACE_WRAPPER="$tmp/runner.sh" \
  CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER="$tmp/runner.sh" CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$tmp/runner.sh" \
  NEXTEST_PROFILE=ci NEXTEST_TARGET_RUNNER="$tmp/runner.sh" RUSTFLAGS="-C linker=$tmp/runner.sh" CARGO_ENCODED_RUSTFLAGS=x \
  CARGO_BUILD_RUSTFLAGS=x RUSTC="$tmp/runner.sh" CARGO_BUILD_RUSTC="$tmp/runner.sh" RUSTDOC="$tmp/runner.sh" \
  CARGO_HOME_KEPT=yes WHIZBANG_TEST_ONLY_DRY=env "$script" -p whizbang-core)
for name in RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER \
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER NEXTEST_PROFILE NEXTEST_TARGET_RUNNER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS \
  CARGO_BUILD_RUSTFLAGS RUSTC CARGO_BUILD_RUSTC RUSTDOC; do
  printf '%s\n' "$handed" | grep -q "^$name=" && fail "the command is not handed $name"
done
printf '%s\n' "$handed" | grep -q -x -F 'CARGO_HOME_KEPT=yes' || fail "the rest of the environment is handed on"

# It runs from this repo, whatever folder it was started in, so a config beside the caller
# is not read.
mkdir -p "$tmp/elsewhere/.cargo"
at=$(cd "$tmp/elsewhere" && WHIZBANG_TEST_ONLY_DRY=pwd "$root/$script" -p whizbang-core)
[ "$at" = "$root" ] || fail "the command runs in this repo's folder (it would run in $at)"

finished
