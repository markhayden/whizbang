# Whizbang: rules for any agent

Whizbang is one toolkit for working on images with models. `SPEC.md` says how it works;
`docs/plans/` holds the plan for the milestone in progress; `docs/carry-over.md` is the only
backlog; `docs/baseline/` describes the private baseline package and holds its public
manifest. This file holds what you cannot work out from the code.

## On a fresh clone

1. Run `just setup`. It installs the git hooks and checks the pinned tools
   (`scripts/tool-versions.txt`), naming the install command for any that is missing.
   Node is one of them: `just check` builds and tests the playground's interface (`ui/`)
   with it, and installs that interface's packages from npm's registry with `npm ci`.
   Google Chrome is needed for `just e2e`. A user of the binary needs neither.
2. Set `WHIZBANG_WORDLIST` to the path of a private word list the owner provides. Set it
   where it lasts: in your shell profile, or
   as an `env` entry in `.claude/settings.local.json`, which is never committed
   (git ignores it, and `just check` fails on a tracked one). A variable does not last from one of
   an agent's shell calls to the next, and a command that starts with `export`, the
   variable and `&&` is not one of the exact allowed commands, so it asks first every time.
   The path may be given to another agent working in this repo.
   It travels only in a brief to an agent or in the local environment.
   The path is never written into a hand-back, a pull request, a commit or a tracked file.
   The list itself is never committed, opened, quoted, printed or copied,
   and what it holds is never passed on. Without it you cannot push; ask the owner.
3. Run `just scan` and see it pass. Then run `just check`. Both pass before you change
   anything.

## What must pass, and who does it

| Step | Must pass first | Who |
|---|---|---|
| Commit | `just check` | Whoever commits |
| Push | `just check` passed on exactly the tree being pushed (it runs on the working tree, so `git status --short` prints nothing after the commit), then `just scan`: the secrets scan and the clean-room scan. The pre-push hook runs both scans again over every commit the push would publish. Never push with `--no-verify` | Whoever pushes |
| Merge | CI green on both runners, and `just scan-range <base> <head>` on the exact head that was reviewed | The orchestrator, and nobody else |
| Tag, release, publish | The same as a merge, on the commit being tagged | The orchestrator |

## Commands

