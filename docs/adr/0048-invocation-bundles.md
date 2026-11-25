# 0048. Invocation bundles

Status: accepted. Changes ADR 0005: the record gains `run/7`
Date: 2026-11-24
Spec: section 3, "The execution contract", "Storage" and "What every run records"; section
4 (the device "recorded in the run"); section 6, "Controls from table-manners" ("Invocation
bundles | Deferred to milestone 7"); section 7, the rows of rascal and the playground that
name milestone 7; section 10 (the engine API exposes no local control); plan m7, task B1

## Context

Milestone 7 carries a call, or a chain of calls, out of one store and into another, and
replays it there (`docs/plans/m7-bundles-and-replay.md`, "Choices this plan makes"). The
spec names invocation bundles and says no more.

The old contract's bundle (table-manners `CONTRACT.md`, section 4.9, at the tag
`final-before-whizbang`) is the reference, never the authority. It carried steps, each with
its resolved call, its original result, its inputs byte for byte and its artifacts' export
files, with `reference_only` and `unportable` lists, in a fixed ustar named by its SHA-256.

What whizbang already has of it:

- an artifact's canonical file, and `store export` and `store import` (ADR 0002;
  `crates/whizbang/src/export.rs`);
- the playground's "Export run", a tar of a run's record, report and outputs
  (`crates/whizbang/src/report.rs`, `tar_file`), whose writer cuts a name at 100 bytes and
  writes no `prefix`;
- "Copy command" (`crates/whizbang/src/command_of.rs`), which names inputs by their
  identity in one store and so does not travel.

What a replay needs of a run, and what the store keeps of it today:

- **A segmenter's answer** is in its run: the masks are committed outputs, and the
  diagnostic `segment.txt` holds each mask's score, box and predicted IoU
  (`crates/whizbang-rascal/src/segment.rs`).
- **A fill's answer is not kept.** The run commits the painted picture composited under
  the mask (`crates/whizbang-rascal/src/fill.rs`, `paste::pasted`), and a rounded blend
  cannot be undone.
- **A judge's turns** are kept whole as exchanges (ADR 0009), but for an opaque part's
  body, a picture (stored as its digest), and the turn's output ceiling, which decides
  whether a cut answer is tried again (`crates/whizbang-squint/src/check.rs`) and was not
  written (`crates/whizbang-core/src/capability/written.rs`).
- **The card a model ran on** is not in the record: a model's entry names its device as one
  word, `cuda`, while the worker names the card in the diagnostics of the `load` that put
  the model on it ("card NVIDIA A10G"; ADR 0046, "Its device"; the carry-over row on the
  run record of a request to a GPU host).

