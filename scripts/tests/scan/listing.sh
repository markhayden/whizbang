#!/usr/bin/env bash
# What the scan of the working tree lists, and what it does when a list cannot be had or
# cannot be trusted. The files to cover are the index's and git's own list of untracked
# files, so what goes on inside an ignored folder cannot fail the scan: not a folder
# there that cannot be read, not a build that is running. Anything git says while it
# lists fails the scan in the words of that list. A file that is gone, or is another
# file, by the time the scan looks at it fails the scan too, with the advice to run it
# again; so does a perl that cannot be run. Tested with the stand-in for git of common.sh.
source "$(dirname "$0")/common.sh"
stand_in_for_git
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }

fresh the-lists
printf '%s\n' 'made/' >.gitignore
mkdir -p made/deep/deeper kept/inner
printf 'token = %s\n' "$key" >made/deep/deeper/ignored.txt
printf 'nothing to see\n' >kept/inner/plain.txt
commit_file "docs: plain" plain.txt "nothing to see"
git add .gitignore
git commit -q --no-verify -m "chore: rules"
expect_pass "a tree with an ignored folder" "$scan"
says "secrets scan: clean (4 files)"

# --- Inside an ignored folder ---------------------------------------------------------
# A folder that cannot be read, inside an ignored folder: the scan passes.
mkdir made/deep/locked
printf 'token = %s\n' "$key" >made/deep/locked/inside.txt
chmod 000 made/deep/locked
if ls made/deep/locked >/dev/null 2>&1; then
  echo "skip: an unreadable folder inside an ignored one; this user can read a folder with mode 000"
else
  expect_pass "an unreadable folder inside an ignored folder" "$scan"
  says "secrets scan: clean (4 files)"
fi
chmod 755 made/deep/locked
# An ignored folder that itself cannot be read: the scan passes.
chmod 000 made
if ls made >/dev/null 2>&1; then
  echo "skip: an unreadable ignored folder; this user can read a folder with mode 000"
else
  expect_pass "an ignored folder that cannot be read" "$scan"
  says "secrets scan: clean (4 files)"
fi
chmod 755 made
# An ignored folder that is rewritten all the while, as by a build: twenty scans pass.
(
  while [ ! -e "$tmp/stop-rewriting" ]; do
    mkdir -p made/churn/a/b/c
    printf 'token = %s\n' "$key" >made/churn/a/b/c/file.txt
    mv made/churn made/churned 2>/dev/null || true
    rm -rf made/churned
  done
) &
rewriter=$!
i=1
while [ "$i" -le 20 ]; do
  "$scan" >"$tmp/out" 2>&1 || { touch "$tmp/stop-rewriting"; wait "$rewriter"; cat "$tmp/out" >&2; fail "scan $i of 20 beside an ignored folder that is being rewritten"; }
  i=$((i + 1))
done
touch "$tmp/stop-rewriting"
wait "$rewriter"
rm -rf made/churn made/churned "$tmp/stop-rewriting"

# --- Outside ignored folders ----------------------------------------------------------
# A folder that cannot be read and is not ignored: refused, with a count and no path.
mkdir kept/locked-folder
printf 'token = %s\n' "$key" >kept/locked-folder/inside.txt
chmod 000 kept/locked-folder
if ls kept/locked-folder >/dev/null 2>&1; then
  echo "skip: an unreadable folder that is not ignored; this user can read a folder with mode 000"
else
  expect_fail "an unreadable folder that is not ignored" "git said something while it listed the working tree (1 lines" "$scan"
  says "what was not listed is not covered"
  says "refusing to pass"
  quiet "locked-folder"
  never_clean "an unreadable folder that is not ignored"
fi
chmod 755 kept/locked-folder
rm -r kept/locked-folder

# --- What git says while it lists ------------------------------------------------------
# Each listing fails the scan in its own words; git's searches say nothing here, so no
# later step would have refused.
noisy_on() { # <words> <command...>
  local on=$1
  shift
  env PATH="$tmp/bin:$PATH" NOISY_ON="$on" "$@"
}
expect_pass "the stand-in for git, speaking on a command the scan never runs" noisy_on "ls-files --no-such" "$scan"
expect_fail "git speaks while it lists the working tree" "git said something while it listed the working tree (1 lines" \
  noisy_on "ls-files -z -co --exclude-standard" "$scan"
