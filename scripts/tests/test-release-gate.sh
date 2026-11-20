#!/usr/bin/env bash
# The gate of a release (scripts/release-gate.sh, scripts/receipt-check.sh and the schema,
# scripts/prerelease-receipt.jq; plan m4, task R2, items 4 and 5): it passes when the
# candidate of the tag's commit passed on GitHub and on the owner's Mac, and it refuses
# each thing a release must never be made on.
#
# Everything is a stand-in: a throwaway repository with tags, a `gh` that answers from a
# folder, archives of a few bytes and a receipt written here. Nothing is contacted, built
# or published, and the real scripts are run as they are.
source "$(dirname "$0")/lib.sh"

gate=$root/scripts/release-gate.sh
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
mac=whizbang-0.1.0-aarch64-apple-darwin.tar.gz
linux=whizbang-0.1.0-x86_64-unknown-linux-gnu.tar.gz

# --- The repository: one commit with the candidates rc.1 and rc.2 and the version tag,
# and an earlier commit with a candidate of its own version.
repo="$tmp/repo"
git init -q "$repo"
commit_as() { git -C "$repo" -c user.name=test -c user.email=test@example.invalid commit -q --allow-empty -m "$1"; }
commit_as "an earlier commit"
earlier=$(git -C "$repo" rev-parse HEAD)
git -C "$repo" tag v0.0.9-rc.1
commit_as "the candidate's commit"
commit=$(git -C "$repo" rev-parse HEAD)
git -C "$repo" tag v0.1.0-rc.1
git -C "$repo" tag v0.1.0-rc.2
git -C "$repo" tag v0.1.0
# A tag that only looks like a candidate of the version is no candidate.
git -C "$repo" tag v0.1.0-rc.9x

# --- The candidate's files: two archives, the Mac's with a binary inside, and SHA256SUMS.
files="$tmp/github/release/v0.1.0-rc.2"
mkdir -p "$files" "$tmp/stage/whizbang-0.1.0-aarch64-apple-darwin"
echo "the binary" >"$tmp/stage/whizbang-0.1.0-aarch64-apple-darwin/whizbang"
(cd "$tmp/stage" && tar -czf "$files/$mac" whizbang-0.1.0-aarch64-apple-darwin)
echo "the linux archive" >"$files/$linux"
printf '%s  %s\n' "$(sha256 "$files/$mac")" "$mac" "$(sha256 "$files/$linux")" "$linux" >"$files/SHA256SUMS"
installed=$(sha256 "$tmp/stage/whizbang-0.1.0-aarch64-apple-darwin/whizbang")

