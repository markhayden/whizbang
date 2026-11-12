#!/usr/bin/env bash
# Names the scan must not trip over, or be fooled by: names that read as patterns; names
# with a byte the scan cannot hold; names with an accent, composed and decomposed, tracked
# and not, with ignore rules in either form; links and message files whose names start
# with a dash; and a path as long as a path may be, and longer. (Names the rules are made
# of are in folder.sh and random.sh.)
source "$(dirname "$0")/common.sh"
stand_in_for_git
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }

# --- Names that read like something else ----------------------------------------------------------------------------
# An ignored file whose name, read as a pattern, matches a tracked file is still ignored.
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
# A tracked file that a rule matches is searched by its name, as a name: one that reads as
# a pattern, or as an instruction to git about patterns, is searched all the same, and
# only that file.
for name in ':(glob)x.tmp' '*.tmp' ':!axb.tmp'; do
  printf 'token = %s\n' "$key" >"$name"
  git add -f -- "./$name"
  expect_fail "a key in a tracked file named $name that a rule matches" "$name:1" "$scan"
  quiet "were not looked in"
  [ "$(grep -c -F -e ".tmp:1" "$tmp/out")" = 1 ] || { cat "$tmp/out" >&2; fail "only the file named $name holds a key"; }
  git rm -q -f --cached -- "./$name"
  rm "$name"
done
# A tracked link that a rule matches, to a tracked file that the rule matches too: the
# link is a link, and the file is searched.
printf 'token = %s\n' "$key" >z-real.tmp
ln -s z-real.tmp a-link.tmp
git add -f a-link.tmp z-real.tmp
expect_fail "a key in a tracked file that a tracked link leads to, a rule matching both" "z-real.tmp:1" "$scan"
quiet "were not looked in"
git rm -q -f --cached a-link.tmp z-real.tmp
rm a-link.tmp z-real.tmp

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
  # Tracked in an ignored folder: the name comes from the index.
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
  # (An empty folder holds nothing to cover, whatever its name.)
  expect_pass "an untracked folder with byte $byte in its name, empty" "$scan"
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
# reads from a folder, and the index holds whichever form was committed: one file, two
# names. The scan compares files, not names. Elsewhere the two forms are two files.
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
[ "$(grep -c -F -e ".txt:1" "$tmp/out")" = 1 ] || { cat "$tmp/out" >&2; fail "a match in a file known by two names is printed once"; }
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
[ "$(grep -c -F -e ".txt:1" "$tmp/out")" = 1 ] || { cat "$tmp/out" >&2; fail "a match in a file known by two names is printed once"; }
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
  printf '%s\n' "dir-$nfc/" >.gitignore
  expect_fail "a tracked decomposed folder that a composed rule ignores" "files to cover were not looked in" "$scan"
  rm .gitignore
else
  echo "skip: two cases of names the index holds decomposed; git does not compose names here"
fi

# --- Names that start with a dash -----------------------------------------------------
# A link is read with readlink, which must not take its name for an option.
fresh dash-names
for name in -dash -- -n; do
  ln -s readme.txt "./$name"
  expect_pass "a link named $name" "$scan"
  says "secrets scan: clean (1 files)"
  rm "./$name"
  ln -s "it-says-$word" "./$name"
  expect_fail "a link named $name whose target holds a listed word" "clean-room scan: 1 matching lines (contents 1, paths 0)" "$scan"
  rm "./$name"
done
# A link's own name is a path like any other: a key in it is found, and a listed word in
# it is counted, among the paths.
ln -s readme.txt "$key.link"
expect_fail "a link whose name holds a key" "tree-paths/working-tree:" "$scan"
quiet "$key"
never_clean "a link whose name holds a key"
rm "$key.link"
ln -s readme.txt "link-$word"
expect_fail "a link whose name holds a listed word" "clean-room scan: 1 matching lines (contents 0, paths 1)" "$scan"
quiet "$word"
rm "link-$word"
# On macOS the index can hold one link under two names, composed and decomposed. Its
# target is read once: a listed word in it is one matching line, as a match in a file
# known by two names is printed once.
if [ "$(uname -s)" = Darwin ] && [ "$(git config --type=bool core.precomposeunicode || true)" = true ]; then
  nfc=$(printf 'caf\303\251')
  nfd=$(printf 'cafe\314\201')
  ln -s "it-says-$word" "link-$nfd"
  git -c core.precomposeunicode=false add -- "link-$nfd"
  git -c core.precomposeunicode=false update-index --add --cacheinfo \
    "120000,$(printf '%s' "it-says-$word" | git hash-object -w --stdin),link-$nfc"
  [ "$(git ls-files | wc -l | tr -d ' ')" = 3 ] || fail "the index holds the link under two names"
  expect_fail "a link the index holds under two names, whose target holds a listed word" \
    "clean-room scan: 1 matching lines (contents 1, paths 0)" "$scan"
  # Two links to the same place are two links: each is counted.
  ln -s "it-says-$word" another-link
  expect_fail "two links whose targets hold a listed word" "clean-room scan: 2 matching lines (contents 2, paths 0)" "$scan"
  rm another-link
  git -c core.precomposeunicode=false update-index --force-remove "link-$nfc" "link-$nfd"
  rm "link-$nfd"
