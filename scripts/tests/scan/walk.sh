#!/usr/bin/env bash
# How the census lists the untracked files of the working tree: folder by folder, and it
# never enters a folder that git says is ignored. So nothing inside an ignored folder can
# make the scan refuse: not a folder there that cannot be read, not one that changes during
# the walk (a build that is running). Outside ignored folders it still refuses whatever it
# cannot list, with a count and no path. And it does not guess: anything find or git says
# while listing, or a question git does not answer, fails the scan in the words of that
# step, not of a later one. Tested with the stand-ins for find and for git of common.sh.
source "$(dirname "$0")/common.sh"
stand_in_for_git
stand_in_for_find
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }
could_not_list="the files of the working tree could not all be listed"
run_again="a file or folder may have changed during the walk: run it again"

mkdir "$tmp/hush-bin"
printf '#!/bin/sh\n' >"$tmp/hush-bin/git"
chmod +x "$tmp/hush-bin/git"

fresh the-walk
printf '%s\n' 'made/' >.gitignore
mkdir -p made/deep/deeper kept/inner
printf 'token = %s\n' "$key" >made/deep/deeper/ignored.txt
printf 'nothing to see\n' >kept/inner/plain.txt
commit_file "docs: plain" plain.txt "nothing to see"
git add .gitignore
git commit -q --no-verify -m "chore: rules"

# The stand-in for find changes nothing when it is told to do nothing.
expect_pass "the stand-in for find, as find is" finding nothing "$scan"
says "secrets scan: clean (4 files)"

# --- Inside an ignored folder ---------------------------------------------------------
# The walk is never asked to look in an ignored folder and lists nothing inside one. The
# stand-in fails if it is, as find fails in a folder that changes under it.
expect_pass "an ignored folder that would fail the walk if it were entered" \
  finding keep-out FIND_FOLDER=made "$scan"
says "secrets scan: clean (4 files)"
# The stand-in does fail a walk that enters the folder it guards.
expect_fail "a folder that is not ignored and changes under the walk" "$could_not_list" \
  finding keep-out FIND_FOLDER=kept "$scan"
says "$run_again"
never_clean "a folder that changes under the walk"
# Nor is an ignored folder that holds a tracked file: that file comes from the index.
printf 'token = %s\n' "$key" >made/tracked.txt
git add -f made/tracked.txt
git commit -q --no-verify -m "feat: a tracked file in an ignored folder"
expect_fail "an ignored folder with a tracked file, which would fail the walk if it were entered" "made/tracked.txt:1" \
  finding keep-out FIND_FOLDER=made "$scan"
quiet "ignored.txt"
quiet "could not"
git rm -q -f made/tracked.txt
git commit -q --no-verify -m "feat: no tracked file there"
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

# --- Outside ignored folders ----------------------------------------------------------
# A folder that cannot be read and is not ignored: refused, with a count and no path.
mkdir kept/locked-folder
printf 'token = %s\n' "$key" >kept/locked-folder/inside.txt
chmod 000 kept/locked-folder
if ls kept/locked-folder >/dev/null 2>&1; then
  echo "skip: an unreadable folder that is not ignored; this user can read a folder with mode 000"
else
  expect_fail "an unreadable folder that is not ignored" "refusing to pass" "$scan"
  says "1 lines of"
  says "what was not listed is not covered"
  quiet "locked-folder"
  never_clean "an unreadable folder that is not ignored"
  # The same when git's own listing says nothing of it: the walk refuses by itself.
  printf '#!/bin/sh\ncase " $* " in *" ls-files -z -co "*) exec "%s" "$@" 2>/dev/null ;; esac\nexec "%s" "$@"\n' \
    "$real_git" "$real_git" >"$tmp/hush-bin/git"
  expect_fail "an unreadable folder that is not ignored, the walk alone" "$could_not_list (find failed, with 1 lines of errors" \
    env PATH="$tmp/hush-bin:$PATH" "$scan"
  says "$run_again"
  quiet "locked-folder"
  never_clean "an unreadable folder, the walk alone"
fi
chmod 755 kept/locked-folder
rm -r kept/locked-folder

