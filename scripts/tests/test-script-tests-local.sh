#!/usr/bin/env bash
# Which script tests the local `just check` runs (scripts/script-tests-local.sh), shown on
# changes planted in a throwaway repository with its own origin/main, and run with
# --dry-run, which prints the choice and runs nothing. And the `--only` of
# scripts/tests/run.sh, which runs the part it chose. CI runs every script test whatever
# changed: scripts/tests/test-ci-jobs.sh holds that.
source "$(dirname "$0")/lib.sh"

gate="$root/scripts/script-tests-local.sh"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
unset WHIZBANG_SCRIPT_TESTS

repo="$tmp/repo"
git init -q "$repo"
git -C "$repo" config user.name "A Tester"
git -C "$repo" config user.email "tester@example.com"
mkdir -p "$repo/crates/a/src" "$repo/scripts/tests" "$repo/pack" "$repo/docs"
printf '[package]\nname = "a"\n' >"$repo/crates/a/Cargo.toml"
printf 'pub fn a() {}\n' >"$repo/crates/a/src/lib.rs"
printf 'echo x\n' >"$repo/scripts/x.sh"
printf 'x = 1\n' >"$repo/pack/x.py"
printf 'notes\n' >"$repo/docs/notes.md"
printf 'default:\n' >"$repo/justfile"
printf 'readme\n' >"$repo/README.md"
git -C "$repo" add -A
git -C "$repo" commit -q -m "chore: start"
git -C "$repo" update-ref refs/remotes/origin/main HEAD
git -C "$repo" switch -q -c work

# choice: what the gate prints for the repo as it is now.
choice() { (cd "$repo" && "$gate" --dry-run) >"$tmp/out" 2>&1 || { cat "$tmp/out" >&2; fail "the gate failed"; }; }
# says <case> <text>: the choice, printed after `choice`, holds the text.
says() { grep -F -e "$2" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "$1: expected the output to say: $2"; }; }
everything='would run: scripts/tests/run.sh'
# ran_all <case> <why>
ran_all() {
  choice
  says "$1" "script tests: ran all of them: $2"
  [ "$(sed -n 2p "$tmp/out")" = "$everything" ] || { cat "$tmp/out" >&2; fail "$1: expected every script test"; }
}
# ran_part <case> <why>
ran_part() {
  choice
  says "$1" "script tests: ran the "
  says "$1" "$2"
  says "$1" "; skipped the rest: no change under scripts/, justfile, .github/, .config/, .claude/"
  case "$(sed -n 2p "$tmp/out")" in "$everything --only "*) ;; *) cat "$tmp/out" >&2; fail "$1: expected some of the script tests" ;; esac
}
skipped() { # <case>
  choice
  says "$1" "script tests: skipped: no change since the merge-base with origin/main"
  [ "$(sed -n 2p "$tmp/out")" = "would run: nothing" ] || { cat "$tmp/out" >&2; fail "$1: expected no script test"; }
}
# One line says what was chosen and why, and one names what would run.
skipped "nothing changed"
[ "$(awk 'END { print NR }' "$tmp/out")" = 2 ] || { cat "$tmp/out" >&2; fail "two lines, the choice and what would run"; }

# --- A change to the crates' sources alone: the tests that read them, not the rest.
printf 'pub fn b() {}\n' >>"$repo/crates/a/src/lib.rs"
ran_part "a Rust change in the working tree" "(crates/a/src/lib.rs changed)"
part=$(sed -n 2p "$tmp/out" | sed "s|^$everything --only ||")
git -C "$repo" commit -q -am "feat: b"
ran_part "a Rust change committed on the branch" "(crates/a/src/lib.rs changed)"
# Each test of the part is one of this repo's script tests, and the ones that read the
# crates' sources or every tracked file are among them.
for t in $part; do
  case "$t" in scripts/tests/test-*.sh) ;; *) fail "the part names something else than a script test: $t" ;; esac
  [ -f "$root/$t" ] || fail "the part names a script test that is not there: $t"
done
for t in test-binary-start test-sign-in test-thread-lint test-notes test-pack test-compile-fail test-baseline test-deny test-sets; do
  case " $part " in *" scripts/tests/$t.sh "*) ;; *) fail "the part leaves out $t, which reads the crates' sources or every tracked file" ;; esac
done
[ "$(ls "$root"/scripts/tests/test-*.sh | wc -l)" -gt "$(echo "$part" | wc -w)" ] || fail "the part is every script test"
git -C "$repo" commit -q --allow-empty -m "chore: nothing"
printf 'fn c() {}\n' >"$repo/crates/a/src/new.rs"
ran_part "a file under crates/ added and not yet tracked" "changed)"
grep -F -e "crates/a/src/new.rs" "$tmp/out" >/dev/null || grep -F -e "crates/a/src/lib.rs" "$tmp/out" >/dev/null ||
  { cat "$tmp/out" >&2; fail "the new file or the committed change is named"; }
