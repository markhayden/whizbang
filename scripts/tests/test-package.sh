#!/usr/bin/env bash
# The release archive and its line of SHA256SUMS (scripts/package.sh; plan m4, task R1,
# item 1): what the archive holds, that the same tree gives the same archive, and that it
# is never made of a binary that is not this checkout's.
#
# The binary here is a stand-in, a script that prints a version line: nothing is built, so
# this is no release build (plan m4, "Keeping tests lean (R1)"). The real binary is
# packaged in CI's `release` job on each runner (`just package`), and installed there.
# Nothing uses the network; the licence texts are read from cargo's home, where `just
# deny` put the crates: this test runs after it in `just check`.
source "$(dirname "$0")/lib.sh"

script=scripts/package.sh
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | sed -n '1p')
commit=$(git rev-parse HEAD)
target=$(rustc -vV | sed -n 's/^host: //p')
case "$target" in
  aarch64-apple-darwin | x86_64-unknown-linux-gnu) ;;
  *) fail "this machine's target is $target, and an archive is made for the two targets of SPEC.md section 5 only" ;;
esac
name=whizbang-$version-$target
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# stand_in <file> <version line>: a binary that prints that line for --version.
stand_in() {
  printf '%s\n' '#!/bin/bash' "[ \"\$*\" = --version ] && echo \"$2\"" >"$1"
  chmod +x "$1"
}
stand_in "$tmp/whizbang" "whizbang $version (commit $commit)"

# --- The archive holds one folder with exactly the five files.
expect_pass "the archive" "$script" --binary "$tmp/whizbang" --out "$tmp/a"
[ "$(ls -A "$tmp/a" | tr '\n' ' ')" = "SHA256SUMS $name.tar.gz " ] || fail "the folder holds the archive and SHA256SUMS (found: $(ls -A "$tmp/a" | tr '\n' ' '))"
listed=$(tar -tzf "$tmp/a/$name.tar.gz" | sed 's|/$||' | tr '\n' ' ')
[ "$listed" = "$name $name/COPYRIGHT-library.html $name/LICENSE $name/README.md $name/THIRD-PARTY-LICENSES.md $name/whizbang " ] ||
  fail "the archive lists the folder and exactly the five files, in the order of their names (it lists: $listed)"
mkdir "$tmp/unpacked"
tar -xzf "$tmp/a/$name.tar.gz" -C "$tmp/unpacked"
[ "$(cd "$tmp/unpacked" && find . | sort | tr '\n' ' ')" = ". ./$name ./$name/COPYRIGHT-library.html ./$name/LICENSE ./$name/README.md ./$name/THIRD-PARTY-LICENSES.md ./$name/whizbang " ] ||
  fail "the archive unpacks to the folder and the five files and nothing else"
cmp -s "$tmp/unpacked/$name/whizbang" "$tmp/whizbang" && [ -x "$tmp/unpacked/$name/whizbang" ] || fail "the archive's binary is the one packaged, and can be run"
cmp -s "$tmp/unpacked/$name/LICENSE" LICENSE || fail "the archive's LICENSE is the repo's"
cmp -s "$tmp/unpacked/$name/README.md" docs/guides/install.md || fail "the archive's README.md is the install guide"
grep -x -F '# Third-party licences' "$tmp/unpacked/$name/THIRD-PARTY-LICENSES.md" >/dev/null &&
  grep -F "| clap | " "$tmp/unpacked/$name/THIRD-PARTY-LICENSES.md" >/dev/null ||
  fail "the archive's THIRD-PARTY-LICENSES.md holds the licences of the binary's crates"
# The notices of what the standard library holds: the toolchain's own file, as it is, and
# named by the file of licences beside it.
notices=$(rustc --print sysroot)/share/doc/rust/COPYRIGHT-library.html
[ -s "$notices" ] || fail "the pinned toolchain carries COPYRIGHT-library.html in share/doc/rust"
cmp -s "$tmp/unpacked/$name/COPYRIGHT-library.html" "$notices" || fail "the archive's COPYRIGHT-library.html is the toolchain's, byte for byte"
grep -F 'is the file `COPYRIGHT-library.html` beside this one' "$tmp/unpacked/$name/THIRD-PARTY-LICENSES.md" >/dev/null ||
  fail "THIRD-PARTY-LICENSES.md names the notices beside it"
