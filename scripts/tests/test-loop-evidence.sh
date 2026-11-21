#!/usr/bin/env bash
# The loop's evidence (scripts/loop-evidence.sh and its tally, scripts/loop-evidence.py;
# plan m4, task E1, items 1 and 2), against a stand-in binary that answers each command
# and writes run records as the binary stores them, in a throwaway repository with
# stand-ins for the three guards, which write down that they ran. Nothing is built, no
# model is loaded and no provider is called. One case at the end runs through the real
# lock and the real guards.
#
# What it holds: each of the 24 entries is run once under its request id; a run of the
# script that stopped goes on from where it stopped, and a second run starts nothing
# again; the file written holds aggregates and no item; a set with a picture missing, with
# one changed, with a 25th entry, or with an entry that names a truth mask is refused
# before anything is imported or run; every command that loads a model, the three cases
# among them, runs under the watch on memory and the thread guard at 200; and without the
# heavy lock nothing starts. For a manifest of the kind segment-01 (plan e1, task L1): its
# 20 entries are run, each under a request id that holds the repetition; one that names a
# truth mask is refused; and a call whose plan is fewer than three checked attempts stops
# the script before any run.
source "$(dirname "$0")/lib.sh"

command -v python3 >/dev/null || fail "python3 is required (3.9 or later, standard library only)"
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
# Whatever this test was started under, the script is held to what the test gives it.
unset WHIZBANG_HEAVY_LOCK WHIZBANG_THREAD_LIMIT WHIZBANG_MEMORY_REPORT WHIZBANG_MEMORY_LIMIT_KIB

fix="$tmp/fix"
mkdir -p "$fix" "$tmp/bin"
# A made-up key, in the env file the script must never open: it may appear nowhere.
echo "FAL_KEY='made-up-key-0123456789'" >"$tmp/keys.env"
echo '# a config the stand-in never reads' >"$tmp/config.toml"

# --- The throwaway repository: the script and its tally as they are, and stand-ins for
# the lock and the two guards.
repo="$tmp/repo"
mkdir -p "$repo/scripts"
cp scripts/loop-evidence.sh scripts/loop-evidence.py "$repo/scripts/"
cat >"$repo/scripts/heavy.sh" <<'SH'
#!/bin/bash
# The stand-in for the heavy lock: it holds the lock while its command runs.
shift
mkdir "$WHIZBANG_HEAVY_LOCK" && echo $$ >"$WHIZBANG_HEAVY_LOCK/pid"
status=0
"$@" || status=$?
rm -rf "$WHIZBANG_HEAVY_LOCK"
exit "$status"
SH
for guard in memory-watch thread-guard; do
  printf '%s\n' '#!/bin/bash' "export GUARDS=\"\${GUARDS:-}$guard,\"" 'exec "$@"' >"$repo/scripts/$guard.sh"
