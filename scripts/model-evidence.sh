#!/usr/bin/env bash
# Writes the model tier's evidence record (docs/plans/m1c-local-pack.md, task R2b; decision
# D15): docs/results/m1c-local-pack.md and its numbers file, docs/results/m1c-local-pack.toml;
# and the timings of checkpoint 1D (plan 1D, task V1), docs/results/m1d-segment-coverage.md,
# from one run of `scripts/test-models.sh coverage` after the three rounds.
# On the owner's Mac only, never in CI.
#
#   WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh model-evidence scripts/model-evidence.sh
#
# It runs the model tier (scripts/test-models.sh, which runs it under scripts/memory-watch.sh)
# three times, so that the load, the first request and the second are each timed three
# times; the times are reported as they came, with no order asserted between them. Each
# time it keeps what the tests wrote and what the watch saw. Then it writes the two files
# from them, with the commit of the tree measured, the digest of the pack's lock file, of
# the adapter's source and of the fixture picture, and the adapter's revision in the
# binary's table: what the fast test
# `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`
# holds the committed record to. Nothing in either file is a path.
#
# It refuses unless the variable scripts/thread-guard.sh sets for what it runs is there
# (scripts/heavy.sh runs its command under the guard) with the limit at 200, and, through
# scripts/test-models.sh, unless WHIZBANG_MODELS_DIR names the models directory. The
# variable is a tripwire against a run by hand, not a proof that the guard is there. It
# refuses a tree with changes not committed, before anything runs and again before it
# writes, so that the commit the record names is the tree that ran (plan 2A, task R1);
# the record says `tree = "clean"`, which the fast test holds it to.
#
# Before the three rounds it runs the model tier once more and keeps nothing of it: that
# run's full check of the installation's files leaves its record in the models directory
# (plan 2A, task R1), so each round's first load is timed without the full hash, and each
# round says whether it found that record (`check_record_before`).
# `--candidate` as its one argument measures the staged tree instead (plan m6, task P1,
# item 5): when the sources the records name change, the records cannot be committed after
# them, since the fast tests fail on the sources until the records match. It refuses unstaged
# changes and untracked files under `pack/` and `crates/`; scripts/candidate.sh prepares the
# candidate once (the index exported, its binary built, its pack installed into a models
# directory of its own and held to the tree's sources); each round runs the exported tree's
# own scripts/test-models.sh against it; the digests are of the exported files; and the
# records say `tree = "candidate"` and the tree's id, `candidate_tree`, beside the commit the
# index was staged on. Before it writes, the index must still be that tree. The records are
# then committed with exactly the staged sources, in one commit.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "model-evidence: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

candidate=0
case "${1-}" in
  "") ;;
  --candidate) candidate=1 ;;
  *) echo "model-evidence: the one argument it takes is --candidate" >&2; exit 2 ;;
esac
[ "$#" -le 1 ] || { echo "model-evidence: the one argument it takes is --candidate" >&2; exit 2; }

[ -n "${WHIZBANG_THREAD_GUARD:-}" ] || {
  echo "model-evidence: run it through scripts/heavy.sh, with WHIZBANG_THREAD_LIMIT=200" >&2
  exit 2
}
[ "${WHIZBANG_THREAD_LIMIT:-}" = 200 ] || {
  echo "model-evidence: the thread guard's limit must be 200 (WHIZBANG_THREAD_LIMIT=200)" >&2
  exit 2
}

