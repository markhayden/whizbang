#!/usr/bin/env bash
# Fails when a shell script has a syntax error, and names it.
#
#   scripts/syntax.sh          every shell script under scripts/
#   scripts/syntax.sh DIR      every shell script under another folder
#
# A shell script is a file whose name ends in `.sh`, or whose first line is a `#!` line
# that names bash or sh (the hooks have no extension). The files are found, not listed, so
# a new script is covered from the moment it exists.
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

checked=0
broken=0
while IFS= read -r -d '' f; do
  case "$f" in
    *.sh) ;;
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
done < <(find "$dir" -type f -print0)

if [ "$broken" -gt 0 ]; then
  echo "syntax: $broken of $checked shell scripts under $dir cannot be parsed" >&2
  exit 1
fi
[ "$checked" -gt 0 ] || { echo "syntax: no shell script under $dir" >&2; exit 1; }
echo "syntax: ok ($checked shell scripts under $dir)"
