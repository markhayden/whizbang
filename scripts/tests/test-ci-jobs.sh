#!/usr/bin/env bash
# The jobs of CI (.github/workflows/ci.yml) and the recipes they run.
#
# On the macOS runner the script tests alone took three to five and a half minutes, which
# put the one job over its budget of ten (docs/plans/m1c-local-pack.md, tasks C0 and C0b),
# and the Linux runner's one job was over it too (docs/plans/m2b-checkpoint-1d.md, task
# F1, item 5). So each runner runs `just ci` as jobs side by side: `check (<runner>)`
# runs `just ci-without-scripts` and `scripts (<runner>)` runs `just ci-scripts`. Later the
# release build and what follows it took `check (macos-15)` to 13 to 15 minutes of its 15,
# so `release (<runner>)` runs those: `just ci-release`. Since plan m4, task R1, that job
# also makes the release archive and installs it: `just package`, then `just install-check`,
# which is the bootstrap of what a user must have and the install check on what was built.
# What is held here:
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
#   only when each of them did;
# - since plan m5, task U1: the Chrome tier is the fourth part, the recipe `e2e`, in the
#   jobs `e2e (<runner>, shard <k>)`, two shards on each runner (since the m5 live
#   fixes), set up as `check` is and with Node; `check` and `scripts` set up
#   the same Node, since `ui-check` is in the first part and a script test builds ui/; a
#   type error or a stale ui/dist/ fails `ui-check` (scripts/tests/test-ui.sh), so the
#   first part;
# - since November 2026: the fast tests are split in two, each under the limit of 300
#   seconds, because with all of them in `check (macos-15)` its test time came to 225 to
#   271 seconds: the first part runs `timing-fast` in the place of `timing`, and the jobs
#   `sweeps (<runner>)` run the rest, `timing-sweeps`, set up as `check` is, with no
#   condition but the one that puts Linux's scratch folders in memory. `check` and
#   `check-full` still run `timing`, every test. That the two parts are the whole is held
#   by scripts/tests/test-timing.sh, where the split is named (scripts/timing.sh);
# - since 2026-11-22: the script tests are split in two shards on each runner, the jobs
#   `scripts (<runner>, shard <k>)`, each handed WHIZBANG_SCRIPT_SHARD=<k>/2
#   (scripts/tests/run.sh), because with all of them in `scripts (macos-15)` that job took
#   11 to 12.5 minutes of pull-request CI's 10 (SPEC.md section 13).
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
  # `ci`, with `check-full` written out, the script tests left out, and the fast tests'
  # first part in the place of the whole (the second is the job `sweeps`).
  for word in $(deps "$file" ci); do
    if [ "$word" = check-full ]; then
      for word in $(deps "$file" check-full); do
        [ "$word" = timing ] && word=timing-fast
        [ "$word" = test-scripts ] || want="$want $word"
      done
    else
      want="$want $word"
    fi
  done
  [ " $(deps "$file" ci-without-scripts) $(deps "$file" ci-release) e2e" = "$want" ] ||
    { echo "justfile: ci-without-scripts, then ci-release, then e2e must be exactly what ci runs, in its order, without test-scripts and with timing-fast for timing:$want"; return 1; }
  # The fast tests: the whole in `check`, and its two parts, each one run of the one script.
  case " $(deps "$file" check-full) " in *" timing "*) ;; *)
    echo "justfile: check-full must run timing, every fast test"; return 1 ;; esac
  [ "$(body "$file" timing)" = "scripts/timing.sh" ] ||
    { echo "justfile: timing must run exactly: scripts/timing.sh"; return 1; }
  [ "$(body "$file" timing-fast)" = "scripts/timing.sh --part fast" ] ||
    { echo "justfile: timing-fast must run exactly: scripts/timing.sh --part fast"; return 1; }
  [ "$(body "$file" timing-sweeps)" = "scripts/timing.sh --part sweeps" ] ||
    { echo "justfile: timing-sweeps must run exactly: scripts/timing.sh --part sweeps"; return 1; }
  # The interface: checked in the fast tier before the timing table counts its seconds;
  # built and checked by the one script; the Chrome tier by its own.
  case " $(deps "$file" check-full) " in *" ui-check timing "*) ;; *)
    echo "justfile: check-full must run ui-check just before timing, which counts its seconds"; return 1 ;; esac
  [ "$(body "$file" ui-check)" = "scripts/ui.sh check" ] ||
    { echo "justfile: ui-check must run exactly: scripts/ui.sh check"; return 1; }
  [ "$(body "$file" ui-build)" = "scripts/ui.sh build" ] ||
    { echo "justfile: ui-build must run exactly: scripts/ui.sh build"; return 1; }
  [ "$(body "$file" e2e)" = "scripts/e2e.sh" ] ||
    { echo "justfile: e2e must run exactly: scripts/e2e.sh"; return 1; }
  # The release build and what follows it, in the job `release`; nothing else is there.
  # The archive is made after the smoke, which built the binary, and installed after it
  # is made.
  [ "$(deps "$file" ci-release)" = "smoke compile-fail scan-secrets package install-check" ] ||
    { echo "justfile: ci-release must depend on exactly: smoke compile-fail scan-secrets package install-check"; return 1; }
  # `package` is the one script. `install-check` is the bootstrap into a fresh folder, with
  # the uv that is on the path, and then the install check on what `package` made, with
  # that folder and nothing else of the machine's.
  [ "$(body "$file" package)" = "scripts/package.sh" ] ||
    { echo "justfile: package must run exactly: scripts/package.sh"; return 1; }
  [ "$(body "$file" install-check)" = 'rm -rf target/install-prereqs
scripts/install-prereqs.sh "$(command -v uv)" target/install-prereqs
scripts/install-check.sh target/package/whizbang-*.tar.gz target/package/SHA256SUMS target/install-prereqs
rm -rf target/install-prereqs' ] ||
    { echo "justfile: install-check must run exactly the bootstrap into a fresh target/install-prereqs and then the install check on the archive of target/package"; return 1; }
  # The other half. `deny` goes first, as in `check`: scripts/tests/test-deny.sh uses no
  # network and reads the workspace's crates from cargo's home, where `just deny` put them.
  [ "$(deps "$file" ci-scripts)" = "deny test-scripts" ] ||
    { echo "justfile: ci-scripts must depend on exactly: deny test-scripts"; return 1; }
  [ -z "$(body "$file" ci-without-scripts)$(body "$file" ci-release)$(body "$file" ci-scripts)" ] ||
    { echo "justfile: ci-without-scripts, ci-release and ci-scripts run nothing of their own"; return 1; }
}

