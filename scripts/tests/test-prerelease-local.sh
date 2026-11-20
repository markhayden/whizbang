#!/usr/bin/env bash
# The local leg (scripts/prerelease-local.sh; plan m4, task R2, items 3 and 4), against
# stand-ins for everything it drives: the candidate on GitHub (`gh`), the installed binary
# (a script named `whizbang` inside the Mac's archive, which answers each command and
# hands out run records written here), the model tests and the three guards (scripts of a
# throwaway repository the leg clones), and `uname`, so the test runs on Linux too.
# Nothing is built, no model is loaded, no provider is called and nothing is published.
# The loop's command does not exist yet (task L1): the stand-in answers it as that task's
# record will, and one case holds what the leg does with a binary that refuses it.
source "$(dirname "$0")/lib.sh"

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
tag=v0.1.0-rc.2
fix="$tmp/fix"
mkdir -p "$fix/records" "$tmp/bin" "$tmp/models/models"
image=sha256:$(printf 'a%.0s' $(seq 1 64))
mask=sha256:$(printf 'b%.0s' $(seq 1 64))
filled=sha256:$(printf 'c%.0s' $(seq 1 64))
# A made-up key, in the env file the leg must never open: it may appear nowhere.
echo "FAL_KEY='made-up-key-0123456789'" >"$tmp/keys.env"

# --- The repository the candidate is cloned from: the real scripts the leg holds itself
# to, and stand-ins for the model tests and the guards, which write down that they ran.
origin="$tmp/origin"
mkdir -p "$origin/scripts"
cp scripts/prerelease-local.sh scripts/tag-version.sh scripts/release-publish.sh scripts/receipt-check.sh scripts/prerelease-receipt.jq "$origin/scripts/"
for guard in heavy memory-watch thread-guard; do
  case "$guard" in heavy) take='shift' ;; *) take=':' ;; esac
  printf '%s\n' '#!/bin/bash' "[ $guard != heavy ] || echo \"heavy \$1\" >>\"$fix/heavy\"" "$take" "export GUARDS=\"\${GUARDS:-}$guard,\"" 'exec "$@"' >"$origin/scripts/$guard.sh"
done
cat >"$origin/scripts/test-models.sh" <<SH
#!/bin/bash
# The stand-in for the model tests: a list, or a run that leaves the report of the case.
if [ "\${1:-}" = --list ]; then
  echo "list in \$PWD under \${GUARDS:-none} at \${WHIZBANG_THREAD_LIMIT:-none}" >>"$fix/tests-ran"
  echo "test-models list" >>"$fix/order"
  printf '%s\n' 'whizbang::it model_tier::a_phrase' 'whizbang::it parity::gpu_and_cpu'
  exit 0
