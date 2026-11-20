#!/usr/bin/env bash
# Installs a release archive as the install guide tells a person to, and shows that what
# was installed runs (plan m4, task R1, item 5; SPEC.md section 13, "install from a release
# archive"). CI's `release` job runs it on each runner on the archive `just package` made
# (`just install-check`), and the `prerelease` workflow on a candidate's.
#
#   scripts/install-check.sh <archive> <sums> <prereqs folder>
#
#   <archive>          `whizbang-<version>-<target>.tar.gz`, for this machine's target
#   <sums>             the `SHA256SUMS` that holds the archive's line
#   <prereqs folder>   what scripts/install-prereqs.sh made: `uv`, and `python`, the folder
#                      `uv` keeps its Pythons in
#
# It contacts nothing, and does nothing a person with the guide could not. Everything it
# runs, it runs in a folder of its own that starts empty, with a cleared environment that
# holds four variables and no other: HOME, a new empty folder; TMPDIR, another; PATH, the
# folder the binary is installed in, the system's own folders (/usr/bin, /bin, /usr/sbin,
# /sbin) and the prerequisites' folder; and UV_PYTHON_INSTALL_DIR, the prerequisites'
# `python`, which is where a person's `uv` would have put the Python had they not been
# given a home that is empty. No Rust toolchain may be on that path. So nothing of the
# checkout, of the build or of whoever runs this can stand in for something the archive
# or the guide forgot. The checkout is read for two things only: this script, and the
# committed reference the installed binary is compared with.
#
# What it holds, in order, stopping at the first that fails:
#
#  1. <sums> holds exactly one line for the archive, whole (64 lower-case hexadecimal
#     digits, two spaces, its name), and the archive's SHA-256, taken here with the
#     program the guide names, is that line's;
#  2. the archive lists one folder and exactly the five files of item 1, each a regular
#     file and none a link, and unpacks;
#  3. `whizbang --version` gives the version the archive is named for, and a commit;
#  4. `whizbang describe --json --pretty` is docs/reference/description.json byte for
#     byte: the binary installed is the binary documented;
#  5. a run through the executor with no model: a picture is imported into a new store,
#     the recipe `mask_rect` is run on it, and `whizbang runs show` gives a completed run
#     that lists its two steps;
#  6. the server: `whizbang serve` answers `initialize`, lists its tools, answers one call
#     of `mask_rect`, and ends when its input closes. One that does not end within the
#     limit is killed and waited for, and the check fails; it is killed and waited for on
#     every other way out of this script too, and no wait for it is without a bound;
#  7. the pack: `whizbang setup` installs the pack's core, offline, and its worker says
#     hello;
#  8. `whizbang backends status` answers;
#  9. what this install cannot do is refused in its own words. With a config that names
#     the local segmenter, `whizbang rascal remove-prompt` is not ready and names the
#     segmenter: on Linux because SAM 3 does not run there, on a Mac because no model is
#     installed yet. On Linux `whizbang setup --extra torch` is refused before anything is
#     downloaded.
#     OWED (plan m4, task R1, item 5, last bullet): the same refusal of the loop's
#     command, `whizbang rascal remove-checked`, belongs here, marked `HOOK remove-checked`
#     below. The command is task L1's and is not built yet; whichever of L1 and R2 lands
#     second adds the lines, with their case in scripts/tests/test-install-check.sh.
#
# WHIZBANG_INSTALL_CHECK_LIMIT: the seconds the server has for each answer and for ending,
# a whole number from 1 to 30; 15 when not set (scripts/tests/test-install-check.sh
# lowers it).
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "install-check: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C

die() {
  echo "install-check: failed: $*" >&2
  exit 1
}
ok() { echo "install-check: ok: $*"; }
[ $# -eq 3 ] || { echo "usage: scripts/install-check.sh <archive> <sums> <prereqs folder>" >&2; exit 2; }
archive=$1
sums=$2
prereqs=$3
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
reference=$root/docs/reference/description.json

limit=${WHIZBANG_INSTALL_CHECK_LIMIT:-15}
case "$limit" in
  [1-9] | [12][0-9] | 30) ;;
  *) echo "install-check: WHIZBANG_INSTALL_CHECK_LIMIT must be a whole number of seconds from 1 to 30" >&2; exit 2 ;;