| Command | When |
|---|---|
| `scripts/worktree.sh add <slug>` | At the start of a task: fetches `origin/main` and makes the worktree `../whizbang-<slug>` on a new branch `<slug>`. `remove <slug>` takes both away once the branch is merged into `origin/main`, and refuses otherwise; `list` shows the worktrees |
| `just check` | Before every commit: format check, lint, the crate graph, licences and sources, the syntax of every script under `scripts/`, the script tests this branch's change can reach, the generated reference against a fresh copy, the fast tests with seconds per suite, then the pack's Python tests. The script tests run all when the branch (its commits since the merge-base with `origin/main`, and the working tree) changes a path one of them reads: `scripts/`, the `justfile`, `.github/`, `.config/`, any `.claude/` or `.mcp.json`, `clippy.toml`, `deny.toml`, `AGENTS.md`, `CLAUDE.md`, `SPEC.md`, the workspace's or a crate's `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, a crate's `build.rs`, `.gitignore`, `pack/`, `docs/` or `ui/`. Any other change (a crate's sources) runs only the eight that read the crates' sources or every tracked file; no change runs none. One line says which ran and why; `WHIZBANG_SCRIPT_TESTS=1 just check` runs them all (`scripts/script-tests-local.sh`). CI runs every script test on every push (`just ci` runs `check-full`, which is `check` with all of them). The fast tests' limits: one test 30 seconds (`.config/nextest.toml`); all of them 300 seconds of test time, here and on every CI runner, the owner's decisions of 2026-11-17 (the hosted macOS runner) and 2026-11-19 (everywhere else, until then 180) (`scripts/timing.sh`, `SPEC.md` section 13) |
| `just test-pack` | The pack's Python tests (`pack/tests/`) alone: with the Python `pack/.python-version` pins, found by `uv` offline, through `scripts/thread-guard.sh`; the runner ends itself at 60 seconds (`scripts/test-pack.sh`). Where that Python is missing it fails and names the one command that installs it, from GitHub's releases only |
| `just docs` | After changing what `whizbang describe` reports (the description type, an action, the version): rewrites `docs/reference/`; commit the result, because `just check` fails on a stale copy |
| `scripts/tool-record.sh view` | After changing what `view` gives: writes the record's example images and measurements again (`docs/reference/tools/view.md`), and keeps its times unless one moved by more than a third or something else between the marks changed; commit the result, because the fast tests fail on stale images or measurements. A change in the times alone on a busy machine is discarded and the script run again. Heavy |
| `scripts/tool-record.sh grow` | After changing what `grow` gives: the same for its record (`docs/reference/tools/grow.md`), whose images and measurements the fast tests of `whizbang-rascal` hold. Heavy |
| `just ui-build` | After changing anything under `ui/` but `ui/e2e/`: writes the playground's built interface, `ui/dist/`, with the Node that `scripts/tool-versions.txt` pins and the packages `ui/package-lock.json` pins (`npm ci` installs them when missing); commit the result with its sources, because the binary embeds it and `just check` fails on a stale copy (`scripts/ui.sh`, ADR 0043) |
| `just ui-check` | Part of `just check`: the interface's type check and unit tests, `ui/dist/` against a fresh build, and the host's list of the files it embeds (`crates/whizbang-serve/src/play/assets.rs`) against `ui/dist/`. The unit tests' seconds count under the fast tier's one limit (`scripts/timing.sh`) |
| `just e2e` | After changing `ui/` or the playground's host: builds the fixture host (`crates/whizbang/examples/play_fixture.rs`) and runs the Chrome cases of `ui/e2e/` against it in the Google Chrome installed on the machine, one worker, no retry, held to 180 seconds of test time (`scripts/e2e.sh`). CI runs it in the jobs `e2e (<runner>)`. Heavy |
| `just scan` | Before every push: the secrets scan and the clean-room scan, over the working tree |
| `just scan-range <base> <head>` | Before a merge: the same two scans and the attribution check over every commit in `base..head` (contents, paths, messages) and the tree of `head` |
| `just scan-range-secrets <base> <head>` | The range scan without the clean-room scan: what CI runs over the commits of a pull request (with the base branch's copy of the scan), and of a push to `main` |
| `just ci` | What a pull request runs: `check-full` (`check` with every script test, whatever changed), advisories, a smoke of the release binary, the example that must not compile, the secrets scan, and the release archive made and installed (`just package`, `just install-check`) |
| `just ci-without-scripts`, `just ci-release`, `just ci-scripts` | Never by hand: three of the four parts of `just ci` as CI runs them, in jobs side by side on each runner, because the script tests alone take a runner three to six minutes and the release build took the macOS job past its limit. `check (macos-15)` and `check (ubuntu-22.04)` run the first, everything but the script tests and the release build; `release (macos-15)` and `release (ubuntu-22.04)` run the second, the smoke of the release binary, the example that must not compile, the secrets scan of the tree, and then the release archive of that runner made and installed (`just package`, `just install-check`); `scripts (macos-15)` and `scripts (ubuntu-22.04)` run the third, the script tests; `e2e (macos-15)` and `e2e (ubuntu-22.04)` run the fourth, `just e2e`, the Chrome tier. The job `secrets` runs the secrets scan and the attribution check over every commit, on the Linux runner, beside them. A pull request waits for those nine jobs and for the aggregate `ci`, which needs all nine and is the one check the protection of `main` requires. `scripts/tests/test-ci-jobs.sh` holds the parts to `just ci` |
| `just package` | Never needed before a commit; CI's `release` job runs it on each runner, and the `prerelease` workflow builds a candidate with it: the release archive of this machine's target, `whizbang-<version>-<target>.tar.gz`, and its line of `SHA256SUMS`, in `target/package`. The archive holds five files: the binary, `LICENSE`, `THIRD-PARTY-LICENSES.md`, `COPYRIGHT-library.html` and `README.md`, which is the install guide. The binary is built by `scripts/release-build.sh`, as `just smoke` builds it, with the builder's folders written under fixed names; the script refuses a checkout with tracked changes, a binary of another version or commit, a binary that holds a path of the machine that built it, and on Linux one that needs more than Ubuntu 22.04 has (`scripts/package.sh`). Heavy |
| `just install-check` | After `just package`: the bootstrap of what a user must have (the pinned `uv` and the pinned Python, which is downloaded from GitHub's releases) into a fresh folder, then the archive installed as the install guide says and shown to run, in an empty home with a cleared environment and no Rust toolchain on its path; it contacts nothing (`scripts/install-prereqs.sh`, `scripts/install-check.sh`). CI's `release` job runs it on each runner, and the `prerelease` workflow on a candidate's archive |
| `scripts/tag-version.sh <tag>` | To read a release tag: a candidate tag `v<version>-rc.<n>` or a version tag `v<version>`. The base version is what a binary says of itself; the candidate number is kept apart and is in no binary. The two release workflows, the local leg and the release gate all read a tag through it |
| `scripts/prerelease-local.sh [--no-attach] <candidate tag> <folder>` | The orchestrator only, on the owner's Mac, never in CI, once the `prerelease` workflow has made a candidate: the local leg. From a new clean checkout of the tag it runs the model tests, after `setup`; with the candidate's own downloaded archive, in a new store with the decision cache off, it runs `setup` with the extra, a mask on SAM 3, one fill, one check by a judge of each provider and the loop with its check required. `setup` comes first and replaces the pack in the models directory with the one the candidate carries, so the model tests and every later check load a model with the candidate's pack. It writes the receipt `prerelease-receipt/1` and attaches it to the candidate when every check passed; `--no-attach` as its first argument attaches nothing, and a candidate that holds a receipt already stops the leg before anything is spent. It needs `WHIZBANG_ENV_FILE` and `WHIZBANG_MODELS_DIR`, and spends well under one US dollar. Heavy |
| `scripts/release-gate.sh <version tag> <folder>` | Never by hand: the `release` workflow runs it on a version tag before anything is published. It refuses unless the tag's commit has a candidate that is the latest of its version, whose `prerelease` run succeeded, and whose receipt is read whole by its schema with every required check passed and every digest that of the candidate's bytes (`scripts/receipt-check.sh`, `scripts/prerelease-receipt.jq`). `scripts/release-publish.sh` then publishes the candidate's own files; nothing is built again |
| `just compile-fail` | After changing `Ledger`, `MadeName` or the example on `Ledger` that must not compile: the example is compiled against the core, must fail with the one error it is there for and no other, and must compile without the line that causes it (`scripts/compile-fail.sh`). Heavy |
| `just test-scripts-slow` | After changing `.config/nextest.toml` or `scripts/timing.sh`: about 40 seconds; a 31-second test really fails by timeout (CI runs that part as `just test-timeout`) |
| `just search-canary` | When a scan test fails once and passes on a rerun: 30 seconds of the scan's search against its known output (CI runs it on macOS) |
| `scripts/heavy.sh <label> <command...>` | Around any heavy command: one runs at a time on a machine, the next waits. The command runs under `scripts/thread-guard.sh` |
| `just test-only <which tests>` | One crate's tests or one test, as in `just test-only -p whizbang-core watch::`: the only way to run some of the tests. It runs them under `scripts/thread-guard.sh`, and takes `-p <crate>` or `--package <crate>`, `--lib`, `--test <name>`, `-E <filter>`, `--no-capture`, `--no-fail-fast` and filter words; anything else is refused before anything runs (`scripts/test-only.sh`). Heavy |
| `scripts/thread-guard.sh <command...>` | Around anything else that runs tests: it kills the command when one process holds more than 300 threads, or when it can no longer count them |
| `just test-models` | The model tier, on a machine with the model installed, never in CI: SAM 3 answering a phrase on the Mac's GPU through the binary's wiring (`crates/whizbang/tests/it/model_tier.rs`). It refuses unless `WHIZBANG_MODELS_DIR` names the models directory that holds `sam3` and the pack `whizbang setup --extra torch` installed; it runs under the heavy lock, the thread guard at 200 and `scripts/memory-watch.sh` (`scripts/test-models.sh`). `WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh model-evidence scripts/model-evidence.sh` runs it three times and writes `docs/results/m1c-local-pack.md` and its numbers; the fast tests fail on a record that is not of the tree's adapter, lock and fixture. Heavy |
| `scripts/memory-watch.sh <command...>` | Around anything that loads a real model, beside the thread guard: it kills the command when it and everything it started hold more than 24 GiB resident, and reports the peaks it saw and any process the command left running |

An action's name and purpose are printed in the reference as they are, so `just docs` refuses
any that could be read as markup. A name is a lower-case letter, then lower-case letters,
digits, `_` or `-`, 64 characters at most. A purpose is one line of at most 200 characters
that starts with a letter, does not end with a space, and holds only ASCII letters, digits,
spaces and `. , ; : ' " ( ) / % + -`, with no `://` and no `www.`. No two actions share a name.
The rule is at the top of `scripts/docs.sh`, and is held a second time in
`crates/whizbang-core/src/contract/text.rs`, where an action's descriptor is made, so an
action that breaks it cannot be registered; one table of cases
(`scripts/tests/fixtures/names-and-purposes.tsv`) is run against both. `docs/reference/` holds the two generated files
and `tools/` only; `just check` fails on any other file there that git does not ignore.

