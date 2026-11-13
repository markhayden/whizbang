# Plan: checkpoint 1B, policy and controls

Draft 2, 2026-11-13. From `SPEC.md` sections 3, 4, 6, 10, 11 and 16. This file is the task
list.

Checkpoint 1A is still being built while this is written. Merged, and read for this draft:
request ids and admission (task A6, ADR 0007) and the commit step (task A7, its first part,
ADR 0008). Not merged: cancellation, status and nested calls (A7, parts 2 and 3), and A9,
A11 and A12. Nothing here is built before A12, the exit check of 1A. Where a task below
depends on something 1A has yet to decide, it says so under "What this plan needs from 1A",
and task B0 settles each open point from the merged code before any other task starts.

## What this checkpoint delivers

The policy around a call, proven against fakes. No model runs, nothing is downloaded and
nothing is sent: every backend, every download and every recipient is a fake from the test
kit.

1. **Spend.** A ledger in the store. A paid call reserves its bound when it is admitted, a
   request is recorded before it leaves, each attempt is settled, and what a crash left
   unsettled is charged at its bound. The five cases of `SPEC.md` section 6 pass: nested
   retries, simultaneous calls, cancellation after dispatch, a crash between dispatch and
   settlement, and a backend that offers only an estimate.
2. **Estimates** that bind one exact call, expire after an hour by an injected clock, and
   are refused when stale.
3. **Config and secrets.** One config file fills the roster of backends and gives the
   executor its real digest; keys are read from an env file and never written.
4. **Model installs** with fake downloads: atomic, checked by hash, cancellable, the
   previous installation left in place on failure, a second request attached to the
   running install, and removal that drains.
5. **Egress denied by default:** data leaves only to a recipient the config approves for
   that class of data, every hop is checked, and each run records where data went.

## What this plan needs from 1A

**Decided by 1A, and what 1B does with each** (ADR 0007, ADR 0008, and ADR 0005 as
amended; B0 reads them again, with every amendment made since this draft):

| What 1A decided | What 1B does |
|---|---|
| The ledger's interface is `Ledger::reserve`, called inside admission for every attempt at its publication, with a `Step`: the generation the attempt found, the reservations it names, and `Step::make` to add records under names made once. A ledger that must not overlook another admission makes a name that admission would make too; the store refuses the later one whole and the ledger is asked again | S2 implements `Ledger` and changes nothing in admission. S1's ADR changes the interface in the three ways `docs/carry-over.md` asks: a ledger cannot return without having made its name, its names have no empty, `.` or `..` segment, and it has a part in the publication that ends a run (S11) |
| A reservation is `reservation/1` at `spend/reservations/<run>`, made once, holding the run, the request, the admission's digest and the amount. A ledger's own names start with `spend/` and not `spend/reservations/`. What was reserved is read through names of the current generation, never from content | S1's records are the ledger's own (an allowance, a dispatch, a settlement, a release), under `spend/`, each made once. Nothing rewrites a reservation |
| Money is millionths of a US dollar (`Estimate`, `reserved_micros`); a sum stops at the largest amount, which is "no bound" and is refused with `Refusal::NoBound` by an executor with a ledger. `Refusal::OverAllowance` says what was asked and what is left | B1 adds no unit. A backend that offers only an estimate states no bound, so `NoBound` is the refusal of `SPEC.md` section 6 at admission; S4 adds the same refusal for a request about to be sent |
| An estimate is the sum of `Backends::bound` over the call's needs; it is not in `prepared/1`, and admission compares it beside the fingerprint (`Stale::Estimate`). A request may come with the preparation its caller was shown (`Request::expecting`) | S3's cap and S8's age are compared beside the fingerprint in the same way; `prepared/1` does not change |
| The config is read when a call is prepared, each time, through `ConfigSource` (`Engine::with_config`) | C4 implements `ConfigSource` over the config file |
| A name outside runs is made once with `Publication::create`; `Publication::put` can still replace it | S1 decides whether the store refuses a `put` under `requests/` and `spend/` |
| The run record is `run/2`: `spend` is an object, `reserved_micros` and an empty list `requests` | `run/3` (B4) defines the element of `spend.requests`, of `asked.settings` and of `egress` |
| A run ends in one publication (`close_run`), which for a completed run is the commit step; a panic in an action ends its run as an error | S11 settles the run's reservation in that publication, for every way a run ends |
| Nothing settles: a reservation stays for good; a run a dead process left is never ended; an admission that was visible and not confirmed takes its id | S11, S6 |

**Still open in 1A.** B0 reads the merged code and the ADRs, writes the answer into this
table in place of "open", and stops if an answer is neither of the two outcomes written
here.

| # | From | What is open | If it came out one way | If it came out the other |
|---|---|---|---|---|
| D5 | A7, parts 2 and 3 | The cancellation rule, the decorator that accounts for requests inside a handle, and how a child's allowance is taken from its parent's | The decorator exists: the gate (E2) and the meter (S5) are two more decorators, composed where the executor makes handles | Accounting is inside the engine: B5 first moves it behind a decorator |
| D6 | A7, part 3 | The elements of `outcome.refusals` | Defined: a refused send (E4) and a refused request (S5) are written as such elements | Not defined: B4 defines them in `run/3` |
| D7 | A7, A11 | What `run/2` holds at the tag `m1a`: the `models` and `time.durations` elements, and the parent | `run/3` adds only what this plan names | Something this plan names was filled already: B4 leaves it as it is |
| D8 | A9 | The one source of a store's location that every command shares | A setting or a default exists: `spend`, `models` and the config's location follow it | Still `--store <dir>` on every call: the new commands take it the same way, and the row stays with its owner |
| D9 | A11 | Where the executor makes a run's capability handles | One function: the composition point of B5 | Several places: B5 first brings them to one |
| D10 | A7, part 2 | Whether an admission can be cancelled and has a deadline, and whether a ledger asked again and again is given a turn | Decided: S2 keeps to it | Left as it is: S2 states the bound it puts on being asked again, or says why none |

ADR numbers are not fixed here, because 1A may take more. Each ADR of this plan takes
the next free number when its pull request opens. The ADRs of checkpoint 1A (0001 to 0008 so far) close with it
and cannot be amended in place after its tag (`docs/adr/README.md`), so every decision below
that changes one of them is a new ADR that names the one it changes.

## Decisions this plan makes

Each is settled here, with its reason, and is the owner's to overturn.

- **The allowance lives in the ledger, not in the config.** `SPEC.md` section 6 says an
  allowance is reset only by the owner's command. A budget written in a config file could be
  raised by editing the file. So `whizbang spend reset --allowance <usd>` is the only thing
  that sets or raises what a store may spend, a store with no allowance refuses every paid
  call, and the config holds only a per-call cap, which can lower what a call may spend and
  never raise it.
- **`whizbang spend reset` is a command and nothing else.** It is not an action, has no
  descriptor, and cannot be reached through the executor or, later, a door. An agent runs
  it only against a temporary store in a test (`SPEC.md` section 15).
- **Balances are a fold, not a file that is rewritten.** ADR 0007 gives every spend record
  a name that is made once. So the ledger has no summary that is given other content: what
  is reserved, spent and unresolved is worked out from the records the current generation
  names. If S13's measurement shows that costs too much, the answer is a checkpoint record
  under a new name, never a rewrite.
- **A damaged spend record stops paid calls until a person acts.** The ledger does not
  guess past a record it cannot read. `whizbang spend set-aside` (S12) is the command, and
  it charges what the record's name and the run's admission show.
- **Models are installed by the binary, in Rust, into a models directory outside any
  store.** Rascal installed from Python. Here a model must be listed, installed and judged
  ready with no Python present (`SPEC.md` section 1, the first user story), and 1B comes
  before the pack. Weights are shared by every store on a machine and are too large to copy
  into each, so they are not in a store; the directory has its own lock and its own atomic
  publication. `SPEC.md` section 3 says the store is the one owner of everything durable;
  M1's pull request brings that sentence in line (the store owns everything durable about
  runs; the models directory owns installed models), and adds "model installations" to what
  `whizbang-backends` holds. These two edits are reviewed here, with this plan.
