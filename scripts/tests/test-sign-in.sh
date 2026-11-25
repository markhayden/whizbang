#!/usr/bin/env bash
# `just check` refuses a call of the function that names the real place of the Hub's
# sign-in anywhere but `main` (plan 1C, task N4; scripts/check-sign-in.sh). The repo as it
# is passes; each other way is planted in a folder of its own, never in the workspace, and
# is refused. Nothing is built or run, and no real place is read.
source "$(dirname "$0")/lib.sh"

check=scripts/check-sign-in.sh

# --- The repo itself. The script tests are part of `just check-full`, which `just ci` runs,
# and this one is among those the local `just check` runs on any change to the crates'
# sources (scripts/script-tests-local.sh), so this is where both refuse.
grep -q -x -F 'check-full: fmt-check clippy graph deny test-scripts docs-check ui-check timing-fast timing-sweeps test-pack' justfile ||
  fail "justfile: check-full runs the script tests, and this check with them"
grep -q -E '^part=".*scripts/tests/test-sign-in\.sh' scripts/script-tests-local.sh ||
  fail "scripts/script-tests-local.sh: the local check runs this test on a change to the crates' sources"
expect_pass "the workspace" "$check"
grep -q -F 'hub_sign_in_token_file is named in main.rs alone' "$tmp/out" || { cat "$tmp/out" >&2; fail "the workspace's main.rs is seen"; }

# --- Planted files.
repo="$tmp/repo"
mkdir -p "$repo/crates/whizbang/src" "$repo/crates/whizbang/tests/it" "$repo/crates/other/src"
main_rs="$repo/crates/whizbang/src/main.rs"
good_main() {
  printf '%s\n' \
    '// main names it, and hands it on: hub_sign_in_token_file() is not called here.' \
    'fn hub_sign_in_token_file() -> Option<std::path::PathBuf> {' \
    '    None' \
    '}' \
    'fn main() {' \
    '    let _ = models::run(' \
    '        &command,' \
    '        std::sync::Arc::new(platforms_models_dir),' \
    '        std::sync::Arc::new(hub_sign_in_token_file),' \
    '    );' \
    '}' >"$main_rs"
}
good_main
expect_pass "main defines it and hands it on" "$check" "$repo"

words='the one function that names the real place of the Hub'"'"'s sign-in'
planted="$repo/crates/whizbang/tests/it/planted.rs"
printf '%s\n' 'fn w() { let _ = hub_sign_in_token_file(); }' >"$planted"
expect_fail "a test calls it" "$words" "$check" "$repo"
grep -q -F "crates/whizbang/tests/it/planted.rs" "$tmp/out" || { cat "$tmp/out" >&2; fail "the refusal names the file"; }
printf '%s\n' 'fn w() {' '    let place =' '        hub_sign_in_token_file' '            ();' '}' >"$planted"
expect_fail "a call broken over lines" "$words" "$check" "$repo"
rm "$planted"
printf '%s\n' 'pub fn place() -> Option<std::path::PathBuf> { crate::hub_sign_in_token_file() }' >"$repo/crates/whizbang/src/wiring.rs"
expect_fail "another source of the binary calls it" "$words" "$check" "$repo"
rm "$repo/crates/whizbang/src/wiring.rs"
printf '%s\n' 'pub use whizbang::hub_sign_in_token_file as place;' >"$repo/crates/other/src/lib.rs"
expect_fail "another crate names it" "$words" "$check" "$repo"
rm "$repo/crates/other/src/lib.rs"
# A comment that names it is no call.
printf '%s\n' '// hub_sign_in_token_file() is main'"'"'s alone.' 'fn w() {}' >"$planted"
expect_pass "a comment names it" "$check" "$repo"
rm "$planted"

# In main.rs: called, handed on twice, or not defined.
printf '%s\n' 'fn hub_sign_in_token_file() -> Option<std::path::PathBuf> { None }' \
  'fn main() { let place = hub_sign_in_token_file(); let _ = std::sync::Arc::new(hub_sign_in_token_file); }' >"$main_rs"
expect_fail "main calls it" "it is defined once and handed on once" "$check" "$repo"
printf '%s\n' 'fn hub_sign_in_token_file() -> Option<std::path::PathBuf> { None }' \
  'fn main() { let _ = std::sync::Arc::new(hub_sign_in_token_file); let _ = std::sync::Arc::new(hub_sign_in_token_file); }' >"$main_rs"
expect_fail "main hands it on twice" "it is defined once and handed on once" "$check" "$repo"
printf '%s\n' 'fn main() {}' >"$main_rs"
expect_fail "main does not define it" "it is defined once and handed on once" "$check" "$repo"
# Handed on once, but outside the models commands' call: kept, and handed elsewhere.
printf '%s\n' 'fn hub_sign_in_token_file() -> Option<std::path::PathBuf> { None }' \
  'fn main() { let kept = std::sync::Arc::new(hub_sign_in_token_file); let _ = models::run(&command, std::sync::Arc::new(platforms_models_dir), kept); }' >"$main_rs"
expect_fail "main hands it on outside the call" "inside models::run" "$check" "$repo"
printf '%s\n' 'fn hub_sign_in_token_file() -> Option<std::path::PathBuf> { None }' \
  'fn main() { let _ = rascal::run(&command, std::sync::Arc::new(hub_sign_in_token_file)); }' >"$main_rs"
expect_fail "main hands it to another command" "inside models::run" "$check" "$repo"
good_main
expect_pass "main as it was" "$check" "$repo"

# --- The folder it is given.
expect_fail "a folder with no crates" "no folder" "$check" "$tmp"
expect_fail "two arguments" "usage" "$check" "$repo" "$repo"

finished
