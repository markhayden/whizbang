# Whizbang

Draft 15, 2026-11-11. Approved by the owner; changes since then follow the review rule under
"Process" in section 14. This is the living description of how whizbang works; it is edited as
things change and is not a history.

Where this spec says a detail is "decided at" a checkpoint, the constraint written here binds
that decision, and the decision is recorded as an ADR before code depends on it.

## 1. Objective

Whizbang is one command-line toolkit for working on images with models. It replaces four
separate projects: **rascal** (finds things and edits them: masks, removals, heals), **squint**
(judges an image against rules and returns a verdict with evidence), **wafflery** (a local
browser playground for both) and **table-manners** (the contract between them).

They are merged because keeping them apart cost more than it gave:

- Each tool had its own Python pack, model cache and model plumbing, so the same segmenter
  would have been installed and loaded twice.
- The tools reached each other by starting each other as separate programs, which caused bugs
  that only exist at that boundary (lost errors, environment variables that could not pass).
- A tool that both needed, such as segmentation by name, had to be copied or left out.

**Users and doors.** Everything is single-owner today; there are no outside users.

| Door | Who uses it |
|---|---|
| Command line (`whizbang …`) | A person, scripts |
| MCP server over stdio (`whizbang serve`) | A local agent |
| The same server over HTTP (`whizbang serve --http`) | A hosted app that uses whizbang as its engine |
| Playground (`whizbang play`) | A person trying tools in a browser |

There is no public Rust library. Only the `whizbang` binary is released.

**Top-level tools.** `rascal` and `squint` are the first two. Adding another must not require
changes to existing tools or to the doors. Pixel, an image generation agent that uses both to
create, verify and edit images, is the known next one and is out of scope here.

### User stories the design must serve

1. **A hosted image editor.** An app lets people upload an image, name or click a thing to
   remove, and download the result. The app server runs `whizbang serve --http` with no GPU and
   no Python. Segmentation runs on SAM 3 on a GPU host; the fill runs on Bria through fal. The
   app calls two or three actions and writes no model code.
2. **Trying a perception idea.** The owner writes a new tool as one Python file, tries it on an
   image pair in seconds, scores it alone on a labelled set, and only then pays to test it with
   a judge model (section 8).
3. **An agent checking its own work.** An agent generates or edits an image, asks squint
   whether it meets the job, and asks rascal to repair what failed, through one server with one
   budget.

## 2. Principles

1. **Nothing inherited is settled.** Every design choice carried from the old repos is either
   re-justified here or listed for evaluation (section 17). The old repos are reference, never
   authority.
2. **Clean beats complete.** Losing a feature is acceptable if the result is a sharper
   codebase. What is kept, deferred and dropped is listed in section 7, and nothing else is
   promised.
3. **Squint supplies the details; the model concludes.** Tools give a judge finished evidence
   about each thing the job names, not a pile of pointers to sort through.
4. **Any model, anywhere, by design.** Every model is reached through a backend that may be
   local, self-hosted or a paid service. No capability is designed in a way that works only
   with a paid service. Which combinations ship when is stated in section 5.
5. **Every tool earns its place.** Anything that makes a claim about quality climbs the
   ladder, inherited or new (section 8).
6. **Stable in Rust, experimental in Python.** New perception and measurement tools start in
   Python, where trying an idea is cheap. A tool moves to Rust only once it is proven and speed
   or distribution calls for it. Nothing is hand-written in Rust that a mature library does
   well.
7. **Build the simple version first.** Schedulers, alternative runtimes and transfer
   optimisations are added when a measurement calls for them.
8. **Fast checks** (section 13).

## 3. Architecture

```
       command line        MCP (stdio, HTTP)        playground (browser)
             \                    |                     /  host API
              +---------------- serve -----------------+
                                  |   executor (injected)
       binary: builds the registry and the executor
                                  |
            rascal            squint            (pixel, later)
                \               |               /
                 backends: implement segment, fill, judge
                                  |
       core: identities, artifacts, store, config, spend, the execution
             contract, the capability interfaces, the executor interface
```

### Crates and their edges

The rules matter, not the count; a crate may start as a module where the compiler boundary is
not yet needed.

| Crate | Holds | May depend on |
|---|---|---|
| `whizbang-core` | Identities, artifacts, the store, config, spend; the execution contract; the request and response interfaces of each capability; the executor interface | nothing in the workspace |
| `whizbang-backends` | Implementations of the capability interfaces: backend kinds, the pack client, provider adapters | core |
| `whizbang-rascal` | Rascal's operations and tasks | core |
| `whizbang-squint` | Squint's rules, checklist, judge loop and Rust tools | core |
| `whizbang-eval` | Sets, references, runs, statistics, gates | core |
| `whizbang-serve` | The MCP server (stdio and HTTP) and the playground host | core |
| `whizbang` | The binary: argument parsing; builds the tool registry, the backends and the executor, and injects them | all of the above |

- The core names no backend implementation, no tool and no transport.
- Tools, `eval` and `serve` receive an executor and capability handles from the binary. That is
  how `eval` runs tools, how the playground host runs tools, and how one tool calls another.
- Tools never depend on each other, on `serve` or on `eval`. MCP types stay inside `serve`.
- `whizbang-testkit` (fakes) is a development dependency only. It depends on
  `whizbang-core`, and any crate, the core included, may take it as a development dependency
  and in no other way.

The edges are listed in one file, `scripts/crate-graph.txt`, and checked by `just check`. A
path dependency on a crate outside the workspace fails the check.

### The execution contract

Defined in the core, independent of any door. It is proven at checkpoint 1A with one rascal
operation and one scripted multi-turn squint run before either tool is ported.

- **Action descriptor.** Static and deterministic: name, kind, typed arguments, typed artifact
  ports (required, optional, variadic), outputs with their bounds, the capabilities and
  features the action needs, and whether it may run concurrently.
- **Prepare.** Resolves a call's artifact bindings and returns readiness for these exact
  arguments and a bounded estimate. It has no side effects and never spends. Its result is
  bound to the digests of the inputs, the config and the model identities it saw.
- **Admit.** Revalidates a prepared call against those digests, reserves its spend, and assigns
  a run. A stale preparation is refused. A refusal happens before any run exists and is not an
  outcome.
- **Execute.** Asynchronous. The context carries a cancellation signal, a deadline, an event
  sink for progress, a spend allowance, the store, capability handles and the executor. Work
  that blocks runs on a dedicated executor.
- **Outcome.** Completed, cancelled, error or invalid, each with its run record.
- **Request identity.** A caller may give a request id. Ids are scoped to the store and bound
  to a fingerprint of the submitted request. Admission records the id, fingerprint, run and
  reservation together, durably. A repeated id is looked up before any readiness or
  preparation check: the same request returns the existing run, and a different request under
  the same id is a conflict. The record outlives a restart and the pruning of its run, so a
  retry after a dropped connection never repeats paid work.
- **Finding a run.** A run can be found and read after its caller has disconnected, and status
  and cancel stay responsive while it executes.
