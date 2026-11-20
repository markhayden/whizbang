#!/usr/bin/env bash
# The files of a candidate and of a release, and the one command that publishes each
# (plan m4, task R2, items 2 and 5). The workflows run it, never a person; and only the
# orchestrator pushes the tag that starts a workflow (AGENTS.md).
#
#   scripts/release-publish.sh join <out> <folder> <folder>
#       Each folder is one target's: its archive and the SHA256SUMS `just package` wrote
#       beside it. <out>, which must not be there, is made with both archives and one
#       SHA256SUMS of their two lines, in the order of their names.
#   scripts/release-publish.sh check <folder> <version>
#       Holds a folder to the files of a release of that version, and changes nothing.
#   scripts/release-publish.sh candidate <candidate tag> <folder>
#       Makes the candidate: a GitHub release for the tag, a draft, marked as a
#       pre-release, with the folder's three files. Job `candidate` of `prerelease`.
#   scripts/release-publish.sh release <version tag> <folder>
#       Publishes the same three files under the version tag, marked as a pre-release
#       (the owner's decision 1). Workflow `release`, after scripts/release-gate.sh, on the
#       folder the gate filled: the candidate's own files, so nothing is built again.
#
# The files of a release of <version>, and nothing else, but for the candidate's receipt
# (`prerelease-receipt.json`), which the gate leaves in the folder and which is not
# published again:
#
#   whizbang-<version>-aarch64-apple-darwin.tar.gz
#   whizbang-<version>-x86_64-unknown-linux-gnu.tar.gz
#   SHA256SUMS     exactly their two lines, each 64 lower-case hexadecimal digits, two
#                  spaces and the name, and each the SHA-256 of that archive's bytes,
#                  taken here
#
# Nothing is published from a folder that fails this, from a tag of the wrong kind
# (scripts/tag-version.sh), or for a version that is not the files'. `gh` is given the tag
# with `--verify-tag`, so it never makes a tag. WHIZBANG_PUBLISH_DRY_RUN=1 makes the last
# two print the `gh` command, one argument a line, and run nothing.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "release-publish: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

die() {
  echo "release-publish: $*" >&2
  exit 1
}
usage() {
  echo "usage: scripts/release-publish.sh join <out> <folder> <folder> | check <folder> <version> | candidate <tag> <folder> | release <tag> <folder>" >&2
  exit 2
}
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
targets="aarch64-apple-darwin x86_64-unknown-linux-gnu"
# line_of <sums> <name>: the digest of the one whole line for that name, or a failure.
line_of() {
  local found
  found=$(awk -v file="$2" 'length($0) == 66 + length(file) && substr($0, 1, 64) ~ /^[0-9a-f]+$/ && substr($0, 65, 2) == "  " && substr($0, 67) == file { print substr($0, 1, 64) }' "$1")
  printf '%s\n' "$found" | grep -x -E '[0-9a-f]{64}' >/dev/null || die "$1 must hold exactly one line for $2 (64 lower-case hexadecimal digits, two spaces and the name)"
  printf '%s\n' "$found"
}
# held <folder> <version>: the folder holds the files of a release of that version.
held() {
  local folder=$1 version=$2 target name want=""
  [ -d "$folder" ] || die "there is no folder $folder"
  for target in $targets; do
    name=whizbang-$version-$target.tar.gz
    want="$want$name "
    [ -f "$folder/$name" ] && [ ! -L "$folder/$name" ] || die "$folder holds no archive $name"
  done
  [ -f "$folder/SHA256SUMS" ] && [ ! -L "$folder/SHA256SUMS" ] || die "$folder holds no SHA256SUMS"
  [ "$(ls -A "$folder" | grep -v -x -F 'prerelease-receipt.json' | tr '\n' ' ')" = "SHA256SUMS $want" ] ||
    die "$folder must hold the two archives of version $version and SHA256SUMS, and nothing else (it holds: $(ls -A "$folder" | tr '\n' ' '))"
  [ "$(wc -l <"$folder/SHA256SUMS" | tr -d ' ')" = 2 ] || die "$folder/SHA256SUMS must hold exactly two lines"
  for target in $targets; do
    name=whizbang-$version-$target.tar.gz
    [ "$(line_of "$folder/SHA256SUMS" "$name")" = "$(sha256 "$folder/$name")" ] ||
      die "the SHA-256 of $name is not the one SHA256SUMS names"
  done
}