- **`whizbang-backends` is created in 1B,** for model installations and for what turns a
  config into a roster. It holds no adapter and no network client yet.
- **No network client is added in 1B.** The core defines the two ways out as interfaces
  (B2): sending data to a recipient, and downloading a model's file. The test kit fakes
  both. The real HTTPS client arrives with the first thing that needs it: a real catalogue
  entry at 1C, a provider at milestone 2. A check added in E5 fails `just check` if any
  crate names a network client or a socket outside the one module allowed to.
- **The shipped catalogue is empty until 1C.** `whizbang models list|get|remove` exist and
  are tested through the binary's own wiring with a test catalogue and fake downloads; the
  shipped binary lists no model.
- **An install belongs to the process that runs it.** A second request for the same model
  inside that process attaches to it. A second process is refused as busy, with the holder
  named, as a second writer of a store is. Attaching across processes is not built: nothing
  in `SPEC.md` asks for it, and `whizbang serve` and `play` are one process.
- **Holds are file locks, not process ids.** A run that uses a model holds a shared lock on
  its installation; removal takes it alone. The operating system lets a lock go when its
  process dies, so nothing reads a process id from a file, nothing is signalled by one, and
  a hold cannot outlive its process (rascal's R3 and R5).
- **Approval of a recipient is the config's.** The build carries no list of recipients. A
  config that names none approves none. The examples and tests in this repo name fake
  recipients only; naming a real one in a config an agent runs with is the owner's decision
  (`SPEC.md` section 10).
- **One version of the run record for the checkpoint.** `run/3` defines the elements of
  `asked.settings`, of a settled `spend` line and of `egress` in one pull request (B4),
  before the strands that fill them, so that three strands do not make three versions. If
  a strand then shows a shape is wrong, the correction is `run/4`, as ADR 0005 requires
  once a store is kept.
- **`whizbang backends status` is not in 1B.** Without `--probe` it would repeat what
  prepare and `models list` say, and a probe needs a backend that can be reached. It goes
  on the carry-over list for 1C, with the first local backend.
- **The scan's pattern is fixed, not accepted.** The carry-over row on a key written
  directly after a hyphen, an underscore, a letter or a digit allows either a fix or the
  owner's acceptance. X1 fixes it.
- **Crates created:** `whizbang-backends`. `whizbang-eval` exists; `whizbang-serve` waits.

## Rules every task's tests follow

These are the cases reviews of 1A kept finding. Each task's "Done when" names where they
apply; a task is not done while one that applies to it has no test.

1. **Decide where you publish.** A decision that depends on what the store holds (is there
   enough allowance, is this model being removed, does this record exist) is made inside
   the store's publishing step, or under the lock that guards the directory, never by a
   read followed by a write. Its test runs two executors over one store, or two installers
   over one models directory, stops both at a barrier after the read a wrong implementation
   would decide from, and releases them together.
2. **A crash at each step.** Every durable write is tested with a crash injected before
   each event the file-system shim records, with unsynced writes and names lost. Reopening
   shows the state before or the state after, whole.
3. **Stored bytes are not trusted.** Whatever is read back from a disk (a ledger record, a
   config, an env file, a catalogue, an installation record, a status) has its length
   looked at before it is read, is parsed strictly (an unknown field, a missing field and
   a second form of the same value are refused), and a refusal shows what it quotes escaped
   and cut, with no path as text or as `{:?}`.
4. **A secret is never written.** Not as a value, not inside another value, not as a name
   or a key, not in `{:?}`, an error, a journal, a record, a status file or a refusal. A
   test plants a marker secret and searches every byte the task wrote.
5. **A document says what the head does.** An ADR, a guide or a help text added by a task
   names, for each thing it claims the code does, the test that shows it; the pull request
   is read against that list at the head that is merged.
6. **No sleeps.** A test waits on a barrier or an event. Time comes from the injected
   clock (`Clock`, ADR 0005), which a test moves by hand; a wait that can time out is woken
   by the clock moving, not by time passing.

7. **A stated number of threads.** The executor runs each started action on a thread of
   its own, and 1A bounds how many started runs are live. A task whose tests drive many
   runs, requests or downloads at once (S2, S5, S7, S11, M2 to M5 above all) states its
   largest number at once in its hand-back, uses two where two show the race, and keeps
   every test process under 200 live threads. A test that needs more than 16 at once
   says why in a comment beside it.

## Strands, and what each owns

After the base tasks, five strands are built at the same time by different builders. A
strand writes only in the files it owns; the files several strands need are changed only by
the base tasks and the join tasks named below.

| Strand | Tasks | Owns | Can start after |
|---|---|---|---|
| Base | B0 to B5 | `crates/whizbang-core/src/capability/declare.rs` and `fault.rs`; `crates/whizbang-core/src/contract/record.rs` and the record's schema; `crates/whizbang-testkit/src/capabilities.rs`; the place handles are composed | A12 |
| Spend | S1 to S13 | `crates/whizbang-core/src/spend/`, its tests in `tests/it/spend/`, `crates/whizbang/src/spend.rs`, the test kit's ledger file | B5 (S1 after B1) |
| Config and secrets | C1 to C4 | `crates/whizbang-core/src/config/`, `crates/whizbang-core/src/contract/redact.rs`, their tests, `crates/whizbang/src/config.rs`, `docs/guides/config.md` | B0 (C1, C2, C3); C4 after B5, S2 and E2 |
| Egress | E2 to E5 | `crates/whizbang-core/src/egress/` beyond B2's types, its tests in `tests/it/egress/`, `scripts/check-egress.sh` and its test | B5 and S5 (E5 after B2) |
| Models | M1 to M7 | `crates/whizbang-backends/`, `crates/whizbang-testkit/src/downloads.rs`, `crates/whizbang/src/models.rs`, `docs/guides/models.md` | B2 (M1 to M5); M6 after C4 |
| Scripts | X1, X2 | `scripts/scan.sh`, `scripts/tests/scan/`, `scripts/tests/test-deny.sh`, `deny.toml` | A12; needs nothing else |

**Files more than one strand must touch, and who touches them.**

| File | Changed by | Rule |
|---|---|---|
| `crates/whizbang-core/src/contract/engine.rs`, `crates/whizbang/src/wiring.rs` | The join tasks only: S2, S5, S11, E2, C4, in that order of merging | One join task is open at a time. A strand's other tasks build behind an interface in the strand's own folder and are tested there |
| The record's types and schema | B4, then S10 and E4 only to write values into shapes B4 defined | A change of shape after B4 is a new version and a pull request of its own |
| `Cargo.toml`, `Cargo.lock` | C1 (a TOML reader), M1 (the new crate) | Whichever merges second merges `main` first |
| `SPEC.md` | C1 (the config example), M1 (the two sentences above), Z1 | One sentence each, named in the task |
| `docs/carry-over.md` | Every task, in its own rows | A conflict is resolved row by row |

## Order

Each task needs every task listed for it to be merged first.

| Task | Strand | Needs | Size |
|---|---|---|---|
| B0 Entry check | Base | A12 | small |
| B1 What a backend says of cost, and how far a request got | Base | B0 | small |
| B2 The ways out: recipients, classes of data, two interfaces | Base | B1 | small |
| B3 Fakes that quote, fail at a stage and name a recipient | Base | B2 | small |
| B4 `run/3`, and a place for a run's facts | Base | B3 | medium |
| B5 One place where handles are composed | Base | B4 | small |
| S1 The ledger's records | Spend | B1 | medium |
| S2 Reserving at admission | Spend | S1, B5 | medium |
| S3 The per-call cap, and a call's own | Spend | S2 | small |
| S4 Quotes: a bound, or no dispatch | Spend | S2 | small |
| S5 Dispatch first, then settlement of each attempt | Spend | S4 | medium |
| S11 The reservation settled when its run ends | Spend | S5 | medium |
| S6 After a crash | Spend | S11 | small |
| S7 Nested calls and retries | Spend | S5 | medium |
| S8 Estimates expire | Spend | S4 | small |
| S9 `whizbang spend status` and `reset` | Spend | S2 | small |
| S10 The run's spend lines | Spend | S11, S7 | small |
| S12 A damaged spend record set aside | Spend | S9 | small |
| S13 What the ledger costs as a store ages | Spend | S2 | small |
| C1 The config: its file, its shape, its digest | Config | B0 | medium |
| C2 Keys from an env file | Config | C1 | small |
| C3 What redaction did not find | Config | B0 | medium |
| C4 The roster, the settings and the policies from the config | Config | C1, C2, S3, E2 | medium |
| E2 The gate: denied by default | Egress | B5, S5 | medium |
| E3 Every hop | Egress | E2 | small |
| E4 The run's recipients | Egress | E2 | small |
| E5 One way out | Egress | B2 | small |
| M1 A models directory: catalogue, installation, identity | Models | B2 | medium |
| M2 Installing, with a crash at each step | Models | M1 | medium |
| M3 Attach and cancel | Models | M2 | small |
| M4 A file two models share | Models | M2 | small |
| M5 Holds, and removal that drains | Models | M2 | medium |
| M6 Readiness from what is installed | Models | M5, C4 | small |
| M7 `whizbang models list`, `get` and `remove` | Models | M3, M4, M5 | small |
| X1 A key directly after a word character | Scripts | A12 | small |
| X2 A planted registry fails `just deny` | Scripts | A12 | small |
| Z1 Exit check | | all | small |

**What runs at the same time.** B0 to B5 are one after another and are the only stretch
that is. From B0: C1, C3, X1 and X2, four builders. From B1: S1 beside them. From B2: M1
and E5. From B5: the spend strand (S2 onward) runs beside the models strand and the config
strand, each in its own folder, and the egress strand joins them at E2, which waits for S5
only because the two wrap the same request. Inside the spend strand, S3,
S8, S9, S12 and S13 are side branches that can be built beside S4, S5, S11 and S7. Inside models, M3, M4 and M5
can be built at the same time once M2 is merged. The join tasks merge in the order S2, S5,
S11, E2, C4; a join task that is ready early waits for the one before it, and its builder merges
`main` before the review.

**Size** is read from the acceptance list: small is up to about 1,000 added lines, tests
included, and medium up to about 2,500. No task is larger. A task whose branch passes 2,500
added lines before it is done stops and is split along its "Done when" lines, the first
part merged first; the builder says so in its hand-back and does not go on.

## Commit strategy

- One task is one branch and one pull request, built by the builder, reviewed by a
  reviewer that did not write it and merged by the orchestrator with a merge commit, once
  `ci` and both scans pass.
- An ADR is in the pull request of the task that names it, with the types it decides and
  their tests. Its implementation is the next task's.
- Ported code names its source in the commit message: the old repo, the path, and the
  pinned commit from `docs/sources.toml`.
- **Until a strand's join task merges, the binary behaves as it did at the tag `m1a`:**
  with no config it has no backend, so nothing can be reserved, sent or installed by the
  shipped binary from a strand that is half built. Every state of `main` between the tasks
  below passes `just check`.
- **Rollback tag:** `m1b` at the exit check. There is no tag before it.
- **If a task shows a decision of this plan is wrong,** the decision is changed here first,
  in a pull request of its own, and if it changes scope or architecture it gets the review
  rounds first (`AGENTS.md`, "Working with the owner"). A test is never bent to fit.

### What undoing each task takes

| Tasks | To undo |
|---|---|
| B1, B2, B3, B4, B5 | Foundations: revert every later task first. A store that holds a `run/3` record is discarded with it; before the tag `m1b` no store written by 1B is worth keeping |
| S1, C1, M1 and the ADR each brings | Revert the strand's later tasks first; the ADR goes with its task. Nothing outside the strand's folder depends on them until its join task |
| S3, S4, S6, S7, S8, S9, S12, S13, C2, C3, E3, E5, M2, M3, M4, M5, M7, X1, X2 | Revert the merge. Each is inside its strand's folder, or a script with its test |
| S2, S5, S11, E2, C4 (the join tasks) | Revert in the reverse of the order they merged, each after the tasks of its own strand that follow it. After S2 is reverted the binary has no ledger again and reserves nothing, as at `m1a`, and the test kit's fake ledger still passes 1A's tests, so the fake is kept in the tree until Z1 |
| S10, E4, M6 | Revert the merge; the record then holds empty lists where they wrote |
| Z1 | The note reverts; the `m1b` tag stays and a corrected checkpoint is `m1b.1` |

A ledger, a models directory or a config written by a reverted task lives only in a test's
temporary folder: no task below is run against a store or a models directory that is kept.

## Tasks

### B0. Entry check

**Depends on:** A12. **Reads:** every ADR merged since 0006, with its amendments; the note
`docs/plans/m1a-note.md`; every row of `docs/carry-over.md` that names 1B.

- [ ] Each open row D5 to D10 above is answered, and the table of what 1A decided is read
      against the ADRs as amended since, from the merged code, in this file.
- [ ] Each task below whose text no longer fits the answer is corrected here. A correction
      that changes what is built, not how it is worded, gets the review rounds first.
- [ ] Every carry-over row whose owner is 1B is matched against "Carry-over rows and their
      tasks" below; a row added since this draft is given a task or a new owner, with the
      reason.

**Done when** the table has no "open", the list of carry-over rows matches
`docs/carry-over.md` at the head, and `just check` passes.
**Size:** small. Documents only.

### B1. What a backend says of cost, and how far a request got

**Depends on:** B0. **Reads:** squint `crates/squint-core/src/ledger.rs` (`Pricing`,
`RequestShape`) and `crates/squint-providers/src/catalog.rs` (`Prices`, `bound_tier`);
rascal `crates/rascal-mask/src/filler/mod.rs` (`FillerCall`, `Charge`, `CallStarted`,
`Spent`); table-manners `CONTRACT.md` section 4.5.

- [ ] An ADR, which changes ADR 0006 and names ADR 0007: a quote, in the unit ADR 0007
      fixed (millionths of a US dollar), which is one of free, an enforceable bound, or an
      estimate only; for a price by time, that only a limit the host enforces or a fixed
      maximum charge makes a bound; how a quote becomes the answer of `Backends::bound`
      (an estimate only is no bound); and what a failed request says of how far it got:
      not sent, answered (with what it cost, when the answer says), or sent with no answer
      that settles it.
- [ ] The types in `crates/whizbang-core/src/capability/`: the quote, a
      backend's statement of its price for a request, and the stage on a `Fault`. A backend
      that states no price is free and local, so no existing backend or fake changes.
- [ ] No ledger, no reservation and no fake in this task.

**Done when** these pass: a quote cannot be made from a number that is not finite or is
negative, and one made from a fraction of the unit is rounded up; a quote of an estimate
only, asked through `Backends::bound`, is the amount that is no bound; a price by time with only a timeout
of whizbang's own gives an estimate, and with a host's limit gives the bound of price times
limit; each of the three stages can be read from a fault, and every fault made before this
task reads as not sent or as unresolved by the rule the ADR states, with a test of each
kind in ADR 0006's table; `{:?}` of every new type shows no more than its numbers.
**Carry-over:** the rows on whether a failed call may have been charged, and on squint's
`pricing` (its interface half). **Size:** small.

### B2. The ways out: recipients, classes of data, two interfaces

**Depends on:** B1. **Reads:** squint `crates/squint-providers/src/privacy.rs` (`Consent`,
`Credential`, `redact_account`) and `http.rs` (`same_origin`); squint
`crates/squint-core/src/provider.rs` (`recipient`); `SPEC.md` section 10, "Egress".

- [ ] An ADR: a recipient (an endpoint's origin, the account used with it, the upstreams it
      may forward to); the four classes of data (source images, derived images, prompts,
      evidence) and who states the class of each part of a request; that a part with no
      stated class needs every class approved; the interface through which data is sent,
      and the separate one through which a model's file is downloaded, which has no place
      for user data or for a credential bound to another origin; where a backend says who
      receives its requests.
- [ ] The types and the two interfaces in `crates/whizbang-core/src/egress/`, and the
      recipient in a backend's declaration. A backend that names no recipient is local.
- [ ] No enforcement in this task: the gate is E2.

**Done when** these pass: a recipient cannot be made from an address with a user, a
password, a query or a fragment in it, and its `{:?}` shows the origin and a cut account
and nothing else; two origins that differ in scheme, host or port are two recipients; the
download interface's request type cannot be built with a body, a picture or a prompt (a
test that fails to compile is not used: the type has no such field, and a test lists its
fields); a declaration with no recipient reads as local.
**Carry-over:** the row on squint's `recipient`. **Size:** small.

