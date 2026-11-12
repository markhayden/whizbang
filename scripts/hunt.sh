#!/usr/bin/env bash
# Temporary: loops the working-tree scan case on a CI runner to find why a file is
# sometimes missing from it. scripts/hunt.sh <job>; see .github/workflows/hunt.yml.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
job=$1
budget=${HUNT_SECONDS:-720}
out="$PWD/hunt-out"
mkdir -p "$out/logs"

echo "== environment"
git --version
which -a git
which -a bash
bash --version | head -n 1
/bin/bash --version | head -n 1
sw_vers 2>/dev/null || uname -a
echo "TMPDIR=${TMPDIR:-}"
mount 2>/dev/null | head -n 12
df -h "${TMPDIR:-/tmp}" . 2>/dev/null
sysctl -n hw.ncpu 2>/dev/null || nproc
git config --system --list 2>/dev/null | sed 's/^/system config: /'

# paths <raw>: the paths of the records in a `git grep -n -z` output.
paths() { LC_ALL=C tr '\0' '\001' <"$1" | LC_ALL=C awk -F "$(printf '\001')" 'NF >= 3 { print $1 }'; }

summary() {
  echo "== mismatches kept by the scan: $(ls -d "$out"/hunt/mismatch.* 2>/dev/null | wc -l | tr -d ' ') of $(wc -l <"$out/hunt/searches" 2>/dev/null | tr -d ' ') repeated searches"
  for d in "$out"/hunt/mismatch.*; do
    [ -d "$d" ] || continue
    echo "-- $d: $(cat "$d/status")"
    echo "   args: $(tr '\n' ' ' <"$d/args" | cut -c1-80)"
    for r in raw raw2 raw3 raw4; do paths "$d/$r" | sort >"$d/$r.paths"; done
    echo "   records: first $(wc -l <"$d/raw.paths" | tr -d ' ') second $(wc -l <"$d/raw2.paths" | tr -d ' ') one-thread $(wc -l <"$d/raw3.paths" | tr -d ' ') later $(wc -l <"$d/raw4.paths" | tr -d ' ')"
    echo "   in second, not first: $(comm -13 "$d/raw.paths" "$d/raw2.paths" | tr '\n' '|')"
    echo "   in first, not second: $(comm -23 "$d/raw.paths" "$d/raw2.paths" | tr '\n' '|')"
    echo "   in one-thread, not first: $(comm -13 "$d/raw.paths" "$d/raw3.paths" | tr '\n' '|')"
    echo "   in first, not one-thread: $(comm -23 "$d/raw.paths" "$d/raw3.paths" | tr '\n' '|')"
    echo "   stderr: $(cat "$d/err2" "$d/err3" | head -n 3 | tr '\n' '|')"
    echo "   files git lists: $(wc -l <"$d/ls-files" | tr -d ' '), files find lists: $(wc -l <"$d/find" | tr -d ' ')"
  done
}

# loop <at once> <command...>: runs the command, that many at once, until the time is up.
loop() {
  local width=$1 runs=0 failed=0 round=0 j pids
  shift
  local deadline=$((SECONDS + budget))
  while [ "$SECONDS" -lt "$deadline" ]; do
    round=$((round + 1))
    pids=()
    j=0
    while [ "$j" -lt "$width" ]; do
      "$@" >"$out/logs/$round.$j.log" 2>&1 &
      pids+=($!)
      j=$((j + 1))
    done
    j=0
    while [ "$j" -lt "$width" ]; do
      runs=$((runs + 1))
      if wait "${pids[$j]}"; then
        rm -f "$out/logs/$round.$j.log"
      else
        failed=$((failed + 1))
        echo "-- failed: round $round, number $j"
        grep -a -e '^missing from' -e 'second scan' -e '^failed:' -e '^files git lists' "$out/logs/$round.$j.log" | sed 's/^/   /'
      fi
      j=$((j + 1))
    done
  done
  echo "== $job: $runs runs, $failed failed"
}

export HUNT_DIR="$out/hunt"
mkdir -p "$HUNT_DIR"
planted=(bash scripts/tests/scan/planted.sh)
case "$job" in
  planted-par) loop 10 "${planted[@]}" ;;
  planted-serial) loop 1 "${planted[@]}" ;;
  planted-threads1)
    export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=grep.threads GIT_CONFIG_VALUE_0=1
    loop 10 "${planted[@]}"
    ;;
  planted-nocache)
    export GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=false \
      GIT_CONFIG_KEY_1=core.untrackedCache GIT_CONFIG_VALUE_1=false
    loop 10 "${planted[@]}"
    ;;
  planted-applegit)
    mkdir -p "$out/shim"
    ln -s /usr/bin/git "$out/shim/git"
    export PATH="$out/shim:$PATH"
    git --version
    loop 10 "${planted[@]}"
    ;;
  planted-plain) # the scan as it is: no repeated searches
    unset HUNT_DIR
    loop 10 "${planted[@]}"
    ;;
  testscan) loop 1 bash scripts/tests/test-scan.sh ;;
  parts-serial)
    serial() {
      local part
      for part in scripts/tests/scan/*.sh; do
        [ "${part##*/}" != common.sh ] || continue
        bash "$part" || return 1
      done
    }
    loop 1 serial
    ;;
  min | min-b) bash scripts/hunt-min.sh "$out" ;;
  *) echo "unknown job: $job"; exit 2 ;;
esac
summary
rm -rf "$out/shim"
exit 0
