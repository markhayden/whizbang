#!/usr/bin/env bash
# Puts in a folder the two things a user must have before `whizbang setup` can run (plan
# m4, task R1, item 4): `uv` at the pinned version, and the Python the pack pins. It is
# the bootstrap that goes before scripts/install-check.sh, and apart from it: this script
# downloads, and the install check contacts nothing.
#
#   scripts/install-prereqs.sh <uv> <folder>
#
#   <uv>       the `uv` to put there, by an absolute path. It must say the version
#              scripts/tool-versions.txt pins; another is refused and nothing is made. In
#              CI it is the one the pinned install action puts on every runner
#   <folder>   a folder that is not there yet, or is empty. Afterwards it holds two things
#              and nothing else: `uv`, a copy of the one given, and `python`, the folder
#              that `uv` keeps its Pythons in, with the pinned Python in it
#
# It does what the install guide (docs/guides/install.md) tells a user to do, and no more.
# The Python is installed by the copied `uv` with the guide's own command, which downloads
# it from GitHub's releases and from nowhere else: the origin CI already downloads it from
# (.github/workflows/ci.yml) and the one the setup guide names. No other place is contacted,
# and nothing is asked of `uv`'s maker's own host: without the mirror `uv` tries that first.
#
# `uv` is given a cleared environment: a home and a cache of this script's own, removed
# when it ends, no configuration, and the folder to install in. So nothing of whoever runs
# this is read, and nothing is written outside the folder.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "install-prereqs: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C

die() {
  echo "install-prereqs: $*" >&2
  exit 1
}
[ $# -eq 2 ] || { echo "usage: scripts/install-prereqs.sh <uv> <folder>" >&2; exit 2; }
uv=$1
folder=$2
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

pinned_uv=$(awk '$1 == "uv" { print $2 }' "$root/scripts/tool-versions.txt")
printf '%s\n' "$pinned_uv" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' >/dev/null ||
  die "scripts/tool-versions.txt must pin uv at one version"
pinned_python=$(tr -d ' \n' <"$root/pack/.python-version")
printf '%s\n' "$pinned_python" | grep -E '^3\.[0-9]+\.[0-9]+$' >/dev/null ||
  die "pack/.python-version must hold one version, as 3.12.15"

case "$uv" in /*) ;; *) die "uv must be named by an absolute path (it was: $uv)" ;; esac
[ -f "$uv" ] && [ -x "$uv" ] || die "there is no uv that can be run at $uv"
said=$(env -i "$uv" --version 2>/dev/null | sed -n '1p') || die "the uv at $uv did not say its version"
[ "$(printf '%s\n' "$said" | awk '{ print $1 " " $2 }')" = "uv $pinned_uv" ] ||
  die "the uv at $uv says '$said', and the pinned version is $pinned_uv (scripts/tool-versions.txt); nothing was made"

if [ -e "$folder" ]; then
  [ -d "$folder" ] && [ -z "$(ls -A "$folder")" ] || die "the folder must be empty or not there yet: $folder"
else
  mkdir -p "$folder"
fi
folder=$(cd "$folder" && pwd -P)

tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "install-prereqs: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT
mkdir "$tmp/home" "$tmp/cache"

cp "$uv" "$folder/uv"
chmod 755 "$folder/uv"

# The guide's own command, by the copy. GitHub's releases are the only place it is told of.
env -i HOME="$tmp/home" PATH=/usr/bin:/bin UV_NO_CONFIG=1 UV_CACHE_DIR="$tmp/cache" UV_PYTHON_INSTALL_DIR="$folder/python" \
  "$folder/uv" python install --no-config --no-bin --mirror https://github.com/astral-sh/python-build-standalone/releases/download "$pinned_python" ||
  die "uv could not install Python $pinned_python from GitHub's releases"

# The Python is there, runs, and is the pinned one.
python=
for found in "$folder"/python/*/bin/python3; do
  [ -x "$found" ] || continue
  if [ "$(env -i "$found" -I -c 'import platform; print(platform.python_version())' 2>/dev/null)" = "$pinned_python" ]; then
    python=$found
    break
  fi
done
[ -n "$python" ] || die "uv said it installed Python $pinned_python, and no Python of that version is in $folder/python"

[ "$(ls -A "$folder" | tr '\n' ' ')" = "python uv " ] ||
  die "the folder holds something else than uv and python: $(ls -A "$folder" | tr '\n' ' ')"
echo "install-prereqs: $folder holds uv $pinned_uv and Python $pinned_python"
done_ok=1
exit 0
}
main "$@"; exit