### B3. Fakes that quote, fail at a stage and name a recipient

**Depends on:** B2.

- [ ] In `crates/whizbang-testkit/src/capabilities.rs`: each fake can be built with a price
      (free, a bound, an estimate only, a price by time), with a recipient, and to fail at
      a named stage. Each can stop at a named barrier before it answers and after it has
      counted the request, if 1A's task A7 did not already give it that.
- [ ] A fake that sends: it passes scripted hops (a redirect, an upload address, a returned
      download address) through the sending interface of B2, and keeps what it was asked
      to send to each.

**Done when** these pass: a fake built with each kind of price quotes it for a request and
counts the request once whether it answers or fails; a fake told to fail at each stage
gives a fault that says that stage; a fake stopped at its barrier holds its request open
until the test releases it and sees the run's cancellation meanwhile; the sending fake
records each hop in order with the headers it would have carried.
**Size:** small.

### B4. `run/3`, and a place for a run's facts

**Depends on:** B3. Needs D6 and D7. `run/2` is frozen at the tag `m1a` (ADR 0005).

- [ ] An ADR, which changes ADR 0005: `run/3`. The element of `asked.settings` (a name and
      a value, as an argument is written, in a stated order); of `spend.requests`, one
      paid request (reserved, settled, unresolved, and the stage it reached); of `egress`
      (the recipient, the classes of data, and whether it was sent or refused). What
      reading does with a `run/2` record: it is read, as frozen at `m1a`.
