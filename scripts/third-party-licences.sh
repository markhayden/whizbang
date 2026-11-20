#!/usr/bin/env bash
# Writes the licence texts of the crates the `whizbang` binary holds: the file
# `THIRD-PARTY-LICENSES.md` of a release archive (plan m4, task R1, item 3). A release
# hands other people's code to a third party, and their licences ask that their text goes
# with it.
#
#   scripts/third-party-licences.sh <target> <file>
#                                    write the file for the binary of that target
#   scripts/third-party-licences.sh --list <target>
#                                    print the crates only, one a line: name, version,
#                                    licence and folder, apart by tabs
#   scripts/third-party-licences.sh --render <list> <target> <file>
#                                    write the file from such a list
#                                    (scripts/tests/test-third-party-licences.sh)
#
# Which crates. Those the binary is linked from, as the lock file resolves them for the
# target: `cargo tree -p whizbang --edges normal`, which follows what a crate uses when it
# is built into the binary and leaves out what only a test or a build script uses, with the
# features of a release build (`cargo build --release -p whizbang`: no `fakes`, no
# `baseline-replay`, ADR 0034). A procedural macro is on the list: what it writes is in the
# binary. The workspace's own crates are not: their licence is the archive's `LICENSE`.
# Every crate comes from crates.io, which `just deny` holds (deny.toml, `[sources]`), and
# carries a licence `just deny` allows; the test holds this list to the one `cargo deny`
# checks.
#
# Which texts. Every file at the top of the crate's folder, as crates.io published it,
# whose name starts with LICENSE, LICENCE, COPYING, UNLICENSE, NOTICE or COPYRIGHT, in any
# letter case, and the file its manifest names as its licence. A text that several crates
# carry byte for byte is written once and named by each. **A crate with no such file fails
# the script**, and no archive is made: its text is then added by a decision, not left out.
#
# The file is not committed: licence texts hold their authors' addresses, which no tracked
# file here may (AGENTS.md, "Secrets and private data"). `just package` makes it for each
# archive, and the same lock file gives the same bytes.
#
# Nothing here uses the network: cargo is run offline and with the lock file as it is. The
# crates are read from cargo's home, where a build or `just deny` put them. Needs jq.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "third-party licences: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

