# Plan: checkpoint 1B, policy and controls

Draft 6, 2026-11-13. From `SPEC.md` sections 3, 4, 6, 10, 11 and 16. This file is the task
list.

Checkpoint 1A is nearly built: everything but its two proofs (A9, `grow`; A11, the
scripted squint run) and its exit check (A12) is merged, and this draft is written from
that code and from ADRs 0001 to 0008 as they stand on `main`. Nothing here is built before
A12. Task B0 reads what A9, A11 and A12 changed and corrects this plan before any other
task starts.

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

## What this plan builds on from 1A

**Decided, and what 1B does with each** (ADR 0007 and ADR 0008 as amended to the end of
task A7; ADR 0005 for `run/2`).

| What 1A decided | What 1B does |
|---|---|
| The ledger's interface is `Ledger::reserve`, called inside admission for every attempt at its publication, with a `Step`: the generation, the reservations it names, and `Step::make` for names made once. A ledger that must not overlook another admission makes a name that admission would make too | I2 changes the interface once (a token, the names' segments, a part in the end of a run); S2 implements it |
| A reservation is `reservation/1` at `spend/reservations/<run>`, for a run that nothing called. A nested call asks no ledger and has no reservation: its admission names its `parent` and says in `reserved_micros` what it took from its parent's allowance (ADR 0008, "Allowances") | The ledger's records name the run that sent a request and the reservation of the run at the root of its tree. The tree is found from the admissions, by `parent` |
| A child's share is taken from its parent's `Allowance` in memory, under the allowance's lock; nothing is given back when the child ends; a run's own capability requests take nothing from its allowance | S7: a request is debited from its run's allowance before it is sent, and what a child did not use goes back to its parent while the parent runs |
| Money is millionths of a US dollar (`Estimate`). No bound is `Refusal::NoBound`; over the allowance is `Refusal::OverAllowance`; a child over its caller's is `Refusal::OverCaller` | B1 adds no unit. A backend with only an estimate states no bound |
| An estimate is the sum of `Backends::bound` over the call's needs, compared beside `prepared/1` at admission (`Stale::Estimate`). A request may come with the preparation it expects, a signal and a deadline | S3's cap and S8's age are checked at admission through I1, beside the fingerprint; `prepared/1` does not change |
| The config is read each time a call is prepared, through `ConfigSource`, which answers a digest. The window between that preparation and the admission's publication is accepted | I6 makes it answer one snapshot; C4 and C5 implement it |
| A run ends in one publication, which for a completed run is its commit step. Before it, the run waits for the runs it called and for every request its handles sent (`Tally`, inside the handle). A panic, a cancellation and a thread that could not be made all end the run through that publication | I2 gives the ledger its part in that publication. I3 puts the wrapper around a request inside the tally, so a request is outstanding until its settlement is durable |
| A run is cancelled by `Executor::cancel` or by its signal; a child's signal reads its parent's; deleting an executing run sets its signal | S5c drives its cancellation case through `Executor::cancel` |
| `RunState::Abandoned`: a run whose admission or end the store did not confirm, in the process that made it. To any other process it is admitted and not ended | S6 and S6b settle and end such a run at the next open to write. Ending one at once, in the process that made it, is moved to milestone 2 |
| A run reads the generation it was admitted against (`Store::hold`) | Recovery and the ledger read the current generation, never a held one |
| `start` runs an action on a thread of its own; 64 started runs at most for one store value, four started by one run, 1,024 runs in one tree | "Rules every task's tests follow", 7 to 10 |
| `run/2` at the end of A7: `identity.parent`, `steps` (calls) and `outcome.refusals` (the action or null, a kind, a sentence; calls to run an action only) are filled. Empty: `asked.settings`, `time.durations`, `models`, `egress`, `judge`, `spend.requests` | `run/3` (B4) defines the elements of `asked.settings`, `spend.requests` and `egress`, and adds kinds of refusal for a request that was not sent |
| An admitted run keeps `admission/1` and nothing else of its call until it ends | B6 keeps the prepared call with the admission |

**The open points of the earlier drafts, settled.**

| # | Was | Is |
|---|---|---|
| D5 | How requests are accounted for, the cancellation rule, a child's allowance | A `Tally` inside each handle, made in `Context::new`; the rule of ADR 0008, "Cancellation"; `Allowance::take` in memory. I3 adds the wrapper inside the tally |
| D6 | The elements of `outcome.refusals` | Defined by ADR 0008 for calls. A capability request that is refused is a fault to the action and is not recorded; B4 adds it, with no action and a kind of its own |
| D7 | The final `run/2` | As the last row above, but for what A11 fills (`models`, `time.durations`, `judge`); see D9 |
| D10 | Whether an admission can be cancelled, and a bound on a ledger being asked again | Cancelled by its request's signal or deadline, looked at before each asking. No bound on the count and no turn given: that stays on the carry-over list with milestone 2, and S2 adds none |
| D11 | Whether an admitted run keeps its call | It does not. B6 is built |
| D12 | Scheduling, and a child's admission | A thread for each started run, bounded; a child has an admission that names its parent and its share, and no reservation |

**Still open, for B0.** Only what A9, A11 and A12 can change. B0 writes the answer here
and stops if it is neither outcome.

| # | From | What is open | Assumed here | If it is otherwise |
|---|---|---|---|---|
| D8 | A9 | The one source of a store's location that every command shares | Still `--store <dir>` on every call; `whizbang spend` takes it the same way | A setting or a default exists: `spend` and the config's location follow it |
| D9 | A11, and ADR 0009 (what a run asks of a judge), which is in review and not on `main` | Where a run's handles are made, how a call's choice picks among backends, and what a run records of each request (`models`, `time.durations`) | Handles are made in `Context::new` from the backends the executor holds, and A11 records each request there, in `run/2` | They are made elsewhere, or recording is a wrapper of its own: I3 puts its wrapper where A11 put that, and B4 leaves alone whatever A11 filled |
| D13 | A12 | What `store delete` removes of a run's outputs, and how `CURRENT` and the generation files are bounded | Neither touches spend records or the names under `requests/` and `spend/` | B0 checks S6's deleted-run cases against it |

Each ADR of this plan takes the next free number when its pull request opens. The ADRs of
checkpoint 1A close with it and cannot be amended in place after its tag
(`docs/adr/README.md`), so a decision below that changes one of them is in a new ADR that
names the one it changes.

## Decisions this plan makes

Each is settled here, with its reason, and is the owner's to overturn. The last three are
the owner's own, given on 2026-11-13.

- **The allowance lives in the ledger, not in the config.** `SPEC.md` section 6 says an
  allowance is reset only by the owner's command. A budget written in a config file could be
  raised by editing the file. So `whizbang spend reset --allowance <usd>` is the only thing
  that sets or raises what a store may spend, a store with no allowance refuses every paid
  call, and the config holds only a per-call cap, which can lower what a call may spend and
  never raise it.
- **`whizbang spend reset` is a command and nothing else.** It is not an action, has no
  descriptor, and cannot be reached through the executor or, later, a door. An agent runs
  it only against a temporary store in a test (`SPEC.md` section 15).
- **Balances are a fold, not a file that is rewritten.** Every spend record has a name
  that is made once (ADR 0007). What is reserved, spent and unresolved is worked out from
  the records the current generation names.
- **A damaged spend record stops paid calls until a person acts.** The ledger does not
  guess past a record it cannot read. `whizbang spend set-aside` (S12) is the command.
- **An admitted run keeps what it was asked** (B6), and **a send note is made durable
  before data leaves for any origin** (E2, E3). "What a crashed run is recovered to",
  below, is the whole of that decision.
- **Interrupted runs are ended, and their spend settled, by the next process that opens
  the store to write.** ADR 0007 turned that down "for now" and named settlement as the
  reason to come back.
- **Models are installed by the binary's own code, in Rust, into a models directory
  outside any store.** Rascal installed from Python. Here a model must be installed and
  judged ready with no Python present (`SPEC.md` section 1, the first user story), and 1B
  comes before the pack. Weights are shared by every store on a machine, so they are not
  in a store; the directory has its own lock and its own atomic publication. `SPEC.md`
  section 3 says the store is the one owner of everything durable; M1's pull request
  brings that sentence in line (the store owns everything durable about runs; the models
  directory owns installed models), and adds "model installations" to what
  `whizbang-backends` holds. These two edits are reviewed here, with this plan.
- **An install is attached to from any process,** and removal drains across processes:
  `SPEC.md` sections 4 and 6 do not limit either to one process.
