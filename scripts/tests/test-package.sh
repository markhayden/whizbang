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

# --- The archive holds one folder with exactly the four files.
expect_pass "the archive" "$script" --binary "$tmp/whizbang" --out "$tmp/a"
[ "$(ls -A "$tmp/a" | tr '\n' ' ')" = "SHA256SUMS $name.tar.gz " ] || fail "the folder holds the archive and SHA256SUMS (found: $(ls -A "$tmp/a" | tr '\n' ' '))"
listed=$(tar -tzf "$tmp/a/$name.tar.gz" | sed 's|/$||' | tr '\n' ' ')
[ "$listed" = "$name $name/LICENSE $name/README.md $name/THIRD-PARTY-LICENSES.md $name/whizbang " ] ||
  fail "the archive lists the folder and exactly the four files, in the order of their names (it lists: $listed)"
mkdir "$tmp/unpacked"
tar -xzf "$tmp/a/$name.tar.gz" -C "$tmp/unpacked"
[ "$(cd "$tmp/unpacked" && find . | sort | tr '\n' ' ')" = ". ./$name ./$name/LICENSE ./$name/README.md ./$name/THIRD-PARTY-LICENSES.md ./$name/whizbang " ] ||
  fail "the archive unpacks to the folder and the four files and nothing else"
cmp -s "$tmp/unpacked/$name/whizbang" "$tmp/whizbang" && [ -x "$tmp/unpacked/$name/whizbang" ] || fail "the archive's binary is the one packaged, and can be run"
cmp -s "$tmp/unpacked/$name/LICENSE" LICENSE || fail "the archive's LICENSE is the repo's"
cmp -s "$tmp/unpacked/$name/README.md" docs/guides/install.md || fail "the archive's README.md is the install guide"
grep -x -F '# Third-party licences' "$tmp/unpacked/$name/THIRD-PARTY-LICENSES.md" >/dev/null &&
  grep -F "| clap | " "$tmp/unpacked/$name/THIRD-PARTY-LICENSES.md" >/dev/null ||
  fail "the archive's THIRD-PARTY-LICENSES.md holds the licences of the binary's crates"
# Fixed owners, a fixed time, and modes that are not this shell's.
tar -tvzf "$tmp/a/$name.tar.gz" >"$tmp/long"
[ "$(awk '{ print $1 }' "$tmp/long" | tr '\n' ' ')" = "drwxr-xr-x -rw-r--r-- -rw-r--r-- -rw-r--r-- -rwxr-xr-x " ] || { cat "$tmp/long" >&2; fail "the entries have fixed modes"; }
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
grep -x -F '  cargo build --release -p whizbang --locked' "$script" >/dev/null || fail "the binary is built as just smoke builds it, with the lock file as it is"
grep -x -F '    cargo build --release -p whizbang' justfile >/dev/null || fail "just smoke builds the release binary the same way"
if grep -E '^[^#]*cargo build.*(--features|--all-features)' "$script" >/dev/null; then fail "the release build names no feature"; fi

finished