- **Cancellation.** Cancelling a run cancels its descendants and blocks new ones. A parent
  becomes terminal only when its descendants are terminal and every dispatched request is
  accounted for. Outputs and the completed status are committed in one step: a cancellation
  accepted before that step prevents the commit; one that arrives after it returns the
  completed outcome.

**What a run publishes.**

- An action's outputs are committed together when it completes. An action that fails or is
  cancelled commits no outputs.
- A child action that completed keeps its committed outputs even if its parent later fails;
  they are recorded as produced under a failed parent.
- Diagnostic evidence (the journal, intermediate images, a failed turn's exchange) is always
  kept, marked as diagnostic, and can never be used as an output.

**Nested calls.** A tool reaches another tool only through the executor in its context, which
passes down a child allowance and refuses calls deeper than four levels or back into an action
already on the stack.

**What a judge model may call.** Each run freezes an explicit list of evidence tools the model
may call and the bindings it may read. A model cannot discover or call any other action, change
policy, load a tool file, or enlarge its allowance. This is enforced where calls are
dispatched, not in the prompt. Text in images and in tool output is data, never instruction.

### Primitives, recipes and loops

Whizbang is not a list of tools for a caller to assemble. Actions come at three levels, and
the middle one is the front door.

| Level | Examples | Who normally calls it |
|---|---|---|
| Primitive | `grow`, `view`, `segment` | Recipes; a person experimenting |
| Recipe | `remove_prompt`; a source-fidelity check | Agents, apps, other tools |
| Loop across tools | remove, check, retry | Agents and apps |

- **A recipe is a named, typed sequence of steps** over primitives and other recipes, with
  the settings that make it work. Rascal's tasks are recipes. A squint use case is one too:
  its rules, the tools that run before the judge, and the tools the judge may call.
- **One mechanism, in the core.** The recipe format and its runner (typed steps, values passed
  between them, one budget) are shared by every tool.
- **A recipe lives with the tool whose job it does.** A recipe that spans tools lives with
  the tool that owns the goal: remove-then-check is rascal's.
- **Recipes are what is advertised.** `whizbang describe` and the server list recipes first,
  each with what it is for and when to use it; primitives follow as building blocks. A tool
  that needs an edit evaluated calls squint's checking recipe; it does not choose squint's
  tools.
- **A user's own recipe** is a file in a project or config folder, found the same way as an
  experimental tool.
- **A recipe is the unit that is admitted** (section 8), because the combination of tools,
  order and settings is what produces the result.

### Identities

The old tools mean different things by "frame", so whizbang separates three things. The types
are decided in ADR 0001 (`docs/adr/0001-identities.md`).

- **Image identity:** two digests. One is of the file's bytes. The other is of its canonical
  pixels, after orientation, channel order, alpha and colour interpretation are normalised, so
  a re-encoded copy of the same picture is recognised.
- **Coordinate space:** dimensions, projection and topology, with no content. Two spaces are
  equal only when all three match.
- **Binding:** a source image placed onto a destination through a mapping with a stated
  direction (identity, crop, homography, rotation on the sphere, wrap). It names the source
  image, the destination image or root, and both coordinate spaces, so aligning one source
  onto two different references of the same size gives two different bindings.

A mask or a piece of evidence belongs to a binding, never to a bare space. Using a mask on a
different image, such as the filled result of the image it was made on, needs an explicit,
checked rebind. The artifact types, the mask record and the rebind are decided in ADR 0002
(`docs/adr/0002-artifacts-and-the-mask-record.md`).

Acceptance at 1A covers: two different images of the same size; one source aligned onto two
different references of the same size; a re-encoded image; a flat
image and a panorama of the same dimensions; crops; homographies, including a singular one;
panorama rotations; rascal's wrapped cut and put mappings; and a mask reused on a composite
derived from its image.

### Storage

One owner for everything durable: the core's store. It holds runs, artifacts, evidence and the
spend ledger. Whizbang starts with an empty store; nothing is migrated from the old tools.

- **Retention.** Everything is kept until deleted. `whizbang store list|delete|prune` remove
  runs and what only they reference, with the caches and exports this store manages for them.
  Copies that left the store are out of its reach. Spend records and request-id records are
  never deleted by these commands.
- **Permissions.** Stored images, evidence and journals are readable by the owner only.
- **One writer.** One process at a time may write a store; a second is refused. While
  `whizbang play` or `whizbang serve` holds a store, command-line actions that write to it are
  refused until that process exits. Reads are always allowed.

Invariants kept whatever the storage format becomes:

- Protected pixels never change.
- A rendering cannot be used where a mask is required.
- Publication is atomic and survives a crash.
- Every artifact records what produced it.
- A mask can be exported from one store and imported into another without loss.

### What every run records

Every run, whatever its outcome, writes one record with a fixed, versioned shape. It is the
single source for debugging, for audit and for metrics, so nothing below is optional.

| Group | Always stored |
|---|---|
| Identity | Run id; request id; parent run, if nested; the whizbang version |
| What was asked | The action; its exact arguments; the resolved settings, with the digest of the config they came from |
| Inputs and outputs | The identity of every input binding and artifact; a reference to every committed output |
| Outcome | Completed, cancelled, error or invalid; for an error, its type and message; for a refusal inside a run, what was refused and why |
| Time | Start and end; the duration of each step, tool call and capability request |
| Models | For every capability request: the backend, the model and its version or reported identity, the device, and whether the answer came from a cache |
| Spend | For every paid request: the amount reserved, the amount settled, and anything unresolved |
| Egress | Every recipient that was sent data, and the class of data |
| Steps | Every recipe step, tool call and nested call, in order, with its arguments and a reference to its result |
| Judge runs | Every turn: what was sent and received, kept whole; token usage; repairs; excluded turns; the checklist and the verdict |
| Diagnostics | Intermediate images, a failed turn's exchange, a worker's error output, and progress events |

Diagnostics are kept for failed and cancelled runs exactly as for completed ones. Secrets are
redacted before anything is written. Records and diagnostics are removed only by the store's
delete and prune commands.

### Metrics from ordinary runs

Because every run writes the same record, a metric is a query over the store. It needs no
labels and is separate from evaluation (section 8), which scores tools against known answers.

- `whizbang runs list|show` finds and reads runs.
- `whizbang runs stats` aggregates over a filter (an action, a recipe, a tool, a model, a
  backend, a period): how many runs; outcome rates; for squint, verdict and unsure rates;
  retries and repairs; time as median and 95th percentile; cost; cache hits. For example,
  how one tool behaved across its last 100 runs.
- `whizbang runs export` writes the matching records as one JSON object per line, so analysis
  and dashboards can live outside whizbang.
- The playground shows the same figures in milestone 5.
- Evaluation results are stored as runs too, with their labels kept apart, so one export
  format serves both.

### The interface

One definition of the actions serves all four doors. What it keeps, replaces and drops from
table-manners is listed in section 6. It is versioned with whizbang.

### The playground

The browser calls a host API served by whizbang; the host runs tools through the executor
inside its own process. The only other process is the optional Python worker. Chrome is the
only supported browser.

## 4. Backends

A **capability** is one kind of model work. Each has its own request and response interface,
defined in the core; no shared execution shape is forced on them.

| Capability | Request and response |
|---|---|
| segment | An image and a batch of requests, each a phrase, a point or a box; returns instances with masks, boxes and scores per request. Image features are computed once per batch |
| fill | An image, one support plane (the area to paint) and an optional prompt; returns pixels of the same dimensions |
| judge | A conversation with images, tool definitions and provider-opaque content, and its controls; returns a turn, its usage, or a typed error |

- **Fill and preservation.** A fill backend is not trusted to leave pixels alone. Rascal
  composites the returned pixels itself and guarantees that pixels outside the support, and
  protected pixels, are unchanged. The support plane's polarity and range, how a mask's core,
  blend and protect planes become it, and the colour and alpha rules of the returned pixels
  are fixed in the capability ADR at checkpoint 1A, with conformance cases.
- **Labelling every pixel** (semantic segmentation) is not part of the segment capability at
  first; it is on the carry-over list.

What every backend shares is the policy around the call:

- **Config:** one shape for choosing a backend per capability.
- **Features.** Each adapter declares the features it supports (for segment: phrase, point,
  box). An action declares what it needs.
- **Resolution** is deterministic: an explicit per-call choice, then the config default. A
  backend lacking a required feature is an error. A model is never substituted silently.
- **Credentials:** named in config and given only to the backend that needs them.
- **Spend, readiness, privacy:** sections 6 and 10.

```toml
[backends.segment]
kind = "remote"
url = "https://example-host/whizbang-sam3"
token_env = "SEGMENT_TOKEN"
model = "sam3"

[backends.fill]
kind = "service"
provider = "fal"
model = "bria-erase"
key_env = "FAL_KEY"
```

### The Python pack

Local and self-hosted backends for segment and fill run the same Python package.

**Protocol.** One versioned logical protocol, decided at checkpoint 1C within these
constraints:

- A handshake that states the protocol version and refuses a mismatch.
- Framed messages: request, result, event, cancel, error, each with a request id.
- Payloads state dimensions, layout, data type, colour and alpha rules, and the binding they
  belong to. Every message and payload has a byte bound.
- Nothing in it is a file path.
- One codec for the local worker and one for HTTP, each the simplest that meets the bounds.
  Transfer is optimised only after profiling shows it matters.

**The worker.** One long-lived process per whizbang process.

- **One model at a time.** At first the worker holds one resident model and runs one model
  call at a time. A request for a different model waits for the current call, then the old
  model is unloaded. Holding several models and running calls concurrently are deferred.
- **A bounded queue.** When it is full, a new request is refused, not held.
- **Tools run beside the model, not in its place.** An experimental tool runs in its own
  executor inside the worker. When it asks for a capability, the request goes back to
  whizbang, which applies policy, allowance and cancellation checks before dispatching it, so
  a tool never waits behind itself and never bypasses the rules.
- **Cancellation is cooperative.** A cancelled call is asked to stop; if it has not stopped by
  its deadline the worker is restarted.
- **A restart fails everything in flight:** the current call, queued requests and running
  tools each end with a clear error. Nothing that had been dispatched is replayed.
- **Removal drains.** Removing a model refuses new requests for it, cancels requests queued
  for it, and waits for the current call. If the wait times out, the removal answers "busy"
  and the model accepts requests again.
- **Device chosen at runtime:** GPU when available, CPU as the fallback, recorded in the run.

No worker is needed when a run uses neither Python tools nor local Python capabilities.

### Segmenters

- **A model is one adapter and one catalogue entry** (files, hashes, licence, size, features),
  with its conformance fixtures. Adding one requires no edit to any existing tool's logic.
- **The default is a setting.** It is SAM 3.
- **SAM 3** ships with phrase, point and box requests. Point and box are new work: the old
  export is text-only.
- **Tiny** (OWLv2 with SAM 2.1 Tiny) ships with phrase requests, and with point and box if its
  SAM stage exposes them cleanly; otherwise it declares them unsupported. It is built
  differently from SAM 3 and proves the interface is not shaped around one model.
- **A fake adapter** serves the fast tests.
- **A user's own model:** their own weights for a known adapter (config only), or their own
  adapter, a Python file named in config that implements the same interface as the built-in
  ones. A conformance test is provided. A user's adapter is trusted code (section 10).

### Experimental tools

A perception tool is a Python file that declares a tool description (name, arguments, output
bounds, the bindings it reads, the kind of evidence it returns) and one function. The function
receives a context and reaches images and capabilities only through it. Its output is
validated against its declared bounds before anything uses it. The exact shape is the first
decision of milestone 2, made before any tool is written.

- **Discovery** is explicit: files listed under `[tools] paths` in config.
- **Live editing.** A run pins the digest of each tool file it uses. An edited file is picked
  up by the next run; nothing is rebuilt or reinstalled.
- **Reach.** An experimental tool is available to `whizbang try` and `whizbang eval tool` at
  once. It is offered to a judge only when a run names it, and it is on by default only after
  it is admitted (section 8).

## 5. What ships when

The architecture supports every cell. These ship first; the rest are on the carry-over list.

| Capability | Local | Self-hosted | Paid service |
|---|---|---|---|
| segment: phrase, point, box | Checkpoint 1D | Milestone 6 | Carry-over |
| fill | Milestone 8 | Milestone 8 | Milestone 3 |
| judge | Milestone 8 | Milestone 8 | Milestone 2 |

Milestone 8 is done when each cell it is assigned here passes that capability's conformance
suite.

**Platforms.**

| Target | Minimum | Local models on |
|---|---|---|
| `aarch64-apple-darwin` | macOS 15 | The Apple silicon GPU |
| `x86_64-unknown-linux-gnu` | Ubuntu 22.04 | NVIDIA with CUDA, or CPU |

## 6. Spend and controls

### Spend

- **A limit is enforced; an estimate is advice.** Each backend states which of its numbers is
  an enforceable upper bound on what a request can cost. A paid request with no enforceable
  bound is refused before dispatch.
- **One ledger per store,** with one writer. An allowance is reset only by the owner's command.
- **Admission is atomic.** A call reserves its bound before any request is sent.
- **Dispatch is recorded first.** A reservation and a dispatch record are written durably
  before a request leaves.
- **Nested calls draw on the parent's reservation;** nothing is reserved twice.
- **Settlement is per attempt.** A request whose cost is unknown is charged at its bound. A
  timeout does not prove a remote host stopped billing, so it is charged at its bound.
- **After a crash,** every request that was dispatched and not settled is charged at its
  bound and is never replayed automatically.
- **Retries belong to the layer that issued the request,** and each is admitted like any other
  attempt.
- **GPU time** on a self-hosted backend has a configured price per second. Its bound must
  come from a limit the host itself enforces, or a fixed maximum charge; a timeout on
  whizbang's side is not a bound.

Tested at checkpoint 1B, with fakes: nested retries, simultaneous calls, cancellation after
dispatch, a crash between dispatch and settlement, and a backend that offers only an estimate.

### Readiness

| Question | Answered by | Network |
|---|---|---|
| What can this build do? | The static description (`whizbang describe`) | None |
| Can this exact call run now? | Prepare | None |
| Is this backend reachable? | An explicit probe (`whizbang backends status --probe`) | Yes |

### Controls from table-manners

| Control | Status |
|---|---|
| Action description; result shape; run record; verdict | Kept |
| Caller request id | Replaced: a repeated id returns the existing run where the old contract refused it; a different request under the same id is a conflict |
| Finding a run after a disconnect | Kept |
| Status and cancel that answer while a run executes; progress heartbeats | Kept |
| Estimates that bind one exact call, expire after an hour, and are refused when stale | Kept |
| Per-call cap and a budget | Kept; one budget per store replaces one per tool session |
| Per-call and per-choice readiness | Kept |
| Model installs: atomic, checked by hash, cancellable, previous install left in place on failure, a second request for the same model attaches to the running install | Kept |
| Removing a model that is in use | Replaced: the model drains, then is removed; a timeout answers "busy" |
| Artifact export and import | Kept |
| Invocation bundles | Deferred to milestone 7 |
| One session and budget per tool | Dropped |
| The ban on accepting another tool's artifact types | Dropped |
| Fixtures copied between repos and pinned by version | Dropped |
| Renders declared as commands for a host to run | Dropped; the playground shows reports itself |

## 7. What is kept, deferred and dropped

This is the full promise. Each kept item has an acceptance case in its milestone's plan.

### Rascal

| Item | Status | Where |
|---|---|---|
| Tasks `mask_rect`, `seam`, `remove`, `remove_prompt`, `repair`, `repair_area`, `protect` | Kept; each model-based one is unverified until it climbs the ladder | Milestone 3, then E |
| All 25 operations: `line`, `rect`, `stroke`, `rasterize`, `tighten`, `grow`, `shrink`, `close`, `plug`, `subtract`, `select`, `invert`, `fan`, `profile`, `feather`, `cut`, `put`, `ramp`, `apply`, `window`, `unwindow`, `render`, `paste`, `measure`, `inspect` | Kept | Milestone 3 |
| `segment`, as an action over the segment capability | Kept | Checkpoint 1D |
| Artifact types `Mask`, `Location`, `Image`, `BandProfile`, `Series`, `Plan` | Kept; formats decided at 1A | Checkpoint 1A |
| Mask record: core, blend and protect planes; prescriptions; provenance | Kept | Checkpoint 1A |
| Recipes: the format and runner move to the core; rascal's tasks are its recipes | Kept | Milestone 3 |
| Mask export and import | Kept | Milestone 3 |
| The lab: recipe and filler matrices, resumable cells, blinded pair review | Kept, on the shared harness | Milestone 3 |
| Bounded check and retry | Kept, as nested calls | Milestone 4 |
| Replay from a journal; invocation bundles | Deferred | Milestone 7 |
| Segmenters other than SAM 3 and Tiny; local fill | Deferred | Milestone 8 |
| Compatibility export of schemas | Dropped | |

### Squint

| Item | Status | Where |
|---|---|---|
| The judge: rules, checklist, verdict | Kept | Milestone 2 |
| Use cases: source fidelity, reference consistency, reality checks, brief compliance, product fidelity | Kept | Milestone 2 |
| Judge providers: Anthropic, Google, OpenAI | Kept | Milestone 2 |
| Rust tools: `view`, `resize`, `align` (the control) | Kept | Milestone 2 |
| Rust tools: `change_map`, `edge_similarity` | Kept, unverified until they climb the ladder | Milestone 2, then E |
| Decision cache | Kept, with the key in section 9 | Milestone 2 |
| Use cases expressed as recipes on the shared mechanism | New | Milestone 3 |
| Evaluation sets and their seals | Kept, moved byte for byte | Milestone 2 |
| Evaluation of evidence tools: matrices, paired statistics, gates | Kept | Milestone 2 |
| Report viewer and export | Kept | Milestone 5 |
| Per-subject comparison on SAM 3 | New, experimental | Milestone 2 |
| `regions`, `classes`, `outline`, and the fixed-class segmenter they use | Deferred: wanted, but each is evaluated on the ladder before it is ported | Milestone E |
| A local or self-hosted judge | Deferred | Milestone 8 |

### Playground

| Item | Status | Where |
|---|---|---|
| Run both tools: typed inputs including lists and artifact bindings; drawing a location; readiness per choice; reviewing the bound cost before a paid run; progress, stop and reconnect | Kept | Milestone 5 |
| Closing the tab does not cancel a run; reopening reconnects to it | Kept | Milestone 5 |
| Before and after view of a source and its result | Kept | Milestone 5 |
| Send a result to another action; rerun into an independent draft; hand one tool's output to the other | Kept | Milestone 5 |
| Model installation from the interface | Kept | Milestone 5 |
| History that survives a restart; evidence that survives a reopen | Kept | Milestone 5 |
| Reports for both tools shown in place and still readable after a restart; squint's exported as a document | Kept | Milestone 5 |
| A saved run gives a usable command for a pipeline, from its saved inputs and resolved settings | Kept | Milestone 5 |
| Saved history owns a snapshot of its inputs; moving or editing the original file does not change it | Kept | Milestone 5 |
| Quit and terminal shutdown cancel and account for running work and release the store | Kept | Milestone 5 |
| Comparisons between saved runs; saved examples; invocation bundles; guided chain replay | Deferred, required | Milestone 7 |
| Tool registration, probing and child-process sessions | Dropped | |

### Doors and releases

| Item | Status | Where |
|---|---|---|
| Command line | Kept | From checkpoint 1A |
| MCP over stdio | Kept | Milestone 2 |
| Release archives for both targets of section 5 | Kept | Milestone 4 |
| MCP over HTTP | New | Milestone 6 |

## 8. Admitting tools

Every tool climbs the ladder as far as the ladder applies to it, whether it is new or came
from the old repos, and every tool has a record of what it showed. The base is built first;
then tools are brought in one at a time and evaluated as they arrive.

| Kind of tool | Example | Rungs that apply |
|---|---|---|
| Pure pixel operation | `grow`, `feather`, `paste` | 1 and 2. Scored alone means: exact against an analytic or reference result, and timed across image sizes |
| The control | `view`, `align` | 1 and 2. For `align`, scored alone means registration error on pairs with a known transform. Every squint comparison is measured against these two, so rungs 3 and 4 cannot apply |
| Any model-based recipe or tool offered to a judge, inherited or new | `remove_prompt`, `change_map`, per-subject comparison | All four |

### Tool records

Every tool and recipe has one record, at `docs/reference/tools/<name>.md`, and the playground
shows the same page. It is built from real runs, so it cannot drift: its examples run as
tests, and its numbers come from a script that is rerun when the tool changes.

| Part | Contents |
|---|---|
| What it does | Purpose, when to use it, when not to, its arguments and outputs |
| Examples | At least two worked examples with their input and output images, and what to look for; one of them a case it handles badly, if one is known |
| Rung 2 results | The measure, the data, the numbers and the date |
| Speed and cost | Time at several image sizes on the reference Mac; cost per call where it spends |
| Alternatives | Other methods that were tried for the same job, each run on the same examples and data, with its result, speed and cost beside the chosen one, and why the default was chosen |
| Limits | Known failure cases, each with an example |
| Mark and history | Its mark, each rung's result, each gate it faced, and what changed |

A tool is not finished, and cannot be marked unverified or admitted, without its record.

**Comparing methods.** A tool may have more than one method for the same job (several ways
to grow a mask; a library call against hand-written code; two segmenters). Each method runs
under the same name with a `method` setting, on the same examples and data, through the same
script. The default is the one that meets the tool's bar at the lowest time and cost, and the
record shows the comparison. A method that loses is removed from the code and kept in the
record.

### Marks

Each tool that the upper rungs apply to carries one mark, shown by `whizbang describe`. A
pixel operation or a control is complete when its record shows rungs 1 and 2.

| Mark | Meaning | Use |
|---|---|---|
| Experimental | Present; nothing shown yet | Runs only when named |
| Unverified | Ported, with its record and rungs 1 and 2 done, but its gate has not been cleared | Usable, and may be a default while nothing admitted replaces it; never described as proven |
| Admitted | Cleared its gate | On by default |

- **Earlier evidence counts.** A tool may be credited with a rung when a recorded result
  already meets that rung's bar: the data, the method and the numbers are cited in the tool's
  record. Rascal's segmenters, each accepted against reference masks, are an example for
  rung 2.
- **A tool that fails its gate** is fixed and tried again, kept as experimental, or dropped.
  The result is recorded either way.
- **Changing an admitted tool's** model, prompt or logic returns it to unverified until it
  clears its gate again.

### The ladder

| Rung | Evidence tool (squint) | Edit tool (rascal) |
|---|---|---|
| 1. Single examples | Evidence looks right on a pair | Mask or edit looks right on an image |
| 2. Scored alone | Its read separates good from bad on a development set | Mask agreement with references; for edits, task success, preservation outside the mask, and visual quality |
| 3. Small end-to-end | Improves a cheap judge on a sample | The finished edit passes on a sample |
| 4. Confirmation | Improves judges across models on fresh, held-out data | Holds on a fresh corpus |

Cost depends on the backend. An offline mode refuses any call that would spend.

### Gates

Before a rung 3 or 4 run starts, its gate is frozen in a file that states:

- the baseline and the primary measure, and exactly how each outcome counts (pass, fail,
  unsure, operational error, missing label);
- the confidence level and the correction for multiple hypotheses;
- the number of independent families required, with resampling by family;
- repetitions and how they are combined;
- the smallest gain worth having and the guardrails, each with its margin;
- limits on cost and latency;
- that too little data gives "inconclusive", never a pass.

Defaults for an evidence tool, to be tuned as results come in:

| Item | Default |
|---|---|
| Primary measure | Balanced accuracy; `unsure` and errors count as neither a catch nor a pass |
| Admission | Observed gain of at least 3 points, and the lower bound of its corrected 95% interval above zero |
| False rejects | The upper bound of the increase is under 3 points |
| Errors | Under 2% of decisions |
| Cost | No more than double per decision |
| Families | At least 20 |

An edit tool has its own gate: task success, preservation outside the mask, and blinded
quality.

### Scoring edits

Squint may steer rascal's retries, or score a finished edit on rung 3. A rung 4 result for an
edit tool rests on labels or blinded human review that squint did not produce.

### Evaluation data

- **`blind-v3` is an exposed regression set.** About 40 of its items were seen with labels
  while it was built, and it has been scored repeatedly. It checks that a port did not change
  behaviour; it never admits a tool.
- **Rung 4 needs fresh data:** new families, disjoint from every set used on rungs 2 and 3,
  unseen by any agent. Each use of a held-out set is recorded with the hypothesis it tested.
- **Sealed sets are never edited.** A correction is a versioned file beside the set. Changed
  jobs or labels make a new set version; nothing is regenerated in place.
- **Nothing public discloses an item-level answer** for an unreleased set: not the backlog, a
  result, a plan or a CI artifact.

## 9. Caches

| Cache | Key | In evaluation |
|---|---|---|
| Inference results (a segmenter's masks) | Canonical pixel digest, the full request, adapter and runtime revision, weights hash, preprocessing version, device class, and the backend's reported identity | Only under a policy frozen in the gate |
| Judge decisions | Known before the run: the complete job (use case, brief, every semantic argument, the bindings in order with their roles and labels); the resolved obligations, checklist and prompt; resolved rules and config; the source and dependency revisions of every enabled tool; capability identities; image preparation; limits; and the harness version | Never read; every repetition is a fresh trial |
| Run resume | The identity of one cell, repetition and attempt | An interrupted attempt continues; a failed attempt is reported, never replaced by a later success |

What a judge actually saw is recorded as provenance, not used as a key.

A remote backend reports the model files it runs by hash. A run records the requested identity
and the reported one. Results from a backend that cannot report its identity are not cached
durably.

## 10. Security and privacy

### Two HTTP surfaces

| | Playground API (milestone 5) | Engine API (milestone 6) |
|---|---|---|
| Listens on | Loopback only | Loopback by default; another address needs a flag |
| Authentication | A token created at launch and handed to the page | A bearer token from the environment or a file |
| Host and Origin | Validated on every request | Origin refused unless listed |
| TLS | Not needed on loopback | Required off loopback, at a reverse proxy; built-in TLS is deferred |
| Local controls (install models, change config) | Allowed | Not exposed |
| File paths from the caller | Inside allowed folders | Refused; images arrive as bytes or by reference to an upload |

Both: limits on request size, image dimensions and concurrent calls; tokens redacted from
logs, results and errors; model and tool text escaped before display; report content shown in
an isolated origin.

Each surface has negative tests at its own milestone: an unauthenticated call, a wrong Host, a
cross-origin request, an oversized image, and for the engine API a path.

### Egress

- **Default deny.** Data leaves only to an approved recipient: an endpoint, the credential or
  account used with it, and any upstream it forwards to. Config lists which classes of data
  each recipient may receive: source images, derived images, prompts, evidence.
- **Every hop is checked:** redirects, upload services and returned download addresses.
  Credentials are never forwarded to another origin.
- **Model downloads and probes** are separate, approved traffic and carry no user data.
- **Before sending,** image metadata is removed.
- **Credentials and tokens are redacted** from logs, events, journals, results and errors.
- **Every run records** where data went.
- **A refusal never falls back** to another remote backend.
- **User adapters and experimental tools are trusted code.** They run with the worker's
  permissions; the egress rules govern calls whizbang itself makes. The guide for writing one
  says so.

Approved recipients at first: fal for fills; Anthropic, Google and OpenAI for judging; one GPU
host for SAM 3 from milestone 6. Adding a recipient is the owner's decision.

### The public repo

Before every push two scans must pass: one for secrets and private data, and the clean-room
scan, which checks against a private word list kept outside every repo. The list is never
committed, quoted or passed to another agent; the scan reports counts only, and fails when
the list is not there.

- **What is scanned.** By hand, the working tree. On a push (the pre-push hook) and before a
  merge, everything that would be published: the files each outgoing commit changed, as they
  were in that commit; each commit's paths and message; the tree of each tip; the names of
  the pushed branches and tags; the message of an annotated tag. A secret added in one
  commit and deleted in the next is found.
- **Nothing is skipped.** Files are searched as bytes, whatever their encoding, their name
  or what `.gitattributes` says of them; a file with NUL bytes is searched again without
  them, which reads UTF-16. A symbolic link is never followed: the text of its target is
  searched.
- **What is outgoing** is every commit reachable from what is pushed and not from a ref the
  destination has, and the destination is asked. A commit being on local `main`, or on a
  remote-tracking ref, is not evidence that it was published. If the destination cannot be
  asked, the scan fails.
- **The secrets scan** looks for provider key shapes, private-key headers, the values of a
  local `.env`, absolute home paths and email addresses. Only an address can be allowed, and
  only by the address itself: never by the rest of its line or by the path of its file. The
  scan prints where a match is, never the text, and not the path when the path holds a
  match.
- **The attribution check** runs with the scans, on every commit and tag message and on
  every author, committer and tagger (section 15). It fails a `Co-Authored-By` line,
  whoever it names; a "Generated with" or "Generated by" line that names an agent, a model
  or a bracketed tool; any trailer that names one (`Signed-off-by`, `Assisted-by`,
  `Co-developed-by`); and a commit or tag made under the name or address of one. The
  `commit-msg` hook applies the same rule. The names are a fixed list of agents and model
  vendors, matched wherever one starts a word. A person whose name is one of those words
  would be refused; the fix is a change to the rule, reviewed like any other, not an
  override.
- **CI** runs the secrets scan and the attribution check over every commit of a pull
  request, and over the commits of a push to `main`, so a commit made on another machine or
  with the hooks bypassed is still checked. A pull request is checked by the base branch's
  copy of the scan, so it cannot pass by changing the scan. Each push to `main` gets its
  own run.

What the scans do not cover:

- **CI is narrower than the local scans.** It does not run the clean-room scan, and it does
  not see `.env` values, branch or tag names, tag messages or taggers, pushes to a branch
  that has no pull request, or the title and body of a pull request. A squash merge turns a
  title and body into a commit on `main`, so merges here are merge commits, never squashes.
- **CI detects after a push; only the local hook prevents.** `git push --no-verify` bypasses
  the hook.
- **A pull request can change the workflow that checks it.** The base branch's copy of the
  scan is used, but the workflow file is the pull request's own; the reviewer reads any
  change under `.github/`.
- **A key split across lines, or encoded** (base64, for one), is not found.
- **A worktree on a branch cut before a change to the hooks** runs that branch's older
  hook.

## 11. Commands

Proposed; exact verbs are settled as each milestone builds them.

```
whizbang rascal <task|operation> …
whizbang squint check <use-case> …
whizbang try <tool> …                  # one tool on one image or pair, no judge
whizbang eval tool <tool> --set <set>  # rung 2
whizbang eval run --gate <file>        # rungs 3 and 4
whizbang models list|get|remove
whizbang backends status [--probe]
whizbang store list|delete|prune
whizbang store import <file> --store <dir> [--create]
                                       # for developers: bring a canonical artifact file
                                       # (ADR 0002) into a store
whizbang runs list|show|stats|export
whizbang spend status|reset
whizbang serve [--http <addr>]
whizbang play
whizbang describe [--json [--pretty]]  # static: identical on every machine; --json is
                                       # one line, and --pretty indents it
whizbang setup                         # install the Python pack
```

Developer commands:

```
just setup              # install the git hooks; check the pinned tools
just check              # fast tier: format check, lint, the crate graph, licences and
                        # sources, the script tests, the reference against a fresh
                        # copy, then the fast tests with seconds per suite and the
                        # 180-second limit
just test-scripts-slow  # the script tests with their opt-in parts (about 40 seconds)
just test-models        # model tier
just test-slow          # slow tier
just docs               # regenerate the reference (docs/reference/) from
                        # `whizbang describe --json --pretty`
just scan               # the secrets scan and the clean-room scan, over the working tree
just scan-range <base> <head>
                        # the two scans and the attribution check over every commit in
                        # base..head and the tree of head
just scan-range-secrets <base> <head>
                        # the same without the clean-room scan: what CI runs over the
                        # commits of a pull request, there with the base branch's copy
                        # of the scan
just ci                 # what a pull request runs: check, advisories, a smoke of the
                        # release binary, the secrets scan
```

`just setup`, `check`, `test-scripts-slow`, `docs`, `scan`, `scan-range`,
`scan-range-secrets` and `ci` exist; the others arrive with the milestone that needs them.
`just check` is made of recipes that also run alone: `fmt-check`, `clippy`, `graph`, `deny`,
`test-scripts`, `docs-check`, `timing`. CI also runs `just test-timeout` on one runner: a test
that sleeps 31 seconds must fail by timeout.

## 12. Project structure

```
crates/                 the crates of section 3, and whizbang-testkit
pack/                   the Python package: adapters, catalogue, worker, experimental tools
ui/                     the playground interface
evalsets/               set manifests and public labels (images: github.com/markhayden/whizbang-evalsets)
docs/
  adr/                  decisions that are costly to reverse, one new series from 0001
  baseline/             what the old harness did: the package's manifest, the transcript
                        format, the audit's counts (the package itself is owner-only)
  guides/               one page per door and per "how to add" task
  reference/            generated from the static description; never edited by hand
  results/              evaluation results: per study, a page and the numbers file its
                        tables are written from
  plans/                one short plan per milestone
  carry-over.md         the only backlog
  sources.toml          the old repos' final commits, fixed before porting starts
.claude/                skills, subagents and permission settings (versioned)
scripts/                scan.sh, heavy.sh, timing.sh, setup.sh with tool-versions.txt,
                        check-crate-graph.sh with crate-graph.txt, docs.sh, hooks/,
                        baseline/ (convert.py, audit.py and tables.py: Python 3.9 or
                        later, standard library only), and tests/ (the tests of the
                        scripts); later
                        worktree.sh and dev.sh
.github/workflows/      CI
SPEC.md                 this file
AGENTS.md               rules for any agent; CLAUDE.md imports it
```

The images repo is renamed from `wafflery-evalsets`. Sealed sets pin the old address, which
GitHub redirects; the old name must never be reused.

## 13. Testing

| Tier | Contains | Runs |
|---|---|---|
| Fast | Format check and lint; Rust and Python logic against fakes; the real pack speaking the real protocol with fake models; UI type checks and unit tests; a smoke of the release binary | Every change |
| Model | Real models through the pack | When the pack or a model-using tool changes; before a release |
| Slow | Chrome end to end; a live call to each provider; install from a release archive | Chrome on every pull request, from milestone 5, when the playground exists; the rest before a release |

**Budgets.**

- Fast tier: under 3 minutes of test time on the owner's Mac (Apple silicon, 64 GB), with a
  warm build cache and compilation not counted.
- Pull-request CI: under 10 minutes on GitHub's hosted `macos-15` and `ubuntu-22.04` runners,
  compilation included, with caches.

These are targets, not measurements: squint's Rust tests alone took about 3 minutes a pass.
Each milestone's note includes a timing report by suite. When a budget is exceeded, coverage
is reviewed first; a test is then sped up, moved to another tier, or removed if what it guards
is covered elsewhere or dropped. Elapsed time alone never removes required behaviour, and a
budget is not raised without the owner.

**Rules.**

- Fakes run through the production execution path, so one build configuration is tested.
- Tests wait on events and controlled clocks, never on sleeps. Tests that must time real
  processes are few and isolated.
- Every test has a timeout well under its tier's budget.
- Heavy checks run one at a time on a machine (`scripts/heavy.sh`).
- A test is ported only if it guards behaviour whizbang keeps.
- Rust tests live in `crates/<crate>/tests/it/`; Python tests in `pack/tests/`.

## 14. Agents and documentation

Agents write most of this code. These pieces exist from milestone 0.

**`AGENTS.md`** is the one instruction file for any agent, whatever its vendor. It starts from
the rules of the old coordination repo, cut to what one repo needs: where agents may work, the
boundaries in section 15, secrets and private data, the definition of done, how to work with
the owner, and known gotchas.

**Precedence.** When documents disagree: this spec, then the milestone plan, then an ADR, then
the carry-over list. The old repos' documents and code are never authority. An agent that finds
a contradiction it cannot settle from these stops and asks.

**Sources.** `docs/sources.toml` fixes each old repo's final commit before porting starts.
Every porting commit names the path it came from at that commit.

**Workflows** live in `.claude/skills/` and are listed in `AGENTS.md` so any agent can follow
them as plain checklists.

| Skill | Exists from | What it standardises |
|---|---|---|
| `kickoff` | Milestone 0 | Starting a task: read the rules, make a worktree, build with tests, hand back |
| `port-module` | Milestone 0 | What to read, reshape or drop; tests to bring; the commit message |
| `review-merge` | Milestone 0 | Read the diff, run both scans, merge, check, clean up |
| `add-adapter` | Checkpoint 1D | A model: adapter, catalogue entry, conformance fixtures |
| `add-tool` | Milestone 2 | A tool, and the route it takes in section 8 |
| `run-eval` | Milestone 2 | Freezing a gate, running, writing up |
| `release` | Milestone 4 | A version release |
| `add-top-level-tool` | When Pixel starts | A tool beside rascal and squint |

The three milestone 0 skills are written before first use and corrected after it.

**Subagents** in `.claude/agents/`: a builder, which does one task in a worktree, pushes its
branch, opens the pull request and never merges; and a reviewer, which reads a branch against
this spec and never merges. The session the owner is talking to is the orchestrator. Only the
orchestrator merges.

**Definition of done.** The tiers that apply to the change pass, with their output attached.
The spec, the reference and the carry-over list are updated when the change affects them;
anything deferred goes on the carry-over list with a milestone.

**Documentation that cannot drift.** `whizbang describe` is static: it reports what the build
can do and contains no installed models, credentials or reachability. The reference is
generated from it, and CI fails if the committed copy is stale. The generator prints names
and purposes as they are, so it refuses any that could be read as markup: an action's name is
a lower-case letter followed by lower-case letters, digits, `_` or `-` (64 characters at
most), and its purpose is one plain line of at most 200 characters (the exact characters
are listed at the top of `scripts/docs.sh`). Guide examples are marked with
the tier that runs them and use offline fixtures in the fast tier; a documentation check never
spends.

**GitHub Actions.**

| Workflow | Runs | Contains |
|---|---|---|
| `ci` | Every pull request | Fast tier on both targets; Chrome end to end, from milestone 5, when the playground exists; reference check; secrets scan; the secrets scan and attribution check over every commit |
| `prerelease` | On a release candidate tag | Model tier; a live call to each provider; install from the built archive on both targets. A release is refused unless this passed on the same commit |
| `release` | On a version tag | Binaries for both targets and the pack |

**Process.** One short plan per milestone in `docs/plans/`. A note and a timing report at each
checkpoint; a tag where the result is an installable state worth returning to. ADRs only for
decisions that are costly to reverse. No mirrored task files.

**Review before building.** A new milestone plan, or a change to this spec that changes what
will be built (scope, architecture, what a milestone holds), gets at least two review rounds by
the independent reviewing model: a model from a different provider than the one that wrote the
text, chosen by the owner, and run through its command-line tool in a read-only mode. Every
blocker and major finding is fixed, and the outcome of each round is recorded in the plan.
Building then starts without waiting for the owner, who is told what was decided. This replaces
the owner's approval of each plan and nothing else. It replaces nothing in section 15: what
that section keeps for the owner, what it says to ask first and what it says never to do all
stand as written, and a change to section 15 itself is the owner's. An edit to this spec that
changes no scope, architecture, boundary or milestone content, a matter of wording or accuracy
that brings it in line with what a reviewed pull request did, is reviewed with that pull
request; any other change gets the two rounds first.

## 15. Boundaries

**Always**
- Run `just check` before a commit, and `just check` and `just scan` before a push. The
  pre-push hook runs the scans again over what the push publishes.
- Name the source path and commit in every porting commit.
- Record the backend, model and version in every result.

**Agents may**
- Do everything in this repo: commit, push, open and merge pull requests, tag, cut version
  releases and publish crates, provided CI and both scans pass first.
- Spend up to 100 US dollars per milestone on approved recipients.

**Only the owner**
- Approve spend beyond that, a new recipient for data, or a reset of the spend allowance.

**Ask first**
- Adding a platform, a door or a top-level tool.
- Using a held-out evaluation set.

**Never**
- Add `Co-Authored-By` or any agent or model attribution to a commit, a pull request, code or
  docs. A `Co-Authored-By` line that names a person is refused too: commits in this repo
  carry none.
- Commit secrets, the clean-room word list, or owner-only material such as answers to
  unreleased evaluation sets.
- Edit a sealed evaluation set, its labels or the rules files its seal covers.
- View the items of a held-out set while it is held out.
- Change the old repos beyond what the freeze allows, or delete one.

## 16. Migration

**Freeze.** In rascal, squint, wafflery and table-manners, only what is needed to tag is
finished. Everything else is recorded in `docs/carry-over.md`. Each repo then gets the tag
`final-before-whizbang`, and its commit goes into `docs/sources.toml`.

**Port, not import.** Whizbang starts with its own history. Code is brought in module by
module, reshaped to fit this design, with the tests that guard kept behaviour.

**Old repos** are archived publicly, read-only, with a pointer here, once milestone 5 ships
and every kept item in section 7 has its replacement. They are never deleted. The coordination
repo is retired at milestone 0.

**Milestones.** Each is cumulative: everything earlier still passes.

| # | Milestone | Done when |
|---|---|---|
| 0 | Freeze and skeleton | In order: the old repos' remaining source changes land; each is tagged and pinned in `docs/sources.toml`; the baseline package (below) is assembled from the tagged squint and validated; the 2026-11-11 study is written up from the package in `docs/results/`. Also: workspace, CI, `AGENTS.md`, the three first skills and both scans are in place; `just check` and `just scan` pass |
| 1A | Contracts and store | Identity types and the six artifact formats are decided; the identity acceptance cases pass; a fake rascal operation and a scripted multi-turn squint run execute through the executor with atomic outputs and kept diagnostics, and each writes a run record holding every group section 3 lists; request ids (simultaneous duplicates, a conflicting payload, a retry after a config change), prepare, admit and cancellation (racing a commit, with children outstanding) behave as section 3 says; the store reopens and recovers after an interrupted publication, keeps diagnostic references, keeps an artifact two runs share when one is deleted, sets owner-only permissions and refuses a second writer; the fill conventions are fixed |
| 1B | Policy and controls | The spend tests of section 6 pass against fakes; estimates expire and are refused when stale; a model installs, attaches, cancels and drains with fake downloads; egress is denied by default |
| 1C | Local pack | The protocol is decided and its conformance suite passes with fake models; the worker's queue, cancellation, restart and removal rules pass; one runtime is chosen (section 17) and SAM 3 answers a phrase request on the Mac's GPU |
| 1D | Segment coverage | SAM 3 answers phrase, point and box requests; Tiny passes conformance for the features it declares; parity fixtures and a mask-agreement tolerance are recorded; timings are measured |
| 2 | Squint and evidence evaluation | The baseline reproduces (below); each of the five use cases returns a verdict on a representative case, from the command line and over stdio MCP; Anthropic, Google and OpenAI each pass provider conformance; the decision cache misses when the use case, brief, an image's role or order, a rule or a tool changes, and hits for an unchanged job; a new tool file is tried, scored, edited and re-run without a rebuild; `whizbang runs stats` and `runs export` report on a tool across at least 100 recorded runs; per-subject comparison exists as an experimental tool and has been scored on rungs 1 and 2 |
| 3 | Rascal and edit evaluation | Each task and operation in section 7 passes its acceptance case, and each model-based recipe has its ladder record with rungs 1 and 2 scored or credited from earlier evidence, with SAM 3 for masks and Bria through fal for fills; a mask exports and imports; rascal's tasks and squint's use cases run as recipes on the core's runner, and `describe` lists recipes before primitives; a lab round runs a recipe and filler matrix, resumes after an interruption, presents blinded pairs, collects the picks, checks them against the frozen assignment and decodes the outcomes |
| 4 | The loop and a release | Remove with a check and bounded retries runs as nested calls on one budget; the `prerelease` workflow passes and a release archive installs and runs on both targets |
| 5 | Playground | Every kept playground item in section 7 passes its acceptance case in Chrome; the playground API's negative tests pass; old repos archived |
| 6 | HTTP and remote hosting | User story 1 runs: the engine API with its negative tests, SAM 3 on a GPU host with parity inside the 1D tolerance, Bria through fal |
| 7 | Bundles and replay | Invocation bundles and journal replay; saved-run comparisons, saved examples and guided chain replay in the playground |
| 8 | More local models | Each cell section 5 assigns to this milestone passes conformance: fill and judge, each both local and self-hosted; plus any further segmenter the evaluations call for |
| E | Evidence and edit quality | Runs alongside milestones 3 onward, once 2 is done. Every unverified tool and recipe is taken up the ladder, squint's first and the most doubtful first, until each is admitted, experimental or dropped. Per-subject comparison climbs rungs 3 and 4 on a fresh confirmation set; `regions`, `classes` and `outline` are each brought in as experimental tools and scored on rungs 1 and 2, on SAM 3 where that works and on their old segmenter as a catalogue entry where it does not, and those that pass go on; the submission format is redesigned so small models can follow it; job requirements carry colour, count and lettering in a new set version. Each change is admitted through its gate |

Checkpoints 1A to 1D each end with a note. Milestones 2 and 3 need 1A and 1B; they need 1C and
1D only for the parts that use a real model.

**The baseline package** is built in milestone 0 from the frozen squint repo, for the arm its
harness calls C (the `view` and `align` tools, under the projection that arm used). It holds
the commit, matrix, descriptor, prompts, image preparation and config identities, the set seal
and corrections version, the recorded provider exchanges and tool artifacts, and the results.

- **Completeness is checked first.** The old journals can truncate long text. Every exchange
  in the package is audited; an item whose record is incomplete is captured again from the
  tagged squint, and newly captured material is labelled apart from the historical results.
- **The gate is offline and exact.** Replay happens at the core's judge interface, with real
  image preparation and real evidence tools: given the recorded judge turns, whizbang must
  produce the same sequence of judge requests, tool calls and verdicts for every item,
  ignoring only fields the package declares volatile. Each provider's wire format is tested
  separately, by provider conformance.
- **A live rerun is descriptive.** One cheap model's arm is run fresh and its agreement with
  the recorded run is reported beside that model's measured self-disagreement. The old study
  has one repetition for most models, so this is not a pass or fail test.

The conclusions of the 2026-11-11 study are provisional until that write-up exists.

## 17. Inherited choices

| Choice in the old repos | Status |
|---|---|
| Rust core with a Python pack for models | Kept: tested code, compiler checks on agent-written code, one binary |
| Models run through ONNX on CPU only | Replaced. One first runtime is chosen at checkpoint 1C: it runs a SAM 3 phrase request on the Mac's GPU, and there is documented evidence it supports point and box requests and CUDA. Point and box are built and verified at 1D. A comparison of runtimes waits for a measured need |
| SAM 3 export is text-only | Replaced by new work at checkpoint 1D |
| The pack is reached over MCP on stdio | Replaced by the protocol of section 4, decided at 1C |
| Squint's hand-written MCP client and pack server; rascal's use of libraries | One implementation survives, chosen at 1C for the pack and at milestone 2 for the server |
| Squint's alignment code | Decided at port: keep, or replace with a library |
| Rascal's hand-written image operations | Each decided at port |
| Squint's checklist submission format and its 4-turn, 8-call budget | Ported as is, then redesigned through the ladder |
| Offline bundles for installing the pack | Decided at 1C against a plain `uv` install |
| Canonical serialisation and durability rules for masks | The invariants of section 3 are kept; formats decided at 1A |
| Individual crates published to crates.io | Dropped |
| WebKit in browser tests | Dropped |
| Linux on ARM | Dropped |

## 18. Success criteria

1. Every item marked kept in section 7 passes its acceptance case from the one binary.
2. With instrumented fakes, two tools in one process that use the same model cause one load.
3. The segment conformance fixtures give the same instances, inside the tolerance recorded at
   checkpoint 1D, from the local worker and from a remote one.
4. Adding a segmenter requires no edit to any existing tool's logic, shown by adding Tiny after
   SAM 3.
5. Adding a top-level tool requires no change to existing tool implementations or to the
   doors, shown when squint is added after rascal's first action exists.
6. The timing reports show the fast tier and pull-request CI inside their budgets.
7. A new tool file is tried, scored, edited and re-run without a rebuild or reinstall.
8. User story 1 runs end to end.
9. The four old repos are tagged, pinned and archived.

## 19. Open questions

None are open. Decisions the owner made on 2026-11-11 are written into the sections above: all
of rascal's tasks and operations are kept; agents may release and publish once CI and the scans
pass; spend is capped at 100 US dollars per milestone; the gate defaults stand until results
say otherwise; the store starts empty.
