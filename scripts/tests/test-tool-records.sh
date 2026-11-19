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
# The records of plan m3, tasks D1 and D2 (both pull requests of each): each states rungs 1
# and 2 in its table under "Mark and history", and the fast tests hold its examples and
# numbers (crates/whizbang-rascal/tests/it/tool_records/). `fill` and `segment` are
# model-based: their rung 2 is task B1's, so their records state rung 1 alone.
for tool in close subtract line rect stroke paste fill mask_rect protect segment select plug feather render \
  rasterize shrink invert inspect cut put window unwindow tighten fan profile ramp apply measure; do
  record="docs/reference/tools/$tool.md"
  [ -f "$record" ] || fail "the record of $tool is missing"
  history=$(sed -n '/^## Mark and history$/,$p' "$record")
  printf '%s\n' "$history" | grep -E '^\| 1\. Single examples \| Shown \|' >/dev/null ||
    fail "the record of $tool does not show rung 1"
  case $tool in fill | segment) continue ;; esac
  printf '%s\n' "$history" | grep -E '^\| 2\. Scored alone \| Shown \|' >/dev/null ||
    fail "the record of $tool does not show rung 2"
done
# The records of the five use cases' recipes (plan m3, task Q1, item 5): rung 1 shown, rung 2
# credited from milestone 2's evaluation of `check`, and each unverified. The fast tests hold
# their examples (crates/whizbang-squint/tests/it/use_case_records.rs).
for use_case in source-fidelity reference-consistency product-fidelity brief-compliance reality-checks; do
  record="docs/reference/tools/$use_case.md"
  [ -f "$record" ] || fail "the record of the use case $use_case is missing"
  history=$(sed -n '/^## Mark and history$/,$p' "$record")
  printf '%s\n' "$history" | grep -E '^\| 1\. Single examples \| Shown \|' >/dev/null ||
    fail "the record of $use_case does not show rung 1"
  printf '%s\n' "$history" | grep -E '^\| 2\. Scored alone \| Credited \|' >/dev/null ||
    fail "the record of $use_case does not credit rung 2"
  printf '%s\n' "$history" | grep -E '^\*\*Unverified\*\*' >/dev/null ||
    fail "the record of $use_case is not marked unverified"
done
# The records of the five recipes that end in a fill (plan m3, task B1, item 2): rung 1
# shown, and rung 2 shown, or in part while its visual quality waits on the owner's picks.
# The fast tests hold their examples (crates/whizbang-rascal/tests/it/tool_records/recipes.rs)
# and their rung 2 to the edits' record and the shipped recipes
# (`evidence::each_model_based_recipes_record_holds_rungs_1_and_2`).
for recipe in remove remove_prompt repair repair_area seam; do
  record="docs/reference/tools/$recipe.md"
  [ -f "$record" ] || fail "the record of the recipe $recipe is missing"
  history=$(sed -n '/^## Mark and history$/,$p' "$record")
  printf '%s\n' "$history" | grep -E '^\| 1\. Single examples \| Shown \|' >/dev/null ||
    fail "the record of $recipe does not show rung 1"
  printf '%s\n' "$history" | grep -E '^\| 2\. Scored alone \| (Shown|In part) \|' >/dev/null ||
    fail "the record of $recipe does not show rung 2"
done
for record in docs/reference/tools/*.md; do
  wrong=$(shape "$record")
  [ -z "$wrong" ] || fail "$wrong"
done

finished
