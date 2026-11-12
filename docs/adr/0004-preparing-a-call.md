# 0004. Preparing a call: what a call may hold, what prepare sees, and what it is bound to

Status: accepted
Date: 2026-11-12
Spec: section 3, "The execution contract", "Identities" and "Storage"; section 6,
"Readiness"

## Context

`SPEC.md` section 3 says of prepare: it resolves a call's artifact bindings and returns
readiness for these exact arguments; it has no side effects and never spends; its result is
bound to the digests of the inputs, the config and the model identities it saw. Admission
(task A6 of the plan for checkpoint 1A) revalidates a prepared call against those digests
and refuses a stale one. Section 6 gives prepare the question "can this exact call run
now?", answered without a network.

Three open questions sat on `docs/carry-over.md` for this task:

- **Who vouches for a derivation.** `Mask::rebind` (ADR 0002) checks a derivation's
  statement and trusts that it is true. `Derivation::stated` is open to any caller, so a
  mask can be moved onto an unrelated image of the same size. ADR 0002 left the gate to
  "the executor or the fill composite", and asked this task to decide the way round it: a
  mask made directly on a hand-made binding, by `Mask::from_core` or by a file brought in
  with `whizbang store import`, never meets a derivation at all.
- **The run id's rule.** The store refuses an empty id and one with a control character
  (`Publication::put_in`), and says the task that makes runs may ask for more.
- **Whether a condition on an argument's value is needed** for prepare (ADR 0003 has
  conditions on whether an argument is given, and no others).

