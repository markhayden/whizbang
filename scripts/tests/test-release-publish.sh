#!/usr/bin/env bash
# The files of a candidate and of a release, and the command that publishes each
# (scripts/release-publish.sh; plan m4, task R2, items 2 and 5). `gh` here is a stand-in
# that writes down what it was asked, and the archives are a few bytes: nothing is
# published, built or contacted.
source "$(dirname "$0")/lib.sh"

script=$root/scripts/release-publish.sh
mac=whizbang-0.1.0-aarch64-apple-darwin.tar.gz
linux=whizbang-0.1.0-x86_64-unknown-linux-gnu.tar.gz
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
mkdir "$tmp/bin"
printf '%s\n' '#!/bin/bash' "printf '%s\\n' \"\$@\" >\"$tmp/gh-asked\"" >"$tmp/bin/gh"
chmod +x "$tmp/bin/gh"
publish() { env PATH="$tmp/bin:$PATH" "$script" "$@"; }

# one <folder> <archive's name> <bytes>: one target's folder, as `just package` leaves it.
one() {
  rm -rf "$1" && mkdir -p "$1"
  printf '%s\n' "$3" >"$1/$2"
  printf '%s  %s\n' "$(sha256 "$1/$2")" "$2" >"$1/SHA256SUMS"
}
one "$tmp/mac" "$mac" "the mac's archive"
one "$tmp/linux" "$linux" "the linux archive"

