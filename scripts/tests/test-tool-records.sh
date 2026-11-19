#!/usr/bin/env bash
# The tool records (SPEC.md section 8, "Tool records"): every tool that ships has its record at
# docs/reference/tools/<name>.md, with the seven parts the spec lists, in its order, and every
# picture a record shows is beside it. The fast tests hold each record's images and numbers
# to its tool; this holds the records' shape.
source "$(dirname "$0")/lib.sh"

parts="## What it does
## Examples
## Rung 2 results
## Speed and cost
## Alternatives
## Limits
## Mark and history"

# shape <record>: prints what is wrong with the record, nothing when nothing is.
shape() {
  local record=$1 dir got
  dir=$(dirname "$record")
  got=$(grep -E '^## ' "$record" || true)
  [ "$got" = "$parts" ] || echo "$record: its parts are not the seven of SPEC.md section 8, in order"
  [ "$({ grep -o -E '!\[[^]]*\]\([^)]+\.png\)' "$record" || true; } | wc -l | tr -d ' ')" -ge 2 ] ||
    echo "$record: fewer than two pictures in its examples"
  { grep -o -E '!\[[^]]*\]\([^)]+\)' "$record" || true; } | sed -E 's/.*\(([^)]+)\)/\1/' |
    while IFS= read -r picture; do
      [ -f "$dir/$picture" ] || echo "$record: $picture is not beside it"
    done
}

# The check itself, on records written here: one whole, one with a part missing and a
# picture that is not there.
mkdir -p "$tmp/tools/good" "$tmp/tools/bad"
: >"$tmp/tools/good/a.png"
: >"$tmp/tools/good/b.png"
{ printf '# good\n'; printf '%s\n' "$parts" | while IFS= read -r part; do printf '%s\n\nText.\n\n' "$part"; done
  printf '![a](good/a.png) ![b](good/b.png)\n'; } >"$tmp/tools/good.md"
[ -z "$(shape "$tmp/tools/good.md")" ] || fail "a whole record is refused: $(shape "$tmp/tools/good.md")"
{ printf '# bad\n## What it does\n## Examples\n![a](bad/a.png) ![b](bad/b.png)\n'; } >"$tmp/tools/bad.md"
said=$(shape "$tmp/tools/bad.md")
case "$said" in *"not the seven"*) ;; *) fail "a record with parts missing is not refused" ;; esac
case "$said" in *"bad/a.png is not beside it"*) ;; *) fail "a picture that is not there is not reported" ;; esac

# The records there are: each tool that ships has one, and each is whole.
for tool in view grow resize align change_map edge_similarity; do
  [ -f "docs/reference/tools/$tool.md" ] || fail "the record of $tool is missing"
done
for record in docs/reference/tools/*.md; do
  wrong=$(shape "$record")
  [ -z "$wrong" ] || fail "$wrong"
done

finished
