#!/usr/bin/env bash
# The scans every push must pass (SPEC.md section 10): secrets and private data, attribution,
# and the clean-room scan.
#
#   scripts/scan.sh                        the working tree: tracked and untracked files that
#                                          are not ignored, and their paths
#   scripts/scan.sh --range <base> <head>  every commit in base..head, and the tree of head
#   scripts/scan.sh --pre-push <remote> <url>
#                                          what a push would publish; reads the lines git
#                                          gives a pre-push hook on stdin
#   scripts/scan.sh --message <file>       the attribution check alone, on one message file
#                                          (the commit-msg hook)
#   --secrets-only                         with any of the first three: skip the clean-room
#                                          scan (CI: the word list is never there)
#
# For a range or a push, the material is: the files each commit changed, as they were in
# that commit (so a secret added in one commit and deleted in the next is found); each
# commit's changed paths and its message; the tree of each tip and its paths; the names of
# the pushed branches and tags; and the message of an annotated tag.
#
# For a push, what is outgoing is every commit reachable from a pushed tip and not from a
# ref the destination has now. The destination is asked (git ls-remote); remote-tracking
# refs and local branches are not trusted. If the destination cannot be asked, or a sha
# cannot be resolved here, the scan fails.
#
# Secrets scan: provider key shapes, private-key headers, absolute home paths, email
# addresses, and the values in a local .env if there is one (values of 8 characters or
# more; a hit names the variable, never the value). It prints where a match is, never the
# matching text.
#
# Attribution check: no commit or tag message has a Co-Authored-By or "Generated with/by"
# line, and no commit or tag is authored by an agent or a model (SPEC.md section 15).
#
# Clean-room scan: reads a private word list from the path in WHIZBANG_WORDLIST. It fails
# closed when the variable is unset, or the list is unreadable or empty. It prints counts
# only: never a word, never a matching line.
set -euo pipefail
umask 077

die() {
  echo "scan: $*" >&2
  exit 1
}
usage() {
  die "usage: scan.sh [--secrets-only] [--range <base> <head> | --pre-push <remote> <url> | --message <file>]"
}

patterns=(
  'sk-[A-Za-z0-9_-]{20,}'
  'AKIA[0-9A-Z]{16}'
  'gh[pousr]_[A-Za-z0-9]{20,}'
  'github_pat_[A-Za-z0-9_]{20,}'
  'AIza[0-9A-Za-z_-]{30,}'
  'xox[abprs]-[A-Za-z0-9-]{10,}'
  'hf_[A-Za-z0-9]{30,}'
  'r8_[A-Za-z0-9]{30,}'
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'
  '/Users/[A-Za-z0-9._-]+'
  '/home/[A-Za-z0-9._-]+/'
  '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
)
allow='noreply@|@example\.|git@github\.com'

# A line of a message that credits an agent or a model; and a name or address that is one.
attribution='^(co-authored-by|generated (with|by)|signed-off-by: *(claude|codex|gpt|gemini))|🤖'
agents='(^|[^a-z])(claude|codex|chatgpt|gpt|gemini|copilot|anthropic|openai)([^a-z]|$)'

secrets_only=0
mode=tree
while [ $# -gt 0 ]; do
  case "$1" in
    --secrets-only)
      secrets_only=1
      shift
      ;;
    --range)
      [ $# -ge 3 ] || usage
      mode=range base=$2 head=$3
      shift 3
      ;;
    --pre-push)
      [ $# -ge 3 ] || usage
      mode=push remote=$2 url=$3
      shift 3
      ;;
    --message)
      [ $# -ge 2 ] || usage
      mode=message message=$2
      shift 2
      ;;
    *) usage ;;
  esac
done

if [ "$mode" = message ]; then
  [ -r "$message" ] || die "cannot read the message file"
  if grep -i -E -q -e "$attribution" "$message"; then
    echo "commit-msg: remove the attribution line; commits here name no agent or model" >&2
    exit 1
  fi
  exit 0
fi