# --- The receipt of a leg that passed.
paid() { # <name> <backend> <origin> <more fields, as JSON>
  jq -n -c --arg name "$1" --arg backend "$2" --arg origin "$3" --argjson more "$4" \
    '{name: $name, exit: 0, run: "100-10", status: "completed", backend: $backend, model: "a-model", model_version: "1+fit1", cache_hit: false,
      requests: [{backend: $backend, ended: "answered", stage: "answered", settled_micros: 1200}], sent_to: [$origin],
      tokens: {input: 900, output: 80}, settled_micros: 1200} + $more'
}
tree=$(git -C "$repo" rev-parse "$commit^{tree}")
jq -n --arg commit "$commit" --arg tree "$tree" --arg mac "$(sha256 "$files/$mac")" --arg linux "$(sha256 "$files/$linux")" --arg installed "$installed" \
  --argjson fill "$(paid fill fal https://fal.run '{"tokens": null}')" \
  --argjson anthropic "$(paid check-anthropic anthropic https://api.anthropic.com '{"verdict": "pass"}')" \
  --argjson google "$(paid check-google google https://generativelanguage.googleapis.com '{"verdict": "unsure"}')" \
  --argjson openai "$(paid check-openai openai https://api.openai.com '{"verdict": "fail"}')" \
  '{schema: "prerelease-receipt/1", tag: "v0.1.0-rc.2", version: "0.1.0", candidate: 2, commit: $commit,
    archives: {"aarch64-apple-darwin": $mac, "x86_64-unknown-linux-gnu": $linux},
    binary: {sha256: $installed, version_line: "whizbang 0.1.0 (commit \($commit))"},
    store: {new: true, cache: "off"},
    checks: [
      {name: "model-tests", exit: 0, checkout: {new: true, clean: true, commit: $commit, tree: $tree},
       expected: ["model_tier::a_phrase", "parity::gpu_and_cpu"], ran: ["parity::gpu_and_cpu", "model_tier::a_phrase"], failed: [], timed_out: [],
       report_sha256: ("d" * 64)},
      {name: "setup", exit: 0, extra: "torch", hello: {device: "cpu", threads: 3}, probe_hello: true},
      {name: "mask", exit: 0, run: "100-1", status: "completed", backend: "pack", model: "sam3", model_version: "sha256:ee", device: "mps"},
      $fill, $anthropic, $google, $openai,
      {name: "loop", exit: 0, run: "100-20", status: "completed", plan_checks: true, kept: {attempt: 1, verdict: "fail", unchecked: null},
       check: {run: "100-25", status: "completed", verdict: "fail", cache_hit: false,
               requests: [{backend: "google", ended: "answered", stage: "answered", settled_micros: 900}], sent_to: ["https://generativelanguage.googleapis.com"]}}
    ]}' >"$tmp/good.json"
cp "$tmp/good.json" "$files/prerelease-receipt.json"
echo '[{"conclusion":"success","event":"push","headBranch":"v0.1.0-rc.2","headSha":"'"$commit"'","status":"completed"}]' >"$tmp/github/runs.json"

