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
# expected anywhere in the crates, in any form of attribute, on one line or over several.
# Each file is read with its white space made single spaces, so a line break inside the
# attribute hides nothing.
check_groups() { # <folder>
  local file found=""
  while IFS= read -r file; do
    if tr -s '[:space:]' ' ' <"$file" | grep -E '(allow|expect) ?\([^)]*(warnings|clippy :: all|clippy::all|clippy :: style|clippy::style)' >/dev/null; then
      found="$found $file"
    fi
  done < <(find "$1" -type f -name '*.rs' ! -path '*/target/*')
  [ -z "$found" ] || { echo "a group of lints is allowed, and the thread lint with it:$found"; return 1; }
}
expect_pass "no group of lints is allowed in the crates" check_groups crates
mkdir -p "$tmp/group/src"
for attribute in '#![allow(clippy::style)]' '#[allow(clippy::all)]' '#[expect(warnings)]' \
  '#![cfg_attr(test, allow(unused, clippy::style))]' '#[allow( warnings )]' \
  "$(printf '#[allow(\n    unused,\n    clippy::style,\n)]')" "$(printf '#![expect\n(\nwarnings\n)]')"; do
  printf '%s\nfn f() {}\n' "$attribute" >"$tmp/group/src/lib.rs"
  expect_fail "a file with $attribute" "a group of lints is allowed" check_groups "$tmp/group"
done
printf '#[allow(clippy::too_many_arguments)]\nfn f() {}\n' >"$tmp/group/src/lib.rs"
expect_pass "an allow of one lint by its name" check_groups "$tmp/group"

# A manifest can allow a lint for a whole crate or the workspace. None sets the thread lint,
# or a group that holds it, or every warning, to allow.
check_manifests() { # <folder>
  local found
  found=$(find "$1" -type f -name Cargo.toml ! -path '*/target/*' \
    -exec grep -n -H -E '^[[:space:]]*"?(disallowed_methods|style|all|warnings)"?[[:space:]]*=.*"allow"' {} + || true)
  [ -z "$found" ] || { echo "a manifest allows the thread lint or a group that holds it: $found"; return 1; }
}
expect_pass "no manifest allows the thread lint" check_manifests .
mkdir -p "$tmp/manifest"
for setting in 'disallowed_methods = "allow"' 'style = "allow"' 'all = { level = "allow", priority = -1 }' \
  '  warnings = "allow"' '"style" = "allow"'; do
  printf '[workspace.lints.clippy]\n%s\n' "$setting" >"$tmp/manifest/Cargo.toml"
  expect_fail "a manifest with $setting" "a manifest allows the thread lint" check_manifests "$tmp/manifest"
done
printf '[workspace.lints.clippy]\nall = { level = "warn", priority = -1 }\nunwrap_used = "allow"\n' >"$tmp/manifest/Cargo.toml"
expect_pass "a manifest that allows another lint" check_manifests "$tmp/manifest"

# clippy reads the clippy.toml nearest a crate, so a second one would take the list away
# from the crates under it. There is one, at the root.
check_one_list() { # <folder>
  local found
  found=$(cd "$1" && find . -type f \( -name clippy.toml -o -name .clippy.toml \) ! -path './target/*' ! -path './clippy.toml' | sort | tr '\n' ' ')
  [ -z "$found" ] || { echo "a second list of what clippy refuses: $found"; return 1; }
}
expect_pass "one clippy.toml, at the root" check_one_list .
mkdir -p "$tmp/lists/crates/x"
: >"$tmp/lists/clippy.toml"
expect_pass "a root with its own list alone" check_one_list "$tmp/lists"
: >"$tmp/lists/crates/x/clippy.toml"
expect_fail "a clippy.toml in a crate" "a second list" check_one_list "$tmp/lists"
rm "$tmp/lists/crates/x/clippy.toml"
: >"$tmp/lists/.clippy.toml"
expect_fail "a hidden clippy.toml" "a second list" check_one_list "$tmp/lists"

# cargo's own config can hand the compiler flags that cap or allow lints. No config in this
# repo does.
check_flags() { # <folder>
  local found
  found=$(find "$1" -type f \( -path '*/.cargo/config.toml' -o -path '*/.cargo/config' \) ! -path '*/target/*' \
    -exec grep -n -H -E -e '--cap-lints|(^|[^A-Za-z0-9_-])-A([^A-Za-z0-9_-]|$)|(^|[^A-Za-z0-9_-])-A[a-z]|--allow' {} + || true)
  [ -z "$found" ] || { echo "a cargo config caps or allows lints: $found"; return 1; }
}
expect_pass "no cargo config caps or allows lints" check_flags .
mkdir -p "$tmp/flags/.cargo"
for flags in 'rustflags = ["--cap-lints", "allow"]' 'rustflags = ["-A", "clippy::disallowed_methods"]' \
  'rustflags = ["-Aclippy::style"]' 'rustflags = "--allow warnings"'; do
  printf '[build]\n%s\n' "$flags" >"$tmp/flags/.cargo/config.toml"
  expect_fail "a cargo config with $flags" "a cargo config caps or allows lints" check_flags "$tmp/flags"
done
printf '[build]\nrustflags = ["-D", "warnings"]\n[alias]\nA-thing = "check"\n' >"$tmp/flags/.cargo/config.toml"
expect_pass "a cargo config that denies" check_flags "$tmp/flags"

finished
