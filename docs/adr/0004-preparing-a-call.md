# 0004. Preparing a call: what a call may hold, what prepare sees, and what it is bound to

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, inside checkpoint 1A, after the review of the code that came with it.
Corrected: what the decision on derivations guarantees (a mask made by hand on a
derivation's binding is the same artifact as the rebound one, so "a mask names another
image only through a rebind" was untrue, and closing `Derivation::stated` cannot make
prepare refuse it); "No side effects", which said prepare locks nothing (each input read
shares the current generation's file) and that the type holds `Backends` to asking nothing
(the trait's shape does, and an implementor could do otherwise); where the run id's rule
had been left open (a code comment and a line of the plan, not `docs/carry-over.md`).
Added: the fingerprint's tags and orders stated exactly, and a second fixed vector; a
refused input as a typed fault, and a failure of the store kept apart from a refusal, with
no path in its text; negative zero prepared as zero; that the same artifact may be given
more than once; that a refused run id is echoed up to 64 characters; that stored run ids
are not held to the run id's rule.
Amended: 2026-11-12, after the review of the amendment above. Added: what a diagnostic
reference does to a call, under a run's names and under an artifact's own; that the
reason left out of a store failure is not left out of a refused input, which can pass on
a decoder's sentence; that neither `{:?}` nor an error's chain of causes shows a path.
Changed: "the same call may be answered once the store is sound" did not hold for a
diagnostic at an artifact's name.
Spec: section 3, "The execution contract", "Identities" and "Storage"; section 6,
"Readiness"

## Context

`SPEC.md` section 3 says of prepare: it resolves a call's artifact bindings and returns
readiness for these exact arguments; it has no side effects and never spends; its result is
bound to the digests of the inputs, the config and the model identities it saw. Admission
(task A6 of the plan for checkpoint 1A) revalidates a prepared call against those digests
and refuses a stale one. Section 6 gives prepare the question "can this exact call run
now?", answered without a network.

Three questions were open for this task. The first and the third were on
`docs/carry-over.md`; the second was a comment in the store's code and a line of the plan:

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
   mask is required, here as everywhere. So is content stored under an artifact's name
   that is not that artifact.
5. Where a port says its artifact belongs to the image at another port, checks that it
   does: the same canonical pixels in the same space. A mask on another image of the same
   size is refused, and so is any artifact whose image has those pixels under another
   space.
6. Asks, for each capability the call needs (`needs_for`, from the names of the arguments
   given), which model would answer it with the features needed.

It answers with a `PreparedCall`: the action and its version, every argument that has a
value, every input, each need with the model that would answer or none, the output ports
that return something for this call, and readiness. Readiness is `Ready`, or `NotReady`
with each need that nothing answers and why. A call that is not ready is still prepared:
that it cannot run now is the answer, not a failure to answer.

A `Refusal` is what comes back instead when the call is not one the descriptor allows, or
an input is not what its port takes. A refusal is not an outcome: no run exists.

A refused input says which case it is as a type, not only as a sentence: the port, the
type the port takes, the artifact's identity, and an `InputFault`, one of `Missing`,
`WrongType`, `WrongImage` and `NotTheArtifact`. A door can tell them apart without reading
the text.

**A failure of the store is not a refusal.** Both methods of the executor answer with a
`CallError`: `Refused`, with the refusal, or `Store`, with a `StoreFailure`. The store
failing is not a judgement of the call. There are three kinds:

- content that a generation names and that is missing or damaged;
- a file that cannot be read, or a generation file that is not one;
- a diagnostic reference under an artifact's own name, outside runs, which the store
  refuses to read as an output. No publication makes such a reference; a generation file
  that other hands wrote can hold one.

For the first two, the same call may be answered once the store is sound. Not for the
third: it stays refused until the reference is one of an output.

**A diagnostic is never an input, wherever it is held.** An artifact is looked for among
the names outside runs. A run's diagnostics are among the run's names, so an identity
that a run holds only as a diagnostic is not in the store as an artifact, and a call that
gives it is refused with `Missing`, like any identity the store does not hold. That is
the case a tool will meet when it is handed something an earlier run kept as evidence.

**No path in what is said.** The text of a `StoreFailure` and of every refusal holds no
path. In a `StoreFailure`, a file of the store is named by its last part, which for
stored content is its digest, and the reason a file was found damaged is left out, since
it can quote the file. What `{:?}` prints of a `CallError` holds no path either. The
store's own error, with its paths, is kept for whoever runs the store and is had by name
(`StoreFailure::cause`); it is not the error's source, and a `CallError` has no source,
so a chain of causes printed for a caller neither reaches a path nor says a refusal
twice.

That a reason is left out holds for a store failure only. A refused input of the kind
`NotTheArtifact` passes on the sentence of whatever read the content: for an image, the
PNG or JPEG decoder's own, which can quote a chunk's four type bytes, escaped, and
checksum values. It quotes bytes of the content, never a place on the machine, and is
kept, because it is what tells one broken file from another.

**The same artifact may be given more than once.** Prepare holds a call to its
descriptor: the type at each port, how many, and which image each belongs to. It does not
ask whether the artifacts differ. The same image twice at a port that takes several, and
the image of one port given again at another, are accepted, listed as they were given and
counted each in the fingerprint. Whether a repeat makes sense is for the action to say
when it runs.

### No side effects, by what prepare is given

Prepare is given two things. Through neither can it make a durable change.

- **A read of the store** (`artifact::Contents`: the current generation and the content of
  an entry). It has no way to write, and it takes no part in the writer lock. It is not
  free of locks: each input read asks for the current generation, and a reader then
  shares that generation's file: a shared advisory lock (`flock`; the store's module
  `fs`), which can wait behind a prune that holds the file alone for the moment it takes
  to remove it. The executor's reader keeps the share until its next snapshot or
  until it is dropped, so that a prune leaves the generation it last read and what that
  names. Nothing durable changes by it. The empty generation has no file and is not
  shared.
- **`Backends`**, one question: which model would answer this capability with these
  features? It is meant to be answered from what is configured.

The tests hold the first on a file system that records every change: preparing calls that
are ready, not ready and refused creates, writes, syncs, renames and removes nothing,
takes no lock file, and leaves every file's bytes as they were. Sharing a file is not one
of the changes that file system records.

The second is held by the shape of the `Backends` trait and no further: its one method
is handed a capability and features, and is handed neither a request nor a ledger. An
implementor could still do anything inside `resolve`; nothing checks that it sends
nothing. A count of zero requests on the capabilities themselves, and of zero reservations
on the ledger, needs fakes that tasks A8 and A6 bring; an ignored test in `prepare.rs`
names what they complete.

### Bound to what it saw

A `PreparedCall` carries a fingerprint, kind `prepared/1`, hashed in the encoding ADR 0001
gives every identity: `whizbang:`, the kind, a zero byte, then the fields. A field is a
tag and a value: `b`, a length as eight bytes and that many bytes; `u`, eight bytes;
`f`, a double; `d`, a digest. The fields, in order:

1. the action's name (`b`) and its version (`b`);
2. the count of arguments that have a value (`u`); then each, in the order the descriptor
   declares its arguments, as its name (`b`) and its value. An argument has a value when
   the call gave one or the descriptor has a default; one with neither is not counted and
   not written. A value is its kind (`u`: 0 a number, 1 a whole number, 2 a boolean, 3 a
   text) and then itself: a number as `f`, with negative zero as zero; a whole number as
   `u` holding the eight bytes of its two's complement; a boolean as `u` 0 or 1; a text,
   and a choice, which is a text, as `b`;
3. the count of inputs (`u`); then each as its port's name (`b`) and its identity (`d`).
   Inputs are in the order the descriptor declares its ports, and within one port in the
   order the call gave them. That order is significant: the same artifacts at one port
   the other way round are another preparation;
4. the config's digest (`d`);
5. the count of needs (`u`); then each, in the order the descriptor declares them, as the
   capability's name (`b`: `segment`, `fill` or `judge`), the count of its features (`u`)
   and each feature (`b`) in the order the descriptor declares them, and then `u` 1 and
   the backend (`b`), the model (`b`) and the version (`b`) of the model that would
   answer, or `u` 0 where none would.

Every name, port, feature and part of a model's identity is the `b` of its UTF-8 bytes,
and a text is hashed as it was given: nothing is normalised, so one word composed (NFC)
and decomposed (NFD) is two values and two preparations.

Two tests pin a fingerprint each. The first has one argument, by default. The second has
a value of each kind, an argument with no value, inputs at two ports, and two needs with
features, one answered and one not; its digest was computed by a script written from this
text and ADR 0001's, apart from the code, which is not kept in the repository.

A prepared argument that is a number holds zero where the call gave negative zero, so
what is bound and what an action is handed do not differ by a sign.

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
  support is the original. Until then a mask that code moved with a false statement names
  the image it was moved onto, and prepare accepts it there; a test pins this
  (`prepare::a_mask_rebound_by_a_stated_derivation_is_still_taken_at_its_word`), beside
  the one ADR 0002 names (`rebind::a_derivation_is_trusted_to_be_true`).
- **A mask made by hand is the statement of whoever made it.** A mask made directly on an
  image, by `Mask::from_core` or as a file brought in with `whizbang store import`, is
  accepted on that image, and its origin says how it was made. Whizbang cannot tell a good
  mask from a bad one by its provenance, and does not try.
- **A rebound mask and a mask made by hand can be one artifact.** A derivation holds a
  binding and adds nothing to it, and an origin is not part of an identity. So a mask made
  with `Mask::from_core` directly on the binding that places a second image onto the first
  by the identity has the same identity as the first image's mask rebound through
  `Derivation::stated`, and its origin has no `rebind` step. A test pins the equality
  (`prepare::a_mask_made_by_hand_on_a_derivations_binding_is_the_rebound_mask`), and
  prepare accepts either on the second image.

What holds, then, is this and no more: **a mask is used only on the image its binding's
source names.** It does not hold that a mask names a second image only through a rebind,
nor that its origin records one. Closing `Derivation::stated` in task A8 closes one way of
making such a mask, in the API; it does not make prepare refuse the artifact, and the
prepare test above cannot be inverted by it. Only the rebind's own test can. What the
executor could add is a record it wrote itself, of the composite and the derivation it
checked, which a mask made by hand cannot have. Task A8 decides between the two, and the
row on `docs/carry-over.md` states them: the trust boundary is the maker's statement, and
the prepare test is rewritten with `Mask::from_core` and stays accepted; or prepare
requires that record for a mask whose binding places its image onto another.

### The run id

A `RunId` is a lower-case letter or a digit, then lower-case letters, digits, `_` or `-`;
64 characters at most. So it is one path segment, one word on a command line and one part
of an address with nothing to escape, and two ids never differ by letter case alone.

It is enforced where a `RunId` is made (`RunId::parse`), and the executor, the only thing
that makes a run, names it with one. The store keeps its looser check as the floor: it is
the layer below, and its own tests publish under ids this rule refuses. Every `RunId` is
an id the store takes. How an id is minted is decided with the code that makes runs.

A refusal shows what was refused escaped, and its first 64 characters at most, with its
length when it was longer.

The layering holds by convention. `Publication::put_in` is public and takes any id the
store's rule allows, and a generation file is not checked against either rule when it is
read. So an id read back from a store is not a `RunId` and is not trusted to be one; what
lists and shows runs must take stored ids as they are (`docs/carry-over.md`).

### The executor's interface, and the types around execution

`Executor` has two methods, and each answers with what it was asked for or a `CallError`.
`prepare` is the above. `submit` is the entry point for a call
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
  rebound masks do not change: a derivation holds a binding and adds nothing to it. For
  the same reason it does not change what prepare accepts (above).

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
