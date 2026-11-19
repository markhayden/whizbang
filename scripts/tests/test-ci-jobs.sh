#!/usr/bin/env bash
# The jobs of CI (.github/workflows/ci.yml) and the recipes they run.
#
# On the macOS runner the script tests alone take three to five and a half minutes, which
# put the one job over its budget of ten (docs/plans/m1c-local-pack.md, tasks C0 and C0b),
# and the Linux runner's one job was over it too (docs/plans/m2b-checkpoint-1d.md, task
# F1, item 5). So each runner runs `just ci` as jobs side by side: `check (<runner>)`
# runs `just ci-without-scripts` and `scripts (<runner>)` runs `just ci-scripts`. Later the
# release build and what follows it took `check (macos-15)` to 13 to 15 minutes of its 15,
# so `release (<runner>)` runs those: `just ci-release`. What is held here:
#
# - the three parts together are `just ci`, recipe for recipe, so a recipe added to `check`
#   or to `ci` and not to the part that runs it on macOS fails `just check`;
# - `ci` runs every script test whatever changed: it depends on `check-full`, which runs
#   `test-scripts` (all of them), and never on `check`, whose `test-scripts-local` runs only
#   those a change can reach (scripts/script-tests-local.sh). `check` is `check-full` with
#   that one recipe in the place of `test-scripts`, so the two cannot drift apart;
# - the jobs `scripts` and `release` are set up exactly as `check` is (the same actions,
#   the same pinned tools), run their part on every pull request and push on both runners,
#   and no step of them may be skipped;
# - the secrets scan and attribution check over every commit is the job `secrets`, on the
#   Linux runner, with the check job's limit and no condition, and in no other job (plan
#   2C, task E1: in `check` it took that job past its limit);
# - the aggregate `ci`, the one check a merge waits for, needs every other job and passes
#   only when each of them did.
#
# Every rule is shown to fail on a copy with one thing changed. Nothing here runs a recipe.
source "$(dirname "$0")/lib.sh"

workflow=.github/workflows/ci.yml

# deps <justfile> <recipe>: the recipes it depends on, in order, on one line.
deps() { awk -v name="$2" 'index($0, name ":") == 1 { sub(/^[^:]*: */, ""); print; exit }' "$1"; }
# body <justfile> <recipe>: the lines it runs, without their indentation.
body() {
  awk -v name="$2" '
    /^[^ #]/ { on = (index($0, name ":") == 1); next }
    on && /^    / { sub(/^    /, ""); print }' "$1"
}

check_justfile() { # <justfile>
  local file=$1 word want="" local_check=""
  case " $(deps "$file" check-full) " in *" test-scripts "*) ;; *)
    echo "justfile: check-full must run every script test (test-scripts)"; return 1 ;; esac
  case " $(deps "$file" ci) " in *" check-full "*) ;; *)
    echo "justfile: ci must depend on check-full"; return 1 ;; esac
  case " $(deps "$file" ci) " in *" check "*)
    echo "justfile: ci must not depend on check, which runs only some of the script tests"; return 1 ;; esac
  [ "$(body "$file" test-scripts)" = "scripts/tests/run.sh" ] ||
    { echo "justfile: test-scripts must run exactly: scripts/tests/run.sh"; return 1; }
  [ "$(body "$file" test-scripts-local)" = "scripts/script-tests-local.sh" ] ||
    { echo "justfile: test-scripts-local must run exactly: scripts/script-tests-local.sh"; return 1; }
  [ -z "$(body "$file" check)$(body "$file" check-full)" ] ||
    { echo "justfile: check and check-full run nothing of their own"; return 1; }
  # `check`: `check-full` with the local choice in the place of the script tests.
  for word in $(deps "$file" check-full); do
    [ "$word" = test-scripts ] && word=test-scripts-local
    local_check="$local_check $word"
  done
  [ " $(deps "$file" check)" = "$local_check" ] ||
    { echo "justfile: check must depend on exactly what check-full does, in its order, with test-scripts-local for test-scripts:$local_check"; return 1; }
  # `ci`, with `check-full` written out and the script tests left out.
  for word in $(deps "$file" ci); do
    if [ "$word" = check-full ]; then
      for word in $(deps "$file" check-full); do
        [ "$word" = test-scripts ] || want="$want $word"
      done
    else
      want="$want $word"
    fi
  done
  [ " $(deps "$file" ci-without-scripts) $(deps "$file" ci-release)" = "$want" ] ||
    { echo "justfile: ci-without-scripts and then ci-release must depend on exactly what ci runs, in its order, without test-scripts:$want"; return 1; }
  # The release build and what follows it, in the job `release`; nothing else is there.
  [ "$(deps "$file" ci-release)" = "smoke compile-fail scan-secrets" ] ||
    { echo "justfile: ci-release must depend on exactly: smoke compile-fail scan-secrets"; return 1; }
  # The other half. `deny` goes first, as in `check`: scripts/tests/test-deny.sh uses no
  # network and reads the workspace's crates from cargo's home, where `just deny` put them.
  [ "$(deps "$file" ci-scripts)" = "deny test-scripts" ] ||
    { echo "justfile: ci-scripts must depend on exactly: deny test-scripts"; return 1; }
  [ -z "$(body "$file" ci-without-scripts)$(body "$file" ci-release)$(body "$file" ci-scripts)" ] ||
    { echo "justfile: ci-without-scripts, ci-release and ci-scripts run nothing of their own"; return 1; }
}

