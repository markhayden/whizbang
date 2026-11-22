#!/usr/bin/env bash
# The Chrome tier's table and its limit (scripts/e2e.sh), against saved Playwright reports,
# so no browser is started: the seconds by file, a failed case, the limit of 180 seconds.
source "$(dirname "$0")/lib.sh"

e2e=scripts/e2e.sh
# report <ms of the first case> <status of the second case>
report() {
  cat <<JSON
{
  "suites": [
    { "title": "shell.spec.ts", "file": "shell.spec.ts", "specs": [
      { "title": "a", "tests": [ { "status": "expected", "results": [ { "duration": $1, "status": "passed", "retry": 0 } ] } ] },
      { "title": "b", "tests": [ { "status": "$2", "results": [ { "duration": 500, "status": "passed", "retry": 0 } ] } ] }
    ] },
    { "title": "security.spec.ts", "file": "security.spec.ts", "specs": [], "suites": [
      { "title": "inner", "file": "security.spec.ts", "specs": [
        { "title": "c", "tests": [ { "status": "expected", "results": [ { "duration": 250, "status": "passed", "retry": 0 } ] } ] }
      ] }
    ] }
  ]
}
JSON
}
here() { env -u WHIZBANG_E2E_LIMIT -u WHIZBANG_E2E_SHARD "$@"; }

report 1500 expected >"$tmp/ok.json"
expect_pass "every case passed under the limit" here "$e2e" --report "$tmp/ok.json"
grep -q -E '^shell\.spec\.ts +2 +2 +2\.000$' "$tmp/out" || { cat "$tmp/out"; fail "a row per file: cases, passed and seconds"; }
grep -q -E '^security\.spec\.ts +1 +1 +0\.250$' "$tmp/out" || { cat "$tmp/out"; fail "the cases of a nested group count for their file"; }
grep -q -F 'test time 2.250s over 3 cases, limit 180s' "$tmp/out" || { cat "$tmp/out"; fail "the total and the limit of 180 s"; }

report 1500 unexpected >"$tmp/failed.json"
expect_fail "a failed case" "a case failed (2 of 3 passed)" here "$e2e" --report "$tmp/failed.json"
report 1500 flaky >"$tmp/flaky.json"
expect_fail "a case that passed only on a retry" "a case failed (2 of 3 passed)" here "$e2e" --report "$tmp/flaky.json"

report 180000 expected >"$tmp/long.json"
expect_fail "a run over 180 s" "test time 180.750s exceeds the limit of 180s" here "$e2e" --report "$tmp/long.json"
expect_pass "--limit" here "$e2e" --report "$tmp/long.json" --limit 200
grep -q -F 'limit 200s: from --limit' "$tmp/out" || { cat "$tmp/out"; fail "--limit says so"; }
expect_fail "the environment's limit" "exceeds the limit of 1s" env WHIZBANG_E2E_LIMIT=1 "$e2e" --report "$tmp/ok.json"
expect_fail "a limit that is not a number" "must be a number of seconds" here "$e2e" --report "$tmp/ok.json" --limit 1e3

# A shard: the limit holds for its own cases, and the line says which shard it is.
expect_pass "a shard" env -u WHIZBANG_E2E_LIMIT WHIZBANG_E2E_SHARD=2/2 "$e2e" --report "$tmp/ok.json"
grep -q -F "limit 180s: the Chrome tier's limit (plan m5), for shard 2/2" "$tmp/out" || { cat "$tmp/out"; fail "a shard says which it is"; }
expect_fail "a shard over the limit" "exceeds the limit of 180s" env -u WHIZBANG_E2E_LIMIT WHIZBANG_E2E_SHARD=1/2 "$e2e" --report "$tmp/long.json"
expect_fail "a shard that is not k/n" "must be <k>/<n>" env -u WHIZBANG_E2E_LIMIT WHIZBANG_E2E_SHARD=half "$e2e" --report "$tmp/ok.json"
expect_fail "a shard past the last" "past the last" env -u WHIZBANG_E2E_LIMIT WHIZBANG_E2E_SHARD=3/2 "$e2e" --report "$tmp/ok.json"

echo '{ "suites": [] }' >"$tmp/empty.json"
expect_fail "a report with no case" "holds no case" here "$e2e" --report "$tmp/empty.json"
expect_fail "no report" "no report at" here "$e2e" --report "$tmp/absent.json"
echo 'not json' >"$tmp/broken.json"
expect_fail "a report that cannot be read" "cannot be read" here "$e2e" --report "$tmp/broken.json"

# The recipe runs exactly this script, and the Playwright configuration keeps the tier's
# rules: Chrome as installed, one worker, no retry, 30 seconds a case, the JSON report
# where the script reads it.
grep -q -x -F '    scripts/e2e.sh' justfile || fail "justfile: e2e must run scripts/e2e.sh"
config=ui/playwright.config.ts
for line in '  workers: 1,' '  retries: 0,' '  timeout: 30_000,' '    channel: "chrome",' \
  '  reporter: [["list"], ["json", { outputFile: "../target/e2e/report.json" }], ["html", { outputFolder: "../target/e2e/html", open: "never" }]],'; do
  grep -q -x -F -e "$line" "$config" || fail "$config must hold: $line"
done

finished