die() {
  echo "third-party licences: $*" >&2
  exit 1
}
usage() {
  echo "usage: scripts/third-party-licences.sh <target> <file> | --list <target> | --render <list> <target> <file>" >&2
  exit 2
}
known_target() {
  case "$1" in
    aarch64-apple-darwin | x86_64-unknown-linux-gnu) ;;
    *) die "no archive is made for the target $1: only aarch64-apple-darwin and x86_64-unknown-linux-gnu (SPEC.md section 5)" ;;
  esac
}
sha256() { # <file>: its SHA-256, in hexadecimal
  if command -v sha256sum >/dev/null; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

mode=write
list=
case "${1:-}" in
  --list)
    [ $# -eq 2 ] || usage
    mode=list
    target=$2
    ;;
  --render)
    [ $# -eq 4 ] || usage
    list=$2
    target=$3
    out=$4
    [ -f "$list" ] || die "no such list: $list"
    ;;
  -*) usage ;;
  *)
    [ $# -eq 2 ] || usage
    target=$1
    out=$2
    ;;
esac
known_target "$target"

tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "third-party licences: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

# crates: the list, sorted, on the standard output.
crates() {
  command -v jq >/dev/null || die "jq is not installed"
  # What the binary is linked from, as `name version`. A crate met again ends in ` (*)`, a
  # procedural macro in ` (proc-macro)`, a crate of the workspace in its folder.
  cargo tree -p whizbang --edges normal --target "$target" --prefix none --format '{p}' \
    --locked --offline >"$tmp/tree" 2>"$tmp/error" ||
    { cat "$tmp/error" >&2; die "cargo could not list the crates of the binary"; }
  awk '{ sub(/^v/, "", $2); print $1 " " $2 }' "$tmp/tree" | sort -u >"$tmp/linked"
  [ -s "$tmp/linked" ] || die "cargo listed no crate of the binary"
  # Every crate of the lock file that is not the workspace's own: its licence and folder.
  cargo metadata --format-version 1 --locked --offline >"$tmp/metadata" 2>"$tmp/error" ||
    { cat "$tmp/error" >&2; die "cargo could not describe the crates of the lock file"; }
  jq -r '.packages[] | select(.source != null)
    | [.name + " " + .version, (.license // ""), (.manifest_path | sub("/Cargo\\.toml$"; "")), (.license_file // "")]
    | @tsv' "$tmp/metadata" | sort >"$tmp/known"
  jq -r '.packages[] | select(.source == null) | .name + " " + .version' "$tmp/metadata" | sort >"$tmp/own"
  local crate found
  while IFS= read -r crate; do
    if grep -x -F -e "$crate" "$tmp/own" >/dev/null; then
      continue
    fi
    found=$(awk -F '\t' -v crate="$crate" '$1 == crate' "$tmp/known")
    [ -n "$found" ] || die "the binary is linked from $crate, which the lock file's crates do not hold"
    [ "$(printf '%s\n' "$found" | wc -l | tr -d ' ')" = 1 ] || die "the lock file holds $crate from more than one place"
    printf '%s\n' "$found" | awk -F '\t' '{ split($1, id, " "); print id[1] "\t" id[2] "\t" $2 "\t" $3 "\t" $4 }'
  done <"$tmp/linked"
}

if [ -z "$list" ]; then
  crates >"$tmp/list"
  list=$tmp/list
fi
[ -s "$list" ] || die "the list holds no crate"
if [ "$mode" = list ]; then
  cut -f 1-4 "$list"
  done_ok=1
  exit 0
fi

# The list with another mark than a tab between its fields: `read` takes several tabs
# for one, and a field that is empty would be lost.
tr '\t' '\037' <"$list" >"$tmp/fields"

# The texts of each crate: one line for each, its SHA-256, the crate, the file's name and
# where it is. A crate with none is kept for the complaint.
: >"$tmp/texts"
: >"$tmp/without"
while IFS=$'\037' read -r name version licence folder named; do
  [ -n "$name" ] && [ -n "$version" ] || die "a line of the list names no crate and version"
  [ -n "$licence" ] || die "$name $version states no licence"
  [ -d "$folder" ] || die "the folder of $name $version is not there: $folder (a build, or just deny, puts it in cargo's home)"
  : >"$tmp/files"
  for file in "$folder"/*; do
    [ -f "$file" ] || continue
    case "$(basename "$file" | tr '[:lower:]' '[:upper:]')" in
      LICENSE* | LICENCE* | COPYING* | UNLICENSE* | UNLICENCE* | NOTICE* | COPYRIGHT*) printf '%s\n' "$file" >>"$tmp/files" ;;
    esac
  done
  if [ -n "$named" ] && [ -f "$folder/$named" ]; then
    printf '%s\n' "$folder/$named" >>"$tmp/files"
  fi
  if [ ! -s "$tmp/files" ]; then
    printf '%s %s (%s)\n' "$name" "$version" "$licence" >>"$tmp/without"
    continue
  fi
  sort -u "$tmp/files" >"$tmp/files.sorted"
  while IFS= read -r file; do
    [ -s "$file" ] || die "$file of $name $version is empty"
    printf '%s\t%s %s\t%s\t%s\n' "$(sha256 "$file")" "$name" "$version" "${file#"$folder"/}" "$file" >>"$tmp/texts"
  done <"$tmp/files.sorted"
done <"$tmp/fields"
if [ -s "$tmp/without" ]; then
  echo "third-party licences: these crates of the binary carry no licence text, so no archive can carry it:" >&2
  sed 's/^/    /' "$tmp/without" >&2
  exit 1
fi

# A text is named by the first twelve digits of its SHA-256.
label() { printf 'text-%s' "$(printf '%s' "$1" | cut -c 1-12)"; }
fence='~~~~'
released=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | sed -n '1p')
[ -n "$released" ] || die "Cargo.toml names no version"
{
  echo "# Third-party licences"
  echo
  echo "The \`whizbang\` binary of version $released for \`$target\` is built from the crates below,"
  echo "each as crates.io published it, at the version the lock file names. Whizbang's own"
  echo "licence is the file \`LICENSE\` beside this one. A text that several crates carry, byte"
  echo "for byte, is printed once, under the name the table gives it."
  echo
  echo "The binary also holds the Rust standard library, which is under MIT or Apache-2.0 at"
  echo "the choice of whoever uses it; its notices are published with the Rust toolchain and"
  echo "are not repeated here. No model is in the archive, and so no model's licence."
  echo
  echo "## Crates"
  echo
  echo "| Crate | Version | Licence | Texts |"
  echo "|---|---|---|---|"
  while IFS=$'\037' read -r name version licence folder named; do
    texts=$(awk -F '\t' -v crate="$name $version" '$2 == crate { print $1 "\t" $3 }' "$tmp/texts" |
      while IFS=$'\t' read -r sum file; do printf '%s (`%s`), ' "$(label "$sum")" "$file"; done)
    printf '| %s | %s | %s | %s |\n' "$name" "$version" "$licence" "${texts%, }"
  done <"$tmp/fields"
  echo
  echo "## Texts"
  cut -f 1 "$tmp/texts" | sort -u >"$tmp/sums"
  while IFS= read -r sum; do
    file=$(awk -F '\t' -v sum="$sum" '$1 == sum { print $4; exit }' "$tmp/texts")
    if grep -e "^$fence" "$file" >/dev/null; then
      die "$file holds a line that starts with $fence, which would end its block in the file"
    fi
    echo
    echo "### $(label "$sum")"
    echo
    echo "Carried by: $(awk -F '\t' -v sum="$sum" '$1 == sum { print $2 }' "$tmp/texts" | sort -u | tr '\n' ',' | sed 's/,$//; s/,/, /g')."
    echo
    echo "$fence"
    # As it is, without a carriage return at a line's end, and ending in one line break.
    tr -d '\r' <"$file" | sed -e '$a\'
    echo "$fence"
  done <"$tmp/sums"
} >"$tmp/out"
mkdir -p "$(dirname "$out")"
cp "$tmp/out" "$out"
echo "third-party licences: wrote $out ($(wc -l <"$list" | tr -d ' ') crates, $(wc -l <"$tmp/sums" | tr -d ' ') texts)"
done_ok=1
exit 0
}
main "$@"; exit