target=${CARGO_TARGET_DIR:-$root/target}
case "$target" in
  /*) ;;
  *) target=$root/$target ;;
esac
shared=$target/tmp/model-tier
work=$target/tmp/model-evidence
mkdir -p "$work"

# What is measured: this checkout, clean; or the staged tree, exported (see the top).
src=$root
tm=(scripts/test-models.sh)
if [ "$candidate" = 1 ]; then
  cand=$target/tmp/candidate
  rm -rf "$cand"
  mkdir -p "$cand"
  scripts/candidate.sh prepare "$cand"
  candidate_tree=$(cat "$cand/tree-id")
  src=$cand/tree
  shared=$cand/target/tmp/model-tier
  tm=(env CARGO_TARGET_DIR="$cand/target" WHIZBANG_MODELS_DIR="$cand/models" "$cand/tree/scripts/test-models.sh")
else
  [ -z "$(git status --short)" ] || {
    echo "model-evidence: the tree has changes not committed; commit them, then run it again" >&2
    exit 2
  }
fi

sha() { shasum -a 256 "$1" | awk '{ print "sha256:" $1 }'; }
# value <file> <name>: the value of `name=value` in a file of the tests' or the watch's.
value() {
  local found
  found=$(sed -n "s/^$2=//p" "$1" | head -n 1)
  [ -n "$found" ] || { echo "model-evidence: $2 is not in $(basename "$1")" >&2; exit 1; }
  printf '%s\n' "$found"
}

# The run kept from nothing: see the top.
"${tm[@]}" model_tier
for round in 1 2 3; do
  rm -f "$work/phrase-$round.txt" "$work/second-$round.txt" "$work/places-$round.txt" "$work/tiny-$round.txt" "$work/watch-$round.txt"
  WHIZBANG_MEMORY_REPORT=$work/watch-$round.txt "${tm[@]}" model_tier
  cp "$shared/out/phrase.txt" "$work/phrase-$round.txt"
  cp "$shared/out/second.txt" "$work/second-$round.txt"
  cp "$shared/out/places.txt" "$work/places-$round.txt"
  cp "$shared/out/tiny.txt" "$work/tiny-$round.txt"
done

# The timings of checkpoint 1D (plan 1D, task V1): once, under the watch, after the rounds.
rm -f "$work/coverage.txt" "$work/watch-coverage.txt"
WHIZBANG_MEMORY_REPORT=$work/watch-coverage.txt "${tm[@]}" coverage
cp "$shared/out/coverage.txt" "$work/coverage.txt"

commit=$(git rev-parse HEAD)
if [ "$candidate" = 1 ]; then
  tree=candidate
  { [ "$(git write-tree)" = "$candidate_tree" ] && git diff --quiet -- pack crates; } || {
    echo "model-evidence: the index changed while the model tier ran; nothing was written" >&2
    exit 1
  }
else
  tree=clean
  [ -z "$(git status --short)" ] || {
    echo "model-evidence: the tree changed while the model tier ran; nothing was written" >&2
    exit 1
  }
fi
lock=$(sha "$src/pack/uv.lock")
source=$(sha "$src/pack/src/whizbang_pack/sam3.py")
fixture=$(sha "$src/docs/results/m1c-local-pack/circle.png")
revision=$(sed -n 's/.*named("sam3", "\([^"]*\)", true).*/\1/p' "$src/crates/whizbang/src/wiring.rs")
[ -n "$revision" ] || { echo "model-evidence: the binary's table names no shipped sam3" >&2; exit 1; }
tiny_source=$(sha "$src/pack/src/whizbang_pack/owlv2_sam2.py")
tiny_revision=$(sed -n 's/.*named("owlv2-sam2", "\([^"]*\)", true).*/\1/p' "$src/crates/whizbang/src/wiring.rs")
[ -n "$tiny_revision" ] || { echo "model-evidence: the binary's table names no shipped owlv2-sam2" >&2; exit 1; }
tiny_installation=$(value "$work/tiny-1.txt" installation)
p1=$work/phrase-1.txt
installation=$(value "$p1" installation)
device=$(value "$p1" device)
measured=$(date -u +%Y-%m-%d)

