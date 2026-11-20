#!/usr/bin/env bash
# The check that the tests and the shipped binary build serde_json with the same features:
# the real workspace passes, and a feature turned on in one crate only fails. Differences are
# planted in saved answers of `cargo tree`, or in a copy of the workspace under the test's
# own folder, never in the workspace.
source "$(dirname "$0")/lib.sh"

check=scripts/check-features.sh

expect_pass "the workspace" "$check"

echo 'serde_json v1.0.154 default,float_roundtrip,std' >"$tmp/shipped"
cp "$tmp/shipped" "$tmp/same"
expect_pass "the same features" "$check" --shipped "$tmp/shipped" --workspace "$tmp/same"

echo 'serde_json v1.0.154 arbitrary_precision,default,float_roundtrip,std' >"$tmp/more"
expect_fail "a feature only the tests build with" "serde_json's features differ" \
  "$check" --shipped "$tmp/shipped" --workspace "$tmp/more"
expect_fail "a feature only the binary builds with" "the binary (-p whizbang): arbitrary_precision" \
  "$check" --shipped "$tmp/more" --workspace "$tmp/shipped"

echo 'serde_json v1.0.155 default,float_roundtrip,std' >"$tmp/other"
expect_fail "another version" "the binary builds serde_json v1.0.154 and the workspace v1.0.155" \
  "$check" --shipped "$tmp/shipped" --workspace "$tmp/other"

: >"$tmp/empty"
expect_fail "no answer" "cannot read the answer" \
  "$check" --shipped "$tmp/empty" --workspace "$tmp/shipped"
cat "$tmp/shipped" "$tmp/other" >"$tmp/two"
expect_fail "two versions" "cannot read the answer" \
  "$check" --shipped "$tmp/shipped" --workspace "$tmp/two"
echo 'error: package ID specification `serde_json` did not match any packages' >"$tmp/error"
expect_fail "an error of cargo" "cannot read the answer" \
  "$check" --shipped "$tmp/error" --workspace "$tmp/shipped"
expect_fail "one answer alone" "--shipped and --workspace go together" \
  "$check" --shipped "$tmp/shipped"

# The mistake itself, in a copy of the workspace: a crate the binary does not depend on
# turns on `arbitrary_precision`. The copy holds the manifests, the lock file and one empty
# source file per crate, which is all `cargo tree` reads.
copy="$tmp/copy"
mkdir -p "$copy"
cp Cargo.toml Cargo.lock "$copy/"
for manifest in crates/*/Cargo.toml; do
  crate=$(dirname "$manifest")
  mkdir -p "$copy/$crate/src"
  cp "$manifest" "$copy/$crate/"
  : >"$copy/$crate/src/lib.rs"
  : >"$copy/$crate/src/main.rs"
done
mkdir -p "$copy/scripts"
cp "$check" "$copy/scripts/"
(cd "$copy" && git init -q . && "$PWD/scripts/check-features.sh") >"$tmp/out" 2>&1 \
  || { cat "$tmp/out" >&2; fail "the copy of the workspace: expected success"; }
grep -F -e 'features: ok' "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "the copy: no ok"; }
# The testkit: since task E1 of plan 2C the binary depends on whizbang-eval, which this
# planted the feature in before.
sed 's/^serde_json.workspace = true$/serde_json = { workspace = true, features = ["arbitrary_precision"] }/' \
  crates/whizbang-testkit/Cargo.toml >"$copy/crates/whizbang-testkit/Cargo.toml"
grep -F -e 'arbitrary_precision' "$copy/crates/whizbang-testkit/Cargo.toml" >/dev/null \
  || fail "the copy's whizbang-testkit has no serde_json line to plant the feature in"
expect_fail "a crate the binary does not depend on turns the feature on" \
  "the workspace and its tests: arbitrary_precision" \
  bash -c 'cd "$1" && scripts/check-features.sh' _ "$copy"

# The tests' feature `fakes` in the binary's own build: the copy's whizbang-testkit is put
# back, and the binary's dependency on whizbang-backends turns `fakes` on.
cp crates/whizbang-testkit/Cargo.toml "$copy/crates/whizbang-testkit/Cargo.toml"
(cd "$copy" && "$PWD/scripts/check-features.sh") >"$tmp/clean" 2>&1 \
  || { cat "$tmp/clean" >&2; fail "the copy put back: expected success"; }
grep -F -e 'features: ok (the binary is built without the feature fakes)' "$tmp/clean" >/dev/null \
  || { cat "$tmp/clean" >&2; fail "the copy: the binary was not checked for fakes"; }
sed 's/^whizbang-backends.workspace = true$/whizbang-backends = { workspace = true, features = ["fakes"] }/' \
  crates/whizbang/Cargo.toml >"$copy/crates/whizbang/Cargo.toml"
grep -F -e 'whizbang-backends = { workspace = true, features = ["fakes"] }' "$copy/crates/whizbang/Cargo.toml" >/dev/null \
  || fail "the copy's whizbang has no whizbang-backends line to plant the feature in"
expect_fail "the binary's own build turns the tests' feature fakes on" \
  "is built with the feature fakes" \
  bash -c 'cd "$1" && scripts/check-features.sh' _ "$copy"

# The same for the core's `fakes` (an executor built with a digest alone: plan m4, task K1,
# item 5): the copy's whizbang is put back, and the binary's dependency on whizbang-core
# turns it on.
cp crates/whizbang/Cargo.toml "$copy/crates/whizbang/Cargo.toml"
sed 's/^whizbang-core.workspace = true$/whizbang-core = { workspace = true, features = ["fakes"] }/' \
  crates/whizbang/Cargo.toml >"$copy/crates/whizbang/Cargo.toml"
grep -F -e 'whizbang-core = { workspace = true, features = ["fakes"] }' "$copy/crates/whizbang/Cargo.toml" >/dev/null \
  || fail "the copy's whizbang has no whizbang-core line to plant the feature in"
expect_fail "the binary's own build turns the core's feature fakes on" \
  "is built with the feature fakes" \
  bash -c 'cd "$1" && scripts/check-features.sh' _ "$copy"

# The baseline replay in the binary's own build: the copy's whizbang is put back, then its
# dependency on whizbang-squint turns `baseline-replay` on, which brings jsonschema.
cp crates/whizbang/Cargo.toml "$copy/crates/whizbang/Cargo.toml"
(cd "$copy" && "$PWD/scripts/check-features.sh") >"$tmp/clean" 2>&1 \
  || { cat "$tmp/clean" >&2; fail "the copy put back again: expected success"; }
grep -F -e 'features: ok (the binary links no jsonschema)' "$tmp/clean" >/dev/null \
  || { cat "$tmp/clean" >&2; fail "the copy: the binary was not checked for jsonschema"; }
sed 's/^whizbang-squint.workspace = true$/whizbang-squint = { workspace = true, features = ["baseline-replay"] }/' \
  crates/whizbang/Cargo.toml >"$copy/crates/whizbang/Cargo.toml"
grep -F -e 'features = ["baseline-replay"] }' "$copy/crates/whizbang/Cargo.toml" >/dev/null \
  || fail "the copy's whizbang has no whizbang-squint line to plant the feature in"
expect_fail "the binary's own build turns the baseline replay on" \
  "links jsonschema" \
  bash -c 'cd "$1" && scripts/check-features.sh' _ "$copy"

finished
