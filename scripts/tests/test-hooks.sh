#!/usr/bin/env bash
# The hooks, as `just setup` installs them: each one parses scripts/scan.sh and the hooks
# before it does anything, and refuses the commit or the push when one does not parse. The
# scan runs only here before a push, and the bash that ships with macOS (3.2) exits 0 from
# a script that stopped at a syntax error after setting an `EXIT` trap; without the check a
# broken scan would let a push through.
source "$(dirname "$0")/lib.sh"

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

# A stand-in for cargo, so the format check of pre-commit passes anywhere.
mkdir "$tmp/bin"
# It and a stand-in for just also answer with the pinned versions, so that setup finds the
# tools in order and its status here is about the hooks alone.
pin() { awk -v tool="$1" '$1 == tool { print $2 }' "$root/scripts/tool-versions.txt"; }
printf '#!/bin/sh\ncase "$1" in\n  nextest) echo "cargo-nextest %s" ;;\n  deny) echo "cargo-deny %s" ;;\nesac\nexit 0\n' \
  "$(pin cargo-nextest)" "$(pin cargo-deny)" >"$tmp/bin/cargo"
printf '#!/bin/sh\necho "just %s"\n' "$(pin just)" >"$tmp/bin/just"
chmod +x "$tmp/bin/cargo" "$tmp/bin/just"
export PATH="$tmp/bin:$PATH"

# A throwaway repository with this repo's hooks and setup script, installed by the setup
# script itself. Its remote is a bare repository.
remote="$tmp/remote.git"
repo="$tmp/repo"
git init -q --bare -b main "$remote"
git init -q -b main "$repo"
cd "$repo"
git config user.name "A Tester"
git config user.email "tester@example.com"
git remote add origin "$remote"
mkdir -p scripts
cp -R "$root/scripts/hooks" scripts/hooks
cp "$root/scripts/setup.sh" "$root/scripts/hook-dispatch.sh" "$root/scripts/tool-versions.txt" scripts/
printf 'scripts/\n' >.git/info/exclude
# A clone that an earlier setup pointed at scripts/hooks by a path relative to wherever git
# is run: setup takes that setting away, because a push from another folder then ran no hook.
git config core.hooksPath scripts/hooks
expect_pass "setup, with the hooks and the tools in order" scripts/setup.sh
[ -z "$(git config --get core.hooksPath || true)" ] || fail "setup leaves no core.hooksPath"
# What it installs: the same small script under each hook's name in the repository's own
# hooks folder, where git looks from any folder and from every worktree.
for hook in pre-commit commit-msg pre-push; do
  [ -x ".git/hooks/$hook" ] || fail "setup installs .git/hooks/$hook, and it can be run"
  cmp -s "$root/scripts/hook-dispatch.sh" ".git/hooks/$hook" || fail ".git/hooks/$hook is scripts/hook-dispatch.sh"
done
# A setting that sends git elsewhere for its hooks, which setup did not make, is left and
# is a problem: git would not look in the hooks folder at all.
git config core.hooksPath "$tmp/other-hooks"
expect_fail "setup with core.hooksPath set to another folder" "hook problem: core.hooksPath is set" scripts/setup.sh
quiet_path() { grep -q -F -e "$tmp" "$tmp/out" && { cat "$tmp/out" >&2; fail "$1: no path of this machine is printed"; }; return 0; }
quiet_path "setup with core.hooksPath set"
git config --unset core.hooksPath
expect_pass "setup once the setting is gone" scripts/setup.sh
for hook in pre-commit commit-msg pre-push; do
  [ -x "scripts/hooks/$hook" ] || fail "scripts/hooks/$hook can be run"
  # Git skips a hook it cannot execute, so the three are committed as files that can be.
  [ "$(git -C "$root" ls-files -s "scripts/hooks/$hook" | cut -d' ' -f1)" = 100755 ] ||
    fail "scripts/hooks/$hook is tracked with mode 100755"
