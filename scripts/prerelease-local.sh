#!/usr/bin/env bash
# The local leg of the checks before a release (plan m4, task R2, items 3 and 4): the part
# that needs a real model or a provider's key. The orchestrator runs it on the owner's Mac,
# for a candidate the `prerelease` workflow has made; CI never runs it. Heavy, and it
# spends: one fill and four checks by a judge, well under one US dollar.
#
#   scripts/prerelease-local.sh [--no-attach] <candidate tag> <folder for the receipt>
#
# It needs, in the environment: WHIZBANG_ENV_FILE, the path of the env file that holds the
# providers' keys, which this script never opens and hands to the installed binary by
# name (`--env-file`); and WHIZBANG_MODELS_DIR, the models directory that holds SAM 3
# (`whizbang models get sam3`), as `just test-models` needs it. And on the path: git, gh,
# jq, perl, and the `uv` and the Python `whizbang setup` needs.
#
# It keeps two things apart, and the receipt names them so.
#
# 1. **The checkout's model tests.** `scripts/test-models.sh` builds and tests a checkout,
#    not the archive. They run from a checkout this script makes itself: a new clone of
#    the candidate's tag, from this repository's origin, in a temporary folder; never the
#    folder the script was started in. Before anything is built or listed that checkout is
#    held to be clean: its HEAD is the tag's commit, no tracked file differs from it, and
#    no file is there that git neither tracks nor ignores. One that is not clean stops the
#    leg, whatever its HEAD says. The scripts the leg runs from then on are that
#    checkout's, and this file must be that checkout's own copy, byte for byte: the leg of
#    a candidate is the candidate's leg. The tests the filter names are listed first
#    (`scripts/test-models.sh --list`), then run, and the run's report is read.
# 2. **The archive's acceptance.** Both archives and SHA256SUMS are downloaded from the
#    candidate and held to their sums; the Mac's is unpacked into a temporary folder; the
#    installed binary's digest is taken, and its `--version` must be the tag's base
#    version and the tag's commit, or nothing more runs. Then, with that binary alone, in
#    a store made new for this leg and under a config written here that has no `[cache]`
#    table, so that nothing is answered from earlier work: `whizbang setup --extra torch`
#    and a probe of the pack's worker, first of all, **which replaces the pack in the
#    models directory `WHIZBANG_MODELS_DIR` names with the one the candidate carries**
#    (and says so), so that the model tests of 1 and every check after load a model with
#    the candidate's pack and not with whichever was installed before; `whizbang rascal remove-prompt --until feather` on
#    SAM 3; one fill through fal; `whizbang squint check` by a judge of each of the three
#    providers; and `whizbang rascal remove-checked --check required`. Every command that
#    starts the pack runs under the heavy lock, the thread guard at 200 and the watch on
#    resident memory. The picture is synthetic, drawn here: a red disc on grey.
#
# The installed binary is given HOME, PATH and TMPDIR of whoever runs this and no other
# variable; it reads keys from the env file alone.
#
# It writes the receipt, `prerelease-receipt/1` (the top of scripts/prerelease-receipt.jq
# says its shape), as `<folder>/prerelease-receipt.json`. Every entry is taken from what a
# machine wrote: a run's record, a test report, an exit status, a line the binary printed.
# A check that failed is written as it failed, the receipt is still a receipt, and the leg
# fails. Only a receipt that the release gate's reader passes (scripts/receipt-check.sh)
# is attached to the candidate (`gh release upload`); `--no-attach` attaches nothing. A
# candidate that holds a receipt already stops the leg before anything is spent, unless
# `--no-attach` is given: a candidate is checked once, and the next is a new number.
#
# The loop's command, `remove-checked`, is plan m4's task L1. Until it is built the
# installed binary refuses the command, the entry `loop` records that, and the leg fails:
# no candidate passes without it. How the kept attempt's check is found in the record is
# marked `L1` below, for that task to confirm against the record it writes.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "prerelease-local: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

say() { echo "prerelease-local: $*"; }
stop() {
  echo "prerelease-local: stopped: $*" >&2
  exit 1
}
attach=1
if [ "${1:-}" = --no-attach ]; then
  attach=0
  shift