done
chmod +x "$repo"/scripts/*.sh
git init -q "$repo"
git -C "$repo" add -A
git -C "$repo" -c user.name=test -c user.email=test@example.invalid commit -q -m "the tree the binary is of"
commit=$(git -C "$repo" rev-parse HEAD)
echo "whizbang 0.1.0 (commit $commit)" >"$fix/version-line"
script="$repo/scripts/loop-evidence.sh"
export WHIZBANG_HEAVY_LOCK="$tmp/lock"

# --- The stand-in binary. Everything but a run of the loop is answered in the shell; a
# run of the loop writes its records with the helper beside it.
cat >"$tmp/bin/whizbang" <<SH
#!/bin/bash
[ "\$*" = --version ] && { cat "$fix/version-line"; exit 0; }
printf '%s|%s|%s\n' "\${GUARDS:-}" "\${WHIZBANG_THREAD_LIMIT:-}" "\$*" >>"$fix/commands"
store=\$(printf '%s\n' "\$*" | sed -n 's/^.* --store \([^ ]*\).*\$/\1/p')
case "\$1 \$2" in
  "store import")
    if [ ! -d "\$store" ]; then
      case " \$* " in *" --create "*) mkdir "\$store" && echo "created a store at \$store" ;; *) echo "whizbang: no store" >&2; exit 1 ;; esac
    fi
    if command -v sha256sum >/dev/null; then sum=\$(sha256sum "\$4" | cut -d' ' -f1); else sum=\$(shasum -a 256 "\$4" | cut -d' ' -f1); fi
    echo "imported image sha256:\$sum"
    ;;
  "spend reset") echo "\$*" >>"$fix/allowances"; echo 'allowance reset' ;;
  "runs export") [ ! -f "\$store/records.jsonl" ] || cat "\$store/records.jsonl" ;;
  "rascal remove-checked") exec python3 -B "$tmp/bin/loops.py" "$fix" "\$store" "\$@" ;;
  *) echo "whizbang: the stand-in knows no such command: \$*" >&2; exit 2 ;;
esac
SH
chmod +x "$tmp/bin/whizbang"
cat >"$tmp/bin/loops.py" <<'PY'
"""The stand-in's `rascal remove-checked`: one run of the loop and the runs under it, as
records of `run/6`, appended to the store's export. Which way the run goes is the number
of loops the store already holds, modulo 8, or the word in the file `kind`."""
import json, os, subprocess, sys

fix, store, args = sys.argv[1], sys.argv[2], sys.argv[3:]
flag = lambda name: args[args.index(name) + 1] if name in args else None
request, judge, prompt, image = flag("--request-id"), flag("--judge"), flag("--prompt"), flag("--image")
path = os.path.join(store, "records.jsonl")
held = [json.loads(line) for line in open(path)] if os.path.exists(path) else []
loops = [r for r in held if r["asked"]["action"] == "remove_checked"]
if "--plan-only" in args:
    # The plan of the call, prepared and not run: the line the file `plan` holds.
    open(os.path.join(fix, "planned"), "a").write("%s|%s\n" % (judge, prompt))
    plan_file = os.path.join(fix, "plan")
    print(open(plan_file).read().strip() if os.path.exists(plan_file) else "plan: 3 of 3 attempts asked, checked; segment 17; fill 3; judge 48")
    sys.exit(0)
for loop in loops:
    if loop["identity"]["request"] == request:
        open(os.path.join(fix, "repeats"), "a").write(request + "\n")
        print("run %s completed" % loop["identity"]["run"])
        sys.exit(0)
refuse_at = os.path.join(fix, "refuse-at")
if os.path.exists(refuse_at) and int(open(refuse_at).read()) == len(loops) + 1:
    print("whizbang: the call is not ready: a backend names a key its env file does not set", file=sys.stderr)
    sys.exit(1)
if os.path.exists(os.path.join(fix, "ancestors")):
    # Who is above this process, and what the thread guard told it.
    at, above = os.getpid(), []
    for _ in range(12):
        out = subprocess.run(["ps", "-o", "ppid=,command=", "-p", str(at)], capture_output=True, text=True).stdout.split(None, 1)
        if len(out) < 2 or int(out[0]) <= 1:
            break
        at = int(out[0])
        above.append(out[1].strip())
    open(os.path.join(fix, "above"), "w").write("\n".join(above[1:]) + "\nguard=%s\n" % os.environ.get("WHIZBANG_THREAD_GUARD", ""))
open(os.path.join(fix, "started"), "a").write(request + "\n")
n = len(loops)
kind_file = os.path.join(fix, "kind")
way = int(open(kind_file).read()) if os.path.exists(kind_file) else n % 8
top = "%d-1" % (1000 + n)
made = []
digest = lambda c: "sha256:" + c * 64


def record(run, action, parent=None, **more):
    base = {
        "record": "run/6",
        "identity": {"run": run, "request": None, "parent": parent, "whizbang": "0.1.0"},
        "asked": {"action": action, "version": "1.0.0", "arguments": [], "settings": [], "config": digest("c")},
        "inputs": [], "outputs": [],
        "outcome": {"status": "completed", "error": None, "invalid": None, "refusals": [], "refusals_omitted": 0, "lost": []},
        "time": {"start_ms": 1000, "end_ms": 2000, "durations": []},
        "models": [], "spend": {"reserved_micros": None, "requests": []},
        "egress": [], "steps": [], "judge": [], "diagnostics": [],
    }
    base.update(more)
    made.append(base)
    return base


def paid(capability, backend, model, version, micros):
    return {
        "models": [{"capability": capability, "backend": backend, "model": model, "version": version,
                    "device": None, "cached": None, "ended": "answered"}],
        "spend": {"reserved_micros": micros, "requests": [
            {"request": 0, "reserved_micros": micros, "settled_micros": micros, "unresolved_micros": 0,
             "stage": "answered", "over_bound": False}]},
    }


def count(obligation, disposition, confidence, many=1):
    return {"obligation": obligation, "disposition": disposition, "confidence": confidence, "count": many}


def found(decision, counts, complete=True, places=()):
    left = {"findings": 0, "places": 0} if complete else {"findings": 2, "places": 0}
    return {"decision": decision, "complete": complete, "left_out": left, "counts": counts, "places": list(places)}


BOX = {"image": digest("f"), "box": [0.1, 0.1, 0.3, 0.3]}
POLYGON = {"image": digest("f"), "polygon": [[0.1, 0.1], [0.3, 0.1], [0.2, 0.3]]}
PASSES = found("pass", [count("RQ1", "held", "confident")])
attempts = []


def attempt(verdict, findings, changed=None, unchecked=None, views=0):
    number = len(attempts) + 1
    fill = "%s-a%d-fill" % (top, number)
    record(fill, "remove_prompt", top, **paid("fill", "fal", "bria-eraser", "1+fit1", 40000))
    runs, settled = [], 40000
    for view in range(views):
        # A place a retry looked into: a cut of the picture.
        runs.append("%s-a%d-view%d" % (top, number, view))
        record(runs[-1], "view", top)
    runs.append(fill)
    if findings is not None:
        check = "%s-a%d-check" % (top, number)
        # A judge's words are in the record of a check's own run, and in no tally.
        record(check, "source-fidelity", top, findings=findings)
        record(check + "-turn", "check", check,
               judge=[{"judge": "verdict", "decision": verdict, "why": "MARMALADE in a judge's reason."}],
               **paid("judge", judge or "default", "a-judge-model", "2026-01", 3300))
        runs.append(check)
        settled += 3300
    attempts.append({"attempt": number, "runs": runs, "verdict": verdict if findings is not None else None,
                     "unchecked": unchecked, "changed": changed, "settled_micros": settled})


plan = {"sends": [], "attempts": {"asked": 3, "allowed": 3, "checks": True}, "why": None}
error, ended, kept = None, "attempt 1 passed", 1
if way == 0:
    attempt("pass", PASSES)
elif way == 1:
    attempt("fail", found("fail", [count("RQ1", "violation", "confident")]))
    attempt("pass", PASSES, changed={"change": "wider", "px": 8.0})
    ended, kept = "attempt 2 passed", 2
elif way == 2:
    attempt("fail", found("fail", [count("RQ1", "violation", "confident", 2)], places=[BOX, BOX]))
    crops = [{"crop": digest("1"), "mask": digest("2")}, {"crop": digest("3"), "mask": None}]
    attempt("pass", PASSES, changed={"change": "located", "crops": crops}, views=3)
    ended, kept = "attempt 2 passed", 2
elif way == 3:
    attempt("unsure", found("unsure", [count("RQ1", "violation", "tentative")], places=[POLYGON]))
    ended = "no finding of attempt 1 says with confidence that the named thing is still there"
elif way == 4:
    attempt("fail", found("fail", [count("RQ1", "violation", "confident"), count("SF1", "violation", "confident")]))
    ended = "attempt 1 changed something that must be preserved, which painting more cannot mend"
elif way == 5:
    why = "no judge is ready: the backend's key is not in the env file"
    plan = {"sends": [], "attempts": {"asked": 3, "allowed": 1, "checks": False}, "why": why}
    attempt(None, None, unchecked=why)
    ended = "one unchecked pass: " + why
elif way == 6:
    error = {"kind": "segment", "message": "the segmenter found nothing for the phrase"}
    kept = None
else:
    attempt("fail", found("fail", [count("RQ1", "violation", "confident")], complete=False))
    ended = "the findings of attempt 1 were cut short, and findings cut short never enlarge a mask"
record(top + "-find", "segment", top, **paid("segment", "pack", "sam3", digest("e"), 0))
loop = record(
    top, "remove_checked", None, plan=plan,
    time={"start_ms": 5000, "end_ms": 5000 + (way + 1) * 1000, "durations": []},
    spend={"reserved_micros": 458304, "requests": []},
)
loop["identity"]["request"] = request
loop["asked"]["arguments"] = [{"name": "prompt", "value": {"text": prompt}}] + (
    [{"name": "judge", "value": {"text": judge}}] if judge else [])
loop["asked"]["settings"] = [{"name": "job.template", "value": {"text": digest("d")}}]
loop["inputs"] = [{"port": "image", "type": "image", "artifact": image, "binding": image}]
if error:
    loop["outcome"].update(status="error", error=error)
else:
    loop["attempts"] = {"list": attempts, "kept": kept, "ended": ended}
    if attempts[kept - 1]["verdict"]:
        loop["findings"] = PASSES
os.makedirs(store, exist_ok=True)
with open(path, "a") as f:
    for one in made:
        f.write(json.dumps(one, separators=(",", ":")) + "\n")
if error:
    print("whizbang: the run %s ended in an error of the type segment" % top, file=sys.stderr)
    sys.exit(1)
print("plan: 3 of 3 attempts asked, checked; fill 3; judge 48", file=sys.stderr)
print("run %s completed\nattempts: %d made, attempt %d kept\nended: %s" % (top, len(attempts), kept, ended))
PY

# --- The set: 24 entries of `remove_prompt` among others, each picture a file of its own
# bytes under the hash the manifest pins, beside the truth masks and the clean pictures.
set_dir="$tmp/set"
mkdir -p "$set_dir/ball" "$set_dir/clean"
manifest="$tmp/edits.toml"
write_manifest() { # <how many entries of remove_prompt>
  python3 -B - "$set_dir" "$manifest" "$1" <<'PY'
import hashlib, os, sys

folder, manifest, entries = sys.argv[1], sys.argv[2], int(sys.argv[3])


def written(name, text):
    with open(os.path.join(folder, name), "w") as f:
        f.write(text)
    return "sha256:" + hashlib.sha256(text.encode()).hexdigest()


out = ['version = 1', 'set = "made-up/set-v1"', 'seed = 7', 'phrase = "a red ball"']
for i in range(1, 25):
    clean = written("clean/scene-%02d.png" % i, "clean %02d\n" % i)
    out += ['', '[[source]]', 'id = "scene-%02d"' % i, 'path = "items/scene-%02d/candidate.png"' % i,
            'sha256 = "sha256:%064d"' % 0, 'clean = "clean/scene-%02d.png"' % i, 'clean_sha256 = "%s"' % clean]
for recipe in ("remove", "remove_prompt"):
    for i in range(1, (entries if recipe == "remove_prompt" else 24) + 1):
        picture = written("ball/scene-%02d.png" % i, "picture %02d\n" % i)
        truth = written("ball/scene-%02d-truth.png" % i, "truth %02d\n" % i)
        out += ['', '[[item]]', 'id = "%s-scene-%02d"' % (recipe, i), 'recipe = "%s"' % recipe,
                'source = "scene-%02d"' % i, 'picture = "ball/scene-%02d.png"' % i, 'picture_sha256 = "%s"' % picture,
                'truth = "ball/scene-%02d-truth.png"' % i, 'truth_sha256 = "%s"' % truth, 'truth_pixels = 100',
                'phrase = "a red ball"' if recipe == "remove_prompt" else 'box = [1.0, 2.0, 3.0, 4.0]']
with open(manifest, "w") as f:
    f.write("\n".join(out) + "\n")
PY
}
write_manifest 24
store="$tmp/store"
out="$tmp/m4-loop-terra.toml"
evidence() { # <arguments of the script, after the store, the config and the env file>
  (cd "$repo" && WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh loop-evidence scripts/loop-evidence.sh \
    --binary "$tmp/bin/whizbang" --store "$store" --config "$tmp/config.toml" --env-file "$tmp/keys.env" "$@")
}
whole=(--set "$set_dir" --manifest "$manifest" --judge terra --out "$out" --allowance 8.00)
nothing_ran() { [ ! -e "$fix/commands" ] && [ ! -e "$store" ] && [ ! -e "$out" ] || fail "$1: something was imported, run or written"; }

# --- Without the heavy lock nothing starts; nor under another's lock, nor at another limit.
bare() { (cd "$repo" && "$@" scripts/loop-evidence.sh --binary "$tmp/bin/whizbang" --store "$store" --config "$tmp/config.toml" --env-file "$tmp/keys.env" "${whole[@]}"); }
expect_fail "without the heavy lock" "it is not under the heavy lock" bare env WHIZBANG_THREAD_LIMIT=200
nothing_ran "without the heavy lock"
mkdir "$tmp/lock" && echo 1 >"$tmp/lock/pid"
expect_fail "under a lock another process holds" "it is not under the heavy lock" bare env WHIZBANG_THREAD_LIMIT=200
rm -rf "$tmp/lock"
nothing_ran "under a lock another process holds"
expect_fail "with the guard at another limit" "must be 200" \
  bash -c 'cd "$1" && shift && WHIZBANG_THREAD_LIMIT=300 scripts/heavy.sh loop-evidence scripts/loop-evidence.sh "$@"' - "$repo" \
  --binary "$tmp/bin/whizbang" --store "$store" --config "$tmp/config.toml" --env-file "$tmp/keys.env" "${whole[@]}"
nothing_ran "with the guard at another limit"

# --- A set that is not the manifest's is refused before anything is imported or run.
mv "$set_dir/ball/scene-07.png" "$tmp/aside.png"
expect_fail "a picture missing" "the entry remove_prompt-scene-07: its picture is not in the folder" evidence "${whole[@]}"
nothing_ran "a picture missing"
echo "another picture" >"$set_dir/ball/scene-07.png"
expect_fail "a picture changed" "the entry remove_prompt-scene-07: its picture is not the one the manifest pins" evidence "${whole[@]}"
nothing_ran "a picture changed"
mv "$tmp/aside.png" "$set_dir/ball/scene-07.png"
write_manifest 25
expect_fail "a 25th entry" "holds 25 entries of remove_prompt, and the evidence is of exactly 24 (the last is remove_prompt-scene-25)" evidence "${whole[@]}"
nothing_ran "a 25th entry"
write_manifest 24
cp "$manifest" "$tmp/edits-good.toml"
# The ninth entry of remove_prompt names the truth mask of its picture, hash and all.
truth_sum=$(sha256 "$set_dir/ball/scene-09-truth.png")
awk -v sum="$truth_sum" '
  /^id = "remove_prompt-scene-09"$/ { mine = 1 }
  mine && /^picture = / { print "picture = \"ball/scene-09-truth.png\""; next }
  mine && /^picture_sha256 = / { print "picture_sha256 = \"sha256:" sum "\""; mine = 0; next }
  { print }' "$tmp/edits-good.toml" >"$manifest"
expect_fail "an entry that names a truth mask" "the entry remove_prompt-scene-09 names a truth mask" evidence "${whole[@]}"
nothing_ran "an entry that names a truth mask"
cp "$tmp/edits-good.toml" "$manifest"
# A binary that is not of this tree runs nothing either.
echo "whizbang 0.1.0 (commit $(printf 'a%.0s' $(seq 1 40)))" >"$fix/version-line"
expect_fail "a binary of another tree" "is not a commit of this checkout's history" evidence "${whole[@]}"
nothing_ran "a binary of another tree"
echo "whizbang 0.1.0 (commit $commit)" >"$fix/version-line"

# --- A run that stops at its 13th entry, because the call left no run, keeps the twelve.
echo 13 >"$fix/refuse-at"
expect_fail "a call that leaves no run" "a call left no run (status 1)" evidence "${whole[@]}"
[ "$(wc -l <"$fix/started" | tr -d ' ')" = 12 ] || fail "twelve entries ran before the one that left no run"
[ ! -e "$out" ] || fail "a run that stopped wrote a tally"
rm "$fix/refuse-at"
# The same command goes on from there: the twelve are not started again.
expect_pass "the same command again" evidence "${whole[@]}"
grep -F "started 12 runs of 24" "$tmp/out" >/dev/null || fail "the second run started the twelve that were left: $(cat "$tmp/out")"
# What it prints as it goes names no entry, no picture and no word of a judge.
! grep -E 'scene-|\.png|a red ball|MARMALADE|made-up-key' "$tmp/out" >/dev/null || fail "the script printed an entry, a picture, a phrase or a key"
expected=$(for i in $(seq -w 1 24); do echo "m4-loop-terra-remove_prompt-scene-$i"; done)
[ "$(cat "$fix/started")" = "$expected" ] || fail "each of the 24 is run once, in the manifest's order, under its request id"
[ ! -e "$fix/repeats" ] || fail "an entry the store held was asked again"
[ "$(cat "$fix/allowances")" = "spend reset --store $store --allowance 8.00" ] || fail "the store it made is given its allowance, once"
# Every command that loads a model ran under the watch on memory and the thread guard at
# 200; the imports and the exports load none and ran bare.
loops=$(grep -c '|rascal remove-checked ' "$fix/commands" || true)
[ "$loops" = 25 ] || fail "25 runs of the loop were asked, the one that left no run among them: $loops"
[ "$(grep -c '^memory-watch,thread-guard,|200|rascal remove-checked ' "$fix/commands" || true)" = 25 ] ||
  fail "a run of the loop was not under the watch on memory and the thread guard at 200"
[ -z "$(grep -v '|rascal remove-checked ' "$fix/commands" | grep -v '^|200|' || true)" ] || fail "an import or an export ran under a guard"
grep -F -- "--judge terra --request-id m4-loop-terra-remove_prompt-scene-01 --store $store --config $tmp/config.toml --env-file $tmp/keys.env" "$fix/commands" >/dev/null ||
  fail "a run names its judge, its request id, the store, the config and the env file"
cp "$out" "$tmp/first.toml"
cp "$fix/commands" "$tmp/commands-before"

# --- A second run of the whole script starts nothing again, and writes the same file.
expect_pass "a second run" evidence "${whole[@]}"
grep -F "started 0 runs of 24" "$tmp/out" >/dev/null || fail "a second run started something"
[ "$(wc -l <"$fix/started" | tr -d ' ')" = 24 ] && [ ! -e "$fix/repeats" ] || fail "a second run asked the binary for a run again"
[ -z "$(diff "$tmp/commands-before" "$fix/commands" | grep -E '^> ' | grep -v -F '|runs export ' || true)" ] ||
  fail "a second run asked for more than the store's export"
cmp -s "$out" "$tmp/first.toml" || fail "a second run wrote another tally"

# --- The tally: what the 24 runs came to, three of each of the stand-in's eight ways.
holds() { grep -x -F -e "$1" "$out" >/dev/null || { cat "$out" >&2; fail "the tally does not say: $1"; }; }
for line in 'schema = "loop-evidence/1"' "commit = \"$commit\"" 'whizbang = "0.1.0"' 'judge = "terra"' \
  'judge_model = "terra a-judge-model 2026-01"' 'filler = "fal bria-eraser 1+fit1"' \
  "finder = \"pack sam3 sha256:$(printf 'e%.0s' $(seq 1 64))\"" 'action = "remove_checked 1.0.0"' \
  "manifest_sha256 = \"sha256:$(sha256 "$manifest")\"" 'entries = 24' 'runs = 24' 'not_run = 0' 'attempts_made = 27' \
  'unsure_a_retry_under_rascals_rule_would_have_followed = 3' 'stopped_on_findings_not_complete = 3' \
  'median_ms = 4500' 'least_ms = 1000' 'most_ms = 8000' 'reserved_micros = 10999296' 'settled_micros = 1159200' \
  'settled_by_attempts_micros = 1159200' 'unresolved_micros = 0' \
  'completed = 21' 'error = 3' 'segment = 3' \
  'passed_at_first = 3' 'passed_after_retry = 6' 'kept_unsure = 3' 'kept_fail = 6' 'unchecked = 3' \
  'checked = 21' 'allowed_3 = 21' 'allowed_1 = 3' 'made_1 = 15' 'made_2 = 6' 'made_3 = 0' \
  'pass = 9' 'fail = 12' 'unsure = 3' 'wider = 3' 'located = 3' \
  'passed = 9' 'row_0_cut_short = 3' 'row_1_damaged = 3' 'row_2_remains = 0' 'row_3_not_sure = 3' 'not_checked = 3' 'no_attempt = 3' \
  'findings_cut_short = 3' 'damaged_something_preserved = 3' 'nothing_says_it_remains = 3' 'one_unchecked_pass = 3' 'other = 0' \
  'looked_into = 9' 'joined = 6' 'found_something = 3' 'found_nothing_joined_as_itself = 3' \
  'found_nothing_left_out_over_a_tenth = 3' 'cited_by_checks = 9' 'cited_as_polygons = 3' \
  'retries_wider_after_places_gave_nothing = 0' 'retries_wider_though_a_check_cited_a_place = 0'; do
  holds "$line"
done
# The findings, by obligation, disposition and confidence, over every check of every attempt.
findings=$(awk '/^\[\[finding\]\]$/ { getline o; getline d; getline c; getline n; print o " " d " " c " " n }' "$out")
[ "$findings" = 'obligation = "RQ1" disposition = "held" confidence = "confident" count = 9
obligation = "RQ1" disposition = "violation" confidence = "confident" count = 15
obligation = "RQ1" disposition = "violation" confidence = "tentative" count = 3
obligation = "SF1" disposition = "violation" confidence = "confident" count = 3' ] || fail "the findings are not counted as they were made: $findings"

# --- It holds aggregates and no item: every line is a comment, a table's name, a count, or
# one of the few lines that name a version, a model or a digest; and nothing of an entry,
# a picture, a path, a key or a judge's words is in it.
aggregates_only() { # <file> <what>
  local bad
  bad=$(awk '
    /^#/ || /^$/ { next }
    /^\[[a-z_]+\]$/ || /^\[\[(finding|attempt)\]\]$/ { next }
    /^[a-z0-9_]+ = [0-9]+$/ || /^[a-z0-9_]+ = (true|false)$/ { next }
    /^(schema|commit|whizbang|judge|judge_model|filler|finder|action|job_template|config|manifest_sha256) = "[A-Za-z0-9 ._:,+\/-]+"$/ { next }
    /^(obligation|disposition|confidence) = "[A-Za-z0-9_]+"$/ { next }
    { print }' "$1")
  [ -z "$bad" ] || fail "$2 holds a line that is not a count, a version, a model or a digest: $bad"
}
aggregates_only "$out" "the tally"
no_item() { # <file> <what>
  local held
  for held in scene- remove_prompt- ball/ .png "a red ball" "$tmp" /Users/ /home/ /private/ made-up-key FAL_KEY MARMALADE keys.env \
    "$(printf 'f%.0s' $(seq 1 64))" "$(printf '1%.0s' $(seq 1 64))"; do
    ! grep -F -e "$held" "$1" >/dev/null || fail "$2 holds $held"
  done
}
no_item "$out" "the tally"

# --- The three cases by hand, through the same script, each under the same guards.
store="$tmp/cases"
: >"$fix/commands"
: >"$fix/started"
for one in "pass 0 a red ball" "vague 6 a thing that is not there" "no-key 5 a red ball"; do
  # shellcheck disable=SC2086 # split on purpose: a name, a way, then the phrase.
  set -- $one
  name=$1
  echo "$2" >"$fix/kind"
  shift 2
  expect_pass "the case $name" evidence --case "$name" --image "$set_dir/ball/scene-03.png" --prompt "$*" --out "$tmp/case-$name.toml" --allowance 5.00
  no_item "$tmp/case-$name.toml" "the case $name"
done
rm "$fix/kind"
[ "$(cat "$fix/started")" = "m4-loop-case-pass
m4-loop-case-vague
m4-loop-case-no-key" ] || fail "each case is run once under its request id"
[ "$(grep -c '^memory-watch,thread-guard,|200|rascal remove-checked ' "$fix/commands" || true)" = 3 ] ||
  fail "a case was not under the watch on memory and the thread guard at 200"
grep -F -- "--prompt a thing that is not there --request-id m4-loop-case-vague" "$fix/commands" >/dev/null || fail "a case is asked its phrase, with the config's own judge"
said() { grep -x -F -e "$2" "$tmp/case-$1.toml" >/dev/null || { cat "$tmp/case-$1.toml" >&2; fail "the case $1 does not say: $2"; }; }
said pass 'outcome = "completed"'
said pass 'attempts = 1'
said pass 'verdict = "pass"'
said pass 'ended = "attempt 1 passed"'
said pass 'settled_micros = 43300'
said vague 'outcome = "error"'
said vague 'error_type = "segment"'
said vague 'attempts = 0'
said vague 'verdict = "none"'
said vague 'ended_as = "error"'
said no-key 'checked = false'
said no-key 'plan_checks = false'
said no-key "unchecked_why = \"no judge is ready: the backend's key is not in the env file\""
said no-key 'ended_as = "unchecked"'
# A case asked again is answered by the store, and nothing is started.
expect_pass "a case again" evidence --case pass --image "$set_dir/ball/scene-03.png" --prompt "a red ball" --out "$tmp/case-again.toml"
[ "$(wc -l <"$fix/started" | tr -d ' ')" = 3 ] || fail "a case asked again was started again"
cmp -s "$tmp/case-pass.toml" "$tmp/case-again.toml" || fail "a case asked again wrote another file"

# --- The page: the two tallies and the three cases, and still no item.
sed 's/^judge = "terra"$/judge = "haiku"/' "$out" >"$tmp/m4-loop-haiku.toml"
expect_pass "the page" python3 -B scripts/loop-evidence.py page --tally "$tmp/m4-loop-haiku.toml" --tally "$out" \
  --case "$tmp/case-pass.toml" --case "$tmp/case-vague.toml" --case "$tmp/case-no-key.toml" --out "$tmp/m4-loop.md"
for held in '## The 24 entries under `haiku`' '## The 24 entries under `terra`' '### `pass`' '### `vague`' '### `no-key`' \
  "commits = \"$commit\"" "settled_micros = \"$((1159200 * 2 + 43300 + 0 + 40000))\"" 'passed_after_retry = 6'; do
  grep -F -e "$held" "$tmp/m4-loop.md" >/dev/null || fail "the page does not hold: $held"
done
no_item "$tmp/m4-loop.md" "the page"
expect_fail "a page of one tally" "two tallies and three cases" python3 -B scripts/loop-evidence.py page --tally "$out" \
  --case "$tmp/case-pass.toml" --case "$tmp/case-vague.toml" --case "$tmp/case-no-key.toml" --out "$tmp/short.md"
[ -z "$(find scripts -name __pycache__ -o -name '*.pyc')" ] || fail "the tally left bytecode in the repository"
# The tally imports nothing outside the standard library.
others=$(grep -h -E '^ *(import|from) +[A-Za-z0-9_]+( |$)' scripts/loop-evidence.py |
  sed -E 's/^ *(import|from) +([A-Za-z0-9_]+).*/\2/' | sort -u | grep -v -x -E 'argparse|hashlib|json|os|re|sys|tomllib' || true)