says "refusing to pass"
quiet "something git says"
never_clean "git speaks while it lists the working tree"
expect_fail "git speaks while it lists the tracked files a rule matches" \
  "the tracked files that an ignore rule matches could not be listed (git ls-files: exit 0, 1 lines" \
  noisy_on "ls-files -z -ci --exclude-standard" "$scan"
quiet "something git says"
never_clean "git speaks while it lists the tracked files a rule matches"
expect_fail "git speaks while it lists the untracked files" "the untracked files could not be listed (git ls-files: exit 0, 1 lines" \
  noisy_on "ls-files -z -o --exclude-standard" "$scan"
quiet "something git says"
never_clean "git speaks while it lists the untracked files"
expect_fail "git speaks while it lists the tracked files" "the tracked files could not be listed (git ls-files: exit 0, 1 lines" \
  noisy_on "ls-files -z -c" "$scan"
quiet "something git says"
never_clean "git speaks while it lists the tracked files"
mkdir "$tmp/hush-bin"
for listing in "ls-files -z -ci --exclude-standard" "ls-files -z -o --exclude-standard" "ls-files -z -c"; do
  printf '#!/bin/sh\ncase " $* " in *" %s "*) exit 3 ;; esac\nexec "%s" "$@"\n' "$listing" "$real_git" >"$tmp/hush-bin/git"
  chmod +x "$tmp/hush-bin/git"
  expect_fail "git failing without a word ($listing)" "could not be listed (git ls-files: exit 3, 0 lines" \
    env PATH="$tmp/hush-bin:$PATH" "$scan"
  never_clean "git failing without a word ($listing)"
done

# --- A tree that changes under the scan -----------------------------------------------
# An untracked file that git listed and that is gone when the scan looks it up.
printf 'nothing to see\n' >kept/soon-gone.txt
expect_fail "an untracked file that vanishes after it was listed" "1 untracked files that git listed are no longer there" \
  env PATH="$tmp/bin:$PATH" VANISH=kept/soon-gone.txt VANISH_ON="ls-files -z -o --exclude-standard" "$scan"
says "run it again"
never_clean "an untracked file that vanishes after it was listed"
# A tracked file that is gone is a file deleted here: nothing to cover.
expect_pass "a tracked file that vanishes after it was listed" \
  env PATH="$tmp/bin:$PATH" VANISH=plain.txt VANISH_ON="ls-files -z -o --exclude-standard" "$scan"
git checkout -q -- plain.txt
# A name from the search that leads nowhere.
expect_fail "a search that names a file that is not there" "1 names the search gave for the working tree lead to no file" \
  env PATH="$tmp/bin:$PATH" GHOST=no-such-file.txt "$scan"
says "run it again"
never_clean "a search that names a file that is not there"
# A file that is another file by the time the search looks in it.
expect_fail "a file replaced between the list and the search" "1 of the 4 files to cover were not looked in" \
  env PATH="$tmp/bin:$PATH" REPLACE=kept/inner/plain.txt "$scan"
says "a file that was replaced while the scan ran is one cause: run it again"
never_clean "a file replaced between the list and the search"

# --- perl ------------------------------------------------------------------------------
# The scan looks files up with perl. One that cannot be run, or fails, fails the scan in
# words, at either place it is used.
real_perl=$(command -v perl)
mkdir "$tmp/perl-bin"
printf '#!/bin/sh\ncase " $* " in *" -e "*) exit 3 ;; esac\nexec "%s" "$@"\n' "$real_perl" >"$tmp/perl-bin/perl"
chmod +x "$tmp/perl-bin/perl"
expect_fail "a perl that fails when the files to search by name are chosen" \
  "could not be looked up (perl: exit 3); the scan needs perl; refusing to pass" env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that fails when the files to search by name are chosen"
printf '#!/bin/sh\ncase " $* " in *" -ne "*) exit 3 ;; esac\nexec "%s" "$@"\n' "$real_perl" >"$tmp/perl-bin/perl"
expect_fail "a perl that fails when the files to cover are looked up" \
  "could not be looked up (perl: exit 3); the scan needs perl to tell which files the search looked in" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that fails when the files to cover are looked up"

finished
