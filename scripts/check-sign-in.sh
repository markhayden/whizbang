#!/usr/bin/env bash
# Fails when the one function that names the real place of the Hub's sign-in,
# `hub_sign_in_token_file` in the binary's `main.rs`, is named anywhere else (plan 1C, task
# N4, D12, "A test cannot reach the owner's store").
#
#   scripts/check-sign-in.sh          this repo
#   scripts/check-sign-in.sh DIR      another folder that holds a `crates` folder
#                                     (scripts/tests/test-sign-in.sh)
#
# The function reads the variables that say where the Hub's command-line tool keeps its
# token, from the environment of the process, and so gives the real place. Every other
# piece of the binary is handed what says the place, and every test hands it a temporary
# folder. So the function's name is in `crates/whizbang/src/main.rs` alone, and there
# exactly twice: its definition, `fn hub_sign_in_token_file()`, and the one place `main`
# hands it on uncalled, `std::sync::Arc::new(hub_sign_in_token_file)`, as the third
# argument of `models::run(&command, std::sync::Arc::new(platforms_models_dir), ...)` and
# nowhere else. A call of it,
# `hub_sign_in_token_file()` anywhere but its definition, is refused, in `main.rs` too.
#
# It reads every `.rs` file under `crates/`, outside any folder named `target`. Lines that
# are comments alone (they start with `//`) are left out, and then all white space, so a
# line break hides nothing.
#
# What it does not catch. It reads text: the function given another name by a `use ... as`
# in `main.rs`, or its body copied into another function, which is a review's to see; a
# file that is not `.rs`. The binary's own test
# (`wired::only_main_reads_where_the_platforms_data_folder_is`) holds that no other source
# of the binary reads the environment at all.
#
# It sets no `EXIT` trap and writes no file, so its own syntax error is a failure.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
export LC_ALL=C

if [ $# -gt 1 ]; then
  echo "sign-in: usage: check-sign-in.sh [DIR]" >&2
  exit 2
fi
dir=${1:-}
if [ -z "$dir" ]; then
  cd "$(git rev-parse --show-toplevel)"
  dir=.
fi
case "$dir" in -*) dir="./$dir" ;; esac
[ -d "$dir/crates" ] || { echo "sign-in: no folder $dir/crates" >&2; exit 2; }

name=hub_sign_in_token_file
main_rs="$dir/crates/whizbang/src/main.rs"

# code <file>: the file without the lines that are comments alone, and without white
# space.
code() {
  sed -e '/^[[:space:]]*\/\//d' "$1" | tr -d '[:space:]'
}

# count <fixed text>: how often it is in what is read on the standard input.
count() {
  { grep -o -F -e "$1" || true; } | wc -l | tr -d ' '
}

bad=0
refuse() { # <file> <what>
  echo "sign-in: $1: $2" >&2
  bad=1
}

files=0
while IFS= read -r file; do
  files=$((files + 1))
  [ "$file" = "$main_rs" ] && continue
  if [ "$(code "$file" | count "$name")" -gt 0 ]; then
    refuse "$file" "it names $name, the one function that names the real place of the Hub's sign-in; only main.rs of the binary names it, and hands it on uncalled"
  fi
done < <(find "$dir/crates" -type f -name '*.rs' ! -path '*/target/*' | sort)

if [ ! -f "$main_rs" ]; then
  refuse "$main_rs" "it is not there, and it is where $name is defined"
else
  text=$(code "$main_rs")
  named=$(printf '%s\n' "$text" | count "$name")
  defined=$(printf '%s\n' "$text" | count "fn$name()")
  # Handed on inside the call of the models commands, as their third argument, and
  # nowhere else: an `Arc::new` of it beside that call would be another holder of the
  # real place.
  handed=$(printf '%s\n' "$text" | count "models::run(&command,std::sync::Arc::new(platforms_models_dir),std::sync::Arc::new($name)")
  anywhere=$(printf '%s\n' "$text" | count "std::sync::Arc::new($name)")
  [ "$anywhere" -eq "$handed" ] || handed=0
  called=$(printf '%s\n' "$text" | count "$name()")
  if [ "$defined" -ne 1 ] || [ "$handed" -ne 1 ] || [ "$named" -ne 2 ] || [ "$called" -ne 1 ]; then
    refuse "$main_rs" "it names $name $named times ($defined definitions, $handed handed on, $called with brackets); it is defined once and handed on once, as std::sync::Arc::new($name) inside models::run(...), and never called there"
  fi
fi

if [ "$bad" -ne 0 ]; then
  exit 1
fi
echo "sign-in: ok ($files files; $name is named in main.rs alone, defined and handed on)"
exit 0
}
main "$@"; exit