done
# Setup says so, and fails, when a hook has lost that. With the hooks in order it passes,
# so the failures below are the hook's.
expect_pass "setup --check, with the hooks in order" scripts/setup.sh --check
for hook in pre-commit commit-msg pre-push; do
  grep -q -x -F "hook ok: $hook" "$tmp/out" || { cat "$tmp/out" >&2; fail "setup finds $hook in order"; }
done
grep -q -F "hook problem" "$tmp/out" && fail "setup finds no problem with the hooks"
chmod -x scripts/hooks/pre-push
expect_fail "setup --check with a hook that cannot be executed" \
  "hook problem: scripts/hooks/pre-push is missing or cannot be executed, so git would skip it" scripts/setup.sh --check
expect_fail "setup with a hook that cannot be executed" "hook problem: scripts/hooks/pre-push" scripts/setup.sh
chmod +x scripts/hooks/pre-push
rm scripts/hooks/commit-msg
expect_fail "setup --check with a hook that is missing" "hook problem: scripts/hooks/commit-msg" scripts/setup.sh --check
cp "$root/scripts/hooks/commit-msg" scripts/hooks/

# Two stand-ins for the scan. The sound one records that it ran. The broken one sets a
# trap, would record that it ran, and then has a syntax error: bash 3.2 exits 0 from it.
sound_scan() {
  printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' "echo \"\$*\" >>\"$tmp/scan-ran\"" >scripts/scan.sh
  chmod +x scripts/scan.sh
}
broken_scan() {
  printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' "trap 'true' EXIT" \
    "echo \"\$*\" >>\"$tmp/scan-ran\"" 'if true; then' '  echo "inside"' 'done' >scripts/scan.sh
  chmod +x scripts/scan.sh
}
# only <hook...>: the hooks in place are these, as this repo has them. Each of the others is
# a stand-in that passes and says nothing, so that what follows is the work of the hooks
# named: a hook that is not there at all is refused where it would be run (below).
only() {
  rm -rf scripts/hooks
  mkdir scripts/hooks
  local hook
  for hook in pre-commit commit-msg pre-push; do
    printf '%s\n' '#!/bin/sh' 'exit 0' >"scripts/hooks/$hook"
    chmod +x "scripts/hooks/$hook"
  done
  for hook in "$@"; do cp "$root/scripts/hooks/$hook" scripts/hooks/; done
}
change() {
  echo "$1" >>file.txt
  git add file.txt
}
refuses="has a syntax error; refusing"

# With a sound scan, a commit and a push go through, and the scan was run for each.
sound_scan
change one
expect_pass "a commit with a sound scan" git commit -m "feat: one"
expect_pass "a push with a sound scan" git push origin main
grep -q -e '^--message ' "$tmp/scan-ran" || fail "commit-msg runs the scan on the message"
grep -q -e '^--pre-push origin ' "$tmp/scan-ran" || fail "pre-push runs the scan on the push"
[ "$(git ls-remote "$remote" refs/heads/main | cut -f1)" = "$(git rev-parse HEAD)" ] || fail "the push arrived"
published=$(git rev-parse HEAD)

# With a broken scan, each hook alone refuses, says why, and the scan is not started.
broken_scan
for hook in pre-commit commit-msg; do
  only "$hook"
  : >"$tmp/scan-ran"
  change "$hook"
  expect_fail "a commit with a broken scan, $hook alone" "$hook: scripts/scan.sh $refuses the commit" \
    git commit -m "feat: with $hook"
  [ "$(git rev-parse HEAD)" = "$published" ] || fail "$hook: no commit was made"
  [ ! -s "$tmp/scan-ran" ] || fail "$hook: no line of a broken scan runs"
