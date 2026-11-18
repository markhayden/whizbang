#!/usr/bin/env bash
# The jobs of CI (.github/workflows/ci.yml) and the recipes they run.
#
# On the macOS runner the script tests alone take three to five and a half minutes, which
# put the one job over its budget of ten (docs/plans/m1c-local-pack.md, tasks C0 and C0b).
# So that runner runs `just ci` as two jobs side by side: `check (macos-15)` runs
# `just ci-without-scripts` and `scripts (macos-15)` runs `just ci-scripts`. The Linux
# runner runs `just ci` in one job. What is held here:
#
# - the two halves together are `just ci`, recipe for recipe, so a recipe added to `check`
#   or to `ci` and not to the half that runs it on macOS fails `just check`;
# - `ci` runs every script test whatever changed: it depends on `check-full`, which runs
#   `test-scripts` (all of them), and never on `check`, whose `test-scripts-local` runs only
#   those a change can reach (scripts/script-tests-local.sh). `check` is `check-full` with
#   that one recipe in the place of `test-scripts`, so the two cannot drift apart;
# - the new job is set up exactly as the other is (the same actions, the same pinned tools),
#   runs its half on every pull request and push, and no step of it may be skipped;
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
  [ " $(deps "$file" ci-without-scripts)" = "$want" ] ||
    { echo "justfile: ci-without-scripts must depend on exactly what ci runs, in its order, without test-scripts:$want"; return 1; }
  # The other half. `deny` goes first, as in `check`: scripts/tests/test-deny.sh uses no
  # network and reads the workspace's crates from cargo's home, where `just deny` put them.
  [ "$(deps "$file" ci-scripts)" = "deny test-scripts" ] ||
    { echo "justfile: ci-scripts must depend on exactly: deny test-scripts"; return 1; }
  [ -z "$(body "$file" ci-without-scripts)$(body "$file" ci-scripts)" ] ||
    { echo "justfile: ci-without-scripts and ci-scripts run nothing of their own"; return 1; }
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
  local file=$1 check scripts ci name
  [ "$(jobs "$file" | tr '\n' ' ')" = "check scripts ci " ] ||
    { echo "CI: the jobs must be check, scripts and ci (found: $(jobs "$file" | tr '\n' ' '))"; return 1; }
  # A step or a job that may fail without failing the run would pass anything.
  if grep -E 'continue-on-error' "$file" >/dev/null; then
    echo "CI: nothing may continue on error"; return 1
  fi
  check=$(job "$file" check)
  scripts=$(job "$file" scripts)
  ci=$(job "$file" ci)

  # check: both runners; `just ci` on every runner but macOS, and its first half there.
  grep -x -F '        os: [macos-15, ubuntu-22.04]' <<<"$check" >/dev/null ||
    { echo "CI: check must run on macos-15 and ubuntu-22.04"; return 1; }
  [ "$(grep -c -E '^ +run: just ci' <<<"$check")" = 2 ] ||
    { echo "CI: check must have two steps that run a half or the whole of just ci: one for macOS, one for the other runner"; return 1; }
  [ "$(before "$check" '^ +run: just ci$')" = "        if: matrix.os != 'macos-15'" ] ||
    { echo "CI: check must run just ci on every runner but macos-15"; return 1; }
  [ "$(before "$check" '^ +run: just ci-without-scripts$')" = "        if: matrix.os == 'macos-15'" ] ||
    { echo "CI: check must run just ci-without-scripts on macos-15"; return 1; }

  # scripts: the other half, on that runner, set up as check is, with no step that can be
  # skipped. A job that is skipped does not count as passed below either.
  grep -x -F '    runs-on: macos-15' <<<"$scripts" >/dev/null ||
    { echo "CI: scripts must run on macos-15"; return 1; }
  [ "$(grep -c -E '^ +run: just ' <<<"$scripts")" = 1 ] && grep -E '^ +run: just ci-scripts$' <<<"$scripts" >/dev/null ||
    { echo "CI: scripts must run exactly: just ci-scripts"; return 1; }
  if grep -E '^ +if:' <<<"$scripts" >/dev/null; then
    echo "CI: no step of scripts, and not the job, may have a condition"; return 1
  fi
  [ -n "$(set_up "$check")" ] && [ "$(set_up "$scripts")" = "$(set_up "$check")" ] ||
    { echo "CI: scripts must be set up exactly as check is: the checkout, the toolchain, the cache and the pinned tools"; return 1; }

  # ci: runs whatever happened, needs every other job, and passes only when each passed.
  grep -x -F '    if: always()' <<<"$ci" >/dev/null ||
    { echo "CI: the aggregate ci must run always, so that a failed job fails it and does not skip it"; return 1; }
  grep -x -F '    needs: [check, scripts]' <<<"$ci" >/dev/null ||
    { echo "CI: the aggregate ci must need: [check, scripts]"; return 1; }
  for name in check scripts; do
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

