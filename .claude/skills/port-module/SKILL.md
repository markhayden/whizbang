---
name: port-module
description: "Bring one module from a frozen old repo into a whizbang crate, tests first, naming its source at the pinned commit. For the builder."
---

# port-module

For the builder, when a task brings code from one of the four old repos (rascal, squint,
wafflery, table-manners). It is a checklist on top of `kickoff`: follow `kickoff` for the
worktree, the commits, the push, the pull request and CI, and this for what is specific to
porting.

## Inputs

- **Source repo:** one of the four old repos, by its name in `docs/sources.toml`.
- **Module or path:** what to bring, as a path in that repo.
- **Target crate:** the whizbang crate that receives it.
- And the three inputs of `kickoff`: task id, plan file, base commit.

If one is missing, ask the orchestrator. Do not guess.

## Checklist

- [ ] 1. Start with `kickoff`, steps 1 to 4.
- [ ] 2. The source commit is the one `docs/sources.toml` pins for the source repo, and no
      other. In your checkout of the old repo, the tag that entry names points at that
      commit; if it does not, stop.
- [ ] 3. The old repo is frozen and read-only. Read it at the pinned commit with `git show`,
      `git ls-tree` and `git grep`. Do not check out, commit, stash, fetch into or change
      anything there.
- [ ] 4. Clean room. Copy from the pinned repos only: never from another folder on the
      machine, another project, or text found elsewhere. The clean-room scan reports counts,
      never text; when it fails, find the cause by reading your own change.
- [ ] 5. Read the module and decide, piece by piece: **ported** (same behaviour, reshaped
      to whizbang's types and names), **rewritten** (the behaviour is kept, the code is
      new), or **dropped**. Drop what `SPEC.md` drops (section 7 and the sections the task
      cites). The old repo is reference, never authority: where it disagrees with the spec,
      the spec wins. State the closure first: which files of the old repo the piece needs,
      and no more. Its helpers come with it, cut to what it uses; another operation or
      tool does not.
- [ ] 6. Tests first. Port the tests that guard behaviour whizbang keeps, and see them fail
      before any code is brought. Where the old module has no such test, write
      characterisation tests that fix what the old code does at the pinned commit, then
      port against them. Build each fixture directly, in the test, from numbers: a fixture
      that needs another old module to make it drags that module in.
- [ ] 6a. A difference from the old code that is made on purpose is a decision, not a
      slip: state it where the code is, give its reason, and put it under "Not ported" in
      `docs/carry-over.md` and, for a tool, under "Limits" in its record. What decides
      which pixel or which number comes out is worked in whole numbers where it can be,
      so that every machine gives the same answer; a result that is hashed never comes
      from a machine's own mathematics library (ADR 0001).
- [ ] 6b. Before porting code that a library may already do (a dilation, a resize, a
      transform), look for a crate that does the same. If one does, run both through the
      tool's record script on the same inputs: the faster one that is still exact becomes
      the method, and the comparison goes in the record's "Alternatives". A library that
      is tried and not kept leaves the dependencies as they were.
- [ ] 7. No shims and no backwards compatibility: no old names kept as aliases, no reading
      of old formats or settings, no layer that lets old callers keep working, unless the
      spec asks for it by name.
- [ ] 8. Check the crate graph. The target crate depends only on what
      `scripts/crate-graph.txt` allows (`just graph`). Changing that file is changing the
      architecture: stop and ask.
- [ ] 9. Every commit that ports code names the source repo, the path and the pinned commit
      in its message. The pull request's body names them too.
- [ ] 10. Everything not ported goes in `docs/carry-over.md`: deferred, with the milestone
      that owns it, or dropped, with one line saying why. A row this port closes is removed.
- [ ] 10a. When the port is a tool or an operation, write its record,
      `docs/reference/tools/<name>.md`, with the parts `SPEC.md` section 8 lists: what it
      does, at least two worked examples with their images, the rung 2 results, speed at
      several sizes, the alternatives that were tried, its limits, and its mark and
      history. The record is made by a test of the tool's crate (`record::write_the_record`,
      ignored in the fast tier) that `scripts/tool-record.sh <name>` runs: add the tool to
      that script. The fast tests then hold the record to the tool, its example images
      byte for byte and every measurement that does not depend on the machine, so the
      examples run as tests. Times are measured on a quiet machine and are not held by a
      test. A tool is not done without its record.
- [ ] 10b. When the port is an action, register it where the binary builds its registry
      (`crates/whizbang/src/wiring.rs`) and run `just docs`: `just check` fails on a
      reference that is not current. A port that comes to more than one pull request's
      worth is split, the action and its tests first, then its command and its record, and
      the plan says so.
- [ ] 11. Finish with `kickoff`, steps 6 to 15.
      Never merge a pull request, and never merge into `main`: only the orchestrator does.

## Commands

```sh
# In your checkout of the old repo: read only.
git -C <old-repo> rev-parse '<tag>^{commit}'                   # equals the pin
git -C <old-repo> ls-tree -r --name-only <commit> -- <path>
git -C <old-repo> show <commit>:<path>
git -C <old-repo> grep -n <text> <commit> -- <path>

# In the worktree.
grep -A 4 'name = "<source repo>"' docs/sources.toml           # its <tag> and <commit>
just graph                                                     # the crate edges
scripts/heavy.sh record scripts/tool-record.sh <name>          # 10a: the tool's record
just docs                                                      # 10b: the reference
just check
just scan
```

A commit that ports code:

```text
feat(<target crate>): <what was brought>

Ported from <source repo> <path> at <full pinned commit>.
```

## Hand-back format

The hand-back of `kickoff`, with these two parts added:

```text
Source: <source repo> <path> at <full pinned commit> (docs/sources.toml)

Ported, rewritten, dropped
| Old path or item | Result | Where in whizbang, or why dropped | Test |
|---|---|---|---|
| <path> | ported / rewritten / dropped | <file, or the reason and the carry-over row> | <test, or "none: dropped"> |

Record: <docs/reference/tools/<name>.md, and the test that holds it; or "none: not a tool">
Differences made on purpose: <each, with where it is stated; or "none">
```
