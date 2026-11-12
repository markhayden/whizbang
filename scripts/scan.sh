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
# Every file is searched as bytes: nothing is skipped for looking binary or for what
# .gitattributes says, and a file git takes for binary is searched a second time with its
# NUL bytes removed, which is what makes UTF-16 text readable. A symbolic link is never
# followed: the text of its target is searched instead.
#
# Secrets scan: provider key shapes, private-key headers, absolute home paths, email
# addresses, and the values in a local .env if there is one (values of 8 characters or
# more; a hit names the variable, never the value). It prints where a match is, never the
# matching text, and not the path either when the path itself holds a match.
#
# Attribution check: no commit or tag message has a Co-Authored-By line, for a person or
# an agent, or a line that credits an agent or a model; no commit or tag is authored by
# one (SPEC.md section 15).
#
# Clean-room scan: reads a private word list from the path in WHIZBANG_WORDLIST. It fails
# closed when the variable is unset, or the list is unreadable or empty. It prints counts
# only: never a word, never a matching line.
#
# The first line below parses this whole file before anything runs. The bash that ships
# with macOS (3.2) runs a script up to its first syntax error and then, if the script has
# set an `EXIT` trap, exits 0: without this line a broken scan would pass. Every script
# here that sets such a trap starts the same way (scripts/tests/test-syntax.sh holds them
# to it).
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "scan: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
set -euo pipefail
umask 077
# Bytes, not characters: a byte that is not valid UTF-8 must not hide the rest of its line.
export LC_ALL=C

die() {
  echo "scan: $*" >&2
  exit 1
}
usage() {
  die "usage: scan.sh [--secrets-only] [--range <base> <head> | --pre-push <remote> <url> | --message <file>]"
}

# Shapes where any matching line is a hit.
patterns=(
  'AKIA[0-9A-Z]{16}'
  'gh[pousr]_[A-Za-z0-9]{20,}'
  'github_pat_[A-Za-z0-9_]{20,}'
  'glpat-[A-Za-z0-9_-]{20,}'
  'AIza[0-9A-Za-z_-]{30,}'
  'xox[abprs]-[A-Za-z0-9-]{10,}'
  'xai-[A-Za-z0-9]{20,}'
  'hf_[A-Za-z0-9]{30,}'
  'r8_[A-Za-z0-9]{30,}'
  '[sr]k_(live|test)_[A-Za-z0-9]{10,}'
  '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}:[0-9a-f]{32,}'
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'
  '/Users/[A-Za-z0-9._-]+'
  '/home/[A-Za-z0-9._-]+'
)
# The sk- shape is looked at match by match: it must start a word, and its body must hold
# a digit or a capital, or start with a known prefix, so ordinary kebab-case is not a key.
sk_shape='(^|[^A-Za-z0-9_-])sk-[A-Za-z0-9_-]{20,}'
sk_body='sk-(ant|proj|or|svcacct|admin)-|sk-[a-z_-]*[0-9A-Z]'
# An address is a hit unless the address itself is an allowed one. Nothing else is allowed.
email='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
allow='noreply@|@example\.|git@github\.com'

# Attribution. A name counts wherever it starts a word, whatever follows it, so a joined
# name is caught too. The model family names (opus, sonnet, haiku) need no rule of their
# own: they count only beside a vendor or agent name, and that name already fails the line.
names='claude|codex|chatgpt|gpt|gemini|copilot|anthropic|openai|devin|aider|cursor[ -]?agent'
agent="(^|[^a-z])($names)"
# A Co-Authored-By line, quoted or indented or not, whoever it names.
credit_line='^[[:space:]>]*co-authored-by'
# "Generated with/by" an agent, a model or a link to one, or with a bracketed tool name;
# any trailer (Signed-off-by, Assisted-by, Co-developed-by ...) that names one.
agent_line="generated[[:space:]]+(with|by)([[:space:]]|[[:space:]].*[^a-z])($names)"
agent_line="$agent_line|generated[[:space:]]+with[[:space:]]+\\["
agent_line="$agent_line|^[[:space:]>]*[a-z-]+-by:(.*[^a-z])?($names)"
agent_line="$agent_line|🤖"
has_credit="the message has a Co-Authored-By line; this repo allows none, for a person or an agent"
credits_agent="the message credits an agent or a model"

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

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [ "$mode" = message ]; then
  [ -r "$message" ] || die "cannot read the message file"
  # What git will drop is not the message: comment lines, and the diff of `commit -v`.
  sed -e '/^# -* >8 -*$/,$d' -e '/^#/d' "$message" >"$work/message"
  problems=0
  if grep -a -i -E -q -e "$credit_line" "$work/message"; then
    echo "commit-msg: $has_credit" >&2
    problems=1
  fi
  if grep -a -i -E -q -e "$agent_line" "$work/message"; then
    echo "commit-msg: $credits_agent" >&2
    problems=1
  fi
  [ "$problems" -eq 0 ] || {
    echo "commit-msg: remove the line; commits here name no agent or model" >&2
    exit 1
  }
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
  if [ ! -f "$list" ] || [ ! -r "$list" ] || ! grep -a -q '[^[:space:]]' "$list"; then
    echo "clean-room scan: the word list is unreadable or empty; refusing to pass" >&2
    exit 1
  fi
