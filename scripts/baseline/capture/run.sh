#!/usr/bin/env bash
# Makes the synthetic replay fixtures again (plan 2C, task E2, item 3), from frozen squint.
#
#   scripts/baseline/capture/run.sh <squint checkout> <scratch folder>
#
# <squint checkout> is a clone of the old squint repository; it is read and never changed.
# <scratch folder> is an empty folder outside this repository. The script clones squint into
# it at the tag `final-before-whizbang`, adds this folder's program to that clone's workspace,
# builds it against squint's own lock file (offline: the crates must already be in cargo's
# cache) and runs it, which writes a synthetic package; then it writes the package's listing
# and digest as docs/baseline/README.md says, converts the package with
# scripts/baseline/convert.py, and copies the set, the decisions' package files the replay
# reads and the transcripts into fixtures/baseline/synthetic/. Nothing is pushed, nothing in
# the old repository changes, and no model is called. It builds, so it is heavy: run it
# through scripts/heavy.sh.
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
[ $# -eq 2 ] || { echo "usage: run.sh <squint checkout> <scratch folder>" >&2; exit 2; }
squint=$1
scratch=$2
[ -d "$scratch" ] && [ -z "$(ls -A "$scratch")" ] || { echo "capture: $scratch is not an empty folder" >&2; exit 2; }
case "$(cd "$scratch" && pwd)/" in "$root"/*) echo "capture: the scratch folder is inside this repository" >&2; exit 2 ;; esac

git clone --quiet "$squint" "$scratch/squint"
git -C "$scratch/squint" checkout --quiet final-before-whizbang
mkdir -p "$scratch/squint/crates/baseline-capture"
cp -R "$here/Cargo.toml" "$here/src" "$scratch/squint/crates/baseline-capture/"
# The program joins the clone's workspace, so it builds against squint's lock file.
awk '{ print } $0 == "  \"crates/squint-cli\"," { print "  \"crates/baseline-capture\"," }' \
  "$scratch/squint/Cargo.toml" >"$scratch/Cargo.toml.joined"
mv "$scratch/Cargo.toml.joined" "$scratch/squint/Cargo.toml"
grep -q -F '"crates/baseline-capture"' "$scratch/squint/Cargo.toml" || { echo "capture: the workspace was not joined" >&2; exit 1; }
(cd "$scratch/squint" && cargo build --offline --quiet -p baseline-capture)
package="$scratch/package"
mkdir "$package"
"$scratch/squint/target/debug/baseline-capture" "$package"

# The package's listing and digest (docs/baseline/README.md, "The package digest").
ROOT="$root" python3 -B -c '
import os, sys
sys.path.insert(0, os.path.join(os.environ["ROOT"], "scripts", "baseline"))
import common
package = sys.argv[1]
data, _ = common.listing(package, leave_out=("PACKAGE.sha256", "PACKAGE.digest"))
open(os.path.join(package, "PACKAGE.sha256"), "wb").write(data)
open(os.path.join(package, "PACKAGE.digest"), "w").write(common.digest_of_listing(data)[len("sha256:"):] + "\n")
' "$package"
python3 -B "$root/scripts/baseline/convert.py" --package "$package" --out "$scratch/transcripts" --fixtures replay

# What is committed: the transcripts, and of the package what `whizbang eval baseline` reads:
# the listing and digest, the preparation, the set, and each decision's header and old
# result. The rest (the journals, the stored exchanges, the frames, and the run records,
# which name this scratch folder) stays here; the listing still names it.
out="$root/fixtures/baseline/synthetic"
rm -rf "$out"
mkdir -p "$out/package"
(cd "$package" && find PACKAGE.sha256 PACKAGE.digest assembly evalsets -type f \
  && find replay -type f \( -name result.json -o -name decision.json \)) | sort | while IFS= read -r file; do
  mkdir -p "$out/package/$(dirname "$file")"
  cp "$package/$file" "$out/package/$file"
done
cp -R "$scratch/transcripts" "$out/transcripts"
if grep -r -l -F -e "$scratch" -e "$HOME" "$out" >/dev/null; then
  echo "capture: a committed file names a local folder" >&2
  exit 1
fi
echo "capture: written to fixtures/baseline/synthetic"
}
main "$@"; exit
