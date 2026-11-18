#!/usr/bin/env bash
# A test starts the real binary through the test kit's one function and no other way
# (scripts/check-binary-start.sh). The repo as it is passes; each other way is planted in
# a folder of its own, never in the workspace, and is refused. Nothing is built or run.
source "$(dirname "$0")/lib.sh"

check=scripts/check-binary-start.sh
call='whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))'

# --- The repo itself. The script tests are part of `just check-full`, which `just ci` runs,
# and this one is among those the local `just check` runs on any change to the crates'
# sources (scripts/script-tests-local.sh), so this is where both refuse: the check passes
# on the workspace, and it has seen the places that start the binary (a check that found
# none would be looking in the wrong place).
grep -q -x -F 'check-full: fmt-check clippy graph deny test-scripts docs-check timing test-pack' justfile ||
  fail "justfile: check-full runs the script tests, and this check with them"
grep -q -E '^part=".*scripts/tests/test-binary-start\.sh' scripts/script-tests-local.sh ||
  fail "scripts/script-tests-local.sh: the local check runs this test on a change to the crates' sources"
expect_pass "the workspace" "$check"
started=$(sed -n 's/^binary start: ok (.* the real binary is started in \([0-9][0-9]*\) places, .*$/\1/p' "$tmp/out")
# Exactly the number the plan states (N3c, "Shown by"): a test that starts the binary is
# added to both, or the check has stopped seeing some.
[ "$started" = 23 ] ||
  { cat "$tmp/out" >&2; fail "the check counts the places that start the real binary: twenty-three, not ${started:-none}"; }
grep -q -F 'with twenty-three places counted' docs/plans/m1c-local-pack.md ||
  fail "the plan states the number of places, twenty-three"
# The function is the test kit's, clears the environment, and is the only thing in the
# crates that does: no test builds a cleared command of its own beside it.
grep -q -F 'pub fn command(path: impl AsRef<OsStr>) -> Binary' crates/whizbang-testkit/src/binary.rs ||
  fail "the test kit has the function, binary::command"
grep -q -F '.env_clear()' crates/whizbang-testkit/src/binary.rs || fail "the function clears the environment"

# --- Planted files. A folder with one crate that starts the binary the right way, and
# a file beside it that a case writes.
repo="$tmp/repo"
mkdir -p "$repo/crates/whizbang/tests/it" "$repo/crates/whizbang/src" "$repo/crates/other/tests"
good="$repo/crates/whizbang/tests/it/good.rs"
planted="$repo/crates/whizbang/tests/it/planted.rs"
cat >"$good" <<RS
// A comment may name env!("CARGO_BIN_EXE_whizbang"), and "HOME".
fn whizbang(args: &[&str]) -> std::process::Output {
    $call
        .args(args)
        .env("WHIZBANG_MADE_UP", "1")
        .output()
        .unwrap()
}
const VERSION: &str = env!("CARGO_PKG_VERSION");
RS
# The binary's own source reads the environment: that is not a test's.
printf '%s\n' 'fn home() -> Option<std::ffi::OsString> { std::env::var_os("HOME") }' >"$repo/crates/whizbang/src/main.rs"
expect_pass "a crate that starts the binary through the function" "$check" "$repo"
grep -q -F 'started in 1 places' "$tmp/out" || { cat "$tmp/out" >&2; fail "one place is counted"; }

# The call as the formatter breaks it over lines is the same call.
printf '%s\n' 'fn w() {' '    let mut c = whizbang_testkit::binary::command(' '        env!("CARGO_BIN_EXE_whizbang"),' '    );' '    c.arg("x");' '}' >"$planted"
expect_pass "the call broken over lines" "$check" "$repo"
grep -q -F 'started in 2 places' "$tmp/out" || { cat "$tmp/out" >&2; fail "two places are counted"; }

# refused <name> <the message's words> <line...>: the lines, as the planted file, fail.
refused() {
  local name=$1 words=$2
  shift 2
  printf '%s\n' "$@" >"$planted"
  expect_fail "$name" "$words" "$check" "$repo"
  grep -q -F "crates/whizbang/tests/it/planted.rs" "$tmp/out" || { cat "$tmp/out" >&2; fail "$name: the refusal names the file"; }
  grep -q -F "good.rs" "$tmp/out" && { cat "$tmp/out" >&2; fail "$name: the file that is right is not named"; }
  return 0
}

