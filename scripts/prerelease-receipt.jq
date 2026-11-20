# The receipt of the local leg, `prerelease-receipt/1` (plan m4, task R2, item 4): its
# shape, and what it must say for a release to be allowed. scripts/receipt-check.sh runs
# this program; scripts/prerelease-local.sh writes the receipt and scripts/release-gate.sh
# reads it. Written for jq 1.6, which the Linux runner has.
#
# The input is the receipt. The output is texts, one a line, each one thing that is
# wrong; nothing printed is a receipt that passes. A text starts with `shape: ` when the
# file is not a receipt at all (a field missing, a field too many, a value of another
# type), and with `check: ` when it is a receipt that records something a release is
# refused for. When the shape is wrong nothing else is reported: the rest is not read.
#
# The shape, with no field beyond it:
#
#   schema      "prerelease-receipt/1"
#   tag, version, candidate     the candidate tag, its base version and its number
#   commit      the commit the tag names
#   archives    the SHA-256 of each target's archive, by the target's name
#   binary      sha256: of the installed binary; version_line: what `--version` printed
#   store       new: it was made by the leg and held no run; cache: "off" or "on"
#   checks      one entry for each required check, each with a `name` and an `exit`
#               (the status of the command; null when it was not run):
#
#   model-tests       checkout {new, clean, commit, tree}; expected, ran, failed and
#                     timed_out, lists of test names; report_sha256
#   setup             extra; hello {device, threads}; probe_hello
#   mask              run, status; backend, model, model_version, device
#   fill              run, status; backend, model, model_version; cache_hit; requests
#                     [{backend, ended, stage, settled_micros}]; sent_to [origins];
#                     tokens; settled_micros
#   check-anthropic, check-google, check-openai
#                     as fill, and verdict
#   loop              run, status; plan_checks; kept {attempt, verdict, unchecked};
#                     check {run, status, verdict, cache_hit, requests, sent_to}
#
# It holds no reply text, no path and no key: every text is a name, a digest, a version,
# an origin or one of a closed set of words.
#
# Given with --arg, each held when it is not empty: `tag` and `commit`, which the receipt
# must name; `mac`, `linux` and `installed`, the digests taken again from the candidate's
# bytes. `shape_only`, given with --argjson: 1 reports the shape and nothing else.

def sha: type == "string" and test("^[0-9a-f]{64}$");
def commit_name: type == "string" and test("^([0-9a-f]{40}|[0-9a-f]{64})$");
def word: type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._+:/-]{0,199}$");
def words: type == "array" and length <= 512 and all(.[]; type == "string" and test("^[A-Za-z0-9_:]{1,300}$"));
def count: type == "number" and . >= 0 and . == floor;
def nullable(f): . == null or f;
def bool: type == "boolean";
def verdict: . == "pass" or . == "fail" or . == "unsure";
def has_only($names): type == "object" and (keys == ($names | sort));
def requests: type == "array" and length <= 64 and all(.[];
  has_only(["backend", "ended", "stage", "settled_micros"]) and (.backend | word) and (.ended | word) and (.stage | word) and (.settled_micros | count));
def origins: type == "array" and length <= 64 and all(.[]; type == "string" and test("^https://[a-z0-9.-]{1,200}$"));
def tokens: has_only(["input", "output"]) and (.input | count) and (.output | count);
def paid_fields: ["name", "exit", "run", "status", "backend", "model", "model_version", "cache_hit", "requests", "sent_to", "tokens", "settled_micros"];
def paid_shape:
  (.exit | nullable(count)) and (.run | nullable(word)) and (.status | nullable(word)) and (.backend | nullable(word))
  and (.model | nullable(word)) and (.model_version | nullable(word)) and (.cache_hit | nullable(bool)) and (.requests | requests)
  and (.sent_to | origins) and (.tokens | nullable(tokens)) and (.settled_micros | nullable(count));

def providers: {"check-anthropic": ["anthropic", "https://api.anthropic.com"], "check-google": ["google", "https://generativelanguage.googleapis.com"], "check-openai": ["openai", "https://api.openai.com"], "fill": ["fal", "https://fal.run"]};
def required: ["model-tests", "setup", "mask", "fill", "check-anthropic", "check-google", "check-openai", "loop"];

