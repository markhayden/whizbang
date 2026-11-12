#!/usr/bin/env bash
# Names the scan must not trip over, or be fooled by. Composing: where git composes the
# names it reads from a folder (macOS, with core.precomposeUnicode), the census composes
# its names the same way, with iconv. It does so nowhere else, with one iconv for
# hundreds of names, and it does not take iconv's word: a list that comes back shorter, with two names
# made one, or with a name that no longer leads to its file, fails the scan. Tested with
# stand-ins for uname and iconv, so every case runs on every system; the cases with real
# decomposed names are in walk.sh. Then: links and message files whose names start with
# a dash.
source "$(dirname "$0")/common.sh"
never_clean() { grep -a -q -e "secrets scan: clean" "$tmp/out" && fail "$1: the scan does not say clean"; return 0; }

real_iconv=$(command -v iconv)
mkdir "$tmp/os-bin"
printf '#!/bin/sh\necho "$UNAME_SAYS"\n' >"$tmp/os-bin/uname"
cat >"$tmp/os-bin/iconv" <<SH
#!/bin/sh
echo x >>"$tmp/iconv-calls"
# After the two encodings: the files to convert, or none for what comes on stdin.
shift 4
case "\$ICONV_DOES" in
  nothing) cat -- "\$@" >/dev/null ;;
  same) cat -- "\$@" ;;
  merge) cat -- "\$@" | tr 1 2 ;;
  rename) cat -- "\$@" | tr q z ;;
  shorten) cat -- "\$@" | perl -pe 's|/dir-[^/\\0]*||g' ;;
  hollow) cat -- "\$@" | perl -pe 's|dir-[^/\\0]*||g' ;;
  picky)
    cat -- "\$@" >"$tmp/iconv-in.\$\$"
    if grep -a -q -e refuse "$tmp/iconv-in.\$\$"; then
      echo "iconv: it refuses" >&2
      exit 1
    fi
    cat "$tmp/iconv-in.\$\$"
    ;;
  real) exec "$real_iconv" -f UTF-8-MAC -t UTF-8 "\$@" ;;
esac
SH
chmod +x "$tmp/os-bin/uname" "$tmp/os-bin/iconv"
# as_os <what uname says> <what iconv does> <command...>
as_os() {
  local os=$1 does=$2
  shift 2
  : >"$tmp/iconv-calls"
  env PATH="$tmp/os-bin:$PATH" UNAME_SAYS="$os" ICONV_DOES="$does" "$@"
}
calls() { wc -l <"$tmp/iconv-calls" | tr -d ' '; }
not_composed="could not be composed as git composes them"

# Names with an accent, composed already: iconv has nothing to change in them, so the
# stand-ins decide what happens.
fresh composing
nfc=$(printf 'caf\303\251')
i=1
while [ "$i" -le 20 ]; do
  printf 'nothing to see\n' >"$nfc-$i.txt"
  i=$((i + 1))
done
printf 'nothing to see\n' >"q-$nfc.txt"
mkdir "dir-$nfc"
printf 'nothing to see\n' >"dir-$nfc/in-$nfc.txt"

# Not macOS: iconv is never run, whatever the setting says. (Here an iconv that printed
# nothing would empty the list.)
git config core.precomposeunicode true
expect_pass "the setting on, not on macOS" as_os Linux nothing "$scan"
says "secrets scan: clean (23 files)"
[ "$(calls)" = 0 ] || fail "iconv is not run where git does not compose (ran $(calls) times)"
# macOS with the setting off: the same.
git config core.precomposeunicode false
expect_pass "the setting off, on macOS" as_os Darwin nothing "$scan"
says "secrets scan: clean (23 files)"
[ "$(calls)" = 0 ] || fail "iconv is not run with the setting off (ran $(calls) times)"
git config --unset core.precomposeunicode
expect_pass "the setting unset, on macOS" as_os Darwin nothing "$scan"
[ "$(calls)" = 0 ] || fail "iconv is not run with the setting unset (ran $(calls) times)"