# jobs <workflow>: the name of each job, one a line.
jobs() { awk '/^jobs:$/ { on = 1; next } on && /^  [a-z][a-z-]*:$/ { sub(/^  /, ""); sub(/:$/, ""); print }' "$1"; }
# job <workflow> <name>: the lines of one job.
job() {
  awk -v head="  $2:" '/^jobs:$/ { in_jobs = 1; next }
    in_jobs && /^  [a-z][a-z-]*:$/ { on = ($0 == head); next } on' "$1"
}
# before <text> <step's line, as a pattern>: the line above that step's line.
before() { grep -B1 -E "$2" <<<"$1" | head -n 1 || true; }
# set_up <job's lines>: from its checkout to the line that names the pinned tools.
set_up() { awk '/^      - uses: actions\/checkout@/ { on = 1 } on { print } on && /^          tool: / { exit }' <<<"$1"; }

check_workflow() { # <workflow>
  local file=$1 check scripts release secrets ci name
  [ "$(jobs "$file" | tr '\n' ' ')" = "check scripts release secrets ci " ] ||
    { echo "CI: the jobs must be check, scripts, release, secrets and ci (found: $(jobs "$file" | tr '\n' ' '))"; return 1; }
  # A step or a job that may fail without failing the run would pass anything.
  if grep -E 'continue-on-error' "$file" >/dev/null; then
    echo "CI: nothing may continue on error"; return 1
  fi
  check=$(job "$file" check)
  scripts=$(job "$file" scripts)
  release=$(job "$file" release)
  secrets=$(job "$file" secrets)
  ci=$(job "$file" ci)

  # The build cache is keyed by the workspace's Cargo.toml, where the profiles are (the
  # cache's own key does not read it), and saved when a step fails or the job is
  # cancelled; the other jobs are set up as check is, so this holds for them too.
  grep -E "^          key: .*hashFiles\('Cargo.toml', " <<<"$(set_up "$check")" >/dev/null ||
    { echo "CI: the cache must be keyed by the workspace's Cargo.toml"; return 1; }
  grep -x -F '          cache-on-failure: true' <<<"$(set_up "$check")" >/dev/null ||
    { echo "CI: the cache must be saved when a step fails or the job is cancelled"; return 1; }

  # check: both runners, the first part of `just ci`, in a step with no condition.
  grep -x -F '        os: [macos-15, ubuntu-22.04]' <<<"$check" >/dev/null ||
    { echo "CI: check must run on macos-15 and ubuntu-22.04"; return 1; }
  [ "$(grep -c -E '^ +run: just ci' <<<"$check")" = 1 ] && grep -E '^ +run: just ci-without-scripts$' <<<"$check" >/dev/null ||
    { echo "CI: check must have one step that runs just ci-without-scripts"; return 1; }
  case "$(before "$check" '^ +run: just ci-without-scripts$')" in *' if: '*)
    echo "CI: check must run just ci-without-scripts on every runner, with no condition"; return 1 ;; esac

  # scripts: the other half, on the same runners, set up as check is, with no step that
  # can be skipped. A job that is skipped does not count as passed below either.
  grep -x -F '        os: [macos-15, ubuntu-22.04]' <<<"$scripts" >/dev/null &&
    grep -x -F '    runs-on: ${{ matrix.os }}' <<<"$scripts" >/dev/null ||
    { echo "CI: scripts must run on macos-15 and ubuntu-22.04"; return 1; }
  [ "$(grep -c -E '^ +run: just ' <<<"$scripts")" = 1 ] && grep -E '^ +run: just ci-scripts$' <<<"$scripts" >/dev/null ||
    { echo "CI: scripts must run exactly: just ci-scripts"; return 1; }
  if grep -E '^ +if:' <<<"$scripts" >/dev/null; then
    echo "CI: no step of scripts, and not the job, may have a condition"; return 1
  fi
  [ -n "$(set_up "$check")" ] && [ "$(set_up "$scripts")" = "$(set_up "$check")" ] ||
    { echo "CI: scripts must be set up exactly as check is: the checkout, the toolchain, the cache and the pinned tools"; return 1; }

  # release: the third part, the same way, with its own limit.
  grep -x -F '        os: [macos-15, ubuntu-22.04]' <<<"$release" >/dev/null &&
    grep -x -F '    runs-on: ${{ matrix.os }}' <<<"$release" >/dev/null ||
    { echo "CI: release must run on macos-15 and ubuntu-22.04"; return 1; }
  grep -x -F '    timeout-minutes: 15' <<<"$release" >/dev/null ||
    { echo "CI: release must run within 15 minutes"; return 1; }
  [ "$(grep -c -E '^ +run: just ' <<<"$release")" = 1 ] && grep -E '^ +run: just ci-release$' <<<"$release" >/dev/null ||
    { echo "CI: release must run exactly: just ci-release"; return 1; }
  if grep -E '^ +if:' <<<"$release" >/dev/null; then
    echo "CI: no step of release, and not the job, may have a condition"; return 1
  fi
  [ "$(set_up "$release")" = "$(set_up "$check")" ] ||
    { echo "CI: release must be set up exactly as check is: the checkout, the toolchain, the cache and the pinned tools"; return 1; }

  # secrets: the scan of every commit, alone, on Linux, within the check job's limit, with
  # no condition; and in no other job.
  grep -x -F '    runs-on: ubuntu-22.04' <<<"$secrets" >/dev/null &&
    grep -x -F '    timeout-minutes: 15' <<<"$secrets" >/dev/null ||
    { echo "CI: secrets must run on ubuntu-22.04 within 15 minutes"; return 1; }
  [ "$(grep -c -F 'bash "$scan" --secrets-only --range "$BASE" "$HEAD"' "$file")" = 1 ] &&
    grep -F 'bash "$scan" --secrets-only --range "$BASE" "$HEAD"' <<<"$secrets" >/dev/null ||
    { echo "CI: secrets, and no other job, must scan every commit"; return 1; }
  grep -F 'fetch-depth: 0' <<<"$secrets" >/dev/null ||
    { echo "CI: secrets must check out every commit (fetch-depth: 0)"; return 1; }
  if grep -E '^ +if:' <<<"$secrets" >/dev/null; then
    echo "CI: no step of secrets, and not the job, may have a condition"; return 1
  fi

  # ci: runs whatever happened, needs every other job, and passes only when each passed.
  grep -x -F '    if: always()' <<<"$ci" >/dev/null ||
    { echo "CI: the aggregate ci must run always, so that a failed job fails it and does not skip it"; return 1; }
  grep -x -F '    needs: [check, scripts, release, secrets]' <<<"$ci" >/dev/null ||
    { echo "CI: the aggregate ci must need: [check, scripts, release, secrets]"; return 1; }
  for name in check scripts release secrets; do
    grep -x -F "          test \"\${{ needs.$name.result }}\" = \"success\"" <<<"$ci" >/dev/null ||
      { echo "CI: the aggregate ci must fail unless $name succeeded"; return 1; }
  done
  # Each line of that step is a test of its own: one that fails ends the step.
  grep -x -F '        run: |' <<<"$ci" >/dev/null ||
    { echo "CI: the aggregate ci must hold one test a line"; return 1; }
  if grep -E '\|\||&& *true|; *true' <<<"$ci" >/dev/null; then
    echo "CI: the aggregate ci must not pass a failed test by"; return 1
  fi
}

