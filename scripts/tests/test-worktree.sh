#!/usr/bin/env bash
# scripts/worktree.sh: add makes a worktree and a branch at a freshly fetched origin/main;
# remove takes them away only when the branch is merged there; a bad slug changes nothing.
# Everything runs in a throwaway repo whose "origin" is a bare repo beside it. The script
# under test is this checkout's copy, committed into the throwaway repo, because it works
# on the repo it lives in.
source "$(dirname "$0")/lib.sh"

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset WHIZBANG_WORDLIST

w=$(cd "$tmp" && pwd -P)/w
repo=$w/whizbang
other=$w/elsewhere/other
mkdir -p "$w/elsewhere"

# new_clone <folder>: a checkout with an identity and `origin`, on the branch main.
new_clone() {
  git init -q "$1"
  git -C "$1" symbolic-ref HEAD refs/heads/main
  git -C "$1" config user.name "A Tester"
  git -C "$1" config user.email "tester@example.com"
  git -C "$1" remote add origin "$w/origin.git"
}

git init -q --bare "$w/origin.git"
git -C "$w/origin.git" symbolic-ref HEAD refs/heads/main
new_clone "$repo"
mkdir -p "$repo/scripts/hooks"
cp scripts/worktree.sh "$repo/scripts/worktree.sh"
cp scripts/hook-dispatch.sh "$repo/scripts/hook-dispatch.sh"
for hook in pre-commit commit-msg pre-push; do
  printf '%s\n' '#!/bin/sh' 'exit 0' >"$repo/scripts/hooks/$hook"
  chmod +x "$repo/scripts/hooks/$hook"
done
printf '%s\n' '.env' '.env.*' 'target/' >"$repo/.gitignore"
git -C "$repo" add scripts .gitignore
git -C "$repo" commit -q -m "build: the script and its hooks"
git -C "$repo" push -q origin main
new_clone "$other"
git -C "$other" pull -q origin main

# wt <arguments...>: the script, run from the main checkout of the throwaway repo.
wt() { (cd "$repo" && scripts/worktree.sh "$@"); }
# state: every folder beside the repo, and every local branch.
state() {
  ls "$w"
  git -C "$repo" for-each-ref --format='%(refname)' refs/heads
}
unchanged() {
  [ "$(state)" = "$1" ] || { state >&2; fail "$2: something was created or removed"; }
}
# advance_origin <file>: another checkout adds a commit to origin's main.
advance_origin() {
  git -C "$other" pull -q origin main
  echo "$1" >"$other/$1"
  git -C "$other" add "$1"
  git -C "$other" commit -q -m "docs: $1"
  git -C "$other" push -q origin main
}

# --- Where it runs -----------------------------------------------------------------------

expect_fail "from a folder that is no repo" "run it from inside the repo it belongs to" \
  bash -c 'cd "$1" && "$2/scripts/worktree.sh" list' - "$w" "$repo"
expect_fail "from another repo" "run it from inside the repo it belongs to" \
  bash -c 'cd "$1" && "$2/scripts/worktree.sh" add s1' - "$other" "$repo"
expect_fail "no command" "usage: worktree.sh" wt
expect_fail "an unknown command" "usage: worktree.sh" wt prune
expect_fail "add with no slug" "usage: worktree.sh" wt add
expect_fail "add with two slugs" "usage: worktree.sh" wt add s1 s2
expect_fail "remove with no slug" "usage: worktree.sh" wt remove
expect_fail "list with an argument" "usage: worktree.sh" wt list s1

# --- The hooks must be installed first ---------------------------------------------------

