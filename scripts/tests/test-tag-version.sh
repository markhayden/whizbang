#!/usr/bin/env bash
# How a tag is read (scripts/tag-version.sh; plan m4, task R2, item 1): a candidate tag is
# a base version and a candidate number, kept apart; a version tag is the base version
# alone. The base version is what is compared with a binary's version, so the binary of
# `v0.1.0-rc.2` and of `v0.1.0` both say `0.1.0`: they are the same bytes.
source "$(dirname "$0")/lib.sh"

script=scripts/tag-version.sh

# read_as <tag> [<binary's version>]: what the script prints, on one line.
read_as() { "$script" "$@" | tr '\n' ';'; }

# --- Both forms are accepted for a binary that says 0.1.0, and name the same version.
[ "$(read_as v0.1.0-rc.2 0.1.0)" = "kind: candidate;version: 0.1.0;candidate: 2;" ] ||
  fail "v0.1.0-rc.2 is candidate 2 of 0.1.0 (read: $(read_as v0.1.0-rc.2 0.1.0 2>&1 || true))"
[ "$(read_as v0.1.0 0.1.0)" = "kind: release;version: 0.1.0;candidate: none;" ] ||
  fail "v0.1.0 is the release of 0.1.0 (read: $(read_as v0.1.0 0.1.0 2>&1 || true))"
[ "$("$script" v0.1.0-rc.2 0.1.0 | sed -n 's/^version: //p')" = "$("$script" v0.1.0 0.1.0 | sed -n 's/^version: //p')" ] ||
  fail "a candidate and its release name the same version"
# Without a binary's version the tag is only read.
[ "$(read_as v12.3.45-rc.10)" = "kind: candidate;version: 12.3.45;candidate: 10;" ] || fail "a tag is read without a version to compare with"

# --- The candidate number is read apart: it is no part of the version, and a binary that
# names it is not the candidate's.
expect_fail "a binary that names the candidate number" "is not a version of three numbers" "$script" v0.1.0-rc.2 0.1.0-rc.2
[ "$("$script" v0.1.0-rc.2 | sed -n 's/^candidate: //p')" = 2 ] && [ "$("$script" v0.1.0-rc.11 | sed -n 's/^candidate: //p')" = 11 ] ||
  fail "the candidate number is a line of its own"

# A candidate number has nine digits at most: one that a shell cannot count with is no
# candidate, so nothing that compares numbers is handed one.
[ "$(read_as v0.1.0-rc.999999999)" = "kind: candidate;version: 0.1.0;candidate: 999999999;" ] || fail "a candidate number of nine digits is read"
for tag in v0.1.0-rc.1000000000 v0.1.0-rc.99999999999999999999999999; do
  expect_fail "the candidate number of $tag" "is neither a version tag" "$script" "$tag"
done

# --- Another base version than the binary's is refused, in both forms.
expect_fail "a candidate of another version" "is not the binary's version" "$script" v0.1.1-rc.1 0.1.0
expect_fail "a release of another version" "is not the binary's version" "$script" v0.1.1 0.1.0
"$script" v0.1.1-rc.1 0.1.0 >"$tmp/out" 2>/dev/null && fail "a refused tag exits with a failure"
[ ! -s "$tmp/out" ] || fail "a refused tag prints nothing on its standard output"

# --- A tag of another form is refused.
for tag in 0.1.0 v0.1 v0.1.0.0 v0.1.0-rc v0.1.0-rc. v0.1.0-rc.0 v0.1.0-rc.02 v0.1.0-rc.2a v0.1.0-RC.2 v0.1.0-beta.1 \
  v0.1.0+x v0.1.0-rc.1.2 v0.1.0-rc.1+x $'v0.1.0\n' $'v0.1.0-rc.2\n' $'v0.1.0\r' $'v0.1.0-rc.2\r' \
  v0.1.0rc.2 v0.1.0-rc2 v01.1.0 v0.01.0 v0.1.00 V0.1.0 'v0.1.0 ' ' v0.1.0' v0.1.0-rc.2-rc.3 vx.y.z refs/tags/v0.1.0 m3 '' \
  'v0.1.0
v0.1.0'; do
  expect_fail "the tag '$tag'" "is neither a version tag" "$script" "$tag"
  expect_fail "the tag '$tag' with a version" "is neither a version tag" "$script" "$tag" 0.1.0
done
# A version to compare with that is no version is refused too.
for said in 0.1 v0.1.0 '' '0.1.0 ' 1; do
  expect_fail "the version '$said'" "is not a version of three numbers" "$script" v0.1.0 "$said"
done

# --- A workflow holds its tag to its kind.
expect_pass "a candidate tag asked as one" "$script" --candidate v0.1.0-rc.2 0.1.0
expect_pass "a version tag asked as one" "$script" --release v0.1.0 0.1.0
expect_fail "a version tag where a candidate is asked for" "is a release tag, and a candidate tag is asked for" "$script" --candidate v0.1.0 0.1.0
expect_fail "a candidate tag where a release is asked for" "is a candidate tag, and a release tag is asked for" "$script" --release v0.1.0-rc.2 0.1.0

# --- Anything else is a usage error.
for args in "" "v0.1.0 0.1.0 extra"; do
  status=0
  # shellcheck disable=SC2086 # the arguments are split on purpose.
  "$script" $args >"$tmp/out" 2>&1 || status=$?
  [ "$status" = 2 ] || fail "'$args' is a usage error (got $status)"
done

finished
