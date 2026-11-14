# Plan: checkpoint 1A, contracts and store

Draft 6, 2026-11-11. From `SPEC.md` sections 3, 4 and 16. This file is the task list.

## What this checkpoint delivers

The core that everything else is ported onto, proven by two thin paths through it:

1. **A rascal operation,** `grow`, run from the command line on a real mask, through the
   executor, with its output committed to the store.
2. **A scripted squint run:** a fake judge takes several turns, calls one evidence tool
   (`view`), and ends in a verdict, with its tool calls and the images it may read limited to
   what the run froze.

No model runs in this checkpoint. Every capability is a fake from the test kit.

## Decisions this plan makes

- **Three ADRs, each merged with the first code that depends on it:** identities (A1),
  artifacts and the mask record (A4), and the capability interfaces with fill's conventions
  (A8). The execution contract (A5) turned out to need its own: ADR 0003 is the action
  descriptor's and ADR 0004 is preparing a call's, and the run record will have one; so the
  capability interfaces take the next free number, and the cancellation rule of A7 goes in
  the record for executing a call. Request ids and admission (A6) took one too: ADR 0007.
- **The store is files on disk,** published the way rascal's workspace publishes: immutable
  generations, a single pointer that names the current one, and file and directory syncs in a
  fixed order. No database.
- **Two partial ports, stated as partial.** `grow` is ported whole. `view` is ported for flat
  images only (crop, snapping, downscaling, the second-image argument, and the mapping back);
  its panorama rendering waits for milestone 2. Each brings only the tests that guard what is
  ported, with fixtures built directly, so no other operation or tool is dragged in.
- **The contract is checked against the hard cases on paper as fixtures.** Descriptors for
  rascal's `seam` and `remove_prompt` are written as test fixtures, without porting either, to
  show the descriptor can express several outputs of different types, an optional protect
  input, a bounded number of instances, a capability needed only for some arguments, and a
  mask rebound onto a composite.
- **Test actions never ship.** The registry is built by code the binary and the tests share,
  and the tests add their fake actions to it. The shipping binary's description is asserted
  not to contain them, and the binary does not depend on the test kit.
- **Crates created:** `whizbang-rascal`, `whizbang-squint`, `whizbang-testkit`.
  `whizbang-backends`, `whizbang-eval` and `whizbang-serve` wait.

## Order

Each task needs every task listed for it to be merged first.

| Task | Needs |
|---|---|
| A1 Identities | milestone 0 |
| A2 Store: publication and recovery | milestone 0 |
| A3 Store: locking, deletion, diagnostics | A2 |
| A4 Artifacts and the mask record | A1, A2 |
| A5 The execution contract | A3, A4 |
| A6 Request ids and admission | A5, A8 |
| A7 Nested calls, cancellation, publication | A6, A8 |
| A8 Capabilities and the fill composite | A5 |
| A9 `grow` through the executor | A7, A8 |
| A10 `view`, extracted | A4 |
| A11 The scripted squint run | A7, A8, A10 |
| A12 Exit check | all |

A1 and A2 can run together. A8 follows A5; A6 follows A8; A7 follows A6. A10 can run any time
after A4. Nothing else is parallel.

## Commit strategy

- One task is one branch and one pull request, built by the builder, reviewed by the reviewer
  and merged by the orchestrator with a merge commit once `ci` and both scans pass.
- An ADR is in the same pull request as the first code that depends on it.
- Ported code names its source in the commit message: the old repo, the path, and the pinned
  commit from `docs/sources.toml`.
- **Rollback tag:** `m1a` at the exit check. There is no tag before it; an intermediate state
  of the contract is not worth returning to.
- **If a proof shows the contract is wrong** (A9 or A11 cannot be written cleanly), the
  contract is fixed in its own pull request and its ADR amended. A proof is never bent to fit.

### What undoing each task takes

| Tasks | To undo |
|---|---|
| A1, A2, A3, A4, A5, A8 | These are foundations. Reverting one means first reverting every later task that depends on it (the order table shows which). A store written by newer code is discarded, not migrated; nothing in this checkpoint holds data worth keeping |
| A6, A7 | Revert every later task that depends on it first, then the merge. Run and request-id records written by tests live in temporary stores only |
| A9, A10, A11 | Revert every later task that depends on it first (A11 depends on A10), then the merge; the registry, the command, the reference and the tests are in the same pull request and go together |
| A12 | The note reverts; the `m1a` tag stays and a corrected checkpoint is `m1a.1` |