[ $# -ge 1 ] || usage
mode=$1
shift
case "$mode" in
  join)
    [ $# -eq 3 ] || usage
    out=$1
    shift
    [ ! -e "$out" ] || die "$out is there already"
    version=
    for folder in "$@"; do
      [ -d "$folder" ] || die "there is no folder $folder"
      [ "$(ls -A "$folder" | grep -c -E '^whizbang-.*\.tar\.gz$' || true)" = 1 ] && [ "$(ls -A "$folder" | wc -l | tr -d ' ')" = 2 ] && [ -f "$folder/SHA256SUMS" ] ||
        die "$folder must hold one archive and its SHA256SUMS, and nothing else"
      name=$(ls -A "$folder" | grep -E '^whizbang-.*\.tar\.gz$')
      found=${name#whizbang-}
      found=${found%%-*}
      [ -z "$version" ] || [ "$found" = "$version" ] || die "the two archives are of two versions: $version and $found"
      version=$found
      [ "$(wc -l <"$folder/SHA256SUMS" | tr -d ' ')" = 1 ] || die "$folder/SHA256SUMS must hold exactly one line"
      [ "$(line_of "$folder/SHA256SUMS" "$name")" = "$(sha256 "$folder/$name")" ] || die "the SHA-256 of $folder/$name is not the one its SHA256SUMS names"
    done
    printf '%s\n' "$version" | grep -x -E '[0-9]+\.[0-9]+\.[0-9]+' >/dev/null || die "the archives name no version of three numbers"
    made=$(mktemp -d)
    for folder in "$@"; do
      cp "$folder"/whizbang-*.tar.gz "$made/"
      cat "$folder/SHA256SUMS" >>"$made/lines"
    done
    sort -k 2 "$made/lines" >"$made/SHA256SUMS"
    rm "$made/lines"
    # What was joined is a release's files: one archive for each target, and their lines.
    (held "$made" "$version") || { rm -rf "$made"; exit 1; }
    mkdir -p "$(dirname "$out")"
    mv "$made" "$out"
    chmod 755 "$out"
    echo "release-publish: joined: $out holds both archives of $version and SHA256SUMS"
    ;;
  check)
    [ $# -eq 2 ] || usage
    held "$1" "$2"
    echo "release-publish: ok: $1 holds the files of a release of $2"
    ;;
  candidate | release)
    [ $# -eq 2 ] || usage
    tag=$1
    folder=$2
    read_tag=$("$root/scripts/tag-version.sh" "--$mode" "$tag") || exit 1
    version=$(printf '%s\n' "$read_tag" | sed -n 's/^version: //p')
    held "$folder" "$version"
    files=("$folder/whizbang-$version-aarch64-apple-darwin.tar.gz" "$folder/whizbang-$version-x86_64-unknown-linux-gnu.tar.gz" "$folder/SHA256SUMS")
    if [ "$mode" = candidate ]; then
      command=(gh release create "$tag" --draft --prerelease --verify-tag --title "$tag"
        --notes "Candidate $(printf '%s\n' "$read_tag" | sed -n 's/^candidate: //p') of whizbang $version: the archives of both targets as the prerelease workflow built and installed them. A draft until the local leg has attached its receipt." "${files[@]}")
    else
      command=(gh release create "$tag" --prerelease --verify-tag --title "whizbang $version"
        --notes "Whizbang $version, for macOS 15 on Apple silicon and Ubuntu 22.04 on x86-64. The archives are those of the candidate that passed, byte for byte; check one against SHA256SUMS before unpacking it. The install guide is README.md in each archive." "${files[@]}")
    fi
    if [ "${WHIZBANG_PUBLISH_DRY_RUN:-}" = 1 ]; then
      printf '%s\n' "${command[@]}"
    else
      "${command[@]}"
      echo "release-publish: published: $tag with the two archives of $version and SHA256SUMS"
    fi
    ;;
  *) usage ;;
esac
}
main "$@"; exit
