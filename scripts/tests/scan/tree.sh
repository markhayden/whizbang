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