fi
# The words, without blank lines or the spaces and carriage returns that end a line.
words() { sed 's/[[:space:]]*$//' "$list" | grep -a -v '^$'; }

cd "$(git rev-parse --show-toplevel)"
text="$work/text"
mkdir "$text" "$work/env" "$work/strip"
: >"$work/hits"
: >"$work/attribution-hits"
# The field separator of a found line: sha, path, line number, text.
S=$'\001'

# The values of a local .env, one file per value, so a hit can name its variable.
env_count=0
if [ -f .env ]; then
  while IFS= read -r line || [ -n "$line" ]; do
    line=${line%$'\r'}
    line=${line#"${line%%[![:space:]]*}"}
    case "$line" in export[[:space:]]*)
      line=${line#export}
      line=${line#"${line%%[![:space:]]*}"}
      ;;
    esac
    case "$line" in '' | '#'* | [!A-Za-z_]*) continue ;; *=*) ;; *) continue ;; esac
    name=${line%%=*}
    value=${line#*=}
    name=${name%"${name##*[![:space:]]}"}
    case "$name" in *[!A-Za-z0-9_]*) continue ;; esac
    value=${value#"${value%%[![:space:]]*}"}
    case "$value" in
      \"*) value=${value#\"} value=${value%%\"*} ;;
      \'*) value=${value#\'} value=${value%%\'*} ;;
      *)
        # An unquoted value ends at a comment.
        case "$value" in '#'*) value= ;; esac
        value=${value%%[[:space:]]#*}
        value=${value%"${value##*[![:space:]]}"}
        ;;
    esac
    [ "${#value}" -ge 8 ] || continue
    env_count=$((env_count + 1))
    printf '%s\n' "$name" >"$work/env/$env_count.name"
    printf '%s\n' "$value" >"$work/env/$env_count.value"
    printf '%s\n' "$value" >>"$work/env/all"
  done <.env
fi

# What `search` looks in: the working tree; some commits (all of their trees, or some
# paths of them); or files of this script's own, under a folder, reported under a label.
target=
target_dir=
target_revs=()
target_items=()
label_sha=
label_path=
look_in_worktree() { target=worktree; }
# The commits in $target_revs, and of them the paths in $target_items (none: whole trees).
look_in_commits() { target=commits; }
look_in_files() { # <folder> <sha label> <path label, or empty for the file's name> <file...>
  target=files target_dir=$1 label_sha=$2 label_path=$3
  shift 3
  target_items=("$@")
}

# search <git grep options...>: writes what was found to $work/found, one line each, as
# sha, path, line number and text, separated by $S. Every backend is git grep, so the
# matching is the same everywhere. No match is fine; a search that could not run is not.
search() {
  local rc=0
  case "$target" in
    worktree)
      git grep --untracked -a -n -z "$@" >"$work/raw" || rc=$?
      ;;
    files)
      (cd "$target_dir" && env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
        git grep --no-index -a -n -z "$@" -- "${target_items[@]}") >"$work/raw" || rc=$?
      ;;
    commits)
      git --literal-pathspecs grep -a -n -z "$@" "${target_revs[@]}" -- ${target_items[@]+"${target_items[@]}"} >"$work/raw" || rc=$?
      ;;
  esac
  [ "$rc" -le 1 ] || die "a search failed (exit $rc); refusing to pass"
  : >"$work/found"
  [ -s "$work/raw" ] || return 0
  # A line that is not path, number, text is the first part of a path with a newline in
  # it; the rest of that path follows with the number and the text.
  case "$target" in
    worktree) tr '\0' "$S" <"$work/raw" | sed "s/^/$S/" ;;
    files)
      tr '\0' "$S" <"$work/raw" |
        LABEL_SHA=$label_sha LABEL_PATH=$label_path awk -F "$S" -v OFS="$S" \
          '{ if (ENVIRON["LABEL_PATH"] != "") $1 = ENVIRON["LABEL_PATH"]; print ENVIRON["LABEL_SHA"], $0 }'
      ;;
    commits) tr '\0' "$S" <"$work/raw" | sed -e "s/^\([0-9a-f]\{40,64\}\):/\1$S/" -e t -e "s/^/$S/" ;;
  esac | awk -F "$S" 'NF >= 4 && $3 ~ /^[0-9]+$/' >"$work/found"
}

