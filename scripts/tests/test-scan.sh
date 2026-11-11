#!/usr/bin/env bash
# The scans: what a push would publish is covered commit by commit, against what the remote
# has; the attribution check; the clean-room scan fails closed and prints counts only.
# Every case runs in a throwaway repository with a throwaway bare remote and a made-up word
# list. Keys, addresses and attribution lines are put together here at run time, so this
# file holds nothing the real scan would flag.
source "$(dirname "$0")/lib.sh"

scan="$root/scripts/scan.sh"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

key="sk""-$(printf '%s' abcdefghij0123456789ABCDEFGHIJ)"
home_path="/Us""ers/someone/notes"
address="someone@""corp.test"
word=zorblaxquint
credit="Co""-Authored-By: Some One <one@example.com>"
made="Generated ""with a tool"
zeros=0000000000000000000000000000000000000000

words="$tmp/words"
printf '%s\n\nflimwozzle\n' "$word" >"$words"
export WHIZBANG_WORDLIST="$words"

# A repository whose `main` is published to a bare remote, with this repo's hooks and scan
# installed (ignored there, as untracked tooling), so `git push` runs the real pre-push hook.
remote="$tmp/remote.git"
repo="$tmp/repo"
git init -q --bare -b main "$remote"
git init -q -b main "$repo"
cd "$repo"
git config user.name "A Tester"
git config user.email "tester@example.com"
mkdir scripts
cp "$scan" scripts/scan.sh
mkdir scripts/hooks
cp "$root/scripts/hooks/pre-push" "$root/scripts/hooks/commit-msg" scripts/hooks/
echo "scripts/" >.git/info/exclude
git config core.hooksPath scripts/hooks
git remote add origin "$remote"
echo "hello" >readme.txt
git add readme.txt
git commit -q -m "docs: a readme"
git push -q origin main >/dev/null 2>&1
first=$(git rev-parse HEAD)

# commit_file <message> <file> <content>: one commit that sets the file's content. It is
# made with the commit hooks bypassed, as a commit from another machine would arrive.
commit_file() {
  mkdir -p "$(dirname "$2")"
  printf '%s\n' "$3" >"$2"
  git add -- "$2"
  git commit -q --no-verify -m "$1"
}
# fresh <name>: a new branch from the published commit.
fresh() { git checkout -q -B "$1" "$first"; }
# not_on_remote <ref>: the push was refused, so the remote does not have the ref.
not_on_remote() {
  [ -z "$(git ls-remote "$remote" "$1")" ] || fail "$1 reached the remote"
}
# quiet <text>: the last output does not hold the text.
quiet() {
  if grep -q -F -e "$1" "$tmp/out"; then fail "the output holds text it must never print"; fi
}

# The working tree.
expect_pass "a clean working tree" "$scan"
grep -q "clean-room scan: 0 matching lines" "$tmp/out" || fail "a clean tree: the count line"
printf 'token = %s\n' "$key" >planted.txt
expect_fail "a key in the working tree" "planted.txt:1" "$scan"
quiet "$key"
expect_fail "a key in the working tree, secrets only" "planted.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
rm planted.txt
printf 'see %s\n' "$home_path" >planted.txt
expect_fail "a home path in the working tree" "planted.txt:1" "$scan"
printf 'write to %s\n' "$address" >planted.txt
expect_fail "an email address in the working tree" "planted.txt:1" "$scan"
printf 'write to someone@example.com or git@github.com:o/r\n' >planted.txt
expect_pass "the allowed addresses" "$scan"
rm planted.txt

# The values of a local .env: the variable is named, the value is never printed.
secret_value="value-$(printf '%s' 0918273645)-of-env"
printf '# local\nexport SOME_TOKEN="%s"\nSHORT=abc\n' "$secret_value" >.env
printf 'x = %s\n' "$secret_value" >planted.txt
expect_fail "a value from .env in the working tree" "planted.txt:1 (the value of SOME_TOKEN in .env)" "$scan"
quiet "$secret_value"
printf 'abc is short\n' >planted.txt
expect_pass "a short .env value is not searched for" "$scan"
rm planted.txt
fresh env-range
commit_file "feat: a setting" settings.txt "x = $secret_value"
expect_fail "a value from .env in a commit" "settings.txt:1 (the value of SOME_TOKEN in .env)" "$scan" --secrets-only --range "$first" HEAD
quiet "$secret_value"
rm .env

