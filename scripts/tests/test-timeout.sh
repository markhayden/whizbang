#!/usr/bin/env bash
# The fast tier's per-test timeout. Always: the config says 30 seconds and to stop the test at
# that point. With WHIZBANG_SLOW_TESTS=1: a fixture test that sleeps 31 seconds is really run
# under this repo's config and fails by timeout (about 35 seconds, so it is opt-in).
source "$(dirname "$0")/lib.sh"

config=.config/nextest.toml
# The [profile.default] table, up to the next table.
default=$(awk '/^\[/ { on = ($0 == "[profile.default]") } on' "$config")
echo "$default" | grep -q -E '^slow-timeout *= *\{ *period *= *"30s", *terminate-after *= *1 *\}$' ||
  fail "$config: [profile.default] must set slow-timeout to a 30s period that terminates after 1"

fixture=scripts/tests/fixtures/slow-crate
run_fixture() {
  CARGO_TARGET_DIR="$root/target/script-tests" SLOW_CRATE_SLEEP_SECS=$1 \
    cargo nextest run --manifest-path "$fixture/Cargo.toml" --config-file "$config" \
    --no-fail-fast
}

if [ "${WHIZBANG_SLOW_TESTS:-}" = "1" ]; then
  expect_fail "a test that sleeps 31 seconds" "TIMEOUT" run_fixture 31
  grep -q -E 'TIMEOUT \[ +30\.' "$tmp/out" || { cat "$tmp/out"; fail "the test was stopped at 30 seconds"; }
  grep -q -F '1 timed out' "$tmp/out" || { cat "$tmp/out"; fail "the summary counts one timeout"; }
  # The same fixture passes when it does not sleep, so the failure above is the timeout.
  expect_pass "the same test without the sleep" run_fixture 0
else
  echo "skip: the 31-second run; set WHIZBANG_SLOW_TESTS=1 (just test-slow)"
fi