expect_pass "the justfile" check_justfile justfile
expect_pass "the workflow" check_workflow "$workflow"

# AGENTS.md names the jobs a builder waits for, and the recipes of the three parts.
check_rules() { # <AGENTS.md>
  local row text
  row=$(grep -F '| `just ci-without-scripts`, `just ci-release`, `just ci-scripts` |' "$1" || true)
  for text in '`check (macos-15)` and `check (ubuntu-22.04)` run the first' \
    '`release (macos-15)` and `release (ubuntu-22.04)` run the second' \
    '`scripts (macos-15)` and `scripts (ubuntu-22.04)` run the third' \
    'The job `secrets` runs the secrets scan and the attribution check over every commit' \
    'the aggregate `ci`, which needs all seven'; do
    case "$row" in *"$text"*) ;; *) echo "AGENTS.md: the row of the three parts must say: $text"; return 1 ;; esac
  done
}
expect_pass "AGENTS.md" check_rules AGENTS.md

# changed <check> <file> <sed script> <expected text>: the file after the edit must fail.
changed() {
  sed "$3" "$2" >"$tmp/changed"
  cmp -s "$2" "$tmp/changed" && fail "the edit changed nothing: $3"
  expect_fail "$2 after: $3" "$4" "$1" "$tmp/changed"
}

