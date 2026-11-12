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
run_again="a file or folder changed during the walk, or one cannot be read: run it again, and if it refuses again, look for a folder that cannot be read"

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
for listing in "ls-files -z -c" "ls-files -z -ci --exclude-standard" "check-ignore -v -n --no-index"; do
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
# Nor is an answer for a path nobody asked about.
expect_fail "git check-ignore answering for more than it was asked" "git check-ignore answered for 3 of the 2 paths it was asked about" \
  env PATH="$tmp/bin:$PATH" EXTRA=check-ignore "$scan"
never_clean "git check-ignore answering for more than it was asked"
mv "$tmp/scripts-aside" scripts
printf 'scripts/\n.env\n' >.git/info/exclude

# --- Names ----------------------------------------------------------------------------
# An ignored file whose name, read as a pattern, matches a tracked file is still ignored:
# git is asked about names, by the rules alone.
fresh pattern-names
printf '%s\n' '*.tmp' >.gitignore
printf 'nothing to see\n' >axb.tmp
git add -f axb.tmp .gitignore
git commit -q --no-verify -m "feat: a tracked file that a rule matches"
for name in 'a*b.tmp' 'a?b.tmp' 'a[wxy]b.tmp' '*.tmp'; do
  printf 'token = %s\n' "$key" >"$name"
  expect_pass "an ignored file named $name beside the tracked axb.tmp" "$scan"
  says "secrets scan: clean (3 files)"
  rm "$name"
done

# A name that holds a byte the scan uses to tell names and fields apart (\001, \002) is
# refused, with a count and without the name: tracked or not, a file or a folder, in the
# working tree and in commits.
fresh byte-names
for byte in 001 002; do
  odd=$(printf "odd\\${byte}name")
  held="names hold the byte \\001 or \\002"
  printf 'nothing to see\n' >"$odd.txt"
  expect_fail "an untracked file with byte $byte in its name" "1 $held" "$scan"
  says "refusing to pass"
  quiet "odd"
  never_clean "an untracked file with byte $byte in its name"
  git add -- "$odd.txt"
  git commit -q --no-verify -m "feat: a file with a byte in its name"
  expect_fail "a tracked file with byte $byte in its name" "1 $held" "$scan"
  never_clean "a tracked file with byte $byte in its name"
  expect_fail "a commit that adds a file with byte $byte in its name" "1 $held" "$scan" --range HEAD~1 HEAD
  quiet "odd"
  never_clean "a commit that adds a file with byte $byte in its name"
  # A commit that adds it and one that takes it away: the tree of the head has no such
  # name, the commits' changed paths do.
  git rm -q -- "$odd.txt"
  git commit -q --no-verify -m "feat: no such file"
  expect_fail "commits that add and remove a file with byte $byte in its name" "$held" "$scan" --range HEAD~2 HEAD
  quiet "odd"
  never_clean "commits that add and remove a file with byte $byte in its name"
  git reset -q --hard HEAD~2
  # Tracked in an ignored folder, where the walk does not go: the name comes from the index.
  mkdir hidden
  printf 'nothing to see\n' >"hidden/$odd.txt"
  printf '%s\n' 'hidden/' >.gitignore
  git add -f -- "hidden/$odd.txt" .gitignore
  expect_fail "a tracked file with byte $byte in its name, in an ignored folder" "1 $held" "$scan"
  quiet "odd"
  never_clean "a tracked file with byte $byte in its name, in an ignored folder"
  git reset -q --hard
  rm -rf hidden
  mkdir "$odd"
  expect_fail "an untracked folder with byte $byte in its name, empty" "1 $held" "$scan"
  quiet "odd"
  printf 'nothing to see\n' >"$odd/in.txt"
  expect_fail "an untracked folder with byte $byte in its name" "1 $held" "$scan"
  never_clean "an untracked folder with byte $byte in its name"
  git add -- "$odd/in.txt"
  git commit -q --no-verify -m "feat: a folder with a byte in its name"
  expect_fail "a tracked folder with byte $byte in its name" "1 $held" "$scan"
  never_clean "a tracked folder with byte $byte in its name"
  # The tree of the head, when no commit of the range changed the path.
  commit_file "docs: more" more.txt "nothing to see"
  expect_fail "a tree that holds a folder with byte $byte in its name" "1 $held" "$scan" --range HEAD~1 HEAD
  never_clean "a tree that holds a folder with byte $byte in its name"
  git reset -q --hard HEAD~2