toml=docs/results/m1c-local-pack.toml
{
  echo "# The numbers of the model tier of checkpoint 1C (task R2b): SAM 3 answering a phrase on"
  echo "# the Mac's GPU through the binary's wiring. Written by scripts/model-evidence.sh; see"
  echo "# m1c-local-pack.md beside this file. No path."
  echo
  echo "record = \"m1c-local-pack\""
  echo "measured = \"$measured\""
  echo "commit = \"$commit\""
  echo "tree = \"$tree\""
  [ "$candidate" = 0 ] || echo "candidate_tree = \"$candidate_tree\""
  echo "lock = \"$lock\""
  echo "adapter = \"sam3\""
  echo "adapter_revision = \"$revision\""
  echo "adapter_source = \"$source\""
  echo "tiny_adapter = \"owlv2-sam2\""
  echo "tiny_adapter_revision = \"$tiny_revision\""
  echo "tiny_adapter_source = \"$tiny_source\""
  echo "tiny_installation = \"$tiny_installation\""
  echo "fixture = \"$fixture\""
  echo "phrase = \"circle\""
  echo "installation = \"$installation\""
  echo "device = \"$device\""
  echo "fallback = \"off\""
  echo "hosts_contacted = \"none\""
  for round in 1 2 3; do
    p=$work/phrase-$round.txt
    s=$work/second-$round.txt
    w=$work/watch-$round.txt
    echo
    echo "[[round]]"
    echo "round = $round"
    echo "phrase_run = \"$(value "$p" run)\""
    echo "phrase_device = \"$(value "$p" device)\""
    echo "phrase_instances = $(value "$p" request_instances)"
    echo "phrase_score = \"$(value "$p" request_1_score)\""
    echo "phrase_box = \"$(value "$p" request_1_box)\""
    echo "phrase_mask = \"$(value "$p" request_1_mask)\""
    echo "phrase_pixels_on = $(value "$p" request_1_pixels_on)"
    echo "first_run = \"$(value "$s" first_run)\""
    echo "second_run = \"$(value "$s" second_run)\""
    echo "loads = $(value "$s" loads)"
    echo "until_load_written_seconds = \"$(value "$s" until_load_written_seconds)\""
    echo "load_seconds = \"$(value "$s" load_seconds)\""
    echo "first_load_seconds = \"$(value "$s" first_load_seconds)\""
    echo "check_record_before = \"$(value "$s" check_record_before)\""
    echo "first_request_seconds = \"$(value "$s" first_request_seconds)\""
    echo "second_request_seconds = \"$(value "$s" second_request_seconds)\""
    echo "first_mask = \"$(value "$s" first_1_mask)\""
    l=$work/places-$round.txt
    echo "places_run = \"$(value "$l" run)\""
    echo "places_device = \"$(value "$l" device)\""
    echo "places_seconds = \"$(value "$l" request_seconds)\""
    echo "point_instances = $(value "$l" point_instances)"
    echo "point_score = \"$(value "$l" point_1_score)\""
    echo "point_box = \"$(value "$l" point_1_box)\""
    echo "point_mask = \"$(value "$l" point_1_mask)\""
    echo "box_instances = $(value "$l" box_instances)"
    echo "box_score = \"$(value "$l" box_1_score)\""
    echo "box_box = \"$(value "$l" box_1_box)\""
    echo "box_mask = \"$(value "$l" box_1_mask)\""
    echo "second_mask = \"$(value "$s" second_1_mask)\""
    t=$work/tiny-$round.txt
    echo "tiny_phrase_run = \"$(value "$t" phrase_run)\""
    echo "tiny_places_run = \"$(value "$t" places_run)\""
    echo "tiny_device = \"$(value "$t" device)\""
    echo "tiny_phrase_instances = $(value "$t" phrase_instances)"
    echo "tiny_phrase_seconds = \"$(value "$t" phrase_seconds)\""
    echo "tiny_phrase_score = \"$(value "$t" phrase_1_score)\""
    echo "tiny_phrase_box = \"$(value "$t" phrase_1_box)\""
    echo "tiny_phrase_mask = \"$(value "$t" phrase_1_mask)\""
    echo "tiny_places_seconds = \"$(value "$t" places_seconds)\""
    echo "tiny_point_instances = $(value "$t" point_instances)"
    echo "tiny_point_score = \"$(value "$t" point_1_score)\""
    echo "tiny_point_box = \"$(value "$t" point_1_box)\""
    echo "tiny_box_instances = $(value "$t" box_instances)"
    echo "tiny_box_score = \"$(value "$t" box_1_score)\""
    echo "tiny_box_box = \"$(value "$t" box_1_box)\""
    echo "peak_resident_kib = $(value "$w" peak_resident_kib)"
    echo "most_threads_in_one_process = $(value "$w" most_threads)"
    echo "most_processes = $(value "$w" most_processes)"
    echo "still_running_after = $(value "$w" still_running)"
  done
} >"$toml"

