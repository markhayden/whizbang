# Whizbang: rules for any agent

Whizbang is one toolkit for working on images with models. `SPEC.md` says how it works;
`docs/plans/` holds the plan for the milestone in progress; `docs/carry-over.md` is the only
backlog; `docs/baseline/` describes the private baseline package and holds its public
manifest. This file holds what you cannot work out from the code.

## On a fresh clone

1. Run `just setup`. It installs the git hooks and checks the pinned tools
   (`scripts/tool-versions.txt`), naming the install command for any that is missing.
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
| `just check` | Before every commit: format check, lint, the crate graph, licences and sources, the syntax of every script under `scripts/`, the script tests, the generated reference against a fresh copy, then the fast tests with seconds per suite |
| `just docs` | After changing what `whizbang describe` reports (the description type, an action, the version): rewrites `docs/reference/`; commit the result, because `just check` fails on a stale copy |
| `just scan` | Before every push: the secrets scan and the clean-room scan, over the working tree |
| `just scan-range <base> <head>` | Before a merge: the same two scans and the attribution check over every commit in `base..head` (contents, paths, messages) and the tree of `head` |
| `just scan-range-secrets <base> <head>` | The range scan without the clean-room scan: what CI runs over the commits of a pull request (with the base branch's copy of the scan), and of a push to `main` |
| `just ci` | What a pull request runs: `check`, advisories, a smoke of the release binary, the secrets scan |
| `just test-scripts-slow` | After changing `.config/nextest.toml` or `scripts/timing.sh`: about 40 seconds; a 31-second test really fails by timeout (CI runs that part as `just test-timeout`) |
| `just search-canary` | When a scan test fails once and passes on a rerun: 30 seconds of the scan's search against its known output (CI runs it on macOS) |
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

These are heavy: `just check`, `just ci`, `just test`, `just timing`, `just clippy`,
`just docs`, `just docs-check`, `just smoke`, `just test-scripts-slow`, and any `cargo`
command that builds or tests. When another agent may be building on the machine, each goes
through the lock, as in `scripts/heavy.sh check just check`. The scans, `just test-scripts`
and one script test alone are not heavy.

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
asks first. So does one crate's tests (`cargo nextest run -p <crate>`), one script test
(`bash scripts/tests/<name>.sh`), and any allowed command with something in front of it,
such as an `export` and `&&`: a rule matches the exact text. That is why
`WHIZBANG_WORDLIST` is set where it lasts ("On a fresh clone"), not at the front of each
command. The local settings file that may hold it is never committed.

The tests on this file, the subagent files and the skills are a tripwire, not a proof. They
hold some phrases. They also hold to exact lists every line in which one of a set of words
and command forms appears: those for a forced push, for the git hooks and ways past them,
for the owner's private list and its variable, and for merging through the API. The set
is at the top of that part of `scripts/tests/test-skills.sh`. A sentence on those
subjects that uses none of the words, and any sentence on another subject, passes
unseen. The heading **Never** under "Boundaries" and its bullets are held too, in order.
When a line trips that test, its message names the word and says what to do. Every change to these files is read in full by the reviewer and by the orchestrator.

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
  architecture, a boundary, what a milestone holds), is reviewed before building starts:
  at least two rounds by the independent reviewing model, through its command-line tool,
  with every blocker and major finding fixed and the outcome of each round recorded in
  the plan. After that, building starts without waiting for the owner, who is told what
  was decided. The owner gave this rule on 2026-11-11; until then each plan waited for
  the owner's approval. An edit that brings the spec in line with what a reviewed pull
  request did is reviewed with that pull request.
- Spend above 100 US dollars in a milestone, and any new recipient of data, still need
  the owner, whatever the reviews said.
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
- **CI cannot run the clean-room scan,** because the word list never leaves the owner's
  machine. A green CI says nothing about it; only the local scans do.
- **A worktree on a branch cut before a change to the hooks** runs that branch's older hook.
- **Linux CI catches timing races that macOS misses.** Tests wait on events, never on sleeps.