## Tasks

### A1. Identities

**Reads:** rascal `crates/rascal-core/src/frame.rs` and ADR 0016; squint
`crates/squint-core/src/frames/mod.rs` and `crates/squint-core/src/image.rs`. Neither old
identity is kept as it is: rascal's frame ignores image content, and squint's decoder records
a colour profile without applying it.

- [x] ADR 0001: the file digest; the canonical pixel digest and exactly what canonical means
      for orientation, channel order, alpha and colour profiles; coordinate space; binding
      (source image, destination image or root, both spaces, direction, mapping); the order in
      which identities depend on each other, with content first, bindings next and run
      provenance last, and nothing local (ids, timestamps, paths) in any of them.
- [x] The types in `whizbang-core`. No masks and no store in this task.
- [x] Fixed vectors for the canonical digest: the same picture stored with each orientation
      tag; with and without an alpha channel; with and without a colour profile. Re-encoding
      means the same samples in another container, not a lossy recompression.

**Done when** each case below has a test asserting the stated result:

| Case | Expected |
|---|---|
| Two different images of the same size | Different image identities; equal spaces |
| The same pixels in two containers | Different file digests; equal canonical digests |
| A flat image and a panorama of the same dimensions | Different spaces |
| One source aligned onto two different references of the same size | Two different bindings |
| A crop | A point maps to its root and back within one pixel; a point outside the crop is refused |
| A homography | Maps in the stated direction; the reverse binding is its inverse within tolerance |
| A singular homography | Refused when the binding is built |
| A rotation on the sphere | Composes with its inverse to the identity; wraps at the seam |
| A wrapped cut and put | A put after a cut returns every pixel to its original place |

**Verify:** `just test-only -p whizbang-core identity`.
**Size:** medium.

### A2. Store: publication and recovery

**Reads:** rascal `crates/rascal-core/src/workspace/commit.rs` and `registry.rs`; squint
`crates/squint-core/src/journal.rs` and `artifacts.rs`.

- [x] Content-addressed files staged on the same filesystem as the store.
- [x] Publication writes a new immutable generation and then replaces one pointer. Files are
      synced, then their directory, then the pointer, then its directory.
- [x] Two separate guarantees. **Visibility:** replacing the pointer makes the new generation
      visible. **Durability:** success is reported to the caller only after the final
      directory sync.
- [x] Recovery on open: leftover staging is removed. After a crash before success was
      reported, the store holds either the old generation or the new one, each complete,
      never a mixture. After a crash once success was reported, it holds the new one.
- [x] Faults are injected through a file-system shim in the test kit that drops writes and
      renames which were not synced, as rascal's test kit does. Raising an error and reopening
      the same files does not test this.

**Done when** the shim-based tests pass for a crash at each of these points: while writing
payload files; after payloads and before the pointer; after the pointer and before its
directory sync; after the final sync. For the first three, reopening shows one complete
generation, old or new. For the last, it shows the new one.
**Size:** medium.

### A3. Store: locking, deletion and diagnostics

**Depends on:** A2.

- [x] One writer: a lock held for the life of a writing process. A real second process is
      refused, with a message naming the holder, while a reader in a third process works.
- [x] Recovery and the removal of staging happen only after the writer lock is taken. Opening
      to read, and an open that is refused, clean up nothing.
- [x] `whizbang store list|delete|prune`: deleting a run removes what only it references and
      keeps what another run shares. A reader that opened a generation keeps reading it while
      a prune runs. Spend and request-id records are never removed.
- [x] Two kinds of reference to stored content: a committed output, and a diagnostic. The kind
      belongs to the reference, not to the file, so two references to the same bytes can
      differ.
- [x] Everything the store writes is readable by the owner only: final files, staging,
      journals and the index.

**Done when** these pass: with a first writer paused after staging its files, a reader opens
and a second writer is refused, the staging is untouched, and the first writer's publication
then succeeds; a second writing process is refused and a reader is not; two runs
share an artifact and deleting one keeps it; a prune during a read does not break the read; a
failed run's diagnostics are readable after reopening and after pruning other runs; the same
bytes referenced as a diagnostic by one run and as an output by another are treated as each
reference says; every created path has owner-only permissions.
**Size:** medium.