# jobs <workflow>: the name of each job, one a line.
jobs() { awk '/^jobs:$/ { on = 1; next } on && /^  [a-z][a-z0-9-]*:$/ { sub(/^  /, ""); sub(/:$/, ""); print }' "$1"; }
# job <workflow> <name>: the lines of one job.
job() {
  awk -v head="  $2:" '/^jobs:$/ { in_jobs = 1; next }
    in_jobs && /^  [a-z][a-z0-9-]*:$/ { on = ($0 == head); next } on' "$1"
}
# before <text> <step's line, as a pattern>: the line above that step's line.
before() { grep -B1 -E "$2" <<<"$1" | head -n 1 || true; }
# set_up <job's lines>: from its checkout to the line that names the pinned tools.
set_up() { awk '/^      - uses: actions\/checkout@/ { on = 1 } on { print } on && /^          tool: / { exit }' <<<"$1"; }

# node <job's lines>: the step that sets up Node, from its `uses` to the next step.
node() { awk '/^      - uses: actions\/setup-node@/ { on = 1; print; next } on && /^        / { print; next } on { exit }' <<<"$1"; }
NODE_STEP='      - uses: actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1
        with:
          node-version: 26.13.1
          cache: npm
          cache-dependency-path: ui/package-lock.json'

check_workflow() { # <workflow>
  local file=$1 check scripts release e2e sweeps secrets ci name
  [ "$(jobs "$file" | tr '\n' ' ')" = "check scripts release e2e sweeps secrets ci " ] ||
    { echo "CI: the jobs must be check, scripts, release, e2e, sweeps, secrets and ci (found: $(jobs "$file" | tr '\n' ' '))"; return 1; }
  # A step or a job that may fail without failing the run would pass anything.
  if grep -E 'continue-on-error' "$file" >/dev/null; then
    echo "CI: nothing may continue on error"; return 1
  fi
  check=$(job "$file" check)
  scripts=$(job "$file" scripts)
  release=$(job "$file" release)
  e2e=$(job "$file" e2e)
  sweeps=$(job "$file" sweeps)
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
  # In two shards on each runner, each named by the job and handed to the script tests.
  grep -x -F '        shard: [1, 2]' <<<"$scripts" >/dev/null &&
    grep -x -F '          WHIZBANG_SCRIPT_SHARD: ${{ matrix.shard }}/2' <<<"$scripts" >/dev/null &&
    grep -x -F '    name: scripts (${{ matrix.os }}, shard ${{ matrix.shard }})' <<<"$scripts" >/dev/null ||
    { echo "CI: scripts must run in two shards on each runner, each handed WHIZBANG_SCRIPT_SHARD"; return 1; }
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

  # Node: the one step, pinned to a commit, at the pin, with npm's cache keyed by the
  # lock file, in check, scripts and e2e (ui-check, the script test of ui/, the Chrome tier).
  for name in check scripts e2e; do
    [ "$(node "$(job "$file" "$name")")" = "$NODE_STEP" ] ||
      { echo "CI: $name must set up Node with actions/setup-node pinned to its commit, at the pin, with npm's cache keyed by ui/package-lock.json"; return 1; }
  done
  [ "$(grep -c -F 'uses: actions/setup-node@' "$file")" = 3 ] ||
    { echo "CI: Node is set up in check, scripts and e2e, and nowhere else"; return 1; }

  # e2e: the fourth part, on both runners, set up as check is, within 15 minutes, running
  # the Chrome tier in a step with no condition; only the report of a failed run is kept.
  grep -x -F '        os: [macos-15, ubuntu-22.04]' <<<"$e2e" >/dev/null &&
    grep -x -F '    runs-on: ${{ matrix.os }}' <<<"$e2e" >/dev/null ||
    { echo "CI: e2e must run on macos-15 and ubuntu-22.04"; return 1; }
  grep -x -F '    timeout-minutes: 15' <<<"$e2e" >/dev/null ||
    { echo "CI: e2e must run within 15 minutes"; return 1; }
  # In two shards on each runner, each named by the job and handed to the tier.
  grep -x -F '        shard: [1, 2]' <<<"$e2e" >/dev/null &&
    grep -x -F '          WHIZBANG_E2E_SHARD: ${{ matrix.shard }}/2' <<<"$e2e" >/dev/null &&
    grep -x -F '    name: e2e (${{ matrix.os }}, shard ${{ matrix.shard }})' <<<"$e2e" >/dev/null ||
    { echo "CI: e2e must run in two shards on each runner, each handed WHIZBANG_E2E_SHARD"; return 1; }
  [ "$(set_up "$e2e")" = "$(set_up "$check")" ] ||
    { echo "CI: e2e must be set up exactly as check is: the checkout, the toolchain, the cache and the pinned tools"; return 1; }
  [ "$(grep -c -E '^ +run: just ' <<<"$e2e")" = 1 ] && grep -E '^ +run: just e2e$' <<<"$e2e" >/dev/null ||
    { echo "CI: e2e must run exactly: just e2e"; return 1; }
  case "$(before "$e2e" '^ +run: just e2e$')" in *' if: '*)
    echo "CI: e2e must run just e2e on every runner, with no condition"; return 1 ;; esac
  [ "$(grep -E '^ +if:' <<<"$e2e")" = "        if: failure()" ] &&
    grep -A1 -x -F '        if: failure()' <<<"$e2e" | grep -x -F '        uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9' >/dev/null ||
    { echo "CI: the one condition in e2e keeps the report of a failed run, with actions/upload-artifact pinned to its commit, and nothing else"; return 1; }

  # sweeps: the fast tests' second part, on both runners, set up as check is, within 15
  # minutes, in a step with no condition; the one condition puts Linux's scratch folders
  # in memory, as in check.
  grep -x -F '        os: [macos-15, ubuntu-22.04]' <<<"$sweeps" >/dev/null &&
    grep -x -F '    runs-on: ${{ matrix.os }}' <<<"$sweeps" >/dev/null ||
    { echo "CI: sweeps must run on macos-15 and ubuntu-22.04"; return 1; }
  grep -x -F '    timeout-minutes: 15' <<<"$sweeps" >/dev/null ||
    { echo "CI: sweeps must run within 15 minutes"; return 1; }
  [ "$(set_up "$sweeps")" = "$(set_up "$check")" ] ||
    { echo "CI: sweeps must be set up exactly as check is: the checkout, the toolchain, the cache and the pinned tools"; return 1; }
  [ "$(grep -c -E '^ +run: just ' <<<"$sweeps")" = 1 ] && grep -E '^ +run: just timing-sweeps$' <<<"$sweeps" >/dev/null ||
    { echo "CI: sweeps must run exactly: just timing-sweeps"; return 1; }
  [ "$(grep -E '^ +if:' <<<"$sweeps")" = "        if: matrix.os == 'ubuntu-22.04'" ] &&
    grep -A3 -x -F "        if: matrix.os == 'ubuntu-22.04'" <<<"$sweeps" | grep -x -F '          echo "TMPDIR=/dev/shm" >>"$GITHUB_ENV"' >/dev/null ||
    { echo "CI: the one condition in sweeps puts Linux's scratch folders in memory, and nothing else"; return 1; }

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
  grep -x -F '    needs: [check, scripts, release, e2e, sweeps, secrets]' <<<"$ci" >/dev/null ||
    { echo "CI: the aggregate ci must need: [check, scripts, release, e2e, sweeps, secrets]"; return 1; }
  for name in check scripts release e2e sweeps secrets; do
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