before=$(state)
expect_fail "add before the hooks are installed" "the hooks are not installed" wt add s1
unchanged "$before" "add before the hooks are installed"
# The setting an earlier setup made is not an installation: a push from another folder ran
# no hook with it.
git -C "$repo" config core.hooksPath scripts/hooks
expect_fail "add with the hooks path of an earlier setup" "the hooks are not installed" wt add s1
unchanged "$before" "add with the hooks path of an earlier setup"
git -C "$repo" config --unset core.hooksPath
# Installed as `just setup` installs them.
install_hooks() {
  local hook
  for hook in pre-commit commit-msg pre-push; do
    cp "$repo/scripts/hook-dispatch.sh" "$repo/.git/hooks/$hook"
    chmod +x "$repo/.git/hooks/$hook"
  done
}
install_hooks
# One that is missing, cannot be run, or is not this repo's: not installed.
rm "$repo/.git/hooks/pre-push"
expect_fail "add with an installed hook missing" "the hooks are not installed" wt add s1
install_hooks
chmod -x "$repo/.git/hooks/commit-msg"
expect_fail "add with an installed hook that cannot be run" "the hooks are not installed" wt add s1
install_hooks
printf '%s\n' '#!/bin/sh' 'exit 0' >"$repo/.git/hooks/pre-push"
expect_fail "add with an installed hook that is another script" "the hooks are not installed" wt add s1
install_hooks
# With them installed, a setting that sends git elsewhere for its hooks switches them off.
git -C "$repo" config core.hooksPath "$w/other-hooks"
expect_fail "add with core.hooksPath set" "core.hooksPath is set" wt add s1
git -C "$repo" config --unset core.hooksPath
unchanged "$before" "add while the hooks are not installed"

# --- A bad slug changes nothing ----------------------------------------------------------

long=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
[ "${#long}" -eq 65 ] || fail "the long slug is 65 characters"
for bad in '' '-' '-x' '--force' '--detach' 'a/b' '/abs' '..' '../x' 'a..b' 'x.y' 'A' 'Task' 'a b' ' a' \
  'a;b' 'a$b' 'a*' 'a~1' 'a^' 'a:b' 'a@{0}' 'a\b' $'a\nb' $'a\tb' "$long"; do
  expect_fail "add with the slug [$bad]" "a slug is" wt add "$bad"
  unchanged "$before" "add with the slug [$bad]"
  expect_fail "remove with the slug [$bad]" "a slug is" wt remove "$bad"
  unchanged "$before" "remove with the slug [$bad]"
done
for name in main head; do
  expect_fail "add $name" "the slug $name is not a task" wt add "$name"
  expect_fail "remove $name" "the slug $name is not a task" wt remove "$name"
done
expect_fail "add HEAD" "a slug is" wt add HEAD
expect_fail "add Head" "a slug is" wt add Head
unchanged "$before" "the slugs main and head"

# --- add ---------------------------------------------------------------------------------

# origin's main moves after the repo last fetched it: the worktree starts at the new commit.
advance_origin one
tip=$(git -C "$w/origin.git" rev-parse refs/heads/main)
[ "$(git -C "$repo" rev-parse refs/remotes/origin/main)" != "$tip" ] || fail "the repo has not fetched the new commit yet"

expect_pass "add" wt add s1
[ "$(git -C "$w/whizbang-s1" rev-parse HEAD)" = "$tip" ] || fail "add: the worktree starts at the fetched origin/main"
[ "$(git -C "$w/whizbang-s1" symbolic-ref --short HEAD)" = s1 ] || fail "add: the worktree is on the branch s1"
[ -z "$(git -C "$repo" config --get branch.s1.merge || true)" ] || fail "add: the new branch follows no upstream"
grep -q -F "hooks ok" "$tmp/out" || fail "add: says the hooks are installed"
grep -q -F "WHIZBANG_WORDLIST is not set: set it to the private word list the owner provides" "$tmp/out" ||
  fail "add: reminds about the word list"

# With the variable set, its value is never printed.
expect_pass "add with the word list set" env WHIZBANG_WORDLIST="$w/a-private-place" bash -c 'cd "$1" && scripts/worktree.sh add s2' - "$repo"
grep -q -F "WHIZBANG_WORDLIST is set" "$tmp/out" || fail "add: says the variable is set"
if grep -q -F "a-private-place" "$tmp/out"; then fail "add: printed the value of WHIZBANG_WORDLIST"; fi

