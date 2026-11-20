#!/usr/bin/env bash
# The bootstrap of what a user must have before `whizbang setup` (scripts/install-prereqs.sh;
# plan m4, task R1, item 4): the pinned `uv` and the pinned Python, in one folder, by the
# install guide's own command, from the guide's one origin and no other.
#
# Every `uv` here is a stand-in, a short script of this test's that writes down what it
# was given and makes a folder where a Python would be: nothing is downloaded and nothing
# is contacted. The real bootstrap runs in CI's `release` job (`just install-check`).
source "$(dirname "$0")/lib.sh"

script=scripts/install-prereqs.sh
pinned_uv=$(awk '$1 == "uv" { print $2 }' scripts/tool-versions.txt)
pinned_python=$(tr -d ' \n' <pack/.python-version)
[ -n "$pinned_uv" ] && [ -n "$pinned_python" ] || fail "the repo pins uv and a Python"
mirror=https://github.com/astral-sh/python-build-standalone/releases/download

# stand_in <file> <version it says>: a `uv` that says that version; asked for anything
# else it writes its arguments and its environment under $tmp/record, and then makes a
# Python of the version it is asked for, unless a file there tells it otherwise.
mkdir "$tmp/record"
stand_in() {
  cat >"$1" <<SH
#!/bin/bash
if [ "\$*" = --version ]; then
  echo "uv $2 (a stand-in)"
  exit 0
fi
printf '%s\n' "\$@" >"$tmp/record/args"
env | sort >"$tmp/record/env"
[ ! -e "$tmp/record/fails" ] || { echo "uv: planted failure" >&2; exit 2; }
[ ! -e "$tmp/record/installs-nothing" ] || exit 0
says=\${!#}
[ ! -e "$tmp/record/installs-another" ] || says=3.11.0
bin="\$UV_PYTHON_INSTALL_DIR/cpython-\$says-stand-in/bin"
mkdir -p "\$bin"
printf '%s\n' '#!/bin/bash' "echo \$says" >"\$bin/python3"
chmod +x "\$bin/python3"
SH
  chmod +x "$1"
}
stand_in "$tmp/uv" "$pinned_uv"
stand_in "$tmp/uv-other" "0.0.1"

# --- The good case: the folder holds the two things and nothing else.
expect_pass "the bootstrap" "$script" "$tmp/uv" "$tmp/prereqs"
grep -F "holds uv $pinned_uv and Python $pinned_python" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "it says what the folder holds"; }
[ "$(ls -A "$tmp/prereqs" | tr '\n' ' ')" = "python uv " ] || fail "only uv and python are put in the folder (found: $(ls -A "$tmp/prereqs" | tr '\n' ' '))"
cmp -s "$tmp/uv" "$tmp/prereqs/uv" && [ -x "$tmp/prereqs/uv" ] || fail "the folder's uv is a copy of the one given, and can be run"
[ -x "$tmp/prereqs/python/cpython-$pinned_python-stand-in/bin/python3" ] || fail "the pinned Python is in the folder's python"

# The Python is asked of the guide's one origin and no other: the guide's own command,
# word for word, with the mirror the only address among its arguments.
want_args=$(printf '%s\n' python install --no-config --no-bin --mirror "$mirror" "$pinned_python")
[ "$(cat "$tmp/record/args")" = "$want_args" ] ||
  { cat "$tmp/record/args" >&2; fail "uv is started as: $(echo $want_args)"; }
[ "$(grep -c '://' "$tmp/record/args")" = 1 ] || fail "uv is told of one address"
if grep -E '://|MIRROR|INDEX|PROXY|proxy' "$tmp/record/env" >/dev/null; then
  cat "$tmp/record/env" >&2
  fail "no variable uv is given names another place to fetch from"