fi
echo "run in \$PWD under \${GUARDS:-none} at \${WHIZBANG_THREAD_LIMIT:-none}" >>"$fix/tests-ran"
echo "test-models run" >>"$fix/order"
mkdir -p target/nextest/model-tier
[ ! -f "$fix/junit.xml" ] || cp "$fix/junit.xml" target/nextest/model-tier/junit.xml
exit "\$(cat "$fix/tests-exit")"
SH
chmod +x "$origin"/scripts/*.sh
echo "target/" >"$origin/.gitignore"
echo "tracked" >"$origin/tracked.txt"
git init -q "$origin"
git -C "$origin" add -A
git -C "$origin" -c user.name=test -c user.email=test@example.invalid commit -q -m "the candidate"
git -C "$origin" tag "$tag"
commit=$(git -C "$origin" rev-parse HEAD)
git clone -q "$origin" "$tmp/main"
leg_script="$tmp/main/scripts/prerelease-local.sh"

# --- The report of the model tests: both tests, passed; and one with a failure of a kind.
junit() { # <kind of the second test: passed, failed, timed-out, left-out>
  {
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<testsuites name="nextest-run" tests="2" failures="0" errors="0" time="12.5">'
    echo '    <testsuite name="whizbang::it" tests="2" disabled="0" errors="0" failures="0">'
    echo '        <testcase name="model_tier::a_phrase" classname="whizbang::it" timestamp="2026-11-19T10:00:00.000+00:00" time="4.2">'
    echo '        </testcase>'
    case "$1" in
      passed) echo '        <testcase name="parity::gpu_and_cpu" classname="whizbang::it" time="8.3"/>' ;;
      failed)
        echo '        <testcase name="parity::gpu_and_cpu" classname="whizbang::it" time="8.3">'
        echo '            <failure type="test failure with exit code 101">thread panicked</failure>'
        echo '        </testcase>'
        ;;
      timed-out)
        echo '        <testcase name="parity::gpu_and_cpu" classname="whizbang::it" time="600.0">'
        echo '            <failure type="test timeout">the test timed out</failure>'
        echo '        </testcase>'
        ;;
    esac
    echo '    </testsuite>'
    echo '</testsuites>'
  } >"$fix/junit.xml"
}

# --- The run records the installed binary hands out, as the binary stores them.
record() { jq -n -c "$2" >"$fix/records/$1.json"; }
base='def base($action; $status): {record: "run/5", identity: {run: "x", request: null, parent: null, whizbang: "0.1.0"},
  asked: {action: $action, version: "1.0.0", arguments: [], settings: [], config: "sha256:00"}, inputs: [], outputs: [],
  outcome: {status: $status, error: null, invalid: null, refusals: [], refusals_omitted: 0, lost: []}, time: {start_ms: 1, end_ms: 2, durations: []},
  models: [], spend: {reserved_micros: null, requests: []}, egress: [], steps: [], judge: [], diagnostics: []};
  def answered($capability; $backend; $origin): {models: [{capability: $capability, backend: $backend, model: "a-model", version: "1+fit1", device: null, cached: null, ended: "answered"}],
    spend: {reserved_micros: 50000, requests: [{request: 0, reserved_micros: 50000, settled_micros: 40000, unresolved_micros: 0, stage: "answered", over_bound: false}]},
    egress: [{origin: $origin, account: null, classes: ["source_images"], outcome: "sent"}]};
  def judged($backend; $origin; $decision): base("check"; "completed") + answered("judge"; $backend; $origin)
    + {judge: [{judge: "turn", request: 0, usage: {input_tokens: 900, output_tokens: 80, cache_read_tokens: 0, cache_write_tokens: 0}}, {judge: "verdict", decision: $decision, why: "a reason that is in no receipt"}]};'
records() {
  rm -f "$fix/records"/*.json
  record 100-1 "$base"'base("remove_prompt"; "completed") + {steps: [{step: "recipe", id: "segment", action: "segment", run: "100-2", status: "completed"}]}'
  record 100-2 "$base"'base("segment"; "completed") + {models: [{capability: "segment", backend: "pack", model: "sam3", version: "sha256:ee", device: "mps", cached: false, ended: "answered"}]}'
  record 100-9 "$base"'base("fill"; "completed") + answered("fill"; "fal"; "https://fal.run")'
  local provider origin_of
  for provider in anthropic google openai; do
    case "$provider" in
      anthropic) origin_of=https://api.anthropic.com ;;
      google) origin_of=https://generativelanguage.googleapis.com ;;
      openai) origin_of=https://api.openai.com ;;
    esac
    record "200-$provider" "$base"'base("source-fidelity"; "completed") + {steps: [{step: "recipe", id: "judge", action: "check", run: "201-'"$provider"'", status: "completed"}]}'
    record "201-$provider" "$base"'judged("'"$provider"'"; "'"$origin_of"'"; "pass")'
  done
  record 300-1 "$base"'base("remove_checked"; "completed") + {record: "run/6", plan: {sends: [], attempts: {asked: 3, allowed: 3, checks: true}, why: null},
    attempts: {list: [{attempt: 1, runs: ["300-2", "300-3"], verdict: "pass", unchecked: null, changed: null, settled_micros: 41200}], kept: 1, ended: "passed"}}'
  record 300-2 "$base"'base("remove_prompt"; "completed")'
  record 300-3 "$base"'base("source-fidelity"; "completed") + {steps: [{step: "recipe", id: "judge", action: "check", run: "300-4", status: "completed"}]}'
  record 300-4 "$base"'judged("anthropic"; "https://api.anthropic.com"; "pass")'
}

# --- The installed binary. What it does wrong is the word in $fix/kind.
stage="$tmp/stage/whizbang-0.1.0-aarch64-apple-darwin"
mkdir -p "$stage" "$fix/candidate"
cat >"$stage/whizbang" <<SH
#!/bin/bash
kind=\$(cat "$fix/kind")
[ "\$*" = --version ] && { cat "$fix/version-line"; exit 0; }
printf '%s|%s|%s\n' "\${GUARDS:-}" "\${WHIZBANG_THREAD_LIMIT:-}" "\$*" >>"$fix/commands"
printf '%s %s\n' "\$1" "\$2" >>"$fix/order"
env | sed 's/=.*//' | sort | tr '\n' ' ' >>"$fix/env"
echo >>"$fix/env"
# The leg's own folder, from the store or the config it names.
work=\$(printf '%s\n' "\$*" | sed -n -e 's/^.* --store \([^ ]*\)\/store.*$/\1/p' -e 's/^.* --config \([^ ]*\)\/config.toml.*$/\1/p' | sed -n '1p')
store=\$work/store
under="--config \$work/config.toml --env-file $tmp/keys.env --models-dir $tmp/models"
case "\$*" in
  "store import --picture \$work/picture.png --store \$store --create")
    head -c 8 "\$work/picture.png" | od -An -c | tr -d ' \n' | grep -F 'PNG' >/dev/null || { echo "whizbang: not a PNG" >&2; exit 1; }
    mkdir "\$store" && printf '%s\n' "created a store at \$store" "imported image $image"
    ;;
  "runs list --store \$store") [ "\$kind" = not-empty ] && echo "runs: 3" || echo "runs: 0" ;;
  "spend reset --store \$store --allowance 5.00") printf '%s\n' 'allowance reset: \$5.00' 'epoch: 1' 'generation: 2' ;;
  "setup --extra torch --models-dir $tmp/models")
    [ "\$kind" != setup-fails ] || { echo "whizbang: uv was not found" >&2; exit 1; }
    printf '%s\n' "the pack is installed in $tmp/models/pack" "extra: \$([ "\$kind" = other-extra ] && echo none || echo torch)"
    [ "\$kind" = no-hello ] || echo 'the worker it made said hello (lifetime 120 s): device cpu, 3 threads'
    ;;
  "backends status --probe \$under")
    # The config the leg wrote names SAM 3 and has no cache table.
    grep -x -F 'model = "sam3"' "\$work/config.toml" >/dev/null && ! grep -F '[cache]' "\$work/config.toml" >/dev/null || exit 3
    echo 'backends: 5'
    [ "\$kind" = no-probe ] && echo 'probe segment default: the pack'"'"'s worker said no hello: it ended' ||
      echo 'probe segment default: the pack'"'"'s worker said hello: pack/1, device cpu, 3 threads; it was stopped'
    ;;
  "rascal remove-prompt --image $image --prompt the red disc --until feather --store \$store \$under")
    printf '%s\n' 'run 100-1 completed' 'step segment segment run 100-2' 'step feather feather run 100-8' "mask $mask" ;;
  "rascal fill --image $image --mask $mask --store \$store \$under")
    [ "\$kind" != fill-fails ] || { echo "whizbang: the fill backend answered with an error" >&2; exit 1; }
    printf '%s\n' 'run 100-9 completed' "image $filled" "mask $mask" ;;
  "squint check source-fidelity --source $image --candidate $filled --judge "*" --brief The red disc was removed and the grey background was kept. --store \$store \$under")
    judge=\$(printf '%s\n' "\$*" | sed -n 's/^.* --judge \([a-z]*\) .*$/\1/p')
    printf '%s\n' "run 200-\$judge completed" 'verdict: pass' 'reason: a reason that is in no receipt' ;;
  "rascal remove-checked --image $image --prompt the red disc --check required --store \$store \$under")
    [ "\$kind" != no-loop ] || { echo "error: unrecognized subcommand 'remove-checked'" >&2; exit 2; }
    printf '%s\n' 'run 300-1 completed' "image $filled" ;;
  "runs show "*" --store \$store --json") cat "$fix/records/\$3.json" 2>/dev/null || { echo "whizbang: no such run" >&2; exit 1; } ;;
  *) echo "the stand-in was asked what it does not know: \$*" >&2; exit 64 ;;