before=$(state)
expect_fail "add a slug that has a folder and a branch" "is already there; nothing was created" wt add s1
git -C "$repo" branch only-a-branch
expect_fail "add a slug that has a branch" "the branch only-a-branch is already there" wt add only-a-branch
git -C "$repo" branch -D only-a-branch >/dev/null
mkdir "$w/whizbang-only-a-folder"
expect_fail "add a slug that has a folder" "is already there; nothing was created" wt add only-a-folder
[ ! -e "$w/whizbang-only-a-folder/.git" ] || fail "add: a folder that was there is left alone"
rmdir "$w/whizbang-only-a-folder"
unchanged "$before" "add a slug that is taken"

# From inside another worktree, the new one still goes beside the main checkout.
expect_pass "add from a worktree" bash -c 'cd "$1" && scripts/worktree.sh add s3' - "$w/whizbang-s1"
[ -d "$w/whizbang-s3" ] || fail "add from a worktree: the folder is beside the main checkout"

# A worktree whose branch has no hooks is made, and reported.
git -C "$other" pull -q origin main
git -C "$other" rm -q scripts/hooks/pre-push
git -C "$other" commit -q -m "build: no pre-push hook"
git -C "$other" push -q origin main
expect_fail "add when main has no pre-push hook" "its hook scripts/hooks/pre-push is missing" wt add s4

# --- list --------------------------------------------------------------------------------

expect_pass "list" wt list
for name in whizbang-s1 whizbang-s2 whizbang-s3 whizbang-s4 '[s1]' '[main]'; do
  grep -q -F -e "$name" "$tmp/out" || { cat "$tmp/out" >&2; fail "list: names $name"; }
done

# --- remove ------------------------------------------------------------------------------

# Not merged: a commit on the branch that origin/main does not have, pushed or not.
echo work >"$w/whizbang-s1/work"
git -C "$w/whizbang-s1" add work
git -C "$w/whizbang-s1" commit -q -m "feat: work"
before=$(state)
expect_fail "remove a branch that is not merged" "the branch s1 is not merged into origin/main; nothing was removed" wt remove s1
git -C "$w/whizbang-s1" push -q origin s1
expect_fail "remove a branch that is pushed but not merged" "is not merged into origin/main; nothing was removed" wt remove s1
for flag in --force -f; do
  expect_fail "remove with $flag" "usage: worktree.sh" wt remove "$flag" s1
  expect_fail "remove with $flag after the slug" "usage: worktree.sh" wt remove s1 "$flag"
done
unchanged "$before" "remove a branch that is not merged"
[ -f "$w/whizbang-s1/work" ] || fail "remove: the unmerged work is still there"

# Merged by another checkout, with a merge commit: the repo learns of it by fetching.
git -C "$other" fetch -q origin
git -C "$other" merge -q --no-ff -m "Merge s1" origin/s1
git -C "$other" push -q origin main
expect_fail "remove from inside the worktree" "run it from another worktree" \
  bash -c 'cd "$1" && scripts/worktree.sh remove s1' - "$w/whizbang-s1"
unchanged "$before" "remove from inside the worktree"
expect_pass "remove a merged branch" wt remove s1
[ ! -e "$w/whizbang-s1" ] || fail "remove: the worktree is gone"
if git -C "$repo" show-ref --verify --quiet refs/heads/s1; then fail "remove: the local branch is gone"; fi
git -C "$w/origin.git" show-ref --verify --quiet refs/heads/s1 || fail "remove: the branch on origin is left alone"

