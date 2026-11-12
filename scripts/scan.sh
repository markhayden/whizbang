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
#
# That bash also exits 0, once the trap is set, from faults no parse finds: an unset
# variable, a bad substitution, an assignment to a readonly variable, a syntax error in an
# `eval` or a sourced file; and any bash exits 0 from a file whose rest was swallowed by a
# here-document with no end. So the trap turns an exit of 0 into 70, and says so, unless
# `done_ok` is 1, and that is set only on the line before an exit that is meant to succeed.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "scan: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
set -euo pipefail
umask 077
# Bytes, not characters: a byte that is not valid UTF-8 must not hide the rest of its line.
export LC_ALL=C
# Anything git says on stderr during a search fails the scan, so git must not be told to
# talk: tracing is switched off for this run.
for name in ${!GIT_TRACE@} GIT_CURL_VERBOSE; do unset "$name"; done
unset name

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

# In TMPDIR when it is set (a bare `mktemp -d` on macOS does not look at it); only this
# user can enter the folder.
work=$(mktemp -d "${TMPDIR:-/tmp}/whizbang-scan.XXXXXX")
done_ok=0
trap 'rc=$?; rm -rf "$work"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "scan: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT
# A signal ends the scan through the same trap, so the folder is removed then too. Two
# cases leave it behind: a kill -9, and a signal that lands while the trap is already
# running. What is left is readable by this user alone: this run's findings with the text
# of each matching line (so a listed word that is in the repository is there, in its
# line), and the values of a local .env. The word list itself is never there.
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

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
  done_ok=1
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
# No copy of the list is made: each run of a search reads it again through `words`.

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
# Set by each of the three below: the next search starts with a census of its target.
censused=0
# The tracked files that an ignore rule matches (working tree only).
ignored_tracked=()
look_in_worktree() { target=worktree censused=0; }
# The commits in $target_revs, and of them the paths in $target_items (none: whole trees).
look_in_commits() { target=commits censused=0; }
look_in_files() { # <folder> <sha label> <path label, or empty for the file's name> <file...>
  target=files target_dir=$1 label_sha=$2 label_path=$3 censused=0
  shift 3
  target_items=("$@")
}

# run_grep <to> <git grep options...>: one git grep over the current target. When
# $with_words is 1 the listed words are added as fixed patterns; they are read from the
# list anew for this one run, through a pipe, so the search can run twice and no copy of
# the list is ever written.
with_words=0
run_grep() {
  local to=$1
  shift
  : >"$work/search-errors"
  run_part "$target" "$to" "$@"
  # --untracked leaves out what an ignore rule matches, tracked or not. A tracked file is
  # published whatever the rules say, so those are searched by name. (Such a file that is
  # also marked assume-unchanged is searched as the index has it, not as the folder has
  # it; while it is marked, a change to it cannot be committed.)
  if [ "$target" = worktree ] && [ "${#ignored_tracked[@]}" -gt 0 ]; then
    run_part ignored "$work/raw-ignored" "$@"
    cat "$work/raw-ignored" >>"$to"
  fi
  # git grep goes on past a file it cannot read, says so and still exits 0 or 1. Anything
  # at all that git says on stderr counts, a warning too. It is not printed: it names
  # paths, and a path can hold a match.
  [ ! -s "$work/search-errors" ] ||
    die "git said something during a search ($(wc -l <"$work/search-errors" | tr -d ' ') lines of warnings or errors, such as a file it could not read); any such line counts, because what git did not read is not covered; refusing to pass. The lines are not shown, since they name paths; to see them, run a search by hand, for example: git -c grep.threads=1 grep --untracked -c -e . >/dev/null"
}
# run_part <which> <to> <options...>: one git grep. The words come through a pipe, which
# one git grep reads once, so each part of a search gets its own.
run_part() {
  if [ "$with_words" -eq 1 ]; then
    run_grep_once "$@" -i -F -f <(words)
  else
    run_grep_once "$@"
  fi
}
run_grep_once() {
  local which=$1 to=$2 rc=0
  shift 2
  case "$which" in
    worktree)
      git -c grep.threads=1 grep --untracked -a -n -z "$@" >"$to" 2>>"$work/search-errors" || rc=$?
      ;;
    ignored)
      # The names are the index's. Where git composes names (macOS), it would compose
      # these too and then not find one that the index holds decomposed: not here.
      GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.precomposeunicode GIT_CONFIG_VALUE_0=false \
        git --literal-pathspecs -c grep.threads=1 grep -a -n -z "$@" -- "${ignored_tracked[@]}" >"$to" 2>>"$work/search-errors" || rc=$?
      ;;
    files)
      (cd "$target_dir" && env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
        git -c grep.threads=1 grep --no-index -a -n -z "$@" -- "${target_items[@]}") >"$to" 2>>"$work/search-errors" || rc=$?
      ;;
    commits)
      git --literal-pathspecs -c grep.threads=1 grep -a -n -z "$@" "${target_revs[@]}" -- ${target_items[@]+"${target_items[@]}"} >"$to" 2>>"$work/search-errors" || rc=$?
      ;;
  esac
  [ "$rc" -le 1 ] || die "a search failed (exit $rc); refusing to pass"
}