esac
SH
chmod +x "$stage/whizbang"
mac=whizbang-0.1.0-aarch64-apple-darwin.tar.gz
linux=whizbang-0.1.0-x86_64-unknown-linux-gnu.tar.gz
(cd "$tmp/stage" && tar -czf "$fix/candidate/$mac" whizbang-0.1.0-aarch64-apple-darwin)
echo "the linux archive" >"$fix/candidate/$linux"
printf '%s  %s\n' "$(sha256 "$fix/candidate/$mac")" "$mac" "$(sha256 "$fix/candidate/$linux")" "$linux" >"$fix/candidate/SHA256SUMS"
cp "$fix/candidate/SHA256SUMS" "$fix/sums.kept"

# --- gh, uname and git. git is the real one, but a clone can be made dirty afterwards,
# which is the only way a new checkout of a tag is ever not clean.
real_git=$(command -v git)
cat >"$tmp/bin/gh" <<SH
#!/bin/bash
printf '%s\n' "\$*" >>"$fix/gh-asked"
pwd -P >>"$fix/gh-pwd"
case "\$1 \$2" in
  'release view') [ "\$3" = "$tag" ] && [ "\$4 \$5 \$6 \$7" = '--json assets --jq .assets[].name' ] || exit 64; cat "$fix/assets" ;;
  'release download') [ "\$3" = "$tag" ] && [ "\$4" = --dir ] || exit 1; mkdir -p "\$5" && cp "$fix/candidate"/* "\$5/" ;;
  'release upload') [ "\$3" = "$tag" ] && cp "\$4" "$fix/uploaded.json" ;;
  *) exit 64 ;;
esac
SH
printf '%s\n' '#!/bin/bash' 'case "$1" in -s) echo Darwin ;; -m) echo arm64 ;; *) exit 64 ;; esac' >"$tmp/bin/uname"
cat >"$tmp/bin/git" <<SH
#!/bin/bash
"$real_git" "\$@" || exit \$?
if [ "\$1" = clone ]; then
  for last; do :; done
  case "\$(cat "$fix/dirty")" in
    tracked) echo "changed" >>"\$last/tracked.txt" ;;
    staged) echo "changed" >>"\$last/tracked.txt"; "$real_git" -C "\$last" add tracked.txt ;;
    untracked) echo "added" >"\$last/scripts/added.sh" ;;
    ignored) mkdir -p "\$last/target" && echo "build output" >"\$last/target/left" ;;
  esac
fi
SH
chmod +x "$tmp/bin/gh" "$tmp/bin/uname" "$tmp/bin/git"

# leg [--no-attach]: the leg on the candidate, with everything good unless a case changed it.
reset() {
  rm -f "$fix/commands" "$fix/env" "$fix/gh-asked" "$fix/uploaded.json" "$fix/tests-ran" "$fix/heavy" "$fix/order" "$fix/gh-pwd"
  printf '%s\n' "$mac" "$linux" SHA256SUMS >"$fix/assets"
  rm -rf "$tmp/receipt"
  echo good >"$fix/kind"
  echo none >"$fix/dirty"
  echo 0 >"$fix/tests-exit"
  echo "whizbang 0.1.0 (commit $commit)" >"$fix/version-line"
  cp "$fix/sums.kept" "$fix/candidate/SHA256SUMS"
  junit passed
  records
}
# It is started in a folder that is no repository: what it asks of `gh` is asked from its own.
leg() { (cd "$tmp/models" && env PATH="$tmp/bin:$PATH" WHIZBANG_ENV_FILE="$tmp/keys.env" WHIZBANG_MODELS_DIR="$tmp/models" "$leg_script" "$@" "$tag" "$tmp/receipt"); }
receipt="$tmp/receipt/prerelease-receipt.json"
# of <name> <jq filter>: a value of one entry of the receipt.
of() { jq -c "(.checks[] | select(.name == \"$1\")) | $2" "$receipt"; }

# --- A leg that passes: the receipt is valid by its schema, holds only its fields, and is
# attached to the candidate.
reset
expect_pass "a leg that passes" leg
cp "$tmp/out" "$tmp/passed-out"
grep -F "prerelease-local: passed: the receipt is attached to the candidate $tag" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "it says it passed"; }
expect_pass "the receipt by its schema" scripts/receipt-check.sh "$receipt" "tag=$tag" "commit=$commit" \
  "mac=$(sha256 "$fix/candidate/$mac")" "linux=$(sha256 "$fix/candidate/$linux")" "installed=$(sha256 "$stage/whizbang")"
cmp -s "$receipt" "$fix/uploaded.json" && grep -E "^release upload $tag /.*/receipt/prerelease-receipt\.json$" "$fix/gh-asked" >/dev/null || fail "the receipt written is the receipt attached"
[ "$(jq -c 'keys' "$receipt")" = '["archives","binary","candidate","checks","commit","schema","store","tag","version"]' ] || fail "the receipt holds its fields and no other"
[ "$(jq -c '[.schema, .tag, .version, .candidate, .commit, .store]' "$receipt")" = "[\"prerelease-receipt/1\",\"$tag\",\"0.1.0\",2,\"$commit\",{\"new\":true,\"cache\":\"off\"}]" ] ||
  fail "the receipt names the candidate, its commit and the new store with the cache off"
