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
#   scripts/scan.sh --pr-text <title file> <body file>
#                                          the title and body of a pull request, before they
#                                          are put on it (the skills kickoff and review-merge)
#   --secrets-only                         with any but --message: skip the clean-room scan
#                                          (CI: the word list is never there)
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
# Every mode but --message refuses when a local .env is there and cannot be read (its
# values could not be searched for), when it is run outside a repository, and when a
# second mode is given.
#
# Attribution check: no commit or tag message has a Co-Authored-By line, for a person or
# an agent, or a line that credits an agent or a model; no commit or tag is authored by
# one (SPEC.md section 15).
#
# The title and body of a pull request are two files of plain text, outside the repo. Run
# from the repo, so the values of its local .env are searched for too, they get both
# scans, and an attribution check made for text that is not a commit message: a line counts
# from its first letter, whatever stands before it that is not a letter (a list's number
# or sign, a table bar, a quote, a heading, a backtick, a box, a comment's opening), and so
# does each cell of a table row; an agent's or a model's name counts wherever it starts a
# word, but for the folder `.claude` and the file `CLAUDE.md`, which this repo has, named
# as a folder and as a file; and a path that starts with `~` is refused. Each refusal
# names the file and the line, never the text. A line that says "co-authored-by" or
# "generated with" and is not refused is listed to be read: that is all the scan does
# with a credit behind a mark made of letters (`a. `, `Note: `, `<p>`). A file with a NUL
# byte is refused: it is not plain text; so is a title with no line break at its end.
# What it does not see: a phrase with a zero-width or a non-breaking character inside it;
# a path written with a plus or a minus sign between the tilde and the slash, or with the
# shell's variable for the home folder; and, run from a linked worktree, the .env of
# any other working tree (the .env read is the one at the top it is run from, and none
# there means none is searched for).
#
# Clean-room scan: reads a private word list from the path in WHIZBANG_WORDLIST. It fails
# closed when the variable is unset, or the list is unreadable or empty. It prints counts
# only: never a word, never a matching line.
#
# What a scan of the working tree cannot see. Which untracked files there are, and which
# of them are ignored, is git's answer alone: the list of files to cover and the search
# both rest on git's walk of the folders, so an untracked file that walk misses is
# missed by both. That cannot hide anything a push publishes: a push is scanned commit
# by commit, from the commits' trees, and tracked files are listed from the index.
#
# What the scan defends against. A careless caller: settings in the environment or in a
# configuration file that would change what git lists, searches or prints are overridden
# where the scan depends on them. A faulty tool: where the scan can tell that git or
# perl failed, said something or missed a file, it fails rather than pass. It does not defend against a caller who chooses the
# tools: whoever sets PATH, or replaces git, decides what the scan sees.
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
# The whole script is the function `main`, and the last line calls it and exits. Bash reads
# a script as it runs it, a piece at a time, so a file changed during a run (an editor that
# writes in place, a checkout that rewrites it) would make the rest of the run another
# program: the `EXIT` trap fails a run that stops early, not one that goes on wrongly. A
# function is read whole before any of it runs, and the last line is read with it in hand,
# so the run finishes as the program it started as. Every script under scripts/ that is not
# a test has this form, and scripts/tests/test-syntax.sh holds them to it. The body is not
# indented, so that each line is where it was.
main() {
set -euo pipefail
umask 077
# Bytes, not characters: a byte that is not valid UTF-8 must not hide the rest of its line.
export LC_ALL=C
# Anything git says on stderr during a search fails the scan, so git must not be told to
# talk: tracing is switched off for this run.
for name in ${!GIT_TRACE@} GIT_CURL_VERBOSE; do unset "$name"; done
# Nor may grep be given options from outside (some greps read GREP_OPTIONS).
unset GREP_OPTIONS
# Configuration that comes in environment variables holding the settings themselves
# (GIT_CONFIG_COUNT with its keys and values) is kept from every git the scan runs but
# one: left in place it would set anything for the listings and searches that decide
# what is scanned. The one is the question to the remote of a push (`git ls-remote`),
# which may be reached only through such a setting (`url.<base>.insteadOf`), so it is
# asked with them (`caller_config`). The variables that name configuration files
# (GIT_CONFIG_GLOBAL, GIT_CONFIG_SYSTEM, GIT_CONFIG_NOSYSTEM) are left alone: they choose
# among files this user's git reads anyway; and so are the options of a `git -c` that
# started a hook (GIT_CONFIG_PARAMETERS). Every search sets what it depends on itself,
# with `-c` and options, which outrank all of these: one thread and no colour; and no
# submodules where a setting could send it into one, which is a search of the working
# tree or of commits. (A search by name, or of the scan's own files, cannot reach one;
# and every search starts at the top, so a name is never shortened to a subfolder.)
caller_config=()
for name in GIT_CONFIG_COUNT ${!GIT_CONFIG_KEY_@} ${!GIT_CONFIG_VALUE_@}; do
  [ -z "${!name+set}" ] || caller_config+=("$name=${!name}")
  unset "$name"
done
unset name

die() {
  echo "scan: $*" >&2
  exit 1
}
usage() {
  die "usage: scan.sh [--secrets-only] [--range <base> <head> | --pre-push <remote> <url> | --pr-text <title file> <body file> | --message <file>]"
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
# The same family wherever it starts: directly after a hyphen, an underscore, a letter or
# a digit nothing marks the start of a key, and that is where a key stands after a
# written-out line break or tab in a JSON string (a backslash and a letter) and after
# percent-encoding (%20, %3D). A match runs from the letters to the end of the run of key
# characters, so it holds whatever follows them. Inside a word the rule above would take
# ordinary hyphenated words for keys (a word that ends in s and k, a hyphen, and a digit
# anywhere in the twenty characters after it: a version, a date, a model's name), so here
# what follows is narrowed: one part of it, between hyphens, is 20 characters or more and
# holds a digit or a capital (`sk_anywhere_is_key`). Words joined by hyphens have no such
# part; the keys of this family do, since their random part is long and has few hyphens.
# What it still takes for a key, each directly after a word that ends in s and k and a
# hyphen (task-, mask-, disk-, risk-, ask-): a hex digest; a file name of 20 letters and
# digits or more, as base64 gives; a name in capitals and small letters of 20 characters
# or more with no hyphen in it (a long class name); and a name joined by underscores, 20
# characters or more, with a digit in it (a model's weights, written with underscores
# after the hyphen). None can be told from a key by its shape. When a real name trips
# the scan, write the name another way: an underscore or a space in place of the hyphen
# after the word, or a hyphen inside the long part. There is no list of exceptions, and
# the rule is not loosened for a name.
# What this leaves: a key written directly after a word character whose every part
# between hyphens is under 20 characters, or has no digit and no capital. It is on
# docs/carry-over.md. A key that starts a word is held to the rule above, as before.
sk_anywhere='sk-[A-Za-z0-9_-]{20,}'
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
# In the title and body of a pull request: a path that starts with `~`, alone or with a
# user's name, where the `~` starts a word. The sign before a number ("~5 seconds", or
# before a fraction) starts no path.
tilde='(^|[^A-Za-z0-9_~])~([A-Za-z_][A-Za-z0-9._-]*)?/'

secrets_only=0
mode=tree
# One thing is scanned in a run: a second mode would take the place of the first, and
# what was asked for first would not be scanned.
modes=0
while [ $# -gt 0 ]; do
  case "$1" in --range | --pre-push | --message | --pr-text) modes=$((modes + 1)) ;; esac
  [ "$modes" -le 1 ] || usage
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
    --pr-text)
      [ $# -ge 3 ] || usage
      mode=text pr_title=$2 pr_body=$3
      shift 3
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
  # A name that starts with a dash is a file, not an option.
  case "$message" in /*) ;; *) message="./$message" ;; esac
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

if [ "$mode" = text ]; then
  # The two files are named from where the scan was started; it goes to the top next.
  case "$pr_title" in /*) ;; *) pr_title="$PWD/$pr_title" ;; esac
  case "$pr_body" in /*) ;; *) pr_body="$PWD/$pr_body" ;; esac
fi
# Outside a repository there is nothing to scan and no repo whose .env this would be: the
# scan does not carry on in whatever folder it was started in.
top=$(git rev-parse --show-toplevel 2>/dev/null) && [ -n "$top" ] ||
  die "not inside a repository (git cannot say where its top is); refusing to pass"
cd "$top"
text="$work/text"
mkdir "$text" "$work/env" "$work/strip"
: >"$work/hits"
: >"$work/attribution-hits"
# The field separator of a found line: sha, path, line number, text.
S=$'\001'

# The values of a local .env, one file per value, so a hit can name its variable.
env_count=0
# A .env that is there and cannot be read is not a .env with nothing in it.
[ ! -f .env ] || [ -r .env ] ||
  die "the local .env cannot be read, so its values cannot be searched for; refusing to pass"
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
      git -c grep.threads=1 grep --no-color --no-recurse-submodules --untracked -a -n -z "$@" >"$to" 2>>"$work/search-errors" || rc=$?
      ;;
    ignored)
      # The names are the index's. Where git composes names (macOS), it would compose
      # these too and then not find one that the index holds decomposed: not here.
      git -c core.precomposeunicode=false --literal-pathspecs -c grep.threads=1 grep --no-color -a -n -z "$@" -- "${ignored_tracked[@]}" >"$to" 2>>"$work/search-errors" || rc=$?
      ;;
    files)
      (cd "$target_dir" && env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
        git -c grep.threads=1 grep --no-color --no-index -a -n -z "$@" -- "${target_items[@]}") >"$to" 2>>"$work/search-errors" || rc=$?
      ;;
    commits)
      git --literal-pathspecs -c grep.threads=1 grep --no-color --no-recurse-submodules -a -n -z "$@" "${target_revs[@]}" -- ${target_items[@]+"${target_items[@]}"} >"$to" 2>>"$work/search-errors" || rc=$?
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
# in CI, are for that).
#
# In the working tree the tracked files come from the index, whatever the ignore rules
# say of them, and the untracked ones from git's own list (`git ls-files -o`, made where
# the scan of the working tree starts: $work/entries-others). The scan
# does not work out for itself which untracked files are ignored: it did, by walking the
# folders and asking `git check-ignore`, and each way it found to disagree with git was
# a file dropped from the list. So for untracked files the list and the search both rest
# on git's walk of the folders, and a file that walk misses is missed by both. That
# cannot hide anything a push publishes: a push is scanned commit by commit, from the
# commits' trees, and tracked files are listed from the index.
#
# There the two lists are compared as files, not as names (`look_up ids`): each name on
# either side is looked up once and stands for its device and inode. On macOS one file
# can go by two names (git composes the names it reads from a folder, the index holds
# whichever form was committed), and they are then one file; where the two forms are two
# files, they are two inodes. Two hard links to one file hold the same bytes, so a
# search of one has covered both. A name the index holds for a file that is not there
# ("no such file", or "not a folder" on the way to it) is a file deleted here and not yet
# committed: nothing to cover. A file that may be there and cannot be looked up is
# another matter and fails the scan (`look_up`). So does an untracked name, or a name
# the search gave, that leads nowhere: something changed while the scan ran.
#
# What a scan of the working tree refuses though nothing is wrong with the files, each
# because git's search does not look where the list says a file is. They are on
# docs/carry-over.md:
#   - a tracked file that is now a folder, a submodule that is checked out, a repository
#     inside this one, with or without a commit (counted, as paths that are neither a
#     file nor a link, or as files not looked in);
#   - a tracked file that a rule matches and that is marked skip-worktree, when it is on
#     disk; and a sparse index;
#   - a tracked file that a rule matches and that is marked assume-unchanged, when the
#     index has it empty and the file is not;
#   - a `.gitignore` that is a link (git does not read rules through one, and says so);
#   - on macOS, a file or folder tracked under a decomposed name that a rule written
#     composed ignores.
# And one thing it passes that a reader might expect it to refuse, also there: a tracked
# file that a rule matches and that is marked assume-unchanged is searched as the index
# has it, not as the folder has it; while it is marked, a change to it cannot be
# committed.
census() {
  local f rev missing rc
  case "$target" in
    worktree)
      rc=0
      git ls-files -z -c >"$work/census-entries" 2>"$work/census-errors" || rc=$?
      [ "$rc" -eq 0 ] && [ ! -s "$work/census-errors" ] ||
        die "the tracked files could not be listed (git ls-files: exit $rc, $(wc -l <"$work/census-errors" | tr -d ' ') lines of warnings or errors); refusing to pass"
      plain_names "$work/census-entries"
      plain_names "$work/entries-others"
      # A tracked name with no file behind it is a file deleted here: nothing to cover.
      look_up ids "$work/census-entries" >"$work/census-ids"
      wrote "$work/census-ids" lines
      look_up ids "$work/entries-others" >"$work/census-ids-others"
      wrote "$work/census-ids-others" lines
      [ "$rep_gone" -eq 0 ] ||
        die "$rep_gone untracked files that git listed are no longer there; something changed during the scan: run it again; refusing to pass"
      sort -u "$work/census-ids" "$work/census-ids-others" >"$work/census-listed"
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
  if [ "$target" = worktree ]; then
    look_up ids "$work/census-raw" >"$work/census-ids"
    wrote "$work/census-ids" lines
    [ "$rep_gone" -eq 0 ] ||
      die "$rep_gone names the search gave for the working tree lead to no file; something changed during the scan: run it again; refusing to pass"
    sort -u "$work/census-ids" >"$work/census-searched"
    missing=$(comm -23 "$work/census-listed" "$work/census-searched" | wc -l | tr -d ' ')
    [ "$missing" -eq 0 ] ||
      die "$missing of the $(wc -l <"$work/census-listed" | tr -d ' ') files to cover were not looked in by the search ($target); they are not named here, since a path can hold a match; a file that was replaced while the scan ran is one cause: run it again; refusing to pass"
    return 0
  fi
  names <"$work/census-raw" >"$work/census-searched"
  missing=$(comm -23 "$work/census-listed" "$work/census-searched" | wc -l | tr -d ' ')
  [ "$missing" -eq 0 ] ||
    die "$missing of the $(wc -l <"$work/census-listed" | tr -d ' ') files to cover were not looked in by the search ($target); they are not named here, since a path can hold a match; refusing to pass"
}

# look_up <what> <file...>: looks up, once each, the NUL-ended names of the files given,
# which are relative to the top of the working tree, and prints what <what> asks for:
#
#   ids <names>                  one line, device and inode, for each name that is a regular
#                                file with something in it (a link is not followed, so it
#                                is not one)
#   kinds <names> <paths> <targets>
#                                adds to <paths> the names that are files or links, one a
#                                line, and to <targets> where each link leads; a link
#                                that goes by two names (on macOS the index can hold one
#                                composed and decomposed) is two paths and one target:
#                                the same device, inode and target. Two links that
#                                lead to different places are two targets, whatever
#                                the file system says of their inodes
#   by-name <others> <ignored>   the names of <ignored>, NUL-ended, whose file is not one
#                                of <others> and has not been printed already
#
# perl, because it is on every system the scan runs on and reads a NUL-ended list and
# looks a name up in one process; `stat` would do, but its options differ between the
# systems.
#
# A name that is not there is one thing, a name that cannot be looked up another. Only
# "no such file" and "not a folder" on the way mean the file is gone. Anything else (a
# folder that cannot be searched, a path too long to open) leaves a file that may well be
# there and is not covered: the scan fails, with a count.
#
# perl is not taken at its word either. It reports how many names it read and how many
# lines it wrote, and ends its report with a line of its own; a perl that fails, stops
# early or says nothing fails the scan. The report is left in the rep_ variables: read,
# gone (names that are not there), out, and for `kinds` files, other (what is neither
# a file nor a link), and paths and targets (the bytes it added to each of its two
# files). What it wrote is then held to the report: `wrote` for what it printed,
# `holds` for the two files of `kinds`.
lookup='
  use strict;
  my $what = shift @ARGV;
  my %n = (read => 0, gone => 0, barred => 0, out => 0, files => 0, other => 0, paths => 0, targets => 0);
  sub names { open(my $in, "<", $_[0]) or die "$_[0]: $!\n"; $in }
  sub look {
    $n{read}++;
    my @s = lstat($_[0]);
    if (!@s) { $n{ $!{ENOENT} || $!{ENOTDIR} ? "gone" : "barred" }++ }
    @s;
  }
  if ($what eq "ids") {
    my $in = names($ARGV[0]);
    while (<$in>) {
      chomp;
      my @s = look($_) or next;
      next unless -f _ && $s[7] > 0;
      print "$s[0]:$s[1]\n";
      $n{out}++;
    }
  } elsif ($what eq "kinds") {
    my ($in, $paths, $targets, %link) = (names($ARGV[0]));
    while (<$in>) {
      chomp;
      my @s = look($_) or next;
      if (-l _) {
        my $target = readlink($_);
        if (!defined $target) { $n{barred}++; next }
        if (!$link{"$s[0]:$s[1]:$target"}++) {
          $targets or open($targets, ">>", $ARGV[2]) or die "$ARGV[2]: $!\n";
          print $targets "$target\n";
          $n{targets} += length($target) + 1;
        }
      } elsif (-f _) {
        $n{files}++;
      } else {
        $n{other}++;
        next;
      }
      $paths or open($paths, ">>", $ARGV[1]) or die "$ARGV[1]: $!\n";
      print $paths "$_\n";
      $n{paths} += length($_) + 1;
      $n{out}++;
    }
    !$paths or close($paths) or die "$ARGV[1]: $!\n";
    !$targets or close($targets) or die "$ARGV[2]: $!\n";
  } elsif ($what eq "by-name") {
    my %seen;
    my $in = names($ARGV[0]);
    while (<$in>) { chomp; my @s = look($_) or next; $seen{"$s[0]:$s[1]"} = 1 }
    $in = names($ARGV[1]);
    while (<$in>) {
      chomp;
      my @s = look($_) or next;
      next if $seen{"$s[0]:$s[1]"}++;
      print "$_\0";
      $n{out}++;
    }
  } else {
    die "look_up: $what?\n";
  }
  close(STDOUT) or die "stdout: $!\n";
  print STDERR "$_ $n{$_}\n" for sort keys %n;
  print STDERR "end 1\n";
'
rep_read=0 rep_gone=0 rep_out=0 rep_files=0 rep_other=0 rep_paths=0 rep_targets=0
look_up() {
  local rc=0 key value barred= end= asked=0 f
  perl -0 -e "$lookup" "$@" 2>"$work/lookup-report" || rc=$?
  [ "$rc" -eq 0 ] ||
    die "the files of the working tree could not be looked up (perl: exit $rc); the scan needs perl to tell which files there are and which the search looked in; refusing to pass"
  rep_read= rep_gone= rep_out= rep_files= rep_other= rep_paths= rep_targets=
  while read -r key value; do
    case "$value" in '' | *[!0-9]*) continue ;; esac
    case "$key" in
      read) rep_read=$value ;;
      gone) rep_gone=$value ;;
      barred) barred=$value ;;
      out) rep_out=$value ;;
      files) rep_files=$value ;;
      other) rep_other=$value ;;
      paths) rep_paths=$value ;;
      targets) rep_targets=$value ;;
      end) end=$value ;;
    esac
  done <"$work/lookup-report"
  # Every name of the lists it was given, and nothing left unsaid.
  shift
  for f in "$@"; do
    case "$f" in "$text"/*) ;; *) asked=$((asked + $(tr -c -d '\0' <"$f" | wc -c))) ;; esac
  done
  [ "$end" = 1 ] && [ -n "$rep_gone" ] && [ -n "$barred" ] && [ -n "$rep_out" ] && [ -n "$rep_files" ] &&
    [ -n "$rep_other" ] && [ -n "$rep_paths" ] && [ -n "$rep_targets" ] && [ "$rep_read" = "$asked" ] ||
    die "perl did not finish looking the files of the working tree up (it reports ${rep_read:-nothing} of $asked names); refusing to pass"
  [ "$barred" -eq 0 ] ||
    die "$barred files of the working tree could not be looked up, though they may be there (a folder that cannot be read or searched, or a path too long to open); they are not covered; refusing to pass"
}
# wrote <file> <lines|names>: fails unless the file holds as many lines, or NUL-ended
# names, as the last look_up reports it wrote.
wrote() {
  local held
  if [ "$2" = lines ]; then held=$(($(wc -l <"$1"))); else held=$(($(tr -c -d '\0' <"$1" | wc -c))); fi
  [ "$held" = "$rep_out" ] ||
    die "perl did not finish looking the files of the working tree up (it reports $rep_out $2 written, and $held are there); refusing to pass"
}

# holds <file> <bytes> <what>: fails unless the file holds as many bytes as the last
# `look_up kinds` reports it added to it. The file was not there before, and is not made
# when nothing is added. Bytes, not lines: a name can hold a newline.
holds() {
  local held=0
  [ ! -e "$1" ] || held=$(($(wc -c <"$1")))
  [ "$held" = "$2" ] ||
    die "perl did not finish looking the files of the working tree up (it reports $2 bytes of $3 written, and $held are there); refusing to pass"
}

# plain_names <file>: fails if any of the NUL-ended names in the file holds the byte \001
# or \002. The scan separates the fields of what it finds with \001, and holds the paths
# of commits as lines with a newline kept as \002: such a name would be taken for
# another, or its match lost. The names are counted and not shown.
plain_names() {
  local odd
  odd=$(tr '\n\0' ' \n' <"$1" | { grep -a -c -e "[$S"$'\002'"]" || true; })
  [ "$odd" -eq 0 ] ||
    die "$odd names hold the byte \\001 or \\002, which the scan uses to tell names and fields apart, so it cannot cover them; rename them; refusing to pass"
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

# sk_anywhere_is_key: of the found lines on stdin, each a match of $sk_anywhere, those
# where a part of the match after its first hyphen, between hyphens, is 20 characters or
# more and holds a digit or a capital.
sk_anywhere_is_key() {
  awk -F "$S" '{
    parts = split($NF, part, "-")
    for (i = 2; i <= parts; i++)
      if (length(part[i]) >= 20 && part[i] ~ /[0-9A-Z]/) { print; next }
  }'
}

secrets_here() {
  local args=() p n
  for p in "${patterns[@]}"; do args+=(-e "$p"); done
  # One search for every shape; a closer look only where that finds something.
  search -E "${args[@]}" -e "$sk_anywhere" -e "$email"
  if [ -s "$work/found" ]; then
    search -E "${args[@]}"
    hit <"$work/found"
    # -o: one found line per match, the match as its text.
    search -o -E -e "$sk_shape"
    { grep -a -E -e "$S.?($sk_body)[^$S]*\$" "$work/found" || true; } | hit
    search -o -E -e "$sk_anywhere"
    sk_anywhere_is_key <"$work/found" | hit
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
    contents | link-targets | pull-request) words_contents=$((words_contents + $2)) ;;
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

# pr_lines <file> <what>: the title or the body of a pull request against the attribution
# rules for text. Writes one line to the attribution hits for each line refused, and one
# to $work/to-read for each line that says one of the two phrases and is not refused.
#
# The candidates of a line are the line as it is and, for each part of it between table
# bars, the part from its first letter: every mark before that is taken off, and a ticked
# box with it. $work/candidates holds them one to a line and $work/candidate-lines the
# number of the line each came from, so a rule that is anchored at the start of a line is
# applied to each candidate, and what it finds is named by its line.
pr_lines() {
  local file=$1 what=$2 rc
  : >"$work/candidate-lines"
  awk -v numbers="$work/candidate-lines" '{
    print NR >numbers
    print
    cells = split($0, cell, "|")
    for (i = 1; i <= cells; i++) {
      c = cell[i]
      while (c != "") {
        if (c ~ /^\[[xX]\]/) c = substr(c, 4)
        else if (c ~ /^[^A-Za-z]/) c = substr(c, 2)
        else break
      }
      print NR >numbers
      print c
    }
  }' "$file" >"$work/candidates"
  [ "$(wc -l <"$work/candidates" | tr -d ' ')" = "$(wc -l <"$work/candidate-lines" | tr -d ' ')" ] ||
    die "the lines of the $what could not be taken apart; refusing to pass"
  : >"$work/refused-lines"
  pr_rule "$work/candidates" "$work/candidate-lines" "$credit_line" "$what" "$has_credit" -i
  pr_rule "$work/candidates" "$work/candidate-lines" "$agent_line" "$what" "$credits_agent" -i
  # A name, wherever it starts a word. The folder and the file this repo has are paths,
  # and only as those: the folder where it does not directly follow a letter or a digit
  # and is followed by a slash, a backtick, white space, the end of the line, or closing
  # punctuation other than a dot and a hyphen (so a host's name and a model's name stay
  # names); the file as a whole word in its exact case. The folder is taken out twice,
  # because one that ends where the next begins shares a character with it.
  folder='s/(^|[^A-Za-z0-9])\.claude([]/`[:space:])},;:!?>'"'"'"]|$)/\1.\2/g'
  sed -E -e "$folder" -e "$folder" -e 's/(^|[^A-Za-z0-9_])CLAUDE\.md([^A-Za-z0-9_]|$)/\1\2/g' "$file" >"$work/unnamed"
  [ "$(awk 'END { print NR }' "$work/unnamed")" = "$(awk 'END { print NR }' "$file")" ] ||
    die "the lines of the $what could not be read; refusing to pass"
  pr_rule "$work/unnamed" "" "$agent" "$what" "names an agent or a model" -i
  pr_rule "$file" "" "$tilde" "$what" "a path that starts with ~"
  # What is left to read: either phrase, anywhere in a line that was not refused.
  rc=0
  grep -a -n -i -E -e 'co-authored-by|generated[[:space:]]+with' "$file" >"$work/phrases" || rc=$?
  [ "$rc" -le 1 ] || die "a search failed (exit $rc); refusing to pass"
  cut -d: -f1 "$work/phrases" | REFUSED="$work/refused-lines" WHAT=$what awk '
    BEGIN { while ((getline n < ENVIRON["REFUSED"]) > 0) refused[n] = 1 }
    !($1 in refused) { print "to read: pull-request/" ENVIRON["WHAT"] ":" $1 }' >>"$work/to-read"
}
# pr_rule <file> <numbers file, or empty: a line is its own number> <pattern> <what>
# <problem> [-i]: one attribution hit for each line of the pull request's text that holds
# a line of the file matching the pattern.
pr_rule() {
  local rc=0
  grep -a -n ${6:+"$6"} -E -e "$3" "$1" >"$work/rule-found" || rc=$?
  [ "$rc" -le 1 ] || die "a search failed (exit $rc); refusing to pass"
  cut -d: -f1 "$work/rule-found" >"$work/rule-lines"
  if [ -n "$2" ]; then
    FOUND="$work/rule-lines" awk 'BEGIN { while ((getline n < ENVIRON["FOUND"]) > 0) found[n] = 1 }
      NR in found' "$2" >"$work/rule-numbers"
  else
    cp "$work/rule-lines" "$work/rule-numbers"
  fi
  sort -u -n "$work/rule-numbers" >"$work/rule-lines"
  cat "$work/rule-lines" >>"$work/refused-lines"
  WHAT=$4 PROBLEM=$5 awk '{ print "pull-request/" ENVIRON["WHAT"] ":" $1 ": " ENVIRON["PROBLEM"] }' \
    "$work/rule-lines" >>"$work/attribution-hits"
}

mkdir "$text/tree-paths" "$text/link-targets"
case "$mode" in
  text)
    mkdir "$text/pull-request"
    : >"$work/to-read"
    for what in title body; do
      if [ "$what" = title ]; then f=$pr_title; else f=$pr_body; fi
      [ -f "$f" ] && [ -r "$f" ] || die "cannot read the $what of the pull request (a file is expected); refusing to pass"
      cp -- "$f" "$text/pull-request/$what" || die "cannot read the $what of the pull request; refusing to pass"
      # A title with no line break at its end could join the first line of the body on the
      # pull request, and a credit could be made of the two halves: each file is whole.
      if [ "$what" = title ] && [ -s "$text/pull-request/title" ] && [ -n "$(tail -c 1 "$text/pull-request/title")" ]; then
        die "the title of the pull request does not end in a line break, so its last line could join the first of the body; end it with one; refusing to pass"
      fi
      # Plain text only: a NUL byte hides the rest of its line from a search.
      [ "$(tr -d '\0' <"$text/pull-request/$what" | wc -c | tr -d ' ')" = "$(wc -c <"$text/pull-request/$what" | tr -d ' ')" ] ||
        die "the $what of the pull request holds a NUL byte, so it is not plain text and cannot be scanned; refusing to pass"
      pr_lines "$text/pull-request/$what" "$what"
    done
    covered="the title and body of a pull request"
    ;;
  tree)
    files=0
    # What git says while it lists is not printed either: it names paths.
    git ls-files -z -co --exclude-standard >"$work/entries" 2>"$work/list-errors" ||
      die "the files of the working tree could not be listed (git ls-files failed); refusing to pass"
    [ ! -s "$work/list-errors" ] ||
      die "git said something while it listed the working tree ($(wc -l <"$work/list-errors" | tr -d ' ') lines of warnings or errors, such as a folder it could not read); what was not listed is not covered; refusing to pass"
    # What each listed name is: a file, a link (its target is searched as text), nothing
    # any more (deleted here and not yet committed: nothing to publish), or something the
    # scan cannot cover.
    look_up kinds "$work/entries" "$text/tree-paths/working-tree" "$text/link-targets/working-tree"
    holds "$text/tree-paths/working-tree" "$rep_paths" paths
    holds "$text/link-targets/working-tree" "$rep_targets" "link targets"
    [ "$rep_other" -eq 0 ] ||
      die "$rep_other paths that git lists are neither a file nor a link (a folder where a file is tracked, a submodule that is checked out, a repository inside this one); the scan cannot cover them; refusing to pass"
    files=$rep_files
    rc=0
    git ls-files -z -ci --exclude-standard >"$work/entries-ignored" 2>"$work/list-errors" || rc=$?
    [ "$rc" -eq 0 ] && [ ! -s "$work/list-errors" ] ||
      die "the tracked files that an ignore rule matches could not be listed (git ls-files: exit $rc, $(wc -l <"$work/list-errors" | tr -d ' ') lines of warnings or errors); refusing to pass"
    rc=0
    git ls-files -z -o --exclude-standard >"$work/entries-others" 2>"$work/list-errors" || rc=$?
    [ "$rc" -eq 0 ] && [ ! -s "$work/list-errors" ] ||
      die "the untracked files could not be listed (git ls-files: exit $rc, $(wc -l <"$work/list-errors" | tr -d ' ') lines of warnings or errors); refusing to pass"
    # The tracked files to search by name: the ones a rule matches that are there. Not
    # one that git's walk of the folders reaches as well, under another spelling of its
    # name (on macOS the index may hold a name decomposed and the walk compose it), and
    # not the same file twice: a match is then found once. (A link among them does no
    # harm: a search by name looks in regular files only.)
    look_up by-name "$work/entries-others" "$work/entries-ignored" >"$work/entries-by-name"
    wrote "$work/entries-by-name" names
    while IFS= read -r -d '' f; do ignored_tracked+=("$f"); done <"$work/entries-by-name"
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
      env ${caller_config[@]+"${caller_config[@]}"} git ls-remote "$url" >"$work/remote-refs" ||
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
for kind in link-targets tree-paths commit-paths commit-message ref-names tag-message pull-request; do
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
    args=(-e "$sk_anywhere")
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

if [ "$mode" = text ]; then
  if [ -s "$work/attribution-hits" ]; then
    echo "attribution check: a pull request credits no agent or model, names none, and gives no path that starts with ~:" >&2
    cat "$work/attribution-hits" >&2
    status=1
  else
    echo "attribution check: clean ($covered)"
  fi
  # Not refused, and not passed unread: each line that says one of the two phrases.
  cat "$work/to-read"
elif [ "$mode" != tree ]; then
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
  if [ "$mode" = text ]; then
    echo "clean-room scan: $count matching lines"
  elif [ "$mode" = tree ]; then
    echo "clean-room scan: $count matching lines (contents $words_contents, paths $words_paths)"
  else
    echo "clean-room scan: $count matching lines (contents $words_contents, paths $words_paths, messages $words_messages, ref names $words_refs, tag messages $words_tags)"
  fi
  [ "$count" -eq 0 ] || status=1
fi

done_ok=1
exit "$status"
}
main "$@"; exit
