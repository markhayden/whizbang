#!/usr/bin/env bash
# A key written directly after a hyphen, an underscore, a letter or a digit: nothing before
# it marks its start, and it is found all the same, in a file and in the target of a link.
# A written-out line break or tab in a JSON string ends in a letter, and percent-encoding
# in a digit or a letter, so a key after either is one of these. Ordinary hyphenated words
# that hold the key's first letters are still not keys.
source "$(dirname "$0")/common.sh"

# What stands directly before the key, one case to a line of the file and to a link.
names=(hyphen underscore letter digit line-break tab percent-20 percent-3D after-a-word-in-sk)
before=('token-' 'token_' 'token' 'token9' '{"log":"one\n' '{"row":"one\t' 'q=a%20' 'key%3D' 'task-')
after=('' '' '' '' '"}' '"}' '' '' '')

fresh joined
: >joined.txt
i=0
while [ "$i" -lt "${#names[@]}" ]; do
  printf '%s%s%s\n' "${before[$i]}" "$key" "${after[$i]}" >>joined.txt
  i=$((i + 1))
done
count=${#names[@]}
[ "$(wc -l <joined.txt | tr -d ' ')" = "$count" ] || fail "one line for each case"
# The cases are what they say: a backslash and a letter, not a line break or a tab.
grep -q -F -e 'one\n'"$key" joined.txt || fail "the line break is written out"
grep -q -F -e 'one\t'"$key" joined.txt || fail "the tab is written out"

# In a file, in the working tree.
expect_fail "a key directly after a word character, in the working tree" "secrets scan: found" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only
i=0
while [ "$i" -lt "$count" ]; do
  i=$((i + 1))
  grep -a -q -x -F -e "joined.txt:$i" "$tmp/out" || { cat "$tmp/out" >&2; fail "a key after ${names[$((i - 1))]} is found (joined.txt:$i)"; }
done
quiet "$chars"
# With the clean-room scan too, as `just scan` runs it.
expect_fail "the same, with both scans" "joined.txt:1" "$scan"
says "joined.txt:$count"
quiet "$chars"

# In a file, in a commit: as a range, as CI runs it, and pushed.
commit_all "feat: joined"
sha=$(git rev-parse HEAD)
expect_fail "a key directly after a word character, as CI runs a range" "secrets scan: found" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
i=0
while [ "$i" -lt "$count" ]; do
  i=$((i + 1))
  says "$sha:joined.txt:$i"
done
quiet "$chars"
expect_fail "a key directly after a word character, pushed" "$sha:joined.txt:1" git push origin HEAD
not_on_remote refs/heads/joined
quiet "$chars"
# Deleted in the next commit, it is still in the first.
git rm -q joined.txt
git commit -q --no-verify -m "fix: drop it"
expect_fail "a key directly after a word character, added and then deleted" "$sha:joined.txt:3" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD

# In the target of a link: one link for each case, alone in the working tree, so that the
# one line of targets is that case's.
i=0
while [ "$i" -lt "$count" ]; do
  fresh "joined-link-$i"
  ln -s "${before[$i]}$key${after[$i]}" a-link
  expect_fail "a key after ${names[$i]} in the target of a link" "link-targets/working-tree:1" \
    env -u WHIZBANG_WORDLIST "$scan" --secrets-only
  quiet "$chars"
  commit_all "feat: a link"
  expect_fail "a key after ${names[$i]} in the target of a link, as a range" "link-targets/$(git rev-parse HEAD):1" \
    env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
  quiet "$chars"
  i=$((i + 1))
done

# In a path and in a commit message, which are searched as text too; and the path is not
# printed.
fresh joined-path
commit_file "feat: a path" "notes-token_$key.txt" "nothing here"
expect_fail "a key directly after a word character in a path" "commit-paths/" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
quiet "$chars"
expect_fail "the same, in the working tree" "tree-paths/working-tree:" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
quiet "$chars"
fresh joined-path-and-file
commit_file "feat: a path with a file" "notes-token_$key.txt" "write to $address"
expect_fail "an address in a file named with such a key" "<path withheld>:1" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
quiet "$chars"
fresh joined-message
echo x >m.txt
git add m.txt
git commit -q --no-verify -m "feat: see token_$key"
expect_fail "a key directly after a word character in a message" "commit-message/" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
quiet "$chars"

# A file that looks binary is searched with its NUL bytes removed: UTF-16.
fresh joined-wide
printf 'token_%s\n' "$key" | iconv -f UTF-8 -t UTF-16 >wide.txt
expect_fail "a key directly after a word character, in UTF-16" "wide.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only

# Keys of the family with a known beginning, and one whose body is cut by hyphens but
# holds a long run: each directly after a word character.
fresh joined-family
{
  echo "token_sk""-proj-$chars"
  echo "token-sk""-ant-api03-$chars"
  echo "token9sk""-or-v1-$hex$hex"
  echo "tokensk""-svcacct-ab_cd-$chars"
  echo "token_sk""-${hex:0:12}-$chars-${hex:0:12}"
} >family.txt
family=$(wc -l <family.txt | tr -d ' ')
expect_fail "keys with a known beginning, directly after a word character" "family.txt:1" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only
i=0
while [ "$i" -lt "$family" ]; do
  i=$((i + 1))
  grep -a -q -x -F -e "family.txt:$i" "$tmp/out" || { cat "$tmp/out" >&2; fail "family.txt:$i is found"; }
done
quiet "$chars"

# What starts a word is found as it always was, whatever its body: these are the forms the
# scan found before it looked inside a word, and none is lost.
fresh joined-as-before
{
  echo "sk""-ant-abcdefghijabcdefghij"
  echo "x=sk""-abcdefghijklmnopqrst9"
  echo "see sk""-or-abcdefghij-abcdefghij"
  echo "(sk""-abc-def-ghi-jkl-mno-pqr-9)"
  echo "sk""-proj-ab-cd-ef-gh-ij-kl-mn"
} >as-before.txt
as_before=$(wc -l <as-before.txt | tr -d ' ')
expect_fail "what starts a word, as before" "as-before.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
i=0
while [ "$i" -lt "$as_before" ]; do
  i=$((i + 1))
  grep -a -q -x -F -e "as-before.txt:$i" "$tmp/out" || { cat "$tmp/out" >&2; fail "as-before.txt:$i is found"; }
done

# Ordinary hyphenated words that hold the key's first letters: a word that ends in s and
# k, joined by hyphens to twenty letters or more. None is a key: not all in small letters,
# not with a digit or a capital in a short part, not with one of the known beginnings
# after it, not in a path, a link or a message.
fresh joined-words
{
  echo "risk-assessmentmethodologies"
  echo "task-force-alpha-bravo-charlie-delta-echo"
  echo "disk-usage-report-generator-v2"
  echo "mask-rcnn-resnet50-fpn-coco-v2"
  echo "a-task-or-project-with-a-long-name"
  echo "the desk-admin-console-for-everyone page"
  echo "task-proj-planning-and-estimates"
  echo "ask-ant-colonies-about-their-queens"
  echo "kiosk-Mode-Settings-And-Display-Options"
  echo "multi_task-learning_rate_schedule_names"
  echo "see docs/how-to-ask-10-questions-about-images.md"
  echo "flask-2024-11-13-release-notes-draft"
  echo "tusk-abcdefghijklmnopqrstuvwxyz"
  echo "TASK-FORCE-ALPHA-BRAVO-CHARLIE-DELTA"
  echo "brisk-walks_and_other_things_to_do_outside"
} >words.txt
mkdir -p "docs/task-force-alpha-bravo-charlie"
echo "nothing" >"docs/task-force-alpha-bravo-charlie/risk-assessment-methodology-v2.md"
ln -s "docs/task-force-alpha-bravo-charlie/risk-assessment-methodology-v2.md" a-link
expect_pass "ordinary hyphenated words, in the working tree" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
says "secrets scan: clean"
git add -A
git commit -q --no-verify -m "docs: the task-force-alpha-bravo-charlie notes, and disk-usage-report-generator-v2"
expect_pass "ordinary hyphenated words, as a range" env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
says "secrets scan: clean"
expect_pass "ordinary hyphenated words, pushed" git push origin HEAD
# One key among them is found, and only its line.
printf 'token_%s\n' "$key" >>words.txt
expect_fail "a key among ordinary words" "words.txt:16" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
[ "$(grep -a -c -F -e "words.txt:" "$tmp/out")" = 1 ] || { cat "$tmp/out" >&2; fail "only the line with the key is reported"; }

# The edges of the rule, directly after a word character: a part of exactly 20 characters
# with a digit is a key, one of 19 is not, and a capital counts as a digit does.
fresh joined-edges
letters=abcdefghijklmnopqrstuvwxyz
{
  echo "token_sk""-${letters:0:19}7"
  echo "token_sk""-ab-${letters:0:18}7"
  echo "token_sk""-${letters:0:19}Q"
  echo "token_sk""-${letters:0:19}_"
} >edges.txt
expect_fail "the edges of the rule" "edges.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
grep -a -q -x -F -e "edges.txt:3" "$tmp/out" || { cat "$tmp/out" >&2; fail "a part of 20 with a capital and no digit is found"; }
grep -a -q -x -F -e "edges.txt:2" "$tmp/out" && { cat "$tmp/out" >&2; fail "a part of 19 with a digit is not a key"; }
grep -a -q -x -F -e "edges.txt:4" "$tmp/out" && { cat "$tmp/out" >&2; fail "a part of 20 with no digit and no capital is not a key"; }
[ "$(grep -a -c -F -e "edges.txt:" "$tmp/out")" = 2 ] || { cat "$tmp/out" >&2; fail "two of the four edges are keys"; }

# What the rule takes for a key though it is a name: none can be told from a key by its
# shape (the comment above sk_anywhere in scripts/scan.sh says what to do about one).
# They are put together here, like the keys, because this repo's own scan takes them so.
fresh joined-names
{
  echo "task-$hex"
  echo "mask-${chars}${chars:0:6}.png"
  echo "task""-VeryLongClassNameForSomething"
  echo "mask""-rcnn_resnet50_fpn_coco_weights"
} >names.txt
expect_fail "names that have a key's shape" "names.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
[ "$(grep -a -c -F -e "names.txt:" "$tmp/out")" = 4 ] || { cat "$tmp/out" >&2; fail "each of the four names is taken for a key"; }
# Written another way, each passes.
{
  echo "task_$hex"
  echo "mask ${chars}${chars:0:6}.png"
  echo "task-Very-Long-Class-Name-For-Something"
  echo "mask_rcnn_resnet50_fpn_coco_weights"
} >names.txt
expect_pass "the same names written another way" env -u WHIZBANG_WORDLIST "$scan" --secrets-only

# What is left, and is on docs/carry-over.md: directly after a word character, a key whose
# every part between hyphens is short is not told from hyphenated words. The same text at
# the start of a word is found. If this case starts to fail, the row can go.
fresh joined-left
short="abc-def-ghi-jkl-mno-pqr-st9"
printf 'token_%s\n' "sk""-$short" >left.txt
expect_pass "the gap that is left: short parts, directly after a word character" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
printf 'token %s\n' "sk""-$short" >left.txt
expect_fail "the same at the start of a word" "left.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only

finished
