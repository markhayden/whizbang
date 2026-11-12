# Whizbang: rules for any agent

Whizbang is one toolkit for working on images with models. `SPEC.md` says how it works;
`docs/plans/` holds the plan for the milestone in progress; `docs/carry-over.md` is the only
backlog. This file holds what you cannot work out from the code.

## On a fresh clone

1. `just setup` installs the git hooks and checks the pinned tools
   (`scripts/tool-versions.txt`), naming the install command for any that is missing.
2. Set `WHIZBANG_WORDLIST` to the path of the clean-room word list. It lives outside every
   repo. Never commit it, quote it, print it or pass it to another agent.
3. `just check` and `just scan` must both pass before you change anything.
4. A push runs the scans by itself (the pre-push hook) over every commit it would publish,
   so the variable must be set in the shell you push from.

## Commands

| Command | When |
|---|---|
| `just check` | Before every commit: format check, lint, the crate graph, licences and sources, the script tests, the generated reference against a fresh copy, then the fast tests with seconds per suite |
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

## Roles

- **Builder:** does one task from the plan in a worktree, on one branch, and hands back the
  branch. Never merges, pushes to `main` or tags.
- **Reviewer:** reads a branch against the spec and the task. Never merges.
- **Orchestrator:** the session the owner is talking to. Opens the pull request, runs both
  scans on the exact head that was reviewed, and merges when CI is green.

## When documents disagree

`SPEC.md`, then the milestone plan, then an ADR, then the carry-over list. The old repos
(rascal, squint, wafflery, table-manners) are reference, never authority. If you cannot settle
a contradiction from these, stop and ask.

## Boundaries

**Always**
- Run `just check` before a commit and `just scan` before a push.
- Name the source path and commit in every commit that ports code from an old repo.
- Update the spec, the reference and the carry-over list when a change affects them.

**Never**
- Add `Co-Authored-By`, "Generated with", or any agent or model name to a commit, a pull
  request, code or docs. A `Co-Authored-By` line naming a person is refused too.
- Commit secrets, the word list, or owner-only material such as answers to unreleased
  evaluation sets. This repo is public.
- Edit a sealed evaluation set, its labels or the rules files its seal covers.
- View the items of a held-out evaluation set.
- Rewrite history on `main`, or move a pushed tag.
- Change an old repo beyond what the freeze allows, or delete one.

**Ask the owner first**
- Spend beyond 100 US dollars in a milestone; a new service that receives data; a new
  platform, door or top-level tool; using a held-out evaluation set.

## Definition of done

The test tiers that apply to the change pass, with their output attached. Anything deferred is
on the carry-over list with a milestone. A tool is not done without its record
(`SPEC.md` section 8).

## Working with the owner

Short status lines: what is running, what finished, what needs them. One question at a time,
with a recommendation.
