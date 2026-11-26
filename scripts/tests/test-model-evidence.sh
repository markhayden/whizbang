#!/usr/bin/env bash
# The candidate mode of the model tier's runners (plan m6, task P1, item 5;
# scripts/candidate.sh): the staged tree is what is measured, exported from the index with no
# commit made; an unstaged change or an untracked file under pack/ or crates/ is refused; the
# tree's id is recorded; and a pack installed from other sources than the tree's is refused.
# Nothing here builds, installs or loads anything: each case runs in a throwaway repository,
# and the runners' own refusals come before anything starts.
source "$(dirname "$0")/lib.sh"

cand=$root/scripts/candidate.sh
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid

repo=$tmp/repo
mkdir -p "$repo/pack/src/whizbang_pack" "$repo/crates/one/src" "$repo/docs"
printf 'REVISION = "1"\n' >"$repo/pack/src/whizbang_pack/sam3.py"
printf 'pub fn one() {}\n' >"$repo/crates/one/src/lib.rs"
printf 'notes\n' >"$repo/docs/notes.md"
git -C "$repo" init -q -b main
git -C "$repo" add -A
git -C "$repo" commit -q -m "base"

# A staged change is what is measured: the export holds it, and the id is the index's tree.
printf 'REVISION = "2"\n' >"$repo/pack/src/whizbang_pack/sam3.py"
git -C "$repo" add pack/src/whizbang_pack/sam3.py
(cd "$repo" && "$cand" export "$tmp/one") >"$tmp/id" 2>&1 || { cat "$tmp/id" >&2; fail "a staged change is exported"; }
want=$(git -C "$repo" write-tree)
[ "$(cat "$tmp/id")" = "$want" ] || fail "the id printed is the index's tree"
[ "$(cat "$tmp/one/tree-id")" = "$want" ] || fail "the tree's id is recorded beside it"
[ "$(cat "$tmp/one/tree/pack/src/whizbang_pack/sam3.py")" = 'REVISION = "2"' ] || fail "the export holds the staged source"
[ "$(git -C "$repo" rev-list --count HEAD)" = 1 ] || fail "the export made no commit"
[ ! -e "$tmp/one/tree/.git" ] || fail "the export is a tree, not a checkout"
expect_fail "a folder that holds a tree" "holds a tree already" sh -c "cd '$repo' && '$cand' export '$tmp/one'"
expect_fail "a relative folder" "absolute path" sh -c "cd '$repo' && '$cand' export relative"

# A change elsewhere than pack/ and crates/ is not what the worker or the binary is built
# from, and does not stop it.
printf 'more notes\n' >>"$repo/docs/notes.md"
expect_pass "an unstaged change outside pack and crates" sh -c "cd '$repo' && '$cand' export '$tmp/two'"

# An unstaged change, or a file git does not track, under pack/ or crates/ is refused.
printf 'REVISION = "3"\n' >"$repo/pack/src/whizbang_pack/sam3.py"
expect_fail "an unstaged change under pack" "not staged" sh -c "cd '$repo' && '$cand' export '$tmp/three'"
[ ! -e "$tmp/three" ] || fail "nothing is exported when it refuses"
git -C "$repo" checkout -q -- pack/src/whizbang_pack/sam3.py
printf 'x\n' >"$repo/crates/one/src/new.rs"
expect_fail "an untracked file under crates" "does not track" sh -c "cd '$repo' && '$cand' export '$tmp/four'"
rm "$repo/crates/one/src/new.rs"

