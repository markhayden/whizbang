#!/usr/bin/env bash
# The step CI runs over a pull request, taken from the workflow file.
source "$(dirname "$0")/common.sh"

# CI runs the step of the workflow below over a pull request. The step is taken from the
# workflow file and run here as CI would run it: a pull request that guts its own copy of
# the scan is still checked by the copy on the base branch.
step="$tmp/ci-step.sh"
awk '/name: Secrets scan and attribution check over every commit/ { found = 1 }
  found && /run: \|/ { body = 1; next }
  body && !/^          / { exit }
  body { sub(/^          /, ""); print }' "$root/.github/workflows/ci.yml" >"$step"
[ -s "$step" ] || fail "the scan step was not found in the workflow"
ci="$tmp/ci-repo"
git init -q -b main "$ci"
cd "$ci"
git config user.name "A Tester"
git config user.email "tester@example.com"
echo "hello" >readme.txt
git add readme.txt
git commit -q -m "docs: a readme"
bare=$(git rev-parse HEAD)
mkdir scripts
cp "$scan" scripts/scan.sh
git add scripts/scan.sh
git commit -q -m "build: the scan"
base=$(git rev-parse HEAD)
printf '#!/bin/sh\nexit 0\n' >scripts/scan.sh
printf 'token = %s\n' "$key" >config.txt
git add scripts/scan.sh config.txt
git commit -q -m "feat: a config, and no scan"
head=$(git rev-parse HEAD)
mkdir "$tmp/runner"
ci_step() { env -u WHIZBANG_WORDLIST EVENT="$1" BASE="$2" HEAD="$3" RUNNER_TEMP="$tmp/runner" bash -e "$step"; }
expect_fail "CI: a pull request that guts the scan" "config.txt:1" ci_step pull_request "$base" "$head"
git checkout -q "$base"
git checkout -q -b honest
printf 'token = %s\n' "$key" >config.txt
git add config.txt
git commit -q -m "feat: a config"
expect_fail "CI: a base with no scan falls back to the head's" "config.txt:1" ci_step pull_request "$bare" HEAD
expect_fail "CI: a push to main" "config.txt:1" ci_step push "$base" HEAD
git rm -q config.txt
git commit -q -m "fix: drop the config"
expect_fail "CI: a key added then deleted" "config.txt:1" ci_step pull_request "$base" HEAD
git checkout -q "$base"
git checkout -q -b clean
echo "more" >more.txt
git add more.txt
git commit -q -m "feat: more"
expect_pass "CI: a clean pull request" ci_step pull_request "$base" HEAD
# Every push to main gets its own run: none is cancelled, and none waits behind another
# and is dropped.
grep -q -F "group: ci-\${{ github.event_name == 'pull_request' && github.ref || github.sha }}" \
  "$root/.github/workflows/ci.yml" || fail "CI: the concurrency group is not one per push to main"
grep -q -F "cancel-in-progress: \${{ github.event_name == 'pull_request' }}" \
  "$root/.github/workflows/ci.yml" || fail "CI: only pull request runs may be cancelled"

finished
