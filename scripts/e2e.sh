#!/usr/bin/env bash
# The Chrome tier (plan m5, "Where the Chrome tier runs"; ADR 0042): `just e2e`. Heavy.
#
#   scripts/e2e.sh [--limit SECONDS]
#   scripts/e2e.sh --report FILE [--limit SECONDS]
#                    print the table for a saved Playwright JSON report and run nothing
#                    (scripts/tests/test-e2e.sh)
#
# It builds the fixture host (`cargo build -p whizbang --example play_fixture`), then runs
# Playwright's cases in ui/e2e/ against it, in the Google Chrome installed on the machine,
# one worker, no retry, 30 seconds a case (ui/playwright.config.ts), under
# scripts/thread-guard.sh. Then it reads the run's own JSON report, prints the seconds by
# file, and fails when a case failed or when the whole run took more than the limit of
# test time: 180 seconds, or --limit, or WHIZBANG_E2E_LIMIT. The limit is the Chrome
# tier's own, apart from the fast tier's 300 (SPEC.md section 13); it is not raised
# without the owner.
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
limit=180
why="the Chrome tier's limit (plan m5)"
if [ -n "${WHIZBANG_E2E_LIMIT:-}" ]; then
  limit=$WHIZBANG_E2E_LIMIT
  why="from WHIZBANG_E2E_LIMIT"
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
  cargo build --quiet -p whizbang --example play_fixture || die "the fixture host did not build"
  fixture=$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/debug/examples/play_fixture
  [ -x "$fixture" ] || die "no fixture host at $fixture"
  (cd ui && WHIZBANG_PLAY_FIXTURE="$fixture" "$OLDPWD/scripts/thread-guard.sh" node_modules/.bin/playwright test) || status=$?
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
