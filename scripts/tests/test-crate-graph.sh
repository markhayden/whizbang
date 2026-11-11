#!/usr/bin/env bash
# The crate-graph check: the real workspace passes, and any edge that is not listed fails.
# Forbidden edges are planted in a copy of the metadata or of the edge list, never in the
# workspace.
source "$(dirname "$0")/lib.sh"

check=scripts/check-crate-graph.sh
cargo metadata --format-version 1 --no-deps >"$tmp/metadata.json"

expect_pass "the workspace, from a metadata file" "$check" --metadata "$tmp/metadata.json"

# plant <from> <to> <kind>: add a dependency to a copy of the metadata. kind is null, "dev"
# or "build", as cargo writes it.
plant() {
  jq --arg from "$1" --arg to "$2" --argjson kind "$3" \
    '(.packages[] | select(.name == $from) | .dependencies) += [{name: $to, kind: $kind}]' \
    "$tmp/metadata.json" >"$tmp/planted.json"
}

plant whizbang-core whizbang null
expect_fail "the core depends on the binary" "whizbang-core -> whizbang" \
  "$check" --metadata "$tmp/planted.json"

plant whizbang-core whizbang '"dev"'
expect_fail "a dev edge that is not listed" "whizbang-core -dev-> whizbang" \
  "$check" --metadata "$tmp/planted.json"

plant whizbang-core whizbang '"build"'
expect_fail "a build edge that is not listed" "whizbang-core -> whizbang" \
  "$check" --metadata "$tmp/planted.json"

# A crate the spec does not name yet: two tools that depend on each other.
jq '.packages += [
      {name: "whizbang-rascal", id: "rascal", dependencies: [{name: "whizbang-squint", kind: null}]},
      {name: "whizbang-squint", id: "squint", dependencies: [{name: "whizbang-core", kind: null}]}]
    | .workspace_members += ["rascal", "squint"]' "$tmp/metadata.json" >"$tmp/planted.json"
expect_fail "one tool depends on another" "whizbang-rascal -> whizbang-squint" \
  "$check" --metadata "$tmp/planted.json"

# The testkit is a development dependency only.
jq '.packages += [
      {name: "whizbang-testkit", id: "testkit", dependencies: [{name: "whizbang-core", kind: null}]},
      {name: "whizbang-eval", id: "eval", dependencies: [{name: "whizbang-testkit", kind: "dev"}]}]
    | .workspace_members += ["testkit", "eval"]' "$tmp/metadata.json" >"$tmp/planted.json"
expect_pass "the testkit as a dev dependency" "$check" --metadata "$tmp/planted.json"
jq '(.packages[] | select(.name == "whizbang-eval") | .dependencies[0].kind) = null' \
  "$tmp/planted.json" >"$tmp/planted2.json"
expect_fail "the testkit as a normal dependency" "whizbang-eval -> whizbang-testkit" \
  "$check" --metadata "$tmp/planted2.json"

# A path dependency on a crate outside the workspace could carry any edge unseen: here
# whizbang-core -> helper, and whatever helper depends on. cargo writes a null source for a
# path dependency and the registry's address for a published one.
jq '(.packages[] | select(.name == "whizbang-core") | .dependencies) +=
      [{name: "helper", kind: null, source: null, path: "/somewhere/helper"}]' \
  "$tmp/metadata.json" >"$tmp/planted.json"
expect_fail "a path dependency outside the workspace" \
  "whizbang-core depends on helper by path, and helper is not a workspace crate" \
  "$check" --metadata "$tmp/planted.json"
jq '(.packages[] | select(.name == "whizbang-core") | .dependencies) +=
      [{name: "helper", kind: "dev", source: null, path: "/somewhere/helper"}]' \
  "$tmp/metadata.json" >"$tmp/planted.json"
expect_fail "the same as a dev dependency" "whizbang-core depends on helper by path" \
  "$check" --metadata "$tmp/planted.json"
jq '(.packages[] | select(.name == "whizbang-core") | .dependencies) +=
      [{name: "helper", kind: null, source: "registry+https://github.com/rust-lang/crates.io-index"}]' \
  "$tmp/metadata.json" >"$tmp/planted.json"
expect_pass "a registry dependency of the same name" "$check" --metadata "$tmp/planted.json"

# A workspace crate the edge list does not name.
jq '.packages += [{name: "whizbang-extra", id: "extra", dependencies: []}]
    | .workspace_members += ["extra"]' "$tmp/metadata.json" >"$tmp/planted.json"
expect_fail "an unlisted crate" "whizbang-extra is not named" \
  "$check" --metadata "$tmp/planted.json"

# The real workspace against an edge list that lacks one of its edges.
grep -v '^whizbang -> whizbang-core$' scripts/crate-graph.txt >"$tmp/edges.txt"
expect_fail "an edge list without a real edge" "whizbang -> whizbang-core" \
  "$check" --edges "$tmp/edges.txt"

# A pattern character in the edge list is text, never a file name.
# Unprotected, the star would become the names of the files here and the line would be
# unreadable; read as text, the line is fine and the missing real edge is what fails.
{ grep -v '^whizbang -> whizbang-core$' scripts/crate-graph.txt; echo "* -> whizbang-core"; } >"$tmp/edges.txt"
expect_fail "a star in the edge list is not expanded" "forbidden edge: whizbang -> whizbang-core" \
  "$check" --edges "$tmp/edges.txt"

# A line the check cannot read fails rather than being skipped.
{ cat scripts/crate-graph.txt; echo "whizbang => whizbang-core"; } >"$tmp/edges.txt"
expect_fail "a malformed line" "cannot read" "$check" --edges "$tmp/edges.txt"