Every script test file (`scripts/tests/test-*.sh`, and each part under `scripts/tests/scan/`)
ends with the line `finished`. The bash that ships with macOS can exit 0 from a script that
stopped half-way, so a test passes only when that line ran. A script that sets an `EXIT` trap
copies the first lines and the trap of `scripts/scan.sh`; `scripts/tests/test-syntax.sh` holds
it to that.

These are heavy: `just check`, `just ci`, `just ci-without-scripts`, `just ci-release`, `just e2e`, `just test`, `just test-only`, `just test-models`, `just timing`,
`just clippy`, `just docs`, `just docs-check`, `just smoke`, `just package`, `just compile-fail`,
`just test-scripts-slow`, `scripts/prerelease-local.sh`, and
any `cargo` command that builds or tests. When another agent may be building on the machine,
each goes through the lock, as in `scripts/heavy.sh check just check`. The scans,
`just test-scripts` and one script test alone are not heavy.

Tests are never run by a bare `cargo test` or `cargo nextest run`: only by `just check`,
`just test`, `just test-only`, or a command through `scripts/heavy.sh` or
`scripts/thread-guard.sh`. The guard catches a process whose threads grow over about a third
of a second; it cannot stop a loop that makes them faster than that, which is what the rule
under "Gotchas" is for.

