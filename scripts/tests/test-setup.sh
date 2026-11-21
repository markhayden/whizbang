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
# A fake `python3`: it answers the one question setup asks, with the version in its
# variable; an empty variable means it is not installed.
cat >"$tmp/bin/python3" <<'SH'
#!/bin/sh
[ -n "$FAKE_PYTHON" ] || exit 127
echo "$FAKE_PYTHON"
SH
# A fake `uv`, which prints its version as the real one does, with a build after it. Its
# variable is FAKE_UV, the pinned version when unset; empty, it is not installed.
cat >"$tmp/bin/uv" <<'SH'
#!/bin/sh
[ "$1" = --version ] || exit 2
[ -n "$FAKE_UV" ] || exit 127
echo "uv $FAKE_UV (0123456789 2026-10-03 aarch64-apple-darwin)"
SH
# A fake `node`, which prints its version as the real one does, with a v before it. Its
# variable is FAKE_NODE, the pinned version when unset; empty, it is not installed.
cat >"$tmp/bin/node" <<'SH'
#!/bin/sh
[ "$1" = --version ] || exit 2
[ -n "$FAKE_NODE" ] || exit 127
echo "v$FAKE_NODE"
SH
chmod +x "$tmp/bin/just" "$tmp/bin/cargo" "$tmp/bin/python3" "$tmp/bin/uv" "$tmp/bin/node"
# with_tools <just> <nextest> <deny> [python, 3.12 when not given; "" for not installed]
with_tools() {
  env PATH="$tmp/bin:$PATH" FAKE_JUST="$1" FAKE_NEXTEST="$2" FAKE_DENY="$3" FAKE_PYTHON="${4-3.12}" \
    FAKE_UV="${FAKE_UV-$(pin uv)}" FAKE_NODE="${FAKE_NODE-$(pin node)}" "$setup" --check
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

# uv is checked as the others are, and named with its release: not by cargo, and with no
# command, since uv's own update and its maker's install script try an unapproved host.
[ -n "$(pin uv)" ] || fail "uv is pinned in scripts/tool-versions.txt"
expect_pass "uv at its pin" with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)"
grep -q -F "tool ok: uv $(pin uv)" "$tmp/out" || { cat "$tmp/out"; fail "uv at its pin is reported as ok"; }
FAKE_UV=0.0.1 expect_fail "uv at another version" \
  "uv is at another version: found 0.0.1, pinned $(pin uv); uv $(pin uv) is needed; its release is https://github.com/astral-sh/uv/releases/tag/$(pin uv); downloading it on this machine needs the owner's approval" \
  with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)"
FAKE_UV="" expect_fail "no uv" \
  "uv is missing; uv $(pin uv) is needed; its release is https://github.com/astral-sh/uv/releases/tag/$(pin uv); downloading it on this machine needs the owner's approval" \
  with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)"

# Node builds and tests ui/ (plan m5, task U1): checked as uv is, and named with where it is
# had, not with cargo.
[ -n "$(pin node)" ] || fail "node is pinned in scripts/tool-versions.txt"
expect_pass "node at its pin" with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)"
grep -q -F "tool ok: node $(pin node)" "$tmp/out" || { cat "$tmp/out"; fail "node at its pin is reported as ok"; }
FAKE_NODE=1.2.3 expect_fail "node at another version" \
  "node is at another version: found 1.2.3, pinned $(pin node); Node $(pin node) is needed for ui/" \
  with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)"
FAKE_NODE="" expect_fail "no node" "node is missing; Node $(pin node) is needed for ui/" \
  with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)"
grep -q -F "cargo install node" "$tmp/out" && { cat "$tmp/out"; fail "node is not installed with cargo"; }

# Every problem is reported, not only the first.
expect_fail "two problems at once" "just is missing" with_tools "" 9.9.9 "$(pin cargo-deny)"
grep -q -F "cargo-nextest is at another version: found 9.9.9" "$tmp/out" ||
  { cat "$tmp/out"; fail "the second problem is reported too"; }

# python3 runs the baseline scripts and their tests: 3.9 or later, any later version.
pins_ok() { with_tools "$(pin just)" "$(pin cargo-nextest)" "$(pin cargo-deny)" "$@"; }
for v in 3.9 3.10 3.15 4.0; do
  expect_pass "python3 $v" pins_ok "$v"
  grep -q -F "tool ok: python3 $v (3.9 or later)" "$tmp/out" || { cat "$tmp/out"; fail "python3 $v is reported as ok"; }
done
for v in 3.8 2.7 3.0; do
  expect_fail "python3 $v" "python3 is too old: found $v, and scripts/baseline needs 3.9 or later" pins_ok "$v"
done
expect_fail "no python3" "python3 is missing; scripts/baseline needs 3.9 or later" pins_ok ""
expect_fail "a python3 that answers something else" "python3 is missing" pins_ok "Python"

# CI installs exactly the pinned versions: the tools with taiki-e/install-action, and Node
# with actions/setup-node in every job that sets it up, at the pin and no other.
pins=$(grep -v '^#' scripts/tool-versions.txt | awk 'NF && $1 != "node" { printf "%s%s@%s", sep, $1, $2; sep = "," }')
grep -q -F "tool: $pins" .github/workflows/ci.yml ||
  fail ".github/workflows/ci.yml must install: $pins"
nodes=$(grep -E '^ +node-version: ' .github/workflows/ci.yml | sort -u)
[ "$nodes" = "          node-version: $(pin node)" ] ||
  fail ".github/workflows/ci.yml must set up Node $(pin node) and no other (found: $nodes)"

finished