row() {
  local name=$1 key=$2 file=$3
  printf '| %s | %s | %s | %s |\n' "$name" \
    "$(value "$work/$file-1.txt" "$key")" "$(value "$work/$file-2.txt" "$key")" "$(value "$work/$file-3.txt" "$key")"
}
md=docs/results/m1c-local-pack.md
{
  echo "# Checkpoint 1C: SAM 3 answers a phrase on the Mac's GPU"
  echo
  echo "Written by \`scripts/model-evidence.sh\` (plan 1C, task R2b; decision D15); the numbers are in"
  echo "\`m1c-local-pack.toml\` beside this file, and"
  echo "\`evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has\` fails"
  echo "when they are not of the tree's adapter, lock and fixture."
  echo
  echo "**What ran.** The model tier, \`just test-models\` (\`crates/whizbang/tests/it/model_tier.rs\`),"
  echo "three times, on the owner's Mac, under the heavy lock, the thread guard at 200 and the"
  echo "watch on resident memory at 24 GiB: a request for the phrase \`circle\` about the fixture"
  echo "picture (\`docs/results/m1c-local-pack/circle.png\`, drawn by a program: a red disc"
  echo "on a grey ground beside a blue square), through the binary's wiring, the kind \`pack\`,"
  echo "the queue and the supervisor of the one worker, the worker the installation of"
  echo "\`whizbang setup --extra torch\` runs, and the adapter \`sam3\`, with the CPU fallback of the"
  echo "GPU's runtime off. The run's record is read back by the real binary (\`whizbang runs"
  echo "show\`). No command of the binary sends a phrase (no action is added in 1C), so a test"
  echo "action asks for the segment capability."
  echo
  echo "| Field | Value |"
  echo "|---|---|"
  echo "| Measured | $measured |"
  if [ "$candidate" = 1 ]; then
    echo "| Commit measured | The staged tree \`$candidate_tree\` on \`$commit\` (candidate) |"
  else
    echo "| Commit measured | \`$commit\` ($tree) |"
  fi
  echo "| Lock file | \`$lock\` |"
  echo "| Adapter | \`sam3\`, revision \`$revision\`, source \`$source\` |"
  echo "| Installation | \`$installation\` |"
  echo "| Tiny's adapter (plan 1D, task T1) | \`owlv2-sam2\`, revision \`$tiny_revision\`, source \`$tiny_source\` |"
  echo "| Tiny's installation | \`$tiny_installation\` |"
  echo "| Device | \`$device\`, in every run's record |"
  echo "| CPU fallback | Off: the worker's environment does not set \`PYTORCH_ENABLE_MPS_FALLBACK\` |"
  echo "| Hosts contacted | None: the worker runs with \`HF_HUB_OFFLINE=1\` and loads with \`local_files_only=True\` from the installation's folder |"
  echo
  echo "| Each run | 1 | 2 | 3 |"
  echo "|---|---|---|---|"
  row "Run of the phrase test" run phrase
  row "Instances" request_instances phrase
  row "Best instance's score" request_1_score phrase
  row "Its box (left top right bottom)" request_1_box phrase
  row "Its mask" request_1_mask phrase
  row "Pixels on" request_1_pixels_on phrase
  row "Start of the first request to the load written, s" until_load_written_seconds second
  row "Load, s" load_seconds second
  row "Start of the first request to the model loaded, s" first_load_seconds second
  row "A full check's record found before the first load" check_record_before second
  row "First request after the load, s" first_request_seconds second
  row "Second request, s" second_request_seconds second
  row "Loads for two requests" loads second
  row "Run of the point and the box (plan 1D, task Q1)" run places
  row "Point and box in one request, s" request_seconds places
  row "Point at 420 200: candidates" point_instances places
  row "Its best predicted IoU" point_1_score places
  row "Its box" point_1_box places
  row "Box 300 80 540 320: candidates" box_instances places
  row "Its best predicted IoU" box_1_score places
  row "Its box" box_1_box places
  row "Run of Tiny's phrase (plan 1D, task T1)" phrase_run tiny
  row "Tiny's device" device tiny
  row "Tiny's phrase: instances" phrase_instances tiny
  row "Its phrase's request, s" phrase_seconds tiny
  row "Best instance's score" phrase_1_score tiny
  row "Its box" phrase_1_box tiny
  row "Tiny's point and box in one request, s" places_seconds tiny
  row "Point at 420 200: candidates" point_instances tiny
  row "Its best predicted IoU" point_1_score tiny
  row "Its box" point_1_box tiny
  row "Box 300 80 540 320: candidates" box_instances tiny
  row "Its best predicted IoU" box_1_score tiny
  row "Its box" box_1_box tiny
  printf '| %s | %s | %s | %s |\n' "Peak resident memory of the tree, KiB" \
    "$(value "$work/watch-1.txt" peak_resident_kib)" "$(value "$work/watch-2.txt" peak_resident_kib)" "$(value "$work/watch-3.txt" peak_resident_kib)"
  printf '| %s | %s | %s | %s |\n' "Most threads in one process" \
    "$(value "$work/watch-1.txt" most_threads)" "$(value "$work/watch-2.txt" most_threads)" "$(value "$work/watch-3.txt" most_threads)"
  printf '| %s | %s | %s | %s |\n' "Processes left running after" \
    "$(value "$work/watch-1.txt" still_running)" "$(value "$work/watch-2.txt" still_running)" "$(value "$work/watch-3.txt" still_running)"
  echo
  echo "The fixture's disc spans 330 to 510 across and 110 to 290 down; the loose box drawn by"
  echo "hand around it, which the test holds an instance's box to, is 300, 80, 540, 320, and is"
  echo "the box asked of the tracker; the point asked is the disc's centre, 420, 200. The"
  echo "times are of one machine on one day and are reported, not compared: the load is the"
  echo "worker's \`load\` call (weights from the disk onto the GPU), and the first request's time"
  echo "beside it includes the processor's first run. The time before the load is written holds"
  echo "the check of the installation's files, the worker's start and its import of the runtime."
  echo "The check is the cached one of plan 2A, task R1: a run of the model tier before the"
  echo "three, whose numbers are not kept, hashed the installation's 3.4 GB in full and left"
  echo "the record of that check, so each round's first load compares each file's size, time"
  echo "of last write and inode against it and reads no file whole. The memory is resident"
  echo "memory of the"
  echo "whole tree (cargo, the test, the worker); the weights are in the GPU's memory, which"
  echo "macOS does not count as resident. The thread count is the most in one process, the"
  echo "worker's runtime pools at 8 each."
  echo
  echo "**Tiny** (plan 1D, task T1): the model \`tiny\`, the adapter \`owlv2-sam2\`, asked the same"
  echo "phrase, and then the point and the loose box in one request, through the same wiring, by a"
  echo "test of its own with its own worker and one load for the two requests. OWLv2 finds the"
  echo "boxes for the phrase and SAM 2.1 Hiera-Tiny draws each mask; the point and the box are"
  echo "SAM 2.1's alone, and each answers its candidates, best predicted IoU first."
} >"$md"

