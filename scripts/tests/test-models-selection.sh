#!/usr/bin/env bash
# The selections of the model tier (plan m6, task D1): SAM 3's parity asked of a remote worker,
# `remote_parity::`, is selected by `scripts/test-models.sh parity-remote` alone, which refuses
# unless WHIZBANG_REMOTE_CONFIG names a config whose segment is of the kind remote; the
# default selection, `parity` and a release's local leg hold no remote case. Nothing here
# runs cargo or contacts a host: each case is refused at its first lines, or read from the
# scripts' text.
source "$(dirname "$0")/lib.sh"

tm=scripts/test-models.sh
module=crates/whizbang/tests/it/remote_parity.rs

# The module is ignored in the fast tier, and names the one test L1 runs.
grep -F -x '#[ignore = "the model tier'"'"'s remote part: scripts/test-models.sh parity-remote, under a config whose segment is remote"]' "$module" >/dev/null ||
  fail "remote_parity's test is ignored in the fast tier"
grep -F -x 'fn sam3_answers_each_fixture_inside_the_tolerance_from_a_remote_worker() {' "$module" >/dev/null ||
  fail "remote_parity names the test of plan m6, task L1"

# Each selection's filter, from the script's own text: the default, model_tier, parity and
# coverage reach no test of remote_parity; parity-remote reaches it and nothing else.
grep -F -x "  \"\") filter='test(/^(model_tier|parity|coverage)::/)' ;;" "$tm" >/dev/null ||
  fail "the default selection is model_tier, parity and coverage, each from the start of a name"
grep -F -x '  model_tier | parity | coverage) filter="test(/^$1::/)" ;;' "$tm" >/dev/null ||
  fail "a part's selection is its own module, from the start of a name"
grep -F -x "  parity-remote) filter='test(/^remote_parity::/)' remote=1 ;;" "$tm" >/dev/null ||
  fail "parity-remote selects remote_parity alone"
for name in "remote_parity::sam3_answers_each_fixture_inside_the_tolerance_from_a_remote_worker"; do
  for pattern in '^(model_tier|parity|coverage)::' '^model_tier::' '^parity::' '^coverage::'; do
    printf '%s\n' "$name" | grep -E "$pattern" >/dev/null && fail "the selection $pattern reaches $name"
  done
  printf '%s\n' "$name" | grep -E '^remote_parity::' >/dev/null || fail "parity-remote reaches $name"
done
# The local leg of a release runs and lists the default selection, with no argument.
grep -F 'scripts/test-models.sh --list)' scripts/prerelease-local.sh >/dev/null &&
  grep -F 'scripts/heavy.sh test-models scripts/test-models.sh)' scripts/prerelease-local.sh >/dev/null ||
  fail "the local leg runs the default selection"
grep -F 'parity-remote' scripts/prerelease-local.sh >/dev/null && fail "the local leg names no remote selection"

# parity-remote refuses before it builds anything.
expect_fail "no config" "needs WHIZBANG_REMOTE_CONFIG" env -u WHIZBANG_REMOTE_CONFIG "$tm" parity-remote
expect_fail "a relative config" "must be an absolute path" env WHIZBANG_REMOTE_CONFIG=whizbang.toml "$tm" parity-remote
expect_fail "a config that is not there" "names no file" env WHIZBANG_REMOTE_CONFIG="$tmp/nowhere.toml" "$tm" parity-remote
printf '[backends.segment]\nkind = "pack"\nmodel = "sam3"\n\n[backends.fill]\nkind = "remote"\n' >"$tmp/local.toml"
expect_fail "a config whose segment is local" "not of the kind remote" env WHIZBANG_REMOTE_CONFIG="$tmp/local.toml" "$tm" parity-remote
printf '[backends.segment]\nkind = "remote"\nurl = "https://gpu-host.example"\n' >"$tmp/remote.toml"
expect_fail "a relative env file" "must be an absolute path" \
  env WHIZBANG_REMOTE_CONFIG="$tmp/remote.toml" WHIZBANG_REMOTE_ENV_FILE=whizbang.env "$tm" parity-remote
expect_fail "an env file that is not there" "names no file" \
  env WHIZBANG_REMOTE_CONFIG="$tmp/remote.toml" WHIZBANG_REMOTE_ENV_FILE="$tmp/nowhere.env" "$tm" parity-remote
expect_fail "with a candidate" "are not given together" env WHIZBANG_REMOTE_CONFIG="$tmp/remote.toml" "$tm" --candidate parity-remote
expect_fail "with another part" "names a part of the tier" env WHIZBANG_REMOTE_CONFIG="$tmp/remote.toml" "$tm" parity-remote parity

finished