else
  echo "skip: a link the index holds under two names; git does not compose names here"
fi
# The same for a message file.
mkdir "$tmp/messages"
printf 'fix: a thing\n' >"$tmp/messages/-n"
(cd "$tmp/messages" && expect_pass "a message file named -n" "$scan" --message -n)
printf 'fix: a thing\n\n%s: Someone <someone@example.com>\n' "$credit" >"$tmp/messages/-n"
(cd "$tmp/messages" && expect_fail "a message file named -n, with a credit line" "$has_credit" "$scan" --message -n)


# --- Long names -----------------------------------------------------------------------
# A path of 1,023 bytes, the longest macOS takes, that ends in a letter and its accent,
# decomposed. Whatever is done with names must be done right at that length: a key in
# the file is found, and without one the scan passes with the file counted.
fresh long-names
part=$(printf 'd%.0s' $(seq 1 200))
long="$part/$part/$part/$part/$part"
mkdir -p "$long"
nfd=$(printf 'e\314\201')
# "./" and the path make 1,023 bytes: the folders and a slash are 1,005 of them.
name="$(printf 'f%.0s' $(seq 1 13))$nfd"
[ "$(printf './%s/%s' "$long" "$name" | LC_ALL=C wc -c | tr -d ' ')" = 1023 ] || fail "the path is 1,023 bytes long"
if (printf 'token = %s\n' "$key" >"$long/$name") 2>/dev/null; then
  expect_fail "a key at the end of the longest path" ":1" "$scan"
  quiet "were not looked in"
  never_clean "a key at the end of the longest path"
  printf 'nothing to see\n' >"$long/$name"
  expect_pass "the longest path" "$scan"
  says "secrets scan: clean (2 files)"
  # Three bytes more, 1,024 in all: on macOS the decomposed name is now one byte too
  # long to open in one go, and the composed one, which git lists the file by, is not.
  # The file is covered all the same.
  short=$name
  name="fff$name"
  [ "$(printf '%s/%s' "$long" "$name" | LC_ALL=C wc -c | tr -d ' ')" = 1024 ] || fail "the path is 1,024 bytes long"
  if (cd "$long" && printf 'token = %s\n' "$key" >"$name") 2>/dev/null; then
    expect_fail "a key at the end of a path one byte too long" ":1" "$scan"
    quiet "no longer there"
    quiet "git said something"
    never_clean "a key at the end of a path one byte too long"
    (cd "$long" && printf 'nothing to see\n' >"$name")
    lists_what_git_lists "a path one byte too long"
    says "secrets scan: clean (3 files)"
    listed "$long/$short" || fail "the list holds the file at the end of the longest path"
    [ "$(wc -l <"$tmp/spy.listed" | tr -d ' ')" = 3 ] || fail "the list holds three files"
    # Where git does not compose names, it lists the file by the name that is too long:
    # the file is there and cannot be looked up, which is not the same as gone.
    if [ "$(uname -s)" = Darwin ]; then
      git config core.precomposeunicode false
      expect_fail "an untracked file whose only name is too long to open" \
        "1 files of the working tree could not be looked up, though they may be there" "$scan"
      quiet "no longer there"
      never_clean "an untracked file whose only name is too long to open"
      git config core.precomposeunicode true
    fi
    (cd "$long" && rm "$name")
  else
    echo "skip: a path one byte too long; it cannot be made here"
  fi
  # A tracked file in an ignored folder, at a path as long as the system's limit and so
  # one byte too long to open: git says nothing of it, and it is not gone.
  limit=$(getconf PATH_MAX . 2>/dev/null || echo 4096)
  piece="$(printf 'd%.0s' $(seq 1 199))/"
  folders="ig/$(printf "$piece%.0s" $(seq 1 $(((limit - 4) / 200))))"
  last=$(printf 'k%.0s' $(seq 1 $((limit - ${#folders}))))
  [ "$(printf '%s%s' "$folders" "$last" | LC_ALL=C wc -c | tr -d ' ')" = "$limit" ] || fail "the path is $limit bytes long"
  mkdir -p "$folders"
  if (cd "$folders" && printf 'token = %s\n' "$key" >"$last") 2>/dev/null; then
    printf 'ig/\n' >>.git/info/exclude
    git update-index --add --cacheinfo "100644,$(printf 'token = %s\n' "$key" | git hash-object -w --stdin),$folders$last"
    expect_fail "a tracked file in an ignored folder at a path too long to open" \
      "1 files of the working tree could not be looked up, though they may be there" "$scan"
    quiet "kkkk"
    never_clean "a tracked file in an ignored folder at a path too long to open"
    git update-index --force-remove "$folders$last"
    printf 'scripts/\n.env\n' >.git/info/exclude
  else
    echo "skip: a tracked file at a path of $limit bytes; it cannot be made here"
  fi
  rm -rf ig
  # The longest path, tracked. (Last: git cannot take such a file away again.)
  git -c core.precomposeunicode=false add -- "$long/$short"
  git commit -q --no-verify -m "feat: a long path"
  printf 'token = %s\n' "$key" >"$long/$short"
  expect_fail "a key at the end of the longest path, tracked" ":1" "$scan"
  never_clean "a key at the end of the longest path, tracked"
else
  echo "skip: the longest path; this file system does not take one of 1,023 bytes"
fi

finished