## Where agents may work

- **This repo,** in a git worktree: one task, one worktree, one branch. Do not edit files
  that another task's worktree is changing:
  `gh pr diff <n> --name-only` lists the files an open pull request changes.
- **The four old repos** (rascal, squint, wafflery, table-manners), read-only, at the commits
  pinned in `docs/sources.toml`.
- **Owner storage,** which is private and outside every repo, only when the task's plan
  names it. Nothing in it is copied here.
- **The system temp folder,** for the scratch files of the task in hand and no others: the
  title and body of a pull request while they are scanned, a test's throwaway repo.
  Nothing is kept there.
- **Nowhere else.** Do not read, list or search any other folder on the machine, even one
  that looks related. The clean room depends on it.

## Roles and flow

Development is trunk-based. After the bootstrap, one task is one branch and one pull request
against `main`.

- **Builder:** does one task from the plan in a worktree, on one branch, tests first. Pushes
  the branch, opens the pull request and hands back its head commit. Never merges a pull
  request or into `main`, never pushes to `main`, never tags. It merges `origin/main` into
  its own branch when the orchestrator asks, and the new head is reviewed again.
- **Reviewer:** reads the branch and its pull request against the spec and the task, runs
  `just check` and the range scan itself, and names the head commit it reviewed. Each
  finding either blocks the merge or is a follow-up, which the orchestrator records.
  Changes nothing. Never merges.
- **Orchestrator:** the session the owner is talking to. Only the orchestrator merges. It
  merges with a merge commit, never a squash and never a rebase merge, at the exact head
  that was reviewed, after CI is green on both runners and both scans pass over
  `base..head`. If the head moved after the review, it is reviewed again.

`main` is never rewritten or force-pushed. A pushed tag is never moved: a correction is a new
commit or a new tag. Commit messages are `type(scope): summary`. Stage files by name.

The subagents are defined in `.claude/agents/`.

## Skills

The workflows are checklists in `.claude/skills/<name>/SKILL.md`. Any agent can follow one
as plain text. Each names its inputs, its commands and its hand-back format.

