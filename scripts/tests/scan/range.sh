#!/usr/bin/env bash
# What is outgoing: every commit the remote does not have, whatever local refs say.
source "$(dirname "$0")/common.sh"

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
quiet "$key"
fresh key-in-message
commit_file "feat: a file with $key in its message" plain.txt "nothing here"
expect_fail "a key in a commit message" "commit-message/" git push origin key-in-message
quiet "$key"

finished
