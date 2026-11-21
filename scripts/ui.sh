#!/usr/bin/env bash
# The playground's interface, `ui/` (plan m5, task U1; ADR 0042).
#
#   scripts/ui.sh install the packages and nothing else (scripts/e2e.sh)
#   scripts/ui.sh build   write ui/dist/ from the sources (`just ui-build`); commit the result
#   scripts/ui.sh check   the type check, the unit tests, ui/dist/ against a fresh build, and
#                         the host's list of the files it embeds against ui/dist/
#                         (`just ui-check`, part of `just check`)
#
# Both use the Node that scripts/tool-versions.txt pins (its major version is required
# here; `just setup` checks the whole version) and the packages that ui/package-lock.json
# pins, installed by `npm ci` from npm's registry when ui/node_modules/ is missing or older
# than the lock file. No package runs an install script, and Playwright downloads no
# browser (the Chrome tier uses the Chrome installed on the machine).
#
# `check` writes the unit tests' seconds, the whole run of them as the clock saw it, to
# target/ui/unit-seconds, and their results to target/ui/unit-junit.xml, where
# scripts/timing.sh, which `just check` runs next, counts them with the Rust tests' under
# the one limit of 300 seconds (SPEC.md section 13). It fails when:
# - the type check or a unit test fails;
# - ui/dist/ is not, byte for byte, what the sources build (run `just ui-build` and commit);
# - a file of ui/dist/ is not tracked by git, or one git tracks is not there;
# - crates/whizbang-serve/src/play/assets.rs does not list exactly the files of ui/dist/.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "ui: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
main() {
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

die() {
  echo "ui: $*" >&2
  exit 1
}
[ $# -eq 1 ] || { echo "usage: scripts/ui.sh install | build | check" >&2; exit 2; }
mode=$1
case "$mode" in install | build | check) ;; *) echo "usage: scripts/ui.sh install | build | check" >&2; exit 2 ;; esac

assets=crates/whizbang-serve/src/play/assets.rs
pin=$(awk '$1 == "node" { print $2 }' scripts/tool-versions.txt)
[ -n "$pin" ] || die "scripts/tool-versions.txt pins no Node"
command -v node >/dev/null ||
  die "Node is missing; Node $pin is needed (scripts/tool-versions.txt): install it from nodejs.org, or with a version manager (fnm install $pin)"
have=$(node --version | sed 's/^v//')
[ "${have%%.*}" = "${pin%%.*}" ] ||
  die "Node is at another version: found $have, pinned $pin; the major version must match (fnm install $pin, then use it)"

# The packages, as the lock file pins them, when they are missing or older than it.
if [ ! -f ui/node_modules/.package-lock.json ] || [ ui/package-lock.json -nt ui/node_modules/.package-lock.json ]; then
  echo "ui: installing the packages ui/package-lock.json pins (npm ci)"
  (cd ui && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci --ignore-scripts --no-audit --no-fund --loglevel=error) ||
    die "npm ci failed"
fi

[ "$mode" != install ] || exit 0
if [ "$mode" = build ]; then
  (cd ui && node_modules/.bin/vite build --logLevel warn) || die "the build failed"
  echo "ui: wrote ui/dist/ ($(find ui/dist -type f | wc -l | tr -d ' ') files); commit it with its sources"
  exit 0
fi

(cd ui && node_modules/.bin/tsc --noEmit) || die "the type check failed"
mkdir -p target/ui
rm -f target/ui/unit-junit.xml target/ui/unit-seconds
now() { node -e 'process.stdout.write(String(Date.now()))'; }
started=$(now)
(cd ui && node_modules/.bin/vitest run --reporter=default --reporter=junit --outputFile.junit=../target/ui/unit-junit.xml) ||
  die "a unit test failed"
awk -v a="$started" -v b="$(now)" 'BEGIN { printf "%.3f\n", (b - a) / 1000 }' >target/ui/unit-seconds

tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "ui: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT
(cd ui && node_modules/.bin/vite build --logLevel warn --outDir "$tmp/dist" --emptyOutDir) || die "the build failed"
[ -d ui/dist ] || die "ui/dist/ is missing: run just ui-build and commit it"
diff -r "$tmp/dist" ui/dist >"$tmp/diff" 2>&1 ||
  { sed 's/^/    /' "$tmp/diff" | head -n 20 >&2 || true; die "ui/dist/ is not what the sources build: run just ui-build and commit ui/dist/ with its sources"; }

(cd ui/dist && find . -type f | sed 's|^\./||' | sort) >"$tmp/built"
git ls-files ui/dist | sed 's|^ui/dist/||' | sort >"$tmp/tracked"
cmp -s "$tmp/built" "$tmp/tracked" ||
  { diff "$tmp/tracked" "$tmp/built" | sed 's/^/    /' >&2 || true; die "ui/dist/ and what git tracks of it differ: commit ui/dist/ as the build writes it (< tracked, > built)"; }
sed -n 's/^    asset!("\([^"]*\)", .*$/\1/p' "$assets" | sort >"$tmp/listed"
[ -s "$tmp/listed" ] || die "$assets lists no file"
cmp -s "$tmp/built" "$tmp/listed" ||
  { diff "$tmp/listed" "$tmp/built" | sed 's/^/    /' >&2 || true; die "$assets must list exactly the files of ui/dist/ (< listed, > built)"; }
echo "ui: the type check and the unit tests pass; ui/dist/ is what the sources build, tracked, and listed in $assets"
done_ok=1
exit 0
}
main "$@"; exit