- [ ] The types, the schema `run-3.json` and fixed vectors. The executor writes `run/3`
      with these three lists empty until a strand fills them.
- [ ] A place for a run's facts: the executor gives each run something a setting, a spend
      line and a recipient can be noted in, from another module, and writes what was noted.

**Done when** these pass: a `run/3` record with each new element round-trips and validates,
and the schema and the types refuse the same faults, one document for each (ADR 0005's
three ways); a `run/2` record written by the tag `m1a` is read and shown; a `run/3` record
with an element in a list a strand has not filled is still valid; a recipient's account
and a setting's value pass through redaction before they are written (rule 4).
**Size:** medium.

### B5. One place where handles are composed

**Depends on:** B4. Needs D5 and D9.

- [ ] One function makes a run's handles, and wraps a backend in a fixed order: the
      handle's own checks (ADR 0006), then the gate, then the meter, then 1A's accounting,
      then the backend. The gate and the meter are two interfaces of the core, each with
      an implementation here that lets everything through; E2 and S5 bring the real ones.

**Done when** these pass: the order of the wrappers is asserted by a test that records
which saw a request first; a request the handle's own checks refuse reaches no wrapper; a
wrapper that refuses stops the request before the next one sees it; every test of 1A's
tasks A7 and A11 passes unchanged.
**Size:** small. If D5 or D9 came out the second way, the move it needs is the first
commit of this task, and a pull request of its own if it passes 1,000 lines.

### S1. The ledger's records

**Depends on:** B1. **Reads:** ADR 0007, "The reservation"; squint
`crates/squint-core/src/ledger.rs` (`Ledger`, `CapLedger`, `Account`, `Hold`, `Closing`)
and `crates/squint-eval/src/eval_ledger.rs`; rascal `schemas/rascal/ledger-v1.json`.

- [ ] An ADR, the ledger, which changes ADR 0007's interface: the ledger's own records (an
      allowance set or reset, a dispatch, a settlement, a release) and their names under
      `spend/`, each made once; how the balances are folded from them and from the
      reservations; the name every admission makes so that two cannot both take the last
      of the allowance; that `Ledger::reserve` cannot return without having made it (a
      token only `Step::make` gives); that a ledger's name has no empty, `.` or `..`
      segment; the ledger's part in the publication that ends a run; and whether the
      store refuses a `put` under `requests/` and `spend/`.
- [ ] The record types, written one way and read strictly, in
      `crates/whizbang-core/src/spend/`, and the change to `Ledger` and `Step` with the
      test kit's fake brought in line. No executor uses the new records yet.

**Done when** these pass: each record round-trips to the same bytes; a record over its
bound is refused from its entry's length, before it is read, and one with an unknown
field, a missing field or an amount written a second way is refused, with the refusal
escaped, cut and without a path (rule 3); the fold gives the stated balances for a table
of histories that includes a reset, a release and an unresolved charge; a ledger that
returns from `reserve` without making its name does not compile or is refused, as the ADR
says which, and a name with an empty, `.` or `..` segment is refused; a `put` under
`spend/` and `requests/` is refused by the store, with the pin test
`known_a_name_that_was_made_can_still_be_replaced_by_put` turned round, or the limit is
kept with its reason in the ADR; every test of task A6 passes against the fake as changed.
**Carry-over:** the rows on "never removed" and `put`, and on the ledger (the token, the
names). **Size:** medium.

### S2. Reserving at admission

**Depends on:** S1, B5. A join task. Needs D10.

- [ ] The real ledger implements `Ledger`: asked inside admission, it folds the balances
      from the step's generation, declines over the allowance, and makes its name.
- [ ] The binary's executor is built with it (`wiring::runner`). So every call the binary
      admits has a reservation, and a call whose cost has no bound is refused.
- [ ] A store with no allowance set declines every call whose estimate is over zero.

**Done when** these pass: two executors over one store, each with the real ledger, each
told yes for the last of the allowance and held at a barrier inside the ledger, end with
one admitted and the other asked again and declined (rule 1); a crash before each event of
the admission leaves the run, the id's record, the reservation and the ledger's record
together or none (rule 2); a call with an estimate of zero is admitted in a store with no
allowance, and one over zero is refused there with `OverAllowance`; a call with no bound
is refused with `NoBound` by the binary's executor; every test of `admit.rs` passes with
the real ledger in the fake's place; prepare makes no reservation and no record.
**Carry-over:** the row on the ledger (implemented, and the binary built with it).
**Size:** medium.

### S3. The per-call cap, and a call's own

**Depends on:** S2. **Reads:** rascal `crates/rascal-core/src/config.rs`
(`LimitsConfig`) and `crates/rascal-mask/src/describe.rs` (`common_properties`,
`max_spend_usd`); table-manners `CONTRACT.md` section 4.5.

- [ ] A cap on what one call may reserve, handed to the executor when it is built (C4 reads
      it from the config). A call may carry a lower cap of its own, beside its arguments,
      as a `Request` carries the preparation it expects. The cap is compared at admission,
      beside the fingerprint, as the estimate is; `prepared/1` does not change.

