# Bundles and replay

A bundle carries a run, or a chain of runs, out of one store as one file that another store
takes in (ADR 0048). This page says how to make one, read one, bring one into a store and
remove it, and what it holds and withholds. Replay, offline and live, is added to this page
with plan m7's task R1.

## Making one

```
whizbang runs bundle <run>... --to <file> [--overwrite] --store <dir>
```

Give one run that ended, or a chain of at most 16, in the order they ran, each by its id or
as `runs list` prints it. Each is a run nothing called: a run that another run called travels
inside the run at the top of its tree, and naming it is refused with the name of that run.
Every run under each one comes with it.

Where a later run took an earlier one's output as an input, the bundle links the two, by the
artifact's identity. An input that two earlier outputs could have been (two runs that made
the same picture) refuses the chain: bundle the runs apart.

The file is written as `store export` writes one: in a folder that is there, never over a
file without `--overwrite`, never through a link, never inside the store. The command only
reads the store, so it works while another process writes.

Equal runs make equal bytes: the bundle is an uncompressed tar whose every header is fixed,
named by its SHA-256, which the command prints.

## What it holds, and what it does not

- `bundle.json`, the manifest: each step, its family, its inputs and their links, each file
  and the runs and ports it is the file of, and what is withheld, each with why.
- Each run's record, its journal and its diagnostics, as the store holds them: a fill's
  painted answer and a judge's exchanges among them.
- The canonical file of every input and output, once for each file. Two files of one artifact
  that were made with other origins are both carried.

It holds no path, no account and no value of the env file: every member is a file the store
already holds with its secrets taken out. **It is not encrypted, and it holds the pictures
and the words of the runs.** Send it as you would send the pictures.

What it withholds, `bundle show` lists: an opaque part of a judge's turn (kept as its digest
and length, never its body); a picture a judge was shown that no image of the bundle has; a
diagnostic the store let go of; a fill run made before fills kept their answer; a check
answered from the decision cache, whose turns are another run's; an input or output whose
file the store no longer holds. A bundle with something withheld still bundles.

A bundle carries data, never code. A step of a recipe file or a tool file is carried as
evidence, marked not replayable, and nothing in a bundle is ever run.

## Reading one

```
whizbang bundle show <file>
```

Reads the bundle whole, holds it to every rule, and prints its digest, its steps and their
links, how many files it carries and what it withholds. It reads nothing into a store.

A bundle is trusted for nothing: at most 256 MiB, 4,096 members and a manifest of 1 MiB;
every member a plain file with a plain name that the manifest names; every file the bytes
its name says, decoding to the artifact the manifest says; every record and journal read by
its own reader, every diagnostic the bytes its record says; each family a tree whose root is
its step, its two directions agreeing; every link naming an earlier run's committed output.
A refusal names the first fault and says that nothing was imported and nothing ran.

## Bringing one into a store

```
whizbang bundle import <file> --store <dir> [--create]
```

Checks the bundle, then brings every member in at once, under the bundle's own names
(`bundles/<digest>/…`). Its runs are evidence, not runs of the store: `runs list`, `runs
stats` and the spend books do not count them, and deleting the store's own runs leaves them.
A bundle the store holds already changes nothing.

```
whizbang bundle list --store <dir>
whizbang bundle delete <digest> --store <dir>
```

`list` prints each bundle by its digest, with its steps' actions. `delete` removes the
bundle's names at once and prunes: a file that a run, an ordinary import or another bundle
still names stays.

The playground shows bundles with plan m7's task U2. The engine API and the MCP server over
stdio do not offer them: a bundle carries a store's files out and brings in runs of another
store, which is local control (`SPEC.md` section 10).
