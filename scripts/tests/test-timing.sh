#!/usr/bin/env bash
# The timing table and its limit, against a recorded nextest result file, so no test is run.
source "$(dirname "$0")/lib.sh"

timing=scripts/timing.sh
cat >"$tmp/junit.xml" <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" skipped="0" failures="0" errors="0" time="2.500">
    <testsuite name="whizbang::it" tests="2" skipped="0" errors="0" failures="0">
        <testcase name="a" classname="whizbang::it" time="1.250"/>
        <testcase name="b" classname="whizbang::it" time="2.000"/>
    </testsuite>
    <testsuite name="whizbang-core" tests="1" skipped="0" errors="0" failures="0">
        <testcase name="c" classname="whizbang-core" time="0.500">
        </testcase>
    </testsuite>
</testsuites>
XML

expect_pass "under the default limit" "$timing" --junit "$tmp/junit.xml"
grep -q -E '^whizbang::it +2 +3\.250$' "$tmp/out" || { cat "$tmp/out"; fail "a row per suite: tests and summed seconds"; }
grep -q -E '^whizbang-core +1 +0\.500$' "$tmp/out" || { cat "$tmp/out"; fail "the second suite's row"; }
grep -q -F 'test time 2.500s, limit 180s' "$tmp/out" || { cat "$tmp/out"; fail "the total and the default limit"; }

expect_pass "a limit above the measured time" "$timing" --junit "$tmp/junit.xml" --limit 3
expect_fail "a limit below the measured time" "test time 2.500s exceeds the limit of 2s" \
  "$timing" --junit "$tmp/junit.xml" --limit 2
expect_fail "the limit from the environment" "exceeds the limit of 1s" \
  env WHIZBANG_TIMING_LIMIT=1 "$timing" --junit "$tmp/junit.xml"

# A total or a limit that is not a plain number of seconds cannot pass.
for bad in NaN 1.5.2 1e3 -1 "" .5; do
  sed "s/<testsuites \(.*\) time=\"2.500\"/<testsuites \1 time=\"$bad\"/" "$tmp/junit.xml" >"$tmp/bad.xml"
  expect_fail "a total of '$bad'" "is not a number of seconds" "$timing" --junit "$tmp/bad.xml"
done
for bad in NaN 1.5.2 1e3 -1 .5; do
  expect_fail "a limit of '$bad'" "limit must be a number of seconds" \
    "$timing" --junit "$tmp/junit.xml" --limit "$bad"
done
# Numbers are compared as numbers: 10 is more than 9, and 2.5 is not more than 2.50.
sed 's/time="2.500">/time="10.000">/' "$tmp/junit.xml" >"$tmp/ten.xml"
expect_fail "ten seconds against a limit of nine" "exceeds the limit of 9s" "$timing" --junit "$tmp/ten.xml" --limit 9
expect_pass "a limit equal to the measured time" "$timing" --junit "$tmp/junit.xml" --limit 2.50

# jq is needed to find the result file; without it the script says so.
mkdir "$tmp/nojq"
for tool in git bash sed grep awk cat rm head dirname env; do
  ln -s "$(command -v "$tool")" "$tmp/nojq/$tool"
done
expect_fail "jq is not installed" "jq is not installed" env PATH="$tmp/nojq" "$root/$timing" --junit "$tmp/junit.xml"

# A result file without a total cannot pass.
grep -v '<testsuites' "$tmp/junit.xml" >"$tmp/broken.xml"
expect_fail "no total in the result file" "no total" "$timing" --junit "$tmp/broken.xml"
expect_fail "a missing result file" "no result file" "$timing" --junit "$tmp/absent.xml"

# The real run, opt-in because it runs the whole fast tier a second time: the fast tests,
# with a limit no run can meet.
if [ "${WHIZBANG_SLOW_TESTS:-}" = "1" ]; then
  expect_fail "the real fast tests under a zero limit" "exceeds the limit of 0s" "$timing" --limit 0
  grep -q -F 'whizbang::it' "$tmp/out" || { cat "$tmp/out"; fail "the real run prints a row for the binary's tests"; }
else
  echo "skip: the real run under a zero limit; set WHIZBANG_SLOW_TESTS=1 (just test-scripts-slow)"
fi
