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
printf '#!/bin/sh\ncase " $* " in *" ls-files -z -co --exclude-standard "*) exit 3 ;; esac\nexec "%s" "$@"\n' "$real_git" >"$tmp/hush-bin/git"
chmod +x "$tmp/hush-bin/git"
expect_fail "git failing without a word (ls-files -z -co --exclude-standard)" "could not be listed (git ls-files failed)" \
  env PATH="$tmp/hush-bin:$PATH" "$scan"
never_clean "git failing without a word (ls-files -z -co --exclude-standard)"
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

# --- A file that is there and cannot be looked up ---------------------------------------
# A tracked file in an ignored folder that cannot be searched, or read at all: git says
# nothing of it, and the file is not gone. The scan fails, with a count and no path.
fresh barred
printf '%s\n' 'd/' >.gitignore
mkdir d
printf 'token = %s\n' "$key" >d/k.txt
git add -f d/k.txt .gitignore
git commit -q --no-verify -m "feat: a tracked file in an ignored folder"
expect_fail "a key in a tracked file in an ignored folder" "d/k.txt:1" "$scan"
for mode in 400 000; do
  chmod "$mode" d
  if [ -e d/k.txt ]; then
    echo "skip: a tracked file in a folder with mode $mode; this user can look it up all the same"
  else
    expect_fail "a tracked file in an ignored folder with mode $mode" \
      "1 files of the working tree could not be looked up, though they may be there" "$scan"
    says "refusing to pass"
    quiet "k.txt"
    never_clean "a tracked file in an ignored folder with mode $mode"
  fi
  chmod 755 d
done
# A tracked file that is gone, with its folder, is a file deleted here: nothing to cover.
rm -r d
expect_pass "a tracked file that is deleted, with its folder" "$scan"
says "secrets scan: clean (2 files)"
# ... and so is one whose folder is now a file ("not a folder" on the way).
printf 'nothing to see\n' >d
expect_pass "a tracked file whose folder is now a file" "$scan"
rm d

# --- perl ------------------------------------------------------------------------------
# The scan looks files up with perl, in three places: what kind of thing each listed name
# is, which tracked files to search by name, and which files the list and the search
# hold. A perl that fails, says nothing or stops early fails the scan in words.
fresh perl
commit_file "docs: plain" plain.txt "nothing to see"
real_perl=$(command -v perl)
mkdir "$tmp/perl-bin"
perl_that() { # <what the scan asks of it> <what it does instead>
  printf '#!/bin/sh\ncase " $* " in *" %s "*) %s ;; esac\nexec "%s" "$@"\n' "$1" "$2" "$real_perl" >"$tmp/perl-bin/perl"
  chmod +x "$tmp/perl-bin/perl"
}
for what in kinds by-name ids; do
  perl_that "$what" "exit 3"
  expect_fail "a perl that fails ($what)" "could not be looked up (perl: exit 3)" env PATH="$tmp/perl-bin:$PATH" "$scan"
  says "refusing to pass"
  never_clean "a perl that fails ($what)"
  perl_that "$what" "exit 0"
  expect_fail "a perl that says nothing ($what)" "perl did not finish looking the files of the working tree up" \
    env PATH="$tmp/perl-bin:$PATH" "$scan"
  never_clean "a perl that says nothing ($what)"
done
# One that reports all it should and writes a line too few.
perl_that ids "\"$real_perl\" \"\$@\" | sed 1d; exit 0"
expect_fail "a perl that leaves out a line" "perl did not finish looking the files of the working tree up (it reports 2 lines written, and 1 are there)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that leaves out a line"
# One whose report has no last line, as when it was cut short; and one that reports
# fewer names read than it was given.
perl_that ids "\"$real_perl\" \"\$@\" 2>\"$tmp/report\"; grep -v '^end ' \"$tmp/report\" >&2; exit 0"
expect_fail "a perl whose report has no end" "perl did not finish looking the files of the working tree up" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl whose report has no end"
perl_that ids "\"$real_perl\" \"\$@\" 2>\"$tmp/report\"; sed 's/^read .*/read 1/' \"$tmp/report\" >&2; exit 0"
expect_fail "a perl that read fewer names than it was given" "perl did not finish looking the files of the working tree up (it reports 1 of 2 names)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that read fewer names than it was given"
# ... and one that leaves out a name of a file to search by name.
printf '%s\n' '*.log' >.gitignore
printf 'token = %s\n' "$key" >kept.log
git add -f kept.log
perl_that by-name "\"$real_perl\" \"\$@\" >/dev/null; exit 0"
expect_fail "a perl that leaves out a name" "perl did not finish looking the files of the working tree up (it reports 1 names written, and 0 are there)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that leaves out a name"