done

# A decomposed name (an accent after its letter). On macOS git composes the names it
# reads from a folder and find does not, so the census composes them as git does; the
# search and the list then agree. Elsewhere the two forms are two names.
fresh composed-names
nfc=$(printf 'caf\303\251')
nfd=$(printf 'cafe\314\201')
mkdir "dir-$nfd" "empty-$nfd"
printf 'nothing to see\n' >"file-$nfd.txt"
printf 'nothing to see\n' >"file-$nfc-too.txt"
printf 'nothing to see\n' >"dir-$nfd/in-$nfd.txt"
expect_pass "untracked files and folders with decomposed names" "$scan"
says "secrets scan: clean (4 files)"
printf 'token = %s\n' "$key" >"dir-$nfd/in-$nfd.txt"
expect_fail "a key in a file with a decomposed name" ":1" "$scan"
printf 'nothing to see\n' >"dir-$nfd/in-$nfd.txt"
# Tracked under its decomposed name, as a commit made elsewhere brings it. On macOS the
# search names the file as its folder does, composed; the list and the search still agree.
git -c core.precomposeunicode=false add -- "file-$nfd.txt" "dir-$nfd/in-$nfd.txt"
git commit -q --no-verify -m "feat: decomposed names"
[ "$(git ls-files -z | LC_ALL=C tr -d -c '\314' | wc -c | tr -d ' ')" = 3 ] || fail "the index holds the decomposed names"
expect_pass "tracked files with decomposed names" "$scan"
says "secrets scan: clean"
expect_pass "tracked files with decomposed names, a range" "$scan" --range HEAD~1 HEAD
printf 'token = %s\n' "$key" >"file-$nfd.txt"
expect_fail "a key in a tracked file with a decomposed name" ".txt:1" "$scan"
# ... and when a rule matches it as well, so that it is searched by its name in the index.
printf '%s\n' '*.txt' >.gitignore
expect_fail "a key in a tracked file with a decomposed name that a rule matches" ".txt:1" "$scan"
quiet "were not looked in"
# A rule written decomposed, as the index has the name: the file is searched by its name
# in the index, and where git composes names by the composed one as well. Two names for
# one file are one file to cover.
printf '%s\n' "file-$nfd.txt" >.gitignore
expect_fail "a key in a tracked file with a decomposed name that a decomposed rule matches" ".txt:1" "$scan"
quiet "were not looked in"
quiet "could not"
printf 'nothing to see\n' >"file-$nfd.txt"
expect_pass "a tracked file with a decomposed name that a decomposed rule matches" "$scan"
says "secrets scan: clean"
# The same for a folder.
printf '%s\n' "dir-$nfd/" >.gitignore
expect_pass "a tracked folder with a decomposed name that a decomposed rule matches" "$scan"
says "secrets scan: clean"
printf 'token = %s\n' "$key" >"dir-$nfd/in-$nfd.txt"
expect_fail "a key in a tracked folder with a decomposed name that a decomposed rule matches" ".txt:1" "$scan"
quiet "were not looked in"
printf 'nothing to see\n' >"dir-$nfd/in-$nfd.txt"
rm .gitignore
if [ "$(uname -s)" = Darwin ] && [ "$(git config --type=bool core.precomposeunicode || true)" = true ]; then
  # One name in the index in both forms: one file there, covered once.
  git -c core.precomposeunicode=false update-index --add --cacheinfo "100644,$(git rev-parse "HEAD:file-$nfd.txt"),file-$nfc.txt"
  expect_pass "one name in the index, composed and decomposed" "$scan"
  says "secrets scan: clean"
  git -c core.precomposeunicode=false update-index --force-remove "file-$nfc.txt"
  # Still refused there, and rightly (docs/carry-over.md): a file tracked under a
  # decomposed name that a rule written composed ignores. Git's search leaves it out.
  printf '%s\n' "file-$nfc.txt" >.gitignore
  expect_fail "a tracked decomposed name that a composed rule ignores" "files to cover were not looked in" "$scan"
  rm .gitignore
