#!/usr/bin/env bash
# The fast tier's per-test timeout. Always: the config says 30 seconds and to stop the test at
# that point, and CI has the step that runs the real thing. With WHIZBANG_TIMEOUT_TEST=1
# (`just test-timeout`, a CI step on one runner) or WHIZBANG_SLOW_TESTS=1
# (`just test-scripts-slow`): a fixture test that sleeps 31 seconds is really run under this
# repo's config and fails by timeout. That takes about 35 seconds, so `just check` skips it.
source "$(dirname "$0")/lib.sh"

config=.config/nextest.toml
# The [profile.default] table, up to the next table.
default=$(awk '/^\[/ { on = ($0 == "[profile.default]") } on' "$config")
echo "$default" | grep -q -E '^slow-timeout *= *\{ *period *= *"30s", *terminate-after *= *1 *\}$' ||
  fail "$config: [profile.default] must set slow-timeout to a 30s period that terminates after 1"

# The real run happens automatically in CI, on one runner.
grep -q -E '^ +run: just test-timeout$' .github/workflows/ci.yml ||
  fail ".github/workflows/ci.yml must have a step that runs: just test-timeout"

fixture=scripts/tests/fixtures/slow-crate
run_fixture() {
  CARGO_TARGET_DIR="$root/target/script-tests" SLOW_CRATE_SLEEP_SECS=$1 \
    cargo nextest run --manifest-path "$fixture/Cargo.toml" --config-file "$config" \
    --no-fail-fast
}

if [ "${WHIZBANG_TIMEOUT_TEST:-}" = "1" ] || [ "${WHIZBANG_SLOW_TESTS:-}" = "1" ]; then
  expect_fail "a test that sleeps 31 seconds" "TIMEOUT" run_fixture 31
  grep -q -E 'TIMEOUT \[ +30\.' "$tmp/out" || { cat "$tmp/out"; fail "the test was stopped at 30 seconds"; }
  grep -q -F '1 timed out' "$tmp/out" || { cat "$tmp/out"; fail "the summary counts one timeout"; }
  # The same fixture passes when it does not sleep, so the failure above is the timeout.
  expect_pass "the same test without the sleep" run_fixture 0
else
  echo "skip: the 31-second run; set WHIZBANG_TIMEOUT_TEST=1 (just test-timeout; also in just test-scripts-slow and CI)"
fi
