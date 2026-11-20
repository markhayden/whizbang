#!/usr/bin/env bash
# The install check (scripts/install-check.sh; plan m4, task R1, item 5): it passes on a
# good archive, in an empty home, with a cleared environment and only the prerequisites'
# folder; and it fails on each thing a release must never ship with.
#
# The binary here is a tiny stand-in, a shell script named `whizbang` that answers each
# command the check gives as the real one does, and the prerequisites are stand-ins too:
# nothing is built, installed or contacted (plan m4, "Keeping tests lean (R1)"). The real
# check runs on the real archive in CI's `release` job on each runner (`just
# install-check`). The stand-in writes down the environment and the folder it was first
# started in, which is how this test sees what the check gives an installed binary.
source "$(dirname "$0")/lib.sh"

script=scripts/install-check.sh
case "$(uname -s) $(uname -m)" in
  'Darwin arm64') target=aarch64-apple-darwin; unready="it is not installed" ;;
  'Linux x86_64') target=x86_64-unknown-linux-gnu; unready="it does not run on this platform" ;;
  *) fail "this machine is neither of the two targets" ;;
esac
version=1.2.3
# Not `name`: that is a variable of lib.sh's own functions, which call `check` below.
# Not `name`: lib.sh's `expect_pass` and `expect_fail` have a variable of that name, and
# `check` below is called by them.
stem=whizbang-$version-$target
commit=0123456789abcdef0123456789abcdef01234567
image=sha256:$(printf 'a%.0s' 1 2 3 4 5 6 7 8)$(printf 'b%.0s' 1 2 3 4 5 6 7 8)$(printf 'c%.0s' $(seq 1 48))
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
mkdir "$tmp/record"

# stand_in <file> <kind> [<version>]: the stand-in binary. The kinds: `good`; `stays`,
# whose server does not end when its input does; `silent`, whose server answers nothing;
# `dies`, whose server ends at once; `undocumented`, whose description is not the committed one; `unrefused`, which runs
# what needs a local model; `no-steps`, whose run lists no step.
stand_in() {
  local kind=$2 says=${3:-$version}
  cat >"$1" <<SH
#!/bin/bash
kind=$kind
record="$tmp/record"
if [ ! -e "\$record/env" ]; then
  env | sort >"\$record/env"
  pwd -P >"\$record/folder"
  ls -A >"\$record/in-folder"
  ls -A "\$HOME" >"\$record/in-home"
fi
printf '%s\n' "\$*" >>"\$record/commands"
case "\$*" in
  --version) echo "whizbang $says (commit $commit)" ;;
  'describe --json --pretty')
    cat "$root/docs/reference/description.json"
    [ "\$kind" != undocumented ] || echo "{}"
    ;;
  'store import --picture picture.png --store store --create')
    [ -s picture.png ] || { echo "whizbang: cannot read picture.png" >&2; exit 1; }
    mkdir store
    printf '%s\n' "created a store at \$PWD/store" "imported image $image"
    ;;
  "rascal mask-rect --image $image --box 2 2 10 10 --store store")
    [ -d store ] || exit 1
    printf '%s\n' 'run 100-1 completed' 'step rect rect run 100-2' 'step rasterize rasterize run 100-3' 'mask sha256:dd'
    ;;
  'runs show 100-1 --store store')
    printf '%s\n' 'run: 100-1' 'action: mask_rect 1.0.0' 'status: completed'
    if [ "\$kind" = no-steps ]; then
      echo 'steps: none'
    else
      printf '%s\n' 'steps: 2'
      printf '  call\t100-2\trect\tcompleted\n  call\t100-3\trasterize\tcompleted\n'
    fi
    ;;
  'serve --store store')
    echo "\$\$" >"\$record/server"
    [ "\$kind" != dies ] || exit 0
    while IFS= read -r line; do
      [ "\$kind" != silent ] || continue
      case "\$line" in
        *'"method":"initialize"'*) echo '{"id":0,"jsonrpc":"2.0","result":{"capabilities":{"tools":{"listChanged":false}},"protocolVersion":"2025-06-18","serverInfo":{"name":"whizbang","version":"$says"}}}' ;;
        *'"method":"tools/list"'*) echo '{"id":1,"jsonrpc":"2.0","result":{"tools":[{"name":"check"},{"name":"mask_rect"}]}}' ;;
        *'"method":"tools/call"'*'"name":"mask_rect"'*"$image"*) echo '{"id":2,"jsonrpc":"2.0","result":{"content":[{"text":"run 100-4 completed\nmask sha256:ee\n","type":"text"}],"isError":false}}' ;;
      esac
    done
    # Its input has ended. One that stays ends itself after a minute and not before.
    [ "\$kind" != stays ] || exec sleep 60
    ;;
  setup)
    command -v uv >/dev/null || { echo "whizbang: uv was not found on PATH" >&2; exit 1; }
    [ -d "\$UV_PYTHON_INSTALL_DIR" ] || { echo "whizbang: no Python" >&2; exit 1; }
    printf '%s\n' "the pack is installed in \$HOME/models/pack" 'extra: none' 'the worker it made said hello (lifetime 120 s): device cpu, 3 threads'
    ;;
  'backends status') echo 'backends: none: no config was named, and the empty config offers none' ;;
  "rascal remove-prompt --image $image --prompt the corner --until feather --store store --config config.toml")
    grep -x -F 'model = "sam3"' config.toml >/dev/null || exit 3
    [ "\$kind" != unrefused ] || { echo 'run 100-5 completed'; exit 0; }
    echo "whizbang: remove_prompt cannot run now: it needs segment with phrase, and the segment backend default is local, and its model is not ready: $unready" >&2
    exit 1
    ;;
  'setup --extra torch')
    echo 'whizbang: the extra torch is locked for macOS on Apple silicon only in this build, and this is linux on x86_64; nothing was downloaded and nothing was changed (with no extra, setup installs the pack'"'"'s core, which needs no package)' >&2
    exit 1
    ;;
  *) echo "the stand-in was asked what it does not know: \$*" >&2; exit 64 ;;
