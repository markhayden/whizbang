# Whizbang: rules for any agent

Whizbang is one toolkit for working on images with models. `SPEC.md` says how it works;
`docs/plans/` holds the plan for the milestone in progress; `docs/carry-over.md` is the only
backlog; `docs/baseline/` describes the private baseline package and holds its public
manifest. This file holds what you cannot work out from the code.

## On a fresh clone

1. Run `just setup`. It installs the git hooks and checks the pinned tools
   (`scripts/tool-versions.txt`), naming the install command for any that is missing.
2. Set `WHIZBANG_WORDLIST` to the path of a private word list the owner provides. Never
   commit it, open it, quote it, print it or pass it to another agent. Without it you cannot
   push; ask the owner.
3. Run `just scan` and see it pass. Then run `just check`. Both pass before you change
   anything.

## What must pass, and who does it

| Step | Must pass first | Who |
|---|---|---|
| Commit | `just check` | Whoever commits |
| Push | `just check`, then `just scan`: the secrets scan and the clean-room scan. The pre-push hook runs both scans again over every commit the push would publish. Never push with `--no-verify` | Whoever pushes |
| Merge | CI green on both runners, and `just scan-range <base> <head>` on the exact head that was reviewed | The orchestrator, and nobody else |
| Tag, release, publish | The same as a merge, on the commit being tagged | The orchestrator |

## Commands

