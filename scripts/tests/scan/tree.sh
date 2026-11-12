#!/usr/bin/env bash
# The working tree: keys, home paths, addresses, the key shapes and their lookalikes.
source "$(dirname "$0")/common.sh"

# The working tree.
expect_pass "a clean working tree" "$scan"
says "clean-room scan: 0 matching lines"
printf 'token = %s\n' "$key" >planted.txt
expect_fail "a key in the working tree" "planted.txt:1" "$scan"
quiet "$key"
expect_fail "a key in the working tree, secrets only" "planted.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
rm planted.txt
printf 'see %s\n' "$home_path" >planted.txt
expect_fail "a home path in the working tree" "planted.txt:1" "$scan"
printf 'write to %s\n' "$address" >planted.txt
expect_fail "an email address in the working tree" "planted.txt:1" "$scan"
printf 'write to someone@example.com or git@github.com:o/r\n' >planted.txt
expect_pass "the allowed addresses" "$scan"
rm planted.txt

# A file the scan cannot read is not covered, so the scan fails; it does not report clean.
printf 'token = %s\n' "$key" >locked.txt
chmod 000 locked.txt
if [ -r locked.txt ]; then
  # Whoever runs this (root, for one) can read a file with no permissions: nothing to test.
  echo "skip: an unreadable file; this user can read a file with mode 000"
else
  expect_fail "a file that cannot be read" "git said something during a search (1 lines" "$scan"
  says "refusing to pass"
  quiet "locked.txt"
  grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "an unreadable file is not reported clean"
  expect_fail "a file that cannot be read, secrets only" "refusing to pass" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
fi
chmod 644 locked.txt
rm locked.txt

# What tree_says reports when a place is missing from a scan of the working tree.
printf 'token = %s\n' "$key" >planted.txt
expect_fail "a key, for the report" "planted.txt:1" "$scan"
tree_says "planted.txt:1"
if (tree_says "nowhere.txt:1") >"$tmp/explained" 2>&1; then fail "a place that is not there fails"; fi
for text in "missing from the working-tree scan: nowhere.txt:1" "git version" "./planted.txt" \
  "a second scan does not report it either" "a scan on one thread does not report it either" \
  "failed: expected the output to say: nowhere.txt:1"; do
  grep -a -q -F -e "$text" "$tmp/explained" || { cat "$tmp/explained" >&2; fail "the report says: $text"; }
done
# The output of a scan that dropped the place, as has happened: the next scans report it.
grep -a -v -F -e "planted.txt:1" "$tmp/out" >"$tmp/dropped" || true
cp "$tmp/dropped" "$tmp/out"
if (tree_says "planted.txt:1") >"$tmp/explained" 2>&1; then fail "a dropped place fails"; fi
for text in "a second scan reports it" "a scan on one thread reports it"; do
  grep -a -q -F -e "$text" "$tmp/explained" || { cat "$tmp/explained" >&2; fail "the report says: $text"; }
done
grep -a -q -F -e "$key" "$tmp/explained" && fail "the report never prints a key"
rm planted.txt

# Key shapes, one to a line; and shapes that only look like keys.
{
  echo "AK""IA""ABCDEFGHIJKLMNOP"
  echo "gh""p_${chars:0:24}"
  echo "github""_pat_${chars:0:24}"
  echo "gl""pat-${chars:0:24}"
  echo "AI""za$chars$chars"
  echo "xo""xb-${chars:0:12}"
  echo "xa""i-${chars:0:24}"
  echo "h""f_$chars$chars"
  echo "r""8_$chars$chars"
  echo "sk""_live_${chars:0:16}"
  echo "01234567-89ab-cdef-0123-456789abcdef"":""$hex"
  echo "-----BEGIN ""RSA PRIVATE KEY-----"
  echo "see /ho""me/someone"
  echo "sk""-ant-abcdefghijabcdefghij"
  echo "x=sk""-abcdefghijklmnopqrst9"
} >shapes.txt
shapes=$(wc -l <shapes.txt | tr -d ' ')
expect_fail "every key shape" "shapes.txt:1" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
i=1
while [ "$i" -le "$shapes" ]; do
  says "shapes.txt:$i"
  i=$((i + 1))
done
quiet "$chars"
rm shapes.txt
{
  echo "disk-usage-report-generator-v2"
  echo "sk-learn-pipeline-builder-tools"
  echo "task-force-alpha-bravo-charlie-delta-echo"
  echo "a uuid alone: 01234567-89ab-cdef-0123-456789abcdef"
} >lookalikes.txt
expect_pass "kebab-case and other lookalikes" env -u WHIZBANG_WORDLIST "$scan" --secrets-only
rm lookalikes.txt

finished
