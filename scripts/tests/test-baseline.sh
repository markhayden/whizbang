#!/usr/bin/env bash
# The baseline scripts (scripts/baseline/): their unit tests on a synthetic package, and the
# things that must stay true between them, the comparison in whizbang-eval and the public
# record. Builds nothing and needs no network.
source "$(dirname "$0")/lib.sh"

command -v python3 >/dev/null || fail "python3 is required (3.9 or later, standard library only)"
python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)' \
  || fail "python3 is older than 3.9"

# The unit tests. -B: leave no bytecode behind in the repository.
expect_pass "the unit tests" python3 -B -m unittest -q scripts/baseline/tests/test_baseline.py
expect_pass "the unit tests of the tables" python3 -B -m unittest -q scripts/baseline/tests/test_tables.py
[ -z "$(find scripts/baseline -name __pycache__ -o -name '*.pyc')" ] \
  || fail "the baseline scripts left bytecode in the repository"

# The scripts import nothing outside the standard library and their own folder.
# An import inside a function counts too.
others=$(grep -h -E '^ *(import|from) +[A-Za-z0-9_]+( |$)' scripts/baseline/*.py scripts/baseline/tests/*.py \
  | sed -E 's/^ *(import|from) +([A-Za-z0-9_]+).*/\2/' | sort -u \
  | grep -v -x -E 'argparse|collections|contextlib|copy|hashlib|io|json|os|re|shutil|sys|tempfile|tomllib|unittest|common|convert|audit|tables' || true)
[ -z "$others" ] || fail "scripts/baseline imports outside the standard library: $others"

# The converter and the comparison declare the same fields volatile.
quoted() { grep -o -E '"[^"]*"' | tr '\n' ' '; }
py=$(grep -E '^VOLATILE = ' scripts/baseline/common.py | quoted)
rs=$(awk '/^pub const VOLATILE/,/\];/' crates/whizbang-eval/src/baseline.rs | quoted)
[ -n "$py" ] || fail "common.py has no VOLATILE line"
[ "$py" = "$rs" ] || fail "the volatile lists differ: common.py has $py, baseline.rs has $rs"

# The public record holds counts, versions and digests, and nothing else.
record=docs/baseline/audit.toml
[ -f "$record" ] || fail "$record is missing"
bad=$(awk '
  /^#/ || /^$/ { next }
  /^\[[a-z_]+\]$/ { next }
  /^[a-z_]+ = [0-9]+$/ { next }
  /^tools_version = "[0-9.]+"$/ { next }
  /^(package_digest|transcripts_digest) = "sha256:[0-9a-f]+"$/ { next }
  { print }' "$record")
[ -z "$bad" ] || fail "$record holds a line that is not a count, a version or a digest: $bad"
fixtures=$(grep -E '^fixtures = ' "$record" | sed -E 's/^fixtures = //')
decisions=$(grep -E '^decisions = ' "$record" | sed -E 's/^decisions = //')
complete=$(grep -E '^complete = ' "$record" | sed -E 's/^complete = //')
[ -n "$fixtures" ] && [ "$fixtures" = "$decisions" ] && [ "$fixtures" = "$complete" ] \
  || fail "$record: fixtures, decisions and complete are not one number"
arm_c=$(awk '/^\[\[cell\]\]/ { arm = "" } /^arm = "C"/ { arm = "C" } /^decisions = / && arm == "C" { n += $3 } END { print n }' docs/baseline/manifest.toml)
[ "$fixtures" = "$arm_c" ] || fail "$record: $fixtures fixtures, and manifest.toml counts $arm_c decisions for the replay arm"
grep -q -E '^tools_version = "'"$(grep -E '^TOOLS_VERSION = ' scripts/baseline/common.py | grep -o -E '[0-9.]+' | head -1)"'"$' "$record" \
  || fail "$record was not written by this version of the scripts"
grep -q -E '^incomplete = 0$' "$record" || fail "$record: the baseline has incomplete decisions"
grep -q -E '^invalid = 0$' "$record" || fail "$record: the baseline has invalid transcripts"
manifest=$(grep -E '^digest = ' docs/baseline/manifest.toml | sed -E 's/^digest = //')
audited=$(grep -E '^package_digest = ' "$record" | sed -E 's/^package_digest = //')
[ "$manifest" = "$audited" ] || fail "$record audits another package than manifest.toml names"

# The study's write-up (docs/results/): its tables and the figures in its prose are the
# numbers file's, and the numbers file's counts and spend are the manifest's.
study=docs/results/2026-11-tool-study
[ -f "$study.toml" ] && [ -f "$study.md" ] || fail "$study.toml or $study.md is missing"
expect_pass "the study's document against its numbers" \
  python3 -B scripts/baseline/tables.py check "$study.toml" "$study.md" --manifest docs/baseline/manifest.toml
sed -E 's/\| 97\.3% \(71 of 73\)/| 98.3% (71 of 73)/' "$study.md" >"$tmp/edited.md"
cmp -s "$study.md" "$tmp/edited.md" && fail "the study's document has no line this test can change"
expect_fail "a changed figure in a table" "generated block or value differs" \
  python3 -B scripts/baseline/tables.py check "$study.toml" "$tmp/edited.md"
printf '\nThe tool added 12.3%% to the catch rate.\n' | cat "$study.md" - >"$tmp/edited.md"
expect_fail "a figure in the prose that the numbers do not hold" "the prose states 12.3%" \
  python3 -B scripts/baseline/tables.py check "$study.toml" "$tmp/edited.md"
sed -E 's/(<!-- =meta\/errors -->)41(<!-- \/= -->)/\140\2/' "$study.md" >"$tmp/edited.md"
cmp -s "$study.md" "$tmp/edited.md" && fail "the study's document has no value this test can change"
expect_fail "a changed value in a sentence" "generated block or value differs" \
  python3 -B scripts/baseline/tables.py check "$study.toml" "$tmp/edited.md"
printf '\nIt gained 47 items.\n' | cat "$study.md" - >"$tmp/edited.md"
expect_fail "a count written by hand in the prose" "the prose counts 47 by hand" \
  python3 -B scripts/baseline/tables.py check "$study.toml" "$tmp/edited.md"
grep -q -E '^tables_version = "'"$(grep -E '^TABLES_VERSION = ' scripts/baseline/tables.py | grep -o -E '[0-9.]+' | head -1)"'"$' "$study.toml" \
  || fail "$study.toml was not written by this version of tables.py"
# The numbers file holds aggregates: a number, a truth value, or a name from the lists held
# here. A new run, kind of error or class of error is added here on purpose or not at all.
bad=$(awk '
  /^#/ || /^$/ { next }
  /^\[\[(run|cell|effect|sealed_effect|sensitivity|repeat|error|error_kind)\]\]$/ { next }
  /^[a-z_0-9]+ = -?[0-9]+(\.[0-9]+)?$/ { next }
  /^[a-z_]+ = (true|false)$/ { next }
  /^study = "2026-11-tool-study"$/ { next }
  /^tables_version = "[0-9.]+"$/ { next }
  /^policy_on_(unsure|error) = "review"$/ { next }
  /^run = "(gate-outline-initial|study-(flash|haiku|opus|sol|sonnet|terra))-2026-11"$/ { next }
  /^(arm|tool) = "(C|D_outline|D_regions)"$/ { next }
  /^scope = "(comparisons|comparisons_and_measures)"$/ { next }
  /^kind = "(location|rule_kind|checklist|evidence|image|other)"$/ { next }
  /^error_classes = "submission_invalid"$/ { next }
  /^harness_version = "[0-9.]+ \([0-9a-f]+\)"$/ { next }
  /^prompt_version = "v[0-9]+"$/ { next }
  /^(profile|model) = "[a-z0-9.-]+"$/ { next }
  /^package_digest = "sha256:[0-9a-f]+"$/ { next }
  { print }' "$study.toml")
[ -z "$bad" ] || fail "$study.toml holds a line that is not an aggregate or a known name: $bad"
for planted in 'run = "study-other-2026-11"' 'kind = "a-new-kind"' 'error_classes = "timeout"' 'arm = "D_other"' 'note = "words"'; do
  printf '%s\n' "$planted" | awk '
    /^run = "(gate-outline-initial|study-(flash|haiku|opus|sol|sonnet|terra))-2026-11"$/ { next }
    /^(arm|tool) = "(C|D_outline|D_regions)"$/ { next }
    /^kind = "(location|rule_kind|checklist|evidence|image|other)"$/ { next }
    /^error_classes = "submission_invalid"$/ { next }
    { found = 1 } END { exit found ? 0 : 1 }' || fail "the guard on $study.toml would let through: $planted"
done
# Every model and run it names is one the manifest or the frozen matrices already name.
known=$(grep -E '^model = ' docs/baseline/manifest.toml | sed -E 's/^model = "[^:]*:(.*)"$/\1/' | sort -u)
named=$(grep -E '^model = ' "$study.toml" | sed -E 's/^model = "(.*)"$/\1/' | sort -u)
[ "$known" = "$named" ] || fail "$study.toml names other models than manifest.toml"
known=$(grep -E '^model = ' docs/baseline/manifest.toml | sed -E 's/^model = "([^:]*):.*"$/\1/' | sort -u)
named=$(grep -E '^profile = ' "$study.toml" | sed -E 's/^profile = "(.*)"$/\1/' | sort -u)
[ "$known" = "$named" ] || fail "$study.toml names other backends than manifest.toml"

# The example on the format's page is the file the tests check, and the page says so.
grep -q -F 'example-transcript.json' docs/baseline/transcript.md || fail "transcript.md does not point at the example"

finished
