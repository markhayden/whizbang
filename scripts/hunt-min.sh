#!/usr/bin/env bash
# Temporary: the smallest form of the working-tree scan. A throwaway repository with the
# planted files; the search the scan runs; its output against the same search's known
# output. scripts/hunt-min.sh <out folder>
set -uo pipefail
out=$1
mkdir -p "$out/min"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
base=$(mktemp -d)
each=${HUNT_MIN_SECONDS:-55}

key="sk""-abcdefghij0123456789ABCDEFGHIJ"
sk_shape='(^|[^A-Za-z0-9_-])sk-[A-Za-z0-9_-]{20,}'
all=(-E -e 'AKIA[0-9A-Z]{16}' -e '/Users/[A-Za-z0-9._-]+' -e "$sk_shape" -e '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}')

make_repo() { # <folder>
  git init -q -b main "$1"
  (
    cd "$1" || exit 1
    git config user.name "A Tester"
    git config user.email "tester@example.com"
    echo "hello" >readme.txt
    printf 'token = %s # ask someone@example.com\n' "$key" >al.txt
    mkdir -p "docs/noreply@x"
    printf 'token = %s\n' "$key" >"docs/noreply@x/k.txt"
    printf 'token = %s\n' "$key" >"x@example.txt"
    printf 'someone@example.com and someone@%s\n' "corp.test" >two.txt
    printf 'token = %s\n\0\n' "$key" >n.txt
    printf 'token = %s\n' "$key" | iconv -f UTF-8 -t UTF-16 >u.txt
    printf '*.cfg -diff\n*.dat binary\n' >.gitattributes
    printf 'token = %s\n' "$key" >x.cfg
    printf 'token = %s\n' "$key" >x.dat
    printf 'caf\351 token = %s\n' "$key" >l.txt
    printf 'token = %s\n' "$key" >"$(printf 'caf\303\251.txt')"
    printf 'token = %s\n' "$key" >'has"quote.txt'
    printf 'token = %s\n' "$key" >"$(printf 'two\nlines.txt')"
    for f in "with space.txt" "a:b.txt" "*.txt" "-n"; do printf 'token = %s\n' "$key" >"./$f"; done
    ln -s "/nowhere/notes" link-a
    ln -s "$key" link-b
    git add -A
    git commit -q -m "feat: planted"
  )
}

# The known output of each search, from a repository made once, agreed by five runs.
make_repo "$base/golden"
golden() { # <name> <git options and grep arguments...>
  local name=$1 i
  shift
  (cd "$base/golden" && "$@") >"$base/golden.$name"
  i=0
  while [ "$i" -lt 5 ]; do
    (cd "$base/golden" && "$@") | cmp -s - "$base/golden.$name" || { echo "the known output of $name is not stable"; exit 1; }
    i=$((i + 1))
  done
  echo "known output of $name: $(tr -d -c '\n' <"$base/golden.$name" | wc -c | tr -d ' ') lines"
}
golden sk git grep --untracked -a -n -z -o -E -e "$sk_shape"
golden all git grep --untracked -a -n -z "${all[@]}"
files_known=$(cd "$base/golden" && git ls-files -co --exclude-standard | wc -l | tr -d ' ')
found_known=$(cd "$base/golden" && find . -path ./.git -prune -o -type f -print | wc -l | tr -d ' ')
echo "files git lists: $files_known; files find lists: $found_known"

# check <tag> <repo> <which> <command...>: one search against its known output.
check() {
  local tag=$1 repo=$2 which=$3 rc=0 d
  shift 3
  (cd "$repo" && "$@") >"$base/raw.$tag" 2>"$base/err.$tag" || rc=$?
  if cmp -s "$base/raw.$tag" "$base/golden.$which" && [ ! -s "$base/err.$tag" ]; then return 0; fi
  d="$out/min/$tag.$RANDOM$RANDOM"
  mkdir -p "$d"
  cp "$base/raw.$tag" "$d/raw"
  cp "$base/err.$tag" "$d/err"
  cp "$base/golden.$which" "$d/golden"
  (cd "$repo" && "$@") >"$d/again" 2>&1
  {
    echo "rc=$rc which=$which"
    if cmp -s "$d/again" "$d/golden"; then echo "a second run is right"; else echo "a second run is wrong too"; fi
    echo "files git lists: $(cd "$repo" && git ls-files -co --exclude-standard | wc -l | tr -d ' ') (known $files_known)"
    echo "files find lists: $(cd "$repo" && find . -path ./.git -prune -o -type f -print | wc -l | tr -d ' ') (known $found_known)"
    echo "lines: $(tr -d -c '\n' <"$d/raw" | wc -c | tr -d ' ') (known $(tr -d -c '\n' <"$d/golden" | wc -c | tr -d ' '))"
  } >"$d/status"
  return 1
}

