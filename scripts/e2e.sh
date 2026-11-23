#!/usr/bin/env bash
# The Chrome tier (plan m5, "Where the Chrome tier runs"; ADR 0043): `just e2e`. Heavy.
#
#   scripts/e2e.sh [--limit SECONDS]
#   scripts/e2e.sh --report FILE [--limit SECONDS]
#                    print the table for a saved Playwright JSON report and run nothing
#                    (scripts/tests/test-e2e.sh)
#
# It says which Google Chrome it found, builds the fixture host (`cargo build -p whizbang --example play_fixture`), then runs
# Playwright's cases in ui/e2e/ against it, in the Google Chrome installed on the machine,
# one worker, no retry, 30 seconds a case (ui/playwright.config.ts), under
# scripts/thread-guard.sh. Then it reads the run's own JSON report, prints the seconds by
# file, and fails when a case failed or when the whole run took more than the limit of
# test time: 240 seconds, or --limit, or WHIZBANG_E2E_LIMIT. The limit is the Chrome
# tier's own, apart from the fast tier's 300 (SPEC.md section 13); it is not raised
# without the owner, who raised it from 180 to 240 on 2026-11-22.
#
# WHIZBANG_E2E_SHARD=<k>/<n> runs only the k-th of n shards of the cases (Playwright's
# --shard), as CI's jobs `e2e (<runner>, shard <k>)` do, so that no one job comes near the
# limit; a shard's own test time is then held to 120 seconds, the limit of one shard: half
# of the owner's 240 for the whole tier, so two shards together stay within it. Unset, as
# `just e2e` runs locally, every case runs and all of them are held to the 240.
#
# The packages are installed as scripts/ui.sh installs them. The report is kept in
# target/e2e/ (report.json, and html/ for a person), where CI keeps it when a case fails.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "chrome tier: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
main() {
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

die() {
  echo "e2e: $*" >&2
  exit 1
}
limit=240
why="the Chrome tier's limit (plan m5)"
shard=${WHIZBANG_E2E_SHARD:-}
if [ -n "$shard" ]; then
  echo "$shard" | grep -E '^[1-9][0-9]*/[1-9][0-9]*$' >/dev/null ||
    die "WHIZBANG_E2E_SHARD must be <k>/<n>, as 1/2: $shard"
  [ "${shard%/*}" -le "${shard#*/}" ] || die "WHIZBANG_E2E_SHARD names a shard past the last: $shard"
  # Half of the owner's 240 for the whole tier, which is not raised by sharding it.
  limit=120
  why="the Chrome tier's limit for one shard (plan m5), shard $shard"
fi
if [ -n "${WHIZBANG_E2E_LIMIT:-}" ]; then
  limit=$WHIZBANG_E2E_LIMIT
  why="from WHIZBANG_E2E_LIMIT${shard:+, shard $shard}"
fi
report=""
while [ $# -gt 0 ]; do
  case "$1" in
    --limit) [ $# -ge 2 ] || die "--limit needs a number of seconds"; limit=$2; why="from --limit"; shift 2 ;;
    --report) [ $# -ge 2 ] || die "--report needs a file"; report=$2; shift 2 ;;
    *) echo "usage: scripts/e2e.sh [--report FILE] [--limit SECONDS]" >&2; exit 2 ;;
  esac
done
echo "$limit" | grep -E '^[0-9]+(\.[0-9]+)?$' >/dev/null || die "the limit must be a number of seconds: $limit"
command -v jq >/dev/null || die "jq is not installed"

status=0
if [ -z "$report" ]; then
  report=target/e2e/report.json
  rm -rf target/e2e
  # The packages and Node. The page is the committed ui/dist/, which the fixture host
  # embeds as the binary does; whether it is what the sources build is `just ui-check`'s.
  scripts/ui.sh install
  # The Chrome the cases run in, said in the log: Playwright drives the one installed here.
  case "$(uname -s)" in
    Darwin) chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" ;;
    *) chrome=$(command -v google-chrome || command -v google-chrome-stable || true) ;;
  esac
  [ -n "$chrome" ] && [ -x "$chrome" ] || die "Google Chrome is not installed where Playwright looks for it; the Chrome tier needs it"
  echo "e2e: $("$chrome" --version 2>/dev/null || echo "Google Chrome, version unknown")"
  cargo build --quiet -p whizbang --example play_fixture || die "the fixture host did not build"
  fixture=$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/debug/examples/play_fixture
  [ -x "$fixture" ] || die "no fixture host at $fixture"
  sharded=()
  [ -z "$shard" ] || sharded=("--shard=$shard")
  (cd ui && WHIZBANG_PLAY_FIXTURE="$fixture" "$OLDPWD/scripts/thread-guard.sh" node_modules/.bin/playwright test ${sharded[@]+"${sharded[@]}"}) || status=$?
fi
[ -r "$report" ] || die "no report at $report"

# Each spec file's cases: its count, how many passed, and their seconds.
jq -r '
  def cases: [.. | objects | select(has("tests")) | .tests[]];
  .suites[] | [.file, (cases | length), ([cases[] | select(.status == "expected")] | length),
    ([cases[] | .results[]? | .duration] | add // 0)] | @tsv' "$report" >"${report}.rows" ||
  die "the report at $report cannot be read"
printf '\n%-28s %6s %7s %9s\n' "file" "cases" "passed" "seconds"
awk -F '\t' '{ printf "%-28s %6d %7d %9.3f\n", $1, $2, $3, $4 / 1000 }' "${report}.rows"
total=$(awk -F '\t' '{ ms += $4 } END { printf "%.3f", ms / 1000 }' "${report}.rows")
cases=$(awk -F '\t' '{ n += $2 } END { print n + 0 }' "${report}.rows")
passed=$(awk -F '\t' '{ n += $3 } END { print n + 0 }' "${report}.rows")
rm -f "${report}.rows"
echo "test time ${total}s over ${cases} cases, limit ${limit}s: ${why}"

[ "$cases" -gt 0 ] || die "the report holds no case"
if [ "$status" -ne 0 ] || [ "$passed" -ne "$cases" ]; then
  echo "e2e: a case failed ($passed of $cases passed); the report is in target/e2e/" >&2
  exit 1
fi
if awk -v t="$total" -v l="$limit" 'BEGIN { exit !(t + 0 > l + 0) }'; then
  echo "e2e: test time ${total}s exceeds the limit of ${limit}s; merge or move cases before anything is raised (plan m5, Risks)" >&2
  exit 1
fi
}
main "$@"; exit
