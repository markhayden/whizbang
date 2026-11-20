#!/usr/bin/env bash
# Fails when the tests and the shipped binary would build serde_json with different features,
# and, over the workspace, when the shipped binary would be built with the tests' feature
# `fakes` on `whizbang`, `whizbang-backends` or `whizbang-core`.
# Cargo turns on, for every crate of one build, each feature any crate of that build asks
# for: so a feature one crate turns on reaches every crate of `just check`, clippy and CI's
# tests, which build the whole workspace, and none of the binary `just smoke` builds
# (`-p whizbang`) unless the binary depends on that crate. Then the tests would not test
# the serde_json that ships (`arbitrary_precision` did this: docs/adr/0023-one-serde-json.md).
#
#   scripts/check-features.sh                                  the workspace, from `cargo tree`
#   scripts/check-features.sh --shipped FILE --workspace FILE  two saved answers of `cargo tree`
#
# Each answer is one line, `serde_json v<version> <features, by commas>`, as
#   cargo tree -p whizbang -e normal,build -i serde_json -f '{p} {f}' --prefix none --depth 0
#   cargo tree --workspace -e normal,build,dev -i serde_json -f '{p} {f}' --prefix none --depth 0
# give it. The options exist for scripts/tests/test-features.sh.
# Parse the whole file first: see the top of scripts/scan.sh.
"$BASH" -n "${BASH_SOURCE[0]}" || { echo "features: ${BASH_SOURCE[0]} has a syntax error; refusing to run" >&2; exit 2; }
# The whole script is one function, read before any of it runs, so a change to this file
# during a run does not change the run: see `main` in scripts/scan.sh.
main() {
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

shipped=""
workspace=""
while [ $# -gt 0 ]; do
  case "$1" in
    --shipped) shipped=$2; shift 2 ;;
    --workspace) workspace=$2; shift 2 ;;
    *) echo "features: unknown argument: $1" >&2; exit 2 ;;
  esac
done
if { [ -n "$shipped" ] && [ -z "$workspace" ]; } || { [ -z "$shipped" ] && [ -n "$workspace" ]; }; then
  echo "features: --shipped and --workspace go together" >&2
  exit 2
fi

tmp=$(mktemp -d)
# An exit of 0 that is not one meant to succeed becomes 70: see the top of scripts/scan.sh.
done_ok=0
trap 'rc=$?; rm -rf "$tmp"; [ "$done_ok" = 1 ] || [ "$rc" -ne 0 ] || { echo "features: stopped before its end; failing" >&2; rc=70; }; exit "$rc"' EXIT

computed=0
if [ -z "$shipped" ]; then
  computed=1
  shipped="$tmp/shipped"
  workspace="$tmp/workspace"
  # The binary as `just smoke` builds it: its own dependencies, no development ones.
  cargo tree --locked -p whizbang -e normal,build -i serde_json \
    -f '{p} {f}' --prefix none --depth 0 >"$shipped"
  # Every crate and every test, as `just check` and CI build them.
  cargo tree --locked --workspace -e normal,build,dev -i serde_json \
    -f '{p} {f}' --prefix none --depth 0 >"$workspace"
fi

# One line each, naming serde_json: anything else (none, two versions) is not understood.
for answer in "$shipped" "$workspace"; do
  if [ "$(grep -c '' "$answer")" != 1 ] || ! grep -E '^serde_json v[^ ]+ [a-z0-9_,-]*$' "$answer" >/dev/null; then
    echo "features: cannot read the answer of cargo tree for serde_json:" >&2
    sed 's/^/  /' "$answer" >&2
    exit 1
  fi
done

read -r _ version_shipped features_shipped <"$shipped" || true
read -r _ version_workspace features_workspace <"$workspace" || true
if [ "$version_shipped" != "$version_workspace" ]; then
  echo "features: the binary builds serde_json $version_shipped and the workspace $version_workspace" >&2
  exit 1
fi
if [ "$features_shipped" != "$features_workspace" ]; then
  echo "features: serde_json's features differ between the shipped binary and the tests" >&2
  echo "  the binary (-p whizbang): ${features_shipped:-none}" >&2
  echo "  the workspace and its tests: ${features_workspace:-none}" >&2
  echo "  a crate turns on a feature the binary does not build with: see docs/adr/0023-one-serde-json.md" >&2
  exit 1
fi
echo "features: ok (serde_json $version_shipped with ${features_shipped:-no features}, in the binary and in the tests)"

# The feature `fakes` (`OneWorker::starting`, `Launch::testing`; in the core, an executor
# built with a digest alone, `Engine::over`) is the tests' alone: the binary as `just smoke`
# builds it has it on none of its three crates (plan 1C, the review of task P6b; plan m4,
# task K1, item 5). Checked over the workspace only, not over saved answers.
if [ "$computed" = 1 ]; then
  cargo tree --locked -p whizbang -e normal,build -f '{p} {f}' --prefix none >"$tmp/crates"
  if grep -E '^whizbang(-backends|-core)? v[^ ]+ .*[ ,]fakes(,|$| )' "$tmp/crates" >/dev/null; then
    echo "features: the binary (-p whizbang) is built with the feature fakes, which only the tests may turn on:" >&2
    grep -E '^whizbang(-backends|-core)? ' "$tmp/crates" | sort -u | sed 's/^/  /' >&2
    exit 1
  fi
  echo "features: ok (the binary is built without the feature fakes)"
  # The baseline replay's validator and the crates it brings are the feature
  # `baseline-replay`'s alone: the owner's decision of 2026-11-19 (ADR 0034).
  if grep -E '^jsonschema v' "$tmp/crates" >/dev/null; then
    echo "features: the binary (-p whizbang) links jsonschema, which only the feature baseline-replay may bring (ADR 0034)" >&2
    exit 1
  fi
  echo "features: ok (the binary links no jsonschema)"
fi
done_ok=1
exit 0
}
main "$@"; exit
