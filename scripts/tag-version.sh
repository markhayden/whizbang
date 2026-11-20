#!/usr/bin/env bash
# Reads a release tag (plan m4, task R2, item 1). The `prerelease` and `release` workflows,
# the local leg (scripts/prerelease-local.sh) and the release gate (scripts/release-gate.sh)
# all read a tag through this script, so that no two of them read one differently.
#
#   scripts/tag-version.sh [--candidate | --release] <tag> [<a binary's version>]
#
# With `--candidate` or `--release` the tag must be of that kind, or it is refused: a
# workflow that runs on one kind of tag holds its tag to it.
#
# A tag is one of two forms and nothing else:
#
#   v<major>.<minor>.<patch>            a version tag: a release
#   v<major>.<minor>.<patch>-rc.<n>     a candidate tag: candidate <n> of that version
#
# Each number is written without a sign and without a leading zero, and <n> is 1 or more
# and has nine digits at most, so that whatever compares candidate numbers can count
# with it.
# A candidate tag has two parts that are kept apart: its base version, which is what a
# binary says of itself, and its candidate number, which is the candidate's own identity
# and is in no binary. So the binary of `v0.1.0-rc.2` says `0.1.0`, as the binary of
# `v0.1.0` does: they are the same bytes.
#
# It prints three lines:
#
#   kind: candidate | release
#   version: <the base version>
#   candidate: <n> | none
#
# With a binary's version given, the tag's base version must equal it, or the tag is
# refused: nothing is printed on the standard output and the exit status is 1. A tag of
# any other form is refused the same way. Exit status 2 is a usage error.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "tag-version: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C

die() {
  echo "tag-version: $*" >&2
  exit 1
}
must=
case "${1:-}" in
  --candidate | --release) must=${1#--}; shift ;;
esac
[ $# -eq 1 ] || [ $# -eq 2 ] || { echo "usage: scripts/tag-version.sh [--candidate | --release] <tag> [<a binary's version>]" >&2; exit 2; }
tag=$1

# One number: 0, or digits that do not start with 0.
number='(0|[1-9][0-9]*)'
# The whole text is held, line breaks among them: grep reads lines, so a text of two
# lines that are each a tag would pass it.
case "$tag" in
  *$'\n'*) die "the tag is neither a version tag (v<major>.<minor>.<patch>) nor a candidate tag (v<major>.<minor>.<patch>-rc.<n>)" ;;
esac
if printf '%s\n' "$tag" | grep -x -E "v$number\.$number\.$number" >/dev/null; then
  kind=release
  version=${tag#v}
  candidate=none
elif printf '%s\n' "$tag" | grep -x -E "v$number\.$number\.$number-rc\.[1-9][0-9]{0,8}" >/dev/null; then
  kind=candidate
  version=${tag#v}
  version=${version%-rc.*}
  candidate=${tag##*-rc.}
else
  die "the tag '$tag' is neither a version tag (v<major>.<minor>.<patch>) nor a candidate tag (v<major>.<minor>.<patch>-rc.<n>)"
fi

[ -z "$must" ] || [ "$kind" = "$must" ] || die "the tag '$tag' is a $kind tag, and a $must tag is asked for"

if [ $# -eq 2 ]; then
  said=$2
  case "$said" in
    *$'\n'*) die "the version to compare with is not a version of three numbers" ;;
  esac
  printf '%s\n' "$said" | grep -x -E "$number\.$number\.$number" >/dev/null ||
    die "the version to compare with, '$said', is not a version of three numbers"
  [ "$version" = "$said" ] ||
    die "the tag '$tag' is of version $version, which is not the binary's version, $said"
fi

printf 'kind: %s\nversion: %s\ncandidate: %s\n' "$kind" "$version" "$candidate"
}
main "$@"; exit