# A key added in one commit and deleted in the next.
fresh add-then-delete
commit_file "feat: a config" config.txt "token = $key"
added=$(git rev-parse HEAD)
git rm -q config.txt
git commit -q -m "fix: drop the config"
expect_pass "the tree after the deletion is clean" "$scan"
expect_fail "a key added then deleted, pushed" "$added:config.txt:1" git push origin add-then-delete
quiet "$key"
not_on_remote refs/heads/add-then-delete
expect_fail "a key added then deleted, as a range" "$added:config.txt:1" "$scan" --range "$first" HEAD
expect_fail "a key added then deleted, as CI runs it" "$added:config.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD

# The same on local `main`, never published, pushed under a new name. Being on local main,
# or on a remote-tracking ref that says more than the remote has, is not being published.
git checkout -q main
git merge -q --ff-only add-then-delete
expect_fail "local main, pushed under a new name" "$added:config.txt:1" git push origin main:refs/heads/brand-new
not_on_remote refs/heads/brand-new
git update-ref refs/remotes/origin/main "$(git rev-parse main)"
expect_fail "a remote-tracking ref ahead of the remote" "$added:config.txt:1" git push origin main:refs/heads/brand-new
not_on_remote refs/heads/brand-new
git update-ref refs/remotes/origin/main "$first"
git reset -q --hard "$first"

# Paths and messages are material too.
fresh key-in-path
commit_file "feat: a file" "dir/$key.txt" "nothing here"
expect_fail "a key in a path" "commit-paths/" git push origin key-in-path
fresh key-in-message
commit_file "feat: a file with $key in its message" plain.txt "nothing here"
expect_fail "a key in a commit message" "commit-message/" git push origin key-in-message
quiet "$key"

# Attribution, in a range (what CI runs over a pull request) and on a push.
fresh credited
commit_file "feat: a thing

$credit" thing.txt "a thing"
expect_fail "a credit line, as CI runs it" "the message has an attribution line" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
commit_file "feat: a later, clean commit" thing.txt "another thing"
expect_fail "a credit line under a clean commit" "the message has an attribution line" "$scan" --range "$first" HEAD
expect_fail "a credit line, pushed" "the message has an attribution line" git push origin credited
not_on_remote refs/heads/credited
fresh made-with
commit_file "feat: a thing

$made" thing.txt "a thing"
expect_fail "a made-with line" "the message has an attribution line" "$scan" --secrets-only --range "$first" HEAD
fresh agent-author
printf 'a thing\n' >thing.txt
git add thing.txt
git -c user.name="Cl""aude" commit -q --no-verify -m "feat: a thing"
expect_fail "an agent as the author" "the author or committer is an agent or a model" "$scan" --secrets-only --range "$first" HEAD

# The commit-msg hook applies the same rule.
printf 'feat: a thing\n\n%s\n' "$credit" >"$tmp/message"
expect_fail "the commit-msg hook, a credit line" "remove the attribution line" scripts/hooks/commit-msg "$tmp/message"
printf 'feat: a thing\n\nIt explains itself.\n' >"$tmp/message"
expect_pass "the commit-msg hook, a clean message" scripts/hooks/commit-msg "$tmp/message"

# An annotated tag whose message holds a key, on a commit the remote already has.
git checkout -q main
git tag -a v-planted -m "release notes: $key" "$first"
expect_fail "a key in a tag message" "tag-message/" git push origin v-planted
quiet "$key"
not_on_remote refs/tags/v-planted
git tag -a v-credited -m "release

$credit" "$first"
expect_fail "a credit line in a tag message" "the message has an attribution line" git push origin v-credited

