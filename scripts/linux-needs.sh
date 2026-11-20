#!/usr/bin/env bash
# Holds a Linux binary to what a bare Ubuntu 22.04 has (plan m4, task R1, item 2, and
# "Owed after R1", item 2): scripts/package.sh runs it on the release binary it is about
# to archive, on the Linux target. The fast test
# `release::the_linux_binary_needs_only_the_c_library_of_ubuntu_22_04` holds the same rule
# on the binary cargo builds for the tests; this is the read of the binary that ships.
#
#   scripts/linux-needs.sh <binary>
#
# The binary is read with `readelf` and never started. It passes when:
#
# - every shared library it names is a file of the C library (libc.so.6, libm.so.6,
#   libpthread.so.0, libdl.so.2, librt.so.1, libutil.so.1, ld-linux-x86-64.so.2) or the
#   compiler's runtime library the C library itself depends on (libgcc_s.so.1);
# - every version of a symbol it asks of the C library is `GLIBC_` and numbers, at most
#   2.35, which is Ubuntu 22.04's; and of libgcc_s.so.1, `GCC_` and numbers of GCC 12 or
#   older;
# - the reader found the C library among the libraries and a version asked of it: a
#   reader that finds nothing does not pass.
#
# Exit status 1 names what the binary needs that it may not; 2 is a usage error.
# This script sets no `EXIT` trap, so its own syntax error is a failure; parse it first all
# the same: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "linux-needs: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C

die() {
  echo "linux-needs: $*" >&2
  exit 1
}
[ $# -eq 1 ] || { echo "usage: scripts/linux-needs.sh <binary>" >&2; exit 2; }
binary=$1
[ -f "$binary" ] || die "there is no binary at $binary"
command -v readelf >/dev/null || die "readelf is not installed, and the binary is read with it"

# Both reads are kept whole before either is looked at: a reader that failed half-way
# must not pass for a binary that needs little.
libraries=$(readelf -W -d "$binary") || die "readelf could not read the binary's dynamic section"
versions=$(readelf -W -V "$binary") || die "readelf could not read the binary's version needs"

# The libraries: `... (NEEDED)   Shared library: [libc.so.6]`, one a line.
named=$(printf '%s\n' "$libraries" | sed -n 's/^.*(NEEDED).*\[\(.*\)\][[:space:]]*$/\1/p')
[ -n "$named" ] || die "the reader found no shared library the binary names"
printf '%s\n' "$named" | grep -x -F 'libc.so.6' >/dev/null || die "the reader did not find the C library among the libraries the binary names"
other=$(printf '%s\n' "$named" |
  grep -v -x -E 'libc\.so\.6|libm\.so\.6|libpthread\.so\.0|libdl\.so\.2|librt\.so\.1|libutil\.so\.1|ld-linux-x86-64\.so\.2|libgcc_s\.so\.1' || true)
[ -z "$other" ] || die "the binary needs a library a bare Ubuntu 22.04 may not hold: $(printf '%s' "$other" | tr '\n' ' ')"

# The versions: under `Version needs section`, a line `File: <library>` and then one line
# `Name: <version>` for each version asked of it. Printed as `<library> <version>`.
asked=$(printf '%s\n' "$versions" | awk '
  /^Version needs section/ { on = 1; file = ""; next }
  /^Version (symbols|definition) section/ { on = 0 }
  on && /File: / { file = $0; sub(/^.*File: */, "", file); sub(/[[:space:]].*$/, "", file); next }
  on && /Name: / { name = $0; sub(/^.*Name: */, "", name); sub(/[[:space:]].*$/, "", name); print (file == "" ? "?" : file) " " name }')
printf '%s\n' "$asked" | grep -E '^libc\.so\.6 GLIBC_' >/dev/null || die "the reader found no version the binary asks of the C library"
bad=$(printf '%s\n' "$asked" | awk '
  # numbers(text): the count of numbers in a text of numbers apart by dots, into n[]; 0 when it is anything else.
  function numbers(text,   count, i) {
    if (text !~ /^[0-9]+(\.[0-9]+)*$/) return 0
    count = split(text, n, ".")
    for (i = 1; i <= count; i++) n[i] += 0
    return count
  }
  {
    file = $1; version = $2; ok = 0
    if (file == "libgcc_s.so.1") {
      if (index(version, "GCC_") == 1 && numbers(substr(version, 5)) > 0 && n[1] <= 12) ok = 1
    } else if (file ~ /^(libc\.so\.6|libm\.so\.6|libpthread\.so\.0|libdl\.so\.2|librt\.so\.1|libutil\.so\.1|ld-linux-x86-64\.so\.2)$/) {
      if (index(version, "GLIBC_") == 1) {
        count = numbers(substr(version, 7))
        # At most 2.35, as lists of numbers are compared: the first below 2; or 2 alone;
        # or 2 and then less than 35; or 2.35 itself, with nothing after it.
        if (count > 0 && (n[1] < 2 || (n[1] == 2 && (count == 1 || n[2] < 35 || (n[2] == 35 && count == 2))))) ok = 1
      }
    }
    if (!ok) print file " " version
  }')
[ -z "$bad" ] ||
  die "the binary asks for what Ubuntu 22.04 (glibc 2.35, GCC 12) does not have: $(printf '%s' "$bad" | tr '\n' ';' | sed 's/;/; /g')"
echo "linux-needs: ok: $(printf '%s\n' "$named" | wc -l | tr -d ' ') libraries, all the C library's or the compiler's runtime; $(printf '%s\n' "$asked" | wc -l | tr -d ' ') versions asked, none newer than Ubuntu 22.04's"
}
main "$@"; exit