| Skill | Who | When |
|---|---|---|
| `kickoff` | Builder | Starting any task from the plan: make the worktree, build with tests first, push, open the pull request, wait for CI, hand back the head commit; and again after a review asks for a change |
| `port-module` | Builder | A task that brings code from an old repo: on top of `kickoff`, what to port, rewrite or drop, the tests to bring, and how the source is named |
| `add-adapter` | Builder | A task that adds a local model: on top of `kickoff`, its adapter in the pack, its catalogue entry, its conformance run, its model test, its licence files and the origins to ask the owner for |
| `release` | Orchestrator | A version release: choose the commit and the next candidate number, hold CI and both scans on that commit, tag the candidate, wait for `prerelease`, run the local leg and read its receipt, tag the version, and check what was published against `SHA256SUMS`; a candidate that failed keeps its tag, and the next number is tagged |
| `review-merge` | Orchestrator | After the reviewer's verdict: check that it allows the merge and that the head has not moved, test the merged result, run both scans over `base..head`, merge with a merge commit, confirm it, clean up |

The orchestrator does not review its own build: a branch it wrote is reviewed by a reviewer
that did not write it.

## Permissions

`.claude/settings.json` lists commands an agent may run without a prompt: the local `just`
recipes and a few scripts by their exact text, and some read-only `git` and `gh` commands.
These rules reduce prompts. They are not the boundary. The boundary is branch protection on
`main`, the pre-push hook and CI. Anyone who can edit the justfile or a script changes what
an allowed command runs, so a change to either is reviewed as code. A `gh` command that
names another repository by `-R`, `--repo` or an address asks first, but one that hides the
flag in a bundle of short flags, such as `-cR`, is not caught. The list is repeated in
`scripts/tests/test-rules.sh`, and `just check` fails when the two differ. The same test holds
each subagent file to the keys `name`, `description` and `tools`, with its exact list of
tools, and each skill to one `SKILL.md` whose frontmatter is `name` and `description` only: a
skill grants nothing. It refuses any other file in a folder named `.claude`, at any depth: no
commands, no hooks, no settings further down. `scripts/worktree.sh` is not on the list, so it
asks first. So does one script test (`bash scripts/tests/<name>.sh`), and any allowed
command with something in front of it, such as an `export` and `&&`: a rule matches the
exact text. One crate's tests, or one test, run without a prompt as
`just test-only -p <crate>`. That rule ends in a `*`, so what the recipe takes is a closed
set, held in `scripts/test-only.sh`: `-p <crate>` or `--package <crate>`, `--lib`, `--test <name>`, `-E <filter>`, `--no-capture`, `--no-fail-fast`,
and filter words. Any other argument is refused before anything runs,
because nextest's own arguments can name a program to run (a config with a runner in it, a
manifest elsewhere). That is why
`WHIZBANG_WORDLIST` is set where it lasts ("On a fresh clone"), not at the front of each
command. The local settings file that may hold it is never committed.

The tests on this file, the subagent files and the skills are a tripwire, not a proof. They
hold some phrases. They also hold to exact lists every line in which one of a set of words
and command forms appears: those for a forced push, for the git hooks and ways past them,
for the owner's private list and its variable, and for merging through the API. The set
is at the top of that part of `scripts/tests/test-skills.sh`. A sentence on those
subjects that uses none of the words, and any sentence on another subject, passes
unseen. Lines that say who merges are held the same way.
Three more things are held: everything between the heading **Never** under "Boundaries"
and the next heading, line for line; the review rule for plans under "Working with the
owner", as running words; and two sentences of the `kickoff` skill, the one about a refused
push to the end of its step. When a line trips the exact lists, the message names the word and says what to
do. Every change to these files is read in full by the reviewer and by the orchestrator.

## When documents disagree

`SPEC.md`, then the milestone plan, then an ADR (`docs/adr/`), then the carry-over list. The
old repos (rascal, squint, wafflery, table-manners) are reference, never authority. If you
cannot settle a contradiction from these, stop and ask.

## Boundaries

These are the boundaries of `SPEC.md` section 15, with these additions, each marked
(added): rules the spec states elsewhere, or that the milestone plan makes.

