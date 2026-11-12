# What every part of the scan tests starts from. Sourced, not run.
# Every case runs in a throwaway repository with a throwaway bare remote and a made-up word
# list. Keys, addresses, names and attribution lines are put together at run time, so these
# files hold nothing the real scan would flag.
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

scan="$root/scripts/scan.sh"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

chars=abcdefghij0123456789ABCDEFGHIJ
hex=0123456789abcdef0123456789abcdef
key="sk""-$chars"
home_path="/Us""ers/someone/notes"
address="someone@""corp.test"
word=zorblaxquint
N="Cl""aude"
credit="Co""-Authored-By"
zeros=0000000000000000000000000000000000000000

words="$tmp/words"
printf '%s\n\nflimwozzle\n' "$word" >"$words"
export WHIZBANG_WORDLIST="$words"

# A repository whose `main` is published to a bare remote, with this repo's hooks and scan
# installed (ignored there, as untracked tooling), so `git push` runs the real pre-push hook.
remote="$tmp/remote.git"
repo="$tmp/repo"
git init -q --bare -b main "$remote"
git init -q -b main "$repo"
cd "$repo"
git config user.name "A Tester"
git config user.email "tester@example.com"
mkdir -p scripts/hooks
cp "$scan" scripts/scan.sh
cp "$root/scripts/hooks/pre-push" "$root/scripts/hooks/commit-msg" scripts/hooks/
printf 'scripts/\n.env\n' >.git/info/exclude
git config core.hooksPath scripts/hooks
git remote add origin "$remote"
echo "hello" >readme.txt
git add readme.txt
git commit -q -m "docs: a readme"
git push -q origin main >/dev/null 2>&1
first=$(git rev-parse HEAD)

# commit_file <message> <file> <content>: one commit that sets the file's content. It is
# made with the commit hooks bypassed, as a commit from another machine would arrive.
commit_file() {
  mkdir -p "$(dirname "$2")"
  printf '%s\n' "$3" >"$2"
  git add -- "$2"
  git commit -q --no-verify -m "$1"
}
# commit_all <message>: one commit of everything in the working tree, hooks bypassed.
commit_all() {
  git add -A
  git commit -q --no-verify -m "$1"
}
# fresh <name>: a new branch from the published commit.
fresh() {
  git checkout -q -f -B "$1" "$first"
  git clean -q -f -d
}
# not_on_remote <ref>: the push was refused, so the remote does not have the ref.
not_on_remote() {
  [ -z "$(git ls-remote "$remote" "$1")" ] || fail "$1 reached the remote"
}
# quiet <text>: the last output does not hold the text.
quiet() {
  if grep -a -q -i -F -e "$1" "$tmp/out"; then
    cat "$tmp/out" >&2
    fail "the output holds text it must never print"
  fi
}
# says <text>: the last output holds the text.
says() {
  grep -a -q -F -e "$1" "$tmp/out" || {
    cat "$tmp/out" >&2
    fail "expected the output to say: $1"
  }
}
# The stand-in: any git command but a search goes to git as it is. A search's output loses
# every record and every name of the file LOSE names, when LOSE_WHEN says so: `always`, or
# `sometimes` (every other search for a pattern, never the census). With `always`, git's
# own list of files loses the name too, as it would if the fault were in how git walks a
# folder. BREAK names a git command that fails outright. It can also speak on
# stderr during one kind of search (NOISY: worktree, files or commits), or whenever its
# arguments hold the words of NOISY_ON in a row (`ls-files -z -c` is one listing and no
# other), and then go on as git would; answer nothing when asked which paths are ignored
# (MUTE=check-ignore); wait a moment (SLOW); and note what the scan's own folder holds at
# that moment (SPY).
stand_in_for_git() {
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
  # BREAK names a git command that fails outright, with a message, as on a broken install.
  if [ -n "${BREAK:-}" ] && [ "$arg" = "$BREAK" ]; then
    echo "fatal: this command is broken" >&2
    exit 128
  fi
  case "$arg" in
    grep) search=1 ;;
    -l) census=1 ;;
    ls-files) [ "${LOSE_WHEN:-}" != always ] || search=1 census=1 ;;
  esac
done
if [ -n "${NOISY_ON:-}" ]; then
  case " $* " in *" $NOISY_ON "*) echo "warning: something git says" >&2 ;; esac
fi
if [ "${MUTE:-}" = check-ignore ]; then
  case " $* " in *" check-ignore "*)
    "$real" "$@" >/dev/null 2>&1
    exit 1
    ;;
  esac