[ "$(jq -c '[.checks[].name]' "$receipt")" = '["model-tests","setup","mask","fill","check-anthropic","check-google","check-openai","loop"]' ] || fail "the receipt holds each required check once"
[ "$(of model-tests '[.exit, .checkout.new, .checkout.clean, .checkout.commit, .expected, .ran, .failed, .timed_out]')" = "[0,true,true,\"$commit\",[\"model_tier::a_phrase\",\"parity::gpu_and_cpu\"],[\"model_tier::a_phrase\",\"parity::gpu_and_cpu\"],[],[]]" ] ||
  fail "the model tests' entry is read from the list and the report (it is: $(of model-tests .))"
[ "$(of model-tests .checkout.tree)" = "\"$(git -C "$origin" rev-parse 'HEAD^{tree}')\"" ] && [ "$(of model-tests '.report_sha256 | length')" = 64 ] || fail "the entry names the checkout's tree and the report's digest"
[ "$(of setup '[.exit, .extra, .hello, .probe_hello]')" = '[0,"torch",{"device":"cpu","threads":3},true]' ] || fail "setup's entry is read from what it printed and from the probe"
[ "$(of mask '[.run, .status, .backend, .model, .model_version, .device]')" = '["100-1","completed","pack","sam3","sha256:ee","mps"]' ] || fail "the mask's entry is read from the segment step's record"
[ "$(of fill '[.run, .status, .backend, .cache_hit, .requests, .sent_to, .tokens, .settled_micros]')" = '["100-9","completed","fal",false,[{"backend":"fal","ended":"answered","stage":"answered","settled_micros":40000}],["https://fal.run"],null,40000]' ] ||
  fail "the fill's entry holds its dispatched and settled request (it is: $(of fill .))"