# The clean-room scan covers names, messages, paths and contents, and prints counts only.
git branch "topic-$word" "$first"
expect_fail "a listed word in a branch name" "ref names 1" git push origin "topic-$word"
quiet "$word"
not_on_remote "refs/heads/topic-$word"
git tag "v-$word" "$first"
expect_fail "a listed word in a tag name" "ref names 1" git push origin "v-$word"
quiet "$word"
git tag -a v-noted -m "notes on ${word}" "$first"
expect_fail "a listed word in a tag message" "tag messages 1" git push origin v-noted
quiet "$word"
fresh listed
commit_file "feat: about $word" "dir-$word/file.txt" "it says $word, once"
git rm -q -r "dir-$word"
git commit -q -m "fix: remove it"
# The path counts twice: the commit that adds the file and the one that removes it.
expect_fail "a listed word in a commit's message, path and contents" \
  "clean-room scan: 4 matching lines (contents 1, paths 2, messages 1, ref names 0, tag messages 0)" git push origin listed
quiet "$word"
not_on_remote refs/heads/listed
git checkout -q main
printf 'It says %s here.\n' "$(printf '%s' "$word" | tr 'a-z' 'A-Z')" >planted.txt
expect_fail "a listed word in the working tree, in another case" "clean-room scan: 1 matching lines" "$scan"
quiet "$word"
rm planted.txt

# The clean-room scan fails closed.
: >"$tmp/empty"
printf '\n  \n' >"$tmp/blank"
expect_fail "an empty word list" "unreadable or empty" env WHIZBANG_WORDLIST="$tmp/empty" "$scan"
expect_fail "a blank word list" "unreadable or empty" env WHIZBANG_WORDLIST="$tmp/blank" "$scan"
expect_fail "a missing word list" "unreadable or empty" env WHIZBANG_WORDLIST="$tmp/none" "$scan"
expect_fail "no word list variable" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST "$scan"
expect_fail "no word list variable, on a range" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST "$scan" --range "$first" HEAD
fresh no-list
commit_file "feat: clean" clean.txt "clean"
expect_fail "no word list variable, on a push" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST git push origin no-list
not_on_remote refs/heads/no-list

# A range that cannot be established fails.
unknown=1234567890123456789012345678901234567890
push_line() { printf '%s\n' "$1" | "$scan" --pre-push origin "$remote"; }
expect_fail "an unknown remote sha" "cannot establish the range" \
  push_line "refs/heads/no-list $(git rev-parse HEAD) refs/heads/no-list $unknown"
expect_fail "an unknown local sha" "cannot resolve" \
  push_line "refs/heads/no-list $unknown refs/heads/no-list $zeros"
expect_fail "a malformed line" "cannot read the push" push_line "refs/heads/no-list $(git rev-parse HEAD)"
expect_fail "a remote that cannot be asked" "cannot establish what the remote" \
  bash -c "printf '%s\n' 'refs/heads/x $(git rev-parse HEAD) refs/heads/x $zeros' | '$scan' --pre-push origin '$tmp/no-such-remote.git'"
expect_fail "an unknown base" "cannot establish the range" "$scan" --range "$unknown" HEAD
expect_fail "an unknown head" "cannot establish the range" "$scan" --range "$first" no-such-branch
expect_fail "half a range" "usage" "$scan" --range "$first"

# A clean range passes, and the push goes through.
fresh clean-work
commit_file "feat: one" one.txt "one"
commit_file "feat: two" two.txt "two"
git tag -a v-clean -m "a clean release" HEAD
expect_pass "a clean range" "$scan" --range "$first" HEAD
grep -q "secrets scan: clean (2 commits, 1 trees, 0 tags)" "$tmp/out" || fail "a clean range: what was covered"
expect_pass "a clean range, as CI runs it" env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
expect_pass "a clean push" git push origin clean-work v-clean
grep -q "secrets scan: clean (2 commits, 1 trees, 1 tags)" "$tmp/out" || fail "a clean push: what was covered"
grep -q "attribution check: clean" "$tmp/out" || fail "a clean push: the attribution line"
grep -q "clean-room scan: 0 matching lines" "$tmp/out" || fail "a clean push: the count line"
[ "$(git ls-remote "$remote" refs/heads/clean-work | cut -f1)" = "$(git rev-parse HEAD)" ] || fail "a clean push did not arrive"
# A second push of the same commits under another name has nothing new but the name.
expect_pass "published commits under a new name" git push origin clean-work:refs/heads/again
grep -q "secrets scan: clean (0 commits, 1 trees, 0 tags)" "$tmp/out" || fail "a new name: what was covered"
expect_pass "a deletion" git push origin :refs/heads/again