- **Holds are file locks, not process ids.** The operating system lets a lock go when its
  process dies, so nothing reads a process id from a file and nothing is signalled by one
  (rascal's R3 and R5).
- **No network client is added in 1B.** The core defines the ways out as interfaces (B2)
  and the test kit fakes them. The real client arrives with the first thing that needs
  it: a catalogue entry at 1C, a provider at milestone 2.
- **Approval of a recipient is the config's.** The build carries no list of recipients. A
  config that names none approves none. Tests name fake recipients only.
- **One version of the run record for the checkpoint:** `run/3`, defined in B4 before the
  strands that fill it. A shape a strand shows to be wrong is `run/4`.
- **`whizbang backends status` is not in 1B;** it goes to 1C with the first local backend.
  What a probe may carry and who approves it is decided and tested here (B2, E3).
- **The scan's pattern is fixed, not accepted** (X1).
- **Interfaces first (the owner's).** Seven small tasks, I1 to I7, each fix one interface
  between the executor and a strand: an ADR section, the trait or type, and a fake. After
  them the executor's files are not changed again until the final wiring, and what was a
  single line of nineteen tasks in draft 5 is built beside itself.
- **Cut and moved (the owner's).** Cut: the static scan for sockets and network clients
  (E5 of draft 5). To 1C: the `whizbang models` commands and their guide (M7), and the
  sharing of one file between two installs with its removal cases (M4). To milestone 2:
  the measurement at 100,000 request ids (S13) and the ending at once of an admission
  that was not confirmed (S6c). "Carry-over rows and their tasks" says what each row's
  new owner is; B0 edits the rows.
- **Two pull requests where a task has three kinds of work (the owner's).** A task that
  carries an ADR section, an implementation, and crash or race proofs is planned as two
  pull requests, the proofs second. The order table says which.

## What a crashed run is recovered to

Three review rounds each found a hole here, so it is stated once, whole. A process can die
at any moment. The next process that opens the store to write recovers it, in one
publication, before it admits anything (S6, S6b). What it can say of a run is exactly what
was durable, and the table is the list of everything that is.

| Fact | Made durable | By | Read by recovery for |
|---|---|---|---|
| The run, its parent, its request id, what was reserved or taken for it, its start | In the admission's publication | 1A (`admission/1`) | `identity`, `spend.reserved_micros`, `time.start_ms`; the tree, by `parent` |
| The call as prepared: action, version, arguments, inputs, the config's digest, each need with the model that would answer; the resolved settings | In the same publication, among the run's names | B6, through I1 | `asked`, `inputs` |
| The reservation, for a run nothing called | In the same publication, outside runs | 1A (`reservation/1`), S2 | What the tree may still be charged |
| Each paid attempt: the run that sent it, the root's reservation, its bound, the backend and model | Before the request leaves, outside runs | S5 (a dispatch record) | `spend.requests`, charged at the bound where no settlement names it; `models`, for those requests |
| Each attempt's settlement | When the request ends, outside runs | S5b | `spend.requests`, as settled |
| Each origin data was about to be handed to, with the classes of data | Before each handoff, the first and every later hop, among the run's names | E2, E3, through B4's durable note | `egress`, as "dispatched, with no word of whether it arrived" unless a later note says it arrived |
| The runs it called | Each child's own admission | 1A | `steps`, each with how the child ended; a child that had not ended is recovered in the same publication, as an error |

What was only in memory is lost, and the recovered record says so instead of showing an
empty list as if nothing had happened: its status is `error`, its error's kind is
`interrupted`, and `outcome.lost` (new in `run/3`, empty for every run that ended by
itself) names what cannot be known: the refusals inside the run, the durations, the
requests that cost nothing and went to no recipient, a judge's turns, the journal and the
diagnostics. Its end is the time of the recovery.

The other rules:

- **A run that was deleted** is not made again. Its tree's reservation is settled from
  the spend records, which outlive it, and nothing else is written. Its send notes went
  with it, as its record would have.
- **A child** is recovered as any run. The spend of a tree is settled once, at its root's
  reservation: every dispatch names that reservation.
- **A run abandoned in a live process** (ADR 0008) keeps its reservation unsettled, which
  errs on the side of spending less, until that process ends and the next open recovers
  it. Ending it at once is milestone 2's.
- **A run admitted by the tag `m1a`,** which kept no call, is settled and given a small
  record under the name `interrupted` among its names; `runs list` shows it as
  interrupted, with no record, and a repeat of its request is answered as ended in error.
- **A record recovery cannot read** does not stop the open: the rest is recovered, the
  ledger is marked damaged, paid calls are refused and free ones run, until S12's command.
- **Recovery is one publication** of names made once, so a crash inside it leaves it not
  done, and the next open does it. It runs once for a store value, after the writer lock
  is taken and before any work is accepted, and never when a second executor is built
  over a store that is already open.

## Rules every task's tests follow

These are the cases reviews kept finding, and what the machine's two crashes taught. A
task is not done while one that applies to it has no test.

1. **Decide where you publish.** A decision that depends on what the store holds is made
   inside the store's publishing step, or under the lock that guards the directory, never
   by a read followed by a write. Its test runs two executors over one store, or two
   installers over one models directory, stops both at a barrier after the read a wrong
   implementation would decide from, and releases them together.
2. **A crash at each step.** Every durable write is tested with a crash injected before
   each event the file-system shim records, with unsynced writes and names lost. Reopening
   shows the state before or the state after, whole.
3. **Stored bytes are not trusted.** Whatever is read back from a disk has its length
   looked at before it is read and is parsed strictly: an unknown field and a missing
   field are refused, and for a record whizbang itself wrote, any bytes but the one form
   it writes. A file a person writes, a config or an env file, may say one thing in the
   ways its format documents. A refusal shows what it quotes escaped and cut, with no
   path as text or as `{:?}`.
4. **A secret is never written.** Not as a value, not inside another value, not as a name
   or a key, not in `{:?}`, an error, a journal, a record, a status file or a refusal. A
   test plants a marker secret and searches every byte the task wrote. A name that comes
   from outside the build (a backend's or a model's name, a catalogue's file name, an
   account, a setting's key, a variable's name) is looked at whole for a secret before it
   is parsed, escaped or cut, and one that holds a secret is refused and not shown.
5. **A document says what the head does.** An ADR, a guide or a help text added by a task
   names, for each thing it claims the code does, the test that shows it.
6. **No sleeps.** A test waits on a barrier or an event. Time comes from the injected
   clock (`Clock`), which a test moves by hand.
7. **Threads only through the test kit.** A test makes a thread through
   `whizbang_testkit::threads` and no other way: the build refuses any other, and the
   helper refuses a 65th live thread.
8. **Every loop that starts something is bounded.** A loop that starts a thread, a run or
   a process has a literal bound written in it and waits for each child it started. Two
   show a race; a test that needs more says why beside it.
9. **Nothing is run to find a limit.** No test and no probe written during a task asks a
   system how many threads, processes, files or bytes it will give before it refuses.
10. **Tests are run one way.** Through `just check`, `just test-only <args>` or
    `scripts/heavy.sh`, and never by calling the test runner directly.
11. **A crash sweep is named as one.** A test that crashes before every step has
    `a_crash_at_any_event` in its name, so the `crash-sweeps` group of
    `.config/nextest.toml` takes it and runs it alone, and it takes under about 5 seconds
    alone. A sweep that cannot is cut into sweeps of a part of the steps each.

## Strands, and what each owns

| Strand | Tasks | Owns | Starts after |
|---|---|---|---|
| Base (a line) | B0, B0b, B1, B1b, B2, B3, B4, B4b, I1 to I7, and W1 at the end | The executor's files (`crates/whizbang-core/src/contract/`: `engine.rs`, `request.rs`, `prepare.rs`, `context.rs`, `ledger.rs`, `record.rs`, `live.rs`, `nested.rs`), `crates/whizbang-core/src/capability/`, the record's schema, `crates/whizbang-testkit/src/` but for the files named below, `crates/whizbang/src/wiring.rs`, every root file | A12 |
| Spend | S1 to S12, and B6 | `crates/whizbang-core/src/spend/` and `tests/it/spend/`; `crates/whizbang/src/spend.rs`; `crates/whizbang-testkit/src/spend.rs` | S1 and S1b after B1b; the rest after the interface each names |
| Config and secrets | C1 to C5 | `crates/whizbang-core/src/config/` and `tests/it/config/`; `crates/whizbang-core/src/contract/redact.rs` and its test; `docs/guides/config.md` | B0b; C4 after I6 |
| Egress | E2, E3, E4, E6 | `crates/whizbang-core/src/egress/` beyond B2's types, and `tests/it/egress/` | I5 |
| Models | M1 to M6b | `crates/whizbang-backends/`; `crates/whizbang-testkit/src/downloads.rs` | B2; M6 after I7 |
| Scripts | X1, X2 | `scripts/scan.sh`, `scripts/tests/scan/`, `scripts/tests/test-deny.sh`, `deny.toml` | A12 |

**The line, and why each task is on it.** One builder at a time works in the base
strand, because each of its tasks changes the executor's files:

- B4b, because the executor has to hand each run its place for facts and write them.
- I1 to I7, because each adds one call from the executor to an interface: at admission
  (I1), in the ledger's step and at the end of a run (I2), around a request (I3), when a
  writer opens (I4), before a send (I5), when a call is prepared (I6), and before a
  request to a local model (I7). Each is small: the call, a default that does nothing, and
  a fake. None holds policy.
- W1, because the binary is wired once, when the strands' implementations exist.

Nothing else is on it. S7 was expected to stay (a child's remainder goes back to its
parent's allowance, which is the executor's); I2 and I3 carry what it needs, so it is a
side task. If a builder of a side task finds it must change an executor's file after all,
it stops and says so, and the change is made by a base task.

**Inside the models strand** M2 creates the modules: `install.rs` and `status.rs` are
M2b's and M3's, `hold.rs` is M5's, `remove.rs` is M5b's, M5d's and M5c's, in that order,
and `inventory.rs` is M6's and M6b's.

**Root files** (`Cargo.toml`, `Cargo.lock`, `scripts/crate-graph.txt`, the `lib.rs` and
test roots of each crate, `crates/whizbang/src/main.rs`, `justfile`,
`.config/nextest.toml`) are changed by B0b, which makes every module root, the empty crate
`whizbang-backends`, the TOML reader as a dependency and the `spend` subcommand as a stub;
and after it by a base task only. `SPEC.md` is changed by C1 (the config example), M1 (the
two sentences under "Decisions") and Z1. An ADR is changed by the task that brings it and
by no other, but for the interfaces' ADR, which each of I1 to I7 adds its section to, one
at a time. `docs/carry-over.md` is changed by every task in its own rows.

## Order

Each task needs every task listed for it to be merged first. "PRs" is 2 where the task is
planned as two pull requests, the proofs second.

| Task | Strand | Needs | Size | PRs |
|---|---|---|---|---|
| B0 Entry check | Base | A12 | small | 1 |
| B0b Roots | Base | B0 | small | 1 |
| B1 What a backend says of cost | Base | B0b | small | 1 |
| B1b What a request cost, and how far it got | Base | B1 | small | 1 |
| B2 The ways out: recipients, classes of data, three interfaces | Base | B1b | small | 1 |
| B3 Fakes that quote, charge, fail at a stage and name a recipient | Base | B2 | small | 1 |
| B4 `run/3` | Base | B3 | medium | 1 |
| B4b A place for a run's facts | Base | B4 | medium | 2 |
| I1 A part in the admission | Base | B4b | small | 1 |
| I2 The ledger's token, and its part in the end of a run | Base | I1 | small | 1 |
| I3 A wrapper around each request | Base | I2 | small | 1 |
| I4 A part in the writer's recovery | Base | I3 | small | 1 |
| I5 A policy before a send, and a checked send | Base | I4 | small | 1 |
| I6 One snapshot of the config | Base | I5 | small | 1 |
| I7 An inventory, and a lease on an installation | Base | I6 | small | 1 |
| W1 The binary wired | Base | S2, S5b, S11, S6b, E2, C5, M6b | small | 1 |
| B6 An admitted run keeps what it was asked | Spend | I1 | small | 1 |
| S1 The ledger's records | Spend | B1b | small | 1 |
| S1b The fold | Spend | S1 | small | 1 |
| S2 Reserving at admission | Spend | S1b, I2 | medium | 2 |
| S3 The per-call cap, and a call's own | Spend | I1 | small | 1 |
| S4 Quotes: a bound, or no dispatch | Spend | S1b, B3 | small | 1 |
| S5 Dispatch first | Spend | S4, S2, I3 | medium | 2 |
| S5b Settlement of each attempt | Spend | S5 | small | 1 |
| S5c Settlement under cancellation and a crash | Spend | S5b | small | 1 |
| S7 Nested calls and retries | Spend | S5b | medium | 2 |
| S7b Nested calls under a crash | Spend | S7, S6b | small | 1 |
| S8 Estimates expire | Spend | I1 | small | 1 |
| S9 `whizbang spend status` and `reset` | Spend | S2 | small | 1 |
| S10 The run's spend lines | Spend | S5b, S7 | small | 1 |
| S11 The reservation settled when its run ends | Spend | S5b | small | 1 |
| S11b Settlement at the end, under a crash and a race | Spend | S11 | small | 1 |
| S6 After a crash: the ledger | Spend | S11b, I4 | medium | 2 |
| S6b After a crash: the run and its request | Spend | S6, B6 | medium | 2 |
| S12 A damaged spend record set aside | Spend | S9, S6 | small | 1 |
| C1 The config: its file, its shape, its digest | Config | B0b | medium | 1 |
| C2 Keys from an env file | Config | C1 | small | 1 |
| C3 What redaction did not find | Config | B0b | medium | 2 |
| C4 The roster from a snapshot of the config | Config | C1, C2, I6 | medium | 1 |
| C5 The cap, the policy, the secrets and the settings from the snapshot | Config | C4, C3, S3, E2 | medium | 1 |
| E2 The gate: denied by default | Egress | I5 | medium | 1 |
| E3 Every hop, and a probe | Egress | E2 | medium | 2 |
| E4 The run's recipients | Egress | E3, S6b | small | 1 |
| E6 Nothing but pixels | Egress | E2 | small | 1 |
| M1 A models directory: catalogue, installation, identity | Models | B2 | medium | 1 |
| M2 Downloading into staging | Models | M1 | small | 1 |
| M2b Publishing an installation, with a crash at each step | Models | M2 | medium | 2 |
| M3 Attach and cancel | Models | M2b | medium | 1 |
| M5 Holds | Models | M2b | small | 1 |
| M5b Removal that drains | Models | M5 | small | 1 |
| M5d Draining across processes, and against an install | Models | M5b, M3 | small | 1 |
| M5c Removal on disk | Models | M5d | small | 1 |
| M6 Readiness from what is installed | Models | M5c, I7 | small | 1 |
| M6b The hold on the admitted installation | Models | M6 | small | 1 |
| X1 A key directly after a word character | Scripts | A12 | small | 1 |
| X2 A planted registry fails `just deny` | Scripts | A12 | small | 1 |
| Z1 Exit check | | all | small | 1 |

**What runs at the same time.** The base line is sixteen tasks, nearly all small, and one
builder walks it. Beside it:

- From B0b: C1, then C2; C3; X1; X2. From B1b: S1, S1b. From B2: M1 to M5c, the whole
  models strand but its last two tasks. From B3: S4.
- From I1: B6, S3 and S8. From I2: S2, then S9. From I3: S5 and all that follows it in
  spend. From I4: S6. From I5: the egress strand. From I6: C4. From I7: M6.

So while the interface tasks are being walked, config, models, scripts and the first of
spend are already being built, and from I3 on four or five builders work at once with no
executor's file among theirs.

**Size** is read from the acceptance list: small is up to about 1,000 added lines, tests
included, and medium up to about 2,500. Tasks sized "medium" in 1A came out at 3,000 to
6,700 lines, so every medium here that mixes kinds of work is already planned as two pull
requests, and a branch that passes 2,500 added lines stops and is split along its "Done
when" lines, the first part merged first.

## Commit strategy

- One task is one branch and one pull request, or two where the table says, built by the
  builder, reviewed by a reviewer that did not write it and merged by the orchestrator
  with a merge commit, once `ci` and both scans pass.
- An ADR section is in the pull request of the task that names it, with the types it
  decides and their tests.
- Ported code names its source in the commit message: the old repo, the path, and the
  pinned commit from `docs/sources.toml`.
- **Until W1 merges, the binary behaves as it did at the tag `m1a`:** it has no ledger, no
  config, no gate and no inventory, each interface's default does nothing, and so nothing
  can be reserved, sent or installed by the shipped binary from a strand that is half
  built. Every state of `main` between the tasks below passes `just check`.
- **Rollback tag:** `m1b` at the exit check. There is no tag before it.
- **If a task shows a decision of this plan is wrong,** the decision is changed here first,
  in a pull request of its own, and if it changes scope or architecture it gets the review
  rounds first (`AGENTS.md`, "Working with the owner"). A test is never bent to fit.

### What undoing each task takes

In every row: first revert each later task that needs it (the order table shows which).

| Tasks | To undo |
|---|---|
| B0b to B4b, I1 to I7 | Foundations. A store that holds a `run/3` record is discarded with B4; before the tag `m1b` no store written by 1B is worth keeping. An interface task reverts to an executor that makes one call fewer |
| W1 | Revert the merge: the binary is as at `m1a` again, and every strand's tests still pass, because they build their own executor |
| Any side task | Revert the merge; it is inside its strand's folder, or a script with its test |
| Z1 | The note reverts; the `m1b` tag stays and a corrected checkpoint is `m1b.1` |

A ledger, a models directory or a config written by a reverted task lives only in a test's
temporary folder: no task below is run against a store or a models directory that is kept.

## Tasks

### B0. Entry check

**Depends on:** A12. **Reads:** every ADR merged since 0008, with its amendments, and the
amendments to 0005 to 0008 made since this draft; the note `docs/plans/m1a-note.md`; every
row of `docs/carry-over.md` that names 1B.

- [ ] D8, D9 and D13 are answered from the merged code, in this file, and the table of
      what 1A decided is read against the ADRs as they then stand.
- [ ] Each task below whose text no longer fits is corrected here. A correction that
      changes what is built, not how it is worded, gets the review rounds first.
- [ ] Every carry-over row whose owner is 1B is matched against "Carry-over rows and their
      tasks"; the rows that table gives a new owner are edited; a row added since this
      draft is given a task or a new owner, with the reason.

**Done when** the table has no assumption left unchecked, the carry-over rows match the
table at the head, and `just check` passes.
**Size:** small. Documents only.

### B0b. Roots

**Depends on:** B0.

- [ ] The root files of "Strands": the module roots `spend/`, `config/` and `egress/` in
      the core with their test roots, the empty crate `whizbang-backends` in the workspace
      and in `scripts/crate-graph.txt`, the TOML reader as a dependency of the workspace,
      and the subcommand `spend` as a stub.

**Done when** `just check` passes; `whizbang spend` says that it is not built and exits
with an error; the description (`whizbang describe`) and the generated reference are
unchanged; `cargo deny` accepts the new dependency's licence and source.
**Size:** small.

### B1. What a backend says of cost

**Depends on:** B0b. **Reads:** squint `crates/squint-core/src/ledger.rs` (`Pricing`,
`RequestShape`) and `crates/squint-providers/src/catalog.rs` (`Prices`, `bound_tier`);
table-manners `CONTRACT.md` section 4.5.

- [ ] An ADR, which changes ADR 0006 and names ADR 0007: a quote, in the unit ADR 0007
      fixed (millionths of a US dollar), which is one of free, an enforceable bound, or an
      estimate only; for a price by time, that only a limit the host enforces or a fixed
      maximum charge makes a bound; and how a quote becomes the answer of
      `Backends::bound` (an estimate only is no bound). It leaves to B1b what a request
      cost and how far it got, and to B2 which prices go with which recipients.
- [ ] The quote and a backend's statement of its price, in
      `crates/whizbang-core/src/capability/`. No ledger, no reservation, no fake.

**Done when** these pass: a quote cannot be made from a number that is not finite or is
negative, and one made from a fraction of the unit is rounded up; a quote of an estimate
only, asked through `Backends::bound`, is the amount that is no bound; a price by time
with only a timeout of whizbang's own gives an estimate, and with a host's limit gives the
bound of price times limit; a backend of 1A that states no price answers as it did;
`{:?}` of each new type shows no more than its numbers.
**Carry-over:** the row on squint's `pricing` (its interface half). **Size:** small.

### B1b. What a request cost, and how far it got

**Depends on:** B1. **Reads:** rascal `crates/rascal-mask/src/filler/mod.rs`
(`FillerCall`, `Charge`, `CallStarted`, `Spent`); table-manners `CONTRACT.md` section 4.5.

- [ ] In B1's ADR, by the same builder: what a failed request says of how far it got (not
      sent; answered, with what it cost when the answer says; sent with no answer that
      settles it), and what a request that succeeded cost: a response may carry a charge,
      which the adapter states or works out from the usage the answer reports at the
      prices it quoted from; a response without one cost its bound.
- [ ] The stage on a `Fault` and the charge on each response, as private fields (ADR 0006
      lets both gain members).

**Done when** these pass: each of the three stages can be read from a fault, and every
fault made before this task reads as not sent or as unresolved by the rule the ADR states,
with a test of each kind in ADR 0006's table; a response's charge cannot be made negative
or over the largest amount; a response made as 1A makes it carries no charge; `{:?}` of a
fault and of a response shows the stage and the charge and nothing new besides.
**Carry-over:** the row on whether a failed call may have been charged (its interface
half). **Size:** small.

### B2. The ways out: recipients, classes of data, three interfaces

**Depends on:** B1b. **Reads:** squint `crates/squint-providers/src/privacy.rs` (`Consent`,
`Credential`, `redact_account`) and `http.rs` (`same_origin`); squint
`crates/squint-core/src/provider.rs` (`recipient`); `SPEC.md` section 10, "Egress".

- [ ] An ADR: a recipient (an endpoint's origin, the account used with it, the upstreams it
      may forward to); the four classes of data (source images, derived images, prompts,
      evidence) and who states the class of each part of a request; that a part with no
      stated class needs every class approved; the interface through which data is sent,
      and the separate one through which a model's file is downloaded, which has no place
      for user data or for a credential bound to another origin; a third, a probe of a
      backend, which carries no user data either and is approved apart from sending; which
      declarations go together (no recipient and no price is local and free; a recipient
      and no price has no bound and is refused as any paid request without one is; free
      for a backend with a recipient is said, never assumed); that a send note is made
      durable before data leaves for any origin; where a backend says who
      receives its requests.
- [ ] The types and the three interfaces in `crates/whizbang-core/src/egress/`, and the
      recipient in a backend's declaration. A backend that names no recipient is local.
- [ ] No enforcement in this task: the gate is E2.

**Done when** these pass: a recipient cannot be made from an address with a user, a
password, a query or a fragment in it, and its `{:?}` shows the origin and a cut account
and nothing else; two origins that differ in scheme, host or port are two recipients; the
download interface's request type cannot be built with a body, a picture or a prompt (a
test that fails to compile is not used: the type has no such field, and a test lists its
fields); the probe's request type has none either; a declaration with no recipient reads as
local; a backend that names a recipient and states no price answers `Backends::bound`
with the amount that is no bound, and one that says it is free answers zero; an account
or an origin that holds a secret the constructor was told of is refused whole, before it
is cut, and not shown (rule 4).
**Carry-over:** the row on squint's `recipient`. **Size:** small.

### B3. Fakes that quote, fail at a stage and name a recipient

**Depends on:** B2.

- [ ] In `crates/whizbang-testkit/src/capabilities.rs`: each fake can be built with a price
      (free, a bound, an estimate only, a price by time), to report a charge with its
      answer (under the bound, none, over the bound), with a recipient, and to fail at
      a named stage. The fake filler of 1A already stops at a named barrier and sees the
      cancellation; the other two gain the same.
- [ ] A fake that sends: it passes scripted hops (a redirect, an upload address, a returned
      download address) through the sending interface of B2, and keeps what it was asked
      to send to each.

**Done when** these pass: a fake built with each kind of price quotes it for a request and
counts the request once whether it answers or fails; a fake told to fail at each stage
gives a fault that says that stage; a fake stopped at its barrier holds its request open
until the test releases it and sees the run's cancellation meanwhile; the sending fake
records each hop in order with the headers it would have carried.
**Size:** small.

### B4. `run/3`

**Depends on:** B3. **Reads:** ADR 0005; ADR 0008, "What is refused inside a run is in its
record".

- [ ] An ADR, which changes ADR 0005: `run/3`. The element of `asked.settings` (a name and
      a value, as an argument is written, in a stated order); of `spend.requests`, one
      paid request (reserved, settled, unresolved, the stage it reached, and whether its
      charge was over its bound); of `egress` (the recipient, the classes of data, and one
      of three states: sent, refused, or dispatched with no word of whether it arrived);
      the kinds a refusal gains for a capability request that was not sent, with no
      action; `outcome.lost`, as "What a crashed run is recovered to" says; and the error
      kind `interrupted`. What reading does with a `run/2` record: it is read, as frozen
      at `m1a`.
- [ ] The types, the schema `run-3.json` and fixed vectors. The executor writes `run/3`
      with the new lists empty.

**Done when** these pass: a `run/3` record with each new element round-trips and validates,
and the schema and the types refuse the same faults, one document for each (ADR 0005's
three ways); a `run/2` record written by the tag `m1a` is read and shown; `outcome.lost`
with anything in it is refused unless the status is `error` and the kind `interrupted`; a
recipient's account and a setting's value pass through redaction before they are written
(rule 4); what A11 filled of `models` and `time.durations` is as A11 left it.
**Size:** medium.

### B4b. A place for a run's facts

**Depends on:** B4. Two pull requests: the place and the notes at the end of a run; then
the durable note with its crash proofs.

- [ ] The executor gives each run a place (`RunFacts`) in which a setting, a spend line, a
      refusal of a request and a recipient can be noted from another module, and writes
      what was noted when the run ends.
- [ ] A note can also be made durable at once, among the run's names, before the act it is
      about, in a publication of its own that says the run must be there. That is what a
      send note is.

**Done when** these pass: a fact of each kind noted from a test's module is in the run's
record; a refusal of a request noted inside a run whose action then completes is in that
run's `outcome.refusals`, within ADR 0008's bound of 256; a durable note is in the store
before the call that made it returns; `durable_note::a_crash_at_any_event…` leaves the
note whole or absent and the run as it was (rule 2); a durable note is read strictly and
is bounded, and a run holds at most a stated number of them, one more refused (rule 3); a
note for a run that was deleted meanwhile is refused and makes no run.
**Size:** medium.

### I1. A part in the admission

**Depends on:** B4b. The first section of the interfaces' ADR, which changes ADR 0007.

- [ ] `AdmissionPart`, in the core: the executor holds a list of parts. Each is shown the
      request, the preparation made at admission and the time, before each attempt at the
      admission's publication; it may refuse, with a refusal of its own kind, and it may
      add records among the run's names to that publication. A refusal by a part is a
      refusal after the look, so the id is looked up once more (ADR 0007).
- [ ] The executor calls its parts; with none it behaves as before. A fake part in the
      test kit that refuses, or adds a record, on demand.

**Done when** these pass: a part's record is in the store with the run or not at all,
shown by refusing each call of the admission's publication in turn; a part that refuses
leaves no run, no reservation and the request's id free; parts are asked in the order they
were given and the first refusal is the answer; a part's record that holds a secret is
refused and not shown (rule 4); every test of `admit.rs` passes unchanged.
**Size:** small.

### I2. The ledger's token, and its part in the end of a run

**Depends on:** I1. A section of the interfaces' ADR, which changes ADR 0007's interface
and ADR 0008's allowance. **Reads:** `docs/carry-over.md`, the row on the ledger.

- [ ] `Ledger::reserve` cannot return without having made its name: what it returns can
      only be had from `Step::make`. A name with an empty, `.` or `..` segment is refused.
- [ ] `Ledger::settle`: called inside the publication that ends a run, for a run nothing
      called, with the run, its reservation and a way to add records made once. A failure
      of it is a failure of that publication: the run is not reported as ended.
- [ ] `Allowance` gains what S7 needs: an amount can be debited before a request and given
      back, and what a child did not use is handed to its parent's allowance when the
      child's end is durable, if the parent is still executing. The amounts are whatever
      the wrapper of I3 reports; with no wrapper nothing is debited, as today.
- [ ] The store's rule for a `put` under `requests/` and `spend/`: refused, or kept with
      the reason, as this section decides.
- [ ] The test kit's fake ledger brought in line; its `settle` does nothing.

**Done when** these pass: a ledger that returns from `reserve` without making its name
does not compile, or is refused, as the section says which; a name with an empty, `.` or
`..` segment is refused; a `settle` that fails leaves the run admitted and its caller told
of the failure; the pin test `known_a_name_that_was_made_can_still_be_replaced_by_put` is
turned round, or kept with the reason; an amount given back to an allowance whose run has
entered its commit step is dropped, not added; every test of `admit.rs`, `nested.rs` and
`commit.rs` passes against the fake as changed.
**Carry-over:** the rows on "never removed" and `put`, and on the ledger (the token, the
names). **Size:** small.

### I3. A wrapper around each request

**Depends on:** I2. Needs D9. A section of the interfaces' ADR, which changes ADR 0006.

- [ ] `Around`, in the core: what stands between a handle and its backend. For each
      request it is told the run, the run at the root of its tree, the backend's
      declaration and quote, the classes of data the request holds, and the run's
      allowance and place for facts. Before the request it may refuse, with a fault and a
      refusal to note; after it, it is handed the answer or the fault.
- [ ] The order is fixed in `Context::new`: the handle's own checks (ADR 0006), then the
      tally, then the wrappers in three named places (the gate, the lease, the meter),
      then the backend. The tally is outside them, so a request is outstanding, and its
      run and every run above it not terminal, until the last wrapper has returned.
- [ ] Each place is empty by default. A fake wrapper in the test kit records what it saw
      and can refuse or stop at a barrier.

**Done when** these pass: three fake wrappers see a request in the stated order, and none
sees one the handle's own checks refused; a wrapper that refuses stops the request before
the next sees it and before the backend, and the fault reaches the action; with a wrapper
stopped at a barrier after the backend answered, the run and its parent are not terminal,
and are once it is released; a wrapper's refusal is in the run's `outcome.refusals`; with
no wrapper, every test of `cancel.rs` and `nested.rs` passes unchanged.
**Size:** small.

### I4. A part in the writer's recovery

**Depends on:** I3. A section of the interfaces' ADR, which changes ADR 0007 (the
alternative it turned down for now).

- [ ] `RecoveryPart`, in the core: when an executor is first built over a store value
      that was opened to write, and before it accepts a call, each part is shown the
      current generation and adds to one publication. Parts may mark the store as
      holding damage, which the executor then answers to those who ask.
- [ ] It runs once for a store value. An executor built over a read of a store, and a
      second executor over the same store value, recover nothing.

**Done when** these pass: a fake part's records are published before the first call is
admitted, and all of the parts' records together or none, shown by refusing each call of
the publication; a second executor over the same store value does not ask the parts
again, shown with a run of the first still executing, which is left alone; a reader asks
no part; a part that fails leaves the store as it was and the executor refusing calls,
with the failure said without a path (rule 3).
**Size:** small.

### I5. A policy before a send, and a checked send

**Depends on:** I4. A section of the interfaces' ADR.

- [ ] `DispatchPolicy`, in the core: a value the executor holds, asked whether a recipient
      may receive given classes of data, and whether it may be probed. The default
      approves nothing. The gate's place in I3's order is given this policy.
- [ ] `Sender`, the sending interface of B2, as the executor hands it to a backend: every
      origin it is asked to reach goes through one check and one durable note (B4b)
      before data is handed over. The check and the note are E2's and E3's; here they are
      a default that refuses everything, and a fake.

**Done when** these pass: with the default policy a fake backend that names a recipient is
refused at the gate's place and one that names none is sent; a `Sender` with the default
check hands nothing to any origin; a fake check that approves sees each origin before the
sending fake does; the policy's `{:?}` shows recipients cut and no credential (rule 4).
**Size:** small.

### I6. One snapshot of the config

**Depends on:** I5. A section of the interfaces' ADR, which changes ADR 0007's
`ConfigSource`.

- [ ] `ConfigSource` answers a snapshot or a refusal. A snapshot is the digest together
      with what it is the digest of: the roster, the per-call cap, the dispatch policy,
      the secrets, and the settings a run records. A call is prepared under one snapshot;
      admission prepares again under the snapshot current then (ADR 0007); the run then
      executes under that one, whatever the file becomes.
- [ ] Secrets are kept for redaction for as long as the executor lives: a value a later
      snapshot no longer holds is still taken out of what is written.
- [ ] A digest alone is still a source, of a snapshot that holds nothing else, so the
      binary and 1A's tests are unchanged. A fake source in the test kit that a test
      swaps.

**Done when** these pass: a source swapped between prepare and admission is
`Stale::Config`; a source that refuses makes the next call refused with no run, and the
call after it is put right is admitted; a run admitted under one snapshot resolves a
backend, and is held to a cap and a policy, of that snapshot though the source was swapped
while it ran; a secret of an earlier snapshot is still redacted from a later run's record
(rule 4).
**Size:** small.

### I7. An inventory, and a lease on an installation

**Depends on:** I6. A section of the interfaces' ADR.

- [ ] `Inventory`, in the core: for a local model, whether it is ready and, if so, the
      identity of its installation, which is the version in what a preparation saw; or
      why it is not (not installed, out of date, damaged, not in the catalogue).
- [ ] `Lease`: before a request to a local model leaves, at the lease's place in I3's
      order, a lease on the installation the run was admitted with is taken, and let go
      when the request is over. A lease that cannot be had is a fault and a noted
      refusal; nothing is dispatched.
- [ ] Defaults that know no model, and a fake inventory in the test kit.

**Done when** these pass: a roster over the fake inventory prepares a model that is not
ready as not ready, with the reason; a request whose lease is refused reaches neither the
meter's place nor the backend; a lease is let go when its request ends by an answer, a
fault and a panic, counted by the fake; the lease is asked for the identity the admission
recorded, not the one current at the request.
**Size:** small.

### W1. The binary wired

**Depends on:** S2, S5b, S11, S6b, E2, C5, M6b.

- [ ] `wiring::runner` builds the executor with the real ledger, the config's source, the
      admission's parts (B6, S3, S8), the wrappers (the gate, the lease, the meter), the
      recovery's parts and the inventory. With no config file it has no backend, as
      before.

**Done when** these pass, through the binary: with no config, `grow` runs as at `m1a` and
its run has a reservation of nothing; with a config that names a fake backend kind
registered by the test, a paid call is refused without an allowance and admitted with
one; a store left by a killed process is recovered by the next command that opens it to
write, and not by one that reads; the shipped description does not change.
**Size:** small.

### B6. An admitted run keeps what it was asked

**Depends on:** I1. **Reads:** ADR 0007, "Two records" and "Admission". An admission's
part, in `spend/`, with its section in S1's ADR.

- [ ] In the admission's publication, among the run's names: a record of the call as it
      was prepared at admission. The action, its version, the arguments, the inputs, the
      config's digest, each need with the model that would answer, and the resolved
      settings of the snapshot. It is versioned, written one way and read strictly, and
      the secrets are taken out of it as they are out of a run record.

**Done when** these pass: a run that was admitted and did not end can be given, from the
store alone, the `identity`, `asked` and `inputs` of a `run/3` record that validates;
the settings are those of the snapshot at admission though the source was swapped since;
a marker secret in an argument is not in the record (rule 4); the record is refused over
its bound before it is read, and with an unknown or a missing field, escaped and cut and
without a path (rule 3); a nested call keeps its own; a run admitted without one, as by
the tag `m1a`, is read and listed as before.
**Size:** small.

### S1. The ledger's records

**Depends on:** B1b. **Reads:** ADR 0007, "The reservation"; ADR 0008, "Allowances";
squint `crates/squint-core/src/ledger.rs` (`Ledger`, `CapLedger`, `Account`, `Hold`,
`Closing`) and `crates/squint-eval/src/eval_ledger.rs`; rascal
`schemas/rascal/ledger-v1.json`.

- [ ] An ADR, the ledger: its own records (an allowance set or reset, a dispatch, a
      settlement, a release, an interruption, a setting aside) and their names under
      `spend/`, each made once; that a dispatch names the run that sent it, the
      reservation of the run at the root of its tree, its bound, and the backend and
      model; how the balances are folded from them and from the reservations; the name
      every admission makes so that two cannot both take the last of the allowance; the
      name every dispatch makes so that two requests under one reservation cannot both
      take the last of it; what is done with a charge over its bound.
- [ ] The record types, written one way and read strictly, in
      `crates/whizbang-core/src/spend/`. Nothing uses them yet.

**Done when** these pass: each record round-trips to the same bytes; a record over its
bound is refused from its entry's length, before it is read; one with an unknown field, a
missing field or an amount written a second way is refused; a refusal is escaped, cut and
without a path (rule 3); a record whose name does not agree with what it holds is refused;
every claim of the ADR about code that exists names its test (rule 5).
**Size:** small.

### S1b. The fold

**Depends on:** S1. In `spend/` only.

- [ ] The fold: from the records and reservations a generation names, what is allowed,
      reserved, spent and unresolved, for the store, for one reservation, and for one run
      under it.

**Done when** these pass: the fold gives the stated balances for a table of histories that
includes a reset, a release, an unresolved charge, an interruption, a setting aside, a
charge over its bound and dispatches by a child under its root's reservation; a history
with a record the fold cannot place (a settlement with no dispatch, two settlements of one
dispatch) is refused, not summed; the fold reads records through names only, shown with
content in `objects/` that no name holds.
**Size:** small.

### S2. Reserving at admission

**Depends on:** S1b, I2. Two pull requests: the ledger; then its race and crash proofs.

- [ ] The real ledger implements `Ledger::reserve`: it folds the balances from the step's
      generation, declines over the allowance, and makes its name.
- [ ] A store with no allowance set declines every call whose estimate is over zero.

**Done when** these pass, each with an executor a test builds with the real ledger: two
executors over one store, each told yes for the last of the allowance and held at a
barrier inside the ledger, end with one admitted and the other asked again and declined
(rule 1); `reserve::a_crash_at_any_event…` leaves the run, the id's record, the
reservation and the ledger's record together or none (rule 2); a call with an estimate of
zero is admitted in a store with no allowance, and one over zero is refused there with
`OverAllowance`; a call with no bound is refused with `NoBound`; every test of `admit.rs`
passes with the real ledger in the fake's place; prepare makes no reservation and no
record; a nested call asks the ledger nothing.
**Carry-over:** the row on the ledger (the real one). **Size:** medium.

### S3. The per-call cap, and a call's own

**Depends on:** I1. **Reads:** rascal `crates/rascal-core/src/config.rs` (`LimitsConfig`)
and `crates/rascal-mask/src/describe.rs` (`common_properties`, `max_spend_usd`);
table-manners `CONTRACT.md` section 4.5. An admission's part, in `spend/`.

- [ ] A cap on what one call may reserve, from the snapshot (C5 reads it from the config;
      until then a test gives it). A request may carry a lower cap of its own; I1's
      section says where a request carries it.

**Done when** these pass: a call whose estimate is over the cap is refused before a run
exists; a call's own cap lowers the cap for that call; a call's own cap above the
configured one is refused, not lowered; a cap of zero is refused as not a cap; a refusal
by the cap makes no run and no reservation and leaves the request's id free; a nested call
is held to the cap too; the two fixed vectors of `prepared/1` are as they were.
**Carry-over:** the row on `max_spend_usd`. **Size:** small.

### S4. Quotes: a bound, or no dispatch

**Depends on:** S1b, B3. **Reads:** ADR 0007, "The estimate"; squint
`crates/squint-core/src/ledger.rs` (`plan`, `Envelope`, `PlannedRequest`) and
`crates/squint-eval/src/estimate.rs`; rascal `crates/rascal-mask/src/estimate.rs`.

- [ ] A wrapper in `spend/` around any `Backends` answers `Backends::bound` from the
      quotes of the backends it resolves (B1); the roster's own file is not changed. The
      bound a backend states for a need covers every request the call sends it.
- [ ] The check a request is held to when it is about to be sent, against what its run
      has left: a function of `spend/` that S5's meter calls.

**Done when** these pass: a call that needs a backend offering only an estimate is refused
at admission with `NoBound`, with no run; the check, asked directly, refuses a request to
such a backend and one whose quote is over what a run has left; a price that changed
since the preparation is `Stale::Estimate` at admission; a backend priced by time with a
host's limit is admitted at price times limit, and with only a timeout of whizbang's own
is refused.
**Carry-over:** the row on squint's `pricing` (where a price is stated). **Size:** small.

### S5. Dispatch first

**Depends on:** S4, S2, I3. Two pull requests: the meter's dispatch; then its race and
crash proofs.

- [ ] The meter, a wrapper for I3's last place: before a paid request leaves, it is quoted
      (S4), its bound is debited from its run's allowance (I2), and its dispatch record is
      made durable in a publication that also makes the dispatch's name under its root's
      reservation, so the store decides which of two requests took what was left.

**Done when** these pass: the fake, stopped at its barrier, reads the store and finds its
own dispatch record durable before it was called; a request to a backend that offers only
an estimate, from inside a run, is refused without being sent and without a dispatch
record, counted by the fake; a request whose quote is over what the run has left is
refused the same way; a run whose action catches such a refusal and then completes has
the refusal in its record, with what was refused and why; a free request to a local
backend makes no record. Then: two requests of one run, each within what is left alone and
not together, stopped after reading what is left and released together, end with one
dispatch durable and one request at the fake (rule 1); `dispatch::a_crash_at_any_event…`
leaves the record whole or absent, and nothing was sent without it (rule 2).
**Size:** medium.

### S5b. Settlement of each attempt

**Depends on:** S5. **Reads:** rascal `crates/rascal-mask/src/filler/mod.rs` (`Charge`,
`spent`); squint `crates/squint-core/src/ledger.rs` (`settle_with_unresolved`).

- [ ] When a request ends, the meter settles it by the stage it reached, and gives back to
      the run's allowance what the bound held beyond the charge. Not sent: nothing is
      charged. Answered: the cost the answer gives, or the bound when it gives none.
      Unresolved, a timeout among them: the bound, counted as unresolved.

**Done when** these pass: a request that succeeds with a charge under its bound is charged
that, and the run can spend the difference on its next request; one that succeeds with
none is charged its bound; one whose charge is over its bound is settled as S1's ADR says
and marked as over; a fault at each of the three stages settles as stated, one test each;
a request that times out is charged its bound.
**Carry-over:** the row on whether a failed call may have been charged (its ledger half).
**Size:** small.

### S5c. Settlement under cancellation and a crash

**Depends on:** S5b. In `spend/` and its tests only.

**Done when** these pass: a run cancelled after dispatch, through `Executor::cancel`,
whose request then ends with no answer that settles it, is charged the bound as
unresolved; one whose request then answers with a charge is charged that charge; in both,
with a barrier after the backend returned and before the settlement is durable, the run
and its parent are not terminal, and are once it is; `settle::a_crash_at_any_event…`
leaves the record whole or absent, and the balances folded after it count the request once
(rule 2).
**Size:** small.

### S7. Nested calls and retries

**Depends on:** S5b. Two pull requests: a run's own requests and a child's remainder;
then retries and the race.

- [ ] A run's own requests draw on its allowance: the meter's debit (S5) is what holds a
      parent that gave its children all it had from sending requests of its own.
- [ ] When a child ends, what it did not use (its share less what its tree was charged or
      holds unresolved) goes back to its parent's allowance while the parent runs (I2).
      Nothing is reserved in the ledger for a child, before or after.
- [ ] A retry is a new attempt, held to what the run has left, with its own dispatch and
      settlement. Whoever issued the request issues the retry.

**Done when** these pass: a parent, a child and a grandchild each send one paid request
and the ledger holds one reservation and three dispatches, each naming that reservation;
a parent that gave a child all of its allowance is refused a request of its own, and is
sent one after the child ended having used nothing; a parent that calls 20 children in
turn, each given half of what is left and using none, still has all of it at the end; a
child that retries a failed request twice has three attempts, each settled by its own
stage; a retry that what is left cannot hold is refused before dispatch; a retry asks the
backend its run was admitted with and no other. Then: two children of one parent, stopped
after reading what the parent has left and released together, cannot both take the last
of it, and two requests of two children cannot both take the last of the root's
reservation (rule 1).
**Carry-over:** the row on settlement (a child's share not given back; a run's own
requests not debited). **Size:** medium.

### S7b. Nested calls under a crash

**Depends on:** S7, S6b. In `spend/` and its tests only.

**Done when** these pass, each with a crash and the next open to write: a crash while a
child is admitted, while a child's request is dispatched, while it is settled, and after a
child ended and before its parent did; in each the store is charged once for each
dispatched request, at its bound where it was not settled, the root's reservation is
settled once, and the parent and the child are both ended as errors unless they had ended;
a child that completed before the crash keeps its outputs and its record, and its parent's
recovered record lists it as completed; the same with the parent deleted before the
restart, where the ledger is settled, the child is ended, and no parent is made.
**Size:** small.

### S8. Estimates expire

**Depends on:** I1. **Reads:** rascal `crates/rascal-mask/src/estimate.rs` (`TTL_MS`, and
the stale refusals); table-manners `CONTRACT.md` section 4.5, "Binding a call to its
estimate", and `schemas/core/estimate.json`. An admission's part, in `spend/`.

- [ ] A preparation carries the time it was made, from the injected clock; I1's section
      says where, beside the fingerprint and not in it. Admission refuses, as stale, a
      preparation a request came with that is an hour old or older, and one made at a
      time later than now.
- [ ] The hour is a constant of the core. No environment variable or test build shortens
      it, as rascal's did: a test moves the clock.

**Done when** these pass, with the clock moved by hand and no sleep (rule 6): a preparation
is admitted one millisecond before the hour; it is refused at the hour, with no run and no
reservation; one made "after now", as a clock set back gives, is refused; the same request
id sent again after the hour returns the first run and is not checked for age, because a
repeated id is looked up first; preparing again after a refusal gives a preparation that
is admitted; a request that came with no preparation is not checked for age.
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

**Depends on:** S5b, S7.

- [ ] Every paid request of a run is an element of `spend.requests` in its record, noted
      by the meter in the run's place for facts, in the order it was dispatched, for a run
      of any outcome.

**Done when** these pass: the records of a completed, a failed and a cancelled run each
list every paid request with what was reserved, settled and unresolved; the sum of a run's
lines equals what the fold holds for that run; a nested call's lines are in the child's
record and the parent's record names the child.
**Carry-over:** the row on the record's empty lists (`spend.requests`, settled spend).
**Size:** small.

### S11. The reservation settled when its run ends

**Depends on:** S5b.

- [ ] The real ledger implements `Ledger::settle` (I2): in the publication that ends a run
      nothing called, what the requests of its tree were charged is spent, what is
      unresolved stays counted, and the rest is released.

**Done when** these pass: a run that reserved ten and spent three leaves seven for the next
call, with one test for each way a run ends: completed, error, invalid, a panic in its
action, cancelled, and a thread that could not be made; a run that spent nothing releases
all of it; a run with an unresolved request releases only what is not unresolved; a nested
call's end settles nothing in the ledger.
**Carry-over:** the row on settlement (a run that ends). **Size:** small.

### S11b. Settlement at the end, under a crash and a race

**Depends on:** S11. In `spend/` and its tests only.

**Done when** these pass: `end::a_crash_at_any_event…` leaves the run admitted with its
reservation whole, or ended with its settlement (rule 2); two runs that end at once over
one store, held at a barrier inside the ledger, are both settled and the balances count
each once (rule 1); a settled reservation is never settled a second time, shown by ending
a run whose settlement is already named; a run whose end the store refused, which is
abandoned, keeps its reservation unsettled and is not counted as released.
**Size:** small.

### S6. After a crash: the ledger

**Depends on:** S11b, I4. Two pull requests: the recovery; then its crash proofs and the
damaged ledger. A recovery's part, in `spend/`.

- [ ] For every reservation with no settlement whose run is not open in this process:
      each request under it that was dispatched and not settled is charged at its bound
      as unresolved, and the rest is released. Nothing is sent again.
- [ ] A record the recovery cannot read does not stop the open. What can be read is
      recovered, the ledger is marked damaged, and while it is, every call with an
      estimate over zero is refused, naming the record and the command (S12); calls that
      cost nothing run.

**Done when** these pass: a process that died between dispatch and settlement leaves,
after the next open to write, that request charged at its bound and the rest released;
the fake counts no second request; the same after the run was deleted and the store pruned
before the restart: the ledger is settled and no run is made; a run that was abandoned,
its end refused by the store, is settled by the next open. Then:
`recover::a_crash_at_any_event…`, followed by another open, settles once and no more
(rule 2); a reader that opens first writes nothing; with a damaged reservation planted,
the open succeeds, the others are recovered, a paid call is refused and a free one runs.
**Carry-over:** the row on settlement (a run that did not end). **Size:** medium.

### S6b. After a crash: the run and its request

**Depends on:** S6, B6. Two pull requests: the recovered record; then the cases through
a deletion, an old store and an unconfirmed admission. A recovery's part, in `spend/`.

- [ ] In the recovery's publication, every run that was admitted, did not end and is
      still in the store is ended as "What a crashed run is recovered to" says: a `run/3`
      record made from B6's record, its admission, the dispatch and settlement records
      that name it, the durable notes among its names, and the admissions that name it as
      their parent; with `outcome.lost`; and for a request with an id, the id's ending.
- [ ] A run admitted without B6's record is given the small record `interrupted` and, with
      an id, an ending.

**Done when** these pass: a recovered run's record validates and holds its arguments, its
settings as they were at admission though the config was replaced before the restart, its
request charged at the bound, a recipient a durable note was planted for in the third
state, and `outcome.lost` naming what was in memory; a repeat of its request is answered
`Ended`, with the error `interrupted`; a marker secret is in no byte the recovery wrote
(rule 4). Then: for a run deleted and pruned before the restart, with a request id, the
repeat is answered `Removed` and no run exists, and without an id nothing but the ledger
changed; a run admitted by a store of the tag `m1a` is settled, marked and listed as
interrupted with no record; an admission that was visible and not confirmed, in a process
that then died, is recovered as any admitted run and its repeat answered `Ended`.
**Carry-over:** the row on an admission that was visible and not confirmed, as far as the
next open; ending it at once goes to milestone 2. **Size:** medium.

### S12. A damaged spend record set aside

**Depends on:** S9, S6.

- [ ] `whizbang spend set-aside <name>` writes a record, under a new name, that takes a
      damaged record's place in the fold: for a reservation, the amount its run's
      admission holds, charged as unresolved; where no amount can be read, the command
      asks for one and records that a person gave it.

**Done when** these pass: with a planted damaged reservation, and again with a planted
damaged settlement, the command opens the store to write though recovery found the
damage; after `set-aside` the recovery completes, paid calls are admitted and the
balances count the amount once; the store closed and opened again is not marked damaged;
the damaged content is still in the store and still named; `set-aside` of a record that
reads well is refused; the refusal and the command's output show the name escaped and
cut, and no path (rule 3); `set_aside::a_crash_at_any_event…` leaves it done or not done
(rule 2).
**Carry-over:** the row on the ledger (a damaged reservation). **Size:** small.

### C1. The config: its file, its shape, its digest

**Depends on:** B0b. Needs D8. **Reads:** squint `crates/squint-core/src/config.rs`,
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
shown (rule 4); a backend's name or a setting's key that holds a secret of the env file is refused
whole and not shown, also where the cut of a refusal would fall inside it (rule 4); every
example in the guide is read by a test (rule 5).
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
a malformed line whose name part holds a value read from another line is refused without
showing it;
a marker key read this way is in no error, no `{:?}`, no record and no journal of a run
that used it (rule 4); a backend is handed its own credential and cannot ask for
another's.
**Carry-over:** the row on what redaction does not find (the config's half: quotes, the
variable named, the least length). **Size:** small.

### C3. What redaction did not find

**Depends on:** B0b. **Reads:** squint `crates/squint-core/src/artifacts.rs` (`redact`).

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

### C4. The roster from a snapshot of the config

**Depends on:** C1, C2, I6.

- [ ] A `ConfigSource` over the config file and its env file: each time it is asked it
      reads both and answers a snapshot (I6), or a refusal when either cannot be read as
      what it is.
- [ ] A snapshot's roster. A backend kind is registered as an action is, by code the
      binary and the tests share; the shipped binary registers none yet, and the tests
      register a fake kind.

**Done when** these pass, each with an executor a test builds over the source: a config
with two fake backends of one capability and a default gives a roster that resolves a
choice and the default as ADR 0006 says; a backend whose kind this build does not have is
offered as not ready, with the reason, and never as another backend; a config whose
backend changed between prepare and admission is `Stale::Config`, on the same executor; a
config replaced by a file that does not parse refuses the next call, with no run, naming
the key escaped and cut and no path, and the call after the file is put right is admitted
(rule 3); a run that was admitted goes on under the snapshot it was admitted with.
**Carry-over:** the row on the executor built with no backends and the digest of no bytes
(W1 wires it into the binary). **Size:** medium.

### C5. The cap, the policy, the secrets and the settings from the snapshot

**Depends on:** C4, C3, S3, E2.

- [ ] The snapshot carries the per-call cap (S3), the egress policy (E2), the secrets (C2,
      C3) and the settings a run records, each read from the config.

**Done when** these pass, each on one executor across a change of the file: a cap lowered
in the config refuses the next call that is over it; a recipient taken out of the `egress`
table is refused on the next call; a credential rotated in the env file is the one the
next call's backend is handed, and both the old and the new value are redacted from what
is written after it (rule 4); a run's `asked.settings` hold what C1's ADR says and no
credential, value or variable name; a marker key is in no byte the store holds after
these runs.
**Carry-over:** the row on the record's empty lists (`asked.settings`). **Size:** medium.

### E2. The gate: denied by default

**Depends on:** I5. **Reads:** squint `crates/squint-providers/src/privacy.rs`
(`consent`).

- [ ] The policy a config's `egress` table makes (I5's `DispatchPolicy`): which recipients
      may receive which classes of data.
