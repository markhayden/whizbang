#!/usr/bin/env bash
# The scan does not trust a search on its word. Before the first search of anything it
# counts: every file it should cover must be one the search looks in. And every search runs
# twice: two answers that differ are no answer. Both fail closed. They are tested with a
# stand-in for git that loses one file from a search's output, always or now and then, as
# the search did on CI's macOS runner (the comment above `search` in scripts/scan.sh).
source "$(dirname "$0")/common.sh"

# The stand-in: any git command but a search goes to git as it is. A search's output loses
# every record and every name of the file LOSE names, when LOSE_WHEN says so: `always`, or
# `sometimes` (every other search for a pattern, never the census).
real_git=$(command -v git)
mkdir "$tmp/bin"
cat >"$tmp/bin/git" <<SH
#!/usr/bin/env bash
real="$real_git"
sep=\$(printf '\\001')
SH
cat >>"$tmp/bin/git" <<'SH'
search=0 census=0
for arg in "$@"; do
  case "$arg" in
    grep) search=1 ;;
    -l) census=1 ;;
  esac
done
lose=0
if [ "$search" -eq 1 ] && [ -n "${LOSE:-}" ]; then
  case "${LOSE_WHEN:-}" in
    always) lose=1 ;;
    sometimes)
      if [ "$census" -eq 0 ]; then
        echo x >>"$LOSE_COUNT"
        [ $(($(wc -l <"$LOSE_COUNT") % 2)) -eq 0 ] || lose=1
      fi
      ;;
  esac
fi
[ "$lose" -eq 1 ] || exec "$real" "$@"
status=0
"$real" "$@" >"$LOSE_COUNT.out" || status=$?
# Records are lines; a list of names (-l, the census) is one line of names, each ended by
# the separator.
if [ "$census" -eq 1 ]; then
  edit="s|\([0-9a-f]\{40,64\}:\)\{0,1\}$LOSE$sep||g"
else
  edit="\\|^\([0-9a-f]*:\)\{0,1\}$LOSE$sep|d"
fi
LC_ALL=C tr '\0' "$sep" <"$LOSE_COUNT.out" | LC_ALL=C sed -e "$edit" | LC_ALL=C tr "$sep" '\0'
exit "$status"
SH
chmod +x "$tmp/bin/git"
export LOSE_COUNT="$tmp/lose-count"
losing() { # <when> <file> <command...>
  local when=$1 file=$2
  shift 2
  : >"$LOSE_COUNT"
  env PATH="$tmp/bin:$PATH" LOSE="$file" LOSE_WHEN="$when" "$@"
}

fresh coverage
commit_file "feat: a key" lost.txt "token = $key"
commit_file "feat: another" kept.txt "token = $key"
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }

# The stand-in itself changes nothing when it is told to lose nothing.
expect_fail "the stand-in, losing nothing" "lost.txt:1" losing never lost.txt "$scan"
says "kept.txt:1"

# A file the search never looks in: the census fails, in the working tree and in commits.
expect_fail "a file lost from every search, working tree" "were not looked in by the search (worktree)" \
  losing always lost.txt "$scan"
says "refusing to pass"
quiet "lost.txt"
never_clean "a lost file, working tree"
expect_fail "a file lost from every search, working tree, secrets only" "were not looked in by the search (worktree)" \
  losing always lost.txt env -u WHIZBANG_WORDLIST "$scan" --secrets-only
expect_fail "a file lost from every search, a range" "were not looked in by the search (commits)" \
  losing always lost.txt "$scan" --range "$first" HEAD
says "2 of the 3 files to cover"
never_clean "a lost file, a range"
printf 'refs/heads/coverage %s refs/heads/coverage %s\n' "$(git rev-parse HEAD)" "$zeros" >"$tmp/push-line"
expect_fail "a file lost from every search, a push" "were not looked in by the search (commits)" \
  losing always lost.txt bash -c "'$scan' --pre-push origin '$remote' <'$tmp/push-line'"
# The scan's own text files (paths, messages, link targets) are counted too.
expect_fail "one of the scan's text files lost from every search" "were not looked in by the search (files)" \
  losing always "tree-paths/working-tree" "$scan"

# A file lost now and then: the two runs of a search differ.
expect_fail "a file lost from every other search, working tree" "two runs of the same search disagree" \
  losing sometimes lost.txt "$scan"
says "refusing to pass"
never_clean "a file lost now and then, working tree"
expect_fail "a file lost from every other search, a range" "two runs of the same search disagree" \
  losing sometimes lost.txt "$scan" --range "$first" HEAD
never_clean "a file lost now and then, a range"

# Without the stand-in the same repository is scanned in full.
expect_fail "the same, with git as it is" "lost.txt:1" "$scan"
says "kept.txt:1"

# What the census counts and what it leaves out: an empty file, a link, a deleted file
# and a file with no newline do not make it fail.
fresh census-kinds
: >empty.txt
printf 'no newline' >short.txt
printf '\0\0' >nul.bin
ln -s short.txt a-link
commit_all "feat: kinds"
commit_file "feat: gone" gone.txt "soon gone"
rm gone.txt
expect_pass "kinds of file, working tree" "$scan"
expect_pass "kinds of file, a range" "$scan" --range "$first" HEAD

# A tracked file that an ignore rule matches is published like any other, so it is
# searched: a search of untracked files alone would skip it.
fresh ignored-tracked
printf '*.log\n' >.gitignore
printf 'token = %s\n' "$key" >kept.log
git add -f kept.log .gitignore
git commit -q --no-verify -m "feat: a log"
printf 'token = %s\n' "$key" >untracked.log
expect_fail "a key in a tracked file that an ignore rule matches" "kept.log:1" "$scan"
quiet "untracked.log"
rm untracked.log

# The canary (scripts/tests/scan/canary.sh) fails when a search loses a file, even once in
# a while, and says how often.
expect_fail "the canary with a search that loses a file now and then" "searches differed from the known output" \
  losing sometimes x.dat bash -c "cd '$root' && bash scripts/tests/scan/canary.sh"
says "the search canary: "
expect_fail "the canary with a search that always loses a file" "searches differed from the known output" \
  losing always x.dat bash -c "cd '$root' && bash scripts/tests/scan/canary.sh"

finished
