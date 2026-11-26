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
for name in source-fidelity reality-checks reference-consistency product-fidelity brief-compliance blind seasonal; do
  plan=$root/tools/sets/tests/plans/$name.plan.toml
  [ "$name" != blind ] || plan=$tmp/blind.plan.toml
  # The seasonal cover pair D1 builds live (plan mE, task B1, item 9), here with the fake.
  [ "$name" != seasonal ] || plan=$root/tools/sets/plans/seasonal-cover-v1.plan.toml
  id=$(sed -n 's/^id = "\([a-z-]*\/[a-z0-9-]*\)"$/\1/p' "$plan" | head -n 1)
  [ -n "$id" ] || fail "$plan names no set"
  expect_pass "build $id" sets build --plan "$plan" --out "$root_sets/$id" --owner "$owner" --fake
  draft=$owner/$id/labels-draft.toml
  if [ "$name" = reality-checks ]; then
    mkdir -p "$tmp/evalsets-unpromoted/reality-checks"
    cp -R "$root_sets/$id" "$tmp/evalsets-unpromoted/$id"
  fi
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
grep -x -F "sets: 7" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "the checker read all seven sets"; }
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
expect_fail "a blind plan inside the work tree" "the plan, the set or an argument was refused" \
  sets build --plan "$root/tools/sets/tests/plans/blind.plan.toml" --out "$tmp/y/source-fidelity/y" \
  --owner "$owner/again" --fake
[ ! -e "$tmp/y" ] || fail "a refused blind build wrote nothing"
grep -F "a blind set's --plan is inside a repository" "$owner/again/source-fidelity/dry-run-blind-v1/messages.log" >/dev/null ||
  fail "a blind build's refusal is kept in owner storage, and printed by its kind alone"
# A live build needs its ledger, and a generator with no dated bound price sends nothing.
printf '[egress.recipients.fal]\norigin = "https://fal.run"\nclasses = ["prompts", "source_images"]\n' >"$tmp/whizbang.toml"
sed 's/^generator = "fake"$/generator = "fal"/' tools/sets/tests/plans/reality-checks.plan.toml >"$tmp/live.plan.toml"
expect_fail "a live build without a ledger" "--ledger" \
  sets build --plan "$tmp/live.plan.toml" --out "$tmp/z/reality-checks/z" --owner "$owner/live" --config "$tmp/whizbang.toml"
with_key() { (export FAL_KEY=made-up-key-for-a-test && sets "$@"); }
expect_fail "a live build of a generator with no price" "no dated bound price for the generator fal" \
  with_key build --plan "$tmp/live.plan.toml" --out "$tmp/z/reality-checks/z" --owner "$owner/live" \
  --config "$tmp/whizbang.toml" --ledger "$owner/live/ledger.jsonl" --cap-usd 1
grep -c . "$owner/live/ledger.jsonl" | grep -x 1 >/dev/null || fail "nothing was charged for a request never sent"

# Publishing: the pictures committed in a clone of the pictures' repository, the revision
# pinned, the set placed without a picture, and a set already published never written over.
clone=$tmp/pictures
git init -q "$clone"
git -C "$clone" config user.name "A Tester"
git -C "$clone" config user.email "tester@example.com"
git -C "$clone" remote add origin git@github.com:markhayden/whizbang-evalsets.git
publish_id=reality-checks/dry-run-v1
mkdir -p "$owner/build/evalsets/reality-checks"
cp -R "$root_sets/../evalsets-unpromoted/$publish_id" "$owner/build/evalsets/$publish_id"
publish() {
  env WHIZBANG_OWNER_DIR="$owner" WHIZBANG_EVALSETS_DIR="$clone" WHIZBANG_SETS_ROOT="$tmp/published" \
    WHIZBANG_SETS_PYTHON="$py" scripts/sets-publish.sh "$@"
}
expect_pass "publish a reviewed set" publish "$publish_id" "the dry run"
grep -F "published $publish_id at " "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "publish said what it did"; }
rev=$(git -C "$clone" rev-parse HEAD)
grep -x -F "images_rev = \"$rev\"" "$tmp/published/$publish_id/set.toml" >/dev/null || fail "the clone's commit is pinned"
[ -z "$(find "$tmp/published" -name '*.png')" ] || fail "no picture is placed with the set"
[ -n "$(git -C "$clone" ls-files -- "$publish_id" | grep '\.png$' || true)" ] || fail "the pictures are committed in the clone"
expect_fail "publish it again" "its pictures are never written over" publish "$publish_id" "the dry run"
git -C "$clone" rm -q -r -- "$publish_id" && git -C "$clone" commit -q -m "taken back"
expect_fail "publish it again, its pictures taken back" "a sealed set is never edited" publish "$publish_id" "the dry run"
for url in git@github.com:markhayden/wafflery-evalsets.git https://evil.example/github.com/markhayden/whizbang-evalsets \
  https://github.com/markhayden/whizbang-evalsets.git.example; do
  git -C "$clone" remote set-url origin "$url"
  expect_fail "a clone whose origin is $url" "is not github.com/markhayden/whizbang-evalsets" publish "$publish_id" "x"
done
git -C "$clone" remote set-url origin https://github.com/markhayden/whizbang-evalsets.git
printf 'x\n' >"$clone/staged.txt" && git -C "$clone" add staged.txt
expect_fail "a clone with changes staged" "changes staged" publish "reality-checks/other-v1" "x"
mkdir -p "$root/tools/sets/clone-inside/.git"
expect_fail "a clone inside this repository" "inside this repository" \
  env WHIZBANG_OWNER_DIR="$owner" WHIZBANG_EVALSETS_DIR="$root/tools/sets/clone-inside" scripts/sets-publish.sh "$publish_id" "x"
rm -r "$root/tools/sets/clone-inside"

finished