- [ ] The check and the note behind a `Sender` (I5) for the first address: before data is
      handed to a recipient the policy approves, a send note is made durable (B4b).

**Done when** these pass, each with an executor a test builds with the policy: with the
empty policy every request to a backend with a recipient is refused and the fake counts
none; a recipient approved for prompts and not for source images is refused a request
that holds a source image, and sent one that holds a prompt alone; a part with no stated
class is refused unless every class is approved; a refusal answers the action with a
fault, and no other backend, remote or local, is asked in its place; a refused request
has no dispatch record, no send note and no charge, a fake meter in the place after the
gate having seen nothing; a run whose action catches the refusal and completes has it in
its record, with the recipient and the reason; the sending fake finds the note in the
store before it is handed anything, for a free request as for a paid one.
**Carry-over:** the row on squint's `recipient`. **Size:** medium.

### E3. Every hop, and a probe

**Depends on:** E2. **Reads:** squint `crates/squint-providers/src/http.rs`
(`same_origin`, `MAX_REDIRECTS`). Two pull requests: the hops; then the probe and the
download.

- [ ] Everything a `Sender` is asked to reach is checked against the recipient: a
      redirect, an upload address and a returned download address, as the first address
      is. A credential is attached only for the origin it was configured for. A send note
      is made durable before each hop is handed data, and a second note says a hop
      answered.