# shape_of: whether one entry of `checks` is of its name's shape.
def shape_of:
  if type != "object" or (.name | type) != "string" then false
  elif .name == "model-tests" then
    has_only(["name", "exit", "checkout", "expected", "ran", "failed", "timed_out", "report_sha256"])
    and (.exit | nullable(count)) and (.checkout | has_only(["new", "clean", "commit", "tree"]))
    and (.checkout | (.new | bool) and (.clean | bool) and (.commit | nullable(commit_name)) and (.tree | nullable(commit_name)))
    and (.expected | words) and (.ran | words) and (.failed | words) and (.timed_out | words) and (.report_sha256 | nullable(sha))
  elif .name == "setup" then
    has_only(["name", "exit", "extra", "hello", "probe_hello"]) and (.exit | nullable(count)) and (.extra | nullable(word))
    and (.hello | nullable(has_only(["device", "threads"]) and (.device | word) and (.threads | count))) and (.probe_hello | bool)
  elif .name == "mask" then
    has_only(["name", "exit", "run", "status", "backend", "model", "model_version", "device"]) and (.exit | nullable(count))
    and all(.run, .status, .backend, .model, .model_version, .device; nullable(word))
  elif .name == "fill" then has_only(paid_fields) and paid_shape
  elif .name == "check-anthropic" or .name == "check-google" or .name == "check-openai" then
    has_only(paid_fields + ["verdict"]) and paid_shape and (.verdict | nullable(verdict))
  elif .name == "loop" then
    has_only(["name", "exit", "run", "status", "plan_checks", "kept", "check"]) and (.exit | nullable(count)) and (.run | nullable(word))
    and (.status | nullable(word)) and (.plan_checks | nullable(bool))
    and (.kept | nullable(has_only(["attempt", "verdict", "unchecked"]) and (.attempt | count) and (.verdict | nullable(verdict)) and (.unchecked | nullable(word))))
    and (.check | nullable(has_only(["run", "status", "verdict", "cache_hit", "requests", "sent_to"]) and (.run | word) and (.status | nullable(word))
      and (.verdict | nullable(verdict)) and (.cache_hit | nullable(bool)) and (.requests | requests) and (.sent_to | origins)))
  else false end;

