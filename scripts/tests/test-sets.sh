#!/usr/bin/env bash
# The set builder (tools/sets/; plan mE, task B1; ADR 0050): its own tests, then the offline
# dry run, whose sets whizbang's checker must read.
#
# - The builder runs on the Python the pack pins, found by uv offline, with no user's
#   configuration and no download of a Python; its one package, Pillow, is installed from
#   its lock (by hash, from PyPI's two origins only) into a folder of this test's own.
# - Its tests run under the pack's runner (pack/tests/run.py): among them, a generator's
#   request or download to an origin the config does not approve is refused, and so is a
#   redirect to one (tools/sets/tests/test_egress.py). Local servers stand in for providers.
# - The dry run builds a set of each use case and a blind one from the plans of
#   tools/sets/tests/plans/ with the fake generator, confirms every draft label as the owner
#   would, promotes and seals; `whizbang eval sets check` must then read every one of them.
# - What a build refuses: owner storage inside a repository, a blind plan inside one, and,
#   until the spend guard is built (B1's second pull request), any generation that is live.
#
# Nothing here calls a provider. uv contacts PyPI only when its cache lacks the package.
source "$(dirname "$0")/lib.sh"

pinned=$(tr -d ' \n' <pack/.python-version)
[ "$(tr -d ' \n' <tools/sets/.python-version)" = "$pinned" ] ||
  fail "tools/sets/.python-version must be the pack's pin, $pinned"
# The lock names no index but PyPI's and no file but on PyPI's file host.
others=$(grep -o -E '(https?|git|file)://[^/"]*' tools/sets/uv.lock | sort -u |
  grep -v -x -F -e 'https://pypi.org' -e 'https://files.pythonhosted.org' || true)
[ -z "$others" ] || fail "tools/sets/uv.lock names another origin: $others"

python=$(env UV_NO_CONFIG=1 UV_PYTHON_DOWNLOADS=never \
  uv --no-config --offline --no-python-downloads --managed-python python find "$pinned" 2>/dev/null) ||
  fail "uv found no Python $pinned that it manages; install it as scripts/test-pack.sh says"
(cd tools/sets && env UV_PROJECT_ENVIRONMENT="$tmp/venv" UV_NO_CONFIG=1 UV_PYTHON_DOWNLOADS=never \
  UV_CONCURRENT_DOWNLOADS=4 UV_CONCURRENT_INSTALLS=4 UV_CONCURRENT_BUILDS=1 \
  uv --no-config --no-python-downloads sync --locked --no-build --quiet --python "$python") ||
  fail "the builder's lock could not be installed"
py=$tmp/venv/bin/python

# The builder's own tests.
expect_pass "the builder's tests" env WHIZBANG_PACK_TEST_LIMIT=60 "$py" -I -B pack/tests/run.py tools/sets/tests
for case in test_a_request_to_an_origin_the_config_does_not_approve_is_refused_before_it_is_sent \
  test_a_download_from_an_origin_the_config_does_not_approve_is_refused \
  test_a_redirect_to_an_origin_the_config_does_not_approve_is_refused \
  test_the_seal_of_every_set_in_the_repository_is_made_again_exactly; do
  grep -E "^$case .* ok$" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "the builder's tests ran and passed $case"; }
done

# sets <args...>: the builder, run from its folder.
sets() { (cd tools/sets && "$py" -B -m whizbang_sets "$@"); }
root_sets=$tmp/evalsets
owner=$tmp/owner
mkdir -p "$owner" "$tmp/home"
cp tools/sets/tests/plans/blind.plan.toml "$tmp/blind.plan.toml"
for name in source-fidelity reality-checks reference-consistency product-fidelity brief-compliance blind; do
  plan=$root/tools/sets/tests/plans/$name.plan.toml
  [ "$name" != blind ] || plan=$tmp/blind.plan.toml
  id=$(sed -n 's/^id = "\([a-z-]*\/[a-z0-9-]*\)"$/\1/p' "$plan" | head -n 1)
  [ -n "$id" ] || fail "$plan names no set"
  expect_pass "build $id" sets build --plan "$plan" --out "$root_sets/$id" --owner "$owner" --fake
  draft=$owner/$id/labels-draft.toml
  # Confirmed as the owner would, after reading the contact sheets.
  sed 's/^status = "draft"$/status = "confirmed"/' "$draft" >"$tmp/draft" && mv "$tmp/draft" "$draft"
  if [ "$name" = blind ]; then
    expect_pass "promote $id" sets promote --draft "$draft" --set "$root_sets/$id" --attest "the dry run" \
      --labels-out "$owner/$id/labels"
    [ -f "$owner/$id/labels/labels.toml" ] && [ ! -e "$root_sets/$id/labels.toml" ] ||
      fail "a blind set's labels leave its folder for owner storage"
  else
    expect_pass "promote $id" sets promote --draft "$draft" --set "$root_sets/$id" --attest "the dry run"
  fi
done

# The dry run builds a dev set that whizbang's checker reads: every set built above. The
# binary is the one cargo builds for the tests, run with a cleared environment and a home of
# its own.
cargo build --quiet -p whizbang --bin whizbang || fail "whizbang did not build"
whizbang=${CARGO_TARGET_DIR:-$root/target}/debug/whizbang
expect_pass "the dry run builds a dev set that whizbang's checker reads" \
  env -i PATH=/usr/bin:/bin HOME="$tmp/home" "$whizbang" eval sets check --root "$root_sets"
grep -x -F "sets: 6" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "the checker read all six sets"; }
# A seal is not a formality: a job changed after it fails the check.
job=$root_sets/reality-checks/dry-run-v1/items/bakery-counter-clean/job.toml
sed 's/^brief = ""$/brief = "changed"/' "$job" >"$tmp/job" && cp "$tmp/job" "$job"
grep -x -F 'brief = "changed"' "$job" >/dev/null || fail "the job was changed for the case"
expect_fail "a job changed after the seal" "the seal does not match the set" \
  env -i PATH=/usr/bin:/bin HOME="$tmp/home" "$whizbang" eval sets check --root "$root_sets"

# Owner material stays outside every repository.
inside=$root/tools/sets/owner-inside-the-repository
expect_fail "--owner inside the work tree" "inside a repository" \
  sets build --plan "$root/tools/sets/tests/plans/reality-checks.plan.toml" --out "$tmp/x/reality-checks/x" \
  --owner "$inside" --fake
[ ! -e "$inside" ] && [ ! -e "$tmp/x" ] || fail "a refused build wrote nothing"
expect_fail "a blind plan inside the work tree" "a blind set's --plan is inside a repository" \
  sets build --plan "$root/tools/sets/tests/plans/blind.plan.toml" --out "$tmp/y/source-fidelity/y" \
  --owner "$owner/again" --fake
[ ! -e "$tmp/y" ] || fail "a refused blind build wrote nothing"
# Nothing is generated live before the spend guard.
printf '[egress.recipients.fal]\norigin = "https://fal.run"\nclasses = ["prompts"]\n' >"$tmp/whizbang.toml"
expect_fail "a live build" "live generation waits for the spend guard" \
  sets build --plan "$root/tools/sets/tests/plans/reality-checks.plan.toml" --out "$tmp/z/reality-checks/z" \
  --owner "$owner/live" --config "$tmp/whizbang.toml"

finished