rm "$repo/crates/a/src/new.rs"
printf 'x\n' >"$repo/README.md"
ran_part "a file outside the list" "changed)"
git -C "$repo" checkout -q -- README.md
git -C "$repo" rm -q crates/a/src/lib.rs
ran_part "a Rust file removed" "(crates/a/src/lib.rs changed)"
git -C "$repo" commit -q -m "refactor: no lib"

# --- What any script test may read: all of them.
printf 'echo y\n' >"$repo/scripts/new.sh"
ran_all "a new script, not yet tracked" "scripts/new.sh changed since the merge-base with origin/main"
rm "$repo/scripts/new.sh"
ran_part "the script taken away again: what the branch committed under crates/ is left" "changed)"
printf 'y = 2\n' >>"$repo/pack/x.py"
ran_all "a change under pack/ (the uv rules of test-pack read it)" "pack/x.py changed"
git -C "$repo" checkout -q -- pack/x.py
mkdir -p "$repo/tools/sets"
printf 'y = 2\n' >"$repo/tools/sets/x.py"
ran_all "a file under tools/ (test-sets runs the set builder)" "tools/sets/x.py changed"
rm -r "$repo/tools"
printf 'check:\n' >>"$repo/justfile"
git -C "$repo" add justfile
ran_all "a staged change to the justfile" "justfile changed"
git -C "$repo" reset -q justfile
git -C "$repo" checkout -q -- justfile
for planted in .github/workflows/ci.yml .config/nextest.toml .claude/settings.json clippy.toml deny.toml \
  AGENTS.md CLAUDE.md SPEC.md Cargo.toml Cargo.lock rust-toolchain.toml .gitignore docs/plans/x.md \
  crates/a/Cargo.toml crates/a/build.rs crates/a/.claude/x.md crates/b/.CLAUDE/x.md crates/a/.mcp.json; do
  mkdir -p "$repo/$(dirname "$planted")"
  printf 'x\n' >>"$repo/$planted"
  ran_all "a change to $planted" "$planted changed"
  git -C "$repo" checkout -q -- "$planted" 2>/dev/null || rm "$repo/$planted"
done
# A file moved from scripts/ to crates/: both sides count.
git -C "$repo" mv scripts/x.sh crates/a/x.sh
ran_all "a script moved under crates/" "scripts/x.sh changed"
git -C "$repo" mv crates/a/x.sh scripts/x.sh

# --- What origin/main gained since the branch left it is not the branch's change.
git -C "$repo" switch -q -c ahead origin/main
printf 'echo z\n' >>"$repo/scripts/x.sh"
git -C "$repo" commit -q -am "build: z"
git -C "$repo" update-ref refs/remotes/origin/main HEAD
git -C "$repo" switch -q work
ran_part "main moved ahead under scripts/; the branch changed only crates/" "(crates/a/src/lib.rs changed)"

# --- The variable forces them all, and is 1 or unset.
(cd "$repo" && WHIZBANG_SCRIPT_TESTS=1 "$gate" --dry-run) >"$tmp/out" 2>&1 || fail "the gate failed when forced"
says "forced" "script tests: ran all of them: WHIZBANG_SCRIPT_TESTS=1"
[ "$(sed -n 2p "$tmp/out")" = "$everything" ] || fail "forced: every script test"
(cd "$repo" && WHIZBANG_SCRIPT_TESTS= "$gate" --dry-run) >"$tmp/out" 2>&1 || fail "an empty variable is unset"
says "an empty variable" "script tests: ran the "
expect_fail "the variable set to anything else" "WHIZBANG_SCRIPT_TESTS must be 1 or unset" \
  env WHIZBANG_SCRIPT_TESTS=0 bash -c 'cd "$1" && "$2" --dry-run' - "$repo" "$gate"
expect_fail "an argument it does not know" "usage" bash -c 'cd "$1" && "$2" --everything' - "$repo" "$gate"

# --- Without origin/main to compare with, all of them.
git -C "$repo" update-ref -d refs/remotes/origin/main
ran_all "no origin/main" "no merge-base with origin/main to compare with"

# --- run.sh --only: the named tests and no other, each held to the same rule.
only="$tmp/only"
mkdir -p "$only"
for name in a b c; do
  printf '%s\n' 'echo "ran: $0"' 'echo "finished: $0"' >"$only/test-$name.sh"
done
expect_pass "--only two tests" scripts/tests/run.sh --only "$only/test-a.sh" "$only/test-c.sh"
grep -F -e "ok    $only/test-a.sh" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "--only runs the first"; }
grep -F -e "ok    $only/test-c.sh" "$tmp/out" >/dev/null || { cat "$tmp/out" >&2; fail "--only runs the second"; }
grep -F -e "test-b.sh" "$tmp/out" >/dev/null && { cat "$tmp/out" >&2; fail "--only runs nothing else"; }
printf '%s\n' 'echo "stopped early"' >"$only/test-b.sh"
expect_fail "--only, a test that did not finish" "FAIL  $only/test-b.sh" scripts/tests/run.sh --only "$only/test-b.sh"
expect_fail "--only, a test that is not there" "no such test" scripts/tests/run.sh --only "$only/test-d.sh"
expect_fail "--only and nothing named" "names no test" scripts/tests/run.sh --only

finished
