#!/usr/bin/env bash
# scripts/test-models.sh and scripts/model-evidence.sh (the model tier, plan 1C, task R2b)
# refuse before they build or start anything: the recipe unless WHIZBANG_MODELS_DIR names an
# absolute models directory with a model and the pack in it, the evidence script unless it
# runs under the thread guard with its limit at 200. Nothing here loads a model or runs cargo:
# each case is refused at its first lines, and the folder of the third holds nothing cargo
# could be asked to build.
source "$(dirname "$0")/lib.sh"

tm=scripts/test-models.sh
ev=scripts/model-evidence.sh

expect_fail "no models directory named" "no models directory is named" env -u WHIZBANG_MODELS_DIR "$tm"
expect_fail "a part the tier has not" "names a part of the tier" env -u WHIZBANG_MODELS_DIR "$tm" everything
expect_fail "two parts" "names a part of the tier" env -u WHIZBANG_MODELS_DIR "$tm" parity coverage
expect_fail "an empty name" "no models directory is named" env WHIZBANG_MODELS_DIR= "$tm"
expect_fail "a relative path" "must be an absolute path" env WHIZBANG_MODELS_DIR=models "$tm"
mkdir -p "$tmp/models-only/models"
expect_fail "a folder without the pack" "no models directory with a model and the pack" \
  env WHIZBANG_MODELS_DIR="$tmp/models-only" "$tm"
expect_fail "a folder that is not there" "no models directory with a model and the pack" \
  env WHIZBANG_MODELS_DIR="$tmp/nowhere" "$tm"
[ ! -e "$tmp/models-only/pack" ] || fail "nothing is made in the folder it refused"

expect_fail "the evidence outside the guard" "run it through scripts/heavy.sh" \
  env -u WHIZBANG_THREAD_GUARD "$ev"
expect_fail "the evidence with another limit" "must be 200" \
  env WHIZBANG_THREAD_GUARD="1 2 3" WHIZBANG_THREAD_LIMIT=300 "$ev"

finished
