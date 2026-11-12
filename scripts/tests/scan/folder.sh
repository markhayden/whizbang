#!/usr/bin/env bash
# What the scan does besides searching: it fails when git says anything during a search,
# it keeps git quiet itself, and its own folder is private, never holds the word list, and
# is gone when the scan ends, however it ends. Tested with the stand-in for git of
# common.sh, which can speak, wait and look.
source "$(dirname "$0")/common.sh"
stand_in_for_git
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }

fresh speaking
commit_file "feat: a key" lost.txt "token = $key"
commit_file "feat: another" kept.txt "token = $key"

# Anything git says on stderr during a search fails the scan, whichever search it is: of
# the working tree, of commits, of the scan's own text files.
speaking() { # <kind> <command...>
  local kind=$1
  shift
  env PATH="$tmp/bin:$PATH" NOISY="$kind" "$@"
}
for kind in worktree files; do
  expect_fail "git speaks during a search ($kind), working tree" "git said something during a search (1 lines" \
    speaking "$kind" "$scan"
  says "any such line counts"
  quiet "something git says"
  never_clean "git speaks ($kind)"
done
for kind in commits files; do
  expect_fail "git speaks during a search ($kind), a range" "git said something during a search" \
    speaking "$kind" "$scan" --range "$first" HEAD
  never_clean "git speaks ($kind), a range"
done
# Tracing would make git speak; the scan switches it off for itself.
rm lost.txt kept.txt
expect_pass "a scan with git's tracing on" env GIT_TRACE=1 GIT_TRACE2=1 GIT_TRACE_SETUP=1 GIT_CURL_VERBOSE=1 "$scan"
says "secrets scan: clean"
git checkout -q -- lost.txt kept.txt

# The scan's own folder: only this user can read anything in it, the word list is never
# copied into it, and it is gone when the scan ends, however it ends.
mkdir "$tmp/scan-tmp"
spied() { # <command...>: the scan with its folder under $tmp/scan-tmp, watched
  rm -f "$tmp/spy.loose" "$tmp/spy.seen" "$tmp/spy.word"
  env PATH="$tmp/bin:$PATH" TMPDIR="$tmp/scan-tmp" SPY="$tmp/spy" SPY_WORD="$word" "$@"
}
gone() { [ -z "$(ls -A "$tmp/scan-tmp")" ] || { ls -lAR "$tmp/scan-tmp" >&2; fail "$1: the scan's folder is gone"; }; }
fresh own-folder
commit_file "docs: no listed word in the contents" plain.txt "nothing to see"
expect_pass "a clean scan, watched" spied "$scan"
[ -s "$tmp/spy.seen" ] || fail "the scan's folder was seen during the scan"
[ ! -s "$tmp/spy.loose" ] || { cat "$tmp/spy.loose" >&2; fail "nothing in the scan's folder can be read by another user"; }
[ ! -e "$tmp/spy.word" ] || fail "the word list is not copied into the scan's folder"
gone "after a clean scan"
expect_pass "a clean range, watched" spied "$scan" --range "$first" HEAD
[ ! -s "$tmp/spy.loose" ] || { cat "$tmp/spy.loose" >&2; fail "nothing in the scan's folder can be read by another user (a range)"; }
[ ! -e "$tmp/spy.word" ] || fail "the word list is not copied into the scan's folder (a range)"
gone "after a clean range"
printf 'token = %s\n' "$key" >found.txt
expect_fail "a scan that finds a key, watched" "found.txt:1" spied "$scan"
gone "after a scan that fails"
expect_fail "a scan that cannot run" "refusing to pass" spied env WHIZBANG_WORDLIST="$tmp/no-such-list" "$scan"
gone "after a scan that cannot run"
rm found.txt
# A scan that stops early through no exit of its own (the trap's 70).
awk '{ print } /^trap .* EXIT$/ && !put { print "exit 0"; put = 1 }' "$scan" >"$tmp/stopping-scan.sh"
status=0
spied bash "$tmp/stopping-scan.sh" >"$tmp/out" 2>&1 || status=$?
[ "$status" = 70 ] || { cat "$tmp/out" >&2; fail "a scan that stops early exits 70 (got $status)"; }
gone "after a scan that stops early"
# A signal. The scan is started with the signal at its default, as a terminal would start
# it (a command put in the background by a script has INT ignored, which no trap undoes).
for signal in TERM INT HUP; do
  rm -f "$tmp/slow"
  env PATH="$tmp/bin:$PATH" TMPDIR="$tmp/scan-tmp" SLOW="$tmp/slow" \
    perl -e '$SIG{INT} = "DEFAULT"; $SIG{HUP} = "DEFAULT"; exec @ARGV' "$scan" >"$tmp/signal-out" 2>&1 &
  pid=$!
  i=0
  until [ -s "$tmp/slow" ] || [ "$i" -ge 100 ]; do sleep 0.1; i=$((i + 1)); done
  [ -s "$tmp/slow" ] || fail "the scan started a search before $signal"
  [ -n "$(ls -A "$tmp/scan-tmp")" ] || fail "the scan has its folder when $signal arrives"
  kill -s "$signal" "$pid"
  status=0
  wait "$pid" || status=$?
  case "$signal:$status" in TERM:143 | INT:130 | HUP:129) ;; *) cat "$tmp/signal-out" >&2; fail "a scan ended by $signal exits with its status (got $status)" ;; esac
  gone "after $signal"