# The pack the worker launches from must be the tree's, file by file.
models=$tmp/models
mkdir -p "$models/pack/project/src"
cp -R "$tmp/one/tree/pack/src/whizbang_pack" "$models/pack/project/src/"
expect_pass "the tree's own pack" "$cand" pack-matches "$tmp/one/tree" "$models"
grep -F "sam3.py sha256:" "$tmp/out" >/dev/null || fail "each source is named with its digest"
printf 'REVISION = "1"\n' >"$models/pack/project/src/whizbang_pack/sam3.py"
expect_fail "a pack installed from other sources" "is not the tree's" "$cand" pack-matches "$tmp/one/tree" "$models"
cp "$tmp/one/tree/pack/src/whizbang_pack/sam3.py" "$models/pack/project/src/whizbang_pack/sam3.py"
printf 'x\n' >"$models/pack/project/src/whizbang_pack/extra.py"
expect_fail "a pack with a file the tree has not" "other files than the tree's" "$cand" pack-matches "$tmp/one/tree" "$models"

# SAM 3, Tiny and the fixed-class segmenter reach the candidate as real folders, never links: the model store calls a
# model whose folder is a link damaged (ADR 0017), as the owner's first run of the mode found.
owner=$tmp/owner-models
for model in sam3 tiny eomt-ade20k; do
  mkdir -p "$owner/models/$model/installations/abc/files" "$owner/models/$model/checks"
  printf 'weights of %s\n' "$model" >"$owner/models/$model/installations/abc/files/w.bin"
  printf 'sha256:abc\n' >"$owner/models/$model/current"
done
expect_pass "the models copied" "$cand" copy-models "$owner" "$tmp/cand-models"
for model in sam3 tiny eomt-ade20k; do
  [ -d "$tmp/cand-models/models/$model" ] && [ ! -L "$tmp/cand-models/models/$model" ] ||
    fail "$model is a folder of its own in the candidate, not a link"
  [ -z "$(find "$tmp/cand-models/models/$model" -type l)" ] || fail "nothing in the copy of $model is a link"
  diff -r "$owner/models/$model" "$tmp/cand-models/models/$model" >/dev/null || fail "the copy of $model is whole"
done
printf 'changed\n' >"$tmp/cand-models/models/sam3/installations/abc/files/w.bin"
[ "$(cat "$owner/models/sam3/installations/abc/files/w.bin")" = "weights of sam3" ] ||
  fail "a change to the candidate's copy leaves the owner's installation as it was"
expect_fail "a candidate that holds the models already" "holds sam3 already" "$cand" copy-models "$owner" "$tmp/cand-models"
mkdir -p "$tmp/linked/models"
ln -s "$owner/models/sam3" "$tmp/linked/models/sam3"
cp -R "$owner/models/tiny" "$tmp/linked/models/tiny"
cp -R "$owner/models/eomt-ade20k" "$tmp/linked/models/eomt-ade20k"
expect_fail "a source that holds a model through a link" "in a folder of its own" "$cand" copy-models "$tmp/linked" "$tmp/cand-two"
grep -F 'copy_models "$source" "$folder/models"' scripts/candidate.sh >/dev/null &&
  ! grep -E '^[^#]*ln -s' scripts/candidate.sh >/dev/null || fail "prepare copies the models in and links none"

# The runners take the mode, and refuse what does not go with it before anything starts.
expect_fail "the candidate's preparation without the owner's models" "WHIZBANG_MODELS_DIR" \
  env -u WHIZBANG_MODELS_DIR "$cand" prepare "$tmp/five"
expect_fail "another argument to the evidence" "the one argument it takes is --candidate" \
  scripts/model-evidence.sh --other
expect_fail "a candidate's list" "not given together" \
  env WHIZBANG_MODELS_DIR="$tmp/owner" sh -c "mkdir -p '$tmp/owner/models' '$tmp/owner/pack' && scripts/test-models.sh --candidate --list"
grep -F 'scripts/candidate.sh" prepare "$work"' scripts/test-models.sh >/dev/null ||
  fail "test-models.sh prepares the candidate with scripts/candidate.sh"
grep -F 'scripts/candidate.sh prepare "$cand"' scripts/model-evidence.sh >/dev/null ||
  fail "model-evidence.sh prepares the candidate with scripts/candidate.sh"

finished