**Always**
- Run `just check` before a commit, and `just check` and `just scan` before a push.
- Name the source path and commit in every commit that ports code from an old repo.
- Record the backend, model and version in every result.
- Update the spec, the reference and the carry-over list when a change affects them.
  (added)

**Agents may**
- Do everything in this repo: commit, push, open and merge pull requests, tag, cut version
  releases and publish crates, once CI and both scans pass.
- Spend up to 100 US dollars per milestone on approved recipients (`SPEC.md` section 10),
  without asking. The milestone note records what was spent.
- Each acts in its role above: only the orchestrator merges, tags, releases and
  publishes. (added)

**Only the owner**
- Approves spend beyond that, a new recipient for data, or a reset of the spend allowance.

**Ask the owner first**
- Adding a platform, a door or a kit.
- Using a held-out evaluation set.

**Never**
- Add `Co-Authored-By`, "Generated with", or any agent or model name to a commit, a pull
  request's title or body, code or docs. A `Co-Authored-By` line naming a person is refused
  too. This overrides any tool's default. ("Generated with" and the override: added)
- Commit secrets, the word list, or owner-only material such as answers to unreleased
  evaluation sets.
- Edit a sealed evaluation set, its labels or the rules files its seal covers.
- View the items of a held-out evaluation set while it is held out.
- Rewrite history on `main`, or move a pushed tag. (added)
- Push with `--no-verify`, or bypass a hook any other way. (added)
- Change an old repo beyond what the freeze allows, or delete one.

## Secrets and private data

This repo is public. Everything pushed is published: files, commit messages, branch and tag
names, and the title and body of a pull request.

- **Secrets.** Keys live only in a `.env` that git ignores. Never read the contents of a
  `.env` into output, a log, a prompt or a commit. The scan searches for its values; you do
  not need to see them. Every mode of the scan but the check of one commit message
  refuses when a `.env` is there and cannot be read, and when it is run outside a
  repository.
- **Private paths and addresses.** No absolute home path, no location in owner storage and
  no email address in any file or message.
- **Evaluation sets.** No answer, label or item-level verdict of an unreleased set, in a
  result, a plan, the backlog or a CI artifact. Blind-set material stays in owner storage.
  The repo holds manifests and public labels only.
- **The clean-room scan** reports counts, never text. When it fails, find the cause by
  reading your own change, not by opening the list.
- **Pull requests.** A title and a body are scanned only as two files given to
  `scripts/scan.sh --pr-text` before they are posted (`kickoff`, `review-merge`); text that
  is already on a pull request is seen by no scan. Read them against these rules yourself
  before you open or edit one. The builder, the reviewer and the orchestrator each do.
- **A new place that receives data** (a service, a host, a provider) is the owner's
  decision.

## Old repos and porting

The old repos are frozen at the tag `final-before-whizbang` and are never deleted. Whizbang
has its own history: code is ported module by module and reshaped, never imported. Nothing is
ported without naming its source: the repo, the path and the commit pinned in
`docs/sources.toml`. A test is ported only if it guards behaviour whizbang keeps.

Anything deferred or left behind goes on `docs/carry-over.md` with the milestone that owns
it. There is no other backlog: no task files, no notes kept elsewhere.

## Definition of done

The test tiers that apply to the change pass, with their output attached. The spec, the
reference and the carry-over list are updated where the change affects them. The builder
ticks, in its branch, each box of the plan that the branch makes true once it merges; a box
about something that happens after the pull request opens
is ticked by a later pull request. Anything
deferred is on the carry-over list with a milestone. A tool is not done without its record
(`SPEC.md` section 8). A decision that is costly to reverse has its ADR (`docs/adr/`).

## Working with the owner

- Short status lines: what is running, what finished, what needs them.
- One question at a time, with a recommendation. Explain mechanism, cost and trade-offs
  once, then recommend.
- When blocked on something only the owner can do, give the exact command.
- A new milestone plan, or a change to the spec that changes what will be built (scope,
  architecture, what a milestone holds), is reviewed before building starts: at least two
  rounds by the independent reviewing model, with every blocker and major finding fixed
  and the outcome of each round recorded in the plan. After that, building starts without
  waiting for the owner, who is told what was decided. The owner gave this rule on
  2026-11-11; until then each plan waited for the owner's approval.