[ "$(of check-google '[.run, .backend, .cache_hit, .requests[0].stage, .sent_to, .tokens, .verdict]')" = '["200-google","google",false,"answered",["https://generativelanguage.googleapis.com"],{"input":900,"output":80},"pass"]' ] ||
  fail "a check's entry is read from the record of its check step (it is: $(of check-google .))"
[ "$(of loop '[.run, .status, .plan_checks, .kept, .check.run, .check.status, .check.verdict, .check.cache_hit, .check.sent_to]')" = '["300-1","completed",true,{"attempt":1,"verdict":"pass","unchecked":null},"300-4","completed","pass",false,["https://api.anthropic.com"]]' ] ||
  fail "the loop's entry holds its plan, the kept attempt and that attempt's check (it is: $(of loop .))"
# No reply text, no path and no key.
if grep -E 'a reason that is in no receipt|made-up-key|/tmp/|/var/|/private/|/Users/|/home/' "$receipt" >/dev/null; then fail "the receipt holds a reply's text, a path or a key"; fi
if grep -r -F 'made-up-key' "$tmp/out" "$fix/commands" >/dev/null; then fail "the key reached an output or a command"; fi
# The model tests ran from the leg's own new checkout, under the heavy lock at 200.
[ "$(sed 's/ in .*checkout/ in checkout/' "$fix/tests-ran" | tr '\n' ';')" = "list in checkout under heavy, at 200;run in checkout under heavy, at 200;" ] || { cat "$fix/tests-ran" >&2; fail "the model tests are listed and then run from the new checkout, each under the heavy lock at 200: the list builds them"; }
# The candidate's pack is installed, and heard, before the model tests run on it; and the
# leg says that the pack of the models directory is replaced.
[ "$(sed -n '/^setup /,$p' "$fix/order" | sed -n '1,4p' | tr '\n' ';')" = "setup --extra;backends status;test-models list;test-models run;" ] ||
  { cat "$fix/order" >&2; fail "setup with the extra and its probe come before the model tests, which test the pack the candidate carries"; }
