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
[ "$(sed -n '/^## Crates$/,/^## Texts$/p' "$tmp/made.md" | grep -c '^| ')" = "$(($(wc -l <"$tmp/list-$host") + 1))" ] || fail "the table has its heading, one row for each crate and no other"
# A licence everybody knows is in it, whole.
grep -F 'Permission is hereby granted, free of charge, to any person obtaining a copy' "$tmp/made.md" >/dev/null || fail "an MIT text is printed"
grep -F 'TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION' "$tmp/made.md" >/dev/null || fail "an Apache text is printed"
# The standard library, which is in the binary and in no lock file: its two licence texts,
# from the toolchain's own folder.
std=$(rustc --print sysroot)/share/doc/rust/licenses
grep -x -F '## The Rust standard library' "$tmp/made.md" >/dev/null || fail "the file names the standard library"
grep -F "$(rustc -V)" "$tmp/made.md" >/dev/null || fail "the file names the compiler whose standard library it is"
for text in MIT.txt Apache-2.0.txt; do
  [ -s "$std/$text" ] || fail "the pinned toolchain carries $text in share/doc/rust/licenses"
  grep -x -F "### The standard library's $text" "$tmp/made.md" >/dev/null || fail "the standard library's $text is printed"
  tail -n 1 "$std/$text" >"$tmp/last"
  grep -x -F -f "$tmp/last" "$tmp/made.md" >/dev/null || fail "the standard library's $text is printed to its end"
done
# The crates under more than one licence at once, as the lock file has them now: each has
# a text for each, one of them in a subfolder.
grep -F '| ring | ' "$tmp/made.md" | grep -F 'Apache-2.0 AND ISC' | grep -F '/LICENSE' >/dev/null ||
  fail "ring, under two licences at once, has its texts, those in its subfolders among them"

# The playground's interface, which the binary embeds (plan m5, task U1): Preact's and
# marked's (task W3) and the fonts' texts, each under its heading, with its table.
grep -x -F "## The playground's interface" "$tmp/made.md" >/dev/null || fail "the file names the playground's interface"
for heading in 'The package marked' 'The package preact' 'The font geist' 'The font geist-mono' 'The font bricolage-grotesque' 'The outlined roboto-slab'; do
  grep -x -F "### $heading" "$tmp/made.md" >/dev/null || fail "the file holds: $heading"
done
grep -F '| package preact | 10.29.8 | MIT | npm preact@10.29.8 |' "$tmp/made.md" >/dev/null || fail "Preact has its row, at the version the lock file pins"
grep -F '| font geist | 5.3.0 | OFL-1.1 | npm @fontsource/geist-sans@5.3.0 |' "$tmp/made.md" >/dev/null || fail "a font's row names where its files came from"
grep -F '| package marked | 18.1.3 | MIT AND BSD-3-Clause | npm marked@18.1.3 |' "$tmp/made.md" >/dev/null || fail "marked has its row, at the version the lock file pins (task W3)"
grep -F 'SIL OPEN FONT LICENSE Version 1.1' "$tmp/made.md" >/dev/null || fail "the fonts' licence is printed"
grep -F '| outlined roboto-slab | 2.002 | Apache-2.0 | github google/fonts@' "$tmp/made.md" | grep -F '| wordmark.svg |' >/dev/null ||
  fail "the wordmark's font has its row, with where it came from and the file drawn from it"
grep -F 'Copyright 2018 The Roboto Slab Project Authors' "$tmp/made.md" >/dev/null || fail "Roboto Slab's notice is printed"
grep -F 'Copyright (c) 2015-present Jason Miller' "$tmp/made.md" >/dev/null || fail "Preact's text is printed"

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
crate first LICENSE-MIT "Permission is hereby granted, free of charge, to use first and second."
crate second LICENCE.txt "Permission is hereby granted, free of charge, to use first and second."
crate third COPYING "Permission to use, copy, modify, and/or distribute third is hereby granted."
crate third src/lib.rs "// not a licence"
crate elsewhere docs/terms.txt "This software is provided 'as-is': elsewhere, where its manifest says."
crate bare src/lib.rs "// a crate that carries no licence text"
{
  line first 1.0.0 MIT
  line second 2.0.0 MIT
  line third 3.0.0 ISC
  line elsewhere 4.0.0 Zlib docs/terms.txt
} >"$tmp/good.tsv"
expect_pass "a list whose crates each carry a text" "$script" --render "$tmp/good.tsv" "$host" "$tmp/planted.md"
# A text two crates carry byte for byte is printed once and names both.
[ "$(grep -c -x -F 'Permission is hereby granted, free of charge, to use first and second.' "$tmp/planted.md")" = 1 ] || fail "a text two crates carry is printed once"
grep -x -F 'Carried by: first 1.0.0, second 2.0.0.' "$tmp/planted.md" >/dev/null || fail "a shared text names each crate that carries it"
grep -x -F 'Permission to use, copy, modify, and/or distribute third is hereby granted.' "$tmp/planted.md" >/dev/null || fail "a file named COPYING is a licence text"
grep -x -F "This software is provided 'as-is': elsewhere, where its manifest says." "$tmp/planted.md" >/dev/null ||
  fail "the file a manifest names as its licence is a licence text"
