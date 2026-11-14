#!/usr/bin/env bash
# `just setup`: installs the git hooks, then checks the pinned tools. After a change to
# scripts/hook-dispatch.sh it is run again: scripts/worktree.sh refuses to add a worktree
# while the installed copies differ from it. It is run only from a working tree that has
# scripts/hook-dispatch.sh: the setup of a branch cut before that file puts the old
# setting back, for every worktree of the clone.
#
#   scripts/setup.sh           install the hooks and check the tools
#   scripts/setup.sh --check   check the tools only
#
# A tool that is missing, or at another version than scripts/tool-versions.txt pins, is
# named with the command that installs the pinned version, and the script fails. jq and
# python3 (3.9 or later, for scripts/baseline) are checked without a pin. The hooks
# are installed either way. `just check` does not run this: only setup is strict about
# versions.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

status=0
if [ "${1:-}" != "--check" ]; then
  # The hooks go into the repository's own hooks folder, which every worktree shares and
  # where git looks from whatever folder it is run in: each is a copy of
  # scripts/hook-dispatch.sh, which runs scripts/hooks/<name> of the working tree in use.
  #
  # A copy is written beside its place and then moved into it, and a link at the place is
  # taken away first, so that a link is replaced and never written through: a link into
  # scripts/hooks would have the tracked hook overwritten, and a link to a folder would
  # have the copy put inside it. A link is never this repo's hook, whatever it leads to.
  # Whatever was there and was not this repo's is replaced, and that is said: a hook of
  # the user's own under one of the three names does not survive setup. An earlier copy
  # of this repo's hook, known by a line every version of scripts/hook-dispatch.sh has,
  # is updated and said to be.
  marker='# What `just setup` installs as each git hook: this file, copied under the hook'"'"'s name'
  hooks_dir=$(git rev-parse --git-common-dir)/hooks
  mkdir -p "$hooks_dir" 2>/dev/null || true
  installed=0
  for hook in pre-commit commit-msg pre-push; do
    to="$hooks_dir/$hook"
    was=nothing
    if [ -L "$to" ]; then
      was=other
    elif [ -e "$to" ]; then
      was=other
      if [ -f "$to" ]; then
        if cmp -s scripts/hook-dispatch.sh "$to"; then
          was=ours
        elif grep -q -x -F -e "$marker" "$to" 2>/dev/null; then
          was=older
        fi
      fi
    fi
    ok=1
    # A folder cannot be replaced by a file with a move: the copy would go inside it.
    if [ -d "$to" ] && [ ! -L "$to" ]; then
      ok=0
    else
      rm -f "$to.new" 2>/dev/null || true
      cp scripts/hook-dispatch.sh "$to.new" 2>/dev/null && chmod +x "$to.new" 2>/dev/null || ok=0
      if [ "$ok" = 1 ] && [ -L "$to" ]; then
        rm -f "$to" 2>/dev/null || ok=0
      fi
      [ "$ok" = 0 ] || mv -f "$to.new" "$to" 2>/dev/null || ok=0
      rm -f "$to.new" 2>/dev/null || true
    fi
    if [ "$ok" = 1 ] && [ -f "$to" ] && [ ! -L "$to" ] && [ -x "$to" ] && cmp -s scripts/hook-dispatch.sh "$to"; then
      installed=$((installed + 1))
      case "$was" in
        older) echo "setup: updated $hook in the repository's hooks folder (an earlier copy of this repo's hook was there)" ;;
        other) echo "setup: replaced what was at $hook in the repository's hooks folder, which was not this repo's hook (a hook of your own there is gone: this repo's hooks are scripts/hooks/<name>)" ;;
      esac
    else
      echo "hook problem: $hook could not be installed in the repository's hooks folder" >&2
      status=1
    fi
  done
  # An earlier setup pointed git at scripts/hooks instead, by a path that git reads from
  # the folder it is run in: a push from another folder then found no hook and ran none.
  # That setting is taken away last, and only when the three hooks are in place and git
  # would then look in the hooks folder, so that a setup that fails leaves the hooks the
  # clone had. Any other value of the setting, here or in the user's configuration, was
  # not made here and is left, and is a problem: git would not look in the hooks folder.
  old=$(git config --local --get core.hooksPath || true)
  if [ "$installed" = 3 ]; then
    if [ "$old" = scripts/hooks ]; then
      git config --local --unset-all core.hooksPath
      if [ -n "$(git config --get core.hooksPath || true)" ]; then
        # Without the old setting another one takes over, and no hook of this repo would
        # run at all: the old one is put back.
        git config --local core.hooksPath scripts/hooks
        echo "hook problem: core.hooksPath is also set outside this repo's configuration (the user's or the system's), and would send git away from the repository's hooks folder. The setting of an earlier setup is kept, so git still runs scripts/hooks from inside a working tree, and none for a push made from another folder. Remove the other setting (git config --show-origin --get-all core.hooksPath shows where it is) and run \`just setup\` again" >&2
        status=1
      else
        echo "hooks installed in the repository's hooks folder: pre-commit, commit-msg, pre-push (each runs scripts/hooks/<name> of the working tree in use)"
      fi
    elif [ -n "$(git config --get core.hooksPath || true)" ]; then
      echo "hook problem: core.hooksPath is set, in this repo's configuration or the user's, so git does not look in the repository's hooks folder and would run none of these hooks; remove the setting (git config --show-origin --get core.hooksPath shows where it is)" >&2
      status=1
    else
      echo "hooks installed in the repository's hooks folder: pre-commit, commit-msg, pre-push (each runs scripts/hooks/<name> of the working tree in use)"
    fi
  elif [ "$old" = scripts/hooks ]; then
    echo "hook problem: the hooks are NOT installed ($installed of 3 are in place). The setting of an earlier setup is left as it was, so git still runs scripts/hooks from inside a working tree, and none for a push made from another folder. Put the hooks folder right and run \`just setup\` again" >&2
  else
    echo "hook problem: the hooks are NOT installed ($installed of 3 are in place), so git runs no hook of this repo in this clone where one is missing: no scan before a push. Put the hooks folder right and run \`just setup\` again" >&2
  fi
  echo "set WHIZBANG_WORDLIST to the clean-room word list before your first push"