- [ ] The download interface is checked the same way against the origins a catalogue entry
      names. A probe is refused unless the config approves probing that recipient, apart
      from any class of data.

**Done when** these pass, against the sending fake of B3: a redirect to an origin that is
not the recipient or one of its upstreams is refused and nothing is sent to it; a redirect
to an approved upstream is followed without the credential; an upload address and a
download address outside the recipient's are each refused; a chain longer than the limit
is refused; the sending fake finds each hop's note in the store before it is handed that
hop. Then: a model's download that is redirected off its catalogue's origins is refused,
and its request carried nothing but the address; a probe is refused by default, sent when
approved, carries no user data, and its redirect off the recipient is refused.
**Size:** medium.

### E4. The run's recipients

**Depends on:** E3, S6b.

- [ ] A run's `egress` list is made from its send notes: the origin that was reached and
      the classes of data, as sent where a note says it answered. A send the gate or a
      hop's check refused is in the record as refused.

**Done when** these pass: a run that sent a prompt and a derived image to one recipient
lists it once with both classes; a run whose request went on to an approved upstream lists
the upstream as reached, and an approved upstream that was not reached is not listed; a
request that failed at its second hop lists the first, which had the data; a run that
crashed before and after a redirect, an upload and a returned download address, a free
request among them, is recovered (S6b) with every origin a note was made for, each as
dispatched with no word of whether it arrived unless its second note was durable, and
none it had no note for; a run whose only request was refused lists the refusal and
nothing as sent; a local run's list is empty; an account in the list is shown cut, and a
marker credential is nowhere in the record (rule 4).
**Carry-over:** the row on the record's empty lists (`egress`). **Size:** small.

