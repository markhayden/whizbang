#!/usr/bin/env bash
# Reads a receipt of the local leg, `prerelease-receipt/1`, whole, by its schema
# (scripts/prerelease-receipt.jq, whose top describes the shape; plan m4, task R2, items 4
# and 5). The local leg checks what it wrote with it, and the release gate what it read.
#
#   scripts/receipt-check.sh [--shape] <receipt> [<name>=<value>...]
#
# It passes when the file is a receipt and every required check is there exactly once and
# meets the conditions of its kind. With `--shape` it passes on any file that is a
# receipt, whatever it records: a leg that failed still writes one. The values a receipt
# is held to, each optional: tag=, commit=, tree= (the tree of that commit, which the
# model tests' checkout must be of), mac=, linux= and installed= (the digests of the two
# archives and of the installed binary, taken again from the candidate's bytes).
#
# Every thing that is wrong is printed, one a line, and the exit status is 1; a file that
# is not JSON is one such thing. Exit status 2 is a usage error. Needs jq.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "receipt-check: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

usage() {
  echo "usage: scripts/receipt-check.sh [--shape] <receipt> [tag=<tag>] [commit=<commit>] [tree=<tree>] [mac=<sha-256>] [linux=<sha-256>] [installed=<sha-256>]" >&2
  exit 2
}
shape=0
if [ "${1:-}" = --shape ]; then
  shape=1
  shift
fi
[ $# -ge 1 ] || usage
receipt=$1
shift
tag='' commit='' tree='' mac='' linux='' installed=''
for held in "$@"; do
  case "$held" in
    tag=?*) tag=${held#tag=} ;;
    commit=?*) commit=${held#commit=} ;;
    tree=?*) tree=${held#tree=} ;;
    mac=?*) mac=${held#mac=} ;;
    linux=?*) linux=${held#linux=} ;;
    installed=?*) installed=${held#installed=} ;;
    *) usage ;;
  esac
done
command -v jq >/dev/null || { echo "receipt-check: jq is not installed" >&2; exit 1; }
[ -f "$receipt" ] && [ ! -L "$receipt" ] || { echo "receipt-check: there is no receipt at $receipt" >&2; exit 1; }
# A receipt is small: one that is not is no receipt, and is not handed to jq.
[ "$(wc -c <"$receipt" | tr -d ' ')" -le 1048576 ] || { echo "receipt-check: shape: the file is larger than a receipt can be" >&2; exit 1; }

# One value and nothing after it: `jq -s` reads every value of the file into one list.
wrong=$(jq -r -s --arg tag "$tag" --arg commit "$commit" --arg tree "$tree" --arg mac "$mac" --arg linux "$linux" --arg installed "$installed" \
  --argjson shape_only "$shape" -f "$root/scripts/prerelease-receipt.jq" "$receipt" 2>/dev/null) || { echo "receipt-check: shape: the file is not JSON, or could not be read by its schema" >&2; exit 1; }
if [ -n "$wrong" ]; then
  printf '%s\n' "$wrong" | sed 's/^/receipt-check: /' >&2
  exit 1
fi
echo "receipt-check: ok: $receipt"
}
main "$@"; exit