### A4. Artifacts and the mask record

**Depends on:** A1, A2. **Reads:** rascal `crates/rascal-core/src/artifact.rs`, `mask.rs`,
ADR 0006 and ADR 0016.

- [x] ADR 0002: the six types (`Mask`, `Location`, `Image`, `BandProfile`, `Series`, `Plan`),
      each with its format, its identity and what it records of its origin; the mask's core,
      blend and protect planes and its prescriptions; what a mask must carry to be read in
      another store.
- [x] The types, serialised canonically.
- [x] A dev command, `whizbang store import <file>`, that brings a canonical artifact file
      into the store, so fixtures can become inputs.

**Done when** these pass: each type is created, stored, read back and has the same identity;
the same mask written in two stores has the same identity; a mask that carries a profile and
prescriptions round-trips with them; a rendering is refused where a mask is required; a mask
is refused on an unrelated image of the same size; a mask is accepted, through a checked
rebind, on a composite derived from its image.
**Size:** medium. `BandProfile`, `Series` and `Plan` get their type and identity here and their
behaviour in milestone 3.

### A5. The execution contract

**Delivered in parts.** Each part ticks the boxes it makes true.

- **Part 1, merged:** the action descriptor (ADR 0003), registration, the static
  description and the two fixtures: boxes 3, 6 and 7, and of "Done when" the two fixtures
  validating, the shipping description without `echo`, and the crate-graph check showing
  that the binary does not depend on the test kit.
- **Part 2:** from box 2, the call, the prepared call, the context, the outcome, the
  executor interface and the capability handle types; box 4, the construction of the
  registry and the executor, with the entry point where a request id will be looked up; of
  "Done when", `remove_prompt` prepared with and without a filler reporting different
  capabilities and readiness. With it: prepare has no side effects and spends nothing, and
  is tested as such; who may state a derivation, and the run id's rule
  (`docs/carry-over.md`).
- **Part 3, in two pull requests.** First, the run record (ADR 0005): the rest of box 2,
  the run record's type, with its schema, its redaction and its reading from a store; and
  the decision that the run record, not an artifact's origin, names the run and the
  models. Then: box 1, the record written by a run, and `whizbang runs list|show`; box 5,
  `echo` running; of "Done when", `echo` through the executor with its record in the
  store, and a failed `echo`; diagnostics written through the store and redacted
  (`docs/carry-over.md`).

**Depends on:** A3, A4. **Reads:** rascal `crates/rascal-core/src/op.rs` and the recipes
`crates/rascal-mask/recipes/seam.toml` and `remove-prompt.toml`; squint
`crates/squint-core/src/tool/manifest.rs`; table-manners `schemas/core/description.json` and
`result.json`.

- [x] The run record as a versioned type holding every group of `SPEC.md` section 3, "What
      every run records", with `whizbang runs list|show` to read it.
- [x] In `whizbang-core`: the action descriptor, call, prepared call, context, outcome, run
      record and the executor interface; and the handle types through which a context offers
      capabilities, with their contents left to A8.
- [x] Also reads rascal `crates/rascal-mask/src/describe.rs`: the optional filler argument
      comes from the action description's common properties, not from the recipes.
- [x] Registry and executor construction in a module the binary and the tests both use. The
      executor's entry point is where a request id is looked up (A6 fills it in).
- [x] `whizbang-testkit`: an `echo` action, registered only by tests.
- [x] Descriptor fixtures for `seam` and `remove_prompt`, written from their recipes and the
      common properties, that must be expressible: several outputs of different types; an
      optional protect input; a bounded number of instances; a capability needed only for some
      arguments (a filler chosen, or the mask alone); a mask rebound onto a composite.
- [x] `whizbang describe` lists what the binary registered.

**Done when** these pass: `echo` runs through the executor in a test and its run record is in
the store, validates against the record's schema, and has every group filled or explicitly
empty; a failed `echo` writes the same record with its error; both descriptor fixtures validate; the `remove_prompt` fixture, prepared once with
a filler and once without, reports different required capabilities and readiness; the
shipping binary's description does not
contain `echo`, and the crate-graph check shows the binary does not depend on the test kit.
**Size:** medium.

