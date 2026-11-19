# Whizbang developer commands. `just setup` once on a fresh clone, `just check` before a
# commit, `just check` and `just scan` before a push.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Install the git hooks, then check the pinned tools (scripts/tool-versions.txt).
setup:
    scripts/setup.sh

# The fast tier on this machine: format check, lint, the repo checks, the script tests a
# change of this branch can reach (test-scripts-local), the generated reference against a
# fresh copy, the fast tests with their timing table, then the pack's Python tests.
check: fmt-check clippy graph deny test-scripts-local docs-check timing test-pack

# The fast tier with every script test, whatever changed: what `ci` runs, so CI runs them
# all on every push. scripts/tests/test-ci-jobs.sh holds `check` to it, the script tests
# aside.
check-full: fmt-check clippy graph deny test-scripts docs-check timing test-pack

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# The fast tests alone, without the timing table or its limit. They run through
# scripts/thread-guard.sh, which kills a test process that holds too many threads.
test:
    scripts/thread-guard.sh cargo nextest run --workspace

# Some of the tests, through the same guard, as in `just test-only -p whizbang-core watch::`.
# This, and not a bare `cargo nextest run`, is how one crate's tests or one test are run.
# It takes a closed set of arguments (the top of scripts/test-only.sh) and refuses any
# other: it is on the allow list, and nextest's own arguments can name a program to run.
[positional-arguments]
test-only +args:
    scripts/test-only.sh "$@"

# The fast tests, then seconds per suite; fails when test time exceeds 180 seconds (300 on
# GitHub's hosted macOS runner: see scripts/timing.sh). They run through
# scripts/thread-guard.sh too.
timing:
    scripts/timing.sh

# The pack's Python tests, with the Python the pack pins (pack/.python-version), offline,
# through the guard; the runner ends itself at 60 seconds (scripts/test-pack.sh).
test-pack:
    scripts/thread-guard.sh scripts/test-pack.sh

# The model tier: the tests that load a real model (SAM 3, on the Mac's GPU), never in CI.
# Heavy, under the thread guard at 200 and the watch on resident memory at 24 GiB; it refuses
# unless WHIZBANG_MODELS_DIR names the models directory (scripts/test-models.sh).
test-models:
    WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh test-models scripts/test-models.sh

# The crate edges of SPEC.md section 3 (scripts/crate-graph.txt), and the features of
# serde_json, the same for the binary and for the tests (ADR 0023).
graph:
    scripts/check-crate-graph.sh
    scripts/check-features.sh

# Licences, bans and sources. These work offline; advisories need the network.
deny:
    cargo deny check licenses bans sources

deny-advisories:
    cargo deny check advisories

# The tests of the scripts themselves (scripts/tests/), every one of them.
test-scripts:
    scripts/tests/run.sh

# The script tests `check` runs on this machine: all of them when this branch (its commits
# since the merge-base with origin/main, and the working tree) changes a path one of them
# reads, such as scripts/, the justfile, .github/, AGENTS.md, pack/ or docs/; only the ones
# that read the crates' sources or every tracked file when the change is elsewhere; none
# when nothing changed. It prints one line saying which and why. WHIZBANG_SCRIPT_TESTS=1
# runs them all. CI does not use it (scripts/script-tests-local.sh).
test-scripts-local:
    scripts/script-tests-local.sh

# Regenerate the reference (docs/reference/) from `whizbang describe --json --pretty`.
docs:
    scripts/docs.sh

# Fails when the committed reference differs from a fresh generation.
docs-check:
    scripts/docs.sh --check

# The real timeout: a fixture test that sleeps 31 seconds fails by timeout at 30. About 35
# seconds, so it is not in `check`; CI runs it on one runner.
test-timeout:
    WHIZBANG_TIMEOUT_TEST=1 bash scripts/tests/test-timeout.sh

# The script tests with every opt-in part, about 40 seconds: the real timeout, and the real
# fast tests failing a zero time limit (which runs the fast tier a second time).
test-scripts-slow:
    WHIZBANG_SLOW_TESTS=1 scripts/tests/run.sh

# A smoke of the release binary: build it, run `describe` in each form, and require success.
smoke:
    cargo build --release -p whizbang
    target/release/whizbang describe
    target/release/whizbang describe --json > /dev/null
    target/release/whizbang describe --json --pretty > /dev/null

# The example on `Ledger` that must not compile (ADR 0016): it fails with the one error it
# is there for and with no other, and compiles without the line that causes it
# (scripts/compile-fail.sh). It builds the core and runs no test.
compile-fail:
    scripts/compile-fail.sh

# The secrets scan and the clean-room scan, over the working tree. A push runs them over
# what it would publish (scripts/hooks/pre-push).
scan:
    scripts/scan.sh

# The search the scan relies on, repeated for 30 seconds against its known output. CI runs
# it on macOS, where the search once lost matches (scripts/tests/scan/canary.sh).
search-canary:
    WHIZBANG_CANARY_SECONDS=30 bash scripts/tests/scan/canary.sh

# The secrets scan only: what CI can run without the private word list.
scan-secrets:
    scripts/scan.sh --secrets-only

# The secrets scan, the attribution check and the clean-room scan over every commit in
# base..head (contents, paths and messages) and the tree of head.
# The arguments reach the scan as "$1" and "$2", never as text a shell reads.
[positional-arguments]
scan-range base head:
    scripts/scan.sh --range "$1" "$2"

# The same without the clean-room scan: what CI runs over a pull request's commits (CI
# uses the base branch's copy of the scan, so a pull request cannot change what checks it).
[positional-arguments]
scan-range-secrets base head:
    scripts/scan.sh --secrets-only --range "$1" "$2"

# What a pull request runs: `check` with every script test (check-full), and more.
ci: check-full deny-advisories smoke compile-fail scan-secrets

# `ci` in three parts, for CI, where the script tests alone take a runner three to five
# minutes and the rest took the macOS runner past its limit: it runs each part in a job of
# its own, side by side, on each runner (.github/workflows/ci.yml). The first is `ci`
# without the script tests and without the release build and what follows it; the second
# is those (the smoke of the release binary, the example that must not compile, the
# secrets scan); the third is the script tests, after `deny` as in `check`: one of them
# reads the workspace's crates from cargo's home, where `deny` puts them.
# scripts/tests/test-ci-jobs.sh holds the three to `check-full` and `ci`.
ci-without-scripts: fmt-check clippy graph deny docs-check timing test-pack deny-advisories

ci-release: smoke compile-fail scan-secrets

ci-scripts: deny test-scripts