done

# The census's own list of the working tree is exactly git's: every tracked file and every
# untracked one that no ignore rule keeps out (regular files with something in them; links
# aside). The census makes it without git walking the folder, and it never enters a folder
# git says is ignored, so the rules are the place to go wrong: rules that put a file back,
# rules in nested files, rules for folders, and what a rule under an ignored folder can
# and cannot put back.
fresh the-lister
# The same name composed and decomposed: one file to macOS, where git composes the names
# it reads from a folder and find does not; two files elsewhere.
nfc=$(printf 'caf\303\251')
nfd=$(printf 'cafe\314\201')
printf '%s\n' '*.log' '!keep.log' 'out/*' '!out/keep/' 'build/' '!build/back.txt' '/top-only.txt' 'docs/**/*.png' '!docs/**/shown.png' \
  'gen/' '!gen/sub/' '!gen/sub/kept.txt' 'cache/**' '!cache/**/keep.me' 'logs/' 'with space ignored/' 'two*ignored/' '-dash-ignored/' \
  'partly/*' '!partly/keep/' '*.gone' "ig-$nfc/" >.gitignore
printf '%s\n' 'local-*' >>.git/info/exclude
two_lines=$(printf 'two\nlines')
mkdir -p out/keep out/drop build/deep sub/inner docs/a/b "a folder" nested/top-only.txt.d \
  gen/sub cache/a/b deep/logs "with space ignored/in" "$two_lines ignored/in" "$two_lines kept" ./-dash-ignored/in ./-dash-kept \
  partly/keep/deeper partly/drop/keep build/own-rules "dir-$nfd/in-$nfd" "dir-$nfc-too" "ig-$nfd" "ig-$nfc"
printf '%s\n' '!special.log' 'secret-*' '*.tmp' >sub/.gitignore
printf '%s\n' '!*.tmp' 'inner-only' >sub/inner/.gitignore
# Files of rules in folders whose names git prints in quotes, or that read like the
# place of a rule themselves.
mkdir "odd:3:!source"
printf '%s\n' 'here-only' >"$two_lines kept/.gitignore"
printf '%s\n' 'here-only' '!back' >"dir-$nfd/.gitignore"
printf '%s\n' 'here-only' 'ba*' '!back' >"odd:3:!source/.gitignore"
for f in "$two_lines kept/here-only" "dir-$nfd/here-only" "dir-$nfd/back" "odd:3:!source/here-only" "odd:3:!source/back" \
  "odd:3:!source/bat"; do
  printf 'content of %s\n' "$f" >"$f"
done
# A file of rules inside an ignored folder puts nothing back.
printf '%s\n' '!*' '!put-back.txt' >build/own-rules/.gitignore
for f in keep.log drop.log out/keep/in.txt out/keep/in.log out/drop/x.txt out/file.txt build/back.txt build/deep/y.txt \
  top-only.txt nested/top-only.txt docs/a/b/hidden.png docs/a/b/shown.png docs/a/plain.md sub/special.log sub/other.log \
  sub/secret-1 sub/x.tmp sub/inner/y.tmp sub/inner/inner-only sub/inner/z.log local-notes.txt "a folder/with space.txt" \
  "a folder/-dash" plain.txt \
  gen/tracked.txt gen/loose.txt gen/sub/kept.txt gen/sub/other.txt \
  cache/keep.me cache/other.txt cache/a/keep.me cache/a/b/keep.me cache/a/other.txt \
  logs deep/logs/day.txt deep/beside.txt \
  "with space ignored/x.txt" "with space ignored/in/y.txt" "$two_lines ignored/x.txt" "$two_lines ignored/in/y.txt" \
  "$two_lines kept/x.txt" ./-dash-ignored/x.txt ./-dash-ignored/in/y.txt ./-dash-kept/x.txt \
  partly/file.txt partly/keep/x.txt partly/keep/deeper/y.txt partly/drop/x.txt partly/drop/keep/z.txt \
  build/own-rules/put-back.txt \
  axb.gone 'a*b.gone' 'a?b.gone' 'a[wxy]b.gone' 'star*.txt' 'query?.txt' 'set[a].txt' \
  "nfd-$nfd.txt" "nfc-$nfc.txt" "both-$nfc.txt" "both-$nfd.txt" "dir-$nfd/in.txt" "dir-$nfd/in-$nfd/deep-$nfd.txt" \
  "dir-$nfc-too/in.txt" "ig-$nfd/x.txt" "ig-$nfc/y.txt"; do
  printf 'content of %s\n' "$f" >"$f"