grep -F 'not a licence' "$tmp/planted.md" >/dev/null && fail "a source file is not a licence text"
[ "$(grep -c '^### text-' "$tmp/planted.md")" = 3 ] || fail "three texts are printed for four crates"

# A crate that carries other people's code keeps their notices in subfolders: a text at
# any depth is taken. And a crate under two licences at once has a text for each: the
# words every text of a licence holds are looked for, however its lines are broken.
crate vendoring LICENSE "Permission to use, copy, modify, and/or distribute vendoring is hereby granted."
crate vendoring third_party/inner/NOTICE.txt "A notice of the code vendoring carries."
crate vendoring third_party/inner/LICENSE-APACHE "$(printf '%s\n' 'Apache' 'License, Version 2.0, of the code vendoring carries.')"
line vendoring 6.0.0 "Apache-2.0 AND ISC" >"$tmp/and.tsv"
expect_pass "a crate under two licences, one text in a subfolder" "$script" --render "$tmp/and.tsv" "$host" "$tmp/and.md"
grep -x -F 'A notice of the code vendoring carries.' "$tmp/and.md" >/dev/null || fail "a notice in a subfolder is printed"
grep -F '`third_party/inner/LICENSE-APACHE`' "$tmp/and.md" >/dev/null || fail "a text in a subfolder is named by its path in the crate"
rm "$tmp/crates/vendoring/third_party/inner/LICENSE-APACHE"
expect_fail "a crate under two licences with the text of one" "vendoring 6.0.0 (Apache-2.0 AND ISC): no text of Apache-2.0 among its files" \
  "$script" --render "$tmp/and.tsv" "$host" "$tmp/x.md"
# A choice of licences needs the text of one of them; a part that is a choice, likewise.
line vendoring 6.0.0 "(MIT OR Apache-2.0) AND ISC" >"$tmp/and.tsv"
expect_fail "a part that is a choice, with a text of neither" "no text of MIT or Apache-2.0 among its files" "$script" --render "$tmp/and.tsv" "$host" "$tmp/x.md"
line vendoring 6.0.0 "MIT OR ISC" >"$tmp/and.tsv"
expect_pass "a choice of licences with the text of one" "$script" --render "$tmp/and.tsv" "$host" "$tmp/or.md"
# The older form of a choice, with `/`, as crates.io still shows it for some crates.
line vendoring 6.0.0 "MIT/ISC" >"$tmp/and.tsv"
expect_pass "a choice written with a slash, with the text of one" "$script" --render "$tmp/and.tsv" "$host" "$tmp/or.md"
line vendoring 6.0.0 "MIT/Apache-2.0" >"$tmp/and.tsv"
expect_fail "a choice written with a slash, with a text of neither" "no text of MIT or Apache-2.0 among its files" "$script" --render "$tmp/and.tsv" "$host" "$tmp/x.md"
line vendoring 6.0.0 "Apache-2.0 WITH LLVM-exception OR ISC" >"$tmp/and.tsv"
expect_pass "an exception is not a licence to find" "$script" --render "$tmp/and.tsv" "$host" "$tmp/or.md"
# A licence the script knows no words of, and expressions it cannot read.
line vendoring 6.0.0 "Made-Up-1.0" >"$tmp/and.tsv"
expect_fail "a licence it knows no words of" "no text of Made-Up-1.0 among its files" "$script" --render "$tmp/and.tsv" "$host" "$tmp/x.md"
for unread in "MIT OR Apache-2.0 AND ISC" "MIT OR (Apache-2.0 AND ISC)" "(MIT AND ISC" "MIT AND"; do
  line vendoring 6.0.0 "$unread" >"$tmp/and.tsv"
  expect_fail "the expression '$unread'" "is not an expression this script can read" "$script" --render "$tmp/and.tsv" "$host" "$tmp/x.md"