### E6. Nothing but pixels

**Depends on:** E2. **Reads:** squint `crates/squint-providers/src/privacy.rs` (`scan`,
`FORBIDDEN_MARKERS`).

- [ ] What leaves for a recipient holds no image metadata, no file name and no path. An
      image is sent as a `Picture` (ADR 0006), which is canonical pixels and has no place
      for any; the sending interface takes a body made by whizbang's own encoder, which
      writes no ancillary data.

**Done when** these pass, through the executor and the sending fake: an image imported
with an EXIF block, a text chunk and a colour profile, each holding a marker, is sent, and
no marker is in any byte the fake was handed; a body that holds an absolute path or the
store's root is refused by the sending interface; the encoder's output for a picture holds
only the chunks the test lists.
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
path (rule 3); a model's name or a file's name that holds a secret the reader was told of
is refused whole and not shown, and never becomes a name on disk (rule 4); two records
that differ in the adapter's revision alone have two
identities; the crate-graph check passes with the new crate; every claim of the ADR about
code names its test (rule 5).
**Carry-over:** the row on a model that never shows as out of date (R2), for what an
installation records. **Size:** medium.

### M2. Downloading into staging

**Depends on:** M1. **Reads:** rascal `tools/python/src/rascal_tools/installer.py` (`get`,
`_copy_bounded`, `model_lock`); rascal `crates/rascal-testkit/src/fake_install.rs`.