esac
SH
  chmod +x "$1"
}

# archive <folder> <kind> [<binary's version>] [<extra file>]: an archive of the stand-in
# and its SHA256SUMS in that folder, named for $version whatever the binary says.
archive() {
  local to=$1 stage="$tmp/stage"
  rm -rf "$stage" "$to"
  mkdir -p "$stage/$stem" "$to"
  stand_in "$stage/$stem/whizbang" "$2" "${3:-$version}"
  echo "the licence" >"$stage/$stem/LICENSE"
  echo "the install guide" >"$stage/$stem/README.md"
  echo "the licences of others" >"$stage/$stem/THIRD-PARTY-LICENSES.md"
  echo "the notices of the standard library" >"$stage/$stem/COPYRIGHT-library.html"
  [ -z "${4:-}" ] || echo "one file too many" >"$stage/$stem/$4"
  (cd "$stage" && tar -czf "$to/$stem.tar.gz" "$stem")
  printf '%s  %s\n' "$(sha256 "$to/$stem.tar.gz")" "$stem.tar.gz" >"$to/SHA256SUMS"
}

# The prerequisites' folder: a uv, and a Python where uv keeps them.
prereqs="$tmp/prereqs"
mkdir -p "$prereqs/python/cpython-3.12.15-stand-in/bin"
printf '%s\n' '#!/bin/bash' 'echo "uv stand-in"' >"$prereqs/uv"
printf '%s\n' '#!/bin/bash' 'echo "3.12.15"' >"$prereqs/python/cpython-3.12.15-stand-in/bin/python3"
chmod +x "$prereqs/uv" "$prereqs/python/cpython-3.12.15-stand-in/bin/python3"

# check <folder>: the check on that folder's archive, with one second for the server. The
# check itself is given 30 seconds and then ended by a signal, so that a check that waits
# on a server it did not kill fails this test and does not hang it (a good one takes two).
check() {
  WHIZBANG_INSTALL_CHECK_LIMIT=1 perl -e 'alarm shift; exec @ARGV or die "cannot start $ARGV[0]\n"' 30 \
    "$script" "$1/$stem.tar.gz" "$1/SHA256SUMS" "${2:-$prereqs}"
}
# gone <pid>: the process is no longer there.
gone() { ! kill -0 "$1" 2>/dev/null; }

# --- A good archive passes, with an empty home and only the prerequisites' folder. A
# variable of whoever runs the check, and a file in their folder, reach nothing.
archive "$tmp/good" good
rm -rf "$tmp/record" && mkdir "$tmp/record"
WHIZBANG_MADE_UP=of-the-caller expect_pass "a good archive" check "$tmp/good"
grep -x -F "install-check: passed: $stem.tar.gz installs and runs on $target" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "it says that the archive installs and runs"; }
grep -x -F "install-check: ok: whizbang $version (commit $commit)" "$tmp/out" >/dev/null || fail "it prints the version line it held"
for name_of in $(sed 's/=.*//' "$tmp/record/env"); do
  case "$name_of" in
    HOME | TMPDIR | PATH | UV_PYTHON_INSTALL_DIR | PWD | SHLVL | _ | OLDPWD) ;;
    *) fail "the installed binary is given a variable outside the four: $name_of" ;;
  esac
