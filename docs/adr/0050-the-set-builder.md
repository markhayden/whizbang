# 0050. The set builder

Status: accepted. Amended 2026-11-25 by B1's second pull request: decisions 5 to 7.
Date: 2026-11-25
Spec: section 8, "Evaluation data" (fresh sets, new set versions, sealed sets never edited);
section 10, "Egress"; section 15 (approved recipients, the spend allowance); plan mE, task B1

## Context

Milestone E needs new versions of two sets and a fresh confirmation set built by
construction (plan mE, gap 9). Whizbang reads, checks and fetches sets
(`crates/whizbang-eval/src/sets.rs`, `whizbang eval sets check|fetch`) but builds none, and
writes no seal. Squint built its sets with a Python toolkit (`scripts/sets/`, `squint-sets
build|rebuild|promote`) and sealed them with its own binary (`squint sets seal`), at the
commit `docs/sources.toml` pins. That toolkit called each provider's address directly with
`urllib`, fetched whatever address fal answered with, followed redirects with their headers,
and used any proxy the environment named.

## Decision

1. **A Python program in `tools/sets/`, ported.** Plans, the three kinds of family, the
   construction ops, the draft labels from the construction alone, `build`, `rebuild` and
   `promote` keep squint's behaviour. It is its own `uv` project, run as `python -m
   whizbang_sets` from its folder on the Python the pack pins; `tool.uv.package = false`, so
   nothing is built and no build backend is fetched. Its one package, Pillow, is pinned by
   hash in `tools/sets/uv.lock`, from `https://pypi.org/simple` and
   `https://files.pythonhosted.org/` only, the origins ADR 0021 already uses. The pictures
   are pure image work that Pillow does well (perspective, resampling, JPEG, the contact
   sheet); nothing of it belongs in the binary.
2. **`promote` seals.** The seal is made as `Set::components` checks it: the same six
   components, the jobs as the Rust reader serialises them, the rules from the files `check`
   ships (`crates/whizbang-squint/rules/`). The binary keeps writing no seal. A test makes
   again, exactly, the seal of every set already in `evalsets/`, and the dry run's sets must
   pass `whizbang eval sets check`, so the two cannot drift apart unseen.
3. **Checked egress.** A request goes only to an origin the whizbang config approves
   (`[egress.recipients]`) for every class it carries: a prompt is `prompts`; a picture
   given to a generator as its input, as it is, is `source_images`. A download goes only to
   an approved recipient's origin or an upstream of the recipient asked, with no key. A
   redirect is followed only where the same rule allows, each hop checked; a key is an
   unredirected header, so it never follows one. No proxy is used. fal is asked to answer
   with the picture itself (`sync_mode`), so a download is the exception. The origins the
   three generators use are `https://fal.run`, `https://api.openai.com` and
   `https://generativelanguage.googleapis.com`, and whatever host fal serves a picture from
   when it does not answer with it.
4. **Owner material stays out of repositories.** `--owner` (and the set's folder in it),
   `--labels-out`, and a blind set's `--plan` and `--draft` are refused when any folder from
   them up holds a `.git`. Every id a plan gives (the set's, `<use case>/<name>`, a
   family's, an item's) is lower-case letters, digits and hyphens, so none leads out of its
   folder; a blind set's item ids, which are public, are `<family>-<n>` and say nothing of
   how an item was made.
5. **A cumulative spend guard** (B1's second pull request). One ledger file in owner
   storage, its allowance set once; a dated bound price per picture for each generator in
   `tools/sets/prices.toml`, none written before the orchestrator reads it from the
   provider's page; every attempt charged at its bound and synced to the ledger before it is
   sent; a request past the allowance not sent.
6. **Output that tells nothing of a blind set** (the same pull request): counts and the
   set's id alone are printed, failures by their kind, and their text kept in owner
   storage. A provider's answer is never printed for any set.
7. **The pictures' repository by its current name.** New sets name
   `github.com/markhayden/whizbang-evalsets`; `SETS_REPOSITORY` names it and reads the old
   name, which sealed sets and the loop's lab manifest (bound by digest in its records)
   keep, as an alias of it. No sealed file changes.

## Consequences

- `scripts/tests/test-sets.sh` runs the program's tests and the dry run; it builds the
  binary, so it is one of the script tests a change to the crates runs, and it needs the
  pinned Python, which CI's `scripts` jobs now install as `check` does.
- `scripts/sets-publish.sh` is ported in the second pull request, beside the drawer of a
  blind plan (`draw`), which squint kept in owner storage as a script of its own.
- To reverse: remove `tools/sets/` and its test; no set and no crate changes.

## Alternatives

- **A `whizbang eval sets seal` command.** It would reuse `Set::components` directly, but
  `crates/whizbang/src/eval.rs` belongs to other tasks in this wave (plan mE, "Shared
  files"), and the seal test against every committed set holds the Python one to it.
- **The program without Pillow.** A perspective warp, Lanczos resampling and a JPEG encoder
  in plain Python is far more code than the port, and slow on a 1024-pixel picture.
- **Running it with the system's Python.** Each runner has another one, or none; the pack's
  pin is already installed where the tests run.