done
only pre-push
git commit -q -m "feat: to push"
: >"$tmp/scan-ran"
expect_fail "a push with a broken scan" "pre-push: scripts/scan.sh $refuses the push" git push origin main
[ "$(git ls-remote "$remote" refs/heads/main | cut -f1)" = "$published" ] || fail "pre-push: nothing reached the remote"
[ ! -s "$tmp/scan-ran" ] || fail "pre-push: no line of a broken scan runs"

# A hook that does not parse stops the others too: a broken pre-push is found at the commit.
sound_scan
only pre-commit commit-msg pre-push
printf '\nif true; then\n' >>scripts/hooks/pre-push
change again
before=$(git rev-parse HEAD)
expect_fail "a commit with a broken pre-push hook" "pre-commit: scripts/hooks/pre-push $refuses the commit" \
  git commit -m "feat: again"
only commit-msg pre-push
printf '\nif true; then\n' >>scripts/hooks/pre-push
expect_fail "the same, with no pre-commit hook" "commit-msg: scripts/hooks/pre-push $refuses the commit" \
  git commit -m "feat: again"
[ "$(git rev-parse HEAD)" = "$before" ] || fail "a broken pre-push: no commit was made"
expect_fail "a push with a broken pre-push hook" "syntax error" git push origin main
[ "$(git ls-remote "$remote" refs/heads/main | cut -f1)" = "$published" ] || fail "a broken pre-push: nothing reached the remote"

# Put right, everything goes through again.
only pre-commit commit-msg pre-push
expect_pass "a commit once the hooks are sound" git commit -m "feat: again"
expect_pass "a push once the hooks are sound" git push origin main
[ "$(git ls-remote "$remote" refs/heads/main | cut -f1)" = "$(git rev-parse HEAD)" ] || fail "the last push arrived"
published=$(git rev-parse HEAD)

# The real scan, with a fault that a parse does not find on the line after its trap: an
# unset variable, which makes bash 3.2 exit 0, and a here-document with no end, which
# makes any bash exit 0 with the rest of the scan unread. The trap of the scan turns that
# into a failure, so the commit and the push are refused.
for fault in 'echo "$no_such_variable"' "cat <<'NEVER_ENDS' >/dev/null"; do
  FAULT="$fault" awk '{ print } /^trap .* EXIT$/ && !put { print ENVIRON["FAULT"]; put = 1 }' \
    "$root/scripts/scan.sh" >scripts/scan.sh
  grep -q -F -e "$fault" scripts/scan.sh || fail "the scan has an EXIT trap to put a fault after"
  chmod +x scripts/scan.sh
  bash -n scripts/scan.sh 2>/dev/null || fail "a parse does not find the fault: $fault"
  # Any bash exits 0 from the here-document, so the scan's trap speaks. At the unset
  # variable bash 3 exits 0 too; a later bash fails by itself. Both name the variable.
  said="scan: stopped before its end; failing"
  case "$fault" in echo*) said="no_such_variable" ;; esac
  only commit-msg
  change "$fault"
  expect_fail "a commit with a scan that stops at: $fault" "$said" git commit -m "feat: a fault"
  [ "$(git rev-parse HEAD)" = "$published" ] || fail "no commit was made with a scan that stops at: $fault"
  only pre-push
  git commit -q -m "feat: to push past a fault"
  expect_fail "a push with a scan that stops at: $fault" "$said" git push origin main
  [ "$(git ls-remote "$remote" refs/heads/main | cut -f1)" = "$published" ] ||
    fail "nothing reached the remote with a scan that stops at: $fault"
  git reset -q --hard "$published"
done

