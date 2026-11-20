#!/usr/bin/env bash
# Builds the release binary, the one way it is built: for `just smoke`, and for the
# release archive (scripts/package.sh), so that the binary the smoke ran is the binary
# that is packaged and the second of the two builds nothing.
#
#   scripts/release-build.sh
#
# It runs `cargo build --release -p whizbang --locked`: no feature is named, so none of
# the tests' (`fakes`) and no `baseline-replay` (ADR 0034), and the lock file is held as
# it is. The binary is `target/release/whizbang`, or under CARGO_TARGET_DIR when set.
#
# The compiler is told to write two folders of the machine that builds under fixed names
# (`--remap-path-prefix`; plan m4, "Owed after R1", item 1): cargo's home, where the
# crates' sources are, as `/cargo`, and this checkout as `/whizbang`. A panic's message
# names the file it came from, so without this every archive would carry the paths of
# whoever built it, and two machines would give different bytes for them. The flags reach
# every crate through CARGO_ENCODED_RUSTFLAGS, which takes a path with a space in it;
# RUSTFLAGS of whoever builds is not read, so that it cannot change a release.
# scripts/package.sh reads the binary for both folders and refuses an archive that holds
# either.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "release-build: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
[ $# -eq 0 ] || { echo "usage: scripts/release-build.sh" >&2; exit 2; }
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

cargo_home=${CARGO_HOME:-${HOME:-}/.cargo}
case "$cargo_home" in
  /*) ;;
  *) echo "release-build: cargo's home is not known as an absolute path (CARGO_HOME, or HOME): the paths of its crates could not be written under a fixed name" >&2; exit 1 ;;
esac
[ -d "$cargo_home" ] && cargo_home=$(cd "$cargo_home" && pwd -P)
case "$cargo_home$root" in
  *$'\037'*) echo "release-build: a folder's name holds the character that separates the compiler's flags" >&2; exit 1 ;;
esac
flags="--remap-path-prefix=$cargo_home=/cargo"$'\037'"--remap-path-prefix=$root=/whizbang"
env -u RUSTFLAGS CARGO_ENCODED_RUSTFLAGS="$flags" cargo build --release -p whizbang --locked
}
main "$@"; exit