done
[ ! -s "$tmp/record/in-home" ] || fail "the home the binary is first started with is empty"
folder=$(cat "$tmp/record/folder")
home=$(sed -n 's/^HOME=//p' "$tmp/record/env")
[ "$home" != "$HOME" ] && [ ! -e "$home" ] && [ ! -e "$folder" ] || fail "the home and the folder are the check's own, and are removed afterwards"
[ "$(tr '\n' ' ' <"$tmp/record/in-folder")" = "SHA256SUMS $stem $stem.tar.gz " ] ||
  fail "the folder holds the download and what it unpacked to, and nothing else (it held: $(tr '\n' ' ' <"$tmp/record/in-folder"))"
bin=${folder%/folder}/bin
grep -x -F "PATH=$bin:/usr/bin:/bin:/usr/sbin:/sbin:$(cd "$prereqs" && pwd -P)" "$tmp/record/env" >/dev/null ||
  { grep '^PATH=' "$tmp/record/env" >&2; fail "the path is the installed binary's folder, the system's own folders and the prerequisites' folder"; }
grep -x -F "UV_PYTHON_INSTALL_DIR=$(cd "$prereqs" && pwd -P)/python" "$tmp/record/env" >/dev/null || fail "uv's Pythons are the prerequisites' folder's"
# Every command of the check was given, the server's and the refusals among them.
for asked in --version 'describe --json --pretty' 'store import --picture picture.png --store store --create' \
  "rascal mask-rect --image $image --box 2 2 10 10 --store store" 'runs show 100-1 --store store' 'serve --store store' \
  setup 'backends status' "rascal remove-prompt --image $image --prompt the corner --until feather --store store --config config.toml"; do
  grep -x -F -e "$asked" "$tmp/record/commands" >/dev/null || fail "the check runs: whizbang $asked"
done
if [ "$target" = x86_64-unknown-linux-gnu ]; then
  grep -x -F 'setup --extra torch' "$tmp/record/commands" >/dev/null || fail "on Linux the check asks for the extra, to see it refused"
else
  grep -x -F 'setup --extra torch' "$tmp/record/commands" >/dev/null && fail "on a Mac the check installs no extra"
fi
gone "$(cat "$tmp/record/server")" || fail "the server of a good archive has ended"

# --- The prerequisites' folder without uv, or without the Python.
cp -R "$prereqs" "$tmp/no-uv" && rm "$tmp/no-uv/uv"
expect_fail "prerequisites without uv" "the prerequisites' folder lacks uv" check "$tmp/good" "$tmp/no-uv"
cp -R "$prereqs" "$tmp/no-python" && rm -r "$tmp/no-python/python"
expect_fail "prerequisites without the Python" "the prerequisites' folder lacks the Python" check "$tmp/good" "$tmp/no-python"
# A Rust toolchain on the path the check would run with.
cp -R "$prereqs" "$tmp/with-cargo" && cp "$prereqs/uv" "$tmp/with-cargo/cargo"
expect_fail "a Rust toolchain on the path" "holds a Rust toolchain (cargo)" check "$tmp/good" "$tmp/with-cargo"

