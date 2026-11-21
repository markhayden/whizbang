#!/usr/bin/env bash
# The script tests that the local `just check` runs: those a change of this branch can
# reach, and not the others (`just test-scripts-local`). CI runs every script test whatever
# changed (`just ci`, `just ci-scripts`), and scripts/tests/test-ci-jobs.sh holds it to that.
#
#   scripts/script-tests-local.sh             choose, print one line saying what and why, run
#   scripts/script-tests-local.sh --dry-run   choose and print, with a second line naming
#                                             what would run, and run nothing
#                                             (scripts/tests/test-script-tests-local.sh)
#
# A change is any path that differs between the merge-base of HEAD and origin/main and the
# working tree (the branch's commits, what is staged and what is not), and any file git
# does not track and does not ignore. A moved file counts on both sides. origin/main is the
# copy this clone last fetched; one that is behind only makes the change larger.
#
# - WHIZBANG_SCRIPT_TESTS=1: all of them, whatever changed. Unset or empty: as below.
#   Anything else is refused.
# - No merge-base with origin/main to compare with, or git fails: all of them.
# - A change to a path that some script test reads, or that is a script test's subject:
#   all of them. Those are the `whole` paths below.
# - Any other change (a crate's sources, README.md): the script tests that read every tracked
#   file or the crates' sources (`part` below), and not the rest.
# - No change: none.
#
# Who reads what was found by reading each test for the files of the repo it opens (not of
# its throwaway copies). When it was unclear whether a test reads a path, the path is in.
# - test-binary-start, test-sign-in and test-thread-lint check every `.rs` under crates/;
#   test-notes looks there for each test a note cites; test-compile-fail reads the example
#   in the core's ledger.rs; test-baseline reads the eval crate's baseline.rs; test-deny
#   copies the crates' manifests and asks which of their source files exist; test-pack
#   holds the uv rules over every tracked file. These are the part.
# - scripts/, the justfile, .github/, .config/, any folder named .claude and any .mcp.json
#   (test-rules, at any depth and in any letter case), clippy.toml, deny.toml, AGENTS.md,
#   CLAUDE.md, SPEC.md, the workspace's Cargo.toml, Cargo.lock and rust-toolchain.toml,
#   each crate's Cargo.toml and build.rs (test-deny, test-features, test-crate-graph),
#   .gitignore, pack/ and docs/ (the plan, the notes, the reference, the baseline, the
#   carry-over, the sources), and ui/ (test-ui builds a copy of it, test-e2e reads its
#   Playwright configuration, test-third-party-licences its licences). These are whole. A path git prints in quotes, for a character
#   it will not print bare, is taken as whole too.
# A new script test that reads the crates' sources joins `part`.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C

part="scripts/tests/test-baseline.sh scripts/tests/test-binary-start.sh scripts/tests/test-compile-fail.sh scripts/tests/test-deny.sh scripts/tests/test-notes.sh scripts/tests/test-pack.sh scripts/tests/test-sign-in.sh scripts/tests/test-thread-lint.sh"
listed="scripts/, justfile, .github/, .config/, .claude/, .mcp.json, clippy.toml, deny.toml, AGENTS.md, CLAUDE.md, SPEC.md, Cargo.toml, Cargo.lock, rust-toolchain.toml, a crate's Cargo.toml or build.rs, .gitignore, pack/, docs/ or ui/"

dry=0
case "$*" in
  '') ;;
  --dry-run) dry=1 ;;
  *) echo "usage: scripts/script-tests-local.sh [--dry-run]" >&2; exit 2 ;;
esac
case "${WHIZBANG_SCRIPT_TESTS:-}" in
  '' | 1) ;;
  *) echo "script tests: WHIZBANG_SCRIPT_TESTS must be 1 or unset (it is: ${WHIZBANG_SCRIPT_TESTS})" >&2; exit 2 ;;
esac
cd "$(git rev-parse --show-toplevel)"

# run <line> <what: all, part or none>: print the line, then run that or say what it would.
run() {
  echo "script tests: $1"
  case "$2" in
    all) [ "$dry" = 1 ] && { echo "would run: scripts/tests/run.sh"; exit 0; }
      exec scripts/tests/run.sh ;;
    part) [ "$dry" = 1 ] && { echo "would run: scripts/tests/run.sh --only $part"; exit 0; }
      # shellcheck disable=SC2086 # the names are split on purpose; none holds a space.
      exec scripts/tests/run.sh --only $part ;;
    none) [ "$dry" = 1 ] && echo "would run: nothing"
      exit 0 ;;
  esac
}

# whole <path>: true when a change to the path runs every script test.
whole() {
  local lower
  case "$1" in
    '"'* | scripts/* | justfile | .github/* | .config/* | clippy.toml | deny.toml | AGENTS.md | CLAUDE.md \
      | SPEC.md | Cargo.toml | Cargo.lock | rust-toolchain.toml | .gitignore | pack/* | docs/* | ui/* \
      | crates/*/Cargo.toml | crates/*/build.rs) return 0 ;;
  esac
  lower=$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')
  case "/$lower/" in */.claude/* | */.mcp.json/*) return 0 ;; esac
  return 1
}

[ "${WHIZBANG_SCRIPT_TESTS:-}" != 1 ] || run "ran all of them: WHIZBANG_SCRIPT_TESTS=1" all
base=$(git merge-base origin/main HEAD 2>/dev/null) ||
  run "ran all of them: no merge-base with origin/main to compare with" all
changed=$(git diff --name-only --no-renames "$base" -- && git ls-files --others --exclude-standard) ||
  run "ran all of them: git could not list what changed since the merge-base with origin/main" all
[ -n "$changed" ] || run "skipped: no change since the merge-base with origin/main" none
while IFS= read -r path; do
  [ -n "$path" ] || continue
  ! whole "$path" || run "ran all of them: $path changed since the merge-base with origin/main" all
done <<<"$(printf '%s\n' "$changed" | sort -u)"
first=$(printf '%s\n' "$changed" | sort -u | sed -n '/./{p;q;}')
run "ran the $(echo "$part" | wc -w | tr -d ' ') that read every tracked file or the crates' sources ($first changed); skipped the rest: no change under $listed" part
}
main "$@"; exit