# --- The justfile: a recipe in one place and not in the other.
changed check_justfile justfile 's/^check-full: fmt-check clippy graph deny test-scripts /check-full: fmt-check clippy graph deny /' \
  "check-full must run every script test"
changed check_justfile justfile 's/^ci: check-full /ci: fmt-check /' "ci must depend on check-full"
changed check_justfile justfile 's/^ci: check-full /ci: check-full check /' "ci must not depend on check"
changed check_justfile justfile 's/^ci: check-full /ci: check-full test-scripts-local /' "ci-without-scripts and then ci-release must depend on exactly"
# `check` and `check-full` apart: a recipe in one and not the other, or the local choice
# in check-full, or every script test in check.
changed check_justfile justfile 's/^check: fmt-check /check: fmt-check new-check /' "check must depend on exactly what check-full does"
changed check_justfile justfile 's/^check-full: fmt-check /check-full: fmt-check new-check /' "check must depend on exactly what check-full does"
changed check_justfile justfile 's/^check: fmt-check clippy graph deny test-scripts-local /check: fmt-check clippy graph deny test-scripts /' \
  "check must depend on exactly what check-full does"
changed check_justfile justfile 's/^check-full: fmt-check clippy graph deny test-scripts /check-full: fmt-check clippy graph deny test-scripts-local /' \
  "check-full must run every script test"
changed check_justfile justfile 's/^    scripts\/script-tests-local.sh$/    scripts\/tests\/run.sh --only scripts\/tests\/test-pack.sh/' \
  "test-scripts-local must run exactly"
changed check_justfile justfile 's/^check-full: .*$/&\
    true/' "run nothing of their own"
# A recipe added to both: ci-without-scripts must have it too.
changed check_justfile justfile 's/^check: fmt-check /check: fmt-check new-check /;s/^check-full: fmt-check /check-full: fmt-check new-check /' \
  "ci-without-scripts and then ci-release must depend on exactly"
changed check_justfile justfile 's/^ci: check-full deny-advisories /ci: check-full deny-advisories new-step /' "ci-without-scripts and then ci-release must depend on exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) timing /\1 /' "ci-without-scripts and then ci-release must depend on exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) docs-check /\1 /' "ci-without-scripts and then ci-release must depend on exactly"
changed check_justfile justfile 's/^\(ci-release: .*\) scan-secrets$/\1/' "ci-without-scripts and then ci-release must depend on exactly"
# The release build back in the first part, or another recipe moved into the third.
changed check_justfile justfile 's/^ci-release: smoke /ci-release: /;s/^\(ci-without-scripts: .*\)$/\1 smoke/' "ci-release must depend on exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) deny-advisories$/\1/;s/^ci-release: /ci-release: deny-advisories /' "ci-release must depend on exactly"
changed check_justfile justfile 's/^ci-release: smoke compile-fail scan-secrets$/&\
    true/' "run nothing of their own"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/ci-scripts: deny/' "ci-scripts must depend on exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/ci-scripts: test-scripts/' "ci-scripts must depend on exactly"
