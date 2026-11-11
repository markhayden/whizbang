#!/usr/bin/env bash
# The two scans every push must pass (SPEC.md section 10).
#
#   scripts/scan.sh                 secrets scan, then clean-room scan, over tracked files
#   scripts/scan.sh --secrets-only  secrets scan only (CI: the word list is never there)
#
# The clean-room scan reads a private word list from the path in WHIZBANG_WORDLIST. It fails
# closed when the variable is unset, or the list is unreadable or empty. It prints counts
# only: never a word, never a matching line.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

secrets_only=0
[ "${1:-}" = "--secrets-only" ] && secrets_only=1

# No mapfile: the bash that ships with macOS is 3.2.
files=()
while IFS= read -r f; do
  [ -f "$f" ] && files+=("$f")
done < <(git ls-files -co --exclude-standard | grep -v '^scripts/scan\.sh$' || true)
status=0

patterns=(
  'sk-[A-Za-z0-9_-]{20,}'
  'AKIA[0-9A-Z]{16}'
  'gh[pousr]_[A-Za-z0-9]{20,}'
  'AIza[0-9A-Za-z_-]{30,}'
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'
  '/Users/[A-Za-z0-9._-]+'
  '/home/[A-Za-z0-9._-]+/'
  '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
)
allow='noreply@|@example\.|git@github\.com'

if [ "${#files[@]}" -gt 0 ]; then
  for pattern in "${patterns[@]}"; do
    hits=$(grep -I -n -E -e "$pattern" -- "${files[@]}" 2>/dev/null | grep -v -E "$allow" || true)
    if [ -n "$hits" ]; then
      echo "secrets scan: found a match for a forbidden pattern in:" >&2
      echo "$hits" | cut -d: -f1,2 | sort -u >&2
      status=1
    fi
  done
fi
[ "$status" -eq 0 ] && echo "secrets scan: clean (${#files[@]} files)"

if [ "$secrets_only" -eq 0 ]; then
  list="${WHIZBANG_WORDLIST:-}"
  if [ -z "$list" ]; then
    echo "clean-room scan: WHIZBANG_WORDLIST is not set; refusing to pass" >&2
    exit 1
  fi
  if [ ! -r "$list" ] || ! grep -q '[^[:space:]]' "$list"; then
    echo "clean-room scan: the word list is unreadable or empty; refusing to pass" >&2
    exit 1
  fi
  count=0
  if [ "${#files[@]}" -gt 0 ]; then
    # grep exits 1 when nothing matches, which is the passing case.
    count=$({ grep -I -i -F -f <(grep -v '^[[:space:]]*$' "$list") -- "${files[@]}" 2>/dev/null || true; } | wc -l | tr -d ' ')
  fi
  echo "clean-room scan: $count matching lines"
  [ "$count" -eq 0 ] || status=1
fi

exit "$status"
