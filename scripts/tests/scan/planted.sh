#!/usr/bin/env bash
# Keys in the places a scan might skip: beside an allowed address, in files that look
# binary, after a byte that is not UTF-8, under unusual names, and as the target of a link.
source "$(dirname "$0")/common.sh"

# Planted cases, one commit each on one branch. The secrets scan refuses them on a push, as
# a range and in the working tree, and names each one by where it is, never by its text.
fresh planted
: >"$tmp/in-commits"
: >"$tmp/in-tree"
plant() { # <path:line, as the scan prints it>
  commit_all "feat: planted"
  printf '%s:%s\n' "$(git rev-parse HEAD)" "$1" >>"$tmp/in-commits"
  printf '%s\n' "$1" >>"$tmp/in-tree"
}
plant_link() {
  commit_all "feat: planted"
  printf 'link-targets/%s:1\n' "$(git rev-parse HEAD)" >>"$tmp/in-commits"
}
# The allow-list is for an address, and only the address: not the line, not the path.
printf 'token = %s # ask someone@example.com\n' "$key" >al.txt
plant "al.txt:1"
mkdir -p "docs/noreply@x"
printf 'token = %s\n' "$key" >"docs/noreply@x/k.txt"
plant "docs/noreply@x/k.txt:1"
printf 'token = %s\n' "$key" >"x@example.txt"
plant "x@example.txt:1"
printf 'someone@example.com and %s\n' "$address" >two.txt
plant "two.txt:1"
# Nothing is skipped for looking binary: a NUL byte, UTF-16, or what .gitattributes says.
printf 'token = %s\n\0\n' "$key" >n.txt
plant "n.txt:1"
printf 'token = %s\n' "$key" | iconv -f UTF-8 -t UTF-16 >u.txt
plant "u.txt:1"
printf '*.cfg -diff\n*.dat binary\n' >.gitattributes
printf 'token = %s\n' "$key" >x.cfg
plant "x.cfg:1"
printf 'token = %s\n' "$key" >x.dat
plant "x.dat:1"
# A byte that is not UTF-8 does not hide the rest of its line.
printf 'caf\351 token = %s\n' "$key" >l.txt
plant "l.txt:1"
# Unusual file names are searched like any other.
letter=$(printf 'caf\303\251.txt')
printf 'token = %s\n' "$key" >"$letter"
plant "$letter:1"
printf 'token = %s\n' "$key" >'has"quote.txt'
plant 'has"quote.txt:1'
# A newline in a name: the hit is reported under the last line of the name.
printf 'token = %s\n' "$key" >"$(printf 'two\nlines.txt')"
commit_all "feat: planted"
echo "lines.txt:1" >>"$tmp/in-commits"
echo "lines.txt:1" >>"$tmp/in-tree"
for f in "with space.txt" "a:b.txt" "*.txt" "-n"; do
  printf 'token = %s\n' "$key" >"./$f"
  plant "$f:1"
done
# A link is never followed; the text of its target is searched.
ln -s "$home_path" link-a
plant_link
ln -s "$key" link-b
plant_link

export LC_ALL=en_US.UTF-8
expect_fail "the planted cases, pushed" "secrets scan: found" git push origin HEAD
while IFS= read -r where; do says "$where"; done <"$tmp/in-commits"
quiet "$key"
not_on_remote refs/heads/planted
expect_fail "the planted cases, as CI runs a range" "secrets scan: found" \
  env -u WHIZBANG_WORDLIST "$scan" --secrets-only --range "$first" HEAD
while IFS= read -r where; do says "$where"; done <"$tmp/in-commits"
quiet "$key"
expect_fail "the planted cases, in the working tree" "secrets scan: found" "$scan"
while IFS= read -r where; do tree_says "$where"; done <"$tmp/in-tree"
says "link-targets/working-tree:1"
says "link-targets/working-tree:2"
quiet "$key"
# A key in a message, after a byte that is not UTF-8.
fresh latin-message
echo x >lm.txt
git add lm.txt
printf 'feat: caf\351 %s\n' "$key" >"$tmp/message"
git commit -q --no-verify -F "$tmp/message" 2>/dev/null
expect_fail "a key after a Latin-1 byte in a message" "commit-message/" git push origin HEAD
quiet "$key"
unset LC_ALL

finished