done

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

# --- The interface's packages and fonts, held to ui/: on copies of the files it reads, in
# a throwaway repository, with the crates of this test's own good list.
ui_copy() {
  rm -rf "$tmp/uirepo"
  mkdir -p "$tmp/uirepo/scripts" "$tmp/uirepo/ui/public"
  cp scripts/third-party-licences.sh "$tmp/uirepo/scripts/"
  cp deny.toml Cargo.toml "$tmp/uirepo/"
  cp ui/package.json ui/package-lock.json "$tmp/uirepo/ui/"
  cp -R ui/licences "$tmp/uirepo/ui/"
  cp -R ui/public/fonts "$tmp/uirepo/ui/public/"
  mkdir -p "$tmp/uirepo/ui/public/brand"
  cp ui/public/brand/wordmark.svg "$tmp/uirepo/ui/public/brand/"
  git -C "$tmp/uirepo" init -q
}
render_ui() { (cd "$tmp/uirepo" && scripts/third-party-licences.sh --render "$tmp/good.tsv" "$host" "$tmp/ui.md"); }
edit_list() { sed "$1" "$tmp/uirepo/ui/licences/list.tsv" >"$tmp/list.tsv" && mv "$tmp/list.tsv" "$tmp/uirepo/ui/licences/list.tsv"; }
ui_copy
expect_pass "the interface as it is" render_ui
grep -x -F '### The font geist' "$tmp/ui.md" >/dev/null || fail "the copy's file holds the fonts"
ui_copy
jq '.dependencies.extra = "1.0.0"' ui/package.json >"$tmp/uirepo/ui/package.json"
expect_fail "a dependency with no row" "must hold one row for each dependency of ui/package.json" render_ui
ui_copy
edit_list 's/^package	preact	10.29.8	/package	preact	10.0.0	/'
expect_fail "a row at another version than the lock file's" "says preact 10.0.0, and ui/package-lock.json pins 10.29.8" render_ui
ui_copy
edit_list 's/^package	preact	10.29.8	MIT	/package	preact	10.29.8	GPL-3.0	/'
expect_fail "a package under a licence deny.toml does not allow" "under GPL-3.0, which deny.toml does not allow" render_ui
ui_copy
: >"$tmp/uirepo/ui/public/fonts/extra.woff2"
expect_fail "a font file with no row" "must name each file of ui/public/fonts/ once" render_ui
ui_copy
edit_list 's/^font	geist	5.3.0	OFL-1.1	/font	geist	5.3.0	Apache-2.0	/'
expect_fail "a font not under OFL-1.1" "a font is under OFL-1.1 only" render_ui
ui_copy
echo "Words of no licence." >"$tmp/uirepo/ui/licences/preact.txt"
expect_fail "a text that is not of its licence" "ui/licences/preact.txt is not a text of MIT" render_ui
ui_copy
rm "$tmp/uirepo/ui/licences/geist-mono.txt"
expect_fail "a text that is missing" "ui/licences/geist-mono.txt, is missing or empty" render_ui
ui_copy
printf 'x' >>"$tmp/uirepo/ui/public/fonts/geist-400.woff2"
expect_fail "a font file that is not the one published" "ui/public/fonts/geist-400.woff2 is not the file ui/licences/fonts.sha256 names" render_ui
ui_copy
grep -v 'geist-500' "$tmp/uirepo/ui/licences/fonts.sha256" >"$tmp/sums" && mv "$tmp/sums" "$tmp/uirepo/ui/licences/fonts.sha256"
expect_fail "a font file with no hash" "ui/licences/fonts.sha256 must hold each file of ui/public/fonts/ once" render_ui
ui_copy
rm "$tmp/uirepo/ui/public/brand/wordmark.svg"
expect_fail "an outlined font whose file is missing" "names ui/public/brand/wordmark.svg, which is missing" render_ui
ui_copy
edit_list 's/^outlined	roboto-slab	2.002	Apache-2.0	/outlined	roboto-slab	2.002	GPL-3.0	/'
expect_fail "an outlined font under a licence deny.toml does not allow" "outlined font roboto-slab is under GPL-3.0" render_ui
ui_copy
edit_list 's/	npm @fontsource\/geist-sans@5.3.0	/		/'
expect_fail "a font with no source" "a licence, a text and a source: font geist" render_ui
[ ! -e "$tmp/x.md" ] || fail "a refused interface writes no file"

finished
