#!/usr/bin/env bash
# Names the scan must not trip over, or be fooled by: a path as long as a path may be,
# and one byte longer, with an accent at its very end; and links and message files whose names start with a
# dash. (Names with accents in both their forms, and names with bytes the scan cannot
# hold, are in walk.sh; names the rules are made of, in folder.sh and random.sh.)
source "$(dirname "$0")/common.sh"
stand_in_for_git
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }

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
  # Three bytes more, 1,024 without the "./": on macOS the decomposed name is now one
  # byte too long to open in one go, and the composed one, which git knows the file by,
  # is not. The file is covered all the same.
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
    (cd "$long" && rm "$name")
  else
    echo "skip: a path one byte too long; it cannot be made here"
  fi
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