**Done when** these pass: a call whose bound is over the cap is refused before a run
exists; a call's own cap lowers the cap for that call; a call's own cap above the
configured one is refused, not lowered; a cap of zero is refused as not a cap; a
refusal by the cap makes no run and no reservation and leaves the request's id free; the
two fixed vectors of `prepared/1` are as they were.
**Carry-over:** the row on `max_spend_usd`. **Size:** small.

### S4. Quotes: a bound, or no dispatch

**Depends on:** S2. **Reads:** ADR 0007, "The estimate"; squint
`crates/squint-core/src/ledger.rs` (`plan`, `Envelope`, `PlannedRequest`) and
`crates/squint-eval/src/estimate.rs`; rascal `crates/rascal-mask/src/estimate.rs`.

- [ ] A roster answers `Backends::bound` from the quotes of its backends (B1). The bound a
      backend states for a need covers every request the call sends it.
- [ ] A request is quoted again when it is about to be sent, against what its run has
      left of its reservation. The check is an interface in `spend/` that S5's meter
      calls; nothing in the engine changes here.

**Done when** these pass: a call that needs a backend offering only an estimate is refused
at admission with `NoBound`, with no run; a request to such a backend from inside a run is
refused without being sent and without a dispatch record, counted by the fake; a request
whose quote is over what the run has left is refused the same way; a price that changed
since the preparation is `Stale::Estimate` at admission; a backend priced by time with a
host's limit is admitted at price times limit, and with only a timeout of whizbang's own
is refused.
**Carry-over:** the row on squint's `pricing` (where a price is stated). **Size:** small.

### S5. Dispatch first, then settlement of each attempt

**Depends on:** S4. A join task. Needs D5. **Reads:** rascal
`crates/rascal-mask/src/filler/mod.rs` (`Charge`, `spent`); squint
`crates/squint-core/src/ledger.rs` (`settle_with_unresolved`).

- [ ] The meter, a wrapper in `spend/`: before a paid request leaves, its dispatch record
      is durable; when it ends, it is settled by the stage it reached.
- [ ] Not sent: nothing is charged and what was held for it returns to the run. Answered:
      the cost the answer gives, or the bound when it gives none. Unresolved, a timeout
      among them: the bound, counted as unresolved.

**Done when** these pass: the fake, stopped at its barrier, reads the store and finds its
own dispatch record durable before it was called; a fault at each of the three stages
settles as stated, one test each; a run cancelled after dispatch, with the fake released
only afterwards, is charged the bound as unresolved and its run is terminal only once that
is recorded; a request that times out is charged its bound; a crash before each event of
the dispatch and of the settlement leaves each record whole or absent, and the balances
folded after it count the request once (rule 2).
**Carry-over:** the row on whether a failed call may have been charged (its ledger half).
**Size:** medium.

### S11. The reservation settled when its run ends

**Depends on:** S5. A join task.

- [ ] The publication that ends a run settles its reservation through the ledger's part in
      that step (S1): what its requests were charged is spent, what is unresolved stays
      counted, and the rest is released. It is one publication with the run's record.
- [ ] That holds for every way a run ends: completed, error, invalid, a panic in its
      action, and cancelled when 1A's cancellation is there.

**Done when** these pass: a run that reserved ten and spent three leaves seven for the next
call, one test for each way a run ends; a crash before each event of the end publication
leaves the run admitted with its reservation whole, or ended with its settlement (rule 2);
a run whose end the store refused keeps its reservation and is settled by S6's rule on the
next open; two runs that end at once over one store, held at a barrier inside the ledger,
are both settled and the balances count each once (rule 1); a settled reservation is
never settled a second time, shown by ending a run whose settlement is already named.
**Carry-over:** the row on settlement (a run that ends). **Size:** medium.

### S6. After a crash

**Depends on:** S11.

- [ ] When a store is opened to write, every run that was admitted and did not end is
      ended as an error, in one publication: each of its requests that was dispatched and
      not settled is charged at its bound as unresolved, the rest of its reservation is
      released, and nothing is sent again. Opening to read changes nothing.
- [ ] An admission that became visible and was not confirmed durable is such a run: the
      executor that made it ends it the same way, so its id answers with an ended run.

**Done when** these pass: a process that died between dispatch and settlement leaves,
after the next open to write, that request charged at its bound, the rest released, and
the run ended as an error; the fake counts no second request; a crash before each event
of the recovery itself, then another open, settles it once and no more; a reader that
opens the same store first sees the run admitted and writes nothing; a repeat of the
request under its id is answered `Ended`, with the error; an unconfirmed admission is
ended by its executor and its repeat answered the same way.
**Carry-over:** the rows on settlement (a run that did not end) and on an admission that
was visible and not confirmed. This turns ADR 0007's alternative "mark an interrupted run
when the next writer opens the store", turned down "for now" with settlement named as the
reason to come back; S1's ADR records it. **Size:** small.

### S7. Nested calls and retries

**Depends on:** S5. Needs D5.

- [ ] A child's allowance is taken from what its parent's reservation has left; nothing is
      reserved a second time in the ledger for a nested call.
- [ ] A retry is a new attempt, admitted like any other against what the run has left, with
      its own dispatch and settlement. Whoever issued the request issues the retry.

**Done when** these pass: a parent, a child and a grandchild each send one paid request and
the ledger holds one reservation and three dispatches; a child that retries a failed
request twice has three attempts, each settled by its own stage; a retry that what is left
cannot hold is refused before dispatch; two children of one parent, stopped after reading
what the parent has left and released together, cannot both take the last of it (rule 1);
what a finished child did not use is its parent's again.
**Size:** medium.

### S8. Estimates expire

**Depends on:** S4. **Reads:** rascal `crates/rascal-mask/src/estimate.rs`
(`TTL_MS`, and the stale refusals); table-manners `CONTRACT.md` section 4.5, "Binding a
call to its estimate", and `schemas/core/estimate.json`.

- [ ] A preparation carries the time it was made, from the injected clock. Admission
      refuses, as stale, a preparation a request came with that is an hour old or older,
      and one made at a time later than now. The time is beside the fingerprint, not in
      it. The rule is written in S1's ADR, or one of its own if S1 has closed.
- [ ] The hour is a constant of the core. No environment variable or test build shortens
      it, as rascal's did: a test moves the clock.

**Done when** these pass, with the clock moved by hand and no sleep (rule 6): a preparation
is admitted one millisecond before the hour; it is refused at the hour, with no run and no
reservation; one made "after now", as a clock set back gives, is refused; the same request
id sent again after the hour returns the first run and is not checked for age, because a
repeated id is looked up first; preparing again after a refusal gives a preparation that
is admitted.
**Size:** small.

### S9. `whizbang spend status` and `reset`

**Depends on:** S2. Needs D8.

- [ ] `whizbang spend status`: the allowance, what is reserved, what is spent, what is
      unresolved, and the last reset. It reads only.
- [ ] `whizbang spend reset --allowance <usd>`: writes a reset record. The records before
      it stay.

**Done when** these pass: `status` answers while another process holds the store to write;
`reset` is refused while another writer holds the store, with the holder named; after a
reset the earlier records are all still read and the balances start again from the new
allowance; no call the executor accepts can change the allowance, shown by listing every
registered action and every record kind an admission can write; `status` on a store whose
ledger holds a damaged record says so escaped and cut, with no path (rule 3).
**Size:** small.

### S10. The run's spend lines

**Depends on:** S11, S7.