And the engine API's hourly collection lets go of the names of the uploads no run took, but
not of their bytes: `whizbang store prune` needs a store no server holds, so a server held
for months grows its folder by every upload it was sent (the carry-over row on the engine
API's hourly collection).

## Decision

### Bundles

**One file, `bundle/1`, an uncompressed ustar that equal contents make byte for byte.**
Each member's header is fixed: mode 0644, uid and gid 0, no names of users, and an mtime
the writer takes as a parameter, 0 for a bundle and the run's end for "Export run", as
`report.rs` gives it today. `bundle.json` comes first and every other member follows in
the order of its name's bytes, so the archive's SHA-256 names it. A name longer than
ustar's `name` (100 bytes) is split at a `/` between `prefix` (155 bytes) and `name`, and a
name that cannot be so written is refused; the reader joins them. "Export run" moves to the
same writer, whose output for its short names is the same bytes.

| Member | Holds |
|---|---|
| `bundle.json` | The manifest, at most 1 MiB |
| `runs/<run id>/record` | Each step's run record and each of its descendants', as the store holds it, byte for byte |
| `runs/<run id>/journal` | Its journal, as stored |
| `runs/<run id>/diagnostics/<name>` | Each diagnostic it kept: its judge's exchanges, `segment.txt` and `fill-answer.png` among them |
| `files/sha256-<hex>` | Each canonical file (ADR 0002) of an input, an output or an artifact a kept diagnostic or exchange names, once for each file digest: two files of one artifact identity with other origins are two members |

**The manifest.** `{"bundle": 1, "whizbang": <version>, "steps": [...], "artifacts":
[...], "withheld": [...]}`.

- `steps`, one or more, in the order they ran, each a root run (a run nothing nested; a
  child travels inside its root's family and is never a step of its own): its run id, its
  action and its version, the digest of the config its settings came from, whether it is
  replayable by a build of this action set and why not, and, for each input port, either
  the artifact's identity or the earlier step's output it took (`{"step": <index>, "port":
  <name>, "place": <n>}`) when its identity is one an earlier step made. A step never
  names a path.
- `artifacts`: for each file member, its SHA-256, the artifact identity and type it holds,
  and each run and port it is the file of, so every run keeps its own file.
- `withheld`: what the bundle does not carry and why, each with a sentence: an opaque part
  of a judge's exchange (its adapter, digest and length, as ADR 0009 keeps it); a picture a
  stored exchange names that the store does not hold; a diagnostic the store let go of; a
  fill whose answer was not kept (a run made before this decision); a check answered from
  the decision cache, whose originating turns are another run's. A bundle with anything
  withheld still bundles; its offline replay says, before it starts, which step cannot be
  answered and why (ADR 0049).

**What a bundle may hold, and what replays.** A bundle carries data, never code. A step
whose action is one this build ships (`whizbang describe`) is replayable. A step of a user's
recipe file or of an experimental tool file is carried as evidence, marked `not
replayable` with its reason, and refused by replay: the importing store never takes the
recipe or the tool from the bundle, and nothing a bundle holds is ever run as code.

**No secret is in a bundle.** Every member is a file the store already holds with its
secrets taken out (section 3); the manifest holds no value of the env file, no path and no
account. The bundle is not encrypted and holds the person's pictures and words: the guides
say so.

**Reading one trusts nothing.** A bundle is read whole and checked before anything acts on
it, and every check passes before the first publication:

- *Bounds*: at most 256 MiB in all, 4,096 members, a manifest of 1 MiB; a member's header
  is read and its size checked before its bytes; artifacts are decoded one at a time under
  the core's limits (ADR 0001, ADR 0002).
- *Names*: each member a plain file, its name relative, with no `..`, no empty part and
  none of a link's or a device's type; no name twice; every member the manifest names there
  and no other.
- *Artifacts*: each file's SHA-256 is the manifest's; its canonical file decodes; the
  identity it decodes to is the one it is named by.
- *Records and journals*: each read by its own strict reader; each diagnostic's bytes
  match what its record says of it (its digest and length).
- *The whole*: each step's record agrees with the manifest (run id, action, version,
  inputs and outputs); each record's parent is in the bundle and the families are trees
  with one root each, the steps' roots; every input and output a record names is a member
  or listed as withheld; every link names an earlier step's output that its record
  committed, at that port and place, with the identity the later step's record took; the
  two directions of a family agree (a child a record lists is a record of the bundle whose
  parent is that record, with the same action and an ending that agrees; a record with a
  parent is listed by that parent once; no run is the child of two). A record ended
  `interrupted` that declares its steps lost is not held to list its children, and a
  parent's step whose ending it noted as null agrees with a child recovered as
  `interrupted`: such a family is carried whole, and its offline replay is refused, naming
  the lost steps.
- *Version*: an unknown manifest version is refused, naming it.

A refusal names the first fault and says that nothing was imported and nothing ran.

**Importing one** brings it into a store in one publication. Its artifacts go through the
store's import path but are named under the bundle's entry alone,
`bundles/<digest>/files/<file digest>`, never with an ordinary import's name or mark, so a
bundle's files are the bundle's, two files of one identity with other origins are both kept
byte for byte, and an independent import keeps its own name. Every other member is an
object of the store named under the same entry: `bundles/<digest>/manifest`,
`bundles/<digest>/runs/<run id>/record`, `…/journal` and `…/diagnostics/<name>`. A
bundle's runs are never made runs of the importing store: no history, stats or ledger
counts them. A second import of a bundle already held changes nothing. A run started from a
bundle reads its inputs through the bundle's names, and its admission gives each input it
took a name of the run's own, so the run holds what it used once the bundle is gone.

**A bundle is deleted** by `whizbang bundle delete <digest>` or from the playground, which
removes the entry's names in one publication ([`Publication::let_go_of_entry`]); its
artifacts stay while a run, an example, an ordinary import or another bundle names them,
and a prune removes the rest.

**Which doors.** The command line and the playground. Not the engine API: a bundle carries a
store's files out and brings in files that name runs of another store, which is the kind of
local control section 10 keeps off it. Not the stdio server: its tools are the
description's actions (ADR 0031), and a bundle is not an action.

**What differs from the old contract's bundle.** `reference_only` becomes `withheld`, each
entry with its reason. There is no `unportable` list: every artifact type has a canonical
file (ADR 0002), so every artifact travels. A step carries the run records whole, byte for
byte, in place of a resolved call and an original result written for the bundle, so what
was run is read by the record's own strict reader. Names longer than 100 bytes use ustar's
`prefix`.

### The record a bundle carries: `run/7`

`run/7` is `run/6` (ADR 0005, "run/6") with two things after it, written only where a run
has what they add, as `run/5` and `run/6` were. A record that holds neither is written as
before, byte for byte, in the oldest version that holds all of it.

- **A model's card**: in an entry of `models`, after `device`, a field `card`, the card the
  worker named on the load that put the model on it, held to the rule of a card (1 to 64
  printable ASCII characters, no space at either end), and written only when known. A
  card that holds a secret of the run is not kept. It is in an entry only when that
  request was answered, as `device` and `cached` are.
- **`replayed`**: a group after `attempts`: the run this run replays, `{"run": <run id>,
  "bundle": null | {"digest": <bundle digest>, "step": <index>}, "mode": "offline" |
  "live"}`. Every run of a replayed family carries it, a child as its root does, so a
  figure over runs can leave the whole family out. ADR 0049 (replay) and the playground's
  guided chain replay set it through this type (`RunFacts::replayed`); it is written with
  the run's admission and in its record at its end, never added afterwards.

Its schema is `crates/whizbang-core/schemas/run-7.json`. Every older version reads as it
did; nothing in a store is migrated.

**Where the card reaches whizbang.** The pack's worker names the card in the result of its
`load`, as the last of the parts it joins with `; `, `card <name>`, where `<name>` is the
runtime's name of the card cut to 64 characters (`pack/src/whizbang_pack/worker.py`,
`adapter.card_name`). Both whizbang sides read it there; `pack/1` is not changed:

