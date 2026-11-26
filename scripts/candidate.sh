#!/usr/bin/env bash
# The candidate mode of the model tier's runners (plan m6, task P1, item 5): what they measure
# when the sources they measure cannot be committed before the records they write, because
# the fast tests hold the committed records to the sources' digests. They measure the staged
# tree instead, exported from the index, which creates no commit; the records then name that
# tree, and are committed with exactly those sources.
#
#   scripts/candidate.sh export <folder>
#   scripts/candidate.sh pack-matches <tree folder> <models directory>
#   scripts/candidate.sh copy-models <models directory> <candidate's models directory>
#   scripts/candidate.sh prepare <folder>
#
# `export`, run inside a checkout: refuses unstaged changes and untracked files under `pack/`
# or `crates/` (what the binary and the worker are built from), then writes the index's tree
# (`git write-tree`) into <folder>/tree with `git archive`, and prints its id. <folder> must
# not hold a tree already.
#
# `pack-matches`: the pack's sources the worker launches from, in the models directory
# (`pack/project/src/whizbang_pack/`, where `whizbang setup` installs them), are, file by
# file, the tree's (`pack/src/whizbang_pack/`): the same names, and each the same SHA-256.
# Prints a line `<name> sha256:<digest>` for each, and refuses any difference.
#
# `copy-models`: SAM 3's, Tiny's and the fixed-class segmenter's folders (`models/sam3`,
# `models/tiny`, `models/eomt-ade20k`; plan mE, task T2) of the first models
# directory copied into the second, each a real folder: the model store refuses a folder of
# a model's layout that is a link, and calls such a model damaged (ADR 0017, "A folder of
# the layout that is a link"; `models::Directory::unlinked`), so a link would leave the
# candidate with no model ready. On APFS the copy is a clone (`cp -c`), which takes no room
# until a file of it changes; elsewhere it is an ordinary copy. Refuses a source that holds
# any of the models through a link, and a destination that holds any already.
# It copies the three models' folders and nothing else: not the pack (`pack/`), which `prepare`
# installs into the candidate's directory from the tree itself. So the second directory is
# not one the model tier accepts as WHIZBANG_MODELS_DIR until a pack is installed in it
# (`whizbang setup --extra torch --models-dir <it>`). To measure a changed catalogue entry
# (plan m6, task W1), install the entry so changed into such a directory with the staged
# tree's binary (`whizbang models get <model> --models-dir <it>`), install the pack there,
# and name it as WHIZBANG_MODELS_DIR for `prepare`, which copies the models again from it.
#
# `prepare`, the whole candidate, on the owner's Mac (heavy; through scripts/heavy.sh):
# `export` into <folder>; the binary built from the exported tree, into <folder>/target;
# that tree's pack installed with it (`whizbang setup --extra torch`) into a new models
# directory <folder>/models; the installed models copied in (`copy-models`) from the
# models directory WHIZBANG_MODELS_DIR names, which `whizbang models verify` has checked; and
# `pack-matches` on the two. It writes <folder>/tree-id and prints the folder's parts.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "candidate: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail

refuse() {
  echo "candidate: $1" >&2
  exit 2
}

sha() { shasum -a 256 "$1" | awk '{ print "sha256:" $1 }'; }