esac

[ -f "$archive" ] || die "there is no archive at $archive"
[ -f "$sums" ] || die "there is no SHA256SUMS at $sums"
[ -d "$prereqs" ] || die "there is no prerequisites' folder at $prereqs"
[ -f "$reference" ] || die "the committed reference is not there: $reference"
prereqs=$(cd "$prereqs" && pwd -P)
case "$archive" in /*) ;; *) archive=$PWD/$archive ;; esac
case "$sums" in /*) ;; *) sums=$PWD/$sums ;; esac

# This machine's target, and the archive's name held to it.
case "$(uname -s) $(uname -m)" in
  'Darwin arm64') target=aarch64-apple-darwin ;;
  'Linux x86_64') target=x86_64-unknown-linux-gnu ;;
  *) die "this machine ($(uname -s) on $(uname -m)) is neither of the two targets an archive is made for" ;;
esac
file=$(basename "$archive")
version=${file#whizbang-}
version=${version%-"$target".tar.gz}
printf '%s\n' "$version" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' >/dev/null && [ "$file" = "whizbang-$version-$target.tar.gz" ] ||
  die "the archive must be named whizbang-<version>-$target.tar.gz on this machine (it is: $file)"
name=whizbang-$version-$target

# The two things a user must have first (docs/guides/install.md).
[ -f "$prereqs/uv" ] && [ -x "$prereqs/uv" ] || die "the prerequisites' folder lacks uv ($prereqs/uv); scripts/install-prereqs.sh puts it there"
python=
for found in "$prereqs"/python/*/bin/python3; do
  [ ! -x "$found" ] || python=$found
done
[ -n "$python" ] || die "the prerequisites' folder lacks the Python ($prereqs/python); scripts/install-prereqs.sh puts it there"

work=$(mktemp -d)
work=$(cd "$work" && pwd -P)
server=
# stop_server: ends the server, if one was started and has not been waited for. Its input
# and output are closed first, so that one that only waits for its input ends by itself;
# then it is killed, and waited for, for the limit and no longer: this script never waits
# without a bound on a process it started, whatever that process does.
stop_server() {
  [ -n "$server" ] || return 0
  exec 3>&- 4<&-
  kill -9 "$server" 2>/dev/null || true
  local waited=0
  while kill -0 "$server" 2>/dev/null; do
    if [ "$waited" -ge $((limit * 10)) ]; then
      echo "install-check: the server (process $server) is still there $limit seconds after it was killed; it is left" >&2
      server=
      return 0
    fi
    sleep 0.1
    waited=$((waited + 1))
  done
  wait "$server" 2>/dev/null || true
  server=
}
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; stop_server; rm -rf "$work"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "install-check: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

mkdir "$work/home" "$work/tmp" "$work/bin" "$work/folder"
cd "$work/folder"
path=$work/bin:/usr/bin:/bin:/usr/sbin:/sbin:$prereqs
cleared=(HOME="$work/home" TMPDIR="$work/tmp" PATH="$path" UV_PYTHON_INSTALL_DIR="$prereqs/python")
# clean <command...>: the command with the cleared environment, in the folder.
clean() { env -i "${cleared[@]}" "$@"; }
for tool in cargo rustc rustup; do
  if clean /bin/sh -c "command -v $tool" >/dev/null 2>&1; then
    die "the path the check runs with holds a Rust toolchain ($tool), which an installed whizbang must not need"
  fi
done
# said <text>: the last command's output, kept in $work/out, holds the text.
said() { grep -F -e "$1" "$work/out" >/dev/null; }
# shown: the last command's output and complaint, for a failure's message.
shown() { cat "$work/out" "$work/err" 2>/dev/null | sed 's/^/    /' >&2; }