# hit [<note>]: records where each found line is, without its text.
hit() {
  NOTE=${1:-} awk -F "$S" -v OFS="$S" '{ print $1, $2, $3, ENVIRON["NOTE"] }' >>"$work/hits"
}

secrets_here() {
  local args=() p n
  for p in "${patterns[@]}"; do args+=(-e "$p"); done
  # One search for every shape; a closer look only where that finds something.
  search -E "${args[@]}" -e "$sk_shape" -e "$email"
  if [ -s "$work/found" ]; then
    search -E "${args[@]}"
    hit <"$work/found"
    # -o: one found line per match, the match as its text.
    search -o -E -e "$sk_shape"
    { grep -a -E -e "$S.?($sk_body)[^$S]*\$" "$work/found" || true; } | hit
    search -o -E -e "$email"
    { grep -a -v -E -e "($allow)[^$S]*\$" "$work/found" || true; } | hit
  fi
  [ "$env_count" -gt 0 ] || return 0
  search -F -f "$work/env/all"
  [ -s "$work/found" ] || return 0
  n=1
  while [ "$n" -le "$env_count" ]; do
    search -F -f "$work/env/$n.value"
    hit " (the value of $(cat "$work/env/$n.name") in .env)" <"$work/found"
    n=$((n + 1))
  done
}

words_contents=0 words_paths=0 words_messages=0 words_refs=0 words_tags=0
words_add() { # <kind> <count>
  case "$1" in
    contents | link-targets) words_contents=$((words_contents + $2)) ;;
    tree-paths | commit-paths) words_paths=$((words_paths + $2)) ;;
    commit-message) words_messages=$((words_messages + $2)) ;;
    ref-names) words_refs=$((words_refs + $2)) ;;
    tag-message) words_tags=$((words_tags + $2)) ;;
  esac
}
# words_here <kind>: counts the lines that hold a listed word. For the kind `text`, the
# target is this script's text files, and each line counts for the folder its file is in.
words_here() {
  [ "$secrets_only" -eq 0 ] || return 0
  local kind n
  search -i -F -f <(words)
  [ -s "$work/found" ] || return 0
  if [ "$1" = text ]; then
    awk -F "$S" '{ split($2, part, "/"); count[part[1]]++ }
      END { for (kind in count) print kind, count[kind] }' "$work/found" >"$work/counts"
    while read -r kind n; do words_add "$kind" "$n"; done <"$work/counts"
  else
    words_add "$1" "$(wc -l <"$work/found" | tr -d ' ')"
  fi
}

# Both scans over the contents of the current target; then, for each file there that
# holds a NUL byte, both again over the file with those bytes removed.
scan_contents() {
  local where=$target p sha blob seen=" "
  secrets_here
  words_here contents
  # The names of the files with a NUL byte, one to a line, a newline in a name held as
  # \002. The bracket is every byte but NUL, so its negation is NUL alone.
  search -l -E -e "[^$(printf '\001')-$(printf '\377')]"
  [ -s "$work/raw" ] || return 0
  tr '\n\0' '\002\n' <"$work/raw" >"$work/nul-files"
  while IFS= read -r p; do
    p=${p//$'\002'/$'\n'}
    if [ "$where" = worktree ]; then
      [ -f "$p" ] && [ ! -L "$p" ] || continue
      tr -d '\0' <"$p" >"$work/strip/blob"
      look_in_files "$work/strip" "" "${p//$'\n'/?}" blob
    else
      sha=${p%%:*}
      p=${p#*:}
      blob=$(git rev-parse --verify --quiet "$sha:$p") || die "cannot read $sha:$p; refusing to pass"
      # The same file in the next commit is the same blob: once is enough.
      case "$seen" in *" $blob "*) continue ;; esac
      seen="$seen$blob "
      git cat-file blob "$blob" | tr -d '\0' >"$work/strip/blob"
      look_in_files "$work/strip" "$sha" "${p//$'\n'/?}" blob
    fi
    secrets_here
    words_here contents
  done <"$work/nul-files"
}

zero() {
  case "$1" in *[!0]*) return 1 ;; esac
}