# AGENTS.md names the jobs a builder waits for, and the recipes of the parts.
check_rules() { # <AGENTS.md>
  local row text
  row=$(grep -F '| `just ci-without-scripts`, `just ci-release`, `just ci-scripts`, `just timing-sweeps` |' "$1" || true)
  for text in '`check (macos-15)` and `check (ubuntu-22.04)` run the first' \
    '`release (macos-15)` and `release (ubuntu-22.04)` run the second' \
    '`scripts (macos-15, shard 1)`, `scripts (macos-15, shard 2)`, `scripts (ubuntu-22.04, shard 1)` and `scripts (ubuntu-22.04, shard 2)` run the third' \
    'The job `secrets` runs the secrets scan and the attribution check over every commit' \
    '`e2e (macos-15, shard 1)`, `e2e (macos-15, shard 2)`, `e2e (ubuntu-22.04, shard 1)` and `e2e (ubuntu-22.04, shard 2)` run the fourth, `just e2e`' \
    '`sweeps (macos-15)` and `sweeps (ubuntu-22.04)` run the fifth, `just timing-sweeps`' \
    'the aggregate `ci`, which needs all fifteen'; do
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
changed check_justfile justfile 's/^ci: check-full /ci: check-full test-scripts-local /' "ci-without-scripts, then ci-release, then e2e must be exactly"
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
  "ci-without-scripts, then ci-release, then e2e must be exactly"
changed check_justfile justfile 's/^ci: check-full deny-advisories /ci: check-full deny-advisories new-step /' "ci-without-scripts, then ci-release, then e2e must be exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) docs-check /\1 /' "ci-without-scripts, then ci-release, then e2e must be exactly"
changed check_justfile justfile 's/^\(ci-release: .*\) scan-secrets /\1 /' "ci-without-scripts, then ci-release, then e2e must be exactly"
# The release build back in the first part, or another recipe moved into the third.
changed check_justfile justfile 's/^ci-release: smoke /ci-release: /;s/^\(ci-without-scripts: .*\)$/\1 smoke/' "ci-release must depend on exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) deny-advisories$/\1/;s/^ci-release: /ci-release: deny-advisories /' "ci-release must depend on exactly"
changed check_justfile justfile 's/^ci-release: smoke compile-fail scan-secrets package install-check$/&\
    true/' "run nothing of their own"
# The release job without the archive, without its install, or with them the other way
# round; the archive made another way; the install check without the bootstrap, on a
# folder that is not fresh, or with the machine's own uv and Python in place of the folder.
changed check_justfile justfile 's/^\(ci: .*\) package install-check e2e$/\1 install-check e2e/;s/^\(ci-release: .*\) package install-check$/\1 install-check/' \
  "ci-release must depend on exactly"
changed check_justfile justfile 's/^\(ci: .*\) package install-check e2e$/\1 package e2e/;s/^\(ci-release: .*\) package install-check$/\1 package/' \
  "ci-release must depend on exactly"
changed check_justfile justfile 's/^\(ci: .*\) package install-check e2e$/\1 install-check package e2e/;s/^\(ci-release: .*\) package install-check$/\1 install-check package/' \
  "ci-release must depend on exactly"
changed check_justfile justfile 's/^    scripts\/package.sh$/    cargo build --release -p whizbang/' "package must run exactly"
changed check_justfile justfile '/^    scripts\/install-prereqs.sh /d' "install-check must run exactly"
awk '/^    rm -rf target\/install-prereqs$/ && !seen { seen = 1; next } { print }' justfile >"$tmp/not-fresh"
expect_fail "the bootstrap into a folder that is not fresh" "install-check must run exactly" check_justfile "$tmp/not-fresh"
changed check_justfile justfile '/^    scripts\/install-check.sh /d' "install-check must run exactly"
changed check_justfile justfile 's/^\(    scripts\/install-check.sh .*\) target\/install-prereqs$/\1 "$HOME"/' "install-check must run exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/ci-scripts: deny/' "ci-scripts must depend on exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/ci-scripts: test-scripts/' "ci-scripts must depend on exactly"
changed check_justfile justfile 's/^    scripts\/tests\/run.sh$/    true/' "test-scripts must run exactly"
changed check_justfile justfile 's/^ci-scripts: deny test-scripts$/&\
    true/' "run nothing of their own"

changed check_rules AGENTS.md 's/ and `scripts (ubuntu-22.04, shard 2)` run the third/ run the third/' "must say: \`scripts (macos-15, shard 1)\`"
changed check_rules AGENTS.md 's/`release (macos-15)` and `release (ubuntu-22.04)` run the second/the other jobs run the second/' "must say: \`release (macos-15)\`"
changed check_rules AGENTS.md 's/ and `check (ubuntu-22.04)` run the first/ runs the first/' "must say: \`check (macos-15)\`"
changed check_rules AGENTS.md 's/, which needs all fifteen/, which needs thirteen/' "must say: the aggregate"
changed check_rules AGENTS.md 's/`sweeps (macos-15)` and `sweeps (ubuntu-22.04)` run the fifth/the sweeps run the fifth/' "must say: \`sweeps (macos-15)\`"
changed check_rules AGENTS.md 's/and `e2e (ubuntu-22.04, shard 2)` run the fourth/run the fourth/' "must say: \`e2e (macos-15, shard 1)\`"
changed check_rules AGENTS.md 's/The job `secrets` runs/A job runs/' "must say: The job"

# --- The workflow.
# The aggregate without the new job: a red script test on macOS would not fail it.
changed check_workflow "$workflow" 's/^    needs: \[check, scripts, release, e2e, sweeps, secrets\]$/    needs: [check, scripts, release, sweeps, secrets]/' "must need: [check, scripts, release, e2e, sweeps, secrets]"
changed check_workflow "$workflow" 's/^    needs: \[check, scripts, release, e2e, sweeps, secrets\]$/    needs: [check, scripts, release, e2e, secrets]/' "must need: [check, scripts, release, e2e, sweeps, secrets]"
changed check_workflow "$workflow" '/needs\.sweeps\.result/d' "must fail unless sweeps succeeded"
# injob <job> <line> <replacement, or delete> <expected text>: that line of that job alone
# changed, and the workflow must fail.
injob() {
  awk -v head="  $1:" -v line="$2" -v repl="$3" '/^  [a-z][a-z0-9-]*:$/ { on = ($0 == head) }
    on && $0 == line { if (repl == "delete") next; $0 = repl } { print }' "$workflow" >"$tmp/injob"
  cmp -s "$workflow" "$tmp/injob" && fail "the edit changed nothing in $1: $2"
  expect_fail "$1 after: $2 -> $3" "$4" check_workflow "$tmp/injob"
}
injob e2e '        shard: [1, 2]' '        shard: [1]' "e2e must run in two shards"
changed check_workflow "$workflow" '/WHIZBANG_E2E_SHARD: /d' "e2e must run in two shards"
injob e2e '    name: e2e (${{ matrix.os }}, shard ${{ matrix.shard }})' '    name: e2e (${{ matrix.os }})' "e2e must run in two shards"
# The script tests in one job on a runner again, or a shard not handed to them.
injob scripts '        shard: [1, 2]' '        shard: [1]' "scripts must run in two shards"
injob scripts '        shard: [1, 2]' delete "scripts must run in two shards"
changed check_workflow "$workflow" '/WHIZBANG_SCRIPT_SHARD: /d' "scripts must run in two shards"
injob scripts '    name: scripts (${{ matrix.os }}, shard ${{ matrix.shard }})' '    name: scripts (${{ matrix.os }})' "scripts must run in two shards"
changed check_workflow "$workflow" '/needs\.e2e\.result/d' "must fail unless e2e succeeded"
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
changed check_workflow "$workflow" 's/^  scripts:$/  tests:/' "the jobs must be check, scripts, release, e2e, sweeps, secrets and ci"
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
fourth() { nth 4 e2e "$@"; }
fourth '          tool: just@1.59.0' '^          tool: '
fourth delete '^        run: rustup show$'

# --- The interface and the Chrome tier (plan m5, task U1).
changed check_justfile justfile 's/^\(check-full: .*\) ui-check timing /\1 timing ui-check /;s/^\(check: .*\) ui-check timing /\1 timing ui-check /;s/^\(ci-without-scripts: .*\) ui-check timing-fast /\1 timing-fast ui-check /' \
  "check-full must run ui-check just before timing"
changed check_justfile justfile 's/^\(check-full: .*\) ui-check /\1 /;s/^\(check: .*\) ui-check /\1 /;s/^\(ci-without-scripts: .*\) ui-check /\1 /' \
  "check-full must run ui-check just before timing"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) ui-check /\1 /' "ci-without-scripts, then ci-release, then e2e must be exactly"