[ -z "$others" ] || fail "scripts/loop-evidence.py imports outside the standard library: $others"

# --- segment-01: a manifest of 20 entries in four groups, a picture and a truth mask
# each, laid out as the repository is.
seg="$tmp/segment"
seg_manifest="$tmp/segment-01.toml"
write_segment() { # <how many entries> [<the id whose picture is its truth mask>]
  python3 -B - "$seg" "$seg_manifest" "$1" "${2:-}" <<'PY2'
import hashlib, os, sys

folder, manifest, entries, truthy = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
groups = [("whole", 5), ("parts", 6), ("thin", 5), ("keep-the-hole", 4)]
names = [(g, "%s-%02d" % (g.split("-")[0], i)) for g, n in groups for i in range(1, n + 1)]
while len(names) < entries:
    names.append(("whole", "whole-%02d" % (len(names) + 1)))
out = ['kind = "segment-01"', 'set = "segmentation/segment-01"',
       'images_repo = "github.com/example/pictures"', 'images_rev = "%s"' % ("a" * 40)]


def written(path, text):
    full = os.path.join(folder, "segmentation/segment-01", path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w") as f:
        f.write(text)
    return "sha256:" + hashlib.sha256(text.encode()).hexdigest()


for group, name in names[:entries]:
    image = written("items/%s/image.png" % name, "picture of %s\n" % name)
    truth = written("items/%s/truth.png" % name, "truth of %s\n" % name)
    picture = "items/%s/image.png" % name
    if name == truthy:
        picture, image = "items/%s/truth.png" % name, truth
    out += ['', '[[item]]', 'id = "%s"' % name, 'group = "%s"' % group, 'phrase = "a thing in %s"' % name,
            'image = "%s"' % picture, 'image_sha256 = "%s"' % image,
            'truth = "items/%s/truth.png"' % name, 'truth_sha256 = "%s"' % truth]
with open(manifest, "w") as f:
    f.write("\n".join(out) + "\n")
PY2
}
store="$tmp/e-loop"
out="$tmp/e-loop-terra-1.toml"
segment=(--set "$seg" --manifest "$seg_manifest" --judge terra --out "$out" --allowance 25.00)
: >"$fix/commands"
rm -f "$fix/started" "$fix/repeats" "$fix/planned"
none_started() { [ ! -e "$fix/started" ] && [ ! -e "$out" ] || fail "$1: a run was started or a tally written"; }
# An entry that names a truth mask, by path and by pin, is refused before anything is
# imported or run; so is a manifest of segment-01 without a repetition.
write_segment 20 thin-03
expect_fail "segment-01: an entry that names a truth mask" "the entry thin-03 names a truth mask" evidence "${segment[@]}" --repetition 1
[ ! -e "$store" ] && [ ! -s "$fix/commands" ] || fail "segment-01: something was imported for a manifest that names a truth mask"
write_segment 20
expect_fail "segment-01 without a repetition" "needs --repetition, 1 to 9" evidence "${segment[@]}"
[ ! -e "$store" ] || fail "segment-01: something was imported without a repetition"
# A cap that plans fewer than three attempts, or none checked, stops before any run.
for plan in "plan: 2 of 3 attempts asked, checked; segment 9; fill 2; judge 32; the call cap covers 2 of the 3 attempts asked" \
  "plan: 1 of 3 attempts asked, unchecked; fill 1; judge 0; the call cap does not cover a check"; do
  printf '%s\n' "$plan" >"$fix/plan"
  expect_fail "segment-01: a plan of fewer than three checked attempts" "the plan is not three checked attempts of three" evidence "${segment[@]}" --repetition 1
  none_started "segment-01: a plan of fewer than three checked attempts"
done
[ "$(wc -l <"$fix/planned" | tr -d ' ')" = 2 ] || fail "segment-01: each refused run prepared one call"
grep -F -- "--plan-only --image sha256:" "$fix/commands" | grep -F -- "--judge terra" >/dev/null ||
  fail "segment-01: the call prepared names the judge"
[ "$(grep -c '^memory-watch,thread-guard,|200|rascal remove-checked --plan-only ' "$fix/commands" || true)" = 2 ] ||
  fail "segment-01: the call prepared was not under the guards"
rm "$fix/plan"
# With three checked attempts planned, the 20 are run, each once, under its request id
# with the repetition in it.
rm -f "$fix/planned"
expect_pass "segment-01, repetition 1" evidence "${segment[@]}" --repetition 1
grep -F "started 20 runs of 20" "$tmp/out" >/dev/null || fail "segment-01: the 20 were not run: $(cat "$tmp/out")"
[ "$(wc -l <"$fix/planned" | tr -d ' ')" = 1 ] || fail "segment-01: one call was prepared before the runs"
expected=$(python3 -B - "$seg_manifest" <<'PY2'
import re, sys
for name in re.findall(r'^id = "([^"]+)"$', open(sys.argv[1]).read(), re.M):
    print("e-loop-terra-1-%s" % name)
PY2
)
[ "$(cat "$fix/started")" = "$expected" ] || fail "segment-01: each of the 20 is run once, in the manifest's order, under e-loop-<judge>-<repetition>-<id>"
grep -F 'segmentation/segment-01/items/whole-01/image.png' "$fix/commands" >/dev/null || fail "segment-01: the picture imported is the entry's image.png"
! grep -F 'truth.png' "$fix/commands" >/dev/null || fail "segment-01: a truth mask was handed to the binary"
for line in 'kind = "segment-01"' 'repetition = 1' 'entries = 20' 'runs = 20' 'not_run = 0' 'judge = "terra"'; do
  holds "$line"
done
by_group=$(awk '/^\[group\./ { g = $0 } g && /^runs = / { print g " " $0 }' "$out")
[ "$by_group" = '[group.keep-the-hole] runs = 4
[group.parts] runs = 6
[group.thin] runs = 5
[group.whole] runs = 5' ] || fail "segment-01: the runs are not tallied by group: $by_group"
bad=$(awk '
  /^#/ || /^$/ { next }
  /^\[[a-z_]+\]$/ || /^\[group\.[a-z0-9-]+\]$/ || /^\[\[finding\]\]$/ { next }
  /^[a-z0-9_]+ = [0-9]+$/ { next }
  /^(schema|commit|whizbang|judge|kind|judge_model|filler|finder|action|job_template|config|manifest_sha256) = "[A-Za-z0-9 ._:,+\/-]+"$/ { next }
  /^(obligation|disposition|confidence) = "[A-Za-z0-9_]+"$/ { next }
  { print }' "$out")
[ -z "$bad" ] || fail "segment-01: the tally holds a line that is not a count, a version, a model or a digest: $bad"
for held in whole-0 parts-0 thin-0 hole-0 "a thing in" image.png truth.png "$tmp" MARMALADE made-up-key; do
  ! grep -F -e "$held" "$out" >/dev/null || fail "segment-01: the tally holds $held"
done
# The second repetition is a run of its own under each entry, and a run of the script again
# starts nothing and prepares nothing.
out="$tmp/e-loop-terra-2.toml"
segment=(--set "$seg" --manifest "$seg_manifest" --judge terra --out "$out")
expect_pass "segment-01, repetition 2" evidence "${segment[@]}" --repetition 2
grep -F "started 20 runs of 20" "$tmp/out" >/dev/null || fail "segment-01: repetition 2 did not run its 20"
[ "$(grep -c '^e-loop-terra-2-' "$fix/started")" = 20 ] || fail "segment-01: repetition 2 has its own request ids"
expect_pass "segment-01, repetition 2 again" evidence "${segment[@]}" --repetition 2
grep -F "started 0 runs of 20" "$tmp/out" >/dev/null || fail "segment-01: a second run started something"
[ "$(wc -l <"$fix/planned" | tr -d ' ')" = 2 ] || fail "segment-01: a run with nothing left to do prepared a call"
[ ! -e "$fix/repeats" ] || fail "segment-01: an entry the store held was asked again"
# A repetition given for the edit set is refused.
expect_fail "a repetition for the edit set" "--repetition is for a manifest of the kind segment-01" evidence --set "$set_dir" --manifest "$manifest" --judge terra --out "$tmp/x.toml" --repetition 1

# --- Once through the real lock and the real guards: the command that loads a model has
# the watch on memory and the thread guard above it, and the guard's limit is 200.
real="$tmp/real"
mkdir -p "$real/scripts"
cp scripts/loop-evidence.sh scripts/loop-evidence.py scripts/heavy.sh scripts/memory-watch.sh scripts/thread-guard.sh "$real/scripts/"
git init -q "$real"
git -C "$real" add -A
git -C "$real" -c user.name=test -c user.email=test@example.invalid commit -q -m "the tree, with the real guards"
echo "whizbang 0.1.0 (commit $(git -C "$real" rev-parse HEAD))" >"$fix/version-line"
: >"$fix/ancestors"
(cd "$real" && WHIZBANG_HEAVY_LOCK="$tmp/real.lock" WHIZBANG_HEAVY_POLL=0.2 WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh loop-evidence scripts/loop-evidence.sh \
  --case real --image "$set_dir/ball/scene-03.png" --prompt "a red ball" --binary "$tmp/bin/whizbang" --store "$tmp/real-store" \
  --config "$tmp/config.toml" --env-file "$tmp/keys.env" --out "$tmp/case-real.toml") >"$tmp/out" 2>&1 || { cat "$tmp/out" >&2; fail "the case through the real guards failed"; }
grep -F "$real/scripts/memory-watch.sh" "$fix/above" >/dev/null || { cat "$fix/above" >&2; fail "the real run of the loop had no watch on memory above it"; }
grep -F "$real/scripts/thread-guard.sh" "$fix/above" >/dev/null || fail "the real run of the loop had no thread guard above it"
grep -E '^guard=[1-9][0-9]* 200 [0-9.]+$' "$fix/above" >/dev/null || { cat "$fix/above" >&2; fail "the thread guard above the real run was not at 200"; }
[ ! -e "$tmp/real.lock" ] || fail "the lock was left behind"
# And without that lock the same command starts nothing.
: >"$fix/commands"
(cd "$real" && WHIZBANG_HEAVY_LOCK="$tmp/real.lock" WHIZBANG_THREAD_LIMIT=200 scripts/thread-guard.sh scripts/loop-evidence.sh \
  --case again --image "$set_dir/ball/scene-03.png" --prompt "a red ball" --binary "$tmp/bin/whizbang" --store "$tmp/real-store" \
  --config "$tmp/config.toml" --env-file "$tmp/keys.env") >"$tmp/out" 2>&1 && fail "under the guard alone, without the lock, it started"
grep -F "it is not under the heavy lock" "$tmp/out" >/dev/null && [ ! -s "$fix/commands" ] || fail "under the guard alone something ran"

finished