fi
if [ "$search" -eq 1 ]; then
  kind=commits
  case " $* " in *" --untracked "*) kind=worktree ;; *" --no-index "*) kind=files ;; esac
  [ "${NOISY:-}" != "$kind" ] || echo "warning: something git says" >&2
  if [ -n "${SPY:-}" ]; then
    # Anything in the scan's folder that another user could read; and the word, anywhere.
    ls -lAR "$TMPDIR" | grep -E '^[-d]' | grep -v -E '^.{4}------' >>"$SPY.loose" || true
    ls -A "$TMPDIR" >>"$SPY.seen"
    ! grep -r -a -q -i -F -e "$SPY_WORD" "$TMPDIR" || echo found >>"$SPY.word"
    # The census's list of the working tree, once it is there.
    for listed in "$TMPDIR"/whizbang-scan.*/census-listed; do
      [ ! -f "$listed" ] || [ "$kind" != worktree ] || cp "$listed" "$SPY.listed"
    done
  fi
  if [ -n "${SLOW:-}" ]; then
    echo started >>"$SLOW"
    sleep 0.3
  fi
fi
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
}

# tree_says <place>: `says`, for a scan of the working tree. When the place is missing it
# first reports what the folder held and what two more scans find, the second with git grep
# on one thread. On CI's macOS runner git grep on several threads has lost a match at
# random (docs/carry-over.md), and a scan on one thread has not.
# The stand-in for find, which the scan uses to list the working tree. FIND_DOES says what
# it does: `speak` fails with a line that names a path; `fail` fails and says nothing;
# `warn` says a line and then lists as find would; `keep-out` fails when it is asked to
# look in the folder FIND_FOLDER or lists anything inside it, as find does when a folder
# changes under it; `vanish` removes FIND_FOLDER after its call number FIND_AFTER.
# Anything else: find as it is.
stand_in_for_find() {
real_find=$(command -v find)
mkdir "$tmp/find-bin"
cat >"$tmp/find-bin/find" <<SH
#!/usr/bin/env bash
real="$real_find"
calls="$tmp/find-calls"
out="$tmp/find-out"
SH
cat >>"$tmp/find-bin/find" <<'SH'
echo x >>"$calls"
call=$(wc -l <"$calls" | tr -d ' ')
case "${FIND_DOES:-}" in
  speak)
    echo "find: ./a-path-it-names: Permission denied" >&2
    exit 1
    ;;
  fail) exit 1 ;;
  warn)
    echo "find: ./a-path-it-names: something find says" >&2
    exec "$real" "$@"
    ;;
  keep-out)
    for arg in "$@"; do
      case "$arg" in "./$FIND_FOLDER" | "./$FIND_FOLDER"/*)
        echo "find: ./$FIND_FOLDER: it changed while find was in it" >&2
        exit 1
        ;;
      esac
    done
    status=0
    "$real" "$@" >"$out.$call" || status=$?
    if LC_ALL=C grep -a -q -F -e "./$FIND_FOLDER/" "$out.$call"; then
      echo "find: ./$FIND_FOLDER: it changed while find was in it" >&2
      exit 1
    fi
    cat "$out.$call"
    exit "$status"
    ;;
  vanish)
    status=0
    "$real" "$@" || status=$?
    [ "$call" -ne "$FIND_AFTER" ] || rm -rf "$FIND_FOLDER"
    exit "$status"
    ;;
  *) exec "$real" "$@" ;;
esac
SH
chmod +x "$tmp/find-bin/find"
}
# finding <what> [NAME=value...] <command...>: the command with the stand-in for find,
# doing <what>.
finding() {
  local what=$1
  shift
  : >"$tmp/find-calls"
  env PATH="$tmp/find-bin:$PATH" FIND_DOES="$what" "$@"
}

tree_says() {
  local f
  if ! grep -a -q -F -e "$1" "$tmp/out"; then
    cp "$tmp/out" "$tmp/first"
    {
      echo "missing from the working-tree scan: $1"
      git --version
      echo "the files of the folder, with their sizes in bytes:"
      for f in ./*; do [ -L "$f" ] || [ ! -f "$f" ] || wc -c "$f"; done
      echo "files git lists: $(git ls-files -co --exclude-standard | wc -l | tr -d ' ')"
      # The scan exits 1 when it finds anything, so its output is kept and then read.
      "$scan" >"$tmp/second" 2>&1 || true
      if grep -a -q -F -e "$1" "$tmp/second"; then
        echo "a second scan reports it"
      else
        echo "a second scan does not report it either"
      fi
      GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=grep.threads GIT_CONFIG_VALUE_0=1 "$scan" >"$tmp/second" 2>&1 || true
      if grep -a -q -F -e "$1" "$tmp/second"; then
        echo "a scan on one thread reports it"
      else
        echo "a scan on one thread does not report it either"
      fi
    } >&2
    cp "$tmp/first" "$tmp/out"
  fi
  says "$1"
}

has_credit="the message has a ${credit} line; this repo allows none, for a person or an agent"
credits_agent="the message credits an agent or a model"
