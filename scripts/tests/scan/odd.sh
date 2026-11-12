#!/usr/bin/env bash
# Listed words in the places a scan might skip; links and paths the scan must not read
# or print.
source "$(dirname "$0")/common.sh"

# The same odd places, for the clean-room scan alone: a file with a NUL byte (counted as
# it is and again without the byte), a UTF-16 file, a file marked binary, a link's target.
fresh listed-odd
printf 'it says %s\n\0\n' "$word" >n.txt
printf 'it says %s\n' "$word" | iconv -f UTF-8 -t UTF-16 >u.txt
printf '*.cfg binary\n' >.gitattributes
printf 'it says %s\n' "$word" >x.cfg
ln -s "to-$word" link
commit_all "feat: odd places"
# Five in the commit and five in the tree of the tip.
expect_fail "listed words in odd places, pushed" "clean-room scan: 10 matching lines (contents 10," git push origin HEAD
says "secrets scan: clean"
quiet "$word"
expect_fail "listed words in odd places, as a range" "clean-room scan: 10 matching lines (contents 10," "$scan" --range "$first" HEAD
quiet "$word"
expect_fail "listed words in odd places, in the working tree" "clean-room scan: 5 matching lines (contents 5," "$scan"
says "secrets scan: clean"
quiet "$word"

fresh link-out
printf 'token = %s\n' "$key" >"$tmp/outside.txt"
ln -s "$tmp/outside.txt" link
expect_pass "a link to a file outside the repository is not read" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
rm link
fresh name-gone
commit_file "feat: z" zzz.txt "z"
rm zzz.txt
expect_pass "a tracked file deleted from the working tree" "$scan"
fresh name-repo
git init -q inner
git -C inner -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "docs: nothing"
git add inner 2>/dev/null
expect_fail "a path that is neither a file nor a link" "1 paths that git lists are neither a file nor a link" "$scan"
quiet "inner"
git rm -q -f --cached inner
rm -rf inner

# A path that holds a match is not printed.
fresh withheld-word
commit_file "feat: w" "dir-$word/f.txt" "token = $key"
expect_fail "a key under a directory named with a listed word" "<path withheld>:1" git push origin HEAD
quiet "$word"
quiet "$key"
expect_fail "the same, in the working tree" "<path withheld>:1" "$scan"
quiet "$word"
fresh withheld-key
commit_file "feat: k" "$key.txt" "write to $address"
expect_fail "an address in a file named with a key, as CI runs it" "<path withheld>:1" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
quiet "$key"
expect_fail "the same, in the working tree" "<path withheld>:1" "$scan"
quiet "$key"

finished