# The path anywhere but as the function's argument.
only='the real binary is started only as whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))'
refused "the path given to Command::new" "$only" \
  'fn w() { std::process::Command::new(env!("CARGO_BIN_EXE_whizbang")).output().unwrap(); }'
refused "the path as a constant" "$only" \
  'const BIN: &str = env!("CARGO_BIN_EXE_whizbang");' \
  'fn w() { std::process::Command::new(BIN).output().unwrap(); }'
refused "the path from a helper" "$only" \
  'fn bin() -> &'"'"'static str {' '    env!("CARGO_BIN_EXE_whizbang")' '}'
refused "the path in a variable for a script" "$only" \
  'fn w() { std::process::Command::new("bash").env("WHIZBANG", env!("CARGO_BIN_EXE_whizbang")); }'
refused "the path beside a right call" "$only" \
  "fn w() { $call.output().unwrap(); }" \
  'fn v() -> &'"'"'static str { env!("CARGO_BIN_EXE_whizbang") }'
refused "the path read when the test is built, if it is there" "$only" \
  'fn w() -> Option<&'"'"'static str> { option_env!("CARGO_BIN_EXE_whizbang") }'
refused "the path read when the test runs" "$only" \
  'fn w() -> String { std::env::var("CARGO_BIN_EXE_whizbang").unwrap() }'
refused "the path with a message" "$only" \
  'fn w() { whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang", "built with cargo")); }'
refused "another binary's path" "$only" \
  'fn w() { whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_other")); }'
refused "the path after a comment on its own line" "$only" \
  'fn w() {' '    // the binary' '    let bin = env!("CARGO_BIN_EXE_whizbang");' '}'
# The function's name written whole: a shorter name could be anything's.
refused "the function by a shorter name" "$only" \
  'use whizbang_testkit::binary;' 'fn w() { binary::command(env!("CARGO_BIN_EXE_whizbang")); }'
refused "a function of the test's own with the same last names" "$only" \
  'mod binary { pub fn command(p: &str) -> std::process::Command { std::process::Command::new(p) } }' \
  'fn w() { binary::command(env!("CARGO_BIN_EXE_whizbang")); }'
refused "a longer name that ends the same" "$only" \
  'fn w() { my_whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang")); }'
refused "a path that ends the same" "$only" \
  'fn w() { crate::whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang")); }'
refused "a module of the test's own named as the test kit" "it gives the name whizbang_testkit" \
  'mod whizbang_testkit { pub mod binary { pub fn command(p: &str) -> std::process::Command { std::process::Command::new(p) } } }' \
  "fn w() { $call; }"
refused "another crate brought in under the test kit's name" "it gives the name whizbang_testkit" \
  'use other   as   whizbang_testkit;' "fn w() { $call; }"
# The name put together from pieces, or a variable of whoever builds.
refused "the name in pieces" "env! or option_env! of something other" \
  'fn w() -> &'"'"'static str { env!(concat!("CARGO_BIN", "_EXE_whizbang")) }'
refused "a variable of whoever builds the test" "env! or option_env! of something other" \
  'const REAL: &str = env!("HOME");'
refused "a variable of whoever builds the test, if it is there" "env! or option_env! of something other" \
  'const TOKEN: Option<&str> = option_env!("HF_TOKEN");'

# In every crate, not only the binary's.
printf '%s\n' 'const BIN: &str = env!("CARGO_BIN_EXE_whizbang");' >"$repo/crates/other/tests/it.rs"
printf '%s\n' "fn w() { $call; }" >"$planted"
expect_fail "the path in another crate's test" "crates/other/tests/it.rs" "$check" "$repo"
printf '%s\n' 'const HOME: &str = env!("HOME");' >"$repo/crates/other/tests/it.rs"
expect_fail "a builder's variable in another crate's test" "crates/other/tests/it.rs" "$check" "$repo"
# A crate that does not start the binary reads the environment as it likes.
printf '%s\n' 'fn w() -> Option<String> { std::env::var("HOME").ok() }' >"$repo/crates/other/tests/it.rs"
expect_pass "another crate's test reads its own environment" "$check" "$repo"