- [ ] Every paid request of a run is an element of `spend.requests` in its record (B4's element), in the
      order it was dispatched, for a run of any outcome.

**Done when** these pass: the records of a completed, a failed and a cancelled run each
list every paid request with what was reserved, settled and unresolved; the sum of a run's
lines equals what the ledger holds for that run; a nested call's lines are in the child's
record and the parent's record names the child.
**Carry-over:** the row on the record's empty lists (`spend.requests`, settled spend).
**Size:** small.

### S12. A damaged spend record set aside

**Depends on:** S9.

- [ ] A reservation or a ledger record that cannot be read stops every paid admission,
      with a refusal that names the record and the command. `whizbang spend set-aside
      <name>` writes a record, under a new name, that takes its place in the fold: for a
      reservation, the amount its run's admission holds, charged as unresolved; where no
      amount can be read, the command asks for one and records that a person gave it.

**Done when** these pass: a planted damaged reservation refuses a paid call and lets a
free one through; after `set-aside` paid calls are admitted and the balances count the
amount once; the damaged content is still in the store and still named; `set-aside` of a
record that reads well is refused; the refusal and the command's output show the name
escaped and cut, and no path (rule 3); a crash before each event of the command leaves it
done or not done (rule 2).
**Carry-over:** the row on the ledger (a damaged reservation). **Size:** small.

### S13. What the ledger costs as a store ages

**Depends on:** S2.

- [ ] A measurement, kept as an ignored test: one admission and one publication in a
      store with 100,000 request ids and their reservations. Then the decision the
      carry-over row asks for, written in S1's ADR or one of its own: the cost is
      accepted; or the ledger reads a checkpoint record and what follows it; or the names
      of ids move out of the generation file, which changes ADR 0007 and gets a plan of
      its own; or ids may be forgotten, which changes `SPEC.md` section 3 and gets the
      review rounds first.

**Done when** the figures are in the ADR with the machine they were measured on; the
decision is one of the four; and if it is a checkpoint record, these pass: the fold from
a checkpoint equals the fold from the start for the table of histories of S1, and a
checkpoint is a new name, never a `put`.
**Carry-over:** the row on what every publication costs as ids accumulate. **Size:**
small. A change to where ids are named is not built in 1B.

### C1. The config: its file, its shape, its digest

**Depends on:** B0. Needs D8. **Reads:** squint `crates/squint-core/src/config.rs`,
`docs/adr/0004-toml-config.md` and `schemas/config-v1.json`; rascal
`crates/rascal-core/src/config.rs` and `schemas/rascal/config-v1.json`.

- [ ] An ADR: one TOML file; where it is found; its tables (`backends`, `spend`, `egress`,
      `models`, and `tools`, which is refused until milestone 2 gives it a shape); how
      several named backends of one capability and a default are written, with the example
      of `SPEC.md` section 4 either read as it stands or corrected in this pull request;
      that a credential is named by its variable and never held; the digest, of the
      settings and not of the file's bytes, and exactly what enters it; which settings a
      run records.
- [ ] The type and its strict reader in `crates/whizbang-core/src/config/`, and
      `docs/guides/config.md`. Nothing is built from a config yet.

**Done when** these pass: the spec's example is read to the backends it shows; an unknown
table or key, a key given twice and a value of the wrong kind are each refused, naming the
key escaped and cut and no path (rule 3); a file over the bound is refused from its length
before it is read; two files that differ in comments, spacing and the order of keys have
one digest, and a change to any setting changes it, with two fixed vectors; a value written
where a variable's name belongs, that has the shape of a key, is refused without being
shown (rule 4); every example in the guide is read by a test (rule 5).
**Size:** medium.

### C2. Keys from an env file

**Depends on:** C1. **Reads:** squint `crates/squint-core/src/env.rs` and
`crates/squint-providers/src/privacy.rs` (`credential`); `.env.example` of squint.

- [ ] An env file is read for the variables the config names and no others. A value in
      single or double quotes has them taken off; a value shorter than the least a secret
      may be is refused, naming the variable and never the value or its length beyond what
      ADR 0005 allows.
- [ ] The secrets the executor redacts are every credential the config names that has a
      value. A credential is handed only to the backend whose table names it.

**Done when** these pass: a quoted value, a value with a trailing line break and an
`export` prefix each give the bare key, shown by redacting a body that holds the bare key;
a short value is refused with the variable's name; a file over its bound, a line that is
not an assignment and a name given twice are refused, escaped and without a path (rule 3);
a marker key read this way is in no error, no `{:?}`, no record and no journal of a run
that used it (rule 4); a backend is handed its own credential and cannot ask for
another's.
**Carry-over:** the row on what redaction does not find (the config's half: quotes, the
variable named, the least length). **Size:** small.

### C3. What redaction did not find

**Depends on:** B0. **Reads:** squint `crates/squint-core/src/artifacts.rs` (`redact`).

- [ ] An ADR, which changes ADR 0005's "What redaction does not find": for each form the
      pin test lists (base64; a `\u00XX` escape; UTF-16 in either byte order; each of the
      five other forms of percent-encoding), found, or accepted with the reason.
- [ ] The forms that are found, in `crates/whizbang-core/src/contract/redact.rs`, each with
      its pin test turned round. A body known to be JSON is redacted as JSON: each string
      is decoded, searched and written again, so no escape can hide a secret or be broken
      by the marker.

**Done when** these pass: `redact::what_redaction_does_not_find_is_known` lists only the
forms the ADR accepts, each with its reason beside it; a secret in each form now found is
replaced, with one that sits across the edge of a base64 group of three; a JSON body with
a secret that ends in a backslash, and one behind an escape, is still valid JSON after it
and holds no part of the secret; a body that is not valid JSON, handed over as JSON, is
redacted as bytes and not refused; the pass over 1 MiB with ten secrets stays inside a
time the task measures and states.
**Carry-over:** the row on what redaction does not find (the forms). **Size:** medium.

### C4. The roster, the settings and the policies from the config

**Depends on:** C1, C2, S3, E2. A join task, and the last of them.

- [ ] In `whizbang-backends`, or in the binary if M1 has not merged: a config becomes a
      roster. A backend kind is registered as an action is, by code the binary and the
      tests share; the shipped binary registers none yet, and the tests register a fake
      kind.
- [ ] The executor is built with a `ConfigSource` over the config file (ADR 0007), so the
      config is read each time a call is prepared; with its per-call cap (S3), its egress
      policy (E2) and its secrets. A run records
      its resolved settings.

**Done when** these pass: a config with two fake backends of one capability and a default
gives a roster that resolves a choice and the default as ADR 0006 says; a backend whose
kind this build does not have is offered as not ready, with the reason, and never as
another backend; with no config file the binary runs `grow` as before and refuses a call
that needs a backend; a config edited between prepare and admission is `Stale::Config`; a recipient taken out of the config's `egress` table is refused on the next call; a run's `asked.settings` hold what C1's ADR says and no credential, value or
variable name (rule 4).
**Carry-over:** the row on the executor built with no backends and the digest of no bytes;
the row on the record's empty lists (`asked.settings`). **Size:** medium.

### E2. The gate: denied by default

**Depends on:** B5, S5. A join task. **Reads:** squint
`crates/squint-providers/src/privacy.rs` (`consent`).

- [ ] The gate, a wrapper in `crates/whizbang-core/src/egress/`: a request reaches a
      backend that names a recipient only when the config approves that recipient for every
      class of data the request holds. It stands before the meter, so a refused request
      reserves and dispatches nothing.
- [ ] The gate's policy is a value the executor is built with (C4 makes it from the
      config's `egress` table); an executor built without one has the empty policy, which
      approves nothing. A local backend passes whatever the policy.

**Done when** these pass: with the empty policy every request to a backend with a
recipient is refused and the fake counts none; a recipient approved for prompts and not
for source images is refused a request that holds a source image, and sent one that holds
a prompt alone; a part with no stated class is refused unless every class is approved; a
refusal answers the action with a fault, and no other backend, remote or local, is asked
in its place; a refused request has no dispatch record and no charge.
**Size:** medium.

### E3. Every hop

**Depends on:** E2. **Reads:** squint `crates/squint-providers/src/http.rs`
(`same_origin`, `MAX_REDIRECTS`).

- [ ] Everything the sending interface is asked to reach is checked against the recipient:
      the first address, a redirect, an upload address and a returned download address.
      A credential is attached only for the origin it was configured for.
- [ ] The download interface is checked the same way against the origins a catalogue entry
      names, and is refused anything else.

**Done when** these pass, against the sending fake of B3: a redirect to an origin that is
not the recipient or one of its upstreams is refused and nothing is sent to it; a redirect
to an approved upstream is followed without the credential; an upload address and a
download address outside the recipient's are each refused; a chain longer than the limit
is refused; a model's download that is redirected off its catalogue's origins is refused,
and its request carried nothing but the address.
**Size:** small.

### E4. The run's recipients

**Depends on:** E2. Needs D6.

- [ ] Every recipient a run sent data to is in its `egress` list with the classes sent. A
      send the gate refused is in the record as refused.

**Done when** these pass: a run that sent a prompt and a derived image to one recipient
lists it once with both classes; a run whose only request was refused lists the refusal
and no recipient as sent to; a local run's list is empty; an account in the list is shown
cut, and a marker credential is nowhere in the record (rule 4).
**Carry-over:** the row on the record's empty lists (`egress`). **Size:** small.

### E5. One way out

**Depends on:** B2.

- [ ] `scripts/check-egress.sh`, run by `just check`: it fails when a crate of the
      workspace names a socket or a network client of the standard library or of any
      dependency, outside the one module the ADR of B2 allows; and when `Cargo.lock` holds
      a network client that the allowed list does not name. Today the allowed list is
      empty.

**Done when** `scripts/tests/test-egress.sh` passes: a planted use of a socket in each
crate fails the check and names the file and line; a planted dependency on a network
client fails it; the tree as it is passes; the test file ends with `finished`.
**Size:** small.

### M1. A models directory: catalogue, installation, identity

**Depends on:** B2. **Reads:** rascal `tools/python/src/rascal_tools/installs.py`
(`Models`, `Installation`, `manifest_for`, `identity_of`, `verify`), `catalogue.py`,
`models/cache.py` and `tools/python/MODEL.md`; table-manners `CONTRACT.md` sections 3.5
and 5.3 and `schemas/core/installable.json`.

- [ ] An ADR: the models directory and its lock; a catalogue entry (files with their sizes
      and hashes, the origins they may be downloaded from, licence, features, and a
      revision); an installation and its record, which holds the catalogue's and the
      adapter's revisions; the installation's identity; the pointer that names the one in
      use; holds as file locks. And the two sentences of `SPEC.md` named under "Decisions".
- [ ] `whizbang-backends` is created, with the catalogue and the installation record as
      types read strictly. Nothing installs yet.

**Done when** these pass: a catalogue and an installation record each round-trip; each is
refused over its bound before it is read, and with an unknown field, a hash of the wrong
length, a file name that is not one plain name, or a size of zero, escaped and without a
path (rule 3); two records that differ in the adapter's revision alone have two
identities; the crate-graph check passes with the new crate; every claim of the ADR about
code names its test (rule 5).
**Carry-over:** the row on a model that never shows as out of date (R2), for what an
installation records. **Size:** medium.

### M2. Installing, with a crash at each step

**Depends on:** M1. **Reads:** rascal `tools/python/src/rascal_tools/installer.py` (`get`,
`_copy_bounded`, `model_lock`) and `installs.py` (`publish`); rascal
`crates/rascal-testkit/src/fake_install.rs`.

- [ ] `crates/whizbang-testkit/src/downloads.rs`: a fake of the download interface that
      serves bytes from memory, can stop at a barrier after a stated count of bytes, serve
      wrong bytes, serve too many, or fail.
- [ ] The installer: each file is downloaded into staging under the directory's lock,
      stopped at its stated size, checked by hash, then the installation is published and
      the pointer replaced, through the same file-system interface the store uses, so the
      shim can crash it.

**Done when** these pass: an install of two files ends with the pointer naming a verified
installation; a wrong hash, a short file and a file one byte too long each fail the
install and leave the previous installation named and readable; a crash before each event
the shim records, with unsynced writes and names lost, reopens to the previous
installation or the new one, whole, and the staging left behind is removed only under the
lock (rule 2); a process that dies in the middle of a download, shown with the barrier and
a crash, leaves the same; two installers over one directory, stopped after each read the
pointer, do not both publish (rule 1).
**Carry-over:** of rascal's untested cases (R6), a signal during a download. **Size:**
medium.

### M3. Attach and cancel

**Depends on:** M2. **Reads:** rascal `crates/rascal-mask/src/install.rs` (`install`, the
attach) and `crates/rascal-mask/tests/it/install_controls.rs`.

- [ ] A second request for a model that is installing, in the same process, attaches: it
      sees progress from then on and the same result. A second process is refused as busy.
- [ ] An install can be cancelled between files and inside one. A cancelled install leaves
      what was installed before.

**Done when** these pass: two requests for one model, the first stopped at a barrier, cause
one download of each file and end with one result; a real second process asking for the
same model is refused with the holder named; a cancel while the fake is stopped mid-file
ends the install as cancelled, removes its staging and leaves the previous installation
named; a cancel after the pointer was replaced returns the installed result; an attached
request that goes away does not cancel the install.
**Size:** small.

### M4. A file two models share

**Depends on:** M2. **Reads:** rascal `tools/python/src/rascal_tools/installs.py` (the
files kept once by digest).

- [ ] A file is kept once by its hash. Two installs that need the same file, running at
      the same time, download it once.

**Done when** these pass: two models that share one file are installed at once, the fake
stopped while the first downloads it, and the fake served that file once; removing one of
the two leaves the shared file for the other; removing both removes it; a crash between
the two installs leaves no file that no installation names once the next install or
removal has run.
**Carry-over:** the row on parallel installs of models sharing a file. **Size:** small.

### M5. Holds, and removal that drains

**Depends on:** M2. Needs D5. **Reads:** rascal `tools/python/src/rascal_tools/installs.py`
(`.held`, `alive`, `Slot`), `installer.py` (`remove`) and
`crates/rascal-mask/src/install.rs` (`uninstall`, `live_pid`, `terminate`).

- [ ] A request for a model takes a hold on its installation for as long as it runs: a
      shared file lock, and a count inside the process.
- [ ] Removal: new requests for the model are refused, requests waiting for it are
      cancelled, and the one running is waited for until a deadline of the injected clock.
      If the deadline passes the removal answers busy and the model takes requests again.
      Removal in another process takes the lock alone or answers busy at once.

**Done when** these pass, with barriers and the clock moved by hand (rule 6): a removal
during a running request refuses a new request, cancels a waiting one, and removes the
model when the running one ends; with the running request held past the deadline the
removal answers busy, and the next request is accepted; a real child process that holds a
model and is killed leaves no hold, and the removal then succeeds; a removal and an
install of the same model, stopped after each read the pointer and released together, end
in one of the two orders, never with a pointer to files that are gone (rule 1); a crash
before each event of a removal leaves the installation whole or gone.
**Carry-over:** rascal's R3 (a process id read from a file is signalled) and R5 (a hold
judged live by its process id), both met by holding with locks; of R6, a removal racing
an install. **Size:** medium.