# flag <folder> <pattern> <what> <problem>: one attribution line for each file in the
# folder that matches; the files are named for the commit or tag they belong to.
flag() {
  local found rc=0 f
  found=$(cd "$1" && grep -a -i -E -l -r -e "$2" .) || rc=$?
  [ "$rc" -le 1 ] || die "a search failed (exit $rc); refusing to pass"
  [ -n "$found" ] || return 0
  while IFS= read -r f; do
    echo "$3 ${f#./}: $4"
  done <<<"$found" >>"$work/attribution-hits"
}

# links <sha> <mode> <blob> <to>: a link's target is text to search.
link() {
  [ "$1" = 120000 ] || return 0
  git cat-file blob "$2" >>"$3"
  echo >>"$3"
}

commits=0
# Everything about the outgoing commits but the trees of the tips: messages, makers,
# changed paths, link targets, and the changed files as they were in each commit.
scan_commits() { # reads $work/outgoing
  local record sha i meta p mode blob all=() union=()
  commits=$(wc -l <"$work/outgoing" | tr -d ' ')
  [ "$commits" -gt 0 ] || return 0
  git log -z --no-walk=unsorted --stdin --format='%H%n%an%n%ae%n%cn%n%ce%n%B' <"$work/outgoing" >"$work/log"
  while IFS= read -r -d '' record; do
    sha=${record%%$'\n'*}
    record=${record#*$'\n'}
    i=0
    while [ "$i" -lt 4 ]; do
      printf '%s\n' "${record%%$'\n'*}" >>"$work/makers/$sha"
      record=${record#*$'\n'}
      i=$((i + 1))
    done
    printf '%s\n' "$record" >"$text/commit-message/$sha"
  done <"$work/log"
  # Against every parent, so a change made in a merge itself is covered.
  git diff-tree --stdin -r --root -m -z <"$work/outgoing" >"$work/changes"
  : >"$work/changed"
  sha=
  while IFS= read -r -d '' meta; do
    case "$meta" in
      :*)
        IFS= read -r -d '' p || die "cannot read the changes of $sha; refusing to pass"
        printf '%s\n' "$p" >>"$text/commit-paths/$sha"
        printf '%s\0' "$p" >>"$work/changed"
        read -r _ mode _ blob _ <<<"$meta"
        link "$mode" "$blob" "$text/link-targets/$sha"
        ;;
      *) sha=$meta ;;
    esac
  done <"$work/changes"
  [ -s "$work/changed" ] || return 0
  # The changed files are searched in every outgoing commit, as each commit had them. A
  # newline in a name is held as \002 while the names are lines.
  tr '\n\0' '\002\n' <"$work/changed" | sort -u >"$work/union"
  # Too many paths for one command line: look in the commits' whole trees instead.
  if [ "$(wc -l <"$work/union" | tr -d ' ')" -le 400 ]; then
    while IFS= read -r p; do union+=("${p//$'\002'/$'\n'}"); done <"$work/union"
  fi
  while read -r sha; do all+=("$sha"); done <"$work/outgoing"
  i=0
  while [ "$i" -lt "${#all[@]}" ]; do
    look_in_commits
    target_revs=("${all[@]:$i:200}")
    target_items=(${union[@]+"${union[@]}"})
    scan_contents
    i=$((i + 200))
  done
}

trees=0
# The whole tree of each tip, its paths and its links.
scan_trees() { # <sha...>
  local sha meta mode blob revs=() seen=" "
  for sha in "$@"; do
    case "$seen" in *" $sha "*) continue ;; esac
    seen="$seen$sha "
    revs+=("$sha")
    trees=$((trees + 1))
    : >"$text/tree-paths/$sha"
    git ls-tree -r -z "$sha" >"$work/entries"
    while IFS= read -r -d '' meta; do
      printf '%s\n' "${meta#*$'\t'}" >>"$text/tree-paths/$sha"
      read -r mode _ blob <<<"${meta%%$'\t'*}"
      link "$mode" "$blob" "$text/link-targets/tree-$sha"
    done <"$work/entries"
  done
  look_in_commits
  target_revs=("${revs[@]}")
  target_items=()
  scan_contents
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
    sed -n '1,/^$/s/^tagger //p' "$work/tag" >"$work/taggers/$obj"
    obj=$(sed -n '1,/^$/s/^object //p' "$work/tag" | head -n 1)
  done
  [ "$kind" = commit ] || die "$obj is a $kind, not a commit; the scan cannot cover it; refusing to pass"
  tip=$obj
}