grep -F "the pack in the models directory is being replaced by the one the candidate carries" "$tmp/passed-out" >/dev/null || { cat "$tmp/passed-out" >&2; fail "the leg says that it replaces the pack"; }
[ "$(sort -u "$fix/gh-pwd")" = "$(cd "$tmp/main" && pwd -P)" ] || fail "gh is asked from the leg's own repository, wherever the leg was started"
grep -F "$tmp/main" "$fix/tests-ran" >/dev/null && fail "the model tests never run from the folder the leg was started in"
# Every command that starts the pack ran under the three guards at 200, and no other did.
[ "$(grep -c '^heavy,memory-watch,thread-guard,|200|' "$fix/commands")" = 4 ] &&
  [ "$(grep '^heavy,' "$fix/commands" | cut -d'|' -f3 | cut -d' ' -f1-2 | tr '\n' ';')" = "setup --extra;backends status;rascal remove-prompt;rascal remove-checked;" ] ||
  { cat "$fix/commands" >&2; fail "setup, the probe, the mask and the loop run under the heavy lock, the watch on memory and the thread guard at 200"; }
[ "$(tr '\n' ';' <"$fix/heavy")" = "heavy prerelease-setup;heavy prerelease-probe;heavy test-models-list;heavy test-models;heavy prerelease-mask;heavy prerelease-loop;" ] || fail "each is one heavy command"
grep -F -e '|rascal remove-checked ' "$fix/commands" | grep -F -e ' --check required ' >/dev/null || fail "the loop is asked with the check required"
# The binary is given three variables of whoever runs the leg, and the guards' own.
for held in $(sort -u "$fix/env" | tr ' ' '\n' | sort -u); do
  case "$held" in HOME | PATH | TMPDIR | GUARDS | WHIZBANG_THREAD_LIMIT | PWD | SHLVL | _ | OLDPWD) ;; *) fail "the installed binary is given the variable $held" ;; esac
done
# The config has no cache table, and names the env file only as an argument.
grep -F -e "--env-file $tmp/keys.env" "$fix/commands" >/dev/null || fail "the env file is handed to the binary by its name"

# --- Not attached when asked not to.
reset
expect_pass "a leg that attaches nothing" leg --no-attach
[ -s "$receipt" ] && [ ! -e "$fix/uploaded.json" ] || fail "--no-attach writes the receipt and attaches nothing"