- [ ] `crates/whizbang-testkit/src/downloads.rs`: a fake of the download interface that
      serves bytes from memory, can stop at a barrier after a stated count of bytes, serve
      wrong bytes, serve too many, or fail.
- [ ] Each file of a catalogue entry is downloaded into staging under the model's lock,
      stopped at its stated size and checked by hash. The modules of the installer are
      laid out as "Strands" says. Nothing is published yet.

**Done when** these pass: two files are staged and verified; a wrong hash, a short file
and a file one byte too long each fail the download and leave no staged file that passed;
a download reads no more than the stated size, counted by the fake; a refusal names the
file by its catalogue name, escaped and cut, with no path (rule 3).
**Size:** small.

### M2b. Publishing an installation, with a crash at each step

**Depends on:** M2. **Reads:** rascal `tools/python/src/rascal_tools/installs.py`
(`publish`).

- [ ] Staged files become an installation and the pointer is replaced, through the same
      file-system interface the store uses, so the shim can crash it. The installer
      publishes a status record as it goes: installing, how far, and how it last ended.

**Done when** these pass: an install of two files ends with the pointer naming a verified
installation; a failed install leaves the previous installation named and readable, and
its status says how it ended; a crash before each event the shim records, with unsynced
writes and names lost, reopens to the previous installation or the new one, whole, and the
staging left behind is removed only under the lock (rule 2); a process that dies in the
middle of a download, shown with the barrier and a crash, leaves the same; two installers
over one directory, in two processes, stopped after each read the pointer, do not both
publish (rule 1); a status record is read strictly and refused over its bound (rule 3).
**Carry-over:** of rascal's untested cases (R6), a signal during a download. **Size:**
medium.