# --- The environment is not put back. In a crate that starts the binary, no test file
# reads the environment it runs in, or names a place the function sets.
reads='such a test reads nothing of the environment it runs in'
places="the home and the folders are the function's"
refused "the whole environment handed on" "$reads" \
  "fn w() { $call.envs(std::env::vars()).output().unwrap(); }"
refused "the whole environment handed on, in the other form" "$reads" \
  "fn w() { $call.envs(std::env::vars_os()).output().unwrap(); }"
refused "the real home handed on" "$reads" \
  "fn w() { $call.env(\"HOME\", std::env::var(\"HOME\").unwrap()).output().unwrap(); }"
refused "a token handed on" "$reads" \
  "fn w() { $call.env(\"HF_TOKEN\", std::env::var_os(\"HF_TOKEN\").unwrap()); }"
refused "the reading brought in by name" "$reads" \
  'use std::env::{var, vars};' "fn w() { $call.envs(vars()); }"
refused "everything of env brought in" "$reads" \
  'use std::env::*;' "fn w() { $call.envs(vars()); }"
refused "the home asked of the standard library" "$reads" \
  "fn w() { $call.env(\"H\", std::env::home_dir().unwrap()); }"
refused "the whole environment by another name for env" "\`vars_os\`" \
  'use std::env as e;' "fn w() { $call.envs(e::vars_os()); }"
refused "one variable by another name for env" "\`var_os\`" \
  'use std::env as e;' "fn w() { $call.env(\"T\", e::var_os(\"T\").unwrap()); }"
refused "the home of another platform set" "$places" \
  "fn w(real: &str) { $call.env(\"USERPROFILE\", real).output().unwrap(); }"
refused "the home set to another place" "$places" \
  "fn w(real: &str) { $call.env(\"HOME\", real).output().unwrap(); }"
refused "the temporary folder set to another place" "$places" \
  "fn w() { $call.env(\"TMPDIR\", \"/tmp\").output().unwrap(); }"
refused "the data folder set to another place" "$places" \
  "fn w(real: &str) { $call.env(\"XDG_DATA_HOME\", real).output().unwrap(); }"
refused "the home put back by removing it" "$places" \
  "fn w() { $call.env_remove(\"HOME\").output().unwrap(); }"
clears='the function sets the environment, and a test adds to it and takes nothing from it'
refused "the environment cleared again" "\`.env_clear(\`" \
  "fn w() { $call.env_clear().output().unwrap(); }"
refused "a variable of the function's taken away by a name in a constant" "\`.env_remove(\`" \
  'const H: &str = concat!("HO", "ME");' "fn w() { $call.env_remove(H).output().unwrap(); }"
refused "the clearing broken over lines" "$clears" \
  'fn w() {' "    $call" '        .env_clear()' '        .output()' '        .unwrap();' '}'
# A helper in another file of the same tests is held too: the files are one program.
printf '%s\n' "fn w() { $call; }" >"$planted"
printf '%s\n' 'pub fn real_home() -> String { std::env::var("HOME").unwrap() }' >"$repo/crates/whizbang/tests/it/helper.rs"
expect_fail "a helper beside the test reads the real home" "crates/whizbang/tests/it/helper.rs" "$check" "$repo"
rm "$repo/crates/whizbang/tests/it/helper.rs"

# What a test may do: variables of its own, a home asked of the command, cargo's own
# variables, and a comment that names what is refused.
printf '%s\n' \
  '// Not Command::new(env!("CARGO_BIN_EXE_whizbang")), and no std::env::var("HOME").' \
  'fn w() {' \
  "    let mut command = $call;" \
  '    let home = command.home().to_path_buf();' \
  '    command.env("HF_TOKEN", "made-up").env("WHIZBANG_HOME_SEEN", home).current_dir(env!("CARGO_MANIFEST_DIR"));' \
  '}' \
  'const NAMED: Option<&str> = option_env!("CARGO_PKG_NAME");' >"$planted"
expect_pass "a test that adds its own variables" "$check" "$repo"

# --- The folder it is given.
expect_fail "a folder with no crates" "no folder" "$check" "$tmp"
expect_fail "two arguments" "usage" "$check" "$repo" "$repo"

finished