changed check_justfile justfile 's/^    scripts\/tests\/run.sh$/    true/' "test-scripts must run exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/&\
    true/' "run nothing of their own"

changed check_rules AGENTS.md 's/`scripts (macos-15)` and `scripts (ubuntu-22.04)` run the third/the other jobs run the third/' "must say: \`scripts (macos-15)\`"
changed check_rules AGENTS.md 's/`release (macos-15)` and `release (ubuntu-22.04)` run the second/the other jobs run the second/' "must say: \`release (macos-15)\`"
changed check_rules AGENTS.md 's/ and `check (ubuntu-22.04)` run the first/ runs the first/' "must say: \`check (macos-15)\`"
changed check_rules AGENTS.md 's/, which needs all seven/, which needs five/' "must say: the aggregate"
changed check_rules AGENTS.md 's/The job `secrets` runs/A job runs/' "must say: The job"

# --- The workflow.
# The aggregate without the new job: a red script test on macOS would not fail it.
changed check_workflow "$workflow" 's/^    needs: \[check, scripts, release, secrets\]$/    needs: [check, scripts, secrets]/' "must need: [check, scripts, release, secrets]"
changed check_workflow "$workflow" '/needs\.release\.result/d' "must fail unless release succeeded"
changed check_workflow "$workflow" '/needs\.secrets\.result/d' "must fail unless secrets succeeded"
# The scan of every commit in another job as well, skipped, allowed to fail, given more
# time, or on macOS.
changed check_workflow "$workflow" 's/^\( *\)bash "$scan" --secrets-only.*$/&\
\1bash "$scan" --secrets-only --range "$BASE" "$HEAD"/' "secrets, and no other job, must scan every commit"
changed check_workflow "$workflow" 's/^  secrets:$/&\
    if: github.event_name == '"'push'"'/' "no step of secrets"
changed check_workflow "$workflow" 's/^  secrets:$/&\
    continue-on-error: true/' "nothing may continue on error"
awk '/^  secrets:$/ { on = 1 } on && /^    timeout-minutes: 15$/ { $0 = "    timeout-minutes: 30"; on = 0 } { print }' "$workflow" >"$tmp/longer"
expect_fail "secrets with a longer limit" "within 15 minutes" check_workflow "$tmp/longer"
awk '/^  secrets:$/ { on = 1 } on && /^    runs-on: ubuntu-22.04$/ { $0 = "    runs-on: macos-15"; on = 0 } { print }' "$workflow" >"$tmp/elsewhere"
expect_fail "secrets on macOS" "secrets must run on ubuntu-22.04" check_workflow "$tmp/elsewhere"
changed check_workflow "$workflow" '/needs\.scripts\.result/d' "must fail unless scripts succeeded"
changed check_workflow "$workflow" '/needs\.check\.result/d' "must fail unless check succeeded"
changed check_workflow "$workflow" 's/^\( *test .*needs\.scripts\.result.*\)$/\1 || true/' "must fail unless scripts succeeded"
changed check_workflow "$workflow" 's/^\( *\)test .*needs\.scripts\.result.*$/&\
\1test "$ok" = 1 || true/' "must not pass a failed test by"
changed check_workflow "$workflow" 's/^    if: always()$/    if: success()/' "must run always"
changed check_workflow "$workflow" 's/^        run: |$/        run: >/' "one test a line"
# The cache keyed without the profiles, or not saved when the job is stopped at its limit.
changed check_workflow "$workflow" "s/hashFiles('Cargo.toml', /hashFiles(/" "keyed by the workspace's Cargo.toml"
changed check_workflow "$workflow" 's/^          cache-on-failure: true$/          cache-on-failure: false/' "saved when a step fails"
# The script tests on macOS in neither job, or the whole of `just ci` there as well.
changed check_workflow "$workflow" 's/^\( *\)run: just ci-scripts$/\1run: just deny/' "scripts must run exactly: just ci-scripts"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-scripts$/&\
\1run: just deny/' "scripts must run exactly: just ci-scripts"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-without-scripts$/\1if: matrix.os == '"'macos-15'"'\
\1run: just ci-without-scripts/' "with no condition"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-without-scripts$/\1run: just check/' "one step that runs just ci-without-scripts"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-without-scripts$/&\
\1run: just ci/' "one step that runs just ci-without-scripts"
changed check_workflow "$workflow" 's/^        os: \[macos-15, ubuntu-22.04\]$/        os: [ubuntu-22.04]/' "check must run on macos-15 and ubuntu-22.04"
# Only the second matrix, which is the job scripts'.
awk '/^        os: \[macos-15, ubuntu-22.04\]$/ { n++; if (n == 2) $0 = "        os: [macos-15]" } { print }' "$workflow" >"$tmp/matrix"
expect_fail "the second matrix on macOS alone" "scripts must run on macos-15 and ubuntu-22.04" check_workflow "$tmp/matrix"
# The new job somewhere else, skipped, or allowed to fail.
changed check_workflow "$workflow" 's/^    runs-on: ${{ matrix.os }}$/    runs-on: macos-15/' "scripts must run on macos-15 and ubuntu-22.04"
changed check_workflow "$workflow" 's/^  scripts:$/&\
    if: github.event_name == '"'push'"'/' "may have a condition"
