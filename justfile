# Whizbang developer commands. `just setup` once on a fresh clone, `just check` before a
# commit, `just check` and `just scan` before a push.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Install the git hooks, then check the pinned tools (scripts/tool-versions.txt).
setup:
    scripts/setup.sh

# The fast tier: format check, lint, the repo checks, the script tests, the generated
# reference against a fresh copy, then the fast tests with their timing table.
check: fmt-check clippy graph deny test-scripts docs-check timing

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# The fast tests alone, without the timing table or its limit.
test:
    cargo nextest run --workspace

# The fast tests, then seconds per suite; fails when test time exceeds 180 seconds.
timing:
    scripts/timing.sh

# The crate edges of SPEC.md section 3 (scripts/crate-graph.txt).
graph:
    scripts/check-crate-graph.sh

# Licences, bans and sources. These work offline; advisories need the network.
deny:
    cargo deny check licenses bans sources

deny-advisories:
    cargo deny check advisories

# The tests of the scripts themselves (scripts/tests/).
test-scripts:
    scripts/tests/run.sh

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

# What a pull request runs.
ci: check deny-advisories smoke scan-secrets
