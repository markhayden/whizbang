#!/usr/bin/env bash
# The clean-room scan: what it covers, that it prints counts only, that it fails closed.
source "$(dirname "$0")/common.sh"

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
# A word list whose lines end in spaces or carriage returns still counts.
printf '%s  \n' "$word" >"$tmp/spaced"
printf '%s\r\nflimwozzle\r\n' "$word" >"$tmp/returns"
expect_fail "a word list with trailing spaces" "clean-room scan: 1 matching lines" env WHIZBANG_WORDLIST="$tmp/spaced" "$scan"
expect_fail "a word list with carriage returns" "clean-room scan: 1 matching lines" env WHIZBANG_WORDLIST="$tmp/returns" "$scan"
rm planted.txt

# The clean-room scan fails closed.
: >"$tmp/empty"
printf '\n  \n' >"$tmp/blank"
mkdir "$tmp/folder"
expect_fail "an empty word list" "unreadable or empty" env WHIZBANG_WORDLIST="$tmp/empty" "$scan"
expect_fail "a blank word list" "unreadable or empty" env WHIZBANG_WORDLIST="$tmp/blank" "$scan"
expect_fail "a missing word list" "unreadable or empty" env WHIZBANG_WORDLIST="$tmp/none" "$scan"
expect_fail "a folder for a word list" "unreadable or empty" env WHIZBANG_WORDLIST="$tmp/folder" "$scan"
expect_fail "no word list variable" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST "$scan"
expect_fail "no word list variable, on a range" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST "$scan" --range "$first" HEAD
fresh no-list
commit_file "feat: clean" clean.txt "clean"
expect_fail "no word list variable, on a push" "WHIZBANG_WORDLIST is not set" env -u WHIZBANG_WORDLIST git push origin no-list
not_on_remote refs/heads/no-list