### M6. Readiness from what is installed

**Depends on:** M5, C4.

- [ ] The roster's answer for a local backend comes from the models directory: installed
      and verified, not installed, out of date, damaged, or not in the catalogue. Only the
      first is ready. A model's version in what a preparation saw is its installation's
      identity.

**Done when** these pass: a configured model that the catalogue does not hold is prepared
as not ready, saying so; an installation whose recorded catalogue or adapter revision is
not the current one is reported out of date by prepare and by the listing; an installation
record that is damaged, or whose pointer names nothing, is reported damaged, never ready,
and a new install repairs it; a model reinstalled between a call and its retry gives the
retry a new preparation, the old one is refused as stale, and each attempt's run names the
installation that answered it.
**Carry-over:** rascal's R2 (reported), R4, and of R6 a corrupt installation record and a
reinstall between a call and its retry. **Size:** small.

### M7. `whizbang models list`, `get` and `remove`

**Depends on:** M3, M4, M5. Needs D8.

- [ ] The three commands, over the binary's shared wiring, and `docs/guides/models.md`.
      `list` shows each catalogue entry with its licence, its size and its state; `get`
      shows progress and can be interrupted; `remove` drains.

**Done when** these pass, through the binary with a test catalogue and the fake downloads:
`get` then `list` shows the model installed; `get` of a name the catalogue does not hold is
refused with the name escaped and cut; `list` over a directory with a damaged record
prints the others and says which is damaged, with no path (rule 3); `remove` of a model
that is not installed says so and exits 0; the shipped binary's `list` is empty and its
help says why; every example in the guide runs as a test (rule 5).
**Size:** small.

