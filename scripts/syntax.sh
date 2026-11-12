#!/usr/bin/env bash
# Fails when a shell script has a syntax error, and names it.
#
#   scripts/syntax.sh          every shell script under scripts/
#   scripts/syntax.sh DIR      every shell script under another folder
#
# A shell script is a file whose name ends in `.sh` or `.bash`, or whose first line is a
# `#!` line that names bash or sh (the hooks have no extension); a link to such a file counts.
# The files are found, not listed, so a new script is covered from the moment it exists. A
# file that cannot be read fails the check, since it cannot be told what it is.
#
# Why: the bash that ships with macOS (3.2) runs a script up to its first syntax error and
# then exits 0 if the script has set an `EXIT` trap. A broken scan would pass, and a test
# file that stopped half-way would be reported as passing. `scripts/tests/run.sh` runs
# this before any test. This script sets no trap, so its own syntax error is a failure.
set -uo pipefail

dir=${1:-}
if [ -z "$dir" ]; then
  cd "$(git rev-parse --show-toplevel)" || exit 2
  dir=scripts
fi
[ -d "$dir" ] || { echo "syntax: no such folder: $dir" >&2; exit 2; }

# A folder whose name starts with `-` is not an option to find.
case "$dir" in -*) dir="./$dir" ;; esac

checked=0
broken=0
while IFS= read -r -d '' f; do
  # A link counts when it leads to a file; a link to a folder or to nothing is not a script.
  [ -f "$f" ] || continue
  if [ ! -r "$f" ]; then
    echo "syntax: $f cannot be read, so it cannot be checked" >&2
    checked=$((checked + 1))
    broken=$((broken + 1))
    continue
  fi
  case "$f" in
    *.sh | *.bash) ;;
    *)
      first=
      IFS= read -r first <"$f" || true
      case "$first" in
        '#!'*/bash | '#!'*/bash' '* | '#!'*' bash' | '#!'*' bash '*) ;;
        '#!'*/sh | '#!'*/sh' '* | '#!'*' sh' | '#!'*' sh '*) ;;
        *) continue ;;
      esac
      ;;
  esac
  checked=$((checked + 1))
  if ! "$BASH" -n "$f"; then
    echo "syntax: $f has a syntax error" >&2
    broken=$((broken + 1))
  fi
done < <(find "$dir" \( -type f -o -type l \) -print0)

if [ "$broken" -gt 0 ]; then
  echo "syntax: $broken of $checked shell scripts under $dir cannot be parsed or read" >&2
  exit 1
fi
[ "$checked" -gt 0 ] || { echo "syntax: no shell script under $dir" >&2; exit 1; }
echo "syntax: ok ($checked shell scripts under $dir)"