# Merged (no commit of its own), but the worktree holds a file git does not ignore.
echo note >"$w/whizbang-s2/note"
before=$(state)
expect_fail "remove a worktree with an untracked file" "git would not remove" wt remove s2
unchanged "$before" "remove a worktree with an untracked file"
[ -f "$w/whizbang-s2/note" ] || fail "remove: the untracked file is still there"
rm "$w/whizbang-s2/note"
expect_pass "remove once the worktree is clean" wt remove s2
[ ! -e "$w/whizbang-s2" ] || fail "remove: the clean worktree is gone"

# A branch that is checked out somewhere else is never deleted.
git -C "$w/whizbang-s3" checkout -q -b s5
before=$(state)
expect_fail "remove a branch checked out in another folder" "is checked out in" wt remove s5
expect_fail "remove a slug whose folder is on another branch" "is not a worktree on the branch s3" wt remove s3
unchanged "$before" "remove a branch that is checked out elsewhere"
git -C "$w/whizbang-s3" checkout -q s3
git -C "$repo" branch -D s5 >/dev/null

# A branch with no worktree, and a slug with nothing.
git -C "$repo" branch -q loose refs/remotes/origin/main
expect_pass "remove a merged branch that has no worktree" wt remove loose
if git -C "$repo" show-ref --verify --quiet refs/heads/loose; then fail "remove: the loose branch is gone"; fi
before=$(state)
expect_fail "remove a slug that has nothing" "there is no local branch nothing-here; nothing was removed" wt remove nothing-here
unchanged "$before" "remove a slug that has nothing"

# origin cannot be reached: nothing is removed on the word of an old fetch.
mv "$w/origin.git" "$w/origin.gone"
before=$(state)
expect_fail "remove when origin cannot be fetched" "cannot fetch origin/main; nothing was changed" wt remove s3
expect_fail "add when origin cannot be fetched" "cannot fetch origin/main; nothing was changed" wt add s6
unchanged "$before" "origin cannot be fetched"
mv "$w/origin.gone" "$w/origin.git"
expect_pass "remove the rest" wt remove s3

# main gets its pre-push hook back, so that add passes again.
git -C "$other" pull -q origin main
printf '%s\n' '#!/bin/sh' 'exit 0' >"$other/scripts/hooks/pre-push"
chmod +x "$other/scripts/hooks/pre-push"
git -C "$other" add scripts/hooks/pre-push
git -C "$other" commit -q -m "build: the pre-push hook again"
git -C "$other" push -q origin main

# --- A name that is taken somewhere else -------------------------------------------------

git -C "$repo" tag tagged
git -C "$w/origin.git" show-ref --verify --quiet refs/heads/s1 || fail "origin still has the branch s1"
if git -C "$repo" show-ref --verify --quiet refs/heads/s1; then fail "the repo has no local branch s1"; fi
before=$(state)
expect_fail "add a slug that is a tag" "there is a tag named tagged" wt add tagged
expect_fail "add a slug that is a branch on origin" "origin already has a branch named s1" wt add s1
unchanged "$before" "add a name that is taken"
git -C "$repo" tag -d tagged >/dev/null

# A tag that only origin has: the repo has never fetched it, so origin is asked.
git -C "$w/origin.git" tag far-tag refs/heads/main
if git -C "$repo" show-ref --verify --quiet refs/tags/far-tag; then fail "the repo has no local tag far-tag"; fi
before=$(state)
expect_fail "add a slug that is a tag on origin" "origin already has a tag named far-tag" wt add far-tag
unchanged "$before" "add a slug that is a tag on origin"
git -C "$w/origin.git" tag -d far-tag >/dev/null
# The same for an annotated tag, which origin lists twice.
git -C "$w/origin.git" -c user.name="A Tester" -c user.email="tester@example.com" tag -a -m "a tag" far-note refs/heads/main
expect_fail "add a slug that is an annotated tag on origin" "origin already has a tag named far-note" wt add far-note
unchanged "$before" "add a slug that is an annotated tag on origin"
git -C "$w/origin.git" tag -d far-note >/dev/null