changed check_justfile justfile 's/^\(ci: .*\) e2e$/\1/' "ci-without-scripts, then ci-release, then e2e must be exactly"
changed check_justfile justfile 's/^    scripts\/ui.sh check$/    scripts\/ui.sh build/' "ui-check must run exactly"
changed check_justfile justfile 's/^    scripts\/e2e.sh$/    scripts\/e2e.sh --limit 999/' "e2e must run exactly"
changed check_workflow "$workflow" 's/^  e2e:$/  chrome:/' "the jobs must be check, scripts, release, e2e, sweeps, secrets and ci"
changed check_workflow "$workflow" 's/^\( *\)run: just e2e$/\1if: matrix.os == '"'macos-15'"'\
\1run: just e2e/' "e2e must run just e2e on every runner, with no condition"
changed check_workflow "$workflow" 's/^\( *\)run: just e2e$/\1run: just ui-check/' "e2e must run exactly: just e2e"
changed check_workflow "$workflow" 's/^  e2e:$/&\
    continue-on-error: true/' "nothing may continue on error"
awk '/^  e2e:$/ { on = 1 } on && /^    timeout-minutes: 15$/ { $0 = "    timeout-minutes: 30"; on = 0 } { print }' "$workflow" >"$tmp/e2e-longer"
expect_fail "e2e with a longer limit" "e2e must run within 15 minutes" check_workflow "$tmp/e2e-longer"
awk '/^        os: \[macos-15, ubuntu-22.04\]$/ { n++; if (n == 4) $0 = "        os: [ubuntu-22.04]" } { print }' "$workflow" >"$tmp/e2e-matrix"
expect_fail "the fourth matrix on Linux alone" "e2e must run on macos-15 and ubuntu-22.04" check_workflow "$tmp/e2e-matrix"
changed check_workflow "$workflow" 's/^        if: failure()$/        if: always()/' "the one condition in e2e"
changed check_workflow "$workflow" 's/^        uses: actions\/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9$/        uses: actions\/upload-artifact@v7/' "the one condition in e2e"
# Node at another version, unpinned, without the lock file's cache, or missing from a job.
changed check_workflow "$workflow" 's/^          node-version: 26.13.1$/          node-version: 26/' "must set up Node with actions/setup-node pinned"
changed check_workflow "$workflow" 's/^      - uses: actions\/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1$/      - uses: actions\/setup-node@v7/' "must set up Node with actions/setup-node pinned"
changed check_workflow "$workflow" '/^          cache-dependency-path: ui\/package-lock.json$/d' "must set up Node with actions/setup-node pinned"
awk '/^      - uses: actions\/setup-node@/ { n++; if (n == 2) { skip = 5 } } skip > 0 { skip--; next } { print }' "$workflow" >"$tmp/no-node"
expect_fail "scripts without Node" "scripts must set up Node" check_workflow "$tmp/no-node"

