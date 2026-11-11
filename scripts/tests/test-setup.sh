#!/usr/bin/env bash
# The pinned tool versions: setup names a tool that is missing or at another version and how
# to install it, and CI installs the same versions.
source "$(dirname "$0")/lib.sh"

setup=scripts/setup.sh
expect_pass "the tools on this machine" "$setup" --check

# A `just` at another version, first on the path.
mkdir "$tmp/bin"
printf '#!/bin/sh\necho "just 0.0.1"\n' >"$tmp/bin/just"
chmod +x "$tmp/bin/just"
want=$(awk '$1 == "just" { print $2 }' scripts/tool-versions.txt)
expect_fail "a tool at another version" "install it with: cargo install just@$want --locked" \
  env PATH="$tmp/bin:$PATH" "$setup" --check
grep -q -F "found: 0.0.1" "$tmp/out" || { cat "$tmp/out"; fail "the version found is named"; }

# CI installs exactly the pinned versions.
pins=$(grep -v '^#' scripts/tool-versions.txt | awk 'NF { printf "%s%s@%s", sep, $1, $2; sep = "," }')
grep -q -F "tool: $pins" .github/workflows/ci.yml ||
  fail ".github/workflows/ci.yml must install: $pins"