- The independent reviewing model is a model from a different provider than the one that
  wrote the text, chosen by the owner, and run through its command-line tool in a
  read-only mode.
- That rule replaces nothing in `SPEC.md` section 15. What section 15 keeps for the owner,
  what it says to ask first, and what it says never to do all stand as written, whatever
  the reviews said. A change to section 15 itself is the owner's.
- An edit to the spec that changes no scope, architecture, boundary or milestone content,
  a matter of wording or accuracy that brings the spec in line with what a reviewed pull
  request did, is reviewed with that pull request. Any other change to the spec gets the
  two rounds first.
- Check a claim about a provider or a model before stating it.
- Do not invent work: if the plan and the carry-over list do not ask for it, propose it.

## Gotchas

- **macOS ships bash 3.2.** There is no `mapfile` and no associative array; scripts here run
  under it. With an `EXIT` trap set, a script that has a syntax error prints the error and
  exits 0, so check a changed script with `bash -n`.
- **`pipefail` and `grep`.** A `grep` that finds nothing exits 1 and fails the pipeline.
  Where no match is a valid answer, end that pipeline with `|| true`.
- **`pipefail` and `grep -q`.** After a pipe, `grep -q` stops at its first match; the
  command before it can be killed while still writing, and the pipeline then fails though
  there was a match. It happened on the Linux runner only. Send the output to `/dev/null`
  instead.
- **The pre-push hook fails closed.** Without `WHIZBANG_WORDLIST` set in the shell you push
  from, the push is refused. That is intended; set the variable, do not bypass the hook.
- **A refused push.** The secrets scan names what it found as `<commit>:<path>:<line>`;
  the clean-room scan prints a count and names nothing. The commits that hold the material
  come off the branch before the next push: those the secrets scan named, or, after a
  clean-room refusal, every commit not yet on origin. They stay in the local repository
  until git collects them and are never brought back. A real key is reported to the owner
  at once and rotated. `kickoff` step 9 has the commands.
- **A version bump needs `just docs`.** The description carries the crate version, so the
  committed reference goes stale and `just check` fails until it is regenerated.
- **The two release workflows run on tags only.** `prerelease` runs on a candidate tag
  and `release` on a version tag (`.github/workflows/`); neither runs on a branch or a
  pull request, so a change to either is seen to work only when the orchestrator tags a
  candidate. Their shape is held by `scripts/tests/test-release-workflows.sh`. No
  provider's key and no model is given to a hosted runner: what needs one is the local
  leg, on the owner's Mac.
- **CI cannot run the clean-room scan,** because the word list never leaves the owner's
  machine. A green CI says nothing about it; only the local scans do.
- **A worktree on a branch cut before a change to the hooks** runs that branch's older hook.
- **`just setup` is run only from a working tree that has `scripts/hook-dispatch.sh`,** and
  again after any change to that file: `scripts/worktree.sh add` refuses while the installed
  copies differ from it. The setup of a branch cut before that file puts the old setting
  back for every worktree of the clone, and a push from another folder then runs no hook.
  Setup replaces whatever is at `pre-commit`, `commit-msg` and `pre-push` in the
  repository's hooks folder, a hook of your own included, and says so.
- **Linux CI catches timing races that macOS misses.** Tests wait on events, never on sleeps.
- **Threads and processes.** A test that made threads until the system refused one took a
  machine down, twice. A test makes a thread only through `whizbang_testkit::threads`, which
  holds a process to 64 of them (`just check` refuses any other way), and no test, probe or
  script holds more than 200 live threads in one process. Every loop that starts a thread, a
  run or a process has a number written in it and waits for each child. Nothing is ever run
  to find out how many a system allows. A thread that cannot be made is shown with
  `Engine::with_spawn`, never by using them up.
- **Starting the real binary in a test.** Only as
  `whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))`, written whole: it
  gives the binary a cleared environment and a temporary home, removed with the command,
  so no test reads or writes a real home. Add arguments, variables of the test's own and
  input to what it returns; ask it for the home (`home()`); keep it until the binary has
  been waited for; never clear or remove a variable of it. `just check` refuses any other
  way, and a test beside it that reads the environment it runs in or takes from the
  command's (`scripts/check-binary-start.sh`, whose top says what it cannot catch).