# A scan that is missing, a folder or unreadable is said to be that, by every hook.
for hook in pre-commit commit-msg pre-push; do
  only "$hook"
  what=commit
  [ "$hook" != pre-push ] || what=push
  try() {
    if [ "$hook" = pre-push ]; then git push origin main; else git commit -m "feat: no scan"; fi
  }
  change "no scan, $hook"
  [ "$hook" != pre-push ] || git commit -q --no-verify -m "feat: to push with no scan"
  rm -rf scripts/scan.sh
  expect_fail "$hook with no scan" "$hook: scripts/scan.sh is missing or cannot be read; refusing the $what" try
  mkdir scripts/scan.sh
  expect_fail "$hook with a folder for a scan" "$hook: scripts/scan.sh is missing or cannot be read; refusing the $what" try
  rmdir scripts/scan.sh
  sound_scan
  chmod 000 scripts/scan.sh
  if [ -r scripts/scan.sh ]; then
    echo "skip: an unreadable scan; this user can read a file with mode 000"
  else
    expect_fail "$hook with an unreadable scan" "$hook: scripts/scan.sh is missing or cannot be read; refusing the $what" try
  fi
  rm -f scripts/scan.sh
  [ "$(git ls-remote "$remote" refs/heads/main | cut -f1)" = "$published" ] || fail "$hook: nothing reached the remote"
  git reset -q --hard "$published"
done

# What is in the hooks folder and is not a hook (a folder, a file git would not run) is
# not parsed, and does not stop a commit or a push.
sound_scan
only pre-commit commit-msg pre-push
mkdir scripts/hooks/notes
printf 'not a script: fi ) done (\n' >scripts/hooks/README
printf 'if true; then\n' >scripts/hooks/notes/draft.sh
change "strays"
expect_pass "a commit with strays in the hooks folder" git commit -m "feat: strays"
expect_pass "a push with strays in the hooks folder" git push origin main
[ "$(git ls-remote "$remote" refs/heads/main | cut -f1)" = "$(git rev-parse HEAD)" ] || fail "the push with strays arrived"

# --- A push made around the folder: git run from another folder, with the repository and
# the working tree named in the environment or by options. The hooks are found all the
# same, because they are in the repository's own hooks folder: the push is scanned by the
# scan of the working tree it names, or refused when none is named. With the real scan.
cp "$root/scripts/scan.sh" scripts/scan.sh
only pre-commit commit-msg pre-push
rm -rf scripts/hooks/notes scripts/hooks/README
printf 'zorblaxquint\n' >"$tmp/words"
export WHIZBANG_WORDLIST="$tmp/words"
elsewhere="$tmp/elsewhere"
mkdir "$elsewhere"
key="sk""-abcdefghij0123456789ABCDEFGHIJ"
on_remote() { git ls-remote "$remote" "refs/heads/$1" | cut -f1; }
# around <way> <git arguments...>: git, run from the other folder, told where the
# repository and its working tree are in one of three ways.
around() {
  local way=$1
  shift
  case "$way" in
    environment) (cd "$elsewhere" && GIT_DIR="$repo/.git" GIT_WORK_TREE="$repo" git "$@") ;;
    options) (cd "$elsewhere" && git --git-dir="$repo/.git" --work-tree="$repo" "$@") ;;
    repository-alone) (cd "$elsewhere" && GIT_DIR="$repo/.git" git "$@") ;;
  esac
}
git checkout -q -b around-key
printf 'token = %s\n' "$key" >config.txt
git add config.txt
git commit -q --no-verify -m "feat: a config"
with_key=$(git rev-parse HEAD)
git checkout -q -b around-word main
echo "it says zorblaxquint" >words.txt
git add words.txt
git commit -q --no-verify -m "feat: words"
git checkout -q -b around-clean main
echo "nothing" >clean.txt
git add clean.txt
git commit -q --no-verify -m "feat: clean"
git checkout -q -b zorblaxquint-branch around-clean
git checkout -q main
for way in environment options; do
  expect_fail "a push around the folder ($way), with a key" "$with_key:config.txt:1" around "$way" push origin around-key
  [ -z "$(on_remote around-key)" ] || fail "a push around the folder ($way): the key did not reach the remote"
  grep -q -F -e "$key" "$tmp/out" && fail "a push around the folder ($way): the key is not printed"
  expect_fail "a push around the folder ($way), with a listed word" "clean-room scan: 2 matching lines" around "$way" push origin around-word
  [ -z "$(on_remote around-word)" ] || fail "a push around the folder ($way): the word did not reach the remote"
  expect_fail "a push around the folder ($way), of a branch named with a listed word" "ref names 1" \
    around "$way" push origin zorblaxquint-branch
  [ -z "$(on_remote zorblaxquint-branch)" ] || fail "a push around the folder ($way): the branch name did not reach the remote"
  expect_fail "a push around the folder ($way), with no word list" "WHIZBANG_WORDLIST is not set" \
    env -u WHIZBANG_WORDLIST bash -c "cd '$elsewhere' && GIT_DIR='$repo/.git' GIT_WORK_TREE='$repo' git push origin around-clean:no-list-$way"
  [ -z "$(on_remote "no-list-$way")" ] || fail "a push around the folder ($way), with no word list: nothing reached the remote"
  # A clean one is scanned and arrives.
  expect_pass "a clean push around the folder ($way)" around "$way" push origin "around-clean:clean-$way"
  grep -q -F "secrets scan: clean (1 commits" "$tmp/out" || { cat "$tmp/out" >&2; fail "a clean push around the folder ($way) is scanned"; }
  [ "$(on_remote "clean-$way")" = "$(git rev-parse around-clean)" ] || fail "a clean push around the folder ($way) arrived"
  git push -q origin ":clean-$way" >/dev/null 2>&1