# A folder that is not ignored and vanishes during the walk: refused, and the message
# says to run the scan again. find's second call lists the folders inside `kept`, so
# after it the walk knows of `kept/inner`: the third call would list the folders inside
# that, and the fourth the files of every folder the walk kept.
for after in 2 3; do
  mkdir -p kept/inner
  printf 'nothing to see\n' >kept/inner/plain.txt
  expect_fail "a folder that vanishes after call $after of find" "$could_not_list (find failed, with 1 lines of errors" \
    finding vanish FIND_FOLDER=kept/inner FIND_AFTER="$after" "$scan"
  says "$run_again"
  says "refusing to pass"
  quiet "kept/inner"
  never_clean "a folder that vanishes after call $after of find"
done
mkdir -p kept/inner
printf 'nothing to see\n' >kept/inner/plain.txt

# find failing: with a line that names a path, which is counted and not shown; without a
# word; and a line from a find that exits 0.
expect_fail "find failing" "$could_not_list (find failed, with 1 lines of errors" finding speak "$scan"
quiet "a-path-it-names"
never_clean "find failing"
expect_fail "find failing without a word" "$could_not_list (find failed, with 0 lines of errors" finding fail "$scan"
never_clean "find failing without a word"
expect_fail "find saying something and going on" "$could_not_list (find said something: 1 lines of errors" finding warn "$scan"
quiet "a-path-it-names"
never_clean "find saying something and going on"

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
expect_fail "git speaks while it lists the tracked files" "the tracked files could not be listed (git ls-files: exit 0, 1 lines" \
  noisy_on "ls-files -z -c" "$scan"
quiet "something git says"
never_clean "git speaks while it lists the tracked files"
expect_fail "git speaks while it lists the tracked files a rule matches" \
  "the tracked files that an ignore rule matches could not be listed (git ls-files: exit 0, 1 lines" \
  noisy_on "ls-files -z -ci --exclude-standard" "$scan"
quiet "something git says"
never_clean "git speaks while it lists the tracked files a rule matches"
for listing in "ls-files -z -c" "ls-files -z -ci --exclude-standard" "check-ignore -z --stdin -v -n"; do
  printf '#!/bin/sh\ncase " $* " in *" %s "*) exit 3 ;; esac\nexec "%s" "$@"\n' "$listing" "$real_git" >"$tmp/hush-bin/git"
  case "$listing" in
    ls-files*) refusal="could not be listed (git ls-files: exit 3, 0 lines" ;;
    *) refusal="could not be told from the ignored ones (git check-ignore: exit 3, 0 lines" ;;
  esac
  expect_fail "git failing without a word ($listing)" "$refusal" env PATH="$tmp/hush-bin:$PATH" "$scan"
  never_clean "git failing without a word ($listing)"
done
# check-ignore warns and exits 0: here a folder is ignored (`made`), so 0 is its answer.
expect_fail "git check-ignore warning, something ignored" \
  "could not be told from the ignored ones (git check-ignore: exit 0, 1 lines" noisy_on check-ignore "$scan"
says "refusing to pass"
quiet "something git says"
never_clean "git check-ignore warning, something ignored"
# ... and exits 1: with nothing ignored at all, 1 is its answer.
fresh nothing-ignored
commit_file "docs: plain" plain.txt "nothing to see"
mv scripts "$tmp/scripts-aside"
: >.git/info/exclude
expect_pass "a tree where nothing is ignored" "$scan"
expect_fail "git check-ignore warning, nothing ignored" \
  "could not be told from the ignored ones (git check-ignore: exit 1, 1 lines" noisy_on check-ignore "$scan"
quiet "something git says"
never_clean "git check-ignore warning, nothing ignored"
# A question git does not answer is not taken for "not ignored", nor for "ignored".
expect_fail "git check-ignore answering nothing" "git check-ignore answered for 0 of the 2 paths it was asked about" \
  env PATH="$tmp/bin:$PATH" MUTE=check-ignore "$scan"
says "refusing to pass"
never_clean "git check-ignore answering nothing"
mv "$tmp/scripts-aside" scripts
printf 'scripts/\n.env\n' >.git/info/exclude

finished