fi
[ $# -eq 2 ] || { echo "usage: scripts/prerelease-local.sh [--no-attach] <candidate tag> <folder for the receipt>" >&2; exit 2; }
tag=$1
out=$2
sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }

read_tag=$("$root/scripts/tag-version.sh" --candidate "$tag") || stop "$tag is not a candidate tag"
version=$(printf '%s\n' "$read_tag" | sed -n 's/^version: //p')
number=$(printf '%s\n' "$read_tag" | sed -n 's/^candidate: //p')
[ "$(uname -s) $(uname -m)" = "Darwin arm64" ] || stop "the local leg runs on a Mac with Apple silicon, where SAM 3 runs"
for tool in git gh jq perl shasum tar; do
  command -v "$tool" >/dev/null || stop "$tool is not installed"
done
env_file=${WHIZBANG_ENV_FILE:-}
case "$env_file" in /*) ;; *) stop "WHIZBANG_ENV_FILE must name the env file that holds the keys of the providers, by an absolute path" ;; esac
[ -f "$env_file" ] || stop "WHIZBANG_ENV_FILE names no file"
models=${WHIZBANG_MODELS_DIR:-}
case "$models" in /*) ;; *) stop "WHIZBANG_MODELS_DIR must name the models directory that holds SAM 3, by an absolute path" ;; esac
[ -d "$models/models" ] || stop "WHIZBANG_MODELS_DIR names no models directory with a model in it"
mkdir -p "$out"
out=$(cd "$out" && pwd -P)
rm -f "$out/prerelease-receipt.json"
# Everything from here is asked from this repository, wherever the leg was started: `gh`
# acts on the repository of the folder it is run in.
cd "$root"

work=$(mktemp -d)
work=$(cd "$work" && pwd -P)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$work"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "prerelease-local: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

# --- The checkout: new, of the candidate's tag, and clean.
origin=$(git -C "$root" remote get-url origin) || stop "this repository names no origin to take the commit of the candidate from"
checkout=$work/checkout
git clone --quiet --depth 1 --branch "$tag" -c advice.detachedHead=false "$origin" "$checkout" || stop "the tag $tag could not be cloned from origin"
commit=$(git -C "$checkout" rev-parse --verify "refs/tags/$tag^{commit}") || stop "the clone does not hold the tag $tag"
[ "$(git -C "$checkout" rev-parse --verify 'HEAD^{commit}')" = "$commit" ] || stop "the HEAD of the new checkout is not the commit of $tag"
# No tracked file differs, staged or not; and nothing is there that git neither tracks nor
# ignores, by the checkout's own rules and no rule of this machine's.
dirty=$(git -C "$checkout" -c core.excludesFile=/dev/null status --porcelain --untracked-files=all) || stop "the new checkout could not be read"
[ -z "$dirty" ] && git -C "$checkout" diff --quiet HEAD -- ||
  stop "the new checkout of $tag is not clean: a changed or added file is a build input the tag does not hold ($(printf '%s\n' "$dirty" | wc -l | tr -d ' ') paths)"
tree=$(git -C "$checkout" rev-parse --verify 'HEAD^{tree}')
for own in prerelease-local.sh tag-version.sh release-publish.sh receipt-check.sh prerelease-receipt.jq; do
  cmp -s "$root/scripts/$own" "$checkout/scripts/$own" ||
    stop "scripts/$own here is not the copy the candidate holds: run the leg from a checkout of $tag"
done
scripts=$checkout/scripts
say "a new checkout of $tag at $commit is clean"

# A candidate that holds a receipt already cannot be given another: attaching replaces
# nothing. Seen now, before anything is spent, and not at the end.
if [ "$attach" = 1 ]; then
  there=$(gh release view "$tag" --json assets --jq '.assets[].name') || stop "the candidate $tag could not be read on GitHub"
  if printf '%s\n' "$there" | grep -x -F 'prerelease-receipt.json' >/dev/null; then
    stop "the candidate $tag holds a receipt already; nothing was run. A candidate is checked once: tag the next number, or pass --no-attach to run the leg and attach nothing"
  fi
fi

# --- The candidate's archives, held to their sums; the Mac's installed; the binary held
# to the tag before anything runs.
gh release download "$tag" --dir "$work/candidate" --pattern "whizbang-$version-*.tar.gz" --pattern SHA256SUMS ||
  stop "the archives of the candidate $tag could not be downloaded"
"$scripts/release-publish.sh" check "$work/candidate" "$version" >/dev/null || stop "the files of the candidate are not their SHA256SUMS; nothing was run"
name=whizbang-$version-aarch64-apple-darwin
mac_sum=$(sha256 "$work/candidate/$name.tar.gz")
linux_sum=$(sha256 "$work/candidate/whizbang-$version-x86_64-unknown-linux-gnu.tar.gz")
mkdir "$work/installed" "$work/bin" "$work/tmp" "$work/records"
# The record of a run that was never made.
echo null >"$work/records/none.json"
tar -xzf "$work/candidate/$name.tar.gz" -C "$work/installed"
[ -f "$work/installed/$name/whizbang" ] && [ ! -L "$work/installed/$name/whizbang" ] || stop "the archive for the Mac holds no binary"
cp "$work/installed/$name/whizbang" "$work/bin/whizbang"
binary=$work/bin/whizbang
binary_sum=$(sha256 "$binary")
cleared=(HOME="$HOME" PATH="$PATH" TMPDIR="$work/tmp")
version_line=$(env -i "${cleared[@]}" "$binary" --version 2>/dev/null) || stop "the installed binary printed no version line"
[ "$version_line" = "whizbang $version (commit $commit)" ] ||
  stop "the installed binary says '$version_line', and the candidate is whizbang $version (commit $commit); nothing more was run"
say "the installed binary is $version_line"

# --- The archive's acceptance. The config: SAM 3 on the pack, fal, a judge of each
# provider, and no `[cache]` table, so the decision cache is off.
store=$work/store
config=$work/config.toml
cat >"$config" <<'TOML'
[backends.segment]
kind = "pack"
model = "sam3"

[backends.fill]
kind = "service"
provider = "fal"
model = "bria-eraser"
key_env = "FAL_KEY"

[backends.judge]
default = "anthropic"
[backends.judge.anthropic]
kind = "service"
provider = "anthropic"
model = "claude-haiku-5-5"
key_env = "ANTHROPIC_API_KEY"
[backends.judge.google]
kind = "service"
provider = "google"
model = "gemini-3.8-flash"
key_env = "GEMINI_API_KEY"
[backends.judge.openai]
kind = "service"
provider = "openai"
model = "gpt-6.1-terra"
key_env = "OPENAI_API_KEY"

[spend]
call_cap_usd = 2.00

[egress.recipients.fal]
origin = "https://fal.run"
classes = ["source_images", "derived_images"]
[egress.recipients.anthropic]
origin = "https://api.anthropic.com"
classes = ["source_images", "derived_images", "prompts", "evidence"]
[egress.recipients.google]
origin = "https://generativelanguage.googleapis.com"
classes = ["source_images", "derived_images", "prompts", "evidence"]
[egress.recipients.openai]
origin = "https://api.openai.com"
classes = ["source_images", "derived_images", "prompts", "evidence"]
TOML
under=(--config "$config" --env-file "$env_file" --models-dir "$models")
# plain <arguments...>: the installed binary, with the cleared environment.
plain() { env -i "${cleared[@]}" "$binary" "$@"; }
# guarded <label> <arguments...>: the same for a command that starts the pack, under the
# heavy lock, the thread guard at 200 and the watch on resident memory.
guarded() {
  local label=$1
  shift
  env -i "${cleared[@]}" WHIZBANG_THREAD_LIMIT=200 "$scripts/heavy.sh" "prerelease-$label" "$scripts/memory-watch.sh" "$scripts/thread-guard.sh" "$binary" "$@"
}
# called <name> <plain|guarded> <arguments...>: one command; its output in $work/out and
# its status in $status. A failure is said and does not end the leg here.
called() {
  local what=$1 how=$2
  shift 2
  status=0
  if [ "$how" = guarded ]; then
    guarded "$what" "$@" >"$work/out" 2>"$work/err" || status=$?
  else
    plain "$@" >"$work/out" 2>"$work/err" || status=$?
  fi
  [ "$status" -eq 0 ] || { say "$what failed with status $status:"; sed 's/^/    /' "$work/err" >&2; }
}
# record <run>: the run's record, as it is stored, in $work/records/<run>.json. A run that
# cannot be read leaves `null` there.
record() {
  plain runs show "$1" --store "$store" --json >"$work/records/$1.json" 2>/dev/null && jq -e . "$work/records/$1.json" >/dev/null 2>&1 ||
    echo null >"$work/records/$1.json"
}
run_of() { sed -n 's/^run \([A-Za-z0-9_-]*\) completed$/\1/p' "$work/out" | sed -n '1p'; }
# What a paid request's record shows, for one capability: who answered, whether a cache
# did, each request's dispatch and settlement, where it was sent, and the tokens.
paid='def paid($capability):
  if . == null then {backend: null, model: null, model_version: null, cache_hit: null, requests: [], sent_to: [], tokens: null, settled_micros: null}
  else . as $record | ([.models[] | select(.capability == $capability)] | first) as $model
  | {backend: $model.backend, model: $model.model, model_version: $model.version,
     cache_hit: (([.asked.settings[]? | select(.name == "decision_cache") | .value.text] | any(.[]; . == "hit")) or any(.models[]; .cached == true)),
     requests: [.spend.requests[]? | {backend: ($record.models[.request].backend // "unknown"), ended: ($record.models[.request].ended // "unknown"), stage: .stage, settled_micros: (.settled_micros // 0)}],
     sent_to: ([.egress[]? | select(.outcome == "sent") | .origin] | unique),
     tokens: ([.judge[]? | select(.judge == "turn") | .usage | select(. != null)] | if length == 0 then null else {input: (map(.input_tokens // 0) | add), output: (map(.output_tokens // 0) | add)} end),
     settled_micros: ([.spend.requests[]?.settled_micros // 0] | add // 0)} end;
  def status_of: if . == null then null else .outcome.status end;
  def verdict_of: if . == null then null else ([.judge[]? | select(.judge == "verdict") | .decision] | first // null) end;'

# The picture: 256 by 256, grey, with a red disc. A PNG written here, byte by byte.
perl -MCompress::Zlib -e '
  my ($w, $h) = (256, 256); my $raw = "";
  for my $y (0 .. $h - 1) { $raw .= "\0"; for my $x (0 .. $w - 1) {
    $raw .= (($x - 128) ** 2 + ($y - 128) ** 2 <= 48 ** 2) ? "\xd0\x20\x20" : "\x80\x80\x80" } }
  sub chunk { my ($t, $d) = @_; pack("N", length $d) . $t . $d . pack("N", crc32($t . $d)) }
  binmode STDOUT;
  print "\x89PNG\r\n\x1a\n", chunk("IHDR", pack("NNCCCCC", $w, $h, 8, 2, 0, 0, 0)), chunk("IDAT", compress($raw)), chunk("IEND", "");' >"$work/picture.png" ||
  stop "the picture could not be drawn"

# The store: made by this leg, and holding no run before the first of them.
called import plain store import --picture "$work/picture.png" --store "$store" --create
[ "$status" -eq 0 ] || stop "the picture could not be imported into a new store"
grep -F "created a store at " "$work/out" >/dev/null || stop "the store was not made by this leg: it was there already"
image=$(sed -n 's/^imported image \(sha256:[0-9a-f]\{64\}\)$/\1/p' "$work/out")
[ -n "$image" ] || stop "the import printed no image"
[ "$(plain runs list --store "$store" 2>/dev/null)" = "runs: 0" ] || stop "the store is not empty: it holds runs before the leg made any"
called allowance plain spend reset --store "$store" --allowance 5.00
[ "$status" -eq 0 ] || stop "the new store was given no allowance"
say "a new store holds the picture and no run"

# setup with the extra, and the probe that hears the installed pack's worker again. It
# comes before every check that starts the pack, the checkout's model tests among them:
# it replaces the pack of the models directory with the one the installed binary carries.
say "the pack in the models directory is being replaced by the one the candidate carries (whizbang setup --extra torch)"
called setup guarded setup --extra torch --models-dir "$models"
setup_exit=$status
extra=$(sed -n 's/^extra: \([a-z0-9_-]*\)$/\1/p' "$work/out" | sed -n '1p')
hello=$(sed -n 's/^the worker it made said hello (lifetime [0-9]* s): device \([a-z0-9:_-]*\), \([0-9]*\) threads$/{"device":"\1","threads":\2}/p' "$work/out" | sed -n '1p')
called probe guarded backends status --probe "${under[@]}"
probe=false
if [ "$status" -eq 0 ] && grep -E "^probe segment [a-z0-9_-]+: the pack.s worker said hello: " "$work/out" >/dev/null; then probe=true; fi
jq -n -c --argjson exit "$setup_exit" --arg extra "$extra" --argjson hello "${hello:-null}" --argjson probe "$probe" \
  '{name: "setup", exit: $exit, extra: (if $extra == "" then null else $extra end), hello: $hello, probe_hello: $probe}' >"$work/entry-setup"

# --- The checkout's model tests: listed, run, and read from the run's report. They come
# after `setup`, so that the pack they load a model with is the one the candidate carries.
expected='[]' ran='[]' failed='[]' timed_out='[]' report_sum=null tests_exit=null
# The list builds the tests, so it is a heavy command too.
if (cd "$checkout" && env -u CARGO_TARGET_DIR WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh test-models-list scripts/test-models.sh --list) >"$work/listed" 2>"$work/list.err"; then
  expected=$(awk 'NF == 2 { print $2 }' "$work/listed" | jq -R -s -c 'split("\n") | map(select(. != "")) | sort')
  report=$checkout/target/nextest/model-tier/junit.xml
  rm -f "$report"
  tests_exit=0
  (cd "$checkout" && env -u CARGO_TARGET_DIR WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh test-models scripts/test-models.sh) >&2 || tests_exit=$?
  if [ -s "$report" ]; then
    report_sum=\"$(sha256 "$report")\"
    # One line for each test the report holds: ran, failed or timed_out, and its name.
    awk '
      function named(line) { return match(line, / name="[^"]*"/) ? substr(line, RSTART + 7, RLENGTH - 8) : "" }
      /<testcase / { test = named($0); if (test != "") print "ran " test }
      test != "" && (/<failure/ || /<error/) { print ((tolower($0) ~ /time(d)?[ -]?out/) ? "timed_out " : "failed ") test }
      /<\/testcase>/ { test = "" }' "$report" >"$work/report-lines"
    listed_as() { { grep "^$1 " "$work/report-lines" || true; } | cut -d' ' -f2- | jq -R -s -c 'split("\n") | map(select(. != "")) | unique'; }
    ran=$({ grep '^ran ' "$work/report-lines" || true; } | cut -d' ' -f2- | jq -R -s -c 'split("\n") | map(select(. != "")) | sort')
    failed=$(listed_as failed)
    timed_out=$(listed_as timed_out)
  fi
else
  sed 's/^/    /' "$work/list.err" >&2
  say "the model tests could not be listed"
fi
jq -n -c --argjson exit "$tests_exit" --arg commit "$commit" --arg tree "$tree" --argjson expected "$expected" --argjson ran "$ran" \
  --argjson failed "$failed" --argjson timed_out "$timed_out" --argjson report "$report_sum" \
  '{name: "model-tests", exit: $exit, checkout: {new: true, clean: true, commit: $commit, tree: $tree},
    expected: $expected, ran: $ran, failed: $failed, timed_out: $timed_out, report_sha256: $report}' >"$work/entry-model-tests"
say "the model tests ended with status $tests_exit"

# The mask, on SAM 3: the recipe to its last mask step, and the record of its segment step.
called mask guarded rascal remove-prompt --image "$image" --prompt "the red disc" --until feather --store "$store" "${under[@]}"
mask_exit=$status
mask_run=$(run_of)
mask=$(sed -n 's/^mask \(sha256:[0-9a-f]\{64\}\)$/\1/p' "$work/out" | sed -n '1p')
segment_run=$(sed -n 's/^step segment segment run \([A-Za-z0-9_-]*\)$/\1/p' "$work/out" | sed -n '1p')
record "${mask_run:-none}"
record "${segment_run:-none}"
jq -n -c "$paid"' {name: "mask", exit: $exit, run: (if $run == "" then null else $run end), status: ($top[0] | status_of)}
  + ($step[0] | if . == null then {backend: null, model: null, model_version: null, device: null}
     else ([.models[] | select(.capability == "segment")] | first) as $m | {backend: $m.backend, model: $m.model, model_version: $m.version, device: $m.device} end)' \
  --argjson exit "$mask_exit" --arg run "$mask_run" --slurpfile top "$work/records/${mask_run:-none}.json" --slurpfile step "$work/records/${segment_run:-none}.json" >"$work/entry-mask"

# paid_entry <name> <capability> <exit, or null> <run> <record of the paid request's run> <more, as JSON>
paid_entry() {
  record "${4:-none}"
  [ "$5" = "${4:-none}" ] || record "$5"
  jq -n -c "$paid"' {name: $name, exit: $exit, run: (if $run == "" then null else $run end), status: ($top[0] | status_of)} + ($asked[0] | paid($capability)) + $more' \
    --arg name "$1" --arg capability "$2" --argjson exit "$3" --arg run "$4" --slurpfile top "$work/records/${4:-none}.json" \
    --slurpfile asked "$work/records/$5.json" --argjson more "$6" >"$work/entry-$1"
}

# One fill through fal, of the mask SAM 3 made.
filled=
if [ -n "$mask" ]; then
  called fill plain rascal fill --image "$image" --mask "$mask" --store "$store" "${under[@]}"
  fill_run=$(run_of)
  filled=$(sed -n 's/^image \(sha256:[0-9a-f]\{64\}\)$/\1/p' "$work/out" | sed -n '1p')
  paid_entry fill fill "$status" "$fill_run" "${fill_run:-none}" '{}'
else
  say "no fill was asked: there is no mask to fill"
  paid_entry fill fill null "" none '{}'
fi

# One check by a judge of each provider: the picture against its fill. The judged request
# is in the record of the recipe's `check` step.
for provider in anthropic google openai; do
  if [ -n "$filled" ]; then
    called "check-$provider" plain squint check source-fidelity --source "$image" --candidate "$filled" --judge "$provider" \
      --brief "The red disc was removed and the grey background was kept." --store "$store" "${under[@]}"
    check_run=$(run_of)
    verdict=$(sed -n -E 's/^verdict: (pass|fail|unsure)$/\1/p' "$work/out" | sed -n '1p')
    record "${check_run:-none}"
    judged=$(jq -r 'if . == null then "none" else ([.steps[]? | select(.action == "check") | .run] | first // "none") end' "$work/records/${check_run:-none}.json")
    paid_entry "check-$provider" judge "$status" "$check_run" "$judged" "$(jq -n -c --arg verdict "$verdict" '{verdict: (if $verdict == "" then null else $verdict end)}')"
  else
    say "no check by $provider was asked: there is no fill to check"
    paid_entry "check-$provider" judge null "" none '{"verdict": null}'
  fi
done

# The loop, with the check required.
called loop guarded rascal remove-checked --image "$image" --prompt "the red disc" --check required --store "$store" "${under[@]}"
loop_exit=$status
loop_run=$(run_of)
record "${loop_run:-none}"
# L1: the kept attempt is the entry of `attempts.list` whose `attempt` is `attempts.kept`;
# its check is the run among that attempt's `runs` whose action is `source-fidelity`, and
# the judged request is in the record of that run's `check` step, as for a check above.
loop_check=none
judged=none
for attempt_run in $(jq -r 'if . == null or .attempts == null or .attempts.kept == null then empty else .attempts.kept as $kept | .attempts.list[] | select(.attempt == $kept) | .runs[] end' "$work/records/${loop_run:-none}.json" | grep -x -E '[A-Za-z0-9_-]+' | sed -n '1,32p' || true); do
  record "$attempt_run"
  if [ "$(jq -r 'if . == null then "" else .asked.action end' "$work/records/$attempt_run.json")" = source-fidelity ]; then
    loop_check=$attempt_run
    judged=$(jq -r '[.steps[]? | select(.action == "check") | .run] | first // "none"' "$work/records/$attempt_run.json")
    record "$judged"
    break
  fi
done
jq -n -c "$paid"' ($top[0]) as $loop
  | {name: "loop", exit: $exit, run: (if $run == "" then null else $run end), status: ($loop | status_of),
     plan_checks: (if $loop == null or $loop.plan == null or $loop.plan.attempts == null then null else $loop.plan.attempts.checks end),
     kept: (if $loop == null or $loop.attempts == null or $loop.attempts.kept == null then null
            else ($loop.attempts.kept as $kept | [$loop.attempts.list[] | select(.attempt == $kept)] | first
                  | if . == null then null else {attempt: .attempt, verdict: .verdict, unchecked: (if .unchecked == null then null else "unchecked" end)} end) end),
     check: (if $check == "none" or $asked[0] == null then null
             else ($asked[0] | paid("judge")) as $p | {run: $judged, status: ($asked[0] | status_of), verdict: ($asked[0] | verdict_of),
                   cache_hit: $p.cache_hit, requests: $p.requests, sent_to: $p.sent_to} end)}' \
  --argjson exit "$loop_exit" --arg run "$loop_run" --arg check "$loop_check" --arg judged "$judged" \
  --slurpfile top "$work/records/${loop_run:-none}.json" --slurpfile asked "$work/records/$judged.json" >"$work/entry-loop"

# Whether any record names the decision cache at all: with the cache off, none does.
cache=off
for kept in "$work"/records/*.json; do
  if [ "$(jq -r 'if . == null then 0 else [.asked.settings[]? | select(.name == "decision_cache")] | length end' "$kept")" != 0 ]; then cache=on; fi
done

# --- The receipt.
jq -n --arg tag "$tag" --arg version "$version" --argjson candidate "$number" --arg commit "$commit" --arg mac "$mac_sum" --arg linux "$linux_sum" \
  --arg binary "$binary_sum" --arg line "$version_line" --arg cache "$cache" \
  --slurpfile checks <(cat "$work/entry-model-tests" "$work/entry-setup" "$work/entry-mask" "$work/entry-fill" \
    "$work/entry-check-anthropic" "$work/entry-check-google" "$work/entry-check-openai" "$work/entry-loop") \
  '{schema: "prerelease-receipt/1", tag: $tag, version: $version, candidate: $candidate, commit: $commit,
    archives: {"aarch64-apple-darwin": $mac, "x86_64-unknown-linux-gnu": $linux},
    binary: {sha256: $binary, version_line: $line}, store: {new: true, cache: $cache}, checks: $checks}' >"$work/receipt.json"
"$scripts/receipt-check.sh" --shape "$work/receipt.json" >/dev/null ||
  stop "the receipt this leg wrote is not of its schema; nothing was written (a fault of this script or of a record it read)"
cp "$work/receipt.json" "$out/prerelease-receipt.json"
say "wrote $out/prerelease-receipt.json"
if ! "$scripts/receipt-check.sh" "$out/prerelease-receipt.json" "tag=$tag" "commit=$commit" "tree=$tree" "mac=$mac_sum" "linux=$linux_sum" "installed=$binary_sum"; then
  echo "prerelease-local: failed: the receipt records what a release is refused for (above); it was not attached to $tag" >&2
  exit 1
fi
if [ "$attach" = 1 ]; then
  gh release upload "$tag" "$out/prerelease-receipt.json" || stop "the receipt could not be attached to the candidate $tag"
  say "passed: the receipt is attached to the candidate $tag"
else
  say "passed: the receipt was not attached to $tag (--no-attach)"
fi
done_ok=1
exit 0
}
main "$@"; exit