# --- An archive whose sum is wrong: nothing of it is unpacked or run.
cp -R "$tmp/good" "$tmp/wrong-sum"
printf '%s  %s\n' "$(printf '0%.0s' $(seq 1 64))" "$stem.tar.gz" >"$tmp/wrong-sum/SHA256SUMS"
rm -rf "$tmp/record" && mkdir "$tmp/record"
expect_fail "a wrong sum" "the archive's SHA-256 is not the one SHA256SUMS names" check "$tmp/wrong-sum"
[ ! -e "$tmp/record/commands" ] || fail "an archive whose sum is wrong is not run"
# A line that is not, whole, 64 lower-case hexadecimal digits, two spaces and the archive's
# name is no line of the archive's, whatever a checking program would make of it (Apple's
# sha256sum passes an empty input and a line it cannot read): a hash that is short, of
# another length or in capitals; the name alone; the name inside another name; one space;
# no line at all. And two lines for the archive are one too many, though both are right.
right=$(sha256 "$tmp/good/$stem.tar.gz")
no_line() { # <name> <line...>
  local case_name=$1
  shift
  printf '%s\n' "$@" >"$tmp/wrong-sum/SHA256SUMS"
  rm -rf "$tmp/record" && mkdir "$tmp/record"
  expect_fail "$case_name" "SHA256SUMS holds no line for $stem.tar.gz" check "$tmp/wrong-sum"
  [ ! -e "$tmp/record/commands" ] || fail "$case_name: the archive is not run"
}
no_line "a short hash" "deadbeef  $stem.tar.gz"
no_line "a hash of 40 digits" "$(printf '%s' "$right" | cut -c 1-40)  $stem.tar.gz"
no_line "a hash in capitals" "$(printf '%s' "$right" | tr 'a-f' 'A-F' | sed 's/^./F/')  $stem.tar.gz"
no_line "the name alone" "$stem.tar.gz"
no_line "the name inside another name" "$right  $stem.tar.gz.sig" "$right  old-$stem.tar.gz"
no_line "one space" "$right $stem.tar.gz"
no_line "a line of something else" "no line of it"
: >"$tmp/wrong-sum/SHA256SUMS"
expect_fail "an empty SHA256SUMS" "SHA256SUMS holds no line for $stem.tar.gz" check "$tmp/wrong-sum"
printf '%s\n' "$right  $stem.tar.gz" "$right  $stem.tar.gz" >"$tmp/wrong-sum/SHA256SUMS"
expect_fail "two lines for the archive" "SHA256SUMS holds 2 lines for $stem.tar.gz, and must hold one" check "$tmp/wrong-sum"
# The right line among lines for other files is taken.
printf '%s\n' "$right  other.tar.gz" "$right  $stem.tar.gz" "$right  $stem.tar.gz.sig" >"$tmp/wrong-sum/SHA256SUMS"
expect_pass "the archive's line among others" check "$tmp/wrong-sum"

# --- A file too many, or one too few: nothing of it is run.
archive "$tmp/extra" good "$version" notes.txt
rm -rf "$tmp/record" && mkdir "$tmp/record"
expect_fail "a file too many" "the archive holds something else than one folder with the five files" check "$tmp/extra"
[ ! -e "$tmp/record/commands" ] || fail "an archive with a file too many is not run"
rm -rf "$tmp/stage" "$tmp/fewer" && mkdir -p "$tmp/stage/$stem" "$tmp/fewer"
stand_in "$tmp/stage/$stem/whizbang" good
echo "the licence" >"$tmp/stage/$stem/LICENSE"
echo "the install guide" >"$tmp/stage/$stem/README.md"
(cd "$tmp/stage" && tar -czf "$tmp/fewer/$stem.tar.gz" "$stem")
printf '%s  %s\n' "$(sha256 "$tmp/fewer/$stem.tar.gz")" "$stem.tar.gz" >"$tmp/fewer/SHA256SUMS"
expect_fail "a file too few" "the archive holds something else than one folder with the five files" check "$tmp/fewer"
# The four files an archive held before the standard library's notices were carried.
echo "the licences of others" >"$tmp/stage/$stem/THIRD-PARTY-LICENSES.md"
rm "$tmp/fewer/$stem.tar.gz"
(cd "$tmp/stage" && tar -czf "$tmp/fewer/$stem.tar.gz" "$stem")
printf '%s  %s\n' "$(sha256 "$tmp/fewer/$stem.tar.gz")" "$stem.tar.gz" >"$tmp/fewer/SHA256SUMS"
rm -rf "$tmp/record" && mkdir "$tmp/record"
expect_fail "an archive without the standard library's notices" "the archive holds something else than one folder with the five files" check "$tmp/fewer"
[ ! -e "$tmp/record/commands" ] || fail "an archive without the notices is not run"