# --- 1. The SHA-256. Nothing is left to a checking program's reading of the line: Apple's
# sha256sum passes an empty input and a line it cannot read. So the line is held to its
# form here, the archive's digest is taken here, and the two are compared as texts.
# SHA256SUMS must hold exactly one line that is, whole: 64 lower-case hexadecimal digits,
# two spaces, and the archive's name; a line that only has the name inside it is no line
# of the archive's.
cp "$archive" "$work/folder/$file"
cp "$sums" "$work/folder/SHA256SUMS"
awk -v file="$file" 'length($0) == 66 + length(file) && substr($0, 1, 64) ~ /^[0-9a-f]+$/ && substr($0, 65, 2) == "  " && substr($0, 67) == file' \
  SHA256SUMS >"$work/line"
lines=$(wc -l <"$work/line" | tr -d ' ')
[ "$lines" != 0 ] || die "SHA256SUMS holds no line for $file (a line is 64 lower-case hexadecimal digits, two spaces and the archive's name)"
[ "$lines" = 1 ] || die "SHA256SUMS holds $lines lines for $file, and must hold one"
named=$(cut -c 1-64 "$work/line")
# The program the guide names for this platform.
case "$target" in
  aarch64-apple-darwin) clean shasum -a 256 "$file" >"$work/out" 2>"$work/err" || { shown; die "the archive's SHA-256 could not be taken"; } ;;
  *) clean sha256sum "$file" >"$work/out" 2>"$work/err" || { shown; die "the archive's SHA-256 could not be taken"; } ;;
esac
taken=$(cut -d' ' -f1 "$work/out")
printf '%s\n' "$taken" | grep -x -E '[0-9a-f]{64}' >/dev/null || { shown; die "the archive's SHA-256 could not be taken"; }
[ "$taken" = "$named" ] || die "the archive's SHA-256 is not the one SHA256SUMS names"
ok "the archive's SHA-256 is the one SHA256SUMS names"

# --- 2. What the archive holds, before and after it is unpacked.
want=$(printf '%s\n' "$name/" "$name/COPYRIGHT-library.html" "$name/LICENSE" "$name/README.md" "$name/THIRD-PARTY-LICENSES.md" "$name/whizbang")
listed=$(clean tar -tzf "$file" | sort)
[ "$listed" = "$want" ] ||
  { printf '%s\n' "$listed" | sed 's/^/    /' >&2; die "the archive holds something else than one folder with the five files (whizbang, LICENSE, THIRD-PARTY-LICENSES.md, COPYRIGHT-library.html, README.md)"; }
# One folder and then five regular files, and nothing that is a link of either kind:
# what a link in an archive points at is not the archive's to choose.
clean tar -tvzf "$file" >"$work/out" 2>"$work/err" || { shown; die "the archive could not be listed"; }
[ "$(cut -c 1 "$work/out" | tr -d '\n')" = "d-----" ] && ! grep -e ' -> ' -e ' link to ' "$work/out" >/dev/null ||
  { shown; die "the archive must hold one folder and then five regular files, and no link"; }
clean tar -xzf "$file"
unpacked=$(cd "$name" && find . | sort | tr '\n' ' ')
[ "$unpacked" = ". ./COPYRIGHT-library.html ./LICENSE ./README.md ./THIRD-PARTY-LICENSES.md ./whizbang " ] ||
  die "the unpacked folder holds something else than the five files: $unpacked"
for held in COPYRIGHT-library.html LICENSE README.md THIRD-PARTY-LICENSES.md whizbang; do
  [ -f "$name/$held" ] && [ ! -L "$name/$held" ] && [ -s "$name/$held" ] || die "$held of the archive is not a file with something in it"
done
[ -x "$name/whizbang" ] || die "the archive's binary cannot be run"
# The guide's last step: the binary in a folder that is on the path.
cp "$name/whizbang" "$work/bin/whizbang"
ok "the archive holds the five files, and the binary is on the path"