# Fixed owners, a fixed time, and modes that are not this shell's.
tar -tvzf "$tmp/a/$name.tar.gz" >"$tmp/long"
[ "$(awk '{ print $1 }' "$tmp/long" | tr '\n' ' ')" = "drwxr-xr-x -rw-r--r-- -rw-r--r-- -rw-r--r-- -rw-r--r-- -rwxr-xr-x " ] || { cat "$tmp/long" >&2; fail "the entries have fixed modes"; }
if awk '{ print $2 "/" $3 }' "$tmp/long" | sed 's|^\([^/]*\)/\([^/]*\).*|\1 \2|' | grep -v -x -E '0 0' >/dev/null; then
  cat "$tmp/long" >&2
  fail "every entry is owned by user 0 and group 0, with no names"
fi
find "$tmp/unpacked/$name" -newer LICENSE | grep . >/dev/null && fail "every entry has the fixed time, which is long past"

# --- The sums line matches the archive: its SHA-256, two spaces, its name, which the
# guide's command reads.
[ "$(cat "$tmp/a/SHA256SUMS")" = "$(sha256 "$tmp/a/$name.tar.gz")  $name.tar.gz" ] || fail "SHA256SUMS is one line: the archive's SHA-256, two spaces and its name"
if command -v sha256sum >/dev/null; then
  (cd "$tmp/a" && sha256sum -c SHA256SUMS) >/dev/null 2>&1 || fail "sha256sum -c takes the line"
else
  (cd "$tmp/a" && shasum -a 256 -c SHA256SUMS) >/dev/null 2>&1 || fail "shasum -a 256 -c takes the line"
fi

# --- Two builds of one tree give the same list and the same sums line. The second is made
# under another umask and into a folder that holds an older archive, which is replaced.
mkdir "$tmp/b"
echo stale >"$tmp/b/whizbang-0.0.0-$target.tar.gz"
echo stale >"$tmp/b/SHA256SUMS"
(umask 077 && "$script" --binary "$tmp/whizbang" --out "$tmp/b") >"$tmp/out" 2>&1 || { cat "$tmp/out" >&2; fail "the archive a second time"; }
cmp -s "$tmp/a/SHA256SUMS" "$tmp/b/SHA256SUMS" || fail "two builds of one tree give the same sums line"
tar -tvzf "$tmp/b/$name.tar.gz" >"$tmp/long-b"
cmp -s "$tmp/long" "$tmp/long-b" || fail "two builds of one tree give the same list"
cmp -s "$tmp/a/$name.tar.gz" "$tmp/b/$name.tar.gz" || fail "two builds of one tree give the same archive"
[ "$(ls -A "$tmp/b" | tr '\n' ' ')" = "SHA256SUMS $name.tar.gz " ] || fail "an older archive in the folder is replaced, so the folder holds one"

# Options for tar and gzip in the environment change nothing: both programs read some there.
(export GZIP=-1 TAR_OPTIONS=--owner=12345 && "$script" --binary "$tmp/whizbang" --out "$tmp/c") >"$tmp/out" 2>&1 || { cat "$tmp/out" >&2; fail "the archive under GZIP and TAR_OPTIONS"; }
cmp -s "$tmp/a/$name.tar.gz" "$tmp/c/$name.tar.gz" || fail "GZIP and TAR_OPTIONS of whoever packages change nothing in the archive"

# --- When the script builds the binary itself, a checkout with a tracked change is
# refused before anything is built. Shown in a throwaway repository of this test's, with
# a copy of the script and stand-ins for cargo and rustc: this checkout is not touched.
repo="$tmp/repo"
mkdir -p "$repo/scripts" "$repo/docs/guides" "$tmp/tools"
cp "$script" scripts/release-build.sh "$repo/scripts/"
cp Cargo.toml LICENSE "$repo/"
cp docs/guides/install.md "$repo/docs/guides/install.md"
printf '%s\n' '#!/bin/bash' "case \"\$*\" in '--print sysroot') echo \"$(rustc --print sysroot)\" ;; *) echo \"host: $target\" ;; esac" >"$tmp/tools/rustc"
printf '%s\n' '#!/bin/bash' "printf '%s\\n' \"\$*\" >>\"$tmp/cargo-ran\"" \
  "printf '%s|RUSTFLAGS %s\\n' \"\$(printf '%s' \"\$CARGO_ENCODED_RUSTFLAGS\" | tr '\\037' '|')\" \"\${RUSTFLAGS-unset}\" >\"$tmp/cargo-flags\"" >"$tmp/tools/cargo"
