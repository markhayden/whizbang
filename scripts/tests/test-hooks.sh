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
printf '#!/bin/sh\nexit 0\n' >"$tmp/bin/cargo"
chmod +x "$tmp/bin/cargo"
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
cp "$root/scripts/setup.sh" "$root/scripts/tool-versions.txt" scripts/
printf 'scripts/\n' >.git/info/exclude
scripts/setup.sh >/dev/null 2>&1 || true # the tools it checks are not this test's subject
[ "$(git config core.hooksPath)" = "scripts/hooks" ] || fail "setup installs the hooks from scripts/hooks"
for hook in pre-commit commit-msg pre-push; do
  [ -x "scripts/hooks/$hook" ] || fail "scripts/hooks/$hook can be run"
  # Git skips a hook it cannot execute, so the three are committed as files that can be.
  [ "$(git -C "$root" ls-files -s "scripts/hooks/$hook" | cut -d' ' -f1)" = 100755 ] ||
    fail "scripts/hooks/$hook is tracked with mode 100755"
done
# Setup says so, and fails, when a hook has lost that (the tools it also checks are not
# this test's subject: their lines are not looked at).
scripts/setup.sh --check >"$tmp/out" 2>&1 || true
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
# only <hook...>: the hooks in place are these, as this repo has them.
only() {
  rm -rf scripts/hooks
  mkdir scripts/hooks
  local hook
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

finished
