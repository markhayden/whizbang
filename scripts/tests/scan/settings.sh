#!/usr/bin/env bash
# The scan against a careless caller's settings. Configuration put into the environment
# of every git (GIT_CONFIG_COUNT and its keys and values) does not reach the gits that
# list and search; it does reach the one question to the remote of a push, which may
# need it. And settings in a configuration file that would change what a search prints,
# or stop it, do neither.
source "$(dirname "$0")/common.sh"
stand_in_for_git
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }

fresh settings
commit_file "feat: a key" lost.txt "token = $key"

# The stand-in notes, for each git: the count, a key and a value from the environment,
# and what two settings are for that git.
rm -f "$tmp/env-log"
expect_fail "a scan with configuration in the environment" "lost.txt:1" \
  env PATH="$tmp/bin:$PATH" ENV_LOG="$tmp/env-log" GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=grep.threads GIT_CONFIG_VALUE_0=8 \
  GIT_CONFIG_KEY_1=core.hooksPath GIT_CONFIG_VALUE_1=/injected "$scan"
[ -s "$tmp/env-log" ] || fail "the scan's gits were seen"
grep -q -x -F -e "unset|unsetunset||scripts/hooks" "$tmp/env-log" || fail "the scan's gits were seen in the repository"
# (A search of the scan's own text files runs outside the repository: no hooks path there.)
if grep -v -x -F -e "unset|unsetunset||scripts/hooks" -e "unset|unsetunset||" "$tmp/env-log" >&2; then
  fail "configuration from the environment reached a git of the scan (above: the count, a key and a value, grep.threads, core.hooksPath)"
fi
# The same for a range and for what a push would publish; there one git, and only one,
# has the caller's configuration: the question to the remote.
rm -f "$tmp/env-log"
printf 'refs/heads/settings %s refs/heads/settings %s\n' "$(git rev-parse HEAD)" "$zeros" >"$tmp/push-line"
expect_fail "a push scanned with configuration in the environment" "lost.txt:1" \
  env PATH="$tmp/bin:$PATH" ENV_LOG="$tmp/env-log" GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=grep.threads GIT_CONFIG_VALUE_0=8 \
  GIT_CONFIG_KEY_1=core.hooksPath GIT_CONFIG_VALUE_1=/injected bash -c "'$scan' --pre-push origin '$remote' <'$tmp/push-line'"
[ "$(grep -c -x -F -e "2|grep.threads/injected|8|/injected" "$tmp/env-log")" = 1 ] ||
  { cat "$tmp/env-log" >&2; fail "one git of a push, the question to the remote, has the caller's configuration"; }
if grep -v -x -F -e "unset|unsetunset||scripts/hooks" -e "unset|unsetunset||" -e "2|grep.threads/injected|8|/injected" "$tmp/env-log" >&2; then
  fail "configuration from the environment reached a listing or a search of a push"
fi

# A remote that is reached only through a setting in the environment: the hook can ask
# it, so a clean push arrives and one with a key is refused for the key.
fresh by-another-name
commit_file "docs: plain" plain.txt "nothing to see"
by_name() { env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="url.$tmp/.insteadOf" GIT_CONFIG_VALUE_0=elsewhere: "$@"; }
expect_pass "a clean push to a remote named through the environment" by_name git push elsewhere:remote.git by-another-name
says "secrets scan: clean (1 commits, 1 trees, 0 tags)"
[ "$(git ls-remote "$remote" refs/heads/by-another-name | cut -f1)" = "$(git rev-parse HEAD)" ] || fail "the clean push did not arrive"
commit_file "feat: a key" config.txt "token = $key"
expect_fail "a push with a key to a remote named through the environment" "config.txt:1" \
  by_name git push elsewhere:remote.git by-another-name
[ "$(git ls-remote "$remote" refs/heads/by-another-name | cut -f1)" = "$(git rev-parse HEAD~1)" ] || fail "the push with a key did not arrive"

# Settings in a configuration file: colour always, full names from a subfolder,
# submodules in every command that can take them. A search prints what it printed.
printf '[color]\n\tui = always\n[grep]\n\tfullName = true\n[submodule]\n\trecurse = true\n' >"$tmp/careless"
careless() { env GIT_CONFIG_GLOBAL="$tmp/careless" "$@"; }
fresh careless
commit_file "feat: a key" lost.txt "token = $key"
printf 'it says %s\n' "$word" >untracked.txt
# ... and a tracked file that a rule matches, which is searched by name.
printf '%s\n' '*.log' >.gitignore
printf 'token = %s\n' "$key" >kept.log
git add -f kept.log
git commit -q --no-verify -m "feat: a log"
expect_fail "a scan with careless settings" "lost.txt:1" careless "$scan"
says "kept.log:1"
says "clean-room scan: 1 matching lines (contents 1, paths 0)"
quiet "git said something"
quiet "were not looked in"
rm untracked.txt .gitignore
expect_fail "a range with careless settings" "lost.txt:1" careless "$scan" --range "$first" HEAD
quiet "git said something"
printf 'refs/heads/careless %s refs/heads/careless %s\n' "$(git rev-parse HEAD)" "$zeros" >"$tmp/push-line"
expect_fail "a push with careless settings" "lost.txt:1" careless bash -c "'$scan' --pre-push origin '$remote' <'$tmp/push-line'"
git reset -q --hard "$first"
commit_file "docs: plain" plain.txt "nothing to see"
expect_pass "a clean scan with careless settings" careless "$scan"
says "secrets scan: clean (2 files)"
expect_pass "a clean range with careless settings" careless "$scan" --range "$first" HEAD
says "secrets scan: clean (1 commits, 1 trees, 0 tags)"
# A commit that adds a submodule: what the submodule holds is another repository's to
# scan, with the setting as without it.
git init -q -b main "$tmp/inner"
(
  cd "$tmp/inner"
  git config user.name "A Tester"
  git config user.email "tester@example.com"
  printf 'token = %s\n' "$key" >inside.txt
  git add inside.txt
  git commit -q -m "feat: a key in another repository"
)
git -c protocol.file.allow=always submodule --quiet add "$tmp/inner" inner-part >/dev/null 2>&1
git commit -q --no-verify -m "feat: a submodule"
expect_pass "a range with a submodule" "$scan" --range HEAD~1 HEAD
quiet "inside.txt"
expect_pass "a range with a submodule, with careless settings" careless "$scan" --range HEAD~1 HEAD
quiet "inside.txt"

finished
