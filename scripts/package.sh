#!/usr/bin/env bash
# Makes the release archive of this machine's target (plan m4, task R1, item 1). `just
# package`; CI's `release` job runs it on each runner, and `scripts/install-check.sh` then
# installs what it made. It builds, so it is heavy (AGENTS.md).
#
#   scripts/package.sh [--out <folder>] [--binary <file>]
#
#   --out <folder>    where the archive and SHA256SUMS are written: `target/package` when
#                     not given. An archive and a SHA256SUMS already there are replaced, so
#                     the folder holds one archive
#   --binary <file>   package this file as the binary and build nothing: for
#                     scripts/tests/test-package.sh and scripts/tests/test-install-check.sh,
#                     which give it a stand-in. It is held to the version line all the same
#
# The archive is `whizbang-<version>-<target>.tar.gz`: the version is the workspace's
# (Cargo.toml), the target is the one this toolchain builds for, and only the two targets
# of SPEC.md section 5 are packaged. It holds one folder, `whizbang-<version>-<target>`,
# with exactly four files:
#
#   whizbang                   the binary, built as `just smoke` builds it: `cargo build
#                              --release -p whizbang`, so with no feature of the tests
#                              (`fakes`) and without `baseline-replay` (ADR 0034), and with
#                              the lock file as it is
#   LICENSE                    whizbang's licence
#   THIRD-PARTY-LICENSES.md    the licence texts of the crates the binary is built from
#                              (scripts/third-party-licences.sh)
#   README.md                  the install guide, docs/guides/install.md, as it is
#
# Nothing else: the pack, the recipes, the rules, the constant sets and the catalogue are
# inside the binary. Beside the archive it writes `SHA256SUMS`, one line: the archive's
# SHA-256, two spaces and its name, which `sha256sum -c` and `shasum -a 256 -c` read.
#
# Before anything is archived the binary is asked for its version line, with no variable
# of this shell's, and it must be `whizbang <version> (commit <the commit checked out>)`:
# an archive is never named for another version than its binary's, and never holds a
# binary of another commit. Files changed since that commit are not looked for: whoever
# packages a release holds the checkout to be clean.
#
# The same sources give the same archive on the same machine: the entries are written in a
# fixed order, each with the time 2000-01-01 00:00 UTC, the owner and group 0 with no
# names, and modes that do not depend on this shell's; nothing a Mac keeps beside a file is
# archived; and the compression records no name and no time. (Whether two builds of the
# binary are the same bytes is the compiler's matter, not this script's.)
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "package: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

die() {
  echo "package: $*" >&2
  exit 1
}
usage() {
  echo "usage: scripts/package.sh [--out <folder>] [--binary <file>]" >&2
  exit 2
}

out=target/package
binary=
while [ $# -gt 0 ]; do
  case "$1" in
    --out)
      [ $# -ge 2 ] && [ -n "$2" ] || usage
      out=$2
      shift 2
      ;;
    --binary)
      [ $# -ge 2 ] && [ -n "$2" ] || usage
      binary=$2
      shift 2
      ;;
    *) usage ;;
  esac
done

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | sed -n '1p')
printf '%s\n' "$version" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' >/dev/null ||
  die "Cargo.toml must name the workspace's version as three numbers (it names: ${version:-none})"
target=$(rustc -vV | sed -n 's/^host: //p')
case "$target" in
  aarch64-apple-darwin | x86_64-unknown-linux-gnu) ;;
  *) die "this toolchain builds for ${target:-no target it names}, and an archive is made only for aarch64-apple-darwin and x86_64-unknown-linux-gnu (SPEC.md section 5)" ;;
esac
commit=$(git rev-parse --verify 'HEAD^{commit}')
guide=docs/guides/install.md
[ -f LICENSE ] || die "there is no LICENSE to package"
[ -f "$guide" ] || die "there is no install guide to package ($guide)"

if [ -z "$binary" ]; then
  # As `just smoke` builds it: no feature is named, so none of the tests' and no
  # `baseline-replay`.
  cargo build --release -p whizbang --locked
  binary=${CARGO_TARGET_DIR:-target}/release/whizbang
fi
[ -f "$binary" ] && [ -x "$binary" ] || die "the binary is not a file that can be run: $binary"
case "$binary" in /*) ;; *) binary=$PWD/$binary ;; esac

said=$(env -i "$binary" --version 2>/dev/null) || die "the binary did not print its version line"
want="whizbang $version (commit $commit)"
[ "$said" = "$want" ] ||
  die "the binary's version line is not this checkout's: it says '$said', and the archive would be of '$want' (build it again from this commit)"

tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "package: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

name=whizbang-$version-$target
stage=$tmp/$name
mkdir "$stage"
cp "$binary" "$stage/whizbang"
cp LICENSE "$stage/LICENSE"
cp "$guide" "$stage/README.md"
scripts/third-party-licences.sh "$target" "$stage/THIRD-PARTY-LICENSES.md" >/dev/null
[ -s "$stage/THIRD-PARTY-LICENSES.md" ] || die "the third-party licences were not written"

# The folder, then its files in the order of their names; nothing else is in it.
entries=("$name" "$name/LICENSE" "$name/README.md" "$name/THIRD-PARTY-LICENSES.md" "$name/whizbang")
[ "$(find "$stage" | wc -l | tr -d ' ')" = "${#entries[@]}" ] || die "the folder to archive holds something else than the four files"
chmod 755 "$stage" "$stage/whizbang"
chmod 644 "$stage/LICENSE" "$stage/README.md" "$stage/THIRD-PARTY-LICENSES.md"
(cd "$tmp" && TZ=UTC touch -t 200001010000.00 "${entries[@]}")

# GNU tar and the tar of a Mac say the same things in different words.
if tar --version 2>/dev/null | grep -F 'GNU tar' >/dev/null; then
  archive() { tar --format=ustar --owner=0 --group=0 --numeric-owner --no-recursion -cf - "$@"; }
else
  archive() {
    COPYFILE_DISABLE=1 tar --format ustar --uid 0 --gid 0 --uname '' --gname '' \
      --no-mac-metadata --no-xattrs --no-acls --no-fflags -n -cf - "$@"
  }
fi
(cd "$tmp" && archive "${entries[@]}") | gzip -n -9 >"$tmp/$name.tar.gz"
# What was written is what was meant: the five entries, in their order.
listed=$(tar -tzf "$tmp/$name.tar.gz" | sed 's|/$||')
[ "$listed" = "$(printf '%s\n' "${entries[@]}")" ] ||
  die "the archive does not list the folder and its four files in their order"

if command -v sha256sum >/dev/null; then
  sum=$(sha256sum "$tmp/$name.tar.gz" | cut -d' ' -f1)
else
  sum=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d' ' -f1)
fi
printf '%s\n' "$sum" | grep -E '^[0-9a-f]{64}$' >/dev/null || die "the archive's SHA-256 could not be taken"

mkdir -p "$out"
rm -f "$out"/whizbang-*.tar.gz "$out/SHA256SUMS"
cp "$tmp/$name.tar.gz" "$out/$name.tar.gz"
printf '%s  %s\n' "$sum" "$name.tar.gz" >"$out/SHA256SUMS"
echo "package: wrote $out/$name.tar.gz"
echo "package: wrote $out/SHA256SUMS: $sum  $name.tar.gz"
done_ok=1
exit 0
}
main "$@"; exit