changed check_workflow "$workflow" 's/^  scripts:$/&\
    continue-on-error: true/' "nothing may continue on error"
changed check_workflow "$workflow" 's/^  scripts:$/  tests:/' "the jobs must be check, scripts, release, secrets and ci"
# The third job: elsewhere, skipped, allowed to fail, given more time, running something
# else, or set up another way.
changed check_workflow "$workflow" 's/^  release:$/&\
    if: github.event_name == '"'push'"'/' "no step of release"
changed check_workflow "$workflow" 's/^  release:$/&\
    continue-on-error: true/' "nothing may continue on error"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-release$/\1run: just smoke/' "release must run exactly: just ci-release"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-release$/\1if: matrix.os == '"'macos-15'"'\
\1run: just ci-release/' "no step of release"
awk '/^  release:$/ { on = 1 } on && /^    timeout-minutes: 15$/ { $0 = "    timeout-minutes: 30"; on = 0 } { print }' "$workflow" >"$tmp/release-longer"
expect_fail "release with a longer limit" "release must run within 15 minutes" check_workflow "$tmp/release-longer"
awk '/^        os: \[macos-15, ubuntu-22.04\]$/ { n++; if (n == 3) $0 = "        os: [ubuntu-22.04]" } { print }' "$workflow" >"$tmp/release-matrix"
expect_fail "the third matrix on Linux alone" "release must run on macos-15 and ubuntu-22.04" check_workflow "$tmp/release-matrix"
# Set up another way than check is: other tools, no toolchain, or a cache that is not saved
# when the job fails. Only the second of the three set-ups (the job scripts) or the third
# (the job release) is changed.
nth() { # <which match> <its job> <replacement line, or delete> <line, as a pattern>
  awk -v which="$1" -v pattern="$4" -v script="$3" '$0 ~ pattern { n++; if (n == which) { if (script == "delete") next; $0 = script } } { print }' "$workflow" >"$tmp/nth"
  cmp -s "$workflow" "$tmp/nth" && fail "the edit changed nothing: $4"
  expect_fail "set-up $1 after: $3" "$2 must be set up exactly as check is" check_workflow "$tmp/nth"
}
second() { nth 2 scripts "$@"; }
third() { nth 3 release "$@"; }
second '          tool: just@1.59.0' '^          tool: '
second delete '^        run: rustup show$'
second delete '^      - uses: Swatinem/rust-cache@'
second '      - uses: actions/checkout@v3' '^      - uses: actions/checkout@'
second '          cache-on-failure: false' '^          cache-on-failure: '
third '          tool: just@1.59.0' '^          tool: '
third delete '^        run: rustup show$'
third delete '^      - uses: Swatinem/rust-cache@'
third '          cache-on-failure: false' '^          cache-on-failure: '

finished