else
  echo "skip: two cases of names the index holds decomposed; git does not compose names here"
fi

# --- What the walk and the comparison refuse ------------------------------------------
fresh the-walk-again
mkdir -p kept/inner
printf 'nothing to see\n' >kept/inner/plain.txt
commit_file "docs: plain" plain.txt "nothing to see"
# A folder that find gives as inside itself, or by way of `..`.
expect_fail "find giving a folder's own folder as one inside it" "find gave 1 names that are not paths inside the folders it was given" \
  finding dots FIND_FOLDER=kept "$scan"
never_clean "find giving a folder's own folder as one inside it"
# Answers in another order than the questions.
expect_fail "git check-ignore answering in another order" "are not for the paths it was asked about, in the order asked" \
  env PATH="$tmp/bin:$PATH" SWAP=check-ignore SWAP_FILE="$tmp/swap" "$scan"
never_clean "git check-ignore answering in another order"
# An answer that cannot be read is not taken for "ignored", nor for "not ignored".
expect_fail "git check-ignore answering in words of its own" "the answers of git check-ignore could not be read" \
  env PATH="$tmp/bin:$PATH" GARBLE=check-ignore GARBLE_FILE="$tmp/garble" "$scan"
never_clean "git check-ignore answering in words of its own"
# xargs, which hands the paths to git, failing without a word.
real_xargs=$(command -v xargs)
mkdir "$tmp/xargs-bin"
printf '#!/bin/sh\ncase "$*" in *check-ignore*) exit 1 ;; esac\nexec "%s" "$@"\n' "$real_xargs" >"$tmp/xargs-bin/xargs"
chmod +x "$tmp/xargs-bin/xargs"
expect_fail "xargs failing before git is asked" "could not be told from the ignored ones (xargs: exit 1)" \
  env PATH="$tmp/xargs-bin:$PATH" "$scan"
never_clean "xargs failing before git is asked"
# A file that is gone between the walk and the look at it (find's fourth call lists the
# files), and a name from the search that leads nowhere.
printf 'nothing to see\n' >kept/soon-gone.txt
expect_fail "a file that vanishes after it was listed" "1 files that were in the working tree when it was listed are no longer there" \
  finding vanish FIND_FOLDER=kept/soon-gone.txt FIND_AFTER=4 "$scan"
says "run it again"
never_clean "a file that vanishes after it was listed"
expect_fail "a search that names a file that is not there" "1 names the search gave for the working tree lead to no file" \
  env PATH="$tmp/bin:$PATH" GHOST=no-such-file.txt "$scan"
never_clean "a search that names a file that is not there"
# A walk that comes back to a folder is going in a circle, and is stopped.
expect_fail "a walk that goes in a circle" "came to a folder a second time" finding circle perl -e 'alarm 60; exec @ARGV' "$scan"
never_clean "a walk that goes in a circle"
# A walk that never comes to an end is stopped. It takes three minutes to get there, so
# it runs only when asked for by name.
if [ "${WHIZBANG_SCAN_DEPTH_TEST:-}" = 1 ]; then
  expect_fail "a walk that goes down for ever" "more than 4096 levels deep" finding deeper perl -e 'alarm 900; exec @ARGV' "$scan"
  never_clean "a walk that goes down for ever"
else
  echo "skip: a walk that goes down for ever; set WHIZBANG_SCAN_DEPTH_TEST=1 (three minutes)"
fi

finished