- the kind `pack`'s supervisor keeps the card the `load` named with the model it made
  resident, and hands it on with each call of that model, a call that finds its model
  resident included (`whizbang_backends::pack::queue`);
- the kind `remote` reads it from the `load`'s result among the frames of the front's
  answer (`whizbang_backends::remote::answer`).

Each carries it into the answer's `Served` (`capability/served.rs`, `Served::with_card`),
where the run's record takes it. A text that is not a card by the rule is not kept. A
worker of a build before this one names a card the same way, so its card is read too.

### What the store keeps for replay

- **A fill run keeps the painted picture it was answered with**, as the diagnostic
  `fill-answer.png`: the filler's red, green and blue with an opaque alpha, written by the
  core's one encoder as a canonical image's PNG (ADR 0002), kept exactly or not at all,
  and only when the answer is the image's size, so it is at most the image's size. A fill
  run made before this decision has none, and its offline replay is `unanswerable`,
  naming that. A segmenter's answer is rebuilt from its run's committed masks and
  `segment.txt`, and a test rebuilds the answer the fake gave and compares it whole; it is
  not kept again.
- **A judge's exchange keeps its turn's output ceiling**: `output_ceiling` in the turn's
  answer of `judge-exchange/1`, written only when the backend states one, read back as
  none from an exchange written before. An older exchange whose answer was cut at a
  ceiling it did not keep is `unanswerable` offline (ADR 0049).
- **A bundle's entry and a saved example are names that keep what they hold.** A bundle's
  names start `bundles/<digest>/` and an example's is `examples/<id>` (plan m7, task U1).
  They belong to no run and are no import's mark, so `store delete` of runs leaves them,
  the collection of unused imports never lets them go, and a prune keeps every object
  they name. Each entry is removed whole by one publication that lets go of it
  ([`Publication::let_go_of_entry`]); nothing else removes them. What the entry named then
  goes at the next prune, unless another name (a run's output, a run's hold, an ordinary
  import, another bundle) still names it.
- **The engine API's hourly collection ends in the store's own prune**
  ([`Store::collect_and_prune`]), run through the writer the server holds, so an upload's
  bytes go with its name. A prune leaves every generation a reader or a run holds and
  everything it names (ADR 0036): a run admitted against a generation holds it until the
  executor is done with the run (`contract/engine.rs`, `Store::hold`), so a run going
  keeps every file it reads. A prune that fails is tried at the next collection; the
  collection's own count is unchanged.
- **Rolling back.** The store's names take any text (`store/buckets.rs`), and the names of
  a bundle's entry and an example are bucketed by the rule that already holds for a name
  with a `/`, so the store's format does not change. A build before this decision opens a
  store with `bundles/` or `examples/` names, reads it, and leaves those names alone; it
  refuses a `run/7` record when it reads one, by its version, and reads every other run of
  the store. A revert keeps the record's reader and drops the writers.

## Consequences

- A run can leave its store, and every later task of milestone 7 reads what it needs from
  the record and the store as this decision keeps them.
- The record of a request to a GPU host names the card it ran on, which closes the
  carry-over row on `device cuda` alone.
- A server held for months no longer grows by every upload it was sent; it prunes once an
  hour, which costs a listing of the store's objects.
- A fill run writes one more diagnostic, as large as the image's PNG at most.
- `run/7` cannot be read by a build before this one, which reads every other record of
  the same store.

## Alternatives

- **A new field of `pack/1`'s hello or result for the card.** The card already reaches
  both sides in the `load`'s result, in a form the worker has written since ADR 0046; a new
  field would be a change of the protocol for nothing more.
- **Keeping the segmenter's answer as a diagnostic too.** It is rebuilt whole from what the
  run already keeps, and a test holds that; a copy would be the same bytes twice.
- **A bundle as a folder, or compressed.** A folder is not one file to send, and a
  compressor's output is not held still the way the core's encoder is; the SHA-256 of an
  uncompressed ustar with fixed headers names its contents.
- **Offering bundles over the engine API.** Section 10 keeps local control off it, and an
  app already finds its runs by id.
- **Pruning the server's store only when the owner runs `store prune`.** It needs a store no
  server holds, which a server held for months never is.