fi
guide_command="uv python install --no-config --no-bin --mirror $mirror $pinned_python"
grep -F -e "$guide_command" docs/guides/install.md >/dev/null || fail "docs/guides/install.md gives the command: $guide_command"
grep -F -e "$guide_command" docs/guides/setup.md >/dev/null || fail "docs/guides/setup.md gives the same command"
grep -F -e "uv\`, at version $pinned_uv" docs/guides/install.md >/dev/null || fail "docs/guides/install.md names the pinned uv, $pinned_uv"
grep -F -e "https://github.com/astral-sh/uv/releases/tag/$pinned_uv" docs/guides/install.md >/dev/null ||
  fail "docs/guides/install.md sends a user to uv's releases on GitHub for $pinned_uv"

# uv is given a cleared environment: a home and a cache of the script's own, no
# configuration, the folder to install in, and nothing of whoever runs the bootstrap.
for name in $(sed 's/=.*//' "$tmp/record/env"); do
  case "$name" in
    HOME | PATH | UV_NO_CONFIG | UV_CACHE_DIR | UV_PYTHON_INSTALL_DIR | PWD | SHLVL | _ | OLDPWD) ;;
    *) fail "uv is given a variable outside the five: $name" ;;
  esac
done
grep -x -F "UV_PYTHON_INSTALL_DIR=$(cd "$tmp/prereqs" && pwd -P)/python" "$tmp/record/env" >/dev/null || fail "uv installs into the folder's python"
grep -x -F "UV_NO_CONFIG=1" "$tmp/record/env" >/dev/null || fail "uv reads no configuration"
grep -x -F "PATH=/usr/bin:/bin" "$tmp/record/env" >/dev/null || fail "uv is given the system's path and no other"
if grep -x -F "HOME=$HOME" "$tmp/record/env" >/dev/null; then fail "uv is not given the home of whoever runs the bootstrap"; fi
home=$(sed -n 's/^HOME=//p' "$tmp/record/env")
cache=$(sed -n 's/^UV_CACHE_DIR=//p' "$tmp/record/env")
[ -n "$home" ] && [ ! -e "$home" ] && [ -n "$cache" ] && [ ! -e "$cache" ] || fail "the home and the cache uv was given are removed afterwards"

# --- A uv of another version is refused, and nothing is made or asked.
rm -f "$tmp/record/args"
expect_fail "a uv of another version" "the pinned version is $pinned_uv" "$script" "$tmp/uv-other" "$tmp/other"
[ ! -e "$tmp/other" ] || fail "nothing is made for a uv of another version"
[ ! -e "$tmp/record/args" ] || fail "a uv of another version is asked for nothing but its version"

# --- What else is refused before anything is made.
expect_fail "a uv named by a relative path" "must be named by an absolute path" "$script" uv "$tmp/relative"
expect_fail "no uv at the path" "there is no uv that can be run" "$script" "$tmp/no-such-uv" "$tmp/none"
mkdir "$tmp/used"
: >"$tmp/used/something"
expect_fail "a folder that holds something" "must be empty or not there yet" "$script" "$tmp/uv" "$tmp/used"
[ "$(ls -A "$tmp/used")" = something ] || fail "a folder that holds something is left as it was"
[ ! -e "$tmp/record/args" ] || fail "uv is asked for nothing when the folder is refused"
[ ! -e "$tmp/relative" ] && [ ! -e "$tmp/none" ] || fail "a refused bootstrap makes no folder"
status=0
"$script" "$tmp/uv" >"$tmp/out" 2>&1 || status=$?
[ "$status" = 2 ] || fail "one argument is a usage error (got $status)"
# An empty folder that is there already is taken.
mkdir "$tmp/empty"
expect_pass "an empty folder" "$script" "$tmp/uv" "$tmp/empty"

# --- A uv that fails, one that installs nothing, and one that installs another Python.
: >"$tmp/record/fails"
expect_fail "uv fails" "uv could not install Python $pinned_python from GitHub's releases" "$script" "$tmp/uv" "$tmp/failed"
rm "$tmp/record/fails"
: >"$tmp/record/installs-nothing"
expect_fail "uv installs nothing" "no Python of that version is in" "$script" "$tmp/uv" "$tmp/nothing"
rm "$tmp/record/installs-nothing"
: >"$tmp/record/installs-another"
expect_fail "uv installs another Python" "no Python of that version is in" "$script" "$tmp/uv" "$tmp/another"
rm "$tmp/record/installs-another"

finished