export_tree() {
  local folder=$1
  case "$folder" in
    /*) ;;
    *) refuse "the folder must be an absolute path" ;;
  esac
  [ ! -e "$folder/tree" ] || refuse "$folder holds a tree already; remove it first"
  git rev-parse --show-toplevel >/dev/null 2>&1 || refuse "not inside a checkout"
  local top
  top=$(git rev-parse --show-toplevel)
  cd "$top"
  git diff --quiet -- pack crates || refuse "pack/ or crates/ has changes that are not staged; stage them or set them aside"
  [ -z "$(git ls-files --others --exclude-standard -- pack crates)" ] ||
    refuse "pack/ or crates/ has files git does not track; add them or set them aside"
  local id
  id=$(git write-tree)
  mkdir -p "$folder/tree"
  git archive --format=tar "$id" | tar -x -C "$folder/tree"
  printf '%s\n' "$id" >"$folder/tree-id"
  printf '%s\n' "$id"
}

pack_matches() {
  local tree=$1 models=$2
  local ours=$tree/pack/src/whizbang_pack installed=$models/pack/project/src/whizbang_pack
  [ -d "$ours" ] || refuse "the tree holds no pack sources"
  [ -d "$installed" ] || refuse "the models directory holds no installed pack"
  local names_ours names_installed
  names_ours=$(cd "$ours" && find . -type f -name '*.py' | LC_ALL=C sort)
  names_installed=$(cd "$installed" && find . -type f -name '*.py' | LC_ALL=C sort)
  [ "$names_ours" = "$names_installed" ] || refuse "the installed pack holds other files than the tree's"
  local name a b
  for name in $names_ours; do
    a=$(sha "$ours/$name")
    b=$(sha "$installed/$name")
    [ "$a" = "$b" ] || refuse "the installed pack's ${name#./} is not the tree's"
    printf '%s %s\n' "${name#./}" "$a"
  done
}

copy_models() {
  local from=$1 to=$2 model
  for model in sam3 tiny eomt-ade20k; do
    [ -d "$from/models/$model" ] && [ ! -L "$from/models/$model" ] ||
      refuse "$from holds no installed $model in a folder of its own"
    [ ! -e "$to/models/$model" ] && [ ! -L "$to/models/$model" ] ||
      refuse "$to holds $model already"
  done
  mkdir -p "$to/models"
  for model in sam3 tiny eomt-ade20k; do
    # A clone where the file system makes one; an ordinary copy where it does not.
    if ! cp -c -R "$from/models/$model" "$to/models/$model" 2>/dev/null; then
      rm -rf "$to/models/$model"
      cp -R "$from/models/$model" "$to/models/$model"
    fi
  done
}

prepare() {
  local folder=$1
  local source=${WHIZBANG_MODELS_DIR:-}
  case "$source" in
    /*) ;;
    *) refuse "WHIZBANG_MODELS_DIR must name, by an absolute path, the models directory that holds sam3, tiny and eomt-ade20k" ;;
  esac
  [ -d "$source/models/sam3" ] && [ -d "$source/models/tiny" ] && [ -d "$source/models/eomt-ade20k" ] ||
    refuse "WHIZBANG_MODELS_DIR holds no installed sam3, tiny and eomt-ade20k"
  export_tree "$folder" >/dev/null
  (cd "$folder/tree" && CARGO_TARGET_DIR=$folder/target cargo build -p whizbang --bin whizbang)
  local bin=$folder/target/debug/whizbang
  [ -x "$bin" ] || refuse "the candidate's binary was not built"
  rm -rf "$folder/models"
  mkdir -p "$folder/models"
  "$bin" setup --extra torch --models-dir "$folder/models"
  copy_models "$source" "$folder/models"
  pack_matches "$folder/tree" "$folder/models" >"$folder/pack-sources"
  echo "candidate: tree $(cat "$folder/tree-id"), in $folder/tree; binary $bin; models $folder/models"
}

case "${1-}" in
  export)
    [ "$#" = 2 ] || refuse "export takes one folder"
    export_tree "$2"
    ;;
  pack-matches)
    [ "$#" = 3 ] || refuse "pack-matches takes a tree folder and a models directory"
    pack_matches "$2" "$3"
    ;;
  copy-models)
    [ "$#" = 3 ] || refuse "copy-models takes a models directory and the candidate's"
    copy_models "$2" "$3"
    ;;
  prepare)
    [ "$#" = 2 ] || refuse "prepare takes one folder"
    prepare "$2"
    ;;
  *) refuse "the first argument is export, pack-matches, copy-models or prepare" ;;
esac
}
main "$@"; exit