# --- `gh`: the runs from a file; a candidate's files from its folder.
mkdir "$tmp/bin"
cat >"$tmp/bin/gh" <<SH
#!/bin/bash
printf '%s\n' "\$*" >>"$tmp/gh-asked"
case "\$1 \$2" in
  'run list') [ ! -e "$tmp/github/runs-fail" ] || exit 1; cat "$tmp/github/runs.json" ;;
  'release download')
    [ "\$4" = --dir ] && [ -d "$tmp/github/release/\$3" ] || { echo "release not found" >&2; exit 1; }
    mkdir -p "\$5" && cp "$tmp/github/release/\$3"/* "\$5/"
    ;;
  *) echo "the stand-in gh was asked what it does not know: \$*" >&2; exit 64 ;;
esac
SH
chmod +x "$tmp/bin/gh"
gated() { rm -rf "$tmp/out-folder"; (cd "$repo" && env PATH="$tmp/bin:$PATH" "$gate" "${1:-v0.1.0}" "$tmp/out-folder"); }

# --- It passes when all agree, and leaves the candidate's own files.
expect_pass "a candidate that passed" gated
grep -F "release-gate: passed: v0.1.0 is of commit $commit, whose candidate v0.1.0-rc.2 passed" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "it says what passed"; }
[ "$(LC_ALL=C ls -A "$tmp/out-folder" | tr '\n' ' ')" = "SHA256SUMS prerelease-receipt.json $mac $linux " ] || fail "the folder holds the candidate's files"
cmp -s "$tmp/out-folder/$mac" "$files/$mac" || fail "the files are the candidate's, byte for byte"
grep -x -F "run list --workflow prerelease.yml --commit $commit --limit 100 --json conclusion,event,headBranch,headSha,status" "$tmp/gh-asked" >/dev/null &&
  grep -x -F "release download v0.1.0-rc.2 --dir $tmp/out-folder" "$tmp/gh-asked" >/dev/null || { cat "$tmp/gh-asked" >&2; fail "it asks for the runs of the commit and the files of the latest candidate"; }
if grep -E '^release (create|upload|edit|delete)' "$tmp/gh-asked" >/dev/null; then fail "the gate publishes nothing"; fi

# refused <name> <expected text> [<tag>]: the gate refuses, and leaves no folder to publish from.
refused() {
  expect_fail "$1" "$2" gated "${3:-v0.1.0}"
  grep -F "release-gate: refused:" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "$1: it says it refused"; }
  [ ! -e "$tmp/out-folder" ] || fail "$1: a refused gate leaves no folder"
}
# gate_receipt <name> <expected text> <jq filter>: the good receipt changed that way, on the
# candidate, is refused by the gate.
gate_receipt() {
  jq "$3" "$tmp/good.json" >"$files/prerelease-receipt.json"
  cmp -s "$tmp/good.json" "$files/prerelease-receipt.json" && fail "$1: the filter changed nothing"
  refused "$1" "$2"
  cp "$tmp/good.json" "$files/prerelease-receipt.json"
}
# receipt <name> <expected text> <jq filter>: the same, asked of the reader the gate hands
# the receipt to, with the values the gate holds it to. The gate refuses whatever that
# reader refuses, which the cases through `gate_receipt` show for each kind; the many
# cases of one kind are asked here, where each costs one jq and no archive.
held=("tag=v0.1.0-rc.2" "commit=$commit" "tree=$tree" "mac=$(sha256 "$files/$mac")" "linux=$(sha256 "$files/$linux")" "installed=$installed")
expect_pass "the reader on the good receipt" "$root/scripts/receipt-check.sh" "$tmp/good.json" "${held[@]}"
receipt() {
  jq "$3" "$tmp/good.json" >"$tmp/changed.json"
  cmp -s "$tmp/good.json" "$tmp/changed.json" && fail "$1: the filter changed nothing"
  expect_fail "$1" "$2" "$root/scripts/receipt-check.sh" "$tmp/changed.json" "${held[@]}"
}
entry() { printf '(.checks[] | select(.name == "%s"))' "$1"; }

# --- No `prerelease` run succeeded on the commit.
with_runs() { # <name> <runs, as JSON>
  cp "$tmp/github/runs.json" "$tmp/runs.kept"
  printf '%s\n' "$2" >"$tmp/github/runs.json"
  refused "$1" "no run of the prerelease workflow for v0.1.0-rc.2 ended in success"
  cp "$tmp/runs.kept" "$tmp/github/runs.json"
}
with_runs "no run" '[]'
# A list that is empty, is no JSON, or could not be asked for, is no run that passed.
cp "$tmp/github/runs.json" "$tmp/runs.first"
: >"$tmp/github/runs.json"
refused "an empty answer for the runs" "the list of the prerelease workflow's runs could not be read"
echo 'not json' >"$tmp/github/runs.json"
refused "an answer for the runs that is no JSON" "the list of the prerelease workflow's runs could not be read"
cp "$tmp/runs.first" "$tmp/github/runs.json"
: >"$tmp/github/runs-fail"
refused "gh failing to list the runs" "the runs of the prerelease workflow could not be listed"
rm "$tmp/github/runs-fail"
with_runs "a failed run" '[{"conclusion":"failure","event":"push","headBranch":"v0.1.0-rc.2","headSha":"'"$commit"'","status":"completed"}]'
with_runs "a run still going" '[{"conclusion":null,"event":"push","headBranch":"v0.1.0-rc.2","headSha":"'"$commit"'","status":"in_progress"}]'
with_runs "a run of another commit" '[{"conclusion":"success","event":"push","headBranch":"v0.1.0-rc.2","headSha":"'"$earlier"'","status":"completed"}]'
with_runs "a run of the earlier candidate" '[{"conclusion":"success","event":"push","headBranch":"v0.1.0-rc.1","headSha":"'"$commit"'","status":"completed"}]'
with_runs "a run started another way" '[{"conclusion":"success","event":"workflow_dispatch","headBranch":"v0.1.0-rc.2","headSha":"'"$commit"'","status":"completed"}]'

# --- The receipt is missing; it is malformed; it holds a field its schema does not.
mv "$files/prerelease-receipt.json" "$tmp/kept.json"
refused "no receipt" "the candidate v0.1.0-rc.2 holds no receipt"
echo '{"schema": "prerelease-receipt/1", ' >"$files/prerelease-receipt.json"
refused "a receipt that is not JSON" "shape: the file is not JSON"
cat "$tmp/good.json" "$tmp/good.json" >"$files/prerelease-receipt.json"
refused "two values in the file" "shape: the file must hold one JSON value"
echo '[]' >"$files/prerelease-receipt.json"
refused "a receipt that is no object" "shape: the receipt is not one object"
cp "$tmp/kept.json" "$files/prerelease-receipt.json"
gate_receipt "a field the schema does not hold" "shape: the receipt must hold exactly" '.note = "passed by hand"'
receipt "a field missing" "shape: the receipt must hold exactly" 'del(.store)'
receipt "another schema" "shape: schema must be prerelease-receipt/1" '.schema = "prerelease-receipt/2"'
receipt "a field too many in an entry" "shape: entry 2 of checks" "$(entry setup).reply = \"hello\""
receipt "a field too many in the store" "shape: store must hold new and cache" '.store.path = "/somewhere"'
receipt "a check the schema does not know" "shape: entry 9 of checks" '.checks += [{name: "by-hand", exit: 0}]'
receipt "a value of another type" "shape: entry 1 of checks" "$(entry model-tests).exit = \"0\""
receipt "a reply's text where a word belongs" "shape: entry 5 of checks" "$(entry check-anthropic).model = \"the judge said: it looks fine to me\""
receipt "a path where a word belongs" "shape: entry 3 of checks" "$(entry mask).device = \"/opt/somewhere/models\""

# --- A required check is missing, is there twice, or did not complete.
for name in model-tests setup mask fill check-anthropic check-google check-openai loop; do
  gate_receipt "$name missing" "check: the required check $name is missing" "del(.checks[] | select(.name == \"$name\"))"
  receipt "$name twice" "check: the required check $name is there 2 times" ".checks += [$(entry "$name")]"
done
for name in mask fill check-anthropic check-google check-openai loop; do
  receipt "$name failed" "check: $name: its run did not complete" "$(entry "$name").status = \"error\""
  receipt "$name exited with a failure" "check: $name: its run did not complete" "$(entry "$name").exit = 1"
  receipt "$name without a run" "check: $name: its run did not complete" "$(entry "$name") |= (.run = null | .status = null)"
done

# --- The model tests: the checkout, the exit status, the report.
m=$(entry model-tests)
receipt "a checkout not made new" "check: model-tests: its checkout is not recorded as made new by the leg and found clean" "$m.checkout.new = false"
gate_receipt "a checkout not clean" "check: model-tests: its checkout is not recorded as made new by the leg and found clean" "$m.checkout.clean = false"
receipt "a checkout of another commit" "check: model-tests: its checkout is of another commit" "$m.checkout.commit = \"$earlier\""
gate_receipt "a checkout of another tree" "check: model-tests: its checkout is of another tree than the tag's commit" "$m.checkout.tree = (\"0\" * 40)"
receipt "a checkout of no tree" "check: model-tests: its checkout names no tree" "$m.checkout.tree = null"
receipt "the model tests failed" "check: model-tests: the test run's exit status is not 0" "$m.exit = 100"
gate_receipt "a failed test" "check: model-tests: the report holds a failed test" "$m.failed = [\"parity::gpu_and_cpu\"]"
receipt "a timed-out test" "check: model-tests: the report holds a timed-out test" "$m.timed_out = [\"parity::gpu_and_cpu\"]"
receipt "a test the filter names left out" "check: model-tests: the report lacks a test the filter names" "$m.ran = [\"model_tier::a_phrase\"]"
receipt "a test the filter does not name" "check: model-tests: the report holds a test the filter does not name" "$m.ran += [\"other::test\"]"
receipt "no report" "check: model-tests: there is no test report" "$m.report_sha256 = null"
receipt "no test at all" "check: model-tests: the filter named no test" "$m |= (.expected = [] | .ran = [])"

# --- setup: a failure, another extra, no hello, a probe that heard none.
s=$(entry setup)
gate_receipt "setup failed" "check: setup: its exit status is not 0" "$s.exit = 1"
receipt "another extra" "check: setup: it did not install the extra torch" "$s.extra = \"none\""
receipt "no hello" "check: setup: it printed no hello of the worker" "$s.hello = null"
receipt "the probe heard no hello" "check: setup: the probe heard no hello" "$s.probe_hello = false"
receipt "another segmenter" "check: mask: it does not name SAM 3 on the pack" "$(entry mask).model = \"tiny\""

# --- A provider's call: answered from a cache, or with no dispatched and settled request.
for name in fill check-anthropic check-google check-openai; do
  e=$(entry "$name")
  gate_receipt "$name from a cache" "check: $name: it was answered from a cache" "$e.cache_hit = true"
  receipt "$name with no request" "check: $name: it names no request dispatched to" "$e.requests = []"
  receipt "$name not sent" "check: $name: it names no request dispatched to" "$e.requests[0].stage = \"not_sent\""
  receipt "$name not answered" "check: $name: it names no request dispatched to" "$e.requests[0].ended = \"timed_out\""
  receipt "$name sent nowhere" "check: $name: it names no request dispatched to" "$e.sent_to = []"
  receipt "$name sent elsewhere" "check: $name: it names no request dispatched to" "$e.sent_to = [\"https://example.invalid\"]"
  receipt "$name by another backend" "check: $name: it does not name the backend" "$e.backend = \"other\""
done
receipt "a check by one provider under another's name" "check: check-openai: it names no request dispatched to https://api.openai.com" \
  "$(entry check-openai) |= (.requests[0].backend = \"anthropic\" | .sent_to = [\"https://api.anthropic.com\"])"
receipt "a check with no verdict" "check: check-google: it has no verdict" "$(entry check-google).verdict = null"

# --- The loop: unchecked, a plan that did not check, a child check that did not complete.
l=$(entry loop)
gate_receipt "the loop unchecked" "check: loop: it completed unchecked" "$l.kept |= (.verdict = null | .unchecked = \"no_judge\")"
receipt "the loop with no kept attempt" "check: loop: it completed unchecked" "$l.kept = null"
receipt "a plan that did not check" "check: loop: its plan did not check" "$l.plan_checks = false"
receipt "no child check" "check: loop: its child check did not complete" "$l.check = null"
gate_receipt "a child check that did not complete" "check: loop: its child check did not complete" "$l.check.status = \"error\""
receipt "a child check with no verdict" "check: loop: its child check did not complete" "$l.check.verdict = null"
receipt "a child check from a cache" "check: loop: its child check names no dispatched and settled request, or was answered from a cache" "$l.check.cache_hit = true"
receipt "a child check with no request" "check: loop: its child check names no dispatched and settled request" "$l.check.requests = []"

# --- The store was not new, or the cache was on.
gate_receipt "a store that was not new" "check: the store was not new" '.store.new = false'
gate_receipt "the cache on" "check: the decision cache was on" '.store.cache = "on"'

# --- Another commit, another candidate, another binary.
gate_receipt "another commit" "check: the receipt names another commit than the tag's" ".commit = \"$earlier\" | .binary.version_line = \"whizbang 0.1.0 (commit $earlier)\" | $m.checkout.commit = \"$earlier\""
gate_receipt "another candidate" "check: the receipt names another candidate than v0.1.0-rc.2" '.tag = "v0.1.0-rc.1" | .candidate = 1'
receipt "a tag that is not its parts" "check: the tag is not the version and the candidate number" '.candidate = 3'
receipt "a binary of another commit" "check: the installed binary's version line is not the candidate's" ".binary.version_line = \"whizbang 0.1.0 (commit $earlier)\""

# --- A digest it names is not that of the bytes.
gate_receipt "another Mac archive" "check: the receipt's digest of the aarch64-apple-darwin archive" '.archives["aarch64-apple-darwin"] = ("0" * 64)'
receipt "another Linux archive" "check: the receipt's digest of the x86_64-unknown-linux-gnu archive" '.archives["x86_64-unknown-linux-gnu"] = ("0" * 64)'
gate_receipt "another installed binary" "check: the receipt's digest of the installed binary" '.binary.sha256 = ("0" * 64)'
# The archive's bytes changed after the receipt, with SHA256SUMS brought along or not.
cp -R "$files" "$tmp/files.kept"
echo "more" >>"$files/$linux"
refused "an archive changed after its sums" "does not hold the files of a release"
printf '%s  %s\n' "$(sha256 "$files/$mac")" "$mac" "$(sha256 "$files/$linux")" "$linux" >"$files/SHA256SUMS"
refused "an archive changed, and its sums line with it" "check: the receipt's digest of the x86_64-unknown-linux-gnu archive"
rm -rf "$files" && cp -R "$tmp/files.kept" "$files"
echo "notes" >"$files/notes.txt"
refused "a file too many on the candidate" "does not hold the files of a release"
rm "$files/notes.txt"

# --- A candidate a later one superseded. rc.3 is tagged: rc.2's receipt is not taken,
# whether rc.3 is of this commit (it has no files yet) or of a later one.
git -C "$repo" tag v0.1.0-rc.3
refused "a later candidate with no run" "no run of the prerelease workflow for v0.1.0-rc.3 ended in success"
echo '[{"conclusion":"success","event":"push","headBranch":"v0.1.0-rc.3","headSha":"'"$commit"'","status":"completed"}]' >"$tmp/github/runs.json"
refused "a later candidate with no files" "the files of the candidate v0.1.0-rc.3 could not be downloaded"
cp -R "$files" "$tmp/github/release/v0.1.0-rc.3"
refused "the earlier candidate's receipt on the later one" "check: the receipt names another candidate than v0.1.0-rc.3"
rm "$tmp/github/release/v0.1.0-rc.3/prerelease-receipt.json"
refused "the later candidate without a receipt" "the candidate v0.1.0-rc.3 holds no receipt"
git -C "$repo" tag -d v0.1.0-rc.3 >/dev/null
commit_as "a later commit"
git -C "$repo" tag v0.1.0-rc.3
refused "a later candidate of another commit" "the latest candidate of 0.1.0, v0.1.0-rc.3, is of commit"
git -C "$repo" tag -d v0.1.0-rc.3 >/dev/null
cp "$tmp/runs.kept" "$tmp/github/runs.json"
# Ten is later than nine: the numbers are compared as numbers.
git -C "$repo" tag v0.1.0-rc.10
refused "candidate 10 after candidate 2" "the latest candidate of 0.1.0, v0.1.0-rc.10, is of commit"
git -C "$repo" tag -d v0.1.0-rc.10 >/dev/null
expect_pass "the candidate again, with the later ones gone" gated

# --- The tag: no candidate of its version, not a version tag, not in the repository.
git -C "$repo" tag v0.0.9 "$earlier"
git -C "$repo" tag v0.2.0
refused "a version with no candidate" "version 0.2.0 has no candidate" v0.2.0
refused "a candidate tag" "v0.1.0-rc.2 is not a version tag" v0.1.0-rc.2
refused "a tag that is not there" "the tag v9.9.9 is not in this repository" v9.9.9
# A version whose candidate is of its commit but has no run, on an earlier commit.
refused "another version's tag" "no run of the prerelease workflow for v0.0.9-rc.1" v0.0.9
# A folder that is there already is not written into.
mkdir "$tmp/there"
expect_fail "a folder that is there" "is there already" env PATH="$tmp/bin:$PATH" bash -c "cd '$repo' && '$gate' v0.1.0 '$tmp/there'"
[ -d "$tmp/there" ] || fail "a folder that was there is left"

finished