# --- join: both archives and one SHA256SUMS of their two lines, in the order of their
# names, whichever folder is named first.
expect_pass "join" "$script" join "$tmp/files" "$tmp/linux" "$tmp/mac"
[ "$(ls -A "$tmp/files" | tr '\n' ' ')" = "SHA256SUMS $mac $linux " ] || fail "the joined folder holds the two archives and SHA256SUMS"
[ "$(cat "$tmp/files/SHA256SUMS")" = "$(sha256 "$tmp/mac/$mac")  $mac
$(sha256 "$tmp/linux/$linux")  $linux" ] || fail "SHA256SUMS is the two lines, in the order of their names"
cmp -s "$tmp/files/$mac" "$tmp/mac/$mac" && cmp -s "$tmp/files/$linux" "$tmp/linux/$linux" || fail "the archives are joined byte for byte"
expect_fail "join into a folder that is there" "is there already" "$script" join "$tmp/files" "$tmp/mac" "$tmp/linux"
expect_fail "the same target twice" "holds no archive $linux" "$script" join "$tmp/twice" "$tmp/mac" "$tmp/mac"
one "$tmp/other" whizbang-0.2.0-x86_64-unknown-linux-gnu.tar.gz "another version"
expect_fail "two versions" "the two archives are of two versions" "$script" join "$tmp/two" "$tmp/mac" "$tmp/other"
one "$tmp/wrong" "$linux" "the linux archive"
echo "changed after its sum was taken" >>"$tmp/wrong/$linux"
expect_fail "an archive that is not its line's" "is not the one its SHA256SUMS names" "$script" join "$tmp/bad" "$tmp/mac" "$tmp/wrong"
one "$tmp/extra" "$linux" "the linux archive"
echo "more" >"$tmp/extra/notes.txt"
expect_fail "a folder with something else" "must hold one archive and its SHA256SUMS, and nothing else" "$script" join "$tmp/bad" "$tmp/mac" "$tmp/extra"
[ ! -e "$tmp/twice" ] && [ ! -e "$tmp/two" ] && [ ! -e "$tmp/bad" ] || fail "a refused join writes nothing"

# --- check: the files of a release of one version, and nothing else but the receipt.
expect_pass "check" "$script" check "$tmp/files" 0.1.0
expect_fail "another version" "holds no archive whizbang-0.1.1-aarch64-apple-darwin.tar.gz" "$script" check "$tmp/files" 0.1.1
# broken <name> <expected text> <command to break a copy, run in it>
broken() {
  rm -rf "$tmp/broken" && cp -R "$tmp/files" "$tmp/broken"
  (cd "$tmp/broken" && eval "$3")
  expect_fail "$1" "$2" "$script" check "$tmp/broken" 0.1.0
  expect_fail "$1, as a candidate" "$2" publish candidate v0.1.0-rc.1 "$tmp/broken"
  expect_fail "$1, as a release" "$2" publish release v0.1.0 "$tmp/broken"
  [ ! -e "$tmp/gh-asked" ] || fail "$1: nothing is published"
}
broken "an archive changed" "is not the one SHA256SUMS names" "echo more >>$linux"
broken "an archive missing" "holds no archive $mac" "rm $mac"
broken "no SHA256SUMS" "holds no SHA256SUMS" "rm SHA256SUMS"
broken "a file too many" "and nothing else" "echo more >notes.txt"
broken "a line too many" "must hold exactly two lines" "sed -n 1p SHA256SUMS >>SHA256SUMS"
broken "a line of another form" "must hold exactly one line for $mac" "sed -i.bak '1s/  / /' SHA256SUMS && rm SHA256SUMS.bak"
broken "an archive that is a link" "holds no archive $mac" "mv $mac kept && ln -s kept $mac"

# --- candidate: a draft, marked as a pre-release, for a candidate tag that exists.
expect_pass "the candidate" publish candidate v0.1.0-rc.2 "$tmp/files"
[ "$(sed -n '1,8p' "$tmp/gh-asked" | tr '\n' ' ')" = "release create v0.1.0-rc.2 --draft --prerelease --verify-tag --title v0.1.0-rc.2 " ] ||
  { cat "$tmp/gh-asked" >&2; fail "the candidate is: gh release create <tag> --draft --prerelease --verify-tag"; }
[ "$(sed -n '11,$p' "$tmp/gh-asked" | tr '\n' ' ')" = "$tmp/files/$mac $tmp/files/$linux $tmp/files/SHA256SUMS " ] || fail "the candidate holds the two archives and SHA256SUMS"
rm "$tmp/gh-asked"
# --- release: the same files, with the receipt beside them left out; not a draft.
echo '{}' >"$tmp/files/prerelease-receipt.json"
expect_pass "the release" publish release v0.1.0 "$tmp/files"
[ "$(sed -n '1,7p' "$tmp/gh-asked" | tr '\n' ' ')" = "release create v0.1.0 --prerelease --verify-tag --title whizbang 0.1.0 " ] ||
  { cat "$tmp/gh-asked" >&2; fail "the release is: gh release create <tag> --prerelease --verify-tag"; }
grep -x -F -e '--draft' "$tmp/gh-asked" >/dev/null && fail "a release is not a draft"
[ "$(sed -n '10,$p' "$tmp/gh-asked" | tr '\n' ' ')" = "$tmp/files/$mac $tmp/files/$linux $tmp/files/SHA256SUMS " ] || fail "the release holds the two archives and SHA256SUMS, and not the receipt"
rm "$tmp/gh-asked"

# --- A tag of the wrong kind, of another version or of no form publishes nothing.
expect_fail "a version tag as a candidate" "a candidate tag is asked for" publish candidate v0.1.0 "$tmp/files"
expect_fail "a candidate tag as a release" "a release tag is asked for" publish release v0.1.0-rc.2 "$tmp/files"
expect_fail "another version's tag" "holds no archive whizbang-0.1.1-" publish release v0.1.1 "$tmp/files"
expect_fail "a tag of another form" "is neither a version tag" publish release latest "$tmp/files"
[ ! -e "$tmp/gh-asked" ] || fail "a refused tag publishes nothing"
# A failing gh fails the script; a dry run asks gh nothing.
printf '%s\n' '#!/bin/bash' 'exit 1' >"$tmp/bin/gh"
if publish release v0.1.0 "$tmp/files" >"$tmp/out" 2>&1; then fail "a failing gh fails the script"; fi
grep -F "published:" "$tmp/out" >/dev/null && fail "nothing is said to be published when gh failed"
WHIZBANG_PUBLISH_DRY_RUN=1 expect_pass "a dry run" publish release v0.1.0 "$tmp/files"
[ "$(sed -n '1,4p' "$tmp/out" | tr '\n' ' ')" = "gh release create v0.1.0 " ] || fail "a dry run prints the command"
status=0
"$script" publish >"$tmp/out" 2>&1 || status=$?
[ "$status" = 2 ] || fail "an unknown mode is a usage error (got $status)"

finished
