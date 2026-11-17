#!/usr/bin/env bash
# Runs the fast tests and prints seconds per test suite (SPEC.md section 13).
#
#   scripts/timing.sh [--limit SECONDS] [--junit FILE]
#
# Fails when a test fails, and when test time exceeds the limit. The limit is --limit, else
# WHIZBANG_TIMING_LIMIT, else 300 seconds on GitHub's hosted macOS runner and 180 seconds
# everywhere else: on the Linux runner, on a self-hosted runner, and on any machine outside
# CI, the owner's Mac among them (SPEC.md section 13). The hosted macOS runner is known by
# the variables GitHub sets in every job: GITHUB_ACTIONS=true, RUNNER_OS=macOS and
# RUNNER_ENVIRONMENT=github-hosted, all three. Its 300 seconds are the owner's decision of
# 2026-11-17: its test time varies about two-fold on unchanged code. The line with the
# total names the limit that applied and why. Raising a limit needs the owner.
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
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if [ "${GITHUB_ACTIONS:-}" = true ] && [ "${RUNNER_OS:-}" = macOS ] \
  && [ "${RUNNER_ENVIRONMENT:-}" = github-hosted ]; then
  limit=300
  # No apostrophes in these two lines: scripts/tests/test-syntax.sh reads one as quoting.
  why="the hosted macOS runner of GitHub, whose test time varies about two-fold on unchanged code (the decision of the owner, 2026-11-17; 180 s everywhere else)"
else
  limit=180
  why="the default, everywhere but the hosted macOS runner of GitHub (300 s there)"
fi
if [ -n "${WHIZBANG_TIMING_LIMIT:-}" ]; then
  limit=$WHIZBANG_TIMING_LIMIT
  why="from WHIZBANG_TIMING_LIMIT"
fi
junit=""
while [ $# -gt 0 ]; do
  case "$1" in
    --limit) limit=$2; why="from --limit"; shift 2 ;;
    --junit) junit=$2; shift 2 ;;
    *) echo "timing: unknown argument: $1" >&2; exit 2 ;;
  esac
done
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
  scripts/thread-guard.sh cargo nextest run --workspace --profile default || test_status=$?
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