# --- The fast tests in two parts.
# The whole back in the first part, the first part with nothing of the fast tests, a part
# run another way, or the whole gone from check.
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) timing-fast /\1 timing /' "ci-without-scripts, then ci-release, then e2e must be exactly"
changed check_justfile justfile 's/^\(ci-without-scripts: .*\) timing-fast /\1 /' "ci-without-scripts, then ci-release, then e2e must be exactly"
changed check_justfile justfile 's/^    scripts\/timing.sh --part sweeps$/    scripts\/timing.sh --part fast/' "timing-sweeps must run exactly"
changed check_justfile justfile 's/^    scripts\/timing.sh --part fast$/    scripts\/timing.sh/' "timing-fast must run exactly"
changed check_justfile justfile 's/^    scripts\/timing.sh$/    scripts\/timing.sh --part fast/' "timing must run exactly"
changed check_justfile justfile 's/^\(check-full: .*\) timing /\1 timing-fast /;s/^\(check: .*\) timing /\1 timing-fast /' "check-full must run timing"
# The job gone, elsewhere, skipped, allowed to fail, given more time, running something
# else, with another condition, or set up another way.
changed check_workflow "$workflow" 's/^  sweeps:$/  slow:/' "the jobs must be check, scripts, release, e2e, sweeps, secrets and ci"
changed check_workflow "$workflow" 's/^  sweeps:$/&\
    if: github.event_name == '"'push'"'/' "the one condition in sweeps"