fi

# The installed version of a tool: the second word of the first line it prints.
installed() {
  case "$1" in
    just) just --version ;;
    cargo-*) cargo "${1#cargo-}" --version ;;
  esac 2>/dev/null | awk 'NR == 1 { print $2 }' || true
}

# Git skips a hook it cannot execute, without a word: a push would then run no scan.
for hook in pre-commit commit-msg pre-push; do
  if [ -f "scripts/hooks/$hook" ] && [ -x "scripts/hooks/$hook" ]; then
    echo "hook ok: $hook"
  else
    echo "hook problem: scripts/hooks/$hook is missing or cannot be executed, so git would skip it; restore it with: git checkout -- scripts/hooks/$hook && chmod +x scripts/hooks/$hook" >&2
    status=1
  fi
done
while read -r tool want; do
  case "$tool" in '' | '#'*) continue ;; esac
  have=$(installed "$tool")
  if [ "$have" = "$want" ]; then
    echo "tool ok: $tool $want"
  elif [ -z "$have" ]; then
    echo "tool problem: $tool is missing; install it with: cargo install $tool@$want --locked" >&2
    status=1
  else
    echo "tool problem: $tool is at another version: found $have, pinned $want; install the pinned one with: cargo install $tool@$want --locked" >&2
    status=1
  fi
done <scripts/tool-versions.txt

if command -v jq >/dev/null; then
  echo "tool ok: jq (any version)"
else
  echo "tool problem: jq is missing; install it with your package manager (brew install jq, apt-get install jq)" >&2
  status=1
fi

# python3 runs scripts/baseline and its tests. Any version from 3.9 on will do.
py=$(python3 -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>/dev/null || true)
case "$py" in
  [0-9]*.[0-9]*)
    if [ "${py%%.*}" -gt 3 ] || { [ "${py%%.*}" -eq 3 ] && [ "${py#*.}" -ge 9 ]; }; then
      echo "tool ok: python3 $py (3.9 or later)"
    else
      echo "tool problem: python3 is too old: found $py, and scripts/baseline needs 3.9 or later; install a newer one with your package manager" >&2
      status=1
    fi
    ;;
  *)
    echo "tool problem: python3 is missing; scripts/baseline needs 3.9 or later; install it with your package manager (brew install python, apt-get install python3)" >&2
    status=1
    ;;
esac
exit "$status"
}
main "$@"; exit
