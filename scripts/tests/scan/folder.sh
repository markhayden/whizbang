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

finished
