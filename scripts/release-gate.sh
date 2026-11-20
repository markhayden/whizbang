#!/usr/bin/env bash
# The gate of a release (plan m4, task R2, item 5): "A release is refused unless this
# passed on the same commit" (SPEC.md section 14). The `release` workflow runs it on a
# version tag, before anything is published, and publishes only what it held.
#
#   scripts/release-gate.sh <version tag> <folder>
#
# <folder> must not be there. When the gate passes it holds the candidate's own files:
# both archives, SHA256SUMS and the receipt. When the gate refuses it is removed, the
# reason is printed and the exit status is 1.
#
# It refuses unless all of these hold, in this order:
#
#  1. the tag is a version tag, `v<version>` (scripts/tag-version.sh), and names a commit;
#  2. the version has a candidate, a tag `v<version>-rc.<n>`. The one with the highest
#     number is the candidate; an earlier one is superseded, and nothing of it is read;
#  3. the candidate's tag names the version tag's commit;
#  4. a run of the workflow `prerelease`, started by the push of the candidate's tag on
#     that commit, ended in success;
#  5. the candidate, downloaded here, holds exactly the two archives, SHA256SUMS and the
#     receipt, `prerelease-receipt.json`; and each archive is its line of SHA256SUMS
#     (scripts/release-publish.sh check);
#  6. the receipt is read whole by its schema, names this candidate and this commit, says
#     its model tests ran from a checkout of this commit's tree, taken here from the
#     repository, holds
#     every required check exactly once, each meeting the conditions of its kind, and
#     names the digests of the two archives and of the binary inside the Mac's archive,
#     all three taken here again from the candidate's bytes (scripts/receipt-check.sh).
#
# It reads tags from the repository it is run in, which must hold every tag (the
# workflow's checkout fetches them all), and asks GitHub through `gh` for the runs and
# for the candidate's files. It builds nothing and publishes nothing. Needs jq.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "release-gate: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

[ $# -eq 2 ] || { echo "usage: scripts/release-gate.sh <version tag> <folder>" >&2; exit 2; }
tag=$1
folder=$2
made=0
tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || { [ "$made" = 0 ] || rm -rf "$folder"; }; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "release-gate: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT
refuse() {
  echo "release-gate: refused: $*" >&2
  exit 1
}
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
command -v jq >/dev/null || refuse "jq is not installed"
[ ! -e "$folder" ] || refuse "$folder is there already, and the candidate's files are downloaded into a folder of their own"

# --- 1. The version tag and its commit.
read_tag=$("$root/scripts/tag-version.sh" --release "$tag") || refuse "$tag is not a version tag"
version=$(printf '%s\n' "$read_tag" | sed -n 's/^version: //p')
commit=$(git rev-parse --verify --quiet "refs/tags/$tag^{commit}") || refuse "the tag $tag is not in this repository"

# --- 2. The candidate: the highest number among the version's candidate tags.
number=0
while IFS= read -r found; do
  [ -n "$found" ] || continue
  one=$("$root/scripts/tag-version.sh" --candidate "$found" "$version" 2>/dev/null | sed -n 's/^candidate: //p') || continue
  [ -n "$one" ] || continue
  [ "$one" -le "$number" ] || number=$one
done <<LIST
$(git tag --list "v$version-rc.*")
LIST
[ "$number" -gt 0 ] || refuse "version $version has no candidate: no tag v$version-rc.<n> is in this repository"
candidate=v$version-rc.$number

# --- 3. The candidate is of this commit.
candidate_commit=$(git rev-parse --verify --quiet "refs/tags/$candidate^{commit}") || refuse "the tag $candidate names no commit"
[ "$candidate_commit" = "$commit" ] ||
  refuse "the latest candidate of $version, $candidate, is of commit $candidate_commit, and $tag is of $commit: an earlier candidate is superseded, and a release is of the commit its candidate passed on"

# --- 4. `prerelease` passed on this commit, for this candidate.
gh run list --workflow prerelease.yml --commit "$commit" --limit 100 --json conclusion,event,headBranch,headSha,status >"$tmp/runs" ||
  refuse "the runs of the prerelease workflow could not be listed"
passed=$(jq --arg commit "$commit" --arg candidate "$candidate" \
  '[.[] | select(.headSha == $commit and .headBranch == $candidate and .event == "push" and .status == "completed" and .conclusion == "success")] | length' \
  "$tmp/runs" 2>/dev/null) || refuse "the list of the prerelease workflow's runs could not be read"
# A count and nothing else: an answer that was empty gives none, and is no run that passed.
case "$passed" in
  '' | *[!0-9]*) refuse "the list of the prerelease workflow's runs could not be read" ;;
esac
[ "$passed" -ge 1 ] || refuse "no run of the prerelease workflow for $candidate ended in success on commit $commit"

# --- 5. The candidate's own files.
made=1
gh release download "$candidate" --dir "$folder" || refuse "the files of the candidate $candidate could not be downloaded"
[ -f "$folder/prerelease-receipt.json" ] && [ ! -L "$folder/prerelease-receipt.json" ] ||
  refuse "the candidate $candidate holds no receipt (prerelease-receipt.json): the local leg has not passed for it"
"$root/scripts/release-publish.sh" check "$folder" "$version" >/dev/null || refuse "the candidate $candidate does not hold the files of a release of $version"

# --- 6. The receipt, held to the candidate's bytes.
mac=$(sha256 "$folder/whizbang-$version-aarch64-apple-darwin.tar.gz")
linux=$(sha256 "$folder/whizbang-$version-x86_64-unknown-linux-gnu.tar.gz")
tar -xzOf "$folder/whizbang-$version-aarch64-apple-darwin.tar.gz" "whizbang-$version-aarch64-apple-darwin/whizbang" >"$tmp/binary" 2>/dev/null && [ -s "$tmp/binary" ] ||
  refuse "the Mac's archive of $candidate holds no binary"
installed=$(sha256 "$tmp/binary")
tree=$(git rev-parse --verify --quiet "$commit^{tree}") || refuse "the commit of $tag names no tree"
"$root/scripts/receipt-check.sh" "$folder/prerelease-receipt.json" "tag=$candidate" "commit=$commit" "tree=$tree" "mac=$mac" "linux=$linux" "installed=$installed" ||
  refuse "the receipt of $candidate does not allow a release (above)"

echo "release-gate: passed: $tag is of commit $commit, whose candidate $candidate passed the prerelease workflow and the local leg; $folder holds its files"
done_ok=1
exit 0
}
main "$@"; exit