mkdir "$text/tree-paths" "$text/link-targets"
case "$mode" in
  tree)
    files=0
    git ls-files -z -co --exclude-standard >"$work/entries"
    while IFS= read -r -d '' f; do
      if [ -L "$f" ]; then
        readlink "$f" >>"$text/link-targets/working-tree"
      elif [ -f "$f" ]; then
        files=$((files + 1))
      elif [ -e "$f" ]; then
        die "$f is neither a file nor a link; the scan cannot cover it; refusing to pass"
      else
        continue # deleted here and not yet committed: nothing to publish
      fi
      printf '%s\n' "$f" >>"$text/tree-paths/working-tree"
    done <"$work/entries"
    look_in_worktree
    scan_contents
    covered="$files files"
    ;;
  range | push)
    mkdir "$text/commit-message" "$text/commit-paths" "$text/ref-names" "$text/tag-message" \
      "$work/makers" "$work/taggers"
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
      # The last line counts even without a newline after it.
      while read -r local_ref local_sha remote_ref remote_sha more || [ -n "$local_ref" ]; do
        [ -n "$local_ref" ] || continue
        if [ -z "$remote_sha" ] || [ -n "$more" ] ||
          [ -n "${local_sha//[0-9a-f]/}" ] || [ -n "${remote_sha//[0-9a-f]/}" ] ||
          { [ "${#local_sha}" -ne 40 ] && [ "${#local_sha}" -ne 64 ]; } ||
          [ "${#remote_sha}" -ne "${#local_sha}" ]; then
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
      git rev-list --reverse --stdin <"$work/revs" >"$work/outgoing" ||
        die "cannot establish the range; refusing to pass"
      scan_commits
      scan_trees "${tips[@]}"
    fi
    flag "$text/commit-message" "$credit_line" commit "$has_credit"
    flag "$text/commit-message" "$agent_line" commit "$credits_agent"
    flag "$work/makers" "$agent" commit "the author or committer is an agent or a model"
    flag "$text/tag-message" "$credit_line" tag "$has_credit"
    flag "$text/tag-message" "$agent_line" tag "$credits_agent"
    flag "$work/taggers" "$agent" tag "the tagger is an agent or a model"
    covered="$commits commits, $trees trees, $tags tags"
    ;;
esac

# The text that is not file contents: link targets, paths, messages, names.
items=()
for kind in link-targets tree-paths commit-paths commit-message ref-names tag-message; do
  [ -d "$text/$kind" ] || continue
  for f in "$text/$kind"/*; do [ -f "$f" ] && items+=("$kind/${f##*/}"); done
done
if [ "${#items[@]}" -gt 0 ]; then
  look_in_files "$text" - "" "${items[@]}"
  secrets_here
  words_here text
fi

status=0
if [ -s "$work/hits" ]; then
  # A path that itself holds a match is not printed. The paths that hold one:
  awk -F "$S" '$1 != "-" { print $2 }' "$work/hits" | sort -u >"$work/strip/paths"
  : >"$work/withheld-lines"
  if [ -s "$work/strip/paths" ]; then
    look_in_files "$work/strip" - "" paths
    args=(-e "$sk_shape")
    for p in "${patterns[@]}"; do args+=(-e "$p"); done
    search -E "${args[@]}"
    cut -d "$S" -f 3 "$work/found" >>"$work/withheld-lines"
    search -o -E -e "$email"
    { grep -a -v -E -e "($allow)[^$S]*\$" "$work/found" || true; } | cut -d "$S" -f 3 >>"$work/withheld-lines"
    if [ "$env_count" -gt 0 ]; then
      search -F -f "$work/env/all"
      cut -d "$S" -f 3 "$work/found" >>"$work/withheld-lines"
    fi
    if [ "$secrets_only" -eq 0 ]; then
      search -i -F -f <(words)
      cut -d "$S" -f 3 "$work/found" >>"$work/withheld-lines"
    fi
  fi
  LINES="$work/withheld-lines" awk 'BEGIN { while ((getline n < ENVIRON["LINES"]) > 0) held[n] = 1 }
    NR in held' "$work/strip/paths" >"$work/withheld"
  echo "secrets scan: found a match for a forbidden pattern in:" >&2
  WITHHELD="$work/withheld" awk -F "$S" 'BEGIN { while ((getline p < ENVIRON["WITHHELD"]) > 0) withheld[p] = 1 }
    {
      path = $2
      if ($1 != "-" && path in withheld) path = "<path withheld>"
      where = path ":" $3 $4
      if ($1 != "" && $1 != "-") where = $1 ":" where
      print where
    }' "$work/hits" | sort -u >&2
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