# names: NUL-ended names on stdin become sorted lines, a newline in a name held as \002.
names() { tr '\n\0' '\002\n' | sort -u; }

# census: fails unless git grep looks in every file of the current target that has anything
# in it. What the target should hold is listed without git grep (the working tree's files,
# the regular files of each commit's tree, the scan's own text files); what git grep looks
# in is every file where the empty pattern matches a line, which is every file that is not
# empty. A link is not a file here: its target is searched as text elsewhere.
#
# What it can and cannot see. It sees a file the search never opened; it does not see a
# match lost inside a file that was opened (the two runs of every search, and the canary
# in CI, are for that). In the working tree the tracked files come from the index and the
# untracked ones from `find`, with the ignored ones taken out by `git check-ignore`, so the
# list shares no folder-walking code with the search.
#
# The walk goes level by level and never enters a folder that git says is ignored: the
# folders directly inside the folders of one level are listed, git is asked about them,
# and the ones it does not ignore are the next level. That leaves out nothing git would
# list, because git cannot put back a file whose folder is ignored: for a file,
# `git check-ignore` first asks the same question of each folder above it, top down, by
# the rules alone, and answers with the first one that is ignored. So a file under an
# ignored folder was always dropped; now it is not looked at. The folders are asked about
# by the rules alone as well (`--no-index`: without it git calls a folder that holds a
# tracked file not ignored). A tracked file in an ignored folder still comes from the
# index. What goes on inside an ignored folder (a build that is running, a folder that
# cannot be read) can no longer fail the scan.
#
# Names. Files are asked about by the rules alone too: otherwise git reads the name as a
# pattern, and an ignored `a*b.tmp` beside a tracked `axb.tmp` counts as tracked. A name
# that holds the byte \001 or \002 is refused (`plain_names`). And where git composes the
# names it reads from a folder (macOS: `core.precomposeUnicode`), the names find gives
# are composed the same way (`compose`) before git is asked about them, and so are both
# lists before they are compared: there the search names a file as its folder does, and
# the index may hold the same name decomposed.
census() {
  local f rev missing rc
  case "$target" in
    worktree)
      rc=0
      git ls-files -z -c >"$work/census-entries" 2>"$work/census-errors" || rc=$?
      [ "$rc" -eq 0 ] && [ ! -s "$work/census-errors" ] ||
        die "the tracked files could not be listed (git ls-files: exit $rc, $(wc -l <"$work/census-errors" | tr -d ' ') lines of warnings or errors); refusing to pass"
      plain_names "$work/census-entries"
      composing=0
      [ "$(git config --type=bool core.precomposeunicode 2>/dev/null || true)" != true ] || composing=1
      printf '.\0' >"$work/census-level"
      : >"$work/census-folders"
      while [ -s "$work/census-level" ]; do
        cat "$work/census-level" >>"$work/census-folders"
        walk d <"$work/census-level"
        # A folder is asked about by its name alone. With a `/` after it git takes the
        # name for a path inside the folder: `out/*` would then match `out/`.
        unignored 1 --no-index >"$work/census-level"
      done
      walk f <"$work/census-folders"
      unignored 3 --no-index >>"$work/census-entries"
      compose "$work/census-entries"
      while IFS= read -r -d '' f; do
        if [ -f "$f" ] && [ ! -L "$f" ] && [ -s "$f" ]; then printf '%s\0' "$f"; fi
      done <"$work/census-entries" | names >"$work/census-listed"
      ;;
    files)
      for f in "${target_items[@]}"; do
        if [ -s "$target_dir/$f" ]; then printf '%s\0' "$f"; fi
      done | names >"$work/census-listed"
      ;;
    commits)
      for rev in "${target_revs[@]}"; do
        git --literal-pathspecs ls-tree -r -l -z "$rev" -- ${target_items[@]+"${target_items[@]}"} |
          tr '\n\0' '\002\n' | REV=$rev awk '{
            tab = index($0, "\t")
            split(substr($0, 1, tab - 1), field, " ")
            if ((field[1] == "100644" || field[1] == "100755") && field[4] + 0 > 0)
              print ENVIRON["REV"] ":" substr($0, tab + 1)
          }'
      done | sort -u >"$work/census-listed"
      ;;
  esac
  run_grep "$work/census-raw" -l -F -e ''
  [ "$target" != worktree ] || compose "$work/census-raw"
  names <"$work/census-raw" >"$work/census-searched"
  missing=$(comm -23 "$work/census-listed" "$work/census-searched" | wc -l | tr -d ' ')
  [ "$missing" -eq 0 ] ||
    die "$missing of the $(wc -l <"$work/census-listed" | tr -d ' ') files to cover were not looked in by the search ($target); they are not named here, since a path can hold a match; refusing to pass"
}