# stopped <name> <expected text>: the leg stops, writes no receipt and attaches nothing.
stopped() {
  expect_fail "$1" "$2" leg
  [ ! -e "$receipt" ] && [ ! -e "$fix/uploaded.json" ] || fail "$1: a leg that stopped writes no receipt"
}
# --- A candidate that holds a receipt already: the leg stops before it spends, since the
# attaching would be refused at the end. With --no-attach it runs.
reset
echo prerelease-receipt.json >>"$fix/assets"
stopped "a candidate with a receipt" "the candidate $tag holds a receipt already"
[ ! -e "$fix/commands" ] && [ ! -e "$fix/tests-ran" ] || fail "a candidate with a receipt: nothing was run"
echo prerelease-receipt.json >>"$fix/assets"
expect_pass "a candidate with a receipt, with nothing to attach" leg --no-attach
# --- A wrong sum stops it before anything runs.
reset
printf '%s  %s\n' "$(printf '0%.0s' $(seq 1 64))" "$mac" "$(sha256 "$fix/candidate/$linux")" "$linux" >"$fix/candidate/SHA256SUMS"
stopped "a wrong sum" "the files of the candidate are not their SHA256SUMS; nothing was run"
[ ! -e "$fix/commands" ] && [ ! -e "$fix/tests-ran" ] || fail "a wrong sum: nothing was run"
# --- A binary whose version line names another commit, or another version.
reset
echo "whizbang 0.1.0 (commit $(printf '0%.0s' $(seq 1 40)))" >"$fix/version-line"
stopped "a binary of another commit" "nothing more was run"
[ ! -e "$fix/commands" ] && [ ! -e "$fix/tests-ran" ] || fail "a binary of another commit: nothing was run"
reset
echo "whizbang 0.1.1 (commit $commit)" >"$fix/version-line"
stopped "a binary of another version" "nothing more was run"
# --- A checkout whose HEAD is the tag's commit and which is not clean stops the leg
# before anything is built, listed or downloaded.
for how in tracked staged untracked; do
  reset
  echo "$how" >"$fix/dirty"
  stopped "a checkout with a file $how" "the new checkout of $tag is not clean"
  [ ! -e "$fix/tests-ran" ] && [ ! -e "$fix/commands" ] && [ ! -e "$fix/gh-asked" ] || fail "a checkout that is not clean: nothing is built, listed, downloaded or run"
done
# A file git ignores there is no build input the tag lacks.
reset
echo ignored >"$fix/dirty"
expect_pass "a checkout with an ignored file" leg
# --- A store that is not empty.
reset
echo not-empty >"$fix/kind"
stopped "a store that is not empty" "the store is not empty"
grep -F -e '|setup ' -e '|rascal ' -e '|squint ' "$fix/commands" >/dev/null && fail "a store that is not empty: no check is run in it"
# --- Another script than the candidate's own, a tag that is no candidate, no env file.
echo "# changed" >>"$tmp/main/scripts/receipt-check.sh"
reset
stopped "a script that is not the candidate's" "scripts/receipt-check.sh here is not the copy the candidate holds"
"$real_git" -C "$tmp/main" checkout -q -- scripts/receipt-check.sh
expect_fail "a version tag" "is not a candidate tag" env PATH="$tmp/bin:$PATH" WHIZBANG_ENV_FILE="$tmp/keys.env" WHIZBANG_MODELS_DIR="$tmp/models" "$leg_script" v0.1.0 "$tmp/receipt"
expect_fail "no env file" "WHIZBANG_ENV_FILE must name the env file" env PATH="$tmp/bin:$PATH" WHIZBANG_MODELS_DIR="$tmp/models" "$leg_script" "$tag" "$tmp/receipt"

