#!/usr/bin/env bash
# Runs the fast tests and prints seconds per test suite (SPEC.md section 13).
#
#   scripts/timing.sh [--limit SECONDS] [--junit FILE] [--part fast|sweeps]
#
# Fails when a test fails, and when test time exceeds the limit. The limit is --limit, else
# WHIZBANG_TIMING_LIMIT, else 300 seconds everywhere: on GitHub's hosted runners, on a
# self-hosted runner, and on any machine outside CI, the owner's Mac among them (SPEC.md
# section 13). The owner set 300 seconds on the hosted macOS runner on 2026-11-17, whose
# test time varies about two-fold on unchanged code, and everywhere else on 2026-11-19
# (until then 180), answering "raise local (and Linux) from 180 to 300 seconds?" with
# "that is ok also i think". The line with the total names the limit that applied and
# why. Raising a limit needs the owner.
#
# Compilation is not counted. Test time is the elapsed time nextest reports for the run,
# which starts after everything is built; a suite's seconds are the sum of its tests'
# durations, so with tests running in parallel the rows can add up to more than the total.
#
# The tests run through scripts/thread-guard.sh, which kills them, and fails, when one test
# process holds more threads than its limit.
#
# --junit FILE prints the table for a saved result file and runs nothing; it exists for
# scripts/tests/test-timing.sh.
#
# The playground interface's unit tests (ui/) are counted too, under the same limit (plan
# m5, "Fast-tier cost"): `just ui-check`, which `just check` runs just before this, leaves
# their seconds in target/ui/unit-seconds and their results in target/ui/unit-junit.xml.
# When it did, each suite is a row of the table, their seconds join the total, and the two
# files are removed, so a later run without `ui-check` does not count them again. --ui DIR
# reads the two files from DIR instead (scripts/tests/test-timing.sh).
#
# --part fast|sweeps runs one of two parts of the fast tests, each under the same limit;
# without it every fast test runs, which is what `just check` does. CI runs the two parts
# in jobs of their own on each runner (`check` and `sweeps`, .github/workflows/ci.yml),
# because the hosted macOS runner's test time for the whole came to 225 to 271 seconds of
# the 300 in November 2026, most of it the sweeps two at a time. `sweeps` is the tests
# that $SWEEPS below names: the test group `crash-sweeps` of .config/nextest.toml (a test
# joins it by its name) and squint's check of every use case through its recipe; `fast`
# is every other test, `not` the same expression, so the two parts are the whole by
# construction. The interface's unit tests are counted in `fast` and in the whole, never
# in `sweeps`. The line with the total names the part.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

limit=300
# No apostrophes in this line: scripts/tests/test-syntax.sh reads one as quoting.
why="the default, on every runner and machine (the decisions of the owner, 2026-11-17 for the hosted macOS runner and 2026-11-19 everywhere else)"
if [ -n "${WHIZBANG_TIMING_LIMIT:-}" ]; then
  limit=$WHIZBANG_TIMING_LIMIT
  why="from WHIZBANG_TIMING_LIMIT"
fi
junit=""
ui=""
part=""
while [ $# -gt 0 ]; do
  case "$1" in
    --limit) limit=$2; why="from --limit"; shift 2 ;;
    --junit) junit=$2; shift 2 ;;
    --ui) ui=$2; shift 2 ;;
    --part) part=$2; shift 2 ;;
    *) echo "timing: unknown argument: $1" >&2; exit 2 ;;
  esac
done
# The one place the split is named. `group()` is not allowed in a profile's default
# filter, so the expression is given to nextest here, with -E.
SWEEPS='group(crash-sweeps) | (package(whizbang-squint) & test(/^check::each_use_case_/))'
case "$part" in
  "") filter=(); what="every fast test" ;;
  fast) filter=(-E "not ($SWEEPS)"); what="the part fast, every fast test but the sweeps" ;;
  sweeps) filter=(-E "$SWEEPS"); what="the part sweeps: $SWEEPS" ;;
  *) echo "timing: the part must be fast or sweeps: $part" >&2; exit 2 ;;