# AGENTS.md names the jobs a builder waits for, and the recipes of the two halves.
check_rules() { # <AGENTS.md>
  local row text
  row=$(grep -F '| `just ci-without-scripts`, `just ci-scripts` |' "$1" || true)
  for text in '`check (macos-15)` runs the first' '`scripts (macos-15)` runs the second' \
    '`check (ubuntu-22.04)` runs `just ci` whole' 'the aggregate `ci`, which needs all three'; do
    case "$row" in *"$text"*) ;; *) echo "AGENTS.md: the row of the two halves must say: $text"; return 1 ;; esac
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
changed check_justfile justfile 's/^ci: check-full /ci: check-full test-scripts-local /' "ci-without-scripts must depend on exactly"
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
  "ci-without-scripts must depend on exactly"
changed check_justfile justfile 's/^ci: check-full deny-advisories /ci: check-full deny-advisories new-step /' "ci-without-scripts must depend on exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) timing /\1 /' "ci-without-scripts must depend on exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) docs-check /\1 /' "ci-without-scripts must depend on exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) scan-secrets$/\1/' "ci-without-scripts must depend on exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/ci-scripts: deny/' "ci-scripts must depend on exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/ci-scripts: test-scripts/' "ci-scripts must depend on exactly"
changed check_justfile justfile 's/^    scripts\/tests\/run.sh$/    true/' "test-scripts must run exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/&\
    true/' "run nothing of their own"

changed check_rules AGENTS.md 's/`scripts (macos-15)` runs the second/the other job runs the second/' "must say: \`scripts (macos-15)\`"
changed check_rules AGENTS.md 's/, which needs all three/, which needs both/' "must say: the aggregate"

# --- The workflow.
# The aggregate without the new job: a red script test on macOS would not fail it.
changed check_workflow "$workflow" 's/^    needs: \[check, scripts\]$/    needs: [check]/' "must need: [check, scripts]"
changed check_workflow "$workflow" '/needs\.scripts\.result/d' "must fail unless scripts succeeded"
changed check_workflow "$workflow" '/needs\.check\.result/d' "must fail unless check succeeded"
changed check_workflow "$workflow" 's/^\( *test .*needs\.scripts\.result.*\)$/\1 || true/' "must fail unless scripts succeeded"
changed check_workflow "$workflow" 's/^\( *\)test .*needs\.scripts\.result.*$/&\
\1test "$ok" = 1 || true/' "must not pass a failed test by"
changed check_workflow "$workflow" 's/^    if: always()$/    if: success()/' "must run always"
changed check_workflow "$workflow" 's/^        run: |$/        run: >/' "one test a line"
# The script tests on macOS in neither job, or the whole of `just ci` there as well.
changed check_workflow "$workflow" 's/^\( *\)run: just ci-scripts$/\1run: just deny/' "scripts must run exactly: just ci-scripts"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-scripts$/&\
\1run: just deny/' "scripts must run exactly: just ci-scripts"
changed check_workflow "$workflow" "s/^        if: matrix.os != 'macos-15'\$/        if: matrix.os == 'ubuntu-22.04'/" \
  "must run just ci on every runner but macos-15"
changed check_workflow "$workflow" "/^        if: matrix.os != 'macos-15'\$/d" "must run just ci on every runner but macos-15"
changed check_workflow "$workflow" 's/^\( *\)run: just ci-without-scripts$/\1run: just check/' "two steps that run"
changed check_workflow "$workflow" 's/^        os: \[macos-15, ubuntu-22.04\]$/        os: [ubuntu-22.04]/' "check must run on macos-15 and ubuntu-22.04"
# The new job somewhere else, skipped, or allowed to fail.
changed check_workflow "$workflow" 's/^    runs-on: macos-15$/    runs-on: ubuntu-22.04/' "scripts must run on macos-15"
changed check_workflow "$workflow" 's/^  scripts:$/&\
    if: github.event_name == '"'push'"'/' "may have a condition"
changed check_workflow "$workflow" 's/^  scripts:$/&\
    continue-on-error: true/' "nothing may continue on error"
changed check_workflow "$workflow" 's/^  scripts:$/  tests:/' "the jobs must be check, scripts and ci"
# Set up another way than check is: other tools, or no toolchain. Only the second of the two
# set-ups is changed, which is the new job's.
second() { # <sed script for the second match of the line> <line, as a pattern>
  awk -v pattern="$2" -v script="$1" '$0 ~ pattern { n++; if (n == 2) { if (script == "delete") next; $0 = script } } { print }' "$workflow" >"$tmp/second"
  cmp -s "$workflow" "$tmp/second" && fail "the edit changed nothing: $2"
  expect_fail "the second set-up after: $1" "scripts must be set up exactly as check is" check_workflow "$tmp/second"
}
second '          tool: just@1.59.0' '^          tool: '
second delete '^        run: rustup show$'
second delete '^      - uses: Swatinem/rust-cache@'
second '      - uses: actions/checkout@v3' '^      - uses: actions/checkout@'

finished
