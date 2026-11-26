#!/usr/bin/env bash
# Publishes a set the owner has reviewed (docs/evalsets.md, "Building a set"; plan mE, task
# B1). Ported from squint scripts/sets-publish.sh at the commit docs/sources.toml pins,
# reshaped: the owner confirms or strikes every label first (promote refuses a draft), the
# seal is written by promote, no picture enters this repository, and a set already here is
# never written over (a sealed set is never edited).
#
#   WHIZBANG_OWNER_DIR=<owner storage> WHIZBANG_EVALSETS_DIR=<a clone of the pictures'
#   repository> scripts/sets-publish.sh <use-case>/<set> "<attestation>"
#
# The steps, from the set built at <owner storage>/build/evalsets/<set id>: promote (a blind
# set's labels move to <owner storage>/<set id>/labels/); the pictures copied into the clone
# and committed there; that commit pinned as the set's images_rev; the set, without its
# pictures, placed under evalsets/; `whizbang eval sets check`. Pushing the clone, the row of
# evalsets/README.md and the commit here are the owner's steps after it. A publish that
# failed part-way is begun again from the start, after the clone's commit of the set (if
# one was made) is taken back and evalsets/<set id> removed: docs/evalsets.md says how.
#
# The clone's remote `origin` must be the pictures' repository by its current name
# (github.com/markhayden/whizbang-evalsets). Neither folder may be inside this repository.
# WHIZBANG_SETS_ROOT names another folder of sets than evalsets/, and WHIZBANG_SETS_PYTHON the
# Python that runs the builder (by default `uv run` in tools/sets/): scripts/tests/test-sets.sh
# uses both.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "sets-publish: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
main() {
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
die() { echo "sets-publish: $*" >&2; exit 1; }

set_id=${1:-}
attest=${2:-}
[ $# -eq 2 ] && [ -n "$attest" ] || die "usage: sets-publish.sh <use-case>/<set> \"<attestation>\""
id_form='^[a-z0-9][a-z0-9-]*/[a-z0-9][a-z0-9-]*$'
[[ "$set_id" =~ $id_form ]] || die "a set's id is <use-case>/<set>, of lower-case letters, digits and hyphens"
owner=${WHIZBANG_OWNER_DIR:?sets-publish: name owner storage in WHIZBANG_OWNER_DIR}
clone=${WHIZBANG_EVALSETS_DIR:?sets-publish: name the clone of the pictures repository in WHIZBANG_EVALSETS_DIR}
sets_root=${WHIZBANG_SETS_ROOT:-$root/evalsets}
[ -d "$owner" ] || die "owner storage is not a folder"
if git -C "$owner" rev-parse --git-dir >/dev/null 2>&1; then die "owner storage is inside a repository"; fi
[ -d "$clone/.git" ] || die "WHIZBANG_EVALSETS_DIR is not a clone"
clone=$(cd "$clone" && pwd -P)
case "$clone/" in "$root"/*) die "WHIZBANG_EVALSETS_DIR is inside this repository" ;; esac
[ "$(git -C "$clone" rev-parse --show-toplevel)" = "$clone" ] || die "WHIZBANG_EVALSETS_DIR is not the top of its clone"
url=$(git -C "$clone" remote get-url origin 2>/dev/null) || die "the clone has no remote origin"
case "$url" in
  https://github.com/markhayden/whizbang-evalsets | https://github.com/markhayden/whizbang-evalsets.git \
  | git@github.com:markhayden/whizbang-evalsets | git@github.com:markhayden/whizbang-evalsets.git \
  | ssh://git@github.com/markhayden/whizbang-evalsets | ssh://git@github.com/markhayden/whizbang-evalsets.git) ;;
  *) die "the clone's origin is not github.com/markhayden/whizbang-evalsets" ;;
esac
[ ! -e "$clone/$set_id" ] || die "the clone holds $set_id already; its pictures are never written over"
git -C "$clone" diff --cached --quiet || die "the clone has changes staged; commit or clear them first"
build=$owner/build/evalsets/$set_id
draft=$owner/$set_id/labels-draft.toml
[ -f "$build/set.toml" ] || die "no set is built at <owner storage>/build/evalsets/$set_id"
[ -f "$draft" ] || die "no draft labels at <owner storage>/$set_id"
[ ! -e "$sets_root/$set_id" ] || die "$set_id is published already; a sealed set is never edited, a change is a new version"

sets() {
  if [ -n "${WHIZBANG_SETS_PYTHON:-}" ]; then
    (cd "$root/tools/sets" && "$WHIZBANG_SETS_PYTHON" -B -m whizbang_sets "$@")
  else
    (cd "$root/tools/sets" && UV_PYTHON_DOWNLOADS=never uv run --locked --no-build --no-config --no-python-downloads python -B -m whizbang_sets "$@")
  fi
}

if grep -x -F 'split = "blind"' "$build/set.toml" >/dev/null; then
  sets promote --draft "$draft" --set "$build" --attest "$attest" --labels-out "$owner/$set_id/labels"
else
  sets promote --draft "$draft" --set "$build" --attest "$attest"
fi

# The pictures into the clone, committed there; nothing is pushed.
pictures=0
while IFS= read -r picture; do
  mkdir -p "$clone/$set_id/$(dirname "$picture")"
  cp "$build/$picture" "$clone/$set_id/$picture"
  pictures=$((pictures + 1))
done <<EOF_PICTURES
$(cd "$build" && find . -type f -name '*.png' | sed 's|^\./||' | sort)
EOF_PICTURES
[ "$pictures" -gt 0 ] || die "the built set holds no picture"
git -C "$clone" add -A -- "$set_id"
git -C "$clone" commit -q -m "$set_id: $pictures pictures"
rev=$(git -C "$clone" rev-parse HEAD)
sets pin --set "$build" --rev "$rev"

# The set into this repository, without a picture.
mkdir -p "$sets_root/$set_id"
while IFS= read -r file; do
  mkdir -p "$sets_root/$set_id/$(dirname "$file")"
  cp "$build/$file" "$sets_root/$set_id/$file"
done <<EOF_FILES
$(cd "$build" && find . -type f ! -name '*.png' | sed 's|^\./||' | sort)
EOF_FILES
(cd "$root" && cargo run --quiet -p whizbang -- eval sets check --root "$sets_root")
echo "published $set_id at $rev: push the clone, add the set's row to evalsets/README.md, and commit it here"
}
main "$@"; exit