### M3. Attach and cancel

**Depends on:** M2b. **Reads:** rascal `crates/rascal-mask/src/install.rs` (`install`, the
attach) and `crates/rascal-mask/tests/it/install_controls.rs`; table-manners
`CONTRACT.md` section 5.3.

- [ ] A second request for a model that is installing attaches, from the same process or
      another: it waits on the install's lock, reads progress from the status record, and
      ends with the same result.
- [ ] An install can be cancelled between files and inside one, by the request that
      started it. A cancelled install leaves what was installed before. An attached
      request that goes away cancels nothing.

**Done when** these pass: two requests in one process, the first stopped at a barrier,
cause one download of each file and end with one result; a real second process asking for
the same model while the first is stopped sees progress and ends with the same
installation, and the fake served each file once; when the installing process is killed,
the attached one reports the install as failed and the previous installation stays named;
a cancel while the fake is stopped mid-file ends the install as cancelled, removes its
staging and leaves the previous installation named; a cancel after the pointer was
replaced returns the installed result; an attached request that goes away does not cancel
the install.
**Size:** medium.

### M5. Holds

**Depends on:** M2b. **Reads:** rascal `tools/python/src/rascal_tools/installs.py`
(`.held`, `alive`, `Slot`).

- [ ] A hold on one installation, by its identity: a shared file lock, and a count inside
      the process. It is what M6b's lease takes. A hold is refused when the
      pointer does not name that installation.

**Done when** these pass: two holds in one process and one in a real second process are
all given; a hold on an identity the pointer no longer names is refused; a real child
process that holds a model and is killed leaves no hold, shown by taking the lock alone;
a hold let go twice is let go once.
**Carry-over:** rascal's R5 (a hold judged live by its process id). **Size:** small.

### M5b. Removal that drains

**Depends on:** M5. **Reads:** rascal `tools/python/src/rascal_tools/installer.py`
(`remove`) and `crates/rascal-mask/src/install.rs` (`uninstall`).

- [ ] Removal, inside one process: under the directory's lock the model is marked as
      draining, so new holds are refused and waiting ones cancelled; the holds are waited
      for until a deadline of the injected clock; then the removal goes on (M5c), or it
      answers busy and the mark is taken off. Here "goes on" ends at the decision: the
      pointer is cleared and nothing is deleted yet.

**Done when** these pass, with barriers and the clock moved by hand (rule 6): a removal
during a held installation refuses a new hold, cancels a waiting one, and goes on when the
hold is let go; with the hold kept past the deadline the removal answers busy, the mark is
gone and the next hold is given; a removal of a model that is not installed says so; the
mark is a record read strictly and bounded (rule 3).
**Size:** small. The waiting requests of the worker's queue are 1C's; here a waiting hold
stands for one.

### M5d. Draining across processes, and against an install

**Depends on:** M5b, M3. **Reads:** rascal `crates/rascal-mask/src/install.rs`
(`live_pid`, `terminate`).

**Done when** these pass: the two cases of M5b with the hold in a real second process; a
removal and an install of the same model, stopped after each read the pointer and
released together, end in one of the two orders (rule 1); a draining mark left by a
process that died is taken off by the next to take the lock; a removal of a model that is
installing cancels the install (M3) and leaves what was installed before it removed or
kept, as the order says, never half of either.
**Carry-over:** rascal's R3 (a process id read from a file is signalled); of R6, a
removal racing an install. **Size:** small.

### M5c. Removal on disk

**Depends on:** M5d.

- [ ] A removal that went on deletes the installation's files, through the file-system
      interface the shim can crash.

**Done when** these pass: `remove::a_crash_at_any_event…` leaves the installation whole
and named, or gone, never a pointer to files that are gone (rule 2); a removal interrupted
after the pointer was cleared is finished by the next install or removal; nothing outside
the models directory is touched, shown on the shim's list of events.
**Size:** small. A file two models share is 1C's; until then each installation has its
own files.

### M6. Readiness from what is installed

**Depends on:** M5c, I7.

- [ ] `Inventory` (I7) over the models directory: installed and verified, not installed,
      out of date, damaged, or not in the catalogue. Only the first is ready.

**Done when** these pass, each with an executor a test builds over the inventory: a
configured model that the catalogue does not hold is prepared as not ready, saying so; an
installation whose recorded catalogue or adapter revision is not the current one is
prepared as out of date; an installation record that is damaged, or whose pointer names
nothing, is prepared as damaged, never ready, and a new install repairs it; after a
reinstall, a new request with the old preparation is `Stale::Model`, one under a new id
with no preparation runs on the new installation, and a repeat of the first request's id
returns the first run.
**Carry-over:** rascal's R2 (reported), R4, and of R6 a corrupt installation record and a
reinstall between a call and its retry. **Size:** small.

### M6b. The hold on the admitted installation

**Depends on:** M6.

- [ ] `Lease` (I7) over M5's holds: a hold on the installation the run was admitted with,
      by identity. If the pointer no longer names that installation, or it is draining,
      the lease is refused.

**Done when** these pass: with a barrier between admission and the lease, a reinstall
there and a removal there each end with the request refused before dispatch, a fake meter
in the place after the lease having seen nothing, and never with another installation
answering; a retry inside an admitted run whose installation was replaced is refused the
same way; a removal that starts while a request holds the model waits for it (M5b); a
model installs, is attached to, is cancelled and is drained, each through the crate's own
interface from a test's process and a second real process, in one test that names the
four clauses of the checkpoint's exit line.
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
- [ ] Every carry-over row whose owner was 1B is gone, or has the owner the table below
      gives it. `whizbang backends status`, the `models` commands, the real network
      client, the shipped catalogue and what the worker's queue owes to removal each have
      a row for 1C.
- [ ] The test kit's fake ledger is removed, or kept with a line saying what still uses it.
- [ ] `just check` is inside its budget; the timing table is in the note, with each crash
      sweep's time alone.
- [ ] `docs/plans/m1b-note.md`: what shipped, what B0 changed in this plan, the timing
      table, what was spent (nothing), what moved to the carry-over list.
