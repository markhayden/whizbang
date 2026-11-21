#!/usr/bin/env bash
# `scripts/ui.sh check` (`just ui-check`, in `ci-without-scripts` and so in the jobs
# `check (<runner>)`) fails on a type error, on a stale ui/dist/, on a file of ui/dist/ that
# git does not track, and on a host's list of embedded files that is not ui/dist/; and it
# passes on the tree as it is (plan m5, task U1, item 6). Each case runs in a throwaway
# copy of ui/, with the packages of this checkout, so nothing here is changed. Needs the
# pinned Node; the packages are installed first when they are missing.
source "$(dirname "$0")/lib.sh"

scripts/ui.sh install >/dev/null
copy=$tmp/repo
mkdir -p "$copy/scripts" "$copy/crates/whizbang-serve/src/play"
cp scripts/ui.sh scripts/tool-versions.txt "$copy/scripts/"
cp crates/whizbang-serve/src/play/assets.rs "$copy/crates/whizbang-serve/src/play/"
# ui/ as git tracks it, and the packages of this checkout beside it, older than its lock
# file so that the copy does not install them again.
git ls-files ui | while IFS= read -r path; do
  mkdir -p "$copy/$(dirname "$path")"
  cp "$path" "$copy/$path"
done
ln -s "$root/ui/node_modules" "$copy/ui/node_modules"
touch -r ui/node_modules/.package-lock.json "$copy/ui/package-lock.json"
(cd "$copy" && git init -q && git add -A && git -c user.name=test -c user.email=test@example.invalid commit -q -m copy)
check() { (cd "$copy" && scripts/ui.sh check); }
restore() { (cd "$copy" && git checkout -q -- . && git clean -q -f -d -e ui/node_modules); }

expect_pass "the tree as it is" check
grep -q -F "ui/dist/ is what the sources build" "$tmp/out" || { cat "$tmp/out"; fail "says what passed"; }
[ -s "$copy/target/ui/unit-seconds" ] && [ -s "$copy/target/ui/unit-junit.xml" ] ||
  fail "leaves the unit tests' seconds and results for scripts/timing.sh"

# A type error: a number where a string is wanted.
printf '\nexport const wrong: string = 1;\n' >>"$copy/ui/src/format.ts"
expect_fail "a type error" "the type check failed" check
restore

# A stale ui/dist/: a source changed and not built again.
sed -i.bak 's/is kept in the store and shows here\./is kept in the store./' "$copy/ui/src/screens/homepage/Home.tsx"
rm "$copy/ui/src/screens/homepage/Home.tsx.bak"
grep -q -F 'is kept in the store.<' "$copy/ui/src/screens/homepage/Home.tsx" || fail "the edit of a source took"
expect_fail "a stale ui/dist/" "ui/dist/ is not what the sources build" check
restore

# A file of ui/dist/ that git does not track.
git -C "$copy" rm -q --cached ui/dist/fonts/geist-700.woff2
expect_fail "an untracked file of ui/dist/" "ui/dist/ and what git tracks of it differ" check
restore
git -C "$copy" reset -q --hard

# The host's list without a file of ui/dist/, and with one that is not there.
sed -i.bak '/fonts\/geist-700.woff2/d' "$copy/crates/whizbang-serve/src/play/assets.rs"
expect_fail "a list without a built file" "must list exactly the files of ui/dist/" check
restore
sed -i.bak 's|asset!("app.css", |asset!("app.css", "text/css"),\n    asset!("old.css", |' "$copy/crates/whizbang-serve/src/play/assets.rs"
expect_fail "a list with a file that is not built" "must list exactly the files of ui/dist/" check
restore

finished
