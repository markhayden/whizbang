#!/usr/bin/env bash
# What `just setup` installs as each git hook: this file, copied under the hook's name
# (pre-commit, commit-msg, pre-push) into the repository's own hooks folder. Git looks
# there from any folder and from every worktree, so a hook is found however git was
# started: also when it is run from another folder with GIT_DIR and GIT_WORK_TREE set, or
# with --git-dir and --work-tree, where a hooks folder named by a relative path was not
# found and nothing ran.
#
# It runs the hook of the same name in the working tree git is acting on,
# scripts/hooks/<name>, so a worktree runs the hooks of its own branch. When that working
# tree has no such hook that can be run, it refuses: with the repository named and no
# working tree, git takes the folder it is in for one, and nothing there can be scanned.
# The folder is not named in the refusal; a path of this machine is not for a log.
#
# The working tree git is told to act on is trusted: its scripts/hooks/<name> is what
# runs, and its scripts/scan.sh is what scans. Whoever names another working tree chooses
# the hook, as whoever edits a hook in their own does.
#
# It sets no `EXIT` trap, so its own syntax error is a failure.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
name=${0##*/}
top=$(git rev-parse --show-toplevel 2>/dev/null) ||
  { echo "$name: git cannot tell which working tree this is, so its hooks cannot be run; refusing. Run git from inside the working tree" >&2; exit 1; }
hook="$top/scripts/hooks/$name"
[ -f "$hook" ] && [ -x "$hook" ] ||
  { echo "$name: no scripts/hooks/$name here that can be run (the working tree git is acting on has none); refusing. Run git from inside the working tree of this repo, or name it with GIT_WORK_TREE as well as GIT_DIR" >&2; exit 1; }
exec "$hook" "$@"
}
main "$@"; exit