| Command | When |
|---|---|
| `just check` | Before every commit: format check, lint, the crate graph, licences and sources, the syntax of every script under `scripts/`, the script tests, the generated reference against a fresh copy, then the fast tests with seconds per suite |
| `just docs` | After changing what `whizbang describe` reports (the description type, an action, the version): rewrites `docs/reference/`; commit the result, because `just check` fails on a stale copy |
| `just scan` | Before every push: the secrets scan and the clean-room scan, over the working tree |
| `just scan-range <base> <head>` | Before a merge: the same two scans and the attribution check over every commit in `base..head` (contents, paths, messages) and the tree of `head` |
| `just scan-range-secrets <base> <head>` | The range scan without the clean-room scan: what CI runs over the commits of a pull request (with the base branch's copy of the scan), and of a push to `main` |
| `just ci` | What a pull request runs: `check`, advisories, a smoke of the release binary, the secrets scan |
| `just test-scripts-slow` | After changing `.config/nextest.toml` or `scripts/timing.sh`: about 40 seconds; a 31-second test really fails by timeout (CI runs that part as `just test-timeout`) |
| `scripts/heavy.sh <label> <command...>` | Around any heavy command: one runs at a time on a machine, the next waits |

An action's name and purpose are printed in the reference as they are, so `just docs` refuses
any that could be read as markup. A name is a lower-case letter, then lower-case letters,
digits, `_` or `-`, 64 characters at most. A purpose is one line of at most 200 characters
that starts with a letter, does not end with a space, and holds only ASCII letters, digits,
spaces and `. , ; : ' " ( ) / % + -`, with no `://` and no `www.`. No two actions share a name.
The rule is at the top of `scripts/docs.sh`. `docs/reference/` holds the two generated files
and `tools/` only; `just check` fails on any other file there that git does not ignore.

Every script test file (`scripts/tests/test-*.sh`, and each part under `scripts/tests/scan/`)
ends with the line `finished`. The bash that ships with macOS can exit 0 from a script that
stopped half-way, so a test passes only when that line ran. A script that sets an `EXIT` trap
copies the first lines and the trap of `scripts/scan.sh`; `scripts/tests/test-syntax.sh` holds
it to that.

## Where agents may work

- **This repo,** in a git worktree: one task, one worktree, one branch. Do not edit files
  that another task's worktree is changing.
- **The four old repos** (rascal, squint, wafflery, table-manners), read-only, at the commits
  pinned in `docs/sources.toml`.
- **Owner storage,** which is private and outside every repo, only when the task's plan
  names it. Nothing in it is copied here.
- **Nowhere else.** Do not read, list or search any other folder on the machine, even one
  that looks related. The clean room depends on it.

## Roles and flow

Development is trunk-based. After the bootstrap, one task is one branch and one pull request
against `main`.

- **Builder:** does one task from the plan in a worktree, on one branch, tests first. Pushes
  the branch, opens the pull request and hands back its head commit. Never merges, pushes to
  `main` or tags.
- **Reviewer:** reads the branch against the spec and the task, and names the head commit it
  reviewed. Changes nothing. Never merges.
- **Orchestrator:** the session the owner is talking to. Only the orchestrator merges. It
  merges with a merge commit, never a squash and never a rebase merge, at the exact head
  that was reviewed, after CI is green on both runners and both scans pass over
  `base..head`. If the head moved after the review, it is reviewed again.

`main` is never rewritten or force-pushed. A pushed tag is never moved: a correction is a new
commit or a new tag. Commit messages are `type(scope): summary`. Stage files by name.

The subagents are defined in `.claude/agents/`.

`.claude/settings.json` lists commands an agent may run without a prompt: the local `just`
recipes and a few scripts by their exact text, and some read-only `git` and `gh` commands.
These rules reduce prompts. They are not the boundary. The boundary is branch protection on
`main`, the pre-push hook and CI. Anyone who can edit the justfile or a script changes what
an allowed command runs, so a change to either is reviewed as code. A `gh` command that
names another repository by `-R`, `--repo` or an address asks first, but one that hides the
flag in a bundle of short flags, such as `-cR`, is not caught. The list is repeated in
`scripts/tests/test-rules.sh`, and `just check` fails when the two differ.

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
- Adding a platform, a door or a top-level tool.
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
  not need to see them.
- **Private paths and addresses.** No absolute home path, no location in owner storage and
  no email address in any file or message.
- **Evaluation sets.** No answer, label or item-level verdict of an unreleased set, in a
  result, a plan, the backlog or a CI artifact. Blind-set material stays in owner storage.
  The repo holds manifests and public labels only.
- **The clean-room scan** reports counts, never text. When it fails, find the cause by
  reading your own change, not by opening the list.
- **Pull requests.** No scan sees a title or a body. Read them against these rules yourself
  before you open or edit one.
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
reference and the carry-over list are updated where the change affects them. Anything
deferred is on the carry-over list with a milestone. A tool is not done without its record
(`SPEC.md` section 8). A decision that is costly to reverse has its ADR (`docs/adr/`).

## Working with the owner

- Short status lines: what is running, what finished, what needs them.
- One question at a time, with a recommendation. Explain mechanism, cost and trade-offs
  once, then recommend.
- When blocked on something only the owner can do, give the exact command.
- A new milestone plan needs the owner's approval before building starts.
- Check a claim about a provider or a model before stating it.
- Do not invent work: if the plan and the carry-over list do not ask for it, propose it.

## Gotchas

- **macOS ships bash 3.2.** There is no `mapfile` and no associative array; scripts here run
  under it. With an `EXIT` trap set, a script that has a syntax error prints the error and
  exits 0, so check a changed script with `bash -n`.
- **`pipefail` and `grep`.** A `grep` that finds nothing exits 1 and fails the pipeline.
  Where no match is a valid answer, end that pipeline with `|| true`.
- **The pre-push hook fails closed.** Without `WHIZBANG_WORDLIST` set in the shell you push
  from, the push is refused. That is intended; set the variable, do not bypass the hook.
- **A version bump needs `just docs`.** The description carries the crate version, so the
  committed reference goes stale and `just check` fails until it is regenerated.
- **CI cannot run the clean-room scan,** because the word list never leaves the owner's
  machine. A green CI says nothing about it; only the local scans do.
- **A worktree on a branch cut before a change to the hooks** runs that branch's older hook.
- **Linux CI catches timing races that macOS misses.** Tests wait on events, never on sleeps.
