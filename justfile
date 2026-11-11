# Whizbang developer commands. `just setup` once on a fresh clone, `just check` before a
# commit, `just scan` before a push.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Install the git hooks and say what was installed.
setup:
    git config core.hooksPath scripts/hooks
    @echo "hooks installed from scripts/hooks: pre-commit, commit-msg, pre-push"
    @echo "set WHIZBANG_WORDLIST to the clean-room word list before your first push"

# The fast tier: format check, lint, fast tests.
check: fmt-check clippy test

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo nextest run --workspace

# The secrets scan and the clean-room scan, over the tree.
scan:
    scripts/scan.sh

# The secrets scan only: what CI can run without the private word list.
scan-secrets:
    scripts/scan.sh --secrets-only

# What a pull request runs.
ci: check scan-secrets
