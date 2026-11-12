#!/usr/bin/env bash
# The generated reference (SPEC.md section 14, "Documentation that cannot drift"):
# docs/reference/description.json is `whizbang describe --json --pretty` byte for byte, and
# docs/reference/actions.md is rendered from that JSON alone.
#
#   scripts/docs.sh                       write both files (`just docs`)
#   scripts/docs.sh --check               fail when the committed copies differ from a fresh
#                                         generation, or the folder holds anything else but
#                                         tools/ and what git ignores (`just docs-check`,
#                                         part of `just check`)
#   scripts/docs.sh --description FILE    a saved description instead of running the binary
#   scripts/docs.sh --out DIR             another folder instead of docs/reference
#
# The last two exist for scripts/tests/test-docs.sh. Needs jq.
#
# The page prints names, the version and purposes as they are, so they must be text that
# cannot be read as Markdown or HTML; anything else is refused and nothing is written:
#
#   description_version   a whole number from 0 to 4294967295
#   the build's name      a lower-case letter, then lower-case letters, digits, `_` or `-`;
#   and an action's name  64 characters at most; no two actions share a name
#   the version           three dot-separated numbers, then optionally `-` or `+` and
#                         letters, digits, `.`, `-`, `+`; 64 characters at most
#   an action's purpose   one line of 1 to 200 characters that starts with a letter, does
#                         not end with a space, holds only letters and digits (ASCII),
#                         spaces and . , ; : ' " ( ) / % + - and holds neither `://` nor
#                         `www.` (in any case), which some renderers turn into a link
#
# Nothing those rules allow can start a heading, a list, a table, a link, a code span, a
# tag, a comment or a new line, so a description cannot forge a section of the page.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "docs: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
set -euo pipefail

check=0
description=""
out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --check) check=1; shift ;;
    --description) description=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    *) echo "docs: unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Paths given by the caller are relative to where the caller stands.