What exists to prepare against: the store (ADR 0002's artifacts in it), and nothing else.
There is no config type, no backend, no ledger and no code that executes an action.

## Decision

The code is in `crates/whizbang-core/src/contract/` (`call.rs`, `prepare.rs`, `engine.rs`,
`run.rs`, `context.rs`); the tests that hold it to this record are in
`crates/whizbang-core/tests/it/contract/prepare.rs` and `run.rs`.

### A call

A `Call` holds an action's name, values for arguments by name, and for each input port a
list of artifact identities. It holds nothing else: no artifact, no binding, no
derivation. An input is something the store already holds.

### What prepare does, in order

1. Finds the action in the registry. An unknown name is refused.
2. Checks the arguments against the descriptor (ADR 0003): none that is not declared, every
   required one, each a value its type accepts. An argument left out takes its default; one
   left out with no default has no value.
3. Checks the inputs: no port that is not declared, and at each port as many artifacts as
   it takes.
4. Reads each input from the store as the type its port takes. One the store does not
   hold, and one it holds as another type, is refused; so a rendering is refused where a
   mask is required, here as everywhere.
5. Where a port says its artifact belongs to the image at another port, checks that it
   does: the same canonical pixels in the same space. A mask on another image of the same
   size is refused.
6. Asks, for each capability the call needs (`needs_for`, from the names of the arguments
   given), which model would answer it with the features needed.

It answers with a `PreparedCall`: the action and its version, every argument that has a
value, every input, each need with the model that would answer or none, the output ports
that return something for this call, and readiness. Readiness is `Ready`, or `NotReady`
with each need that nothing answers and why. A call that is not ready is still prepared:
that it cannot run now is the answer, not a failure to answer.

A `Refusal` is what comes back instead when the call is not one the descriptor allows, or
an input is not what its port takes. A refusal is not an outcome: no run exists.

### No side effects, by what prepare is given

Prepare is given two things, and neither can change anything.

- **A read of the store** (`artifact::Contents`: the current generation and the content of
  an entry). It has no way to write. Reading takes no part in the writer lock.
- **`Backends`**, one question: which model would answer this capability with these
  features? It is answered from what is configured. It has no way to send a request to a
  model and none to reserve spend.

The tests hold the first on a file system that records every change: preparing calls that
are ready, not ready and refused creates, writes, syncs, renames, removes and locks
nothing, and leaves every file's bytes as they were. The second is held by the type today.
A count of zero requests on the capabilities themselves, and of zero reservations on the
ledger, needs fakes that tasks A8 and A6 bring; an ignored test in `prepare.rs` names what
they complete.

### Bound to what it saw

A `PreparedCall` carries a fingerprint, kind `prepared/1`, hashed in the encoding ADR 0001
gives every identity. Its fields, in order: the action; its version; the count of
arguments, and each as its name and value; the count of inputs, and each as its port and
identity; the config's digest; the count of needs, and each as its capability, the count
of its features and each feature, then `u` 1 and the backend, model and version of the
model that would answer, or `u` 0 where none would. A value is its kind as `u` (0 a
number, 1 a whole number, 2 a boolean, 3 a text) and then itself: `f`, `u` (the eight
bytes of the two's complement), `u` (0 or 1), or `b`. A test pins one fingerprint.

- An input's identity is the digest of its content (ADR 0002), so the fingerprint is
  bound to the inputs' digests without reading them twice.
- Arguments enter after defaults are applied, so a default given by hand and the same
  default left out are one preparation.
- A need that nothing answers is hashed as such, so ready and not ready are two
  preparations.

Preparing the same call again and comparing fingerprints is how a stale preparation is
found. This record does not decide when that is done: admission is task A6.

The config's digest is given to the executor when it is built. There is no config yet, so
the binary gives the digest of no bytes.

### Who may state a derivation

Code may; no caller can.

- **A call has no place for one.** Nothing that arrives through a door can state a
  derivation or ask for a rebind. A mask given as an input is used on the image at the
  port it belongs to, or refused (step 5).
- **`Derivation::stated` stays open to code,** for one reason: nothing else makes a
  derivation yet. The one thing that should is the fill composite (task A8), which sees
  both pictures and can check that every protected pixel and every pixel outside the
  support is the original. When it returns the derivation with the image it made,
  `stated` closes. Until then a mask that code moved with a false statement names the
  image it was moved onto, and prepare takes it at its word; a test pins this, beside the
  one ADR 0002 names, and both are inverted then (`docs/carry-over.md`).
- **The way round is not a way round.** A mask made directly on an image, by
  `Mask::from_core` or as a file brought in with `whizbang store import`, is accepted on
  that image. It is a statement about that image by whoever made it, as every hand-drawn
  mask is, and its origin says how it was made; it does not claim to be another image's
  mask moved. Whizbang cannot tell a good mask from a bad one by its provenance, and does
  not try. What it guarantees is narrower and holds: a mask is never used on an image
  other than the one it names, and a mask names another image only through a rebind,
  which its origin records.

### The run id

A `RunId` is a lower-case letter or a digit, then lower-case letters, digits, `_` or `-`;
64 characters at most. So it is one path segment, one word on a command line and one part
of an address with nothing to escape, and two ids never differ by letter case alone.

It is enforced where a `RunId` is made (`RunId::parse`), and the executor, the only thing
that makes a run, names it with one. The store keeps its looser check as the floor: it is
the layer below, and its own tests publish under ids this rule refuses. Every `RunId` is
an id the store takes. How an id is minted is decided with the code that makes runs.

### The executor's interface, and the types around execution

`Executor` has two methods. `prepare` is the above. `submit` is the entry point for a call
that is to run, the one place a call comes in whatever door it came through: it is where a
caller's request id will be looked up, before anything else (task A6); then it prepares,
and refuses a call that is not ready. Past that point nothing is built, and a ready call
is refused with `NothingRunsYet`, which the code that runs an action removes.

`Engine` is the implementation: a registry, the backends, a read of the store and the
config's digest. The binary builds it in the function its tests share
(`crates/whizbang/src/wiring.rs`), with no backends.

The types an executing action is handed and hands back exist, and nothing uses them yet:

- `Context`: a cancellation signal, a deadline, an event sink for progress, a spend
  allowance, a read of the store, capability handles, and the executor.
- `SegmentHandle`, `FillHandle`, `JudgeHandle`, and `Capabilities`, which offers each or
  none. The handles are empty and cannot be made: their contents are the capability
  interfaces' (task A8). `Allowance` is empty too, until there is a ledger.
- `Outcome`: completed with its committed outputs, cancelled, error with a type and a
  message, or invalid with a reason; and `Finished`, an outcome with its run's id.

### Conditions on a value are not added

Prepare needs none. Every action of checkpoint 1A (`echo`, `grow`, `view`, `check`) needs
the same capabilities whatever its arguments' values. Rascal's `remove_prompt` is the
known case that does not, and it goes with rascal's recipes to milestone 3
(`docs/carry-over.md`).

## Consequences

- A door, the evaluation harness and a tool that calls another tool all need only a
  `Call` and an `Executor`.
- Prepare reads every input whole and decodes every image input, to check it as the store
  checks it. That costs time and memory in proportion to the inputs, on each preparation
  and again at admission.
- `Backends` answers for a capability and its features, not for a named choice. A call
  that names its filler or its segmenter is not yet resolved to that choice: resolution
  (an explicit choice, then the default) is the capability interfaces', and `Backends`
  gains the choice then. The fingerprint's kind changes with it.
- Both methods of `Executor` return when they have their answer. Section 3 wants execution
  asynchronous, with status and cancel answered while a run executes. How that is
  scheduled is not decided, and `submit`'s shape may change when it is; it is cheap to
  change while nothing runs.
- `Context` and `Outcome` are shapes read from section 3, not proven by an action. The
  proofs of checkpoint 1A may change them, and the plan says a proof is never bent to fit.
- Closing `Derivation::stated` later changes every test that states one. The vectors of
  rebound masks do not change: a derivation holds a binding and adds nothing to it.

What this record does not decide, and the code does not do: an estimate; admission,
request ids and reservations; executing an action, its commit step and cancellation; the
run record; how a run id is minted.

## Alternatives

- **Close `Derivation::stated` now.** Turned down: with the composite not built there
  would be no derivation at all, and the rebind's own tests and vectors need one. It
  closes with the composite.
- **Accept a rebind when the new image's origin names the mask's image as an input,** as
  the carry-over row offered. Turned down: an origin travels in a file and is not part of
  an identity, so it is as easy to state as the derivation is. Only the pixels, or a
  record the executor wrote itself, vouch for anything.
- **Refuse a mask made by hand, or one that was imported.** Turned down: a fixture is one,
  and so is every mask a person draws. Nothing in a mask's planes says it was copied from
  another image's mask.
- **Tighten the store's rule for a run id.** Turned down: the store is the layer below
  the contract and is tested on its own terms; the narrower rule belongs to what makes
  runs.
- **Bind a preparation to the store's generation number.** Turned down: any publication
  would make every preparation stale. The inputs' identities are what the call depends on.
- **Let an action add its own check to prepare.** Turned down until one needs it: what
  prepare does today is the same for every action, and is read from the descriptor.