### X1. A key directly after a word character

**Depends on:** A12. Scripts only.

- [ ] `sk_shape` in `scripts/scan.sh` finds a key of its family wherever it starts, with
      what follows the prefix narrowed so that an ordinary hyphenated word is not a match.

**Done when** `scripts/tests/scan/` holds a test for each and they pass: a key directly
after a hyphen, an underscore, a letter and a digit is found, in a file and in the target
of a link; after a written-out line break and a written-out tab in a JSON string; after
`%20` and `%3D`; an ordinary hyphenated word that holds the key's first letters, joined to
twenty letters more, is not flagged; the secrets scan over the history of `main` (`just scan-range-secrets`) reports
nothing new, or each new place is dealt with in this task; the scan's time on the tree is
measured before and after and stated.
**Carry-over:** the row on the secrets pattern. If no pattern meets both the finding and
the not-flagging tests, the builder stops: accepting the gap is the owner's. **Size:**
small.

### X2. A planted registry fails `just deny`

**Depends on:** A12. Scripts only.

- [ ] `scripts/tests/test-deny.sh`: in a throwaway copy of the workspace's manifests, a
      dependency from an unknown registry and one from a git source each fail
      `cargo deny check sources`.

**Done when** the test passes for both, passes on the tree as it is, needs no network,
ends with `finished`, and is run by `just test-scripts`.
**Carry-over:** the row on `cargo deny` (deferred from milestone 0). **Size:** small.

### Z1. Exit check

- [ ] Each clause of the checkpoint 1B row in `SPEC.md` section 16 is matched to a named
      test in the note, and each of the five spend cases of section 6 by name.
- [ ] Each kept row of `SPEC.md` section 6, "Controls from table-manners", that 1B owns
      (estimates; the per-call cap and budget; model installs; removing a model in use)
      is matched to a test.
- [ ] Every carry-over row whose owner was 1B is gone, or has a new owner and a reason.
      `whizbang backends status` has its row for 1C; so do the real network client, the
      shipped catalogue, and what the worker's queue owes to removal.
- [ ] The test kit's fake ledger is removed, or kept with a line saying what still uses it.
- [ ] `just check` is inside its budget; the timing table is in the note.
- [ ] `docs/plans/m1b-note.md`: what shipped, what 1A's answers changed in this plan, the
      timing table, what was spent (nothing), what moved to the carry-over list.
- [ ] Tag `m1b`.

## Carry-over rows and their tasks

Every row of `docs/carry-over.md` whose owner names 1B, as of the commit this draft was
written on.

| Row | Task |
|---|---|
| The secrets pattern misses a key directly after a hyphen, an underscore, a letter or a digit | X1 |
| The executor is built with no backends and the digest of no bytes | C4 (the config's shape and digest: C1) |
| The run record's empty lists: `asked.settings`, `spend.requests` and settled spend; `egress` | B4 defines the elements; C4, S10 and E4 fill them. `egress` was owned by "the first backend that sends data": the gate is where a send is known, so 1B takes it, filled by fakes |
| "Never removed" has one limit left: `put` replaces a made name | S1 |
| The ledger: the real one, the binary built with it, the token, a damaged reservation, the names' segments | S1 (the token, the names), S2 (the real ledger, the binary), S12 (a damaged reservation) |
| Settlement | S11 (a run that ends), S6 (a run that did not) |
| An admission that was visible and not confirmed takes its id | S6 |
| What every publication costs as ids and reservations accumulate | S13 |
| Whether a failed call may have been charged | B1 (the stage on a fault), S5 (the settlement) |
| What a provider adapter said of itself: `pricing`, `recipient` | B1 and S4 (`pricing`); B2 and E2 (`recipient`). Image preparation stays with milestone 2 |
| What redaction does not find; quotes around a value; the variable named | C3 (the forms, UTF-16 among them, and JSON as JSON); C2 (quotes, the variable, the least length) |
| `max_spend_usd`, the cap of one call | S3. `set` stays with milestone 3 |
| Parallel installs of models sharing a file each download it | M4 |
| No test plants an unknown registry or git source for `cargo deny` | X2 |
| A model never shows as out of date (R2) | M1 (recorded), M6 (reported) |
| Uninstall signals a process id read from a file (R3) | M5 |
| A configured model that is not in the catalogue can read as ready (R4) | M6 |
| A hold is judged live by its process id alone (R5) | M5 |
| Untested cases around installs (R6) | M2 (a signal during a download), M5 (a removal racing an install), M6 (a reinstall between a call and its retry; a corrupt installation record) |

The row on what running an action does not do yet (task A7) asks for a bound on a ledger
being asked again; it stays with A7, and D10 says what S2 does with the answer.

## Spend

Nothing. Every backend, download and recipient in 1B is a fake, the fast tier is the only
tier these tasks run, and no task has a live call. What would change that: a test that
reaches a real provider or downloads a real file, or a real recipient written into a
config an agent runs with. None is in this plan, E5 fails the build that adds the first,
and either would stop the task and go to the owner as `SPEC.md` section 15 says.

## Review rounds

Recorded as `AGENTS.md`, "Working with the owner", requires.

## Risks

| Risk | Effect | What we do |
|---|---|---|
| 1A's cancellation or nested calls come out unlike either outcome written under "What this plan needs from 1A" | Tasks S5, S7, S11, B4, B5 are wrong as written | B0 is first and stops on such an answer; the correction gets the review rounds before anything is built |
| The record's shapes are fixed in B4 before the strands that fill them | A second version, `run/4`, inside one checkpoint | B4 follows B1 and B2, which decide the amount, the stage and the recipient; the shapes are the spec's table; a wrong one is a new version, never an edit |
| The join tasks queue behind one another | The parallel strands end in a line | Only four tasks touch the engine; each strand's other work is behind an interface in its own folder and merges without waiting |
| Crash tests at every event multiply | The fast tier passes its three minutes | Each task states its count of crash points in its hand-back; the exhaustive loss of single names runs as a seeded sample, as 1A's exit check decides for publication |
| Folding the balances from every record costs more with every run | Admission slows as a store ages | S13 measures it at 100,000 ids before the checkpoint closes, and a checkpoint record is the stated way out |
| A decorator order that charges for a request the gate refused | Spend with nothing sent | The order is fixed in B5 with a test; E2 asserts no dispatch record for a refused request |
| Holds by file lock behave differently on some file system | A model removed under a running request | The store's row on network file systems applies to the models directory as well; M1's ADR says so and the row gains the directory |
| Redaction of more forms costs time on every write | Slow runs | C3 measures and states the cost, and a form too dear to search is accepted in its ADR with the reason |
| Two builders edit one file after all | A conflict a builder may not resolve | The ownership table; `gh pr diff <n> --name-only` before starting; a task that needs another strand's file stops and says so |

## Needs the owner

Nothing to start. No spend, no new recipient of data, and no step that cannot be undone.

Two things would need the owner if they arose, and the task stops there:

- X1 finds no pattern that both catches the key and leaves ordinary words alone: accepting
  the gap, case by case, is the owner's (`docs/carry-over.md`).
- Anyone wants a real recipient, a real download or a real allowance in a store that is
  kept: `SPEC.md` section 15.