# --- A link in the archive, of either kind, in the place of a file: nothing is unpacked.
for kind in symbolic hard; do
  rm -rf "$tmp/stage" "$tmp/linked" && mkdir -p "$tmp/stage/$stem" "$tmp/linked"
  stand_in "$tmp/stage/$stem/whizbang" good
  echo "the licence" >"$tmp/stage/$stem/LICENSE"
  echo "the install guide" >"$tmp/stage/$stem/README.md"
  echo "the notices of the standard library" >"$tmp/stage/$stem/COPYRIGHT-library.html"
  if [ "$kind" = symbolic ]; then
    ln -s LICENSE "$tmp/stage/$stem/THIRD-PARTY-LICENSES.md"
  else
    ln "$tmp/stage/$stem/LICENSE" "$tmp/stage/$stem/THIRD-PARTY-LICENSES.md"
  fi
  (cd "$tmp/stage" && tar -czf "$tmp/linked/$stem.tar.gz" "$stem")
  printf '%s  %s\n' "$(sha256 "$tmp/linked/$stem.tar.gz")" "$stem.tar.gz" >"$tmp/linked/SHA256SUMS"
  rm -rf "$tmp/record" && mkdir "$tmp/record"
  expect_fail "a $kind link in the archive" "the archive must hold one folder and then five regular files, and no link" check "$tmp/linked"
  [ ! -e "$tmp/record/commands" ] || fail "an archive with a $kind link is not run"
done

# --- A binary of another version than the archive is named for.
archive "$tmp/other-version" good 1.2.4
expect_fail "a binary of another version" "whizbang --version must say: whizbang $version (commit" check "$tmp/other-version"
# An archive named for another target, or with no version.
cp "$tmp/good/$stem.tar.gz" "$tmp/good/whizbang-$version-riscv64-unknown-linux-gnu.tar.gz"
expect_fail "an archive for another target" "the archive must be named whizbang-<version>-$target.tar.gz" \
  "$script" "$tmp/good/whizbang-$version-riscv64-unknown-linux-gnu.tar.gz" "$tmp/good/SHA256SUMS" "$prereqs"

# --- A server that does not end when its input does is killed and waited for, and the
# check fails. So is one that never answers.
archive "$tmp/stays" stays
rm -rf "$tmp/record" && mkdir "$tmp/record"
expect_fail "a server that does not end" "the server did not end within 1 seconds of the end of its input; it was killed and waited for" check "$tmp/stays"
gone "$(cat "$tmp/record/server")" || fail "a server that does not end is killed"
archive "$tmp/silent" silent
rm -rf "$tmp/record" && mkdir "$tmp/record"
expect_fail "a server that never answers" "the server gave no answer to initialize within 1 seconds" check "$tmp/silent"
gone "$(cat "$tmp/record/server")" || fail "a server that never answers is killed"
# The script never waits on its server without a bound: each wait counts to the limit.
[ "$(grep -c -x -F '  wait "$server" 2>/dev/null || true' "$script")" = 1 ] && [ "$(grep -c -F 'wait "$server"' "$script")" = 2 ] &&
  [ "$(grep -c -F 'if [ "$waited" -ge $((limit * 10)) ]; then' "$script")" = 2 ] ||
  fail "the check waits for its server in two places, each after a loop that counts to the limit"

# A server that ends before it is asked anything is reported in words, not by a signal.
archive "$tmp/dies" dies
expect_fail "a server that ends at once" "the server gave no answer to initialize" check "$tmp/dies"
grep -F "install-check: failed:" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "a server that ends at once is reported by the check itself"; }

# --- A binary that is not the one documented; a run that lists no step; a binary that
# runs what needs a local model.
archive "$tmp/undocumented" undocumented
expect_fail "a binary that is not the one documented" "whizbang describe --json --pretty is not docs/reference/description.json" check "$tmp/undocumented"
archive "$tmp/no-steps" no-steps
expect_fail "a run that lists no step" "the run's record must show mask_rect completed, with its two steps completed" check "$tmp/no-steps"
archive "$tmp/unrefused" unrefused
expect_fail "what needs a local model is not refused" "remove-prompt with the local segmenter must be refused as not ready" check "$tmp/unrefused"

# --- The limit, and the arguments.
for bad in 0 31 1.5 x; do
  status=0
  WHIZBANG_INSTALL_CHECK_LIMIT=$bad "$script" "$tmp/good/$stem.tar.gz" "$tmp/good/SHA256SUMS" "$prereqs" >"$tmp/out" 2>&1 || status=$?
  [ "$status" = 2 ] && grep -F "must be a whole number of seconds from 1 to 30" "$tmp/out" >/dev/null || fail "the limit '$bad' is refused"
done
status=0
"$script" "$tmp/good/$stem.tar.gz" "$tmp/good/SHA256SUMS" >"$tmp/out" 2>&1 || status=$?
[ "$status" = 2 ] || fail "two arguments are a usage error (got $status)"

finished
