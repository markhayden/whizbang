#!/usr/bin/env bash
# Fails when the workspace has a crate edge that scripts/crate-graph.txt does not allow
# (SPEC.md section 3: "The edges are listed in one file and checked").
#
#   scripts/check-crate-graph.sh                    the workspace, from `cargo metadata`
#   scripts/check-crate-graph.sh --edges FILE       against another edge list
#   scripts/check-crate-graph.sh --metadata FILE    a saved `cargo metadata` document
#
# The two options exist for scripts/tests/test-crate-graph.sh. Needs jq.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

edges=scripts/crate-graph.txt
metadata=""
while [ $# -gt 0 ]; do
  case "$1" in
    --edges) edges=$2; shift 2 ;;
    --metadata) metadata=$2; shift 2 ;;
    *) echo "crate graph: unknown argument: $1" >&2; exit 2 ;;
  esac
done

command -v jq >/dev/null || { echo "crate graph: jq is not installed" >&2; exit 2; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The edge list becomes three files: the crates it names, the edges allowed for any kind of
# dependency, and the edges allowed for a development dependency. No associative arrays: the
# bash that ships with macOS is 3.2.
: >"$tmp/named"; : >"$tmp/normal"; : >"$tmp/dev"
bad=0
n=0
while IFS= read -r line || [ -n "$line" ]; do
  n=$((n + 1))
  line=${line%%#*}
  # shellcheck disable=SC2086  # word splitting is the parser
  set -- $line
  if [ $# -eq 0 ]; then
    continue
  elif [ $# -eq 1 ]; then
    echo "$1" >>"$tmp/named"
  elif [ $# -eq 3 ] && [ "$2" = "->" ]; then
    printf '%s\n%s\n' "$1" "$3" >>"$tmp/named"
    echo "$1 $3" >>"$tmp/normal"
    echo "$1 $3" >>"$tmp/dev"
  elif [ $# -eq 3 ] && [ "$2" = "-dev->" ]; then
    printf '%s\n%s\n' "$1" "$3" >>"$tmp/named"
    echo "$1 $3" >>"$tmp/dev"
  else
    echo "crate graph: cannot read $edges line $n: $line" >&2
    bad=1
  fi
done <"$edges"
[ "$bad" -eq 0 ] || exit 1

if [ -z "$metadata" ]; then
  metadata="$tmp/metadata.json"
  cargo metadata --format-version 1 --no-deps >"$metadata"
fi

# One line per workspace crate, and one per dependency of a workspace crate on another:
# "<from> <kind> <to>", kind being normal, build or dev. A dependency is matched by its
# package name, so a renamed one is still seen.
jq -r '.workspace_members as $ids
  | [.packages[] | select(.id as $id | $ids | index($id))] as $ws
  | ($ws | map(.name)) as $names
  | $ws[]
  | .name as $from
  | "crate \($from)",
    (.dependencies[]
      | select(.name as $to | $names | index($to))
      | select(.name != $from)
      | "edge \($from) \(.kind // "normal") \(.name)")' "$metadata" | sort -u >"$tmp/found"

status=0
crates=0
found=0
while read -r what a b c; do
  if [ "$what" = "crate" ]; then
    crates=$((crates + 1))
    if ! grep -q -x -F -e "$a" "$tmp/named"; then
      echo "crate graph: $a is not named in $edges" >&2
      status=1
    fi
    continue
  fi
  found=$((found + 1))
  if [ "$b" = "dev" ]; then
    allowed="$tmp/dev"; arrow="-dev->"
  else
    allowed="$tmp/normal"; arrow="->"
  fi
  if ! grep -q -x -F -e "$a $c" "$allowed"; then
    echo "crate graph: forbidden edge: $a $arrow $c ($b dependency)" >&2
    status=1
  fi
done <"$tmp/found"

if [ "$crates" -eq 0 ]; then
  echo "crate graph: found no workspace crates; refusing to pass" >&2
  exit 1
fi
if [ "$status" -ne 0 ]; then
  echo "crate graph: the allowed edges are in $edges (SPEC.md section 3)" >&2
  exit 1
fi
echo "crate graph: ok ($crates crates, $found edges between them, all listed in $edges)"