c=$work/coverage.txt
cw=$work/watch-coverage.txt
sam3_model=$(sed -n '/"name": "sam3"/,/"revision"/s/.*"revision": "\([^"]*\)".*/\1/p' "$src/crates/whizbang/src/catalogue.json" | head -n 1)
tiny_model=$(sed -n '/"name": "tiny"/,/"revision"/s/.*"revision": "\([^"]*\)".*/\1/p' "$src/crates/whizbang/src/catalogue.json" | head -n 1)
[ -n "$sam3_model" ] && [ -n "$tiny_model" ] || { echo "model-evidence: the catalogue names no revision of sam3 or tiny" >&2; exit 1; }
coverage=docs/results/m1d-segment-coverage.md
{
  echo "# Checkpoint 1D: timings of phrase, point and box on SAM 3 and Tiny"
  echo
  echo "Written by \`scripts/model-evidence.sh\` (plan 1D, task V1, item 4) from one run of"
  echo "\`scripts/test-models.sh coverage\` (\`crates/whizbang/tests/it/coverage.rs\`) on the owner's"
  echo "Mac, under the heavy lock, the thread guard at 200 and the watch on resident memory;"
  echo "\`evidence::the_coverage_record_names_a_commit_of_this_tree_and_every_timing\` holds the"
  echo "fields below to the tree. The picture is the model tier's (\`circle.png\`, drawn by a"
  echo "program), asked the phrase \`circle\`, the point 420 200 or the box 300 80 540 320. No"
  echo "picture of the owner's."
  echo
  echo "For each model and each kind of query, a worker of its own: \`first\` is the first"
  echo "request from the action's side, the worker's start, the cached check of the installation's"
  echo "files, the runtime's import and the load included; \`load\` is the worker's \`load\` call"
  echo "alone, by the supervisor's status; \`second\` is a second request of the same kind, the"
  echo "model resident. SAM 3's point and box load its tracker on the first of them, inside the"
  echo "request. The GPU's memory is what the driver holds for the worker"
  echo "(\`torch.mps.driver_allocated_memory\`), said by the adapter \`sam3\` beside each answer on"
  echo "the GPU: after a phrase (\`Sam3Model\` resident) and after a point that followed it (both"
  echo "models resident). The resident memory is the watch's peak over the whole tree (cargo, the"
  echo "test, the worker), which does not count the GPU's. Times are of one machine on one day,"
  echo "reported, not compared."
  echo
  echo '```toml'
  echo "record = \"m1d-segment-coverage\""
  echo "measured = \"$measured\""
  echo "commit = \"$commit\""
  echo "tree = \"$tree\""
  [ "$candidate" = 0 ] || echo "candidate_tree = \"$candidate_tree\""
  echo "sam3_revision = \"$sam3_model\""
  echo "sam3_adapter_revision = \"$revision\""
  echo "sam3_adapter_source = \"$source\""
  echo "tiny_revision = \"$tiny_model\""
  echo "tiny_adapter_revision = \"$tiny_revision\""
  echo "tiny_adapter_source = \"$tiny_source\""
  echo "device = \"mps\""
  for model in sam3 tiny; do
    for kind in phrase point box; do
      for part in load first second; do
        echo "${model}_${kind}_${part}_seconds = \"$(value "$c" "${model}_${kind}_${part}_seconds")\""
      done
    done
  done
  echo "sam3_one_model_gpu_mib = $(value "$c" sam3_one_model_gpu_mib)"
  echo "sam3_two_models_gpu_mib = $(value "$c" sam3_two_models_gpu_mib)"
  echo "peak_resident_kib = $(value "$cw" peak_resident_kib)"
  echo "most_threads_in_one_process = $(value "$cw" most_threads)"
  echo "still_running_after = $(value "$cw" still_running)"
  echo '```'
} >"$coverage"
echo "model-evidence: wrote $toml, $md and $coverage"
}
main "$@"; exit
