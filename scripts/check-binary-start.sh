#!/usr/bin/env bash
# Fails when a test starts the real binary any other way than through the test kit's one
# function, `whizbang_testkit::binary::command`, which gives it a cleared environment and
# a temporary home (crates/whizbang-testkit/src/binary.rs; docs/plans/m1c-local-pack.md,
# D12, "A test cannot reach the owner's store").
#
#   scripts/check-binary-start.sh          this repo
#   scripts/check-binary-start.sh DIR      another folder that holds a `crates` folder
#                                          (scripts/tests/test-binary-start.sh)
#
# It reads every `.rs` file under `crates/`, outside any folder named `target`. Lines that
# are comments alone (they start with `//`) are left out, and then all white space, so a
# line break hides nothing. What it refuses:
#
# 1. The name `CARGO_BIN_EXE` anywhere but as
#    `whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))`, the function's
#    name written whole. So the path is never a constant, never a helper's result, never
#    handed to `Command::new` or to a script, never read with `option_env!` or at run
#    time, in any crate.
# 2. `env!` or `option_env!` of anything but one text that starts `CARGO_`, in any crate:
#    the name of the binary's variable cannot be put together from pieces, and no test is
#    built with a variable of whoever built it.
# 3. A module, or another name brought in, called `whizbang_testkit`: the function's name
#    would then mean another function.
# 4. In a crate that starts the binary, in every `.rs` file outside its `src/`: reading
#    the environment of the test itself (`env::var`, `env::vars`, their `_os` forms,
#    anything else brought in from `env::`, `home_dir`), which is how a real home or the
#    whole environment would be handed on; and the texts "HOME", "TMPDIR", "USERPROFILE"
#    and any that starts "XDG_", which is how one of the function's places would be set
#    to another. A test asks the command for its home (`home()`). And `.env_clear(` or
#    `.env_remove(`, which would take one of the function's places away: the function
#    sets the environment, and a test adds variables of its own to it and takes none.
#    A test that searches source for such texts keeps them in a data file, which this
#    check does not read (`crates/whizbang/tests/it/environment-reads.txt`).
#
# What it does not catch. It reads text, and stops a mistake, not a person set on getting
# past it: a path to the binary worked out with no variable (from the test binary's own
# place, say); the binary started by a program the test starts (the function's own
# `under_shell` is the way for a script); a variable's name that is not written as one
# text ("HO" and "ME"), or given by another crate's helper; `env` brought in under
# another name (`use std::env as e`); texts kept in a file that is not `.rs`; a
# dependency renamed to `whizbang_testkit` in a manifest; and a comment that ends a line of code is read as code, so one that names
# the variable is refused. The test kit's own test shows what the function gives.
#
# It sets no `EXIT` trap and writes no file, so its own syntax error is a failure.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C

if [ $# -gt 1 ]; then
  echo "binary start: usage: check-binary-start.sh [DIR]" >&2
  exit 2
fi
dir=${1:-}
if [ -z "$dir" ]; then
  cd "$(git rev-parse --show-toplevel)"
  dir=.
fi
case "$dir" in -*) dir="./$dir" ;; esac
[ -d "$dir/crates" ] || { echo "binary start: no folder $dir/crates" >&2; exit 2; }

# code <file>: the file without the lines that are comments alone, and without white
# space. A trailing comma before a closing bracket, which the formatter writes when it
# breaks a call over lines, is taken out.
code() {
  sed -e '/^[[:space:]]*\/\//d' "$1" | tr -d '[:space:]' | sed -e 's/,)/)/g'
}

# count <fixed text>: how often it is in what is read on the standard input.
count() {
  { grep -o -F -e "$1" || true; } | wc -l | tr -d ' '
}
# count_re <pattern>: the same for an extended pattern.
count_re() {
  { grep -o -E -e "$1" || true; } | wc -l | tr -d ' '
}

bad=0
refuse() { # <file> <what>
  echo "binary start: $1: $2" >&2
  bad=1
}

# The one call, as it reads without white space. Before it comes nothing that could make
# the name a longer one.
one_call='(^|[^A-Za-z0-9_:])whizbang_testkit::binary::command\(env!\("CARGO_BIN_EXE_whizbang"\)\)'
function_name='whizbang_testkit::binary::command'

started=0
files=0
starters=""
while IFS= read -r file; do
  files=$((files + 1))
  text=$(code "$file")

  named=$(printf '%s\n' "$text" | count 'CARGO_BIN_EXE')
  through=$(printf '%s\n' "$text" | count_re "$one_call")
  if [ "$named" -ne "$through" ]; then
    refuse "$file" "it names CARGO_BIN_EXE $named times, and $through of them as the argument of $function_name, written whole; the real binary is started only as $function_name(env!(\"CARGO_BIN_EXE_whizbang\"))"
  fi
  started=$((started + through))
  if [ "$named" -gt 0 ]; then
    crate=${file#"$dir"/crates/}
    crate=${crate%%/*}
    case " $starters " in
      *" $crate "*) ;;
      *) starters="$starters $crate" ;;
    esac
  fi

  macros=$(printf '%s\n' "$text" | count 'env!(')
  cargos=$(printf '%s\n' "$text" | count_re '(^|[^A-Za-z0-9_])(option_)?env!\("CARGO_[A-Za-z0-9_]*"\)')
  if [ "$macros" -ne "$cargos" ]; then
    refuse "$file" "it has env! or option_env! of something other than one text that starts CARGO_ ($macros in all, $cargos of that form); a test is built with no variable of whoever builds it"
  fi

  if sed -e '/^[[:space:]]*\/\//d' "$file" | tr -s '[:space:]' ' ' |
    grep -E '(^|[^A-Za-z0-9_])(mod|as) whizbang_testkit([^A-Za-z0-9_]|$)' >/dev/null; then
    refuse "$file" "it gives the name whizbang_testkit to something of its own, so $function_name would not be the test kit's function"
  fi
done < <(find "$dir/crates" -type f -name '*.rs' ! -path '*/target/*' | sort)

# A crate that starts the binary: none of its files outside src/ reads the environment
# of the test, or names a place the function sets.
for crate in $starters; do
  while IFS= read -r file; do
    text=$(code "$file")
    for reading in 'env::var' 'env::{' 'env::*' 'vars_os' 'var_os' 'home_dir'; do
      if [ "$(printf '%s\n' "$text" | count "$reading")" -gt 0 ]; then
        refuse "$file" "it has \`$reading\` in a crate whose tests start the real binary: such a test reads nothing of the environment it runs in, which it could hand on to the binary"
      fi
    done
    for place in '"HOME"' '"TMPDIR"' '"USERPROFILE"' '"XDG_'; do
      if [ "$(printf '%s\n' "$text" | count "$place")" -gt 0 ]; then
        refuse "$file" "it has the text $place in a crate whose tests start the real binary: the home and the folders are the function's, and a test asks the command for its home (\`home()\`)"
      fi
    done
    for clearing in '.env_clear(' '.env_remove('; do
      if [ "$(printf '%s\n' "$text" | count "$clearing")" -gt 0 ]; then
        refuse "$file" "it has \`$clearing\` in a crate whose tests start the real binary: the function sets the environment, and a test adds to it and takes nothing from it"
      fi
    done
  done < <(find "$dir/crates/$crate" -type f -name '*.rs' ! -path '*/target/*' ! -path "$dir/crates/$crate/src/*" | sort)
done

if [ "$bad" -ne 0 ]; then
  exit 1
fi
echo "binary start: ok ($files files; the real binary is started in $started places, each through $function_name)"
exit 0
}
main "$@"; exit