# --- perl: each list on its own ---------------------------------------------------------
# perl_on <what> <the end of the last argument> <what it does instead>: as perl_that, for
# one of the calls that ask the same thing. In what it does, $last is the call's last
# argument and $before the one before it.
perl_on() {
  cat >"$tmp/perl-bin/perl" <<SH
#!/bin/sh
before= last=
for arg in "\$@"; do before=\$last last=\$arg; done
case " \$* " in *" $1 "*) case "\$last" in *$2) $3 ;; esac ;; esac
exec "$real_perl" "\$@"
SH
  chmod +x "$tmp/perl-bin/perl"
}
fresh perl-lists
commit_file "docs: plain" plain.txt "nothing to see"
for n in one two three; do printf 'nothing to see\n' >"untracked-$n.txt"; done
expect_pass "two tracked files and three untracked" "$scan"
says "secrets scan: clean (5 files)"
# The files to cover are looked up in two lists, the tracked and the untracked, and each
# is held to perl's report: a line too few in one alone is refused, at that list.
perl_on ids /census-entries "\"$real_perl\" \"\$@\" | sed 1d; exit 0"
expect_fail "a perl that leaves out a tracked file" "perl did not finish looking the files of the working tree up (it reports 2 lines written, and 1 are there)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that leaves out a tracked file"
perl_on ids /entries-others "\"$real_perl\" \"\$@\" | sed 1d; exit 0"
expect_fail "a perl that leaves out an untracked file" "perl did not finish looking the files of the working tree up (it reports 3 lines written, and 2 are there)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that leaves out an untracked file"
# ... and so is one whose output cannot be finished: here it has nowhere to write, and
# what it printed is lost when it closes its output.
perl_on ids /census-entries "exec \"$real_perl\" \"\$@\" >&-"
expect_fail "a perl that cannot finish its output" "could not be looked up (perl: exit" env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl that cannot finish its output"

# The two files that `kinds` writes, the paths and where each link leads, are held to
# the report as well: by their bytes, since a name can hold a newline.
ln -s "it-says-$word" a-link
expect_fail "a link whose target holds a listed word" "clean-room scan: 1 matching lines (contents 1, paths 0)" "$scan"
perl_on kinds /link-targets/working-tree "\"$real_perl\" \"\$@\"; : >\"\$last\"; exit 0"
expect_fail "a perl whose list of link targets is emptied after its report" \
  "perl did not finish looking the files of the working tree up (it reports $((${#word} + 9)) bytes of link targets written, and 0 are there)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
quiet "clean-room scan:"
never_clean "a perl whose list of link targets is emptied after its report"
perl_on kinds /link-targets/working-tree "\"$real_perl\" \"\$@\"; rm -f \"\$last\"; exit 0"
expect_fail "a perl whose list of link targets is taken away after its report" "bytes of link targets written, and 0 are there)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
never_clean "a perl whose list of link targets is taken away after its report"
# More than the report says is refused as well, in either file: the count is exact.
perl_on kinds /link-targets/working-tree "\"$real_perl\" \"\$@\"; echo extra >>\"\$last\"; exit 0"
expect_fail "a perl whose list of link targets gains a line after its report" \
  "perl did not finish looking the files of the working tree up (it reports $((${#word} + 9)) bytes of link targets written, and $((${#word} + 15)) are there)" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
quiet "clean-room scan:"
never_clean "a perl whose list of link targets gains a line after its report"
perl_on kinds /link-targets/working-tree "\"$real_perl\" \"\$@\"; echo extra >>\"\$before\"; exit 0"
expect_fail "a perl whose list of paths gains a line after its report" "bytes of paths written, and" env PATH="$tmp/perl-bin:$PATH" "$scan"
says "perl did not finish looking the files of the working tree up"
quiet "clean-room scan:"
never_clean "a perl whose list of paths gains a line after its report"
rm a-link
printf 'nothing to see\n' >"named-$word.txt"
expect_fail "a file whose name holds a listed word" "clean-room scan: 1 matching lines (contents 0, paths 1)" "$scan"
perl_on kinds /link-targets/working-tree "\"$real_perl\" \"\$@\"; grep -v -e $word \"\$before\" >\"\$before.less\"; cat \"\$before.less\" >\"\$before\"; exit 0"
expect_fail "a perl whose list of paths loses a line after its report" "bytes of paths written, and" env PATH="$tmp/perl-bin:$PATH" "$scan"
says "perl did not finish looking the files of the working tree up"
quiet "clean-room scan:"
never_clean "a perl whose list of paths loses a line after its report"
rm "named-$word.txt"

# A link that is there and cannot be read is out of reach, as a file in a folder that
# cannot be searched is: counted, and the scan fails. (No file system here makes one, so
# the stand-in's readlink fails.)
ln -s plain.txt a-link
expect_pass "a link" "$scan"
cat >"$tmp/perl-bin/perl" <<SH
#!/bin/sh
exec "$real_perl" -e 'BEGIN { *CORE::GLOBAL::readlink = sub { \$! = 13; undef } }' "\$@"
SH
expect_fail "a link that cannot be read" "1 files of the working tree could not be looked up, though they may be there" \
  env PATH="$tmp/perl-bin:$PATH" "$scan"
says "refusing to pass"
quiet "a-link"
never_clean "a link that cannot be read"
rm a-link

# Two links are told apart by where they lead as well as by device and inode: a file
# system that gave two different links one inode number hides neither target from the
# search. (No file system here does, so the stand-in gives every name the same inode
# when the scan asks what each listed name is.)
ln -s "it-says-$word" a-link
ln -s "it-also-says-$word" b-link
expect_fail "two links whose targets hold a listed word" "clean-room scan: 2 matching lines (contents 2, paths 0)" "$scan"
cat >"$tmp/perl-bin/perl" <<SH
#!/bin/sh
case " \$* " in *" kinds "*)
  exec "$real_perl" -e 'BEGIN { *CORE::GLOBAL::lstat = sub { my @s = CORE::lstat(\$_[0]); \$s[1] = 7 if @s; @s } }' "\$@" ;;
esac
exec "$real_perl" "\$@"
SH
expect_fail "two links with one inode number, whose targets hold a listed word" \
  "clean-room scan: 2 matching lines (contents 2, paths 0)" env PATH="$tmp/perl-bin:$PATH" "$scan"
rm a-link b-link

finished