# origin answers the fetch but not the question: that is a refusal, never "not found".
# Git's exit of 2 alone means "not found"; every other failure is tried on its own.
mkdir "$tmp/no-answer"
cat >"$tmp/no-answer/template" <<'SH'
#!/bin/sh
case " $* " in
  *" ls-remote "*) echo "$0 $*" >>"$ASKED"; echo "fatal: could not read from remote repository" >&2; exit CODE ;;
esac
exec "$REAL_GIT" "$@"
SH
for code in 128 1 3; do
  sed "s/exit CODE/exit $code/" "$tmp/no-answer/template" >"$tmp/no-answer/git"
  chmod +x "$tmp/no-answer/git"
  grep -q -x -F "  *\" ls-remote \"*) echo \"\$0 \$*\" >>\"\$ASKED\"; echo \"fatal: could not read from remote repository\" >&2; exit $code ;;" "$tmp/no-answer/git" ||
    fail "the stand-in for git does not exit $code"
  : >"$tmp/asked"
  expect_fail "add when origin cannot be asked (exit $code)" "cannot ask origin for its branches and tags; nothing was created" \
    env PATH="$tmp/no-answer:$PATH" REAL_GIT="$(command -v git)" ASKED="$tmp/asked" bash -c 'cd "$1" && scripts/worktree.sh add s12' - "$repo"
  [ -s "$tmp/asked" ] || fail "add when origin cannot be asked (exit $code): the stand-in was not asked"
  unchanged "$before" "add when origin cannot be asked (exit $code)"
done

# --- Git's own variables: each would make git act on another repo ------------------------

git -C "$other" branch -q victim
before=$(state)
other_before=$(git -C "$other" for-each-ref)
for var in GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE; do
  for value in "$other/.git" ""; do
    expect_fail "remove with $var=$value" "$var is set" \
      env "$var=$value" bash -c 'cd "$1" && scripts/worktree.sh remove victim' - "$repo"
    expect_fail "add with $var=$value" "$var is set" \
      env "$var=$value" bash -c 'cd "$1" && scripts/worktree.sh add s7' - "$repo"
    expect_fail "list with $var=$value" "$var is set" \
      env "$var=$value" bash -c 'cd "$1" && scripts/worktree.sh list' - "$repo"
  done
done
unchanged "$before" "git's own variables"
[ "$(git -C "$other" for-each-ref)" = "$other_before" ] || fail "git's own variables: the other repo was changed"

# --- A symbolic link where the folder should be is never followed ------------------------

git -C "$repo" worktree add -q --no-track -b linked "$w/elsewhere/real" refs/remotes/origin/main
ln -s "$w/elsewhere/real" "$w/whizbang-linked"
ln -s "$w/elsewhere" "$w/whizbang-s8"
ln -s "$w/nowhere" "$w/whizbang-s9"
before=$(state)
expect_fail "remove through a link to a worktree elsewhere" "is a symbolic link" wt remove linked
[ -d "$w/elsewhere/real/scripts" ] || fail "remove: the worktree behind the link is still there"
expect_fail "add where a link to a folder is" "is a symbolic link" wt add s8
expect_fail "add where a link to nothing is" "is a symbolic link" wt add s9
unchanged "$before" "a symbolic link"
[ ! -e "$w/nowhere" ] || fail "add: nothing was made behind the link"
rm "$w/whizbang-linked" "$w/whizbang-s8" "$w/whizbang-s9"
git -C "$repo" worktree remove "$w/elsewhere/real"
git -C "$repo" branch -q -D linked

# --- The branch is deleted only if it is still the commit that was checked ---------------

# A git that moves the branch to a commit that is not merged, just before the delete.
git -C "$repo" branch -q --no-track moving refs/remotes/origin/main
unmerged=$(git -C "$repo" commit-tree "$(git -C "$repo" rev-parse 'refs/remotes/origin/main^{tree}')" \
  -p refs/remotes/origin/main -m "feat: not merged")