case "$description" in "" | /*) ;; *) description=$PWD/$description ;; esac
case "$out" in "" | /*) ;; *) out=$PWD/$out ;; esac
cd "$(git rev-parse --show-toplevel)"
[ -n "$out" ] || out=docs/reference
command -v jq >/dev/null || { echo "docs: jq is not installed" >&2; exit 2; }

tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "docs: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

if [ -n "$description" ]; then
  [ -f "$description" ] || { echo "docs: no such description: $description" >&2; exit 2; }
  cp "$description" "$tmp/description.json"
elif ! cargo run --quiet -p whizbang -- describe --json --pretty >"$tmp/description.json"; then
  echo "docs: could not run \`whizbang describe --json --pretty\`" >&2
  exit 1
fi

# The page is a function of the JSON and nothing else: no date, no machine, no path. Actions
# stay in the description's order, which is the order a door lists them in. Each action is
# one section; a field added to an action appears on the page only when a line is added for
# it here, and appears in description.json at once.
#
# The whole input is read as a list of documents, so none and several are both seen. No
# regular expressions: a jq built without them must give the same answer.
if ! jq -r -s '
  def lower: "abcdefghijklmnopqrstuvwxyz";
  def upper: "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
  def digits: "0123456789";
  # only($allowed): a string made of nothing but the characters of $allowed.
  def only($allowed): ($allowed | explode) as $ok | explode | all(. as $c | $ok | index($c) != null);
  def slug: type == "string" and length >= 1 and length <= 64
    and (.[0:1] | only(lower)) and only(lower + digits + "_-");
  def number_text: length >= 1 and only(digits);
  def version: type == "string" and length <= 64 and only(lower + upper + digits + ".+-")
    and ((split("+")[0] // "") | (split("-")[0] // "") | split(".")
         | length == 3 and all(number_text));
  def purpose: type == "string" and length >= 1 and length <= 200
    and (.[0:1] | only(lower + upper)) and .[-1:] != " "
    and only(lower + upper + digits + " .,;:\"()/%+-" + ([39] | implode))  # 39 is the apostrophe
    and (contains("://") | not) and (ascii_downcase | contains("www.") | not);
  def need(ok; $what): if ok then . else error($what) end;

  need(length == 1; "expected exactly one JSON document, found \(length)")
  | .[0]
  | need(type == "object"; "expected an object")
  | need((.description_version | type) == "number" and .description_version >= 0
         and .description_version <= 4294967295
         and .description_version == (.description_version | floor);
         "description_version must be a whole number from 0 to 4294967295")
  | need(.name | slug; "name must be a lower-case letter, then lower-case letters, digits, _ or -")
  | need(.version | version; "version must be three dot-separated numbers, with an optional suffix")
  | need((.actions | type) == "array"; "actions must be a list")
  | reduce (.actions | to_entries[]) as $a (.;
      need(($a.value | type) == "object" and ($a.value.name | slug);
           "action \($a.key + 1): its name must be a lower-case letter, then lower-case letters, digits, _ or -")
      | need($a.value.purpose | purpose;
             "action \($a.key + 1): its purpose must be one plain line (the rule is at the top of scripts/docs.sh)"))
  | (.actions | map(.name)) as $names
  | need(($names | unique | length) == ($names | length);
         "an action name is listed more than once: \($names | group_by(.) | map(select(length > 1)[0]) | join(", "))")
  # However the number was written (1, 1.0, 1E2), every jq prints it the same way.
  | (.description_version | floor | tostring) as $shape
  | [ "<!-- Generated by `just docs` from `whizbang describe --json`. Never edit it by hand. -->",
      "# Actions",
      "What `\(.name)` \(.version) can do, from its static description (version \($shape)).\nThe same as data: [description.json](description.json).",
      (if (.actions | length) == 0 then "This build registers no actions yet."
       else (.actions[] | "## `\(.name)`", .purpose) end) ]
  | join("\n\n")' "$tmp/description.json" >"$tmp/actions.md" 2>"$tmp/error"; then
  echo "docs: not a description: $(head -n 1 "$tmp/error")" >&2
  exit 1
fi
for f in description.json actions.md; do
  [ -s "$tmp/$f" ] || { echo "docs: not a description: the fresh $f is empty" >&2; exit 1; }
done

# The folder as the caller named it, for messages.
shown=${out#"$PWD"/}

if [ "$check" -eq 0 ]; then
  mkdir -p "$out"
  cp "$tmp/description.json" "$tmp/actions.md" "$out/"
  echo "docs: wrote $shown/description.json and $shown/actions.md"
  done_ok=1
  exit 0
fi

stale=0
for f in description.json actions.md; do
  if [ ! -f "$out/$f" ]; then
    echo "docs: $shown/$f is missing" >&2
    stale=1
  elif ! diff -u --label "$f (committed)" --label "$f (fresh)" "$out/$f" "$tmp/$f" >&2; then
    stale=1
  fi
done

# Nothing else lives here but the tool records (SPEC.md section 8), under tools/. Inside a
# repository the folder is listed as git sees it, so a file git ignores is not counted;
# elsewhere, everything in it is. Names are read up to a NUL, so one that holds a newline is
# still one name.
stray=0
if [ -d "$out" ]; then
  if git -C "$out" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    (cd "$out" && git ls-files -z --cached --others --exclude-standard) >"$tmp/listing"
  else
    (cd "$out" && find . -mindepth 1 -maxdepth 1 -print0) >"$tmp/listing"
  fi
  last=""
  while IFS= read -r -d '' entry; do
    entry=${entry#./}
    name=${entry%%/*}
    [ "$name" != "$last" ] || continue
    last=$name
    # A tracked file that has been deleted is listed and is not there.
    [ -e "$out/$name" ] || [ -L "$out/$name" ] || continue
    case "$name" in
      description.json | actions.md) ;;
      tools) [ -d "$out/tools" ] || { echo "docs: $shown/tools is not a folder" >&2; stray=1; } ;;
      *) echo "docs: $shown/$name is not generated" >&2; stray=1 ;;
    esac
  done <"$tmp/listing"
fi

if [ "$stale" -ne 0 ]; then
  echo "docs: $shown is stale: run \`just docs\` and commit the result" >&2
fi
if [ "$stray" -ne 0 ]; then
  echo "docs: $shown holds only description.json, actions.md and tools/: move or delete what is named above" >&2
fi
[ "$stale" -eq 0 ] && [ "$stray" -eq 0 ] || exit 1
echo "docs: ok ($shown matches a fresh generation)"
done_ok=1