done
printf 'a name over two lines\n' >"$(printf 'two\nlines.txt')"
: >empty.txt
ln -s plain.txt a-link
ln -s "a folder" a-folder-link
# A link to an ignored file is a link, not that file.
ln -s out/file.txt a-link-to-an-ignored-file
# A link to a folder, inside a folder that is listed: the walk does not go through it.
ln -s ../build partly/keep/a-way-in
# Tracked files, in and out of what the rules match.
# Among them a tracked file that an ignored one's name matches, when the name is read as
# a pattern: `a*b.gone` is ignored whatever axb.gone is.
# And a tracked link to an ignored file: the link is tracked, the file is not covered.
git add -f plain.txt drop.log build/deep/y.txt sub/secret-1 gen/tracked.txt axb.gone a-link-to-an-ignored-file .gitignore
git commit -q --no-verify -m "feat: tracked"
lists_what_git_lists "a tree full of ignore rules"
# The cases are really in there: put back by a rule, kept out by one, tracked though matched.
for f in keep.log out/keep/in.txt docs/a/b/shown.png sub/special.log sub/inner/y.tmp drop.log build/deep/y.txt sub/secret-1 \
  "a folder/with space.txt" gen/tracked.txt cache/keep.me logs deep/beside.txt -dash-kept/x.txt \
  partly/keep/x.txt partly/keep/deeper/y.txt "$two_lines kept/x.txt" "$two_lines.txt" \
  axb.gone 'star*.txt' 'query?.txt' 'set[a].txt' "nfc-$nfc.txt" "dir-$nfc-too/in.txt" \
  "nfd-$nfd.txt" "dir-$nfd/in.txt" "dir-$nfd/in-$nfd/deep-$nfd.txt" "both-$nfd.txt" "both-$nfc.txt" \
  "dir-$nfd/back" "odd:3:!source/back" "$two_lines kept/.gitignore"; do
  listed "$f" || fail "the list holds $f"
done
for f in out/drop/x.txt out/file.txt build/back.txt top-only.txt docs/a/b/hidden.png sub/other.log sub/x.tmp \
  sub/inner/inner-only local-notes.txt out/keep/in.log empty.txt a-link \
  gen/loose.txt gen/sub/kept.txt gen/sub/other.txt cache/other.txt cache/a/keep.me cache/a/b/keep.me cache/a/other.txt \
  deep/logs/day.txt "with space ignored/x.txt" "with space ignored/in/y.txt" -dash-ignored/x.txt -dash-ignored/in/y.txt \
  "$two_lines ignored/x.txt" "$two_lines ignored/in/y.txt" \
  partly/file.txt partly/drop/x.txt partly/drop/keep/z.txt build/own-rules/put-back.txt build/own-rules/.gitignore \
  partly/keep/a-way-in/back.txt partly/keep/a-way-in/deep/y.txt 'a*b.gone' 'a?b.gone' 'a[wxy]b.gone' "ig-$nfc/y.txt" \
  "$two_lines kept/here-only" "dir-$nfd/here-only" "odd:3:!source/here-only" "odd:3:!source/bat"; do
  listed "$f" && fail "the list does not hold $f"
done
# The folder with the decomposed name: where git composes names, the rule written
# composed ignores it; elsewhere it is another name, and its file is listed.
if [ "$(uname -s)" = Darwin ] && [ "$(git config --type=bool core.precomposeunicode || true)" = true ]; then
  listed "ig-$nfd/x.txt" && fail "a rule with a composed name ignores the folder with the decomposed one"
else
  listed "ig-$nfd/x.txt" || fail "the list holds ig-$nfd/x.txt, which no rule names"
fi
listed nested/top-only.txt || fail "a rule for the top folder alone leaves nested/top-only.txt in"

finished
