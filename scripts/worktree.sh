#!/usr/bin/env bash
# One task, one worktree, one branch (AGENTS.md, "Where agents may work").
#
#   scripts/worktree.sh add <slug>      fetch origin/main, then make the worktree
#                                       ../whizbang-<slug> (beside the main checkout) on a
#                                       new branch <slug> that starts there
#   scripts/worktree.sh remove <slug>   remove that worktree and delete the local branch,
#                                       only if the branch is merged into origin/main
#   scripts/worktree.sh list            the worktrees of this repo
#
# A slug is lower-case letters, digits and hyphens, 64 characters at most, and does not
# start with a hyphen. Nothing else is accepted, so a slug cannot be read as an option or
# lead to another folder.
#
# `add` refuses when the hooks are not installed (`just setup`), or when the folder or the
# branch is already there; it creates nothing when it refuses. The new branch follows no
# upstream: push it by name.
#
# `remove` has no way to force it. It refuses, and removes nothing, when the branch is not
# merged into a freshly fetched origin/main, when the branch is checked out anywhere but
# its own worktree, when it is run from inside that worktree, or when the worktree holds
# changes or files that git does not ignore.
#
# It must be run from inside the repo it belongs to (any of its worktrees). It sets no
# `EXIT` trap, so its own syntax error is a failure.
set -euo pipefail
export LC_ALL=C

die() {
  echo "worktree: $*" >&2
  exit 1
}
usage() {
  die "usage: worktree.sh add <slug> | remove <slug> | list"
}

# common_dir <folder>: the real path of the git folder that every worktree of the repo
# holding <folder> shares. Fails when <folder> is not in a git checkout.
common_dir() {
  (
    cd "$1" 2>/dev/null || exit 1
    dir=$(git rev-parse --git-common-dir 2>/dev/null) || exit 1
    cd "$dir" 2>/dev/null || exit 1
    pwd -P
  )
}

own=$(common_dir "$(dirname "$0")") || die "this script is not inside a git checkout"
here=$(common_dir .) || die "run it from inside the repo it belongs to"
[ "$own" = "$here" ] || die "run it from inside the repo it belongs to"
case "$own" in
  */.git) ;;
  *) die "the repo has no main checkout" ;;
esac
main=${own%/.git}
parent=$(dirname "$main")

# check_slug <slug>: refuses anything but a plain task name.
check_slug() {
  case "$1" in
    '' | -* | *[!a-z0-9-]*)
      die "a slug is lower-case letters, digits and hyphens, and does not start with a hyphen; nothing was changed"
      ;;
  esac
  [ "${#1}" -le 64 ] || die "a slug is 64 characters at most; nothing was changed"
  [ "$1" != main ] || die "the slug main is not a task; nothing was changed"
}

fetch_main() {
  git -C "$main" fetch --quiet origin '+refs/heads/main:refs/remotes/origin/main' ||
    die "cannot fetch origin/main; nothing was changed"
}

[ $# -ge 1 ] || usage
command=$1
shift

case "$command" in
  list)
    [ $# -eq 0 ] || usage
    git -C "$main" worktree list
    ;;

  add)
    [ $# -eq 1 ] || usage
    slug=$1
    check_slug "$slug"
    path="$parent/whizbang-$slug"

    [ "$(git -C "$main" config --get core.hooksPath || true)" = scripts/hooks ] ||
      die "the hooks are not installed: run \`just setup\`, then add the worktree; nothing was created"
    if [ -e "$path" ] || [ -L "$path" ]; then
      die "$path is already there; nothing was created"
    fi
    if git -C "$main" show-ref --verify --quiet "refs/heads/$slug"; then
      die "the branch $slug is already there; nothing was created"
    fi
    fetch_main

    git -C "$main" worktree add --quiet --no-track -b "$slug" -- "$path" refs/remotes/origin/main
    echo "worktree: $path on the new branch $slug, at origin/main $(git -C "$path" rev-parse HEAD)"

    # The setup check. The hooks are found by a path relative to the worktree, so a worktree
    # runs the hooks of its own branch.
    for hook in pre-commit commit-msg pre-push; do
      [ -x "$path/scripts/hooks/$hook" ] ||
        die "the worktree was made, but its hook scripts/hooks/$hook is missing or cannot run"
    done
    echo "worktree: hooks ok (scripts/hooks: pre-commit, commit-msg, pre-push)"
    if [ -n "${WHIZBANG_WORDLIST:-}" ]; then
      echo "worktree: WHIZBANG_WORDLIST is set in this shell"
    else
      echo "worktree: WHIZBANG_WORDLIST is not set: set it to the private word list the owner provides before your first push"
    fi
    ;;

  remove)
    [ $# -eq 1 ] || usage
    slug=$1
    check_slug "$slug"
    path="$parent/whizbang-$slug"

    sha=$(git -C "$main" rev-parse --verify --quiet "refs/heads/$slug^{commit}") ||
      die "there is no local branch $slug; nothing was removed"
    fetch_main
    git -C "$main" merge-base --is-ancestor "$sha" refs/remotes/origin/main ||
      die "the branch $slug is not merged into origin/main; nothing was removed"

    # Where the branch is checked out: its own worktree, or nowhere.
    where=$(git -C "$main" worktree list --porcelain |
      awk -v branch="branch refs/heads/$slug" '/^worktree / { w = substr($0, 10) } $0 == branch { print w }')
    has_worktree=0
    if [ -n "$where" ]; then
      [ -d "$where" ] && [ -d "$path" ] && [ "$(cd "$where" && pwd -P)" = "$(cd "$path" && pwd -P)" ] ||
        die "the branch $slug is checked out in $where, not in $path; nothing was removed"
      has_worktree=1
    elif [ -e "$path" ] || [ -L "$path" ]; then
      die "$path is not a worktree on the branch $slug; nothing was removed"
    fi

    if [ "$has_worktree" -eq 1 ]; then
      case "$(pwd -P)/" in
        "$(cd "$path" && pwd -P)"/*) die "run it from another worktree, not the one being removed; nothing was removed" ;;
      esac
      git -C "$main" worktree remove -- "$path" ||
        die "git would not remove $path (see above); nothing was removed"
      echo "worktree: removed $path"
    fi

    # The branch is deleted only if it still points at the commit that was checked.
    git -C "$main" update-ref -d "refs/heads/$slug" "$sha" ||
      die "the branch $slug moved while it was being removed; it was kept"
    git -C "$main" config --remove-section "branch.$slug" 2>/dev/null || true
    echo "worktree: deleted the local branch $slug (was $sha, merged into origin/main)"
    ;;

  *) usage ;;
esac