changed check_workflow "$workflow" 's/^  sweeps:$/&\
    continue-on-error: true/' "nothing may continue on error"
changed check_workflow "$workflow" 's/^\( *\)run: just timing-sweeps$/\1run: just timing-fast/' "sweeps must run exactly: just timing-sweeps"
changed check_workflow "$workflow" 's/^\( *\)run: just timing-sweeps$/&\
\1run: just test/' "sweeps must run exactly: just timing-sweeps"
changed check_workflow "$workflow" 's/^\( *\)run: just timing-sweeps$/\1if: matrix.os == '"'macos-15'"'\
\1run: just timing-sweeps/' "the one condition in sweeps"
awk '/^  sweeps:$/ { on = 1 } on && /^    timeout-minutes: 15$/ { $0 = "    timeout-minutes: 30"; on = 0 } { print }' "$workflow" >"$tmp/sweeps-longer"
expect_fail "sweeps with a longer limit" "sweeps must run within 15 minutes" check_workflow "$tmp/sweeps-longer"
awk '/^        os: \[macos-15, ubuntu-22.04\]$/ { n++; if (n == 5) $0 = "        os: [ubuntu-22.04]" } { print }' "$workflow" >"$tmp/sweeps-matrix"
expect_fail "the fifth matrix on Linux alone" "sweeps must run on macos-15 and ubuntu-22.04" check_workflow "$tmp/sweeps-matrix"
fifth() { nth 5 sweeps "$@"; }
fifth '          tool: just@1.59.0' '^          tool: '
fifth delete '^        run: rustup show$'
fifth '          cache-on-failure: false' '^          cache-on-failure: '

finished