# failed <name> <expected text>: the leg fails, the receipt is written as it failed, is
# still a receipt, and is not attached.
failed() {
  expect_fail "$1" "$2" leg
  cp "$tmp/out" "$tmp/leg-out"
  grep -F "prerelease-local: failed: the receipt records what a release is refused for" "$tmp/leg-out" >/dev/null || { cat "$tmp/leg-out" >&2; fail "$1: the leg says it failed"; }
  expect_pass "$1: still a receipt" scripts/receipt-check.sh --shape "$receipt"
  [ ! -e "$fix/uploaded.json" ] || fail "$1: a failed leg attaches nothing"
}
# --- A failed call is written as failed and fails the leg; what needed it is not asked.
reset
echo fill-fails >"$fix/kind"
failed "a fill that fails" "check: fill: its run did not complete"
[ "$(of fill '[.exit, .run, .status, .requests]')" = '[1,null,null,[]]' ] || fail "the failed fill is written as failed (it is: $(of fill .))"
[ "$(of check-openai '[.exit, .run, .verdict]')" = '[null,null,null]' ] || fail "a check with nothing to check is written as not run"
grep -F -e '|squint check' "$fix/commands" >/dev/null && fail "no check is asked without a fill"
[ "$(of mask .status)" = '"completed"' ] && [ "$(of loop .status)" = '"completed"' ] || fail "the other checks are still made and written"
# --- A model test that fails, or times out, or is left out; a run with no report.
reset
junit failed
echo 100 >"$fix/tests-exit"
failed "a model test that fails" "check: model-tests: the report holds a failed test"
[ "$(of model-tests '[.exit, .failed, .timed_out]')" = '[100,["parity::gpu_and_cpu"],[]]' ] || fail "the failed test is written from the report and the exit status (it is: $(of model-tests .))"
grep -F "check: model-tests: the test run's exit status is not 0" "$tmp/leg-out" >/dev/null || fail "the exit status is held too"
reset
junit timed-out
echo 100 >"$fix/tests-exit"
failed "a model test that times out" "check: model-tests: the report holds a timed-out test"
[ "$(of model-tests '[.failed, .timed_out]')" = '[[],["parity::gpu_and_cpu"]]' ] || fail "the timed-out test is written from the report"
reset
junit left-out
failed "a model test left out" "check: model-tests: the report lacks a test the filter names"
reset
rm "$fix/junit.xml"
failed "a run with no report" "check: model-tests: there is no test report"
junit passed
# --- A setup that fails, installs another extra or prints no hello; a probe that hears none.
reset
echo setup-fails >"$fix/kind"
failed "a setup that fails" "check: setup: its exit status is not 0"
[ "$(of setup '[.exit, .extra, .hello]')" = '[1,null,null]' ] || fail "the failed setup is written from its exit status and what it printed (it is: $(of setup .))"
for pair in "other-extra|check: setup: it did not install the extra torch" "no-hello|check: setup: it printed no hello of the worker" "no-probe|check: setup: the probe heard no hello"; do
  reset
  echo "${pair%%|*}" >"$fix/kind"
  failed "setup: ${pair%%|*}" "${pair#*|}"
done
# --- A check answered from the cache; a request that was not answered.
reset
record 201-google "$base"'judged("google"; "https://generativelanguage.googleapis.com"; "pass") + {models: [], spend: {reserved_micros: null, requests: []}, egress: []}
  | .asked.settings = [{name: "decision_cache", value: {text: "hit"}}, {name: "answered_by_run", value: {text: "9-9"}}]'
failed "a check answered from the cache" "check: check-google: it was answered from a cache"
grep -F "check: the decision cache was on" "$tmp/leg-out" >/dev/null && [ "$(jq -r .store.cache "$receipt")" = on ] || fail "a record that names the decision cache is written as the cache on"
reset
record 201-openai "$base"'judged("openai"; "https://api.openai.com"; "pass") | .models[0].ended = "timed_out" | .spend.requests[0].stage = "unresolved"'
failed "a request that was not answered" "check: check-openai: it names no request dispatched to https://api.openai.com and settled with an answer"
# --- The loop: unchecked, with a plan that did not check, or refused by a binary that has
# no such command, as every binary is until task L1.
reset
record 300-1 "$base"'base("remove_checked"; "completed") + {record: "run/6", plan: {sends: [], attempts: {asked: 1, allowed: 1, checks: false}, why: "no judge is ready"},
  attempts: {list: [{attempt: 1, runs: ["300-2"], verdict: null, unchecked: "no judge is ready", changed: null, settled_micros: 40000}], kept: 1, ended: "unchecked"}}'
failed "a loop that completed unchecked" "check: loop: it completed unchecked"
grep -F "check: loop: its plan did not check" "$tmp/leg-out" >/dev/null && grep -F "check: loop: its child check did not complete" "$tmp/leg-out" >/dev/null || fail "an unchecked loop fails on its plan and its child check too"
[ "$(of loop '[.plan_checks, .kept, .check]')" = '[false,{"attempt":1,"verdict":null,"unchecked":"unchecked"},null]' ] || fail "the unchecked loop is written as it was (it is: $(of loop .))"
reset
echo no-loop >"$fix/kind"
failed "a binary without the loop's command" "check: loop: its run did not complete"
[ "$(of loop '[.exit, .run, .check]')" = '[2,null,null]' ] || fail "a loop the binary refuses is written as failed"

finished