def shape:
  if type != "object" then ["shape: the receipt is not one object"]
  elif (keys != (["schema", "tag", "version", "candidate", "commit", "archives", "binary", "store", "checks"] | sort)) then
    ["shape: the receipt must hold exactly: schema, tag, version, candidate, commit, archives, binary, store, checks"]
  else
    [ (if .schema != "prerelease-receipt/1" then "shape: schema must be prerelease-receipt/1" else empty end),
      (if (.tag | type == "string" and test("^v(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)-rc\\.[1-9][0-9]*$")) then empty else "shape: tag must be a candidate tag" end),
      (if (.version | type == "string" and test("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$")) then empty else "shape: version must be three numbers" end),
      (if (.candidate | count and . >= 1) then empty else "shape: candidate must be a whole number, 1 or more" end),
      (if (.commit | commit_name) then empty else "shape: commit must be a commit's whole name" end),
      (if (.archives | has_only(["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]) and all(.[]; sha)) then empty
       else "shape: archives must hold the SHA-256 of each of the two targets' archives, and nothing else" end),
      (if (.binary | has_only(["sha256", "version_line"]) and (.sha256 | sha) and (.version_line | type == "string" and test("^whizbang [0-9.]{1,40} \\(commit [0-9a-z]{1,64}\\)$"))) then empty
       else "shape: binary must hold sha256 and version_line, and nothing else" end),
      (if (.store | has_only(["new", "cache"]) and (.new | bool) and (.cache == "off" or .cache == "on")) then empty
       else "shape: store must hold new and cache, and nothing else" end),
      (if (.checks | type) != "array" or (.checks | length) > 32 then "shape: checks must be a list"
       else (.checks | to_entries[] | select(.value | shape_of | not)
         | "shape: entry \(.key + 1) of checks is not of the shape of a check this schema knows, or holds a field beyond it") end)
    ]
  end;

# answered($backend; $origin): a request recorded as dispatched to that provider and
# settled with an answer, and the provider's origin among those sent to.
def answered($backend; $origin):
  (.requests | any(.[]; .backend == $backend and .ended == "answered" and .stage == "answered")) and (.sent_to | any(.[]; . == $origin));

# passed: for one entry, the texts of what it records that a release is refused for.
def passed($receipt):
  .name as $name
  | if $name == "model-tests" then
      (if .checkout.new and .checkout.clean then empty else "check: model-tests: its checkout is not recorded as made new by the leg and found clean" end),
      (if .checkout.commit == $receipt.commit then empty else "check: model-tests: its checkout is of another commit than the candidate's" end),
      (if .checkout.tree == null then "check: model-tests: its checkout names no tree" else empty end),
      (if .exit == 0 then empty else "check: model-tests: the test run's exit status is not 0" end),
      (if .report_sha256 == null then "check: model-tests: there is no test report" else empty end),
      (if (.expected | length) == 0 then "check: model-tests: the filter named no test" else empty end),
      (if (.failed | length) == 0 then empty else "check: model-tests: the report holds a failed test" end),
      (if (.timed_out | length) == 0 then empty else "check: model-tests: the report holds a timed-out test" end),
      (if ((.expected | unique) - (.ran | unique) | length) == 0 then empty else "check: model-tests: the report lacks a test the filter names" end),
      (if ((.ran | unique) - (.expected | unique) | length) == 0 and (.ran | length) == (.ran | unique | length) then empty
       else "check: model-tests: the report holds a test the filter does not name, or one twice" end)
    elif $name == "setup" then
      (if .exit == 0 then empty else "check: setup: its exit status is not 0" end),
      (if .extra == "torch" then empty else "check: setup: it did not install the extra torch" end),
      (if .hello == null then "check: setup: it printed no hello of the worker" else empty end),
      (if .probe_hello then empty else "check: setup: the probe heard no hello of the installed pack's worker" end)
    elif $name == "mask" then
      (if .exit == 0 and .status == "completed" and .run != null then empty else "check: mask: its run did not complete" end),
      (if .backend == "pack" and .model == "sam3" and .model_version != null and .device != null then empty
       else "check: mask: it does not name SAM 3 on the pack, with its version and its device" end)
    elif $name == "loop" then
      (if .exit == 0 and .status == "completed" and .run != null then empty else "check: loop: its run did not complete" end),
      (if .plan_checks == true then empty else "check: loop: its plan did not check" end),
      (if .kept != null and .kept.verdict != null and .kept.unchecked == null then empty else "check: loop: it completed unchecked: the kept attempt has no verdict" end),
      (if .check != null and .check.status == "completed" and .check.verdict != null then empty else "check: loop: its child check did not complete with a verdict" end),
      (if .check != null and .check.cache_hit == false and (.check as $check | any(providers["check-anthropic", "check-google", "check-openai"]; . as $p | $check | answered($p[0]; $p[1]))) then empty
       else "check: loop: its child check names no dispatched and settled request, or was answered from a cache" end)
    else
      providers[$name] as $p
      | (if .exit == 0 and .status == "completed" and .run != null then empty else "check: \($name): its run did not complete" end),
        (if .backend == $p[0] and .model != null and .model_version != null then empty else "check: \($name): it does not name the backend \($p[0]), its model and its version" end),
        (if .cache_hit == false then empty else "check: \($name): it was answered from a cache, or does not say" end),
        (if answered($p[0]; $p[1]) then empty else "check: \($name): it names no request dispatched to \($p[1]) and settled with an answer" end),
        (if .settled_micros == null then "check: \($name): it does not say what was settled" else empty end),
        (if $name == "fill" then empty
         else (if .verdict == null then "check: \($name): it has no verdict" else empty end),
              (if .tokens == null then "check: \($name): it does not say its tokens" else empty end) end)
    end;

def conditions:
  . as $receipt
  | [ (if .tag == "v\(.version)-rc.\(.candidate)" then empty else "check: the tag is not the version and the candidate number the receipt names" end),
      (if .binary.version_line == "whizbang \(.version) (commit \(.commit))" then empty else "check: the installed binary's version line is not the candidate's version and commit" end),
      (if .store.new then empty else "check: the store was not new" end),
      (if .store.cache == "off" then empty else "check: the decision cache was on" end),
      (required[] as $name | ($receipt.checks | map(select(.name == $name)) | length) as $n
        | if $n == 1 then empty elif $n == 0 then "check: the required check \($name) is missing" else "check: the required check \($name) is there \($n) times" end),
      (.checks[] | passed($receipt)),
      (if $tag != "" and .tag != $tag then "check: the receipt names another candidate than \($tag)" else empty end),
      (if $commit != "" and .commit != $commit then "check: the receipt names another commit than the tag's" else empty end),
      (if $mac != "" and .archives["aarch64-apple-darwin"] != $mac then "check: the receipt's digest of the aarch64-apple-darwin archive is not that of the candidate's bytes" else empty end),
      (if $linux != "" and .archives["x86_64-unknown-linux-gnu"] != $linux then "check: the receipt's digest of the x86_64-unknown-linux-gnu archive is not that of the candidate's bytes" else empty end),
      (if $installed != "" and .binary.sha256 != $installed then "check: the receipt's digest of the installed binary is not that of the binary in the candidate's archive" else empty end)
    ];

# verdict_of($shape_only): what is wrong with the receipt; with 1, with its shape alone.
def verdict_of($shape_only): shape as $wrong | if ($wrong | length) > 0 or $shape_only == 1 then $wrong else conditions end;

# The file is read with `jq -s`: one value and nothing after it.
if length != 1 then ["shape: the file must hold one JSON value"] else (.[0] | verdict_of($shape_only)) end | .[]