# plain_names <file>: fails if any of the NUL-ended names in the file holds the byte \001
# or \002. The scan keeps names as lines with a newline held as \002, and separates the
# fields of what it finds with \001: such a name would be taken for another, or lost. The
# names are counted and not shown.
plain_names() {
  local odd
  odd=$(tr '\n\0' ' \n' <"$1" | { grep -a -c -e "[$S"$'\002'"]" || true; })
  [ "$odd" -eq 0 ] ||
    die "$odd names hold the byte \\001 or \\002, which the scan uses to tell names and fields apart, so it cannot cover them; rename them; refusing to pass"
}
# compose <file>: rewrites the NUL-ended names in the file, each part of a path composed
# as git composes a name it reads from a folder: a part with a byte that is not ASCII goes
# through the same conversion (iconv, UTF-8-MAC to UTF-8), and stays as it is when that
# fails. Only where git does it, and only when some name has such a byte.
composing=0
compose() {
  local f rest part new out
  [ "$composing" -eq 1 ] || return 0
  [ "$(($(tr -d '\000-\177' <"$1" | wc -c)))" -gt 0 ] || return 0
  while IFS= read -r -d '' f; do
    case "$f" in
      *[!$'\001'-$'\177']*)
        rest=$f out=
        while :; do
          part=${rest%%/*}
          case "$part" in
            *[!$'\001'-$'\177']*)
              # The dot keeps a newline that ends the name.
              if new=$(printf '%s.' "$part" | iconv -f UTF-8-MAC -t UTF-8 2>/dev/null); then part=${new%.}; fi
              ;;
          esac
          out=$out$part
          case "$rest" in */*) rest=${rest#*/} out=$out/ ;; *) break ;; esac
        done
        f=$out
        ;;
    esac
    printf '%s\0' "$f"
  done <"$1" >"$work/census-composed"
  mv "$work/census-composed" "$1"
}
# walk <d|f>: writes to $work/census-found the folders (d) or the files (f) directly
# inside each folder named on stdin, NUL-ended, as `./path`. find does not follow a link,
# so what a link leads to is not listed (nor searched); and it goes no deeper than the
# folders it is given. Nothing named `.git` is listed. What find says is not printed: it
# names paths.
walk() {
  local rc=0
  xargs -0 sh -c 'kind=$1; shift; exec find "$@" -mindepth 1 -maxdepth 1 ! -name .git -type "$kind" -print0' sh "$1" \
    >"$work/census-found" 2>"$work/census-errors" || rc=$?
  [ "$rc" -eq 0 ] && [ ! -s "$work/census-errors" ] || {
    if [ "$rc" -eq 0 ]; then rc="said something:"; else rc="failed, with"; fi
    die "the files of the working tree could not all be listed (find $rc $(wc -l <"$work/census-errors" | tr -d ' ') lines of errors, such as a folder it could not read); a file or folder changed during the walk, or one cannot be read: run it again, and if it refuses again, look for a folder that cannot be read; what was not listed is not covered; refusing to pass"
  }
  plain_names "$work/census-found"
  compose "$work/census-found"
}
# unignored <from> [--no-index]: of the paths in $work/census-found, NUL-ended, the ones
# no ignore rule keeps out, each from its character number <from> (3 leaves out the `./`).
# Without --no-index git says of a tracked path that no rule matches it, and it reads
# each path as a pattern that a tracked path may match.
# check-ignore prints four fields for each path: where the rule is, its line, the rule,
# the path. No rule matched when the rule is empty; and a rule that starts with `!` puts
# the path back, so that is not ignored either. Anything git says counts, and so does a
# path it gives no answer for: the scan does not guess.
unignored() {
  local rc=0 asked answered
  [ -s "$work/census-found" ] || return 0
  git check-ignore -z --stdin -v -n ${2+"$2"} <"$work/census-found" >"$work/census-rules" 2>"$work/census-errors" || rc=$?
  [ "$rc" -le 1 ] && [ ! -s "$work/census-errors" ] ||
    die "the files of the working tree could not be told from the ignored ones (git check-ignore: exit $rc, $(wc -l <"$work/census-errors" | tr -d ' ') lines of warnings or errors); refusing to pass"
  asked=$(($(tr -c -d '\0' <"$work/census-found" | wc -c)))
  answered=$(($(tr -c -d '\0' <"$work/census-rules" | wc -c)))
  [ "$answered" -eq $((asked * 4)) ] ||
    die "the files of the working tree could not be told from the ignored ones: git check-ignore answered for $((answered / 4)) of the $asked paths it was asked about; refusing to pass"
  tr '\n\0' '\002\n' <"$work/census-rules" |
    awk -v from="$1" 'NR % 4 == 3 { free = ($0 == "" || substr($0, 1, 1) == "!") } NR % 4 == 0 && free { print substr($0, from) }' |
    tr '\002\n' '\n\0'
}

