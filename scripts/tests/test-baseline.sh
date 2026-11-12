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

# The scripts import nothing outside the standard library and their own folder.
others=$(grep -h -E '^(import|from) ' scripts/baseline/*.py scripts/baseline/tests/*.py \
  | sed -E 's/^(import|from) +([A-Za-z0-9_]+).*/\2/' | sort -u \
  | grep -v -x -E 'argparse|collections|contextlib|copy|hashlib|io|json|os|sys|tempfile|unittest|common|convert|audit' || true)
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
grep -q -E '^incomplete = 0$' "$record" || fail "$record: the baseline has incomplete decisions"
grep -q -E '^invalid = 0$' "$record" || fail "$record: the baseline has invalid transcripts"
manifest=$(grep -E '^digest = ' docs/baseline/manifest.toml | sed -E 's/^digest = //')
audited=$(grep -E '^package_digest = ' "$record" | sed -E 's/^package_digest = //')
[ "$manifest" = "$audited" ] || fail "$record audits another package than manifest.toml names"
