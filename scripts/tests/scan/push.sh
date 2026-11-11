#!/usr/bin/env bash
# A push or a range that cannot be read fails; a clean one passes and arrives.
source "$(dirname "$0")/common.sh"

# A range that cannot be established fails; a push that cannot be read fails.
unknown=1234567890123456789012345678901234567890
tip_now=$(git rev-parse HEAD)
push_line() { printf "$1" "$2" | "$scan" --pre-push origin "$remote"; }
expect_fail "an unknown remote sha" "cannot establish the range" \
  push_line '%s\n' "refs/heads/no-list $tip_now refs/heads/no-list $unknown"
expect_fail "an unknown local sha" "cannot resolve" \
  push_line '%s\n' "refs/heads/no-list $unknown refs/heads/no-list $zeros"
expect_fail "a line with too few fields" "cannot read the push" push_line '%s\n' "refs/heads/no-list $tip_now"
expect_fail "a line with too many fields" "cannot read the push" \
  push_line '%s\n' "refs/heads/no-list $tip_now refs/heads/no-list $zeros more"
expect_fail "a shortened sha" "cannot read the push" push_line '%s\n' "refs/heads/no-list 0 refs/heads/no-list $zeros"
expect_fail "a remote that cannot be asked" "cannot establish what the remote" \
  bash -c "printf '%s\n' 'refs/heads/x $tip_now refs/heads/x $zeros' | '$scan' --pre-push origin '$tmp/no-such-remote.git'"
expect_fail "an unknown base" "cannot establish the range" "$scan" --range "$unknown" HEAD
expect_fail "a base of zeros" "cannot establish the range" "$scan" --range "$zeros" HEAD
expect_fail "an unknown head" "cannot establish the range" "$scan" --range "$first" no-such-branch
expect_fail "half a range" "usage" "$scan" --range "$first"
# The last line of a push counts even with no newline after it.
fresh no-newline
commit_file "feat: a config" config.txt "token = $key"
expect_fail "a push line with no newline" "config.txt:1" \
  push_line '%s' "refs/heads/no-newline $(git rev-parse HEAD) refs/heads/no-newline $zeros"

# A clean range passes, and the push goes through.
fresh clean-work
commit_file "feat: one" one.txt "one"
commit_file "feat: two" two.txt "two"
git tag -a v-clean -m "a clean release" HEAD
expect_pass "a clean range" "$scan" --range "$first" HEAD
says "secrets scan: clean (2 commits, 1 trees, 0 tags)"
expect_pass "a clean range, as CI runs it" env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
expect_pass "a clean push" git push origin clean-work v-clean
says "secrets scan: clean (2 commits, 1 trees, 1 tags)"
says "attribution check: clean"
says "clean-room scan: 0 matching lines"
[ "$(git ls-remote "$remote" refs/heads/clean-work | cut -f1)" = "$(git rev-parse HEAD)" ] || fail "a clean push did not arrive"
# A second push of the same commits under another name has nothing new but the name.
expect_pass "published commits under a new name" git push origin clean-work:refs/heads/again
says "secrets scan: clean (0 commits, 1 trees, 0 tags)"
expect_pass "a deletion" git push origin :refs/heads/again
