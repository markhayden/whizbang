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
for tool in view grow resize align change_map edge_similarity per_subject tiles; do
  [ -f "docs/reference/tools/$tool.md" ] || fail "the record of $tool is missing"
done
# The three tools scored on rung 2 in milestone 2 (plan 2C, task X1, items 7 to 9): each
# record's part on rung 2 holds its numbers, the area under the ROC curve among them, which
# the fast tests hold to the tree (`evidence::the_per_subject_record_names_a_commit_of_this_tree_and_its_rung_2_numbers`).
for tool in per_subject change_map edge_similarity; do
  sed -n '/^## Rung 2 results$/,/^## Speed and cost$/p' "docs/reference/tools/$tool.md" |
    grep -E '^auc = [0-9]' >/dev/null || fail "the record of $tool holds no rung 2 numbers"
done
# The records of plan m3, task D1: each states rungs 1 and 2 in its table under "Mark and
# history", and the fast tests hold its examples and numbers
# (crates/whizbang-rascal/tests/it/tool_records/). `fill` is model-based: its rung 2 is
# task B1's, so its record states rung 1 alone.
for tool in close subtract line rect stroke paste fill mask_rect protect; do
  record="docs/reference/tools/$tool.md"
  [ -f "$record" ] || fail "the record of $tool is missing"
  history=$(sed -n '/^## Mark and history$/,$p' "$record")
  printf '%s\n' "$history" | grep -E '^\| 1\. Single examples \| Shown \|' >/dev/null ||
    fail "the record of $tool does not show rung 1"
  [ "$tool" = fill ] && continue
  printf '%s\n' "$history" | grep -E '^\| 2\. Scored alone \| Shown \|' >/dev/null ||
    fail "the record of $tool does not show rung 2"
done
for record in docs/reference/tools/*.md; do
  wrong=$(shape "$record")
  [ -z "$wrong" ] || fail "$wrong"
done

finished
