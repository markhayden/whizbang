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
done

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

finished
