#!/usr/bin/env bash
# The pinned tool versions: setup passes when every tool is at its pin, names a tool that is
# at another version or missing and how to install it, and CI installs the same versions.
# Every case runs against fake tools, so `just check` does not depend on what this machine
# has installed; only `just setup` does.
source "$(dirname "$0")/lib.sh"

setup=scripts/setup.sh
pin() { awk -v tool="$1" '$1 == tool { print $2 }' scripts/tool-versions.txt; }

# Fake `just` and `cargo`, first on the path. Each prints the version in its FAKE_ variable;
# an empty variable means the tool is not installed.
mkdir "$tmp/bin"
cat >"$tmp/bin/just" <<'SH'
#!/bin/sh
[ -n "$FAKE_JUST" ] || exit 127
echo "just $FAKE_JUST"
SH
cat >"$tmp/bin/cargo" <<'SH'
#!/bin/sh
case "$1" in
  nextest) v=$FAKE_NEXTEST ;;
  deny) v=$FAKE_DENY ;;
  *) exit 101 ;;
esac
[ -n "$v" ] || { echo "error: no such command: \`$1\`" >&2; exit 101; }
echo "cargo-$1 $v (a build note)"
SH
chmod +x "$tmp/bin/just" "$tmp/bin/cargo"
with_tools() {
  env PATH="$tmp/bin:$PATH" FAKE_JUST="$1" FAKE_NEXTEST="$2" FAKE_DENY="$3" "$setup" --check
}

expect_pass "every tool at its pin" with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)"

expect_fail "a tool at another version" \
  "just is at another version: found 0.0.1, pinned $(pin just); install the pinned one with: cargo install just@$(pin just) --locked" \
  with_tools 0.0.1 "$(pin cargo-nextest)" "$(pin cargo-deny)"
grep -q -F "missing" "$tmp/out" && { cat "$tmp/out"; fail "a tool at another version is not called missing"; }

expect_fail "a missing tool" \
  "cargo-deny is missing; install it with: cargo install cargo-deny@$(pin cargo-deny) --locked" \
  with_tools "$(pin just)" "$(pin cargo-nextest)" ""
grep -q -F "another version" "$tmp/out" && { cat "$tmp/out"; fail "a missing tool is not called another version"; }

# Every problem is reported, not only the first.
expect_fail "two problems at once" "just is missing" with_tools "" 9.9.9 "$(pin cargo-deny)"
grep -q -F "cargo-nextest is at another version: found 9.9.9" "$tmp/out" ||
  { cat "$tmp/out"; fail "the second problem is reported too"; }

# CI installs exactly the pinned versions.
pins=$(grep -v '^#' scripts/tool-versions.txt | awk 'NF { printf "%s%s@%s", sep, $1, $2; sep = "," }')
grep -q -F "tool: $pins" .github/workflows/ci.yml ||
  fail ".github/workflows/ci.yml must install: $pins"