# macOS with the setting on: iconv is run, a few times for the whole tree and not once
# for each name.
git config core.precomposeunicode true
expect_pass "the setting on, on macOS" as_os Darwin same "$scan"
says "secrets scan: clean (23 files)"
[ "$(calls)" -ge 1 ] || fail "iconv is run where git composes"
[ "$(calls)" -le 6 ] || fail "iconv is run for a whole list at once (ran $(calls) times for 22 names)"
# No name with an accent: iconv is not run at all.
mkdir "$tmp/aside"
mv ./*"$nfc"* "$tmp/aside/"
expect_pass "no name with an accent, on macOS" as_os Darwin nothing "$scan"
[ "$(calls)" = 0 ] || fail "iconv is not run when every name is ASCII (ran $(calls) times)"
mv "$tmp/aside"/* .

# An iconv that is wrong fails the scan; the names are counted, not shown.
expect_fail "an iconv that prints nothing" "$not_composed (iconv gave 0 names for " as_os Darwin nothing "$scan"
says "refusing to pass"
never_clean "an iconv that prints nothing"
expect_fail "an iconv that makes two names one" "$not_composed (23 names became 20)" as_os Darwin merge "$scan"
quiet "$nfc"
never_clean "an iconv that makes two names one"
expect_fail "an iconv that gives a name no file has" "$not_composed (1 names no longer lead to what they named)" \
  as_os Darwin rename "$scan"
quiet "$nfc"
never_clean "an iconv that gives a name no file has"

# Nor may a name lose a part: a folder that came back as the folder it is in would be
# walked again and again. (The scan is given a minute, in case it does just that.)
expect_fail "an iconv that takes a part out of a path" "$not_composed (1 names no longer lead to what they named)" \
  as_os Darwin shorten perl -e 'alarm 60; exec @ARGV' "$scan"
never_clean "an iconv that takes a part out of a path"
expect_fail "an iconv that leaves a part of a path empty" "$not_composed (1 names no longer lead to what they named)" \
  as_os Darwin hollow perl -e 'alarm 60; exec @ARGV' "$scan"
never_clean "an iconv that leaves a part of a path empty"

# A list iconv refuses as a whole is composed name by name, and a name it refuses, part
# by part; a part it refuses stays as it is. A name that ends in a newline keeps it.
mkdir "refuse-$nfc"
printf 'nothing to see\n' >"refuse-$nfc/in-$nfc.txt"
printf 'nothing to see\n' >"refuse-$nfc.txt"
ends=$(printf 'ends-%s\n.' "$nfc")
ends=${ends%.}
printf 'nothing to see\n' >"$ends"
expect_pass "a list iconv refuses as a whole" as_os Darwin picky "$scan"
says "secrets scan: clean (26 files)"
[ "$(calls)" -gt 6 ] || fail "the names were composed one by one (iconv ran $(calls) times)"
printf 'token = %s\n' "$key" >"$ends"
expect_fail "a key in a file whose name ends in a newline, composed by itself" ":1" as_os Darwin picky "$scan"
printf 'token = %s\n' "$key" >"refuse-$nfc/in-$nfc.txt"
printf 'nothing to see\n' >"$ends"
expect_fail "a key in a file under a folder whose name iconv refuses" ":1" as_os Darwin picky "$scan"
quiet "$not_composed"
printf 'nothing to see\n' >"refuse-$nfc/in-$nfc.txt"
# A name that is not UTF-8, where the file system allows one, with the real iconv.
bad=$(printf 'bad-\377-%s.txt' "$nfc")
if (printf 'nothing to see\n' >"$bad") 2>/dev/null; then
  expect_pass "a name that is not UTF-8" as_os Darwin real "$scan"
  says "secrets scan: clean (27 files)"
  rm "$bad"
else
  echo "skip: a name that is not UTF-8; this file system does not allow one"
fi
git config --unset core.precomposeunicode

# --- Names that start with a dash -----------------------------------------------------
# A link is read with readlink, which must not take its name for an option.
fresh dash-names
for name in -dash -- -n; do
  ln -s readme.txt "./$name"
  expect_pass "a link named $name" "$scan"
  says "secrets scan: clean (1 files)"
  rm "./$name"
  ln -s "it-says-$word" "./$name"
  expect_fail "a link named $name whose target holds a listed word" "clean-room scan: 1 matching lines (contents 1, paths 0)" "$scan"
  rm "./$name"
done
# The same for a message file.
mkdir "$tmp/messages"
printf 'fix: a thing\n' >"$tmp/messages/-n"
(cd "$tmp/messages" && expect_pass "a message file named -n" "$scan" --message -n)
printf 'fix: a thing\n\n%s: Someone <someone@example.com>\n' "$credit" >"$tmp/messages/-n"
(cd "$tmp/messages" && expect_fail "a message file named -n, with a credit line" "$has_credit" "$scan" --message -n)

finished