# The ways to run the search. Each is a function: <tag> <repo>.
v_default() { check "$1" "$2" sk git grep --untracked -a -n -z -o -E -e "$sk_shape" && check "$1" "$2" all git grep --untracked -a -n -z "${all[@]}"; }
v_threads1() { check "$1" "$2" sk git -c grep.threads=1 grep --untracked -a -n -z -o -E -e "$sk_shape" && check "$1" "$2" all git -c grep.threads=1 grep --untracked -a -n -z "${all[@]}"; }
v_tracked() { check "$1" "$2" sk git grep -a -n -z -o -E -e "$sk_shape" && check "$1" "$2" all git grep -a -n -z "${all[@]}"; }
v_nocache() { check "$1" "$2" sk git -c core.fsmonitor=false -c core.untrackedCache=false grep --untracked -a -n -z -o -E -e "$sk_shape"; }
v_applegit() { check "$1" "$2" sk /usr/bin/git grep --untracked -a -n -z -o -E -e "$sk_shape" && check "$1" "$2" all /usr/bin/git grep --untracked -a -n -z "${all[@]}"; }
v_applegit1() { check "$1" "$2" sk /usr/bin/git -c grep.threads=1 grep --untracked -a -n -z -o -E -e "$sk_shape"; }
v_listing() { # no search at all: do the two listings agree with what was made?
  local a b
  a=$(cd "$2" && git ls-files -co --exclude-standard | wc -l | tr -d ' ')
  b=$(cd "$2" && find . -path ./.git -prune -o -type f -print | wc -l | tr -d ' ')
  [ "$a" = "$files_known" ] && [ "$b" = "$found_known" ] || { mkdir -p "$out/min/listing.$RANDOM$RANDOM"; return 1; }
}

# run <label> <mode> <workers> <variant>: for $each seconds. Modes: static (one repository,
# made before), fresh (a new repository before every search), synced (fresh, then sync and
# a short sleep before the search).
run() {
  local label=$1 mode=$2 workers=$3 variant=$4 w
  : >"$base/count.$label"
  w=0
  local pids=()
  while [ "$w" -lt "$workers" ]; do
    (
      n=0 bad=0
      deadline=$((SECONDS + each))
      repo="$base/golden"
      while [ "$SECONDS" -lt "$deadline" ]; do
        if [ "$mode" != static ]; then
          repo="$base/$label.$w"
          rm -rf "$repo"
          make_repo "$repo"
          [ "$mode" != synced ] || { sync; sleep 0.2; }
        fi
        n=$((n + 1))
        "$variant" "$label.$w" "$repo" || bad=$((bad + 1))
      done
      echo "$n $bad" >>"$base/count.$label"
    ) &
    pids+=($!)
    w=$((w + 1))
  done
  wait "${pids[@]}"
  awk -v label="$label" '{ n += $1; bad += $2 } END { printf "== min %-28s %7d searches, %d wrong\n", label, n, bad }' "$base/count.$label"
}

run static-default-1 static 1 v_default
run static-default-8 static 8 v_default
run static-threads1-8 static 8 v_threads1
run static-tracked-8 static 8 v_tracked
run static-nocache-8 static 8 v_nocache
run static-applegit-8 static 8 v_applegit
run static-applegit1-8 static 8 v_applegit1
run fresh-default-1 fresh 1 v_default
run fresh-default-8 fresh 8 v_default
run fresh-threads1-8 fresh 8 v_threads1
run fresh-tracked-8 fresh 8 v_tracked
run fresh-applegit-8 fresh 8 v_applegit
run fresh-listing-8 fresh 8 v_listing
run synced-default-8 synced 8 v_default

echo "== min: what was kept"
for d in "$out"/min/*/; do
  [ -f "$d/status" ] || continue
  echo "-- $d: $(tr '\n' ';' <"$d/status")"
  echo "   stderr: $(head -n 2 "$d/err" | tr '\n' '|')"
  echo "   missing or changed lines, paths only:"
  diff <(tr '\0' '\001' <"$d/golden" | cut -d "$(printf '\001')" -f 1) <(tr '\0' '\001' <"$d/raw" | cut -d "$(printf '\001')" -f 1) | head -n 8 | sed 's/^/     /'
done | head -n 200
rm -rf "$base"