# search_words: a search for the listed words, in any case, as fixed strings.
search_words() {
  with_words=1
  search
  with_words=0
}

# search <git grep options...>: writes what was found to $work/found, one line each, as
# sha, path, line number and text, separated by $S. Every backend is git grep, so the
# matching is the same everywhere. No match is fine; a search that could not run, or that
# reported any error, is not. Nor is one whose target git grep did not look through (the
# census), or whose two runs differ.
#
# Every search runs on one thread (`-c grep.threads=1`). On CI's macOS runner (macOS 15),
# git grep with an extended regular expression, in the C locale this script sets, on git's
# default of several threads, lost a match or returned a wrong `-o` match at random: about
# 4 to 10 searches in 100, with no error and the usual exit status, in the working tree, in
# commits and in plain files, with git 2.56 and with Apple's 2.39. On one thread it was
# wrong in none of 6,740 searches; nor was it in a UTF-8 locale, with a fixed string or
# with `-P`. The locale stays C, because in UTF-8 a byte that is not UTF-8 hides the rest
# of its line. Where the fault lies below git has not been established.
# scripts/tests/test-syntax.sh holds every git grep under scripts/ to one thread.
search() {
  # The first search of a target is preceded by a census of it.
  if [ "$censused" -eq 0 ]; then
    censused=1
    census
  fi
  # Twice, and the two must agree: a search that loses a match now and then (as git grep
  # did on several threads, on CI's macOS runner) is not trusted on its first answer.
  run_grep "$work/raw" "$@"
  run_grep "$work/raw-again" "$@"
  cmp -s "$work/raw" "$work/raw-again" ||
    die "two runs of the same search disagree, so neither is trusted; refusing to pass. The ordinary cause is a file that changed during the scan: run it again. If it happens twice with nothing changing, see the comment above \`search\` in scripts/scan.sh"
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
  search_words
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
  plain_names "$work/changed"
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
    plain_names "$work/entries"
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
    # What git says while it lists is not printed either: it names paths.
    git ls-files -z -co --exclude-standard >"$work/entries" 2>"$work/list-errors" ||
      die "the files of the working tree could not be listed (git ls-files failed); refusing to pass"
    [ ! -s "$work/list-errors" ] ||
      die "git said something while it listed the working tree ($(wc -l <"$work/list-errors" | tr -d ' ') lines of warnings or errors, such as a folder it could not read); what was not listed is not covered; refusing to pass"
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
    rc=0
    git ls-files -z -ci --exclude-standard >"$work/entries-ignored" 2>"$work/list-errors" || rc=$?
    [ "$rc" -eq 0 ] && [ ! -s "$work/list-errors" ] ||
      die "the tracked files that an ignore rule matches could not be listed (git ls-files: exit $rc, $(wc -l <"$work/list-errors" | tr -d ' ') lines of warnings or errors); refusing to pass"
    while IFS= read -r -d '' f; do [ ! -f "$f" ] || [ -L "$f" ] || ignored_tracked+=("$f"); done <"$work/entries-ignored"
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
      search_words
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

done_ok=1
exit "$status"