esac
# The interface's seconds are read from target/ui on a real run, and with a saved result
# file only from the folder --ui names; never in the part sweeps.
[ -n "$junit" ] || [ -n "$ui" ] || ui=target/ui
[ "$part" != sweeps ] || ui=""
# A plain number of seconds: digits, with an optional fraction. Anything else (NaN, 1e3,
# 1.5.2) would be compared as text and could pass by accident.
is_seconds() { echo "$1" | grep -E '^[0-9]+(\.[0-9]+)?$' >/dev/null; }
if ! is_seconds "$limit"; then
  echo "timing: the limit must be a number of seconds: $limit" >&2
  exit 2
fi
command -v jq >/dev/null || { echo "timing: jq is not installed" >&2; exit 2; }

test_status=0
if [ -z "$junit" ]; then
  target=$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)
  junit="$target/nextest/default/junit.xml"
  rm -f "$junit"
  scripts/thread-guard.sh cargo nextest run --workspace --profile default ${filter[@]+"${filter[@]}"} || test_status=$?
fi

if [ ! -r "$junit" ]; then
  echo "timing: no result file at $junit" >&2
  exit 1
fi

# nextest writes one element per line: <testsuites ... time="total">, <testsuite name="...">,
# <testcase ... time="seconds">.
attr() { sed -n "s/.* $1=\"\\([^\"]*\\)\".*/\\1/p"; }
total=$({ grep '<testsuites ' "$junit" || true; } | attr time | sed -n '1p')
if ! grep -q '<testsuites .* time="' "$junit"; then
  echo "timing: no total in $junit" >&2
  exit 1
fi
if ! is_seconds "$total"; then
  echo "timing: the total in $junit is not a number of seconds: $total" >&2
  exit 1
fi

echo
printf '%-44s %6s %9s\n' "suite" "tests" "seconds"
awk '
  function attr(name,    m) {
    if (match($0, " " name "=\"[^\"]*\"")) {
      m = substr($0, RSTART, RLENGTH)
      sub(/^[^"]*"/, "", m); sub(/"$/, "", m)
      return m
    }
    return ""
  }
  /<testsuite / { suite = attr("name"); if (!(suite in tests)) order[++n] = suite; tests[suite] += 0 }
  /<testcase /  { tests[suite] += 1; secs[suite] += attr("time") }
  END { for (i = 1; i <= n; i++) printf "%-44s %6d %9.3f\n", order[i], tests[order[i]], secs[order[i]] }
' "$junit"
# The interface's unit tests, when `just ui-check` ran them: their rows, and their seconds
# added to the total.
if [ -n "$ui" ] && [ -r "$ui/unit-seconds" ] && [ -r "$ui/unit-junit.xml" ]; then
  ui_seconds=$(sed -n '1p' "$ui/unit-seconds")
  is_seconds "$ui_seconds" || { echo "timing: the seconds in $ui/unit-seconds are not a number: $ui_seconds" >&2; exit 1; }
  awk '
    function attr(name,    m) {
      if (match($0, " " name "=\"[^\"]*\"")) {
        m = substr($0, RSTART, RLENGTH)
        sub(/^[^"]*"/, "", m); sub(/"$/, "", m)
        return m
      }
      return ""
    }
    /<testsuite / { printf "%-44s %6d %9.3f\n", "ui: " attr("name"), attr("tests"), attr("time") }
  ' "$ui/unit-junit.xml"
  echo "ui unit tests ${ui_seconds}s, as the clock saw the whole run of them"
  total=$(awk -v a="$total" -v b="$ui_seconds" 'BEGIN { printf "%.3f", a + b }')
  rm -f "$ui/unit-seconds" "$ui/unit-junit.xml"
fi
echo "tests run: ${what}"
echo "test time ${total}s, limit ${limit}s: ${why} (compilation not counted)"

if [ "$test_status" -ne 0 ]; then
  echo "timing: a test failed" >&2
  exit "$test_status"
fi
if awk -v t="$total" -v l="$limit" 'BEGIN { exit !(t + 0 > l + 0) }'; then
  echo "timing: test time ${total}s exceeds the limit of ${limit}s" >&2
  echo "timing: review coverage first, then speed a test up or move it to another tier (SPEC.md section 13)" >&2
  exit 1
fi
}
main "$@"; exit
