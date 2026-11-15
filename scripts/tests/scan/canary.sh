#!/usr/bin/env bash
# The search the scan relies on, run over and over against its known output.
#
#   scripts/tests/scan/canary.sh                   for 2 seconds (a part of the scan tests)
#   WHIZBANG_CANARY_SECONDS=30 scripts/tests/scan/canary.sh
#                                                  `just search-canary`; CI, on macOS
#
# On CI's macOS runner git grep lost a match at random when it ran on several threads (the
# comment above `search` in scripts/scan.sh); on one thread it never did. The scan now
# searches on one thread. This loop is there to say so loudly if that stops being enough:
# it plants a key in files of every awkward kind, runs the scan's own search of them, on
# one thread and in the C locale, and fails if a single run differs from what the files
# are known to hold. The known output is written down here, not taken from a first run.
source "$(dirname "$0")/../lib.sh"

seconds=${WHIZBANG_CANARY_SECONDS:-2}
# CI runs the long form, on the runner where the fault was seen.
grep -q -E '^ +run: just search-canary$' .github/workflows/ci.yml ||
  fail ".github/workflows/ci.yml must have a step that runs: just search-canary"
grep -q -x -F "    WHIZBANG_CANARY_SECONDS=30 bash scripts/tests/scan/canary.sh" justfile ||
  fail "the justfile must run the search canary for 30 seconds"
case "$seconds" in '' | *[!0-9]*) fail "WHIZBANG_CANARY_SECONDS must be a whole number of seconds" ;; esac
grep -B1 -E '^ +run: just search-canary$' .github/workflows/ci.yml | grep -F "if: matrix.os == 'macos-15'" >/dev/null ||
  fail ".github/workflows/ci.yml must run the search canary on macos-15"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

key="sk""-abcdefghij0123456789ABCDEFGHIJ"
sk_shape='(^|[^A-Za-z0-9_-])sk-[A-Za-z0-9_-]{20,}'
grep -q -F -e "sk_shape='$sk_shape'" scripts/scan.sh || fail "the canary searches for the shape scripts/scan.sh searches for"

# The files that hold the key, in the order git lists them (by byte).
accent=$(printf 'caf\303\251.txt')
holders=('*.txt' '-n' 'a:b.txt' 'al.txt' "$accent" 'docs/noreply@x/k.txt' 'has"quote.txt' 'l.txt'
  'n.txt' 'with space.txt' 'x.cfg' 'x.dat' 'x@example.txt')
repo="$tmp/repo"
git init -q -b main "$repo"
cd "$repo"
mkdir -p "docs/noreply@x"
for f in "${holders[@]}"; do printf 'token = %s\n' "$key" >"./$f"; done
printf 'token = %s # ask someone@example.com\n' "$key" >al.txt
printf 'caf\351 token = %s\n' "$key" >l.txt
printf 'token = %s\n\0\n' "$key" >n.txt
printf '*.cfg -diff\n*.dat binary\n' >.gitattributes
# And files that hold none: plain text, UTF-16 (the key is there, but not as these bytes).
echo "hello" >readme.txt
printf 'token = %s\n' "$key" | iconv -f UTF-8 -t UTF-16 >u.txt
: >"$tmp/known"
for f in "${holders[@]}"; do printf '%s\0001\000 %s\n' "$f" "$key" >>"$tmp/known"; done

runs=0
wrong=0
started=$SECONDS
deadline=$((SECONDS + seconds))
while [ "$runs" -lt 20 ] || [ "$SECONDS" -lt "$deadline" ]; do
  runs=$((runs + 1))
  status=0
  LC_ALL=C git -c grep.threads=1 grep --untracked -a -n -z -o -E -e "$sk_shape" >"$tmp/raw" 2>"$tmp/errors" || status=$?
  if [ "$status" -ne 0 ] || [ -s "$tmp/errors" ] || ! cmp -s "$tmp/raw" "$tmp/known"; then
    wrong=$((wrong + 1))
  fi
done
cd "$root"
[ "$wrong" -eq 0 ] ||
  fail "the search canary: $wrong of $runs searches differed from the known output ($(git --version), $(uname -sr)); the scan cannot be trusted here: see the comment above \`search\` in scripts/scan.sh"
echo "search canary: $runs searches in $((SECONDS - started)) seconds (asked for $seconds), all as known ($(git --version))"

finished