# --- 3. The version line.
clean whizbang --version >"$work/out" 2>"$work/err" || { shown; die "whizbang --version failed"; }
grep -x -E "whizbang $(printf '%s' "$version" | sed 's/\./\\./g') \(commit ([0-9a-f]{40}|[0-9a-f]{64})\)" "$work/out" >/dev/null ||
  { shown; die "whizbang --version must say: whizbang $version (commit <the commit it was built from>)"; }
ok "$(cat "$work/out")"

# --- 4. The binary installed is the binary documented.
clean whizbang describe --json --pretty >"$work/out" 2>"$work/err" || { shown; die "whizbang describe failed"; }
cmp -s "$work/out" "$reference" ||
  die "whizbang describe --json --pretty is not docs/reference/description.json: the archive is not of this checkout's build"
ok "whizbang describe is the committed reference"

# --- 5. A run through the executor, with no model. The picture: a PNG of 16 by 16, one colour.
printf '\211\120\116\107\015\012\032\012\000\000\000\015\111\110\104\122\000\000\000\020\000\000\000\020\010\002\000\000\000\220\221\150\066\000\000\000\026\111\104\101\124\170\332\143\210\252\230\106\022\142\030\325\060\252\141\370\152\000\000\047\131\150\020\173\352\055\111\000\000\000\000\111\105\116\104\256\102\140\202' >picture.png
clean whizbang store import --picture picture.png --store store --create >"$work/out" 2>"$work/err" ||
  { shown; die "whizbang store import --picture failed"; }
image=$(sed -n 's/^imported image \(sha256:[0-9a-f]\{64\}\)$/\1/p' "$work/out")
[ -n "$image" ] || { shown; die "the import printed no image"; }
clean whizbang rascal mask-rect --image "$image" --box "2 2 10 10" --store store >"$work/out" 2>"$work/err" ||
  { shown; die "the recipe mask_rect did not run"; }
run=$(sed -n 's/^run \([0-9][0-9]*-[0-9][0-9]*\) completed$/\1/p' "$work/out")
[ -n "$run" ] && said "mask sha256:" || { shown; die "mask_rect printed no completed run and mask"; }
clean whizbang runs show "$run" --store store >"$work/out" 2>"$work/err" || { shown; die "whizbang runs show failed"; }
said "action: mask_rect " && said "status: completed" && said "steps: 2" &&
  grep -E "^  call	[0-9-]+	rect	completed$" "$work/out" >/dev/null &&
  grep -E "^  call	[0-9-]+	rasterize	completed$" "$work/out" >/dev/null ||
  { shown; die "the run's record must show mask_rect completed, with its two steps completed"; }
ok "a picture was imported, mask_rect ran on it, and run $run is recorded as completed with its two steps"

# --- 6. The server over stdio.
mkfifo "$work/to-server" "$work/from-server"
env -i "${cleared[@]}" whizbang serve --store store <"$work/to-server" >"$work/from-server" 2>"$work/err" &
server=$!
exec 3>"$work/to-server" 4<"$work/from-server"
# A server that has ended is written to all the same: the write then fails and is
# reported here, where a signal would end this script with no word. Set after the server
# was started, so that it is not the server's setting too.
trap '' PIPE
# asked <what> <request>: sends one request and keeps the one line of its answer.
asked() {
  printf '%s\n' "$2" >&3 2>/dev/null || { sed 's/^/    /' "$work/err" >&2; die "the server gave no answer to $1: it had ended"; }
  answer=
  IFS= read -r -t "$limit" answer <&4 || { sed 's/^/    /' "$work/err" >&2; die "the server gave no answer to $1 within $limit seconds"; }
  printf '%s\n' "$answer" >"$work/out"
}
asked initialize '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"install-check","version":"1"}}}'
said '"id":0' && said '"protocolVersion":"2025-06-18"' && said '"serverInfo":{"name":"whizbang","version":"'"$version"'"}' ||
  { shown; die "the server's answer to initialize does not name whizbang $version"; }
printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}' >&3 2>/dev/null || die "the server ended after it answered initialize"
asked tools/list '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
said '"id":1' && said '"name":"mask_rect"' && said '"name":"check"' ||
  { cut -c 1-400 "$work/out" >&2; die "the server's tools do not hold mask_rect and check"; }
asked "a call of mask_rect" '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"mask_rect","arguments":{"image":"'"$image"'","box":"1 1 9 9"}}}'
said '"id":2' && said '"isError":false' && said ' completed' && said 'mask sha256:' ||
  { shown; die "the server's call of mask_rect did not complete with a mask"; }
# The end of its input ends it.
exec 3>&-
waited=0
while kill -0 "$server" 2>/dev/null; do
  if [ "$waited" -ge $((limit * 10)) ]; then
    stop_server
    die "the server did not end within $limit seconds of the end of its input; it was killed and waited for"
  fi
  sleep 0.1
  waited=$((waited + 1))
done
status=0
wait "$server" || status=$?
server=
exec 4<&-
trap - PIPE
[ "$status" -eq 0 ] || { sed 's/^/    /' "$work/err" >&2; die "the server ended with status $status"; }
ok "whizbang serve answered initialize, listed its tools, ran mask_rect, and ended with its input"

# --- 7. The pack's core, offline, with the uv and the Python of the prerequisites' folder.
clean whizbang setup >"$work/out" 2>"$work/err" || { shown; die "whizbang setup did not install the pack's core"; }
said "the pack is installed in $work/home/" && said "extra: none" && said "the worker it made said hello" ||
  { shown; die "whizbang setup must say where the pack is, inside the empty home, and that its worker said hello"; }
ok "whizbang setup installed the pack's core, and its worker said hello"

# --- 8. The backends, with no network.
clean whizbang backends status >"$work/out" 2>"$work/err" || { shown; die "whizbang backends status failed"; }
said "backends: none" || { shown; die "whizbang backends status must say that no config offers a backend"; }
ok "whizbang backends status answered"

# --- 9. What this install cannot do is refused in its own words.
printf '%s\n' '[backends.segment]' 'kind = "pack"' 'model = "sam3"' >config.toml
status=0
clean whizbang rascal remove-prompt --image "$image" --prompt "the corner" --until feather --store store --config config.toml \
  >"$work/out" 2>"$work/err" || status=$?
cat "$work/err" >>"$work/out"
case "$target" in
  x86_64-unknown-linux-gnu) why="its model is not ready: it does not run on this platform" ;;
  *) why="its model is not ready: it is not installed" ;;
esac
[ "$status" -eq 1 ] && said "remove_prompt cannot run now: it needs segment with phrase" &&
  said "the segment backend default is local" && said "$why" ||
  { shown; die "remove-prompt with the local segmenter must be refused as not ready (exit 1), naming the segmenter and saying: $why"; }
ok "remove-prompt on the local segmenter is not ready: $why"
# HOOK remove-checked: once task L1 has built the loop, the same call with `rascal
# remove-checked --image "$image" --prompt "the corner"` in the place of `remove-prompt
# ... --until feather` goes here, held to exit 1, to `remove_checked cannot run now` and
# to the same `$why`.
if [ "$target" = x86_64-unknown-linux-gnu ]; then
  status=0
  clean whizbang setup --extra torch >"$work/out" 2>"$work/err" || status=$?
  cat "$work/err" >>"$work/out"
  [ "$status" -eq 1 ] &&
    said "the extra torch is locked for macOS on Apple silicon only in this build, and this is linux on x86_64; nothing was downloaded and nothing was changed" ||
    { shown; die "whizbang setup --extra torch must be refused on Linux before anything is downloaded"; }
  ok "whizbang setup --extra torch is refused on Linux, and nothing was downloaded"
fi

echo "install-check: passed: $file installs and runs on $target"
done_ok=1
exit 0
}
main "$@"; exit
