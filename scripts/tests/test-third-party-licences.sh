#!/usr/bin/env bash
# The licence texts a release archive carries (scripts/third-party-licences.sh; plan m4,
# task R1, item 3): every crate the binary is built from has its text in the file, and a
# crate without one fails. The workspace as it is passes, so a dependency added without a
# licence text fails `just check` here; the failures are shown on lists of this test's own.
#
# Nothing here uses the network. cargo is run offline; the workspace's crates are read
# from cargo's home, where `just deny` put them: this test runs after it in `just check`.
source "$(dirname "$0")/lib.sh"

script=scripts/third-party-licences.sh
command -v jq >/dev/null || fail "jq is not installed"
command -v cargo-deny >/dev/null || fail "cargo-deny is not installed (just setup names the command)"
host=$(rustc -vV | sed -n 's/^host: //p')
case "$host" in
  aarch64-apple-darwin | x86_64-unknown-linux-gnu) ;;
  *) fail "this machine's target is $host, and the file is made for the two targets of SPEC.md section 5 only" ;;
esac

# --- The crates of the binary, for each target.
for target in aarch64-apple-darwin x86_64-unknown-linux-gnu; do
  "$script" --list "$target" >"$tmp/list-$target" 2>"$tmp/error" || { cat "$tmp/error" >&2; fail "the crates of the binary are listed for $target"; }
  # Crates the binary certainly holds, a procedural macro among them.
  for crate in clap serde serde_derive serde_json image sha2 rustls toml; do
    cut -f 1 "$tmp/list-$target" | grep -x -F "$crate" >/dev/null || fail "$target: the list holds $crate"
  done
  # The workspace's own crates are not third parties'.
  if cut -f 1 "$tmp/list-$target" | grep -E '^whizbang' >/dev/null; then
    fail "$target: the list holds no crate of the workspace"
  fi
  # What only the tests' build holds is not in the binary: the validator behind the
  # feature `baseline-replay` (ADR 0034), a build script's compiler driver, the test kit.
  for crate in jsonschema cc; do
    if cut -f 1 "$tmp/list-$target" | grep -x -F "$crate" >/dev/null; then
      fail "$target: $crate is not built into the released binary, and is on the list"
    fi
  done
  # Each line: a name, a version, a licence, and a folder that is there.
  while IFS=$'\t' read -r name version licence folder rest; do
    [ -n "$name" ] && [ -n "$version" ] && [ -n "$licence" ] && [ -d "$folder" ] && [ -z "$rest" ] ||
      fail "$target: a line of the list is not a name, a version, a licence and a folder: $name"
  done <"$tmp/list-$target"
done

# --- Held to the list `cargo deny` checks: every crate here is one whose licence and
# source `just deny` passed, at that version.
cargo deny --offline list >"$tmp/deny" 2>"$tmp/error" || { cat "$tmp/error" >&2; fail "cargo deny lists the crates it checks"; }
tr ' ,' '\n\n' <"$tmp/deny" | grep '@' | sort -u >"$tmp/checked"
[ "$(wc -l <"$tmp/checked" | tr -d ' ')" -gt 20 ] || fail "cargo deny's list was read"
while IFS=$'\t' read -r name version licence folder; do
  grep -x -F "$name@$version" "$tmp/checked" >/dev/null ||
    fail "$name $version is in the binary and not among the crates cargo deny checks"
done <"$tmp/list-$host"

# --- The file itself, for this machine's target: every crate of the list has a row that
# names at least one text, and every text a row names is printed.
expect_pass "the file for $host" "$script" "$host" "$tmp/made.md"
grep -x -F '# Third-party licences' "$tmp/made.md" >/dev/null || fail "the file has its title"
while IFS=$'\t' read -r name version licence folder; do
  row=$(grep -F "| $name | $version | $licence | " "$tmp/made.md" || true)
  [ -n "$row" ] || fail "$name $version has a row in the file"
  labels=$(printf '%s\n' "$row" | grep -o 'text-[0-9a-f]\{12\}' || true)
  [ -n "$labels" ] || fail "the row of $name $version names a text"
  for label in $labels; do
    grep -x -F "### $label" "$tmp/made.md" >/dev/null || fail "$label, which $name names, is printed"
  done