- [ ] Tag `m1b`.

## Carry-over rows and their tasks

Every row of `docs/carry-over.md` whose owner names 1B, read again from `main` at
5ae110b, nineteen rows.

| Row | Task, or new owner |
|---|---|
| The secrets pattern misses a key directly after a hyphen, an underscore, a letter or a digit | X1 |
| "Never removed" has one limit left: `put` replaces a made name | I2 |
| The executor is built with no backends and the digest of no bytes | C4 (the shape and digest: C1; the binary: W1) |
| The run record's empty lists: `asked.settings`; `spend.requests` and settled spend; `egress` | B4 defines the elements; C5, S10 and E4 fill them. `egress` was owned by "the first backend that sends data": the gate is where a send is known, so 1B takes it, filled by fakes. `models`, `time.durations` and `judge` stay with A11 |
| The ledger: the real one, the binary built with it, the token, a damaged reservation, the names' segments, a paid request with no bound | I2 (the token, the names), S2 (the real ledger), S4 (no bound), S12 (a damaged reservation), W1 (the binary) |
| Settlement: a run that ends; one that did not; a child's share not given back; a run's own requests not debited | S11; S6 and S6b; S7; S5 and S7 |
| An admission that was visible and not confirmed takes its id | S6b ends it at the next open to write. Ending it at once in the process that made it: **milestone 2**, with the first door that keeps a process alive across runs; until then the limit stays, and the row says so |
| What every publication costs as ids and reservations accumulate | **Milestone 2, before request ids are used in volume** (the measurement at 100,000 ids, S13 of draft 5). 1B adds a ledger record to each admission and two to each paid request; the row gains that |
| Whether a failed call may have been charged | B1b (the stage on a fault), S5b (the settlement) |
| What a provider adapter said of itself: `pricing`, `recipient` | B1 and S4 (`pricing`); B2 and E2 (`recipient`). Image preparation stays with milestone 2 |
| What redaction does not find; quotes around a value; the variable named | C3 (the forms, UTF-16 among them, and JSON as JSON); C2 (quotes, the variable, the least length) |
| `max_spend_usd`, the cap of one call | S3. `set` stays with milestone 3 |
| Parallel installs of models sharing a file each download it | **1C, before a real download** (M4 of draft 5, with the removal of a shared file) |
| No test plants an unknown registry or git source for `cargo deny` | X2 |
| A model never shows as out of date (R2) | M1 (recorded), M6 (reported) |
| Uninstall signals a process id read from a file (R3) | M5d |
| A configured model that is not in the catalogue can read as ready (R4) | M6 |
| A hold is judged live by its process id alone (R5) | M5 |
| Untested cases around installs (R6) | M2b (a signal during a download), M5d (a removal racing an install), M6 (a reinstall between a call and its retry; a corrupt installation record) |

New rows this plan asks for, added by the task named: `whizbang models list|get|remove`
and their guide, 1C (M6b); `whizbang backends status`, 1C (Z1); the real network client
and the shipped catalogue, 1C (Z1). The row on what running an action does not do yet
(milestone 2) keeps the bound on a ledger being asked again; 1B adds none.

## Spend

Nothing. Every backend, download and recipient in 1B is a fake, the fast tier is the only
tier these tasks run, and no task has a live call. What would change that: a test that
reaches a real provider or downloads a real file, or a real recipient written into a
config an agent runs with. None is in this plan, no network client is in the build, and
either would stop the task and go to the owner as `SPEC.md` section 15 says.

## Review rounds

Recorded as `AGENTS.md`, "Working with the owner", requires. Each round was one run of
the Codex CLI, read-only, over this repository at the branch's head, asked for blocker,
major and minor findings on scope, order, contradictions with `SPEC.md` and the ADRs,
missing acceptance cases, carry-over rows and tasks too large.

| Round | Date | Draft | Blockers | Majors | Minors |
|---|---|---|---|---|---|
| 1 | 2026-11-13 | 2 | 2 | 9 | 0 |
| 2 | 2026-11-13 | 3 | 1 | 7 | 1 |
| 3 | 2026-11-13 | 4 | 1 | 6 | 2 |

**Round 1, and what changed in draft 3.**

- Blocker: recovery could not write a run's record, because an admission holds
  fingerprints only. Added B6, which keeps the prepared call with the admission, and the
  open point D11; S6 says what a run admitted without it gets.
- Blocker: installs and removal were narrowed to one process, which `SPEC.md` sections 4
  and 6 do not say. A second process now attaches (M3) and removal drains across
  processes (M5); the decision that said otherwise is replaced.
- Major: tasks that change the contract's files were not all named as joins. The joins
  are now a stated line; the roster from a config moved into the core, out of
  the models strand's crate; the models strand's files are divided.
- Major: four dependencies ran backwards. A refusal from inside a run moved from S4 to
  S5; recovery of a refused end from S11 to S6; removal of a shared file from M4 to M5;
  the listing from M6 to M7.
- Major: recovery skipped a reservation whose run was deleted. S6 recovers from the spend
  records, run or no run, with cases through a delete and a prune.
- Major: accounting could end before settlement was durable. B5 puts accounting outside
  the meter, and S5b has a barrier between the answer and the settlement.
- Major: nothing kept two requests of one run from both taking the last of its
  reservation. S1's ADR gives a dispatch a made name, and S5 races two requests.
- Major: reading a digest again was not a reload. C4 makes `ConfigSource` answer with a
  snapshot or a refusal; C5 tests a changed cap, recipient and credential, and C4 a file
  that no longer parses.
- Major: metadata removal and the hops actually reached were missing. Added E6; E4 notes
  sends where they are made and follows E3.
- Major: a model could be replaced between admission and its hold. M6 holds the admitted
  installation by identity, with a barrier racing a reinstall and a removal.
- Major: S1, S5, S11 and M2 were too large. Each is split in two (S1b, S5b, S11b, M2b),
  and C4 in two (C5).

**Round 2, and what changed in draft 4.** The round confirmed eight of round 1's fixes and
found three incomplete (recovery, the joins, sizes).

- Blocker: a recovered run's record still lacked its settings and where data went. B6
  keeps the resolved settings with the admission; every request to a backend with a
  recipient has a dispatch record that names the recipient and the classes of data (S1,
  S5); `egress` has a third state for a send a crash left unknown (B4); S6b tests a
  recovery after the config was replaced and after a first hop was reached.
- Major: B4, B5 and the change to `Ledger` were outside the line of joins, and root files
  had no owner. B4 and B5 are named in the line; the interface change is its own join,
  S1c, after B6; B0b makes every root file once.
- Major: a request that succeeded had no stated cost. B1 defines a response's charge, B3's
  fakes report one, and S5b tests a charge under the bound, none, and one over it.
- Major: a backend with a recipient and no price read as free. B1 states the allowed
  declarations: it has no bound and is refused.
- Major: recovery and a damaged record were not composed. S6 recovers what it can, marks
  the ledger damaged and refuses paid calls; S12 follows S6 and repairs.
- Major: "retry" in M6 contradicted a run's pinned model. M6 separates a retry inside a
  run, a new request and a repeated id; S7 holds a retry to its run's installation.
- Major: nested accounting had no durable shape and no recovery test. S1's ADR says what a
  dispatch names; S7b crashes each step of a nested call; D12 asks 1A what a child's
  admission holds.
- Major: M5 and S6 were too large. Each is three tasks (M5, M5b, M5c; S6, S6b, S6c).
- Minor: rule 7 stated 1A's scheduling as fact, and two cases fell back when cancellation
  was missing. Scheduling is the open point D12; the fallbacks are gone and B0 stops if
  cancellation is not there.

**Round 3, and what changed in draft 5.** The round found six of round 2's fixes in the
task text and three still incomplete (recovery, dependencies and ownership, sizes). The
fixes below were made after it and have not been through a fourth round.

- Blocker: recovery still lost the origins a request went on to after its first. A send
  note is now made durable among the run's names before data leaves for any origin (B2
  decides it, B4 gives the durable note, E2 and E3 make it, S6b reads it), and E4 crashes
  a run on each side of a redirect, an upload and a returned download address. The
  dispatch record is the ledger's again and holds no recipient.
- Major: six dependencies or owners were wrong. The declarations that pair a price with a
  recipient moved from B1 to B2; S4 wraps `Backends` inside `spend/`; M5d follows M3; C5
  follows C3; an ADR has one task; C1's and C3's own lines say B0b.
- Major: a refusal inside a run had no acceptance case. B4 defines the element if 1A did
  not and has the case; S5 and E2 each have an action that catches a refusal and
  completes.
- Major: probes had no proof. B2 gives a probe its own interface with no place for user
  data, and E3 tests refusal by default, approval and its redirect.
- Major: a secret inside a name had no enforcement point. Rule 4 says a name from outside
  is looked at whole before it is parsed or cut; B2, C1, C2 and M1 each have the case.
- Major: cancellation was taken for an unknown cost. S5c has both cases: no answer that
  settles it, and an answer with a charge.
- Major: B1, S5b, M5b and M6 were too large. Each is two tasks (B1b, S5c, M5d, M6b).
- Minor: side tasks were said to revert alone; they revert after what needs them. Rule 3
  no longer asks one form of a config or an env file.

**Draft 6** was written after the third round and after all of task A7 merged. It
applies the owner's decisions of 2026-11-13 (under "Decisions"): the cuts and moves the
third round listed, and its outline of interfaces first, which this draft's I1 to I7 are.
The task ids of rounds 1 to 3 above are draft 5's: B5 became I3, S1c became I2, and E5,
M4, M7, S13 and S6c are gone from 1B.

## Risks

| Risk | Effect | What we do |
|---|---|---|
| A9, A11 or ADR 0009 changes where handles are made or what a run records of a request | I3 and B4 are wrong as written | B0 is first, checks D9, and stops on an answer that is neither outcome |
| An interface fixed early does not fit the strand that implements it | A base task is reopened and a strand waits | Each interface is the outline a review gave after three rounds of the tasks it serves; its task ships a fake that the strand's tests are first written against; a misfit is changed by a base task, never worked around in a strand |
| Recovery is still incoherent | A crashed run is misreported or its money miscounted | "What a crashed run is recovered to" is the one statement of it; every fact in its table has the task that makes it durable; S6b, S7b and E4 crash each |
| The record's shapes are fixed in B4 before the strands that fill them | A second version, `run/4`, inside one checkpoint | B4 follows B1 to B3, which decide the quote, the stage and the recipient; a wrong shape is a new version, never an edit |
| Crash sweeps multiply | The fast tier passes its three minutes | Rule 11; each task states its sweeps' times alone in its hand-back |
| Folding the balances from every record costs more with every run | Admission slows as a store ages | Known, and measured at 100,000 ids in milestone 2 before ids are used in volume; a checkpoint record under a new name is the stated way out |
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