mkdir -p "$tmp/cargo-home"
chmod +x "$tmp/tools/rustc" "$tmp/tools/cargo"
git -C "$repo" init -q
git -C "$repo" -c user.name=test -c user.email=test@example.invalid add -A
git -C "$repo" -c user.name=test -c user.email=test@example.invalid commit -q -m "a tree"
built() { (cd "$repo" && env PATH="$tmp/tools:$PATH" CARGO_HOME="$tmp/cargo-home" RUSTFLAGS=-Copt-level=0 scripts/package.sh --out "$tmp/built"); }
echo "changed" >>"$repo/LICENSE"
expect_fail "a tracked change, not staged" "the checkout has tracked changes" built
git -C "$repo" add LICENSE
expect_fail "a tracked change, staged" "the checkout has tracked changes" built
[ ! -e "$tmp/cargo-ran" ] && [ ! -e "$tmp/built" ] || fail "nothing is built or written from a checkout with a tracked change"
# Clean again, it goes on to the build: the stand-in cargo is asked, and builds nothing.
git -C "$repo" -c user.name=test -c user.email=test@example.invalid commit -q -m "the change"
echo "not tracked" >"$repo/notes.txt"
expect_fail "a clean checkout, with a stand-in cargo that builds nothing" "the binary is not a file that can be run" built
[ "$(cat "$tmp/cargo-ran")" = "build --release -p whizbang --locked" ] || fail "a clean checkout is built with: cargo build --release -p whizbang --locked"
# The compiler is told to write cargo's home and the checkout under fixed names, in the
# one variable that takes a path with a space, and RUSTFLAGS of whoever builds is not read.
[ "$(cat "$tmp/cargo-flags")" = "--remap-path-prefix=$(cd "$tmp/cargo-home" && pwd -P)=/cargo|--remap-path-prefix=$(cd "$repo" && pwd -P)=/whizbang|RUSTFLAGS unset" ] ||
  fail "the release build remaps cargo's home and the checkout, and no more (it was given: $(cat "$tmp/cargo-flags"))"

# --- A toolchain that does not carry the standard library's notices: no archive.
mkdir -p "$tmp/bare/sysroot" "$tmp/bare/bin"
printf '%s\n' '#!/bin/bash' "case \"\$*\" in -vV) echo \"host: $target\" ;; '--print sysroot') echo \"$tmp/bare/sysroot\" ;; *) exit 1 ;; esac" >"$tmp/bare/bin/rustc"
chmod +x "$tmp/bare/bin/rustc"
expect_fail "a toolchain without the notices" "the toolchain does not carry the standard library's notices" \
  env PATH="$tmp/bare/bin:$PATH" "$script" --binary "$tmp/whizbang" --out "$tmp/refused"
[ ! -e "$tmp/refused" ] || fail "a toolchain without the notices writes nothing"

# --- Never an archive of a binary that holds a path of the machine it was built on: this
# checkout's, cargo's home's, or one below the home folder. The stand-in holds it as a
# real binary would, in a message.
leaky() { # <name> <path> <expected text> [<variable>=<value>]
  printf '%s\n' '#!/bin/bash' "[ \"\$*\" = --version ] && echo \"whizbang $version (commit $commit)\"" "# panicked at $2/src/lib.rs:1:1" >"$tmp/leaky"
  chmod +x "$tmp/leaky"
  expect_fail "$1" "$3" env ${4:+"$4"} "$script" --binary "$tmp/leaky" --out "$tmp/refused"
  [ ! -e "$tmp/refused" ] || fail "$1: nothing is written"
}
leaky "a binary that holds the checkout's path" "$(pwd -P)/crates/whizbang" "the binary holds the path of this checkout"
mkdir -p "$tmp/cargo-home/registry"
leaky "a binary that holds the path of cargo's home" "$(cd "$tmp/cargo-home" && pwd -P)/registry/src/a-crate-1.0.0" "the binary holds the path of cargo's home" "CARGO_HOME=$tmp/cargo-home"
mkdir -p "$tmp/a-home"
leaky "a binary that holds a path below the home folder" "$(cd "$tmp/a-home" && pwd -P)/work" "the binary holds a path below the home folder" "HOME=$(cd "$tmp/a-home" && pwd -P)"
# The same folders under their fixed names are what a release build writes, and pass.
printf '%s\n' '#!/bin/bash' "[ \"\$*\" = --version ] && echo \"whizbang $version (commit $commit)\"" "# panicked at /cargo/registry/src/a-crate-1.0.0/src/lib.rs:1:1 and /whizbang/crates/whizbang/src/main.rs:1:1" >"$tmp/remapped"
chmod +x "$tmp/remapped"
expect_pass "a binary with the fixed names" "$script" --binary "$tmp/remapped" --out "$tmp/remapped-out"