done <"$tmp/list-$host"
[ "$(grep -c '^| ' "$tmp/made.md")" = "$(($(wc -l <"$tmp/list-$host") + 1))" ] || fail "the table has its heading, one row for each crate and no other"
# A licence everybody knows is in it, whole.
grep -F 'Permission is hereby granted, free of charge, to any person obtaining a copy' "$tmp/made.md" >/dev/null || fail "an MIT text is printed"
grep -F 'TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION' "$tmp/made.md" >/dev/null || fail "an Apache text is printed"
# The same lock file gives the same bytes.
expect_pass "the file a second time" "$script" "$host" "$tmp/again.md"
cmp -s "$tmp/made.md" "$tmp/again.md" || fail "two runs give the same file"
# It is made for an archive and never committed: its texts hold their authors' addresses.
[ -z "$(git ls-files THIRD-PARTY-LICENSES.md)" ] || fail "THIRD-PARTY-LICENSES.md is not a tracked file"
# The archive's copy is made by this script.
grep -F 'scripts/third-party-licences.sh "$target" "$stage/THIRD-PARTY-LICENSES.md"' scripts/package.sh >/dev/null ||
  fail "scripts/package.sh makes the archive's THIRD-PARTY-LICENSES.md with this script"

# --- Lists of this test's own: crates in folders it makes.
crate() { # <name> <file> <text>: a crate's folder with one file
  mkdir -p "$tmp/crates/$1/$(dirname "$2")"
  printf '%s\n' "$3" >"$tmp/crates/$1/$2"
}
line() { printf '%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$tmp/crates/$1" "${4:-}"; }
crate first LICENSE-MIT "The licence of first and second."
crate second LICENCE.txt "The licence of first and second."
crate third COPYING "The licence of third."
crate third src/lib.rs "// not a licence"
crate elsewhere docs/terms.txt "The licence of elsewhere, where its manifest says."
crate bare src/lib.rs "// a crate that carries no licence text"
{
  line first 1.0.0 MIT
  line second 2.0.0 MIT
  line third 3.0.0 ISC
  line elsewhere 4.0.0 Zlib docs/terms.txt
} >"$tmp/good.tsv"
expect_pass "a list whose crates each carry a text" "$script" --render "$tmp/good.tsv" "$host" "$tmp/planted.md"
# A text two crates carry byte for byte is printed once and names both.
[ "$(grep -c -x -F 'The licence of first and second.' "$tmp/planted.md")" = 1 ] || fail "a text two crates carry is printed once"
grep -x -F 'Carried by: first 1.0.0, second 2.0.0.' "$tmp/planted.md" >/dev/null || fail "a shared text names each crate that carries it"
grep -x -F 'The licence of third.' "$tmp/planted.md" >/dev/null || fail "a file named COPYING is a licence text"
grep -x -F 'The licence of elsewhere, where its manifest says.' "$tmp/planted.md" >/dev/null ||
  fail "the file a manifest names as its licence is a licence text"
grep -F 'not a licence' "$tmp/planted.md" >/dev/null && fail "a source file is not a licence text"
[ "$(grep -c '^### text-' "$tmp/planted.md")" = 3 ] || fail "three texts are printed for four crates"

# A crate in the binary without its text fails, by name, and nothing is written.
{
  cat "$tmp/good.tsv"
  line bare 5.0.0 MIT
} >"$tmp/bare.tsv"
expect_fail "a crate with no licence text" "these crates of the binary carry no licence text" \
  "$script" --render "$tmp/bare.tsv" "$host" "$tmp/bare.md"
grep -F "bare 5.0.0 (MIT)" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "the complaint names the crate"; }
[ ! -e "$tmp/bare.md" ] || fail "no file is written when a crate has no text"
# A crate that states no licence, a folder that is not there, an empty list, another target.
line third 3.0.0 "" >"$tmp/unstated.tsv"
expect_fail "a crate that states no licence" "third 3.0.0 states no licence" "$script" --render "$tmp/unstated.tsv" "$host" "$tmp/x.md"
printf 'gone\t1.0.0\tMIT\t%s\t\n' "$tmp/crates/gone" >"$tmp/gone.tsv"
expect_fail "a crate whose folder is not there" "the folder of gone 1.0.0 is not there" "$script" --render "$tmp/gone.tsv" "$host" "$tmp/x.md"
: >"$tmp/empty.tsv"
expect_fail "an empty list" "the list holds no crate" "$script" --render "$tmp/empty.tsv" "$host" "$tmp/x.md"
expect_fail "another target" "no archive is made for the target" "$script" --list x86_64-pc-windows-msvc
[ ! -e "$tmp/x.md" ] || fail "a refused list writes no file"

finished