mkdir "$tmp/fake"
cat >"$tmp/fake/git" <<'SH'
#!/bin/sh
case " $* " in
  *" update-ref -d refs/heads/moving "*) "$REAL_GIT" -C "$MOVE_IN" update-ref refs/heads/moving "$MOVE_TO" ;;
esac
exec "$REAL_GIT" "$@"
SH
chmod +x "$tmp/fake/git"
expect_fail "a branch that moves between the check and the delete" "the branch moving moved while it was being removed; it was kept" \
  env PATH="$tmp/fake:$PATH" REAL_GIT="$(command -v git)" MOVE_IN="$repo" MOVE_TO="$unmerged" \
  bash -c 'cd "$1" && scripts/worktree.sh remove moving' - "$repo"
[ "$(git -C "$repo" rev-parse refs/heads/moving)" = "$unmerged" ] || fail "remove: the branch that moved is kept, at its new commit"
git -C "$repo" branch -q -D moving

# --- add leaves nothing behind when git cannot make the folder ---------------------------

before=$(state)
chmod a-w "$w"
if (: >"$w/probe") 2>/dev/null; then
  chmod u+w "$w"
  rm "$w/probe"
  echo "skip: add where the folder cannot be made; this user can write anywhere"
else
  status=0
  wt add s10 >"$tmp/out" 2>&1 || status=$?
  chmod u+w "$w"
  [ "$status" -ne 0 ] || fail "add where the folder cannot be made: expected failure"
  grep -q -F "the branch s10 it had made was deleted again, so nothing was created" "$tmp/out" ||
    { cat "$tmp/out" >&2; fail "add where the folder cannot be made: says the branch was deleted again"; }
  unchanged "$before" "add where the folder cannot be made"
  expect_pass "the worktrees are still in order" git -C "$repo" worktree list
  expect_pass "add once the folder can be made" wt add s10
  expect_pass "remove it again" wt remove s10
fi

# --- remove names a .env, and deletes the rest of what git ignores -----------------------

expect_pass "add a worktree to leave files in" wt add s11
mkdir -p "$w/whizbang-s11/sub" "$w/whizbang-s11/target"
echo "KEY=a-secret-value" >"$w/whizbang-s11/.env"
echo "KEY=a-secret-value" >"$w/whizbang-s11/sub/.env.local"
echo built >"$w/whizbang-s11/target/out"
before=$(state)
expect_fail "remove a worktree that holds a .env" "move or delete that file yourself" wt remove s11
grep -q -x -F "worktree:   .env" "$tmp/out" || fail "remove: names the .env"
grep -q -x -F "worktree:   sub/.env.local" "$tmp/out" || fail "remove: names the .env.local in a folder"
if grep -q -F "a-secret-value" "$tmp/out"; then fail "remove: printed what a .env holds"; fi
unchanged "$before" "remove a worktree that holds a .env"
[ -f "$w/whizbang-s11/.env" ] || fail "remove: the .env is still there"
rm "$w/whizbang-s11/.env"
expect_fail "remove a worktree that holds a .env.local" "move or delete that file yourself" wt remove s11
rm "$w/whizbang-s11/sub/.env.local"
# In another letter case: where the file system ignores case, git ignores it as it does
# a .env, and it may hold the same keys.
echo "KEY=a-secret-value" >"$w/whizbang-s11/sub/.Env.Local"
expect_fail "remove a worktree that holds a .Env.Local" "move or delete that file yourself" wt remove s11
grep -q -x -F "worktree:   sub/.Env.Local" "$tmp/out" || fail "remove: names the .Env.Local"
[ -f "$w/whizbang-s11/sub/.Env.Local" ] || fail "remove: the .Env.Local is still there"
rm "$w/whizbang-s11/sub/.Env.Local"
expect_pass "remove a worktree that holds only build output" wt remove s11
grep -q -F "with what git ignores there" "$tmp/out" || fail "remove: says that ignored files went too"
[ ! -e "$w/whizbang-s11" ] || fail "remove: the worktree and its build output are gone"

finished