# --- Never an archive of a binary that is not this checkout's: another commit, another
# version, no commit, or no answer. Nothing is written.
refused() { # <name> <version line>
  stand_in "$tmp/other" "$2"
  expect_fail "$1" "the binary's version line is not this checkout's" "$script" --binary "$tmp/other" --out "$tmp/refused"
  [ ! -e "$tmp/refused" ] || fail "$1: nothing is written"
}
refused "a binary of another commit" "whizbang $version (commit 0000000000000000000000000000000000000000)"
refused "a binary of another version" "whizbang 9.9.9 (commit $commit)"
refused "a binary that names no commit" "whizbang $version"
refused "a binary built outside a repository" "whizbang $version (commit unknown)"
printf '%s\n' '#!/bin/bash' 'exit 1' >"$tmp/other"
expect_fail "a binary that prints no version line" "the binary did not print its version line" "$script" --binary "$tmp/other" --out "$tmp/refused"
expect_fail "a binary that is not there" "the binary is not a file that can be run" "$script" --binary "$tmp/no-such" --out "$tmp/refused"
[ ! -e "$tmp/refused" ] || fail "a refused binary writes nothing"

# --- Only the two targets are packaged. A stand-in rustc says another.
mkdir "$tmp/bin"
printf '%s\n' '#!/bin/bash' 'echo "host: x86_64-pc-windows-msvc"' >"$tmp/bin/rustc"
chmod +x "$tmp/bin/rustc"
expect_fail "another target" "an archive is made only for aarch64-apple-darwin and x86_64-unknown-linux-gnu" \
  env PATH="$tmp/bin:$PATH" "$script" --binary "$tmp/whizbang" --out "$tmp/refused"
[ ! -e "$tmp/refused" ] || fail "another target writes nothing"

# --- An argument it does not know is a usage error.
status=0
"$script" --release >"$tmp/out" 2>&1 || status=$?
[ "$status" = 2 ] || fail "an unknown argument is a usage error (got $status)"

# --- The real build is `just smoke`'s: the same command, with the lock file held, and no
# feature named, so none of the tests' and no `baseline-replay`.
# After its own build, and only then, the lock file's crates are fetched before the
# licence texts are read offline: a stand-in binary fetches nothing.
grep -x -F '[ "$built" = 0 ] || cargo fetch --locked' "$script" >/dev/null || fail "a build of the script's own is followed by cargo fetch --locked, before the licences are read"
# One script builds for both, so the archive's binary is the one the smoke ran.
grep -x -F '  scripts/release-build.sh' "$script" >/dev/null || fail "the binary is built by scripts/release-build.sh"
grep -x -F '    scripts/release-build.sh' justfile >/dev/null || fail "just smoke builds the release binary the same way"
[ "$(grep -c -E '^[^#]*cargo build' scripts/release-build.sh)" = 1 ] &&
  grep -x -F 'env -u RUSTFLAGS CARGO_ENCODED_RUSTFLAGS="$flags" cargo build --release -p whizbang --locked' scripts/release-build.sh >/dev/null ||
  fail "the release build is: cargo build --release -p whizbang --locked, with the lock file as it is"
if grep -E '^[^#]*cargo build' "$script" justfile >/dev/null; then fail "nothing else builds a release binary"; fi
if grep -E '^[^#]*cargo build.*(--features|--all-features)' scripts/release-build.sh >/dev/null; then fail "the release build names no feature"; fi

finished