list=
if [ "$secrets_only" -eq 0 ]; then
  list="${WHIZBANG_WORDLIST:-}"
  if [ -z "$list" ]; then
    echo "clean-room scan: WHIZBANG_WORDLIST is not set; refusing to pass" >&2
    exit 1
  fi
  case "$list" in /*) ;; *) list="$PWD/$list" ;; esac
  if [ ! -f "$list" ] || [ ! -r "$list" ] || ! grep -q '[^[:space:]]' "$list"; then
    echo "clean-room scan: the word list is unreadable or empty; refusing to pass" >&2
    exit 1
  fi
fi

cd "$(git rev-parse --show-toplevel)"
top=$PWD
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
text="$work/text"
mkdir "$text" "$work/env"
: >"$work/secret-hits"
: >"$work/attribution-hits"

# The values of a local .env, one file per value, so a hit can name its variable.
env_count=0
if [ -f .env ]; then
  while IFS= read -r line || [ -n "$line" ]; do
    line=${line%$'\r'}
    line=${line#"${line%%[![:space:]]*}"}
    line=${line#export }
    case "$line" in '' | '#'* | [!A-Za-z_]*) continue ;; *=*) ;; *) continue ;; esac
    name=${line%%=*}
    value=${line#*=}
    name=${name%"${name##*[![:space:]]}"}
    case "$name" in *[!A-Za-z0-9_]*) continue ;; esac
    value=${value#"${value%%[![:space:]]*}"}
    value=${value%"${value##*[![:space:]]}"}
    case "$value" in
      \"*\") value=${value#\"} value=${value%\"} ;;
      \'*\') value=${value#\'} value=${value%\'} ;;
    esac
    [ "${#value}" -ge 8 ] || continue
    env_count=$((env_count + 1))
    printf '%s\n' "$name" >"$work/env/$env_count.name"
    printf '%s\n' "$value" >"$work/env/$env_count.value"
    printf '%s\n' "$value" >>"$work/env/all"
  done <.env
fi

# What `search` looks in: either files under a folder, or a commit (all of its tree, or
# some paths of it).
target=
target_dir=
target_items=()
look_in_files() { # <folder> <file...>
  target=files target_dir=$1
  shift
  target_items=("$@")
}
look_in_commit() { # <sha> [<path...>]: no paths means the whole tree
  target=$1 target_dir=
  shift
  target_items=()
  [ $# -eq 0 ] || target_items=("$@")
}

# search <grep options...>: writes the matching lines to $work/found, each prefixed with
# where it is. No match is fine; a grep that could not run is not.
search() {
  local rc=0
  if [ "$target" = files ]; then
    (cd "$target_dir" && grep -I -H -n "$@" -- "${target_items[@]}") >"$work/found" || rc=$?
  else
    git --literal-pathspecs grep -I -n "$@" "$target" -- ${target_items[@]+"${target_items[@]}"} >"$work/found" || rc=$?
  fi
  [ "$rc" -le 1 ] || die "a search failed (exit $rc); refusing to pass"
}

# Where a found line is, without its text: file:line, or commit:path:line.
where() {
  if [ "$target" = files ]; then cut -d: -f1,2; else cut -d: -f1-3; fi
}

secrets_here() {
  local args=() p n
  for p in "${patterns[@]}"; do args+=(-e "$p"); done
  search -E "${args[@]}"
  { grep -v -E -e "$allow" "$work/found" || true; } | where >>"$work/secret-hits"
  [ "$env_count" -gt 0 ] || return 0
  search -F -f "$work/env/all"
  [ -s "$work/found" ] || return 0
  n=1
  while [ "$n" -le "$env_count" ]; do
    search -F -f "$work/env/$n.value"
    where <"$work/found" | sed "s/\$/ (the value of $(cat "$work/env/$n.name") in .env)/" >>"$work/secret-hits"
    n=$((n + 1))
  done
}

words_contents=0 words_paths=0 words_messages=0 words_refs=0 words_tags=0
words_here() { # <kind>
  [ "$secrets_only" -eq 0 ] || return 0
  local n
  search -i -F -f <(grep -v '^[[:space:]]*$' "$list")
  n=$(wc -l <"$work/found" | tr -d ' ')
  case "$1" in
    contents) words_contents=$((words_contents + n)) ;;
    paths) words_paths=$((words_paths + n)) ;;
    messages) words_messages=$((words_messages + n)) ;;
    refs) words_refs=$((words_refs + n)) ;;
    tags) words_tags=$((words_tags + n)) ;;
  esac
}

zero() {
  case "$1" in *[!0]*) return 1 ;; esac
}

commits=0
scan_commit() { # <sha>
  local sha=$1 p paths=()
  commits=$((commits + 1))
  git log -1 --format=%B "$sha" >"$text/commit-message/$sha"
  if grep -i -E -q -e "$attribution" "$text/commit-message/$sha"; then
    echo "commit $sha: the message has an attribution line" >>"$work/attribution-hits"
  fi
  git log -1 --format='%an%n%ae%n%cn%n%ce' "$sha" >"$work/who"
  if grep -i -E -q -e "$agents" "$work/who"; then
    echo "commit $sha: the author or committer is an agent or a model" >>"$work/attribution-hits"
  fi
  # Against every parent, so a change made in a merge itself is covered.
  git -c core.quotepath=false diff-tree -r --root -m --no-commit-id --name-only -z "$sha" >"$work/paths"
  tr '\0' '\n' <"$work/paths" | sort -u >"$text/commit-paths/$sha"
  while IFS= read -r -d '' p; do paths+=("$p"); done <"$work/paths"
  [ "${#paths[@]}" -gt 0 ] || return 0
  # Too many paths for one command line: look in the commit's whole tree instead.
  [ "${#paths[@]}" -le 400 ] || paths=()
  look_in_commit "$sha" ${paths[@]+"${paths[@]}"}
  secrets_here
  words_here contents
}

trees=0
scan_tree() { # <sha>
  local sha=$1
  [ ! -e "$text/tree-paths/$sha" ] || return 0
  trees=$((trees + 1))
  git -c core.quotepath=false ls-tree -r --name-only "$sha" >"$text/tree-paths/$sha"
  look_in_commit "$sha"
  secrets_here
  words_here contents
}

tags=0
tip=
# Scans an annotated tag, and any tag it points at; leaves the commit it ends at in $tip.
scan_tag_chain() { # <sha>
  local obj=$1 kind
  while :; do
    kind=$(git cat-file -t "$obj" 2>/dev/null) || die "cannot resolve $obj here; refusing to pass"
    [ "$kind" = tag ] || break
    tags=$((tags + 1))
    git cat-file tag "$obj" >"$work/tag"
    sed '1,/^$/d' "$work/tag" >"$text/tag-message/$obj"
    sed -n '1,/^$/s/^tag //p' "$work/tag" >>"$text/ref-names/tags"
    if grep -i -E -q -e "$attribution" "$text/tag-message/$obj"; then
      echo "tag $obj: the message has an attribution line" >>"$work/attribution-hits"
    fi
    sed -n '1,/^$/s/^tagger //p' "$work/tag" >"$work/who"
    if grep -i -E -q -e "$agents" "$work/who"; then
      echo "tag $obj: the tagger is an agent or a model" >>"$work/attribution-hits"
    fi
    obj=$(sed -n '1,/^$/s/^object //p' "$work/tag" | head -n 1)
  done
  [ "$kind" = commit ] || die "$obj is a $kind, not a commit; the scan cannot cover it; refusing to pass"
  tip=$obj
}

case "$mode" in
  tree)
    files=()
    while IFS= read -r f; do
      [ -f "$f" ] && [ "$f" != .env ] && files+=("$f")
    done < <(git ls-files -co --exclude-standard)
    mkdir "$text/tree-paths"
    if [ "${#files[@]}" -gt 0 ]; then
      printf '%s\n' "${files[@]}" >"$text/tree-paths/working-tree"
      look_in_files "$top" "${files[@]}"
      secrets_here
      words_here contents
    fi
    covered="${#files[@]} files"
    ;;
  range | push)
    mkdir "$text/commit-message" "$text/commit-paths" "$text/tree-paths" "$text/ref-names" "$text/tag-message"
    tips=()
    : >"$work/revs"
    if [ "$mode" = range ]; then
      base_sha=$(git rev-parse --verify --quiet "$base^{commit}") ||
        die "cannot establish the range: the base is not a commit here; refusing to pass"
      head_sha=$(git rev-parse --verify --quiet "$head^{commit}") ||
        die "cannot establish the range: the head is not a commit here; refusing to pass"
      tips+=("$head_sha")
      echo "$head_sha" >>"$work/revs"
      echo "^$base_sha" >>"$work/revs"
    else
      # What the destination has now, from the destination itself.
      git ls-remote "$url" >"$work/remote-refs" ||
        die "cannot establish what the remote '$remote' has; refusing to pass"
      while read -r local_ref local_sha remote_ref remote_sha; do
        [ -n "$local_ref" ] || continue
        if [ -z "$remote_sha" ] || [ -n "${local_sha//[0-9a-f]/}" ] || [ -n "${remote_sha//[0-9a-f]/}" ]; then
          die "cannot read the push: a line on stdin is not '<local ref> <local sha> <remote ref> <remote sha>'; refusing to pass"
        fi
        zero "$local_sha" && continue # a deletion publishes nothing
        if ! zero "$remote_sha"; then
          git cat-file -e "$remote_sha" 2>/dev/null ||
            die "cannot establish the range: what the remote has at $remote_ref is not known here (fetch first); refusing to pass"
          echo "$remote_sha" >>"$work/remote-refs"
        fi
        name=${remote_ref#refs/heads/}
        name=${name#refs/tags/}
        printf '%s\n' "$name" >>"$text/ref-names/pushed"
        scan_tag_chain "$local_sha"
        tips+=("$tip")
        echo "$tip" >>"$work/revs"
      done
      # Of what the remote has, only commits known here can bound the range.
      awk '{ print $1 "^{commit}" }' "$work/remote-refs" | git cat-file --batch-check |
        awk '$2 == "commit" { print "^" $1 }' | sort -u >>"$work/revs"
    fi
    if [ "${#tips[@]}" -gt 0 ]; then
      git rev-list --stdin <"$work/revs" >"$work/outgoing" ||
        die "cannot establish the range; refusing to pass"
      while read -r sha; do scan_commit "$sha"; done <"$work/outgoing"
      for sha in "${tips[@]}"; do scan_tree "$sha"; done
    fi
    covered="$commits commits, $trees trees, $tags tags"
    ;;
esac

# The text that is not file contents: paths, messages, names.
for kind in tree-paths commit-paths commit-message ref-names tag-message; do
  [ -d "$text/$kind" ] || continue
  items=()
  for f in "$text/$kind"/*; do [ -f "$f" ] && items+=("$kind/${f##*/}"); done
  [ "${#items[@]}" -gt 0 ] || continue
  look_in_files "$text" "${items[@]}"
  secrets_here
  case "$kind" in
    *-paths) words_here paths ;;
    commit-message) words_here messages ;;
    ref-names) words_here refs ;;
    tag-message) words_here tags ;;
  esac
done

status=0
if [ -s "$work/secret-hits" ]; then
  echo "secrets scan: found a match for a forbidden pattern in:" >&2
  sort -u "$work/secret-hits" >&2
  status=1
else
  echo "secrets scan: clean ($covered)"
fi

if [ "$mode" != tree ]; then
  if [ -s "$work/attribution-hits" ]; then
    echo "attribution check: commits and tags here name no agent or model:" >&2
    cat "$work/attribution-hits" >&2
    status=1
  else
    echo "attribution check: clean ($commits commits, $tags tags)"
  fi
fi

if [ "$secrets_only" -eq 0 ]; then
  count=$((words_contents + words_paths + words_messages + words_refs + words_tags))
  if [ "$mode" = tree ]; then
    echo "clean-room scan: $count matching lines (contents $words_contents, paths $words_paths)"
  else
    echo "clean-room scan: $count matching lines (contents $words_contents, paths $words_paths, messages $words_messages, ref names $words_refs, tag messages $words_tags)"
  fi
  [ "$count" -eq 0 ] || status=1
fi

exit "$status"
