#!/usr/bin/env bash
# What a Linux binary may need of the machine (scripts/linux-needs.sh; plan m4, "Owed after
# R1", item 2): the read scripts/package.sh makes of the release binary it archives.
#
# `readelf` here is a stand-in that prints what the real one prints for a binary, so the
# test runs on a Mac too and reads no real binary; the real read is made in CI's `release`
# job on the Linux runner, by `just package`, on the binary that is packaged.
source "$(dirname "$0")/lib.sh"

script=$root/scripts/linux-needs.sh
mkdir "$tmp/bin"
cat >"$tmp/bin/readelf" <<'SH'
#!/bin/bash
# readelf -W -d <binary> | readelf -W -V <binary>: the file beside the binary.
[ "$1" = -W ] || exit 64
case "$2" in
  -d) [ -f "$3.dynamic" ] || exit 1; cat "$3.dynamic" ;;
  -V) [ -f "$3.versions" ] || exit 1; cat "$3.versions" ;;
  *) exit 64 ;;
esac
SH
chmod +x "$tmp/bin/readelf"
needs() { env PATH="$tmp/bin:$PATH" "$script" "$@"; }

# binary <name> <libraries...>: a binary whose dynamic section names those libraries, and
# whose version needs are read from the standard input as `<library> <version>` lines.
binary() {
  local file="$tmp/$1" library
  shift
  echo "not read" >"$file"
  {
    echo
    echo "Dynamic section at offset 0xa4b8f0 contains 31 entries:"
    echo "  Tag        Type                         Name/Value"
    for library in "$@"; do
      echo " 0x0000000000000001 (NEEDED)             Shared library: [$library]"
    done
    echo " 0x000000000000001e (FLAGS)              BIND_NOW"
    echo " 0x0000000000000000 (NULL)               0x0"
  } >"$file.dynamic"
  {
    echo
    echo "Version symbols section '.gnu.version' contains 3 entries:"
    echo " Addr: 0x0000000000000a10  Offset: 0x000a10  Link: 6 (.dynsym)"
    echo "  000:   0 (*local*)       2 (GLIBC_2.99)    3 (GCC_99.0)"
    echo
    echo "Version needs section '.gnu.version_r' contains 4 entries:"
    echo " Addr: 0x0000000000000b98  Offset: 0x000b98  Link: 7 (.dynstr)"
    awk '{ if ($1 != last) { printf "  0x%04x: Version: 1  File: %s  Cnt: 1\n", NR * 32, $1; last = $1 }
           printf "  0x%04x:   Name: %s  Flags: none  Version: %d\n", NR * 32 + 16, $2, NR + 1 }'
  } >"$file.versions"
}

# --- A binary as the release build gives it passes.
binary good libgcc_s.so.1 libm.so.6 libc.so.6 ld-linux-x86-64.so.2 <<'NEEDS'
libgcc_s.so.1 GCC_3.0
libgcc_s.so.1 GCC_3.3
libgcc_s.so.1 GCC_4.2.0
libm.so.6 GLIBC_2.2.5
libm.so.6 GLIBC_2.29
libc.so.6 GLIBC_2.2.5
libc.so.6 GLIBC_2.18
libc.so.6 GLIBC_2.34
libc.so.6 GLIBC_2.35
ld-linux-x86-64.so.2 GLIBC_2.3
NEEDS
expect_pass "a binary that needs the C library of Ubuntu 22.04" needs "$tmp/good"
grep -F "4 libraries" "$tmp/out" >/dev/null && grep -F "10 versions asked" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "it says what it read"; }

# --- A library that is not the C library's.
binary openssl libc.so.6 libssl.so.3 <<'NEEDS'
libc.so.6 GLIBC_2.34
NEEDS
expect_fail "another library" "needs a library a bare Ubuntu 22.04 may not hold: libssl.so.3" needs "$tmp/openssl"

# --- A version newer than Ubuntu 22.04 has, of the C library and of the compiler's runtime;
# and a version that is no number.
refused() { # <name> <library> <version>
  binary "$1" libc.so.6 libm.so.6 libgcc_s.so.1 <<NEEDS
libc.so.6 GLIBC_2.34
$2 $3
NEEDS
  expect_fail "$1" "does not have: $2 $3" needs "$tmp/$1"
}
refused newer libc.so.6 GLIBC_2.36
refused newer-patch libc.so.6 GLIBC_2.35.1
refused major libc.so.6 GLIBC_3.0
refused in-another-file libm.so.6 GLIBC_2.38
refused private libc.so.6 GLIBC_PRIVATE
refused other-prefix libc.so.6 GCC_3.0
refused gcc libgcc_s.so.1 GCC_13.0
refused gcc-prefix libgcc_s.so.1 GLIBC_2.2.5

# --- A reader that finds nothing does not pass: no library, no C library, no version of it.
binary nothing <<'NEEDS'
NEEDS
expect_fail "no library found" "found no shared library" needs "$tmp/nothing"
binary no-libc libm.so.6 <<'NEEDS'
libm.so.6 GLIBC_2.29
NEEDS
expect_fail "no C library found" "did not find the C library" needs "$tmp/no-libc"
binary no-versions libc.so.6 <<'NEEDS'
NEEDS
expect_fail "no version found" "found no version the binary asks of the C library" needs "$tmp/no-versions"
# A readelf that fails on either read, and a binary that is not there.
binary half libc.so.6 <<'NEEDS'
libc.so.6 GLIBC_2.34
NEEDS
rm "$tmp/half.versions"
expect_fail "a read that fails" "could not read the binary's version needs" needs "$tmp/half"
expect_fail "no binary" "there is no binary" needs "$tmp/no-such"
status=0
needs >"$tmp/out" 2>&1 || status=$?
[ "$status" = 2 ] || fail "no argument is a usage error (got $status)"

finished