done
# With the repository named and no working tree, git takes the folder it is in for the
# working tree. There is no scan there, so nothing can be scanned: refused, clean or not.
for branch in around-key around-clean; do
  expect_fail "a push with the repository named alone ($branch)" "pre-push: no scripts/hooks/pre-push here that can be run" \
    around repository-alone push origin "$branch"
  [ -z "$(on_remote "$branch")" ] || fail "a push with the repository named alone: $branch did not reach the remote"
  # The hook names no folder; git's own line names the remote, as it always does.
  grep -e '^pre-push:' "$tmp/out" | grep -F -e "$tmp" >/dev/null && fail "the refusal names no path of this machine"
done
# A commit made the same way goes through the commit hooks of the working tree.
printf 'feat: a thing\n\n%s: A Person <person@example.com>\n' "Co""-Authored-By" >"$tmp/message"
echo "more" >>file.txt
expect_fail "a commit around the folder, with a credit" "has a Co""-Authored-By line" \
  around environment commit -q -a -F "$tmp/message"
git checkout -q -- file.txt
# A linked worktree runs the hooks of its own branch, found from its own folder.
git checkout -q main
git worktree add -q "$tmp/linked" -b linked main
mkdir -p "$tmp/linked/scripts"
cp -R scripts/hooks "$tmp/linked/scripts/hooks"
cp scripts/scan.sh "$tmp/linked/scripts/scan.sh"
(
  cd "$tmp/linked"
  printf 'token = %s\n' "$key" >config.txt
  git add config.txt
  git commit -q --no-verify -m "feat: a config"
  expect_fail "a push from a linked worktree, with a key" "config.txt:1" git push origin linked
  printf '%s\n' '#!/usr/bin/env bash' 'echo "the hook of the linked worktree ran" >&2' 'exit 1' >scripts/hooks/pre-push
  expect_fail "a linked worktree runs its own hook" "the hook of the linked worktree ran" git push origin linked
)
[ -z "$(on_remote linked)" ] || fail "a push from a linked worktree: the key did not reach the remote"
# A working tree that has lost its hook: refused, not passed.
rm "$tmp/linked/scripts/hooks/pre-push"
(cd "$tmp/linked" && expect_fail "a push from a working tree with no pre-push hook" \
  "pre-push: no scripts/hooks/pre-push here that can be run" git push origin linked)
[ -z "$(on_remote linked)" ] || fail "a push from a working tree with no hook: nothing reached the remote"

finished