### A6. Request ids and admission

**Delivered in three parts** (ADR 0007 decides all of them). Each part ticks the boxes it makes
true.

- **Part 1:** the ADR; the request id, the fingerprint of a request, the admission and
  ending records and where they are in the store; the look before prepare; the run made
  durably, with the id's record, in one publication, and ended in a second; boxes 1 and
  5, and of "Done when" lines 1 to 5, and line 6 for the id's record and the run.
- **Part 2:** the estimate, and a prepared call handed to admission and checked again:
  box 4, and of "Done when" line 7 as far as "no run". Box 2 is built except for what it
  shares with part 3: that prepare makes no reservation is shown there.
- **Part 3:** the fake ledger and the reservation: boxes 2 and 3, and of "Done when"
  line 8, line 7's "no reservation", and line 6 again with the reservation in the step.

**Depends on:** A5, A8 (its tests count capability requests with A8's fakes).

- [x] A request id is looked up at the executor's entry, before prepare is called.
- [x] Prepare returns readiness for the exact arguments and an estimate, bound to the digests
      of the inputs, the config and the model identities it saw. It writes nothing.
- [x] Admit revalidates, then records the id, the request fingerprint, the run and a
      reservation in one durable step. The reservation goes to a fake ledger that takes part
      in that step; the real ledger is checkpoint 1B.
- [x] A stale preparation is refused and creates no run.
- [x] When a run has been pruned, its request-id record remains and answers a repeat with what
      happened to the run.

**Done when** these pass:

- Two simultaneous requests with one id produce one run.
- A different request under the same id is a conflict.
- The identical request and id, sent again after the config changed, returns the first run;
  prepare is not called (the test's prepare panics if it is) and nothing is dispatched.
- A repeat after a restart returns the first run.
- A repeat after the run was pruned is answered from the record and dispatches nothing.
- A crash injected inside the admission step leaves either no record or all of it.
- A preparation is refused at admit, with no run and no reservation, when the input changed,
  when the config changed, and when the model identity changed; each is its own test.
- Prepare makes no durable write, no reservation and no capability request, asserted against
  a store and fakes that count them.
**Size:** medium.

### A7. Nested calls, cancellation and publication

**Built in three pull requests** (ADR 0008 is written with them, each part amending it),
because every task of this size so far came to two or three times the size of one. A box
is ticked by the pull request that makes it true.

- **Part 1, the commit step:** box 5. An action hands back artifacts; the executor holds
  them to the ports its call returns and publishes them, with the completed status, in
  the publication that ends the run; a panic in an action ends its run through the same
  publication. Of "Done when": the fake action with two outputs, crashed inside its
  commit. With it, from `docs/carry-over.md`: the store reads no more of a stored file
  than its entry says. That covers the files under `objects/` only: `CURRENT`, the
  generation files and the lock file were still read whole, and each was given a limit
  at the exit check (task A12; ADR 0008, "What the store reads of its own files").
- **Part 2, in two pull requests,** because the first half alone came to about a
  thousand lines. First, status and progress: `start`, `status` and `wait` on the
  executor, one table of open runs, and of "Done when" the caller that disconnects and
  returns, and the second caller reading a status. Then cancellation, with everything
  else listed here. Together: boxes 1 and 4, and of box 3 that a
  cancellation is accepted or comes too late. Status and cancel answered while an action
  is busy, by a second caller; progress events; a run found by its id after its caller
  left; a fake capability that blocks at a named barrier and sees the cancellation; a
  hold on the generation a run's inputs were read from. Of "Done when": the cancel before
  the commit step and the one after it; the second caller that reads a status and
  cancels; the caller that disconnects and returns.
- **Part 3, nested calls:** box 2 and the rest of box 3. Calls through the context's
  executor, each with an allowance taken from its parent's; the fifth level and the call
  back refused; a cancellation that reaches descendants; a parent that is terminal only
  when its descendants are and every capability request is accounted for; the parent and
  the refusals in the run record. Of "Done when": the grandchild refused after a
  cancellation; the parent that stays non-terminal while a capability request is
  outstanding; the completed child whose outputs remain when its parent fails.
  It came to over three thousand lines, and is **two pull requests** in turn. First:
  box 2, with the allowance, the two refusals, and the parent, the calls and the
  refusals in the run record. Then: the rest of box 3, with those three of "Done when".

**Depends on:** A6, A8. Rascal's old rule, that a commit which has taken its lock always
completes, is not carried over without being restated here.

- [x] The rule, written in the ADR for the contract: a cancellation is accepted if it arrives
      before the action enters its commit step, and then nothing is committed; one that
      arrives after returns the completed outcome.
- [x] Nested calls through the context's executor, each with an allowance taken from its
      parent's; a fifth level and a call back into an action on the stack are refused.
- [x] Cancellation reaches descendants and refuses new ones. A parent is terminal only when
      its descendants are terminal and every request a capability was sent is accounted for.
- [x] Progress events; status and cancel answered while an action is busy.
- [x] One commit step for an action: all of its output references and its completed status
      are published together, through A2's publication, and the completed outcome is returned
      only after that publication is durable.

**Done when** these pass, with test actions that stop at named barriers: a fake action with
two outputs, crashed between the writes a wrong implementation would separate, reopens with
neither output nor a completed status, or with both and the status; a cancel before the
commit step leaves no outputs; a cancel after it returns the completed outcome; a child that
tries to start a grandchild after cancellation is refused; a parent stays non-terminal while
a fake capability request is still outstanding, and becomes terminal when it is accounted
for; a completed child's outputs remain when its parent then fails; while an action is in
blocking work, a second caller reads its status and cancels it; a caller that disconnects and
returns finds the run by its id.
**Size:** medium.

### A8. Capabilities and the fill composite

**Depends on:** A5. **Reads:** squint `crates/squint-core/src/provider.rs`; rascal
`tools/python/src/rascal_tools/models/adapters.py`, and the composite and its preservation
check in `crates/rascal-mask/src/heal.rs`.

- [x] An ADR, the next free number (0003 is the action descriptor's): the request and response of segment, fill and judge; feature declarations;
      resolution. For judge: ordered mixed content, provider-opaque parts kept whole, usage,
      and typed errors. For fill: the support plane's polarity and range; how core, blend and
      protect become it; the colour and alpha rules of the returned pixels.
- [x] The interfaces in `whizbang-core`; no implementation.
- [x] `whizbang-testkit`: a fake segmenter, a fake filler that can repaint the whole image,
      and a fake judge that plays a scripted transcript.
- [x] `whizbang-rascal` is created with one pure function: composite a fill onto an image
      under a mask.

**Done when** these vectors pass: the fake filler repaints every pixel, and after compositing
every pixel outside the support and every protected pixel is exactly the original; a blend
weight of one half gives the stated mix; a transparent source pixel is handled as the ADR
says; a fill of the wrong dimensions is refused; a support plane with values outside its
range is refused. And: resolution takes an explicit choice, then the default, and errors when
a needed feature is missing.
**Size:** medium.

**Built in three pull requests,** because the task came to about three times the size of
one: first what every capability shares, faults, declarations, resolution, and the fill
and segment interfaces, with those sections of the ADR; then the judge interface, the
fakes, the handles in a context and a call's choice of backend; then `whizbang-rascal`
with the composite. A box is ticked by the pull request that makes it true.

### A9. Proof one: `grow` through the executor

**Depends on:** A7, A8. **Reads:** rascal `crates/rascal-ops/src/grow.rs` and `refine.rs`.

- [x] `whizbang-rascal` gains the action `grow`, with both of its arguments, `px` and the
      optional `down_px`.
- [x] Its behaviour is kept: it clears the blend plane, the profile and the prescriptions, and
      it keeps the protect plane.
- [x] Tests, extracted for `grow` alone with fixtures built directly: growth by a fractional
      radius matches the analytic expectation; downward extension; protect takes precedence;
      the refinement is cleared.
- [x] Before porting the pixel code, check whether a library call does the same. If one does,
      both are run through the record's script on the same masks and the faster one that is
      still exact becomes the method; the comparison goes in the record's "Alternatives".
      (Ticked on a comparison run once, with the library as a temporary development
      dependency that was not kept: the note under "Done when" says how.)
- [x] `whizbang rascal grow --mask <artifact> --px 8 [--down-px 4]`.
- [x] The first tool record, `docs/reference/tools/grow.md`, as `SPEC.md` section 8 describes:
      two worked examples with their images; exactness against the analytic expectation;
      time at three mask sizes; known limits. Its examples run as tests, and a script
      regenerates its numbers.

**Done when** the tests pass; the command, run on a fixture mask brought in with
`whizbang store import`, commits a mask whose origin names the action, its arguments and its
input; the same inputs give the same artifact identity. And the publication tests of A7,
named for this action: a cancelled `grow` commits nothing; a failed `grow` commits nothing
and keeps its diagnostics; a completed outcome is not returned before the publication is
durable; after a reopen the mask and the completed status are both present. After a failure
or an accepted cancellation, including a failure once output bytes are staged, the store is
closed and reopened: the run has its terminal status, no committed output and no completed
status, and every diagnostic written before it ended is readable.
**Size:** medium.

**Built in two pull requests,** because the task came to over three thousand lines: first
the action and its tests, the first three boxes; then the command, the publication tests
of "Done when" and the tool record, the last two. The fourth box is ticked with the second, on this basis: no crate
whizbang depends on does the same job; `imageproc` 0.27.0 does, and was run once, as a
temporary development dependency, through the record's script on the same masks. It
gave the rule's pixels and took about three times as long, so the hand-written method
stays. The dependency was then removed, and the result is in the record's
"Alternatives" as recorded numbers, marked as not verified again by `just check`.

### A10. `view`, extracted

**Depends on:** A4. **Reads:** squint `crates/squint-tools/src/builtin/view.rs` and
`raster.rs`, and their pixel and mapping tests.

- [x] `whizbang-squint` is created with `view` for flat images: a crop by box, snapped
      outward to whole pixels; downscaling to a bound; the optional second image; each
      returned crop as a binding onto its source.
- [x] Its pixel and mapping tests, without the old loop tests.
- [x] Panorama rendering is left for milestone 2 and listed on the carry-over list.

- [x] Its tool record, `docs/reference/tools/view.md`: two worked examples with their images;
      mapping error against known boxes; time at three image sizes; known limits, including
      that panoramas are not yet supported.

**Done when** the extracted tests pass, a location in a returned crop maps back to the
source image within one pixel, and the record's examples run as tests.
**Size:** small.

**As built** (amended at the exit check, task A12). "Within one pixel" is true of the
mapping and is not the whole of what was measured (`docs/reference/tools/view.md`, "Rung
2 results" and "Limits"; ADR 0001 for the mapping). The mapping itself is exact at a
crop's corners and within 1e-9 of a pixel inside it. What a judge sees in a reduced crop
is only as fine as the crop: a thing seen in one crop pixel is found again within half
a crop pixel, which is more than one source pixel once the crop is reduced more than
twice. The sentence above is read as the first of these.

### A11. Proof two: the scripted squint run

**Depends on:** A7, A8, A10. **Reads:** squint `crates/squint-core/src/run.rs`, for the shape
of the loop and how returned crops become readable in later turns.

- [x] `whizbang-squint` gains the action `check`: a judge loop against the fake judge. A run
      freezes its list of tools and the images it may read. A crop returned by a tool in this
      run becomes readable in later turns of this run.
- [x] Evidence a tool returns is kept as a diagnostic and can be read back by the same run. It
      cannot become an output of the action or be read by another run.
- [x] The loop is the smallest that proves the contract. Squint's rules, checklist and
      submission format are not ported here.

**As built** (ADR 0009). Of the second box: a tool is called through the executor, so
its crop is the committed output of the tool's own run as well as a diagnostic of the
`check` run. Another run's judge cannot read it; a caller of the executor that is no
judge takes it by its identity, as any artifact (`docs/carry-over.md`). The task came to
about twice the size of one and is **two pull requests**: first what the contract
gained, in the core and the test kit (a run is handed the backend its call resolved;
each request and each exchange with a judge is recorded; a turn and an exchange as data;
a verdict; the calls a run froze); then `view` as an action, `check`, and the
transcripts. The boxes are ticked by the second.

**Done when** these scripted transcripts behave as stated:

| Transcript | Expected |
|---|---|
| Look, look closer at part of the first crop, decide | Completed; two `view` calls recorded; both crops kept as evidence; a verdict |
| Two tool calls in one turn, with opaque provider content between them | Both dispatched in order; the opaque content returned to the judge unchanged |
| A tool call that the tool rejects | The error goes back to the judge as a tool result; the run continues |
| A response cut off by the token limit, then a retry with a larger limit | The cut-off turn is excluded and recorded; the run completes |
| A call to a tool not on the list | Refused at dispatch; the refusal is recorded |
| A call to `grow` | Refused the same way |
| `view` on an image the run was not given | Refused |
| `view` naming a second image the run was not given | Refused |
| `view` on a guessed artifact id | Refused |
| A failure on turn two | A failed run whose turn-one evidence is still readable |

And: changing the tool list after admission has no effect on the run. (As built, and
amended at the exit check, task A12: the list is frozen at the action's first step, not
at admission. The names a run asks for are an argument, so they are fixed at admission
and are in its record; the tools on offer are read once, when the action starts, and a
tool withdrawn after that does not leave a run that has started. A tool withdrawn
between a run's admission and its action's first step makes the run invalid: it is
never run with a tool that was not on offer when it started. ADR 0009, "`check`, and
what a judge may do";
`check::changing_the_tool_list_after_admission_has_no_effect_on_the_run`.) And the
publication tests of A7, named for this action: a cancelled `check` has no verdict and keeps its
evidence; a completed `check` returns only after its verdict and status are durable, and
both are present after a reopen. After a failure or an accepted cancellation, the store is
closed and reopened: the run has its terminal status, no verdict, no committed output and no
completed status, and every diagnostic written before it ended is readable.
**Size:** medium.

### A12. Exit check

- [x] Each clause of the checkpoint 1A row in `SPEC.md` section 16 is matched to a named test
      in the note.
- [x] The run records written by `grow` and by `check` (completed, failed and cancelled) are
      each checked against the list in `SPEC.md` section 3: steps, timings, the fake
      capabilities' identities, the fake ledger's amounts, and for `check` every turn kept
      whole.
- [x] `just check` is inside its budget; the timing table is in the note.
- [x] The generated reference lists `grow` and `check` and is current.
- [x] `docs/plans/m1a-note.md`: what shipped, what the two proofs changed about the contract,
      the timing table, what moved to the carry-over list.
- [x] `port-module` is corrected from its uses in A9 and A10, and now includes writing the
      tool record.
- [ ] Tag `m1a`. (The orchestrator's, on the merge of the pull request that ticks the
      boxes above; a later pull request ticks this one.)

**Built in two pull requests:** first the leftovers that change code, each from a row of
`docs/carry-over.md` or a reviewer's note, built with a test or given a new owner with
its reason; then the exit check itself, with the note.

## Review rounds

Recorded as `AGENTS.md`, "Working with the owner", requires. This plan and the plan for
milestone 0 (`m0-freeze-and-skeleton.md`) were reviewed together, in three rounds on
2026-11-11, by the independent reviewing model. The counts are for the two plans together,
per the orchestrator's record; the drafts between the rounds are not kept in this repo.

| Round | Blockers | Majors |
|---|---|---|
| 1 | 3 | 17 |
| 2 | 2 | 7 |
| 3 | 0 | 6 |

Every blocker and major finding was addressed in the draft that followed its round; the last
six majors are addressed in this plan's draft 6 and that plan's draft 4. The owner approved
both plans after the third round.

## Risks

| Risk | Effect | What we do |
|---|---|---|
| The identity model does not fit rascal's wrapped mappings or squint's chains of crops | Everything above it is wrong | A1 is first, with a table of expected results; A4, A9, A10 and A11 each use it on real code |
| The contract is shaped by two small proofs | Rework in milestones 2 and 3 | The descriptor fixtures for `seam` and `remove_prompt` in A5, and the judge transcripts in A11, are taken from the real recipes and the real provider interface |
| The durability protocol is subtly wrong | Lost or half-visible results | A2 states the publication point and tests a crash on each side of it |
| The store grows into a project of its own | The checkpoint overruns | Files only. Anything past the listed tests goes to the carry-over list |
| Porting `grow` or `view` drags in their crates | A9 or A10 overruns | The closure of each is stated in its task; fixtures are built directly |
| The fake judge is simpler than a real provider | The judge interface is wrong for milestone 2 | A8's interface is read from squint's provider interface, and A11's transcripts include opaque content, several calls per turn, tool errors and a cut-off response |

## Needs the owner

Nothing. No spend and no irreversible step.
