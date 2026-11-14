# Plan: checkpoint 1B, policy and controls

Draft 13, 2026-11-14, the final revision. From `SPEC.md` sections 3, 4, 6, 10, 11 and 16. This file is the
task list.

Reviewed in six rounds. Scope, order and every area but one are settled; the ledger's
arithmetic is deliberately settled in task I2, with code in hand ("Decisions", the last
item). Checkpoint 1A is closed and tagged `m1a`; building starts with task B0.

Checkpoint 1A closed on 2026-11-14 (`docs/plans/m1a-note.md`). This revision is written
from `main` at that tag: the code, ADRs 0001 to 0009, and the carry-over list as the exit
check left it.

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

**Decided, and what 1B does with each** (ADRs 0005, 0007, 0008 and 0009 as they stand at the tag `m1a`).

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
| `RunState::Abandoned`: a run whose admission or end the store did not confirm, in the process that made it. To any other process it is admitted and not ended | I4b and S6 end and settle such a run at the next open to write. Ending one at once, in the process that made it, is moved to milestone 2 |
| A run reads the generation it was admitted against (`Store::hold`) | Recovery and the ledger read the current generation, never a held one |
| `start` runs an action on a thread of its own; an executor starts 64 runs at once by default and never more than 256 for one store value, a run starts four at once, and at most 1,024 runs are under one run that nothing called | "Rules every task's tests follow", 7 to 10 |
| `run/2`, frozen at the tag: `identity.parent`, `steps`, `outcome.refusals`, `models`, `time.durations` and `judge` are filled (ADR 0008, ADR 0009). A model's `device` and `cached` are null until 1C. Empty: `asked.settings`, `egress`, `spend.requests` | `run/3` (B4) defines the elements of `asked.settings`, `spend.requests` and `egress`, and adds kinds of refusal for a request that was not sent. It leaves alone what 1A filled |
| A run is handed the backend its call resolved and no other (`Capabilities::resolved_for`); its handles are made in `Context::new` and share one `Tally`, which counts each request and records it (ADR 0009). The roster a call is resolved against and the backends an executor holds are given apart, and nothing holds one to the other | I3's wrappers go inside that tally. I6's snapshot makes the roster and the held backends from one thing, and C4 builds it |
| A record is read back only under 4 MiB, and every list in it is bounded where it comes in (ADR 0008, "What a call and a run may hold"; `Engine::with_record_limit` lowers the limit for a test) | B4 gives each new list its bound, and the sum stays under the limit by the same test |
| A generation file is 64 MiB at most (`Store::limit_generations_to` lowers it for a test). At the limit a publication that adds names is refused and a deletion is still published; a run that cannot publish its end is abandoned | Every publication 1B adds (a dispatch, a settlement, a closing, recovery) has a case at the limit: I4, S5, S5b, S11b |
| The binary has one door (`crates/whizbang/src/door.rs`; ADR 0008, "The first door"): one entry point, a quiet panic hook, everything printed escaped | I4's entry to a writer is behind that door; `whizbang spend` goes through it |
| An admitted run keeps `admission/1` and nothing else of its call until it ends: the run, its parent, its request id, the action and its version, what was reserved or taken, and its start | Recovery builds an interrupted run's record from that alone, and keeps nothing more for its sake |

**The open points of the earlier drafts, settled.**

| # | Was | Is |
|---|---|---|
| D5 | How requests are accounted for, the cancellation rule, a child's allowance | A `Tally` inside each handle, made in `Context::new`; the rule of ADR 0008, "Cancellation"; `Allowance::take` in memory. I3 adds the wrapper inside the tally |
| D6 | The elements of `outcome.refusals` | Defined by ADR 0008 for calls. A capability request that is refused is a fault to the action and is not recorded; B4 adds it, with no action and a kind of its own |
| D7 | The final `run/2` | As the last row above, but for what A11 fills (`models`, `time.durations`, `judge`); see D9 |
| D10 | Whether an admission can be cancelled, and a bound on a ledger being asked again | Cancelled by its request's signal or deadline, looked at before each asking. No bound on the count and no turn given: that stays on the carry-over list with milestone 2, and S2 adds none |
| D11 | Whether an admitted run keeps its call | It does not, and 1B does not add it: recovery is lossy by decision |
| D12 | Scheduling, and a child's admission | A thread for each started run, bounded; a child has an admission that names its parent and its share, and no reservation |

**The three points that waited for the end of 1A, as the tag leaves them.**

| # | Was | Is |
|---|---|---|
| D8 | The one source of a store's location | `--store <dir>` is required on every command that touches a store. There is no default and no setting. `whizbang spend` takes it the same way, and the config's location is given the same way, by a flag |
| D9 | Where handles are made, and what a run records of a request | In `Context::new`, from the backend the call resolved. Requests are counted and recorded by the handles' tally, as ADR 0009 says |
| D13 | What `store delete` removes, and how the store's own files are bounded | A deletion removes a run's names and leaves the names under `requests/` and `spend/` untouched. Removing the artifacts a deleted run committed is decided and is milestone 2's. `CURRENT` is 21 bytes at most, a generation file 64 MiB |

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
- **Recovery is lossy about what a crashed run did and pessimistic about what it cost
  (the owner's, 2026-11-13).** Four review rounds each found a deeper hole in a design
  that tried to reconstruct an interrupted run. 1B does not reconstruct. A run a dead
  process left is ended as an error of the kind `interrupted`, with a small record made
  only from its admission, and everything else about it is declared lost. Its
  reservation counts as spent in full unless whole, readable records show that every
  request it dispatched was settled. No task keeps a fact durably for recovery's sake. A
  fuller record of interrupted runs is planned for no milestone; one carry-over line,
  owner "none; reopen on need", says so. "What a crashed run is recovered to", below, is
  the whole of it.
- **A damaged record never stops recovery and never releases money.** A damaged spend
  record freezes the one reservation it belongs to, at its full amount, until a person's
  command (S12); every other reservation and every unrelated paid call goes on. Damage
  to an admission or to any other record makes that run `interrupted`, with the
  unreadable part named.
- **A person corrects, by a new record.** `whizbang spend adjust` (S12) releases or
  changes the charge of a named interrupted or frozen reservation. It writes a spend
  record under a new name and never edits one. It is a command and nothing else, as
  `spend reset` is, and an agent runs it only against a temporary store in a test.
- **Recovery runs where a writer is handed out.** One entry point opens a store to
  write, for the executor and for every command that writes, and runs recovery first.
  If recovery fails, the store is opened to read only and the reason is said.
- **Models are installed by the binary's own code, in Rust, into a models directory
  outside any store.** Rascal installed from Python. Here a model must be installed and
  judged ready with no Python present (`SPEC.md` section 1, the first user story), and 1B
  comes before the pack. Weights are shared by every store on a machine, so they are not
  in a store; the directory has its own lock and its own atomic publication. `SPEC.md`
  section 3 says the store is the one owner of everything durable; B0b's pull request
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
- **The ledger's arithmetic is settled with code in hand, not in this plan (the owner's,
  2026-11-13).** Six review rounds each went one level deeper into names and sums written
  as prose. The plan keeps the invariants; task I2 writes the ADR and has it reviewed
  together with a small executable model of the fold and a table of histories run as
  tests, every sequence the six rounds produced among them. Each later ledger task adds
  its histories to that table and is held to it.
- **Two pull requests where a task has three kinds of work (the owner's).** A task that
  carries an ADR section, an implementation, and crash or race proofs is planned as two
  pull requests, the proofs second. The order table says which.

## What a crashed run is recovered to

A process can die at any moment. The next process that opens the store to write recovers
it before it is handed a writer (I4), in one publication. Recovery does two things and
reconstructs nothing.

**It ends the run.** Every run that is admitted, has no record, and was not made by this
process is ended as an error of the kind `interrupted` (I4b). Its record is small and is
made only from its `admission/1`:

| In the record | From |
|---|---|
| `identity`: the run, its parent, its request id | The admission |
| `asked.action` and its version | The admission |
| `spend.reserved_micros` | The admission |
| `time.start_ms`; the end is the time of the recovery | The admission; the clock |
| `outcome`: `error`, kind `interrupted`, and `outcome.lost` | Fixed |

`outcome.lost` (new in `run/3`, empty for every run that ended by itself) names every
group the record does not know: the arguments, the settings and the config's digest, the
inputs, the refusals, the durations, the models, the requests, the egress, the steps and
a judge's turns. A field such a group would fill is null or empty, which `run/3` allows
exactly when `outcome.lost` names it. The version that ended the run is in the record;
the version that ran it is not known and is named as lost. What the run had already
published (its admission; nothing else is published before a run ends) stays as it is.
For a request with an id, the id's ending is made, so a repeat is answered.

**It settles the money, never under.** "The ledger's invariants", below, is what it is
held to, and I2's ADR the rule. In short, for every reservation that has no settlement and whose run is not
open in this process (S6), run or no run:

- if every spend record under it can be read, and every request dispatched under it has
  a settlement, the tree is charged what those settlements say, and what the reservation
  held beyond that is released; a tree that dispatched nothing is charged nothing,
  because a dispatch is durable before a request leaves;
- otherwise it is charged, as interrupted, the larger of its full amount and what is
  known against it: its readable settlements and the bounds of its dispatches that have
  none;
- a record under it that cannot be read also freezes it: nothing of it is released until
  a person's command.

A child follows its root: the tree has one reservation, at the root, and nothing is
given back to anyone by recovery. A person can correct a charge afterwards with
`whizbang spend adjust` (S12), which writes a new record.

The other rules:

- **A run that was deleted** is not made again; its reservation is settled as above.
- **A damaged admission** cannot give a record its fields: the run is ended with the
  fields its name gives, and `outcome.lost` names the admission too.
- **A run abandoned in a live process** (ADR 0008) is made by this process and is left
  alone. Its reservation stays reserved, which counts against the allowance, until the
  process ends and the next open recovers it.
- **A failed settlement or a backend's panic** inside a run leaves the dispatch
  unsettled and the run abandoned (I3); the next open then charges the full amount.
- **Recovery is one publication of names made once.** A crash inside it leaves it not
  done, and the next open does it; done twice it changes nothing, because every name it
  would make is there. It runs before a writer is handed out, once for a store value.
- **If recovery fails before its publication is visible,** nothing is published. **If
  its publication became visible and was not confirmed** (the store's `Unacknowledged`),
  the whole generation is there and stays. In both, no writer is handed out, the store is
  opened to read only, and the caller is told which, without a path. The next open
  confirms the generation it finds, by a sync, before it is ready, and then finds every
  name of the recovery made and adds nothing.

What readers say of an interrupted run (I4b): `runs list` and `runs show` show it as
ended in error, interrupted, with what is lost; a repeat of its request, `status` and
`wait` answer `Ended`, with the error `interrupted`; `cancel` answers
`NotAccepted(Ended)`. Where the admission itself could not be read, the questions by
the run's id are answered so, and a request under its request id is answered with the
error of an unreadable record.

## The ledger's invariants

This is what every ledger task is held to, and all of the ledger that this plan decides.
The names and the arithmetic that keep these are decided in task I2, by an ADR reviewed
with an executable model and a table of histories; the appendix holds where the reviews
left them, as a starting point and not as a decision. Where a task below states a number
or the form of a name that I2's ADR then decides otherwise, the ADR governs and the
task's case is restated with it.

1. **Never under.** What the ledger counts as spent or held is never less than what its
   readable records show was charged, plus the bounds of what is dispatched and not
   settled. Counting more is allowed where a rule says so; counting less never is.
2. **Debt is not forgotten.** A balance may be below nothing inside the ledger and inside
   an allowance; what can be spent is never reported below nothing; and every return (an
   unused bound, a child's remainder, a share handed back) pays the debt first.
3. **One closing.** A reservation is closed once, by its run's end or by recovery, and a
   closing that releases anything needs a durable settlement for every dispatch under it.
4. **Damage freezes one reservation.** A damaged entry freezes the reservation it belongs
   to, at an amount that can be read without the damaged content. It never stops the
   fold, recovery, or a call that has nothing to do with it.
5. **Every amount the fold needs is in a name.** Holds, dispatches and settlements carry
   their amounts and their reservation in names the ledger makes, with a guard name where
   two must not take one place. The grammar is complete, strict and bounded, and parsed
   as untrusted: it lists every name accepted under `spend/`, the executor's reservation
   name, the turns, and the names of an allowance and a reset among them, and a reset has
   an epoch that ties each reservation to the reset it falls under. Nothing secret is in
   a name.
6. **No admission on a stale balance.** An admission cannot be published on a balance
   that a settlement over its bound has since changed.
7. **A person corrects by a new record,** never by editing one.

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
7. **Threads only through the test kit** (`AGENTS.md`, "Threads and processes"). A test
   makes a thread through `whizbang_testkit::threads` and no other way: `just check`
   refuses any other, and the helper holds a process to 64. No test, probe or script
   holds more than 200 live threads in one process. A thread that cannot be made is shown
   with `Engine::with_spawn`, never by using them up.
8. **Every loop that starts something is bounded.** A loop that starts a thread, a run or
   a process has a literal bound written in it and waits for each child it started. Two
   show a race; a test that needs more says why beside it.
9. **Nothing is run to find a limit.** No test and no probe written during a task asks a
   system how many threads, processes, files or bytes it will give before it refuses.
10. **Tests are run one way.** Through `just check`, `just test-only <args>`, which takes
    a closed set of arguments, or either of them under `scripts/heavy.sh`; never by
    calling the test runner directly.
11. **A crash sweep is named as one.** A test that crashes before every step has
    `a_crash_at_any_event` in its name, so the `crash-sweeps` group of
    `.config/nextest.toml` takes it and runs it alone, and it takes under about 5 seconds
    alone. A sweep that cannot is cut into sweeps of a part of the steps each.

## Strands, and what each owns

| Strand | Tasks | Owns | Starts after |
|---|---|---|---|
| Base (a line) | B0, B0b, B1, B1b, B2, B3, B4, B4b, I1 to I7 with I4b, and W1 at the end | The executor's files (`crates/whizbang-core/src/contract/`: `engine.rs`, `request.rs`, `prepare.rs`, `context.rs`, `ledger.rs`, `record.rs`, `live.rs`, `nested.rs`), `crates/whizbang-core/src/capability/`, the record's schema, `crates/whizbang-testkit/src/` but for the files named below, `crates/whizbang/src/wiring.rs`, `door.rs`, `runs.rs`, `store.rs` and `import.rs`, every root file | The tag `m1a` |
| Spend | S1b to S12 | `crates/whizbang-core/src/spend/` and `tests/it/spend/`; `crates/whizbang/src/spend.rs`; `crates/whizbang-testkit/src/spend.rs` (the model and the table of histories, which I2 makes and each ledger task adds to) | S1b after I2; the rest after the interface each names |
| Config and secrets | C1 to C5 | `crates/whizbang-core/src/config/` and `tests/it/config/`; `crates/whizbang-core/src/contract/redact.rs` and its test; `docs/guides/config.md` | B0b; C4 after I6 |
| Egress | E2, E3, E4, E6 | `crates/whizbang-core/src/egress/` beyond B2's types, and `tests/it/egress/` | I5 |
| Models | M1 to M6b | `crates/whizbang-backends/`; `crates/whizbang-testkit/src/downloads.rs` | B2; M6 after I7 |
| Scripts (a line of its own) | X1 to X6 | `scripts/` and `scripts/tests/` but for root files, `deny.toml`, and the two lines of the `kickoff` and `review-merge` skills that X3 changes | B0 |

**The table of histories** is one file of the test kit that I2 makes. S1b, S5b, S6, S7,
S7b, S11 and S12 each add the histories their task names to it, one at a time in the
order they merge, and each is done only when the model, the real fold and, where the
task has an executor, a run through it all give the same balances.

**The line, and why each task is on it.** One builder at a time works in the base
strand, because each of its tasks changes the executor's files:

- B4b, because the executor has to hand each run its place for facts and write them.
- I1 to I7, because each adds one call from the executor to an interface: at admission
  (I1), in the ledger's step and at the end of a run (I2), around a request (I3), when a
  writer is handed out (I4, with the interrupted record and its readers, I4b), before a send (I5), when a call is prepared (I6), and before a
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
and after it by a base task only. `SPEC.md` is changed by B0b (the
two sentences under "Decisions" on the models directory, and the config example if C1's
reading of it needs one, which C1 then asks a base task for) and Z1, and by no side task. An ADR is changed by the task that brings it and
by no other, but for the interfaces' ADR, which each of I1 to I7 adds its section to, one
at a time. `docs/carry-over.md` is changed by every task in its own rows.

Two things a side task needs of a root file, and where each is made. B0b also makes the
empty module root `downloads` of the test kit, so that M2 fills
`crates/whizbang-testkit/src/downloads.rs` and leaves the test kit's `lib.rs` alone. And
a side task may change `Cargo.lock` in one way: by adding a dependency to the manifest
of its own strand's crate, as M1 will to `whizbang-backends`. A crate that is new to the
lock file is justified in that pull request as B0b's TOML reader was, and `just deny`
passes on it. Any other change to a root file is a base task's.

## Order

Each task needs every task listed for it to be merged first. "PRs" is 2 where the task is
planned as two pull requests, the proofs second.

| Task | Strand | Needs | Size | PRs |
|---|---|---|---|---|
| B0 Entry check | Base | the tag `m1a` | small | 1 |
| B0b Roots | Base | B0 | small | 1 |
| B1 What a backend says of cost | Base | B0b | small | 1 |
| B1b What a request cost, and how far it got | Base | B1 | small | 1 |
| B2 The ways out: recipients, classes of data, three interfaces | Base | B1b | small | 1 |
| B3 Fakes that quote, charge, fail at a stage and name a recipient | Base | B2 | small | 1 |
| B4 `run/3` | Base | B3 | medium | 1 |
| B4b A place for a run's facts | Base | B4 | small | 1 |
| I1 A part in the admission | Base | B4b | small | 1 |
| I2 The ledger's design: an ADR with a model, then its interface and records | Base | I1, B1b | medium | 2 |
| I3 A wrapper around each request | Base | I2 | small | 1 |
| I4 One way to a writer, with recovery before it | Base | I3 | medium | 2 |
| I4b The interrupted record, and what readers say of it | Base | I4 | medium | 2 |
| I5 A policy before a send, and a checked send | Base | I4b | small | 1 |
| I6 One snapshot of the config | Base | I5 | small | 1 |
| I7 An inventory, and a lease on an installation | Base | I6 | small | 1 |
| W1 The binary wired | Base | S3, S8, S10, S7b, S12, E4, E6, C5, M6b | small | 1 |
| S1b The fold | Spend | I2 | small | 1 |
| S2 Reserving at admission | Spend | S1b, I2 | medium | 2 |
| S3 The per-call cap, and a call's own | Spend | I6 | small | 1 |
| S4 Quotes: a bound, or no dispatch | Spend | S1b, B3 | small | 1 |
| S5 Dispatch first | Spend | S4, S2, I3 | medium | 2 |
| S5b Settlement of each attempt | Spend | S5 | small | 1 |
| S5c Settlement under cancellation and a crash | Spend | S5b | small | 1 |
| S7 Nested calls and retries | Spend | S5b | medium | 2 |
| S7b Nested calls under a crash | Spend | S7, S6 | small | 1 |
| S8 Estimates expire | Spend | I1 | small | 1 |
| S9 `whizbang spend status` and `reset` | Spend | S2 | small | 1 |
| S10 The run's spend lines | Spend | S5b, S7 | small | 1 |
| S11 The reservation settled when its run ends | Spend | S5b | small | 1 |
| S11b Settlement at the end, under a crash and a race | Spend | S11 | small | 1 |
| S6 After a crash: the money, never under | Spend | S11b, I4b | medium | 2 |
| S12 A person's correction | Spend | S9, S6 | small | 1 |
| C1 The config: its file, its shape, its digest | Config | B0b | medium | 1 |
| C2 Keys from an env file | Config | C1 | small | 1 |
| C3 What redaction did not find | Config | B0b | medium | 2 |
| C4 The roster from a snapshot of the config | Config | C1, C2, I6 | medium | 1 |
| C5 The cap, the policy, the secrets and the settings from the snapshot | Config | C4, C3, S3, E2 | medium | 1 |
| E2 The gate: denied by default | Egress | I5 | medium | 1 |
| E3 Every hop, and a probe | Egress | E2 | medium | 2 |
| E4 The run's recipients | Egress | E3 | small | 1 |
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
| X1 A key directly after a word character | Scripts | B0 | small | 1 |
| X2 A planted registry fails `just deny` | Scripts | B0 | small | 1 |
| X3 A pull request's title and body, scanned | Scripts | X1 | small | 1 |
| X4 A push made around the hooks | Scripts | X3 | small | 1 |
| X5 A script edited while it runs | Scripts | X4 | small | 1 |
| X6 No `grep -q` after a pipe | Scripts | X5 | small | 1 |
| Z1 Exit check | | all | small | 1 |

**What runs at the same time.** The base line is seventeen tasks, nearly all small, and one
builder walks it. Beside it:

- From B0: X2, and X1 then X3 to X6 one after another, since they share the scan's
  script. From B0b: C1, then C2; C3. From I2: S1b. From B2: M1 to M5c, the whole
  models strand but its last two tasks. From B3: S4.
- From I1: S8. From I2: S2, then S9. From I6: S3. From I3: S5 and all that follows it in
  spend. From I4b: S6. From I5: the egress strand. From I6: C4. From I7: M6.

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

**Depends on:** the tag `m1a`. **Reads:** `docs/plans/m1a-note.md`; `docs/carry-over.md`.
Nothing is left for it to decide: D8, D9 and D13 are settled above.

- [x] The carry-over rows that "Carry-over rows and their tasks" gives a new owner are
      edited to say so, and the new rows it asks for are added.
- [x] If `main` has moved past the tag, each task's "Reads" and "Depends on" lines are
      read against what moved, and a line that no longer fits is corrected here. A
      correction that changes what is built gets the review rounds first.

**Done when** every row of `docs/carry-over.md` that names 1B has the task or the owner
that table gives it, and `just check` passes.
**Size:** small. Documents only.

**What the check found.** `main` was at commit f42623e: the tag `m1a` and two merges
after it, both of documents (the wording of the 1A note, and this plan). No code, no
script and no ADR moved, so every "Reads" and "Depends on" line was read against the
tree of the tag. Each file, type, function, test and ADR section that a task names in
this repo is there, and each file and name of an old repo is there at its commit in
`docs/sources.toml`. Two lines of this plan were corrected, and neither changes what is
built:

- I5 said it depends on I4. The order table, which governs, says I4b; the line now says
  so.
- "Carry-over rows and their tasks" said that each new row is added by the task it
  names, and B0's first box says B0 adds them. B0 added the four, and that paragraph now
  says what the named task does with its row.

Twenty-five rows of `docs/carry-over.md` named 1B, the twenty-five of that table. Each
now names its tasks, or the owner the table gives it: the cost of a publication, in its
own row and in the row on 256 calls, is milestone 2's, and installs that share a file
are 1C's.

### B0b. Roots

**Depends on:** B0.

- [x] The edits to `SPEC.md` that this plan's reviewed decisions make, and no others: the
      two sentences on the models directory; in section 3, that a run a dead process
      left is ended as interrupted with a record that declares what it does not know;
      in section 6, that such a run's reservation is charged in full unless every
      request under it is shown settled, that a damaged spend record freezes its
      reservation, and that a person may adjust such a charge by a new record.
- [x] The root files of "Strands": the module roots `spend/`, `config/` and `egress/` in
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

- [x] An ADR, which changes ADR 0006 and names ADR 0007: a quote, in the unit ADR 0007
      fixed (millionths of a US dollar), which is one of free, an enforceable bound, or an
      estimate only; for a price by time, that only a limit the host enforces or a fixed
      maximum charge makes a bound; and how a quote becomes the answer of
      `Backends::bound` (an estimate only is no bound). It leaves to B1b what a request
      cost and how far it got, and to B2 which prices go with which recipients.
- [x] The quote and a backend's statement of its price, in
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

- [x] In B1's ADR, by the same builder: what a failed request says of how far it got (not
      sent; answered, with what it cost when the answer says; sent with no answer that
      settles it), and what a request that succeeded cost: a response may carry a charge,
      which the adapter states or works out from the usage the answer reports at the
      prices it quoted from; a response without one cost its bound.
- [x] The stage on a `Fault` and the charge on each response, as private fields (ADR 0006
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

- [x] An ADR: a recipient (an endpoint's origin, the account used with it, the upstreams it
      may forward to); the four classes of data (source images, derived images, prompts,
      evidence) and who states the class of each part of a request; that a part with no
      stated class needs every class approved; the interface through which data is sent,
      and the separate one through which a model's file is downloaded, which has no place
      for user data or for a credential bound to another origin; a third, a probe of a
      backend, which carries no user data either and is approved apart from sending; which
      declarations go together (no recipient and no price is local and free; a recipient
      and no price has no bound and is refused as any paid request without one is; free
      for a backend with a recipient is said, never assumed); where a backend says who
      receives its requests.
- [x] The types and the three interfaces in `crates/whizbang-core/src/egress/`, and the
      recipient in a backend's declaration. A backend that names no recipient is local.
- [x] No enforcement in this task: the gate is E2.

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

### B3. Fakes that quote, charge, fail at a stage and name a recipient

**Depends on:** B2.

- [x] In `crates/whizbang-testkit/src/capabilities.rs`: each fake can be built with a price
      (free, a bound, an estimate only, a price by time), to report a charge with its
      answer (under the bound, none, over the bound), with a recipient, and to fail at
      a named stage. The fakes of 1A already stop at a named barrier and see the
      cancellation; nothing of that changes.
- [x] A fake that sends: it passes scripted hops (a redirect, an upload address, a returned
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

- [x] An ADR, which changes ADR 0005: `run/3`. The element of `asked.settings` (a name and
      a value, as an argument is written, in a stated order); of `spend.requests`, one
      paid request (reserved, settled, unresolved, the stage it reached, and whether its
      charge was over its bound); of `egress` (the recipient, the classes of data, and whether
      it was sent or refused);
      the kinds a refusal gains for a capability request that was not sent, with no
      action; `outcome.lost`, the error kind `interrupted`, and which fields may be null or
      empty when `outcome.lost` names their group, as "What a crashed run is recovered
      to" says. What reading does with a `run/2` record: it is read, as frozen
      at `m1a`.
- [x] A bound on each new list where it comes in, so that a record stays readable under
      its limit (ADR 0008, "What a call and a run may hold").
- [x] The types, the schema `run-3.json` and fixed vectors. The executor writes `run/3`
      with the new lists empty.

**Done when** these pass: a `run/3` record with each new element round-trips and validates,
and the schema and the types refuse the same faults, one document for each (ADR 0005's
three ways); a `run/2` record written by the tag `m1a` is read and shown; `outcome.lost`
with anything in it is refused unless the status is `error` and the kind `interrupted`,
and a field that is null or empty only because its group was lost is refused where
`outcome.lost` does not name that group, by the types and by the schema, with the
mapping of fields to groups written in the ADR; a null that says something is known to be
absent (a root's parent, a run with no request id) is as before; three fixed vectors: an
ordinary root, an interrupted root with its admission, and an interrupted run whose
admission could not be read; a
recipient's account and a setting's value pass through redaction before they are written
(rule 4); what 1A filled of `models`, `time.durations` and `judge` is as it was; a run that fills
every list of `run/3` to its bound has a record under the limit, with
`Engine::with_record_limit` set low to show the refusal of one more.
**Size:** medium.

### B4b. A place for a run's facts

**Depends on:** B4.

- [x] The executor gives each run a place (`RunFacts`) in which a setting, a spend line, a
      refusal of a request and a recipient can be noted from another module, and writes
      what was noted when the run ends. Nothing in it is durable before the run's end:
      what a crash loses of it is what `outcome.lost` names.

**Done when** these pass: a fact of each kind noted from a test's module is in the run's
record, for a run that completed, failed, was cancelled and panicked; a refusal of a
request noted inside a run whose action then completes is in that run's
`outcome.refusals`, within ADR 0008's bound of 256; a recipient's account and a setting's
value are redacted when they are noted (rule 4); a note made after the run's end began is
dropped and changes no record.
**Size:** small.

### I1. A part in the admission

**Depends on:** B4b. The first section of the interfaces' ADR, which changes ADR 0007.

- [ ] `AdmissionPart`, in the core: the executor holds a list of parts. Each is shown the
      request, the preparation made at admission and the time, before each attempt at the
      admission's publication; it may refuse, with a refusal of its own kind, and it may
      add records among the run's names to that publication. A refusal by a part is a
      refusal after the look, so the id is looked up once more (ADR 0007).
- [ ] Two things a part reads, added here because they are the executor's: a request may
      carry a cap of its own, which is no part of its fingerprint; and a preparation
      carries the time it was made, from the executor's clock, beside `prepared/1` and
      not in it.
- [ ] The executor calls its parts; with none it behaves as before. A fake part in the
      test kit that refuses, or adds a record, on demand.

**Done when** these pass: a part's record is in the store with the run or not at all,
shown by refusing each call of the admission's publication in turn; a part that refuses
leaves no run, no reservation and the request's id free; parts are asked in the order they
were given and the first refusal is the answer; a part's record that holds a secret is
refused and not shown (rule 4); a request's cap and a preparation's time reach a part as they were given, and the fixed
vectors of `request/1` and `prepared/1` are as they were; every test of `admit.rs` passes
unchanged.
**Size:** small.

### I2. The ledger's design: an ADR with a model, then its interface and records

**Depends on:** I1, B1b. **Reads:** "The ledger's invariants" and the appendix; ADR 0007,
"The reservation" and "The estimate"; ADR 0008, "Allowances"; `docs/carry-over.md`, the
rows on the ledger and on "never removed"; squint `crates/squint-core/src/ledger.rs`
(`Ledger`, `CapLedger`, `Account`, `Hold`, `Closing`) and
`crates/squint-eval/src/eval_ledger.rs`; rascal `schemas/rascal/ledger-v1.json`. This is
the one task that was I2 and S1 in earlier drafts.

**First pull request: the design.**

- [ ] An ADR, the ledger, which changes ADR 0007's interface and ADR 0008's allowance:
      every name under `spend/` and its grammar; the content of each record; what a
      reservation counts for in each state; what an allowance holds and how debt is
      paid; what an admission and a settlement each make so that neither is published
      on a stale balance; how a reset bounds what came before it.
- [ ] A model of the fold in the test kit: pure, with no store. A history is a list of
      events (an allowance set, a reset, an admission, a child's share, a dispatch, a
      settlement, an end, a crash with a recovery, damage to one entry, a deletion with
      a prune, an adjustment), and the model answers what is allowed, held, spent and
      left, for the store and for each reservation and run.
- [ ] The table of histories, run as tests against the model: each history of "The
      histories the reviews produced" below, by name, with the balances it must give;
      and generated histories, bounded and seeded, 4,096 of at most 24 events each from
      a seed written in the test, each checked against invariants 1 to 6.

**Done when** the model and the table agree on every named history; every invariant is
asserted over every history, named and generated, and a generated history that breaks one
is printed with its seed; the generated run takes under about 5 seconds; and a review of
the ADR by the Codex CLI, read-only, with the model and the table in hand and asked only
whether any history can under-count, finds none. That round is recorded in the ADR. A way
to under-count that it finds is a new named history, and the ADR and the model change
until it passes.

**Second pull request: the interface and the records.**

- [ ] `Ledger::reserve` cannot return without having made its names: what it returns can
      only be had from `Step::make`. `Ledger::settle`, called inside the publication
      that ends a run nothing called; a failure of it is a failure of that publication.
- [ ] One parser of the ADR's grammar in the core, which every reader of a name under
      `spend/` uses. Reading reservations answers each entry by itself, sound or damaged.
- [ ] `Allowance` as the ADR has it: debited before a request, given back to, in debt
      where an overrun put it, and a child's remainder handed to its parent when the
      child's end is durable and the parent still executes.
- [ ] The store's rule for a `put` under `requests/` and `spend/`, as the ADR decides.
- [ ] The record types, written one way and read strictly, in
      `crates/whizbang-core/src/spend/`; the test kit's fake ledger brought in line.

**Done when** these pass: a ledger that returns from `reserve` without making its names
does not compile, or is refused, as the ADR says which; each form of name parses to its
parts and back, and one that breaks the grammar in each way the ADR lists is refused,
shown escaped and cut; with one reservation's content damaged, the reading gives the
others and that one as damaged with the amount its name states; a `settle` that fails
leaves the run admitted and its caller told; an allowance in debt reports nothing to
spend, and a return to it pays the debt first; an amount given back to an allowance whose
run has entered its commit step is dropped; each record round-trips to the same bytes, is
refused over its bound from its entry's length before it is read, and with an unknown or
a missing field, escaped, cut and without a path (rule 3); the pin test
`known_a_name_that_was_made_can_still_be_replaced_by_put` is turned round, or kept with
the reason; every test of `admit.rs`, `nested.rs` and `commit.rs` passes against the fake
as changed; every claim of the ADR about code names its test (rule 5).
**Carry-over:** the rows on "never removed" and `put`, and on the ledger (the token, the
names). **Size:** medium, in two pull requests.

#### The histories the reviews produced

Each is a named history in the table, from the six rounds' outputs. Amounts are in whole
units for reading.

1. Two admissions, each told yes for the last of the allowance: one is admitted.
2. Two requests of one run, each within what is left alone and not together: one leaves.
3. Two requests that both fit, of different bounds, choosing one place: two places.
4. A request settled under its bound: the difference can be spent again.
5. A request that succeeds and reports no charge: its bound.
6. A fault at each stage: not sent, nothing; answered, its charge; unresolved, its bound.
7. A first hop handed data and a second refused: unresolved, not "not sent".
8. Reserve 10, two dispatches of 5, the first settles at 12, the second at 0 and gives
   back 5: nothing can be spent; the debt is 2.
9. The same, the run ending by itself: closed at 12, nothing released.
10. The same, interrupted with the second unsettled: 17.
11. The same, with an admission made while the second is unsettled: it sees 17 counted.
12. An admission that judged before a settlement over its bound and publishes after: it
    is asked again.
13. A settlement at 12 over a bound of 5, then its content damaged, the run deleted and
    pruned: no less than 12.
14. A reservation's content damaged, its run deleted and pruned, no request id: frozen at
    the amount its name states.
15. Reserve 10, every dispatch settled for 3 in all, then the reservation damaged: 10.
16. Reserve 10, dispatch 5, settle at 6, the run still open: 10, not 6.
17. A settlement that names no dispatch, and two dispatches at one place: that
    reservation frozen, the others as they were.
18. A child given 5 whose request settles at 12: nothing back; each run above 7 less.
19. One child overruns and a second then returns its unused share: the debt is paid first.
20. A share handed back after a child's admission was refused, to a parent in debt: the
    debt is paid first.
21. A parent that calls 20 children in turn, each using nothing: all of it at the end.
22. A child's settlement fails, or its backend panics, and the root then ends: closed as
    interrupted, nothing released.
23. A crash between dispatch and settlement: the full amount.
24. A crash after every request was settled and before the end: the settled total.
25. A crash with nothing dispatched: nothing.
26. Recovery, then recovery again: the second changes nothing.
27. A recovery whose last sync failed, then an open: confirmed, nothing settled twice.
28. A reservation ended while its settlement is already named: not settled again.
29. Two runs that end at once: each counted once.
30. A reset, then a reservation, then a reset: the reservation is of the closed books.
31. Freeze 10, reset to 20, adjust the frozen one to nothing: 20 to spend, not 30.
32. Adjust, then reset: the reset's amount.
33. An adjustment below a reservation's readable settlements: refused.
34. An adjustment upward after the difference was spent: refused when it cannot be held.
35. A cancellation after dispatch, no settling answer: the bound, unresolved.
36. A cancellation after dispatch, then an answer with a charge: that charge.
37. A run abandoned in a live process: held in full until the process is gone.
38. A store with no allowance: a call of nothing is admitted, a call over nothing is not.

### I3. A wrapper around each request

**Depends on:** I2. A section of the interfaces' ADR, which changes ADR 0006 and names
ADR 0009, whose tally records each request.

- [ ] `Around`, in the core: what stands between a handle and its backend. For each
      request it is told the run, the run at the root of its tree, the backend's
      declaration and quote, the classes of data the request holds, and the run's
      allowance and place for facts. Before the request it may refuse, with a fault and a
      refusal to note; after it, it is handed the answer or the fault.
- [ ] The order is fixed in `Context::new`: the handle's own checks (ADR 0006), then the
      tally, then the wrappers in three named places (the gate, the lease, the meter),
      then the backend. The tally is outside them, so a request is outstanding, and its
      run and every run above it not terminal, until the last wrapper has returned.
- [ ] What a wrapper's failure means: a wrapper that fails after the backend was
      called, and a backend that panics, do not account for the request as free. The
      run is abandoned (ADR 0008), with the failure, and nothing of it is released; the
      tally lets go only so that the run can reach that state.
- [ ] Each place is empty by default. A fake wrapper in the test kit records what it saw
      and can refuse or stop at a barrier.

**Done when** these pass: three fake wrappers see a request in the stated order, and none
sees one the handle's own checks refused; a wrapper that refuses stops the request before
the next sees it and before the backend, and the fault reaches the action; with a wrapper
stopped at a barrier after the backend answered, the run and its parent are not terminal,
and are once it is released; a wrapper's refusal is in the run's `outcome.refusals`; a wrapper that fails after the
backend answered, and a backend that panics under a wrapper, each leave the run abandoned
and not ended; with
no wrapper, every test of `cancel.rs` and `nested.rs` passes unchanged.
**Size:** small.

### I4. One way to a writer, with recovery before it

**Depends on:** I3. Two pull requests: the entry point and its states; then every writing
command on it, with the crash proofs. A section of the interfaces' ADR, which changes ADR
0007 (the alternative it turned down for now) and says, as "Decisions" does, that
recovery is lossy and pessimistic by intent.

- [ ] One entry point opens a store to write. It takes the writer lock, runs recovery,
      and only then hands the writer out. Recovery is a list of parts (`RecoveryPart`),
      each shown the current generation and adding to one publication of names made
      once. The store module itself names nothing above it: the entry point is the
      contract's, over `Store::open`.
- [ ] Three states, shared by everyone who asks for the same store value: initializing,
      ready, failed. Whoever asks while it is initializing waits for the answer; nobody
      is handed a writer before it is ready. If recovery fails, the lock is let go, the
      store is handed out to read only, and the failure is said without a path: as
      nothing published when it failed before its publication was visible, and as
      published and not confirmed when the last sync failed. An open confirms the
      generation it finds before it is ready.
- [ ] The executor, `store delete`, `store prune`, `store import` and the `spend`
      commands that write all open through it. A test holds the binary to no other way.
- [ ] A store at its generation limit refuses a publication that adds names, recovery's
      among them. Then the writer is handed out for deleting and pruning alone, which
      make the file smaller, and every other writing command is refused and told why.
- [ ] With no part, recovery publishes nothing and the store opens as before.

**Done when** these pass: a fake part's records are published before the writer is
handed out, all of the parts' records or none, shown by refusing each call of the
publication; each writing command, run on a store a killed process left, recovers before
it changes anything, one test for each command; two executors built at once over one
store value are both answered after one recovery, and a third built while it runs waits;
after a failed recovery every writing command is refused with the reason and a reading
one works; recovery run twice changes nothing the second time; with the limit set low by
`Store::limit_generations_to`, a recovery that cannot be published leaves `store delete`
and `prune` working and an admission refused, and after a deletion made room the next open
recovers; a recovery whose last sync fails
leaves no writer, and the next open confirms what is there, publishes nothing more, and
settles and ends nothing twice. Then:
`recover::a_crash_at_any_event…`, with a fake part, followed by another open, leaves the
part's records once and whole (rule 2); a part that panics is a failed recovery.
**Size:** medium.

### I4b. The interrupted record, and what readers say of it

**Depends on:** I4. Two pull requests: the record and the recovery's part that writes it;
then the readers. In the executor's files and `crates/whizbang/src/runs.rs`.

- [ ] A recovery's part: every run that is admitted, has no record and is not open in
      this process is ended with the record "What a crashed run is recovered to"
      describes, made from its admission alone, and for a request with an id its ending.
      A run whose admission cannot be read is ended with what its name gives. Its
      request id, if it had one, stays taken, and a request under that id is answered
      with the error of an unreadable record (ADR 0007's `CallError::Record`), never as
      a repeat and never as free: the fingerprint that tells a repeat from a conflict
      was in the admission.
- [ ] What each reader answers of such a run, as that section lists: `runs list`, `runs
      show`, a repeat, `status`, `wait`, `cancel`.

**Done when** these pass: a run a killed process left is ended, after the next open to
write, with a record that validates, holds exactly the admission's facts, and names every
other group in `outcome.lost`; a child and its parent, both left, are both ended, each
from its own admission; a run that is executing in this process, and one this process
abandoned, are left alone; a run with a damaged admission is ended and names the
admission as lost, and the same request and a different request under its id are both
answered with the error of an unreadable record; a marker secret planted in a damaged admission is in no byte the
recovery wrote or said (rule 4). Then: a repeat of the request is `Ended` with the error
`interrupted`; `status` and `wait` say the same and `wait` does not wait; `cancel` is
`NotAccepted(Ended)`; `runs list` and `runs show` show it, with what is lost, escaped;
`interrupted::a_crash_at_any_event…` followed by another open ends each run once
(rule 2).
**Carry-over:** the row on an admission that was visible and not confirmed, as far as the
next open; the new row on a fuller record of interrupted runs (owner: none; reopen on
need); the row on what running an action does not do yet (milestone 2), which says that
nothing marks a run a dead process left: this task marks it, and corrects the row.
**Size:** medium.

### I5. A policy before a send, and a checked send

**Depends on:** I4b. A section of the interfaces' ADR.

- [ ] `DispatchPolicy`, in the core: a value the executor holds, asked whether a recipient
      may receive given classes of data, and whether it may be probed. The default
      approves nothing. The gate's place in I3's order is given this policy.
- [ ] `Sender`, the sending interface of B2, as the executor hands it to a backend: every
      origin it is asked to reach goes through one check before data is handed over, and
      what was handed over is noted in the run's place for facts. The check is E2's and
      E3's; here it is a default that refuses everything, and a fake.

**Done when** these pass: with the default policy a fake backend that names a recipient is
refused at the gate's place and one that names none is sent; a `Sender` with the default
check hands nothing to any origin; a fake check that approves sees each origin before the
sending fake does; the policy's `{:?}` shows recipients cut and no credential (rule 4).
**Size:** small.

### I6. One snapshot of the config

**Depends on:** I5. A section of the interfaces' ADR, which changes ADR 0007's
`ConfigSource`.

- [ ] `ConfigSource` answers a snapshot or a refusal. A snapshot is the digest together
      with what it is the digest of: the roster and the backends it is of, made from one thing so
      that a model a call resolved is always held, the per-call cap, the dispatch policy,
      the secrets, and the settings a run records. A call is prepared under one snapshot;
      admission prepares again under the snapshot current then (ADR 0007); the run then
      executes under that one, whatever the file becomes.
- [ ] Secrets are kept for redaction for as long as the executor lives: a value a later
      snapshot no longer holds is still taken out of what is written.
- [ ] An admission's part (I1) is shown the snapshot its call was prepared under at
      admission.
- [ ] A digest alone is still a source, of a snapshot that holds nothing else, so the
      binary and 1A's tests are unchanged. A fake source in the test kit that a test
      swaps.

**Done when** these pass: a source swapped between prepare and admission is
`Stale::Config`; a source that refuses makes the next call refused with no run, and the
call after it is put right is admitted; a run admitted under one snapshot resolves a
backend, and is held to a cap and a policy, of that snapshot though the source was swapped
while it ran; a secret of an earlier snapshot is still redacted from a later run's record
(rule 4); a call cannot resolve a model the snapshot holds no backend for, and the pin
`asked::known_a_call_that_resolved_a_model_the_executor_does_not_hold_is_handed_no_handle`
is turned round.
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

**Depends on:** S3, S8, S10, S7b, S12, E4, E6, C5, M6b: every implementation it
wires, and through them every other side task of spend, egress, config and models.

- [ ] `wiring::runner` builds the executor with the real ledger, the config's source, the
      admission's parts (S3, S8), the wrappers (the gate, the lease, the meter), the
      recovery's parts and the inventory. With no config file it has no backend, as
      before.

**Done when** these pass, through the binary: with no config, `grow` runs as at `m1a` and
its run has a reservation of nothing; with a config that names a fake backend kind
registered by the test, a paid call is refused without an allowance and admitted with
one; a store left by a killed process is recovered by the next command that opens it to
write, and not by one that reads; the shipped description does not change.
**Size:** small.

### S1b. The fold

**Depends on:** I2. In `spend/` only. The real fold, held to I2's model.

- [ ] The fold: from the records and reservations a generation names, what is allowed,
      reserved, spent and unresolved, for the store, for one reservation, and for one run
      under it.
- [ ] The table of histories runs against the real fold as well: each history is written
      into a store through the record types and folded, and must give what the model
      gives.

**Done when** the real fold and the model agree on every named and every generated
history, and these pass: the fold gives the stated balances for a table of histories that
has one history for each state the ADR gives a reservation, and these: a reset;
an adjustment after an interruption and after a freeze; dispatches by a child under its
root's reservation; reserve 10, dispatch two of 5, one settles at 12, and then each of
the run ending, the run interrupted, and an admission made in between, which sees 17
counted while the second is unsettled; a reservation whose content is damaged and whose
run is deleted and pruned, counted at the amount its name states; an allowance driven
below nothing by an overrun, which reads nothing left; a history
with a record the fold cannot place (a settlement with no dispatch, two dispatches at one
place) freezes the reservation its name gives and stops nothing else; reserve 10, settle
every dispatch for 3 in all, damage the reservation's content: it counts 10; reserve 10,
dispatch 5, settle at 6 with the run still open: it counts 10, not 6; the fold reads records through names only, shown with
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

**Depends on:** I6, for the snapshot a part is shown. **Reads:** rascal `crates/rascal-core/src/config.rs` (`LimitsConfig`)
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

- [ ] A wrapper in `spend/` around any `Backends`. The roster already answers
      `Backends::bound` with the quote of one request, from the price a backend declares
      (B1, ADR 0010); the wrapper delegates to it and adds the per-call part, so that
      the bound a backend states for a need covers every request the call sends it. The
      roster's own file is not changed.
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
backend makes no record; a dispatch the store refuses at its generation limit sends
nothing. Then: two requests of one run, each within what is left alone and
not together, stopped after reading what is left and released together, end with one
dispatch durable and one request at the fake (rule 1); two requests that both fit, of
different bounds, stopped after each chose its place and released together, end with two
dispatches at two places, the second chosen again; `dispatch::a_crash_at_any_event…`
leaves the record whole or absent, and nothing was sent without it (rule 2).
**Size:** medium.

### S5b. Settlement of each attempt

**Depends on:** S5. **Reads:** rascal `crates/rascal-mask/src/filler/mod.rs` (`Charge`,
`spent`); squint `crates/squint-core/src/ledger.rs` (`settle_with_unresolved`).

- [ ] When a request ends, the meter settles it by the stage it reached, and gives back to
      the run's allowance what the bound held beyond the charge; or, where the charge is
      over the bound, takes the excess from the allowance of its run and of each run
      above it, each down to nothing. Not sent: nothing is
      charged. Answered: the cost the answer gives, or the bound when it gives none.
      Unresolved, a timeout among them: the bound, counted as unresolved.

**Done when** these pass: a request that succeeds with a charge under its bound is charged
that, and the run can spend the difference on its next request; one that succeeds with
none is charged its bound; one whose charge is over its bound is settled at the charge and marked as over, its
run and its parent each have that much less, a run left with nothing is refused its next
paid request, and an admission by another executor made straight after sees the store's
allowance reduced by it; a fault at each of the three stages settles as stated, one test each;
a request that times out is charged its bound; a request whose first hop was handed data
and whose second was refused is unresolved, not "not sent"; a settlement whose
publication fails, or is refused at the store's generation limit, leaves the dispatch
unsettled and the run abandoned (I3).
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
      holds unresolved, when that is over nothing) goes back to its parent's allowance
      while the parent runs (I2). A child whose charges passed its share gives nothing
      back, and the excess was taken from each run above it when it was settled (S5b).
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
backend its run was admitted with and no other; a child given 5 whose one request settles
at 12 gives nothing back, its parent and its grandparent each have 7 less, down to
nothing, and the root's reservation is closed at what was charged. Then: two children of one parent, stopped
after reading what the parent has left and released together, cannot both take the last
of it, and two requests of two children cannot both take the last of the root's
reservation (rule 1).
**Carry-over:** the row on settlement (a child's share not given back; a run's own
requests not debited). **Size:** medium.

### S7b. Nested calls under a crash

**Depends on:** S7, S6. In `spend/` and its tests only.

**Done when** these pass, each with a crash and the next open to write: a crash while a
child is admitted, while a child's request is dispatched, while it is settled, and after a
child ended and before its parent did; in each the root's reservation is settled once,
at its settlements where every dispatch under it has one and otherwise at the larger of
its amount and what is known against it, a child's overrun before the crash included; the parent and the
child are both ended as interrupted unless they had ended; a child that completed before
the crash keeps its outputs and its record; the same with the parent deleted before the
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

**Depends on:** S2. The store is named by `--store <dir>` (D8).

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
      unresolved stays counted, and the rest is released; a tree charged more than was
      reserved is closed at what it was charged and releases nothing. A closing that releases needs a
      durable settlement for every dispatch under the reservation, an unresolved one
      charged at its bound among them; where one is missing, as after a child's failed
      settlement or a backend's panic, the reservation is closed as interrupted in the
      same publication, at the larger of its amount and what is known, with nothing
      released.

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
each once (rule 1); a root whose child's settlement failed, and one whose child's backend panicked, each end
with the reservation closed as interrupted and nothing released; a settled reservation is never settled a second time, shown by ending
a run whose settlement is already named; a run whose end the store refused, which is
abandoned, keeps its reservation unsettled and is not counted as released.
**Size:** small.

### S6. After a crash: the money, never under

**Depends on:** S11b, I4b. Two pull requests: the recovery's part; then its crash and
damage proofs. A recovery's part (I4), in `spend/`.

- [ ] For every reservation that is not closed and whose run is not open in this process:
      it is closed as I2's ADR says. At its settlements, with
      the rest released, when every record under it reads and every dispatch has its
      settlement; otherwise as interrupted, at the larger of its amount and what is known
      against it; and frozen as well when an entry under it is damaged.
- [ ] No other reservation and no unrelated call is refused because of a frozen or
      interrupted one.

**Done when** these pass: a process that died between dispatch and settlement leaves,
after the next open to write, the reservation charged in full and nothing released; with reserve 10, two dispatches
of 5 and one settled at 12, it is charged 17; one
that died after every request was settled and before the run ended is charged the
settled total and the rest is released; one that dispatched nothing is charged nothing;
the fake counts no second request in any of them; the same three after the run was
deleted and the store pruned before the restart, and with the reservation's content
damaged as well, where the amount is the one its name states; a run abandoned by a process that then
ended is settled by the same rule. Then: `settle_interrupted::a_crash_at_any_event…`,
followed by another open, settles each reservation once (rule 2); with a damaged
dispatch, a damaged settlement and a damaged reservation planted, one at a time, that
reservation is frozen at no less than its full amount, every other is recovered, and a paid call that
the rest of the allowance covers is admitted; no history in a table of twenty, each a
sequence of dispatches, settlements, damage and crashes, ends with less charged than its
readable settlements say, or with a reservation both frozen and released.
**Carry-over:** the row on settlement (a run that did not end). **Size:** medium.

### S12. A person's correction

**Depends on:** S9, S6.

- [ ] `whizbang spend adjust <run> --charge <usd>`: for a reservation that recovery
      interrupted or froze, a new record that sets what it is charged, from its readable settlements, as the least, to
      the larger of its full amount and what is known against it, and releases the rest. It names the reservation by its run, says
      who would be asked nothing (there is no user), and records the time. The records
      it corrects stay. A reservation that is neither interrupted nor frozen is refused.
      A reset closes the books before it: an adjustment of a reservation made before the
      last reset is recorded and changes nothing the new allowance can spend.
- [ ] `spend status` lists the interrupted and the frozen reservations with their runs.

**Done when** these pass: an interrupted reservation adjusted to a lower charge releases
the difference and the next paid call can use it; a frozen one is unfrozen by its
adjustment, and the damaged content is still in the store and still named; an adjustment
below the reservation's readable settlements, one over what it counted for, of a settled reservation and of a run the store does not know
are each refused; a second adjustment of the same reservation makes a second record and
the fold takes the last; freeze, reset, then adjust to nothing leaves the new allowance
as the reset set it; adjust, then reset starts from the reset's amount; an adjustment
upward after the difference was spent is refused when the allowance cannot hold it; the command opens through I4's entry point, so it recovers
first; its output and its refusals show names escaped and cut, and no path (rule 3);
`adjust::a_crash_at_any_event…` leaves it done or not done (rule 2); no call the
executor accepts can write an adjustment, shown as S9 shows it for a reset.
**Carry-over:** the row on the ledger (a damaged reservation). **Size:** small.

### C1. The config: its file, its shape, its digest

**Depends on:** B0b. The config's file is named by a flag, as the store is (D8). **Reads:** squint `crates/squint-core/src/config.rs`,
`docs/adr/0004-toml-config.md` and `schemas/config-v1.json`; rascal
`crates/rascal-core/src/config.rs` and `schemas/rascal/config-v1.json`.

- [x] An ADR: one TOML file; where it is found; its tables (`backends`, `spend`, `egress`,
      `models`, and `tools`, which is refused until milestone 2 gives it a shape); how
      several named backends of one capability and a default are written, with the example
      of `SPEC.md` section 4 either read as it stands or corrected in this pull request;
      that a credential is named by its variable and never held; the digest, of the
      settings and not of the file's bytes, and exactly what enters it; which settings a
      run records.
- [x] The type and its strict reader in `crates/whizbang-core/src/config/`, and
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

- [x] An env file is read for the variables the config names and no others. A value in
      single or double quotes has them taken off; a value shorter than the least a secret
      may be is refused, naming the variable and never the value or its length beyond what
      ADR 0005 allows.
- [x] The secrets the executor redacts are every credential the config names that has a
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
- [ ] The check behind a `Sender` (I5) for the first address.

**Done when** these pass, each with an executor a test builds with the policy: with the
empty policy every request to a backend with a recipient is refused and the fake counts
none; a recipient approved for prompts and not for source images is refused a request
that holds a source image, and sent one that holds a prompt alone; a part with no stated
class is refused unless every class is approved; a refusal answers the action with a
fault, and no other backend, remote or local, is asked in its place; a refused request
has no dispatch record and no charge, a fake meter in the place after the
gate having seen nothing; a run whose action catches the refusal and completes has it in
its record, with the recipient and the reason.
**Carry-over:** the row on squint's `recipient`. **Size:** medium.

### E3. Every hop, and a probe

**Depends on:** E2. **Reads:** squint `crates/squint-providers/src/http.rs`
(`same_origin`, `MAX_REDIRECTS`). Two pull requests: the hops; then the probe and the
download.

- [ ] Everything a `Sender` is asked to reach is checked against the recipient: a
      redirect, an upload address and a returned download address, as the first address
      is. A credential is attached only for the origin it was configured for.
- [ ] The download interface is checked the same way against the origins a catalogue entry
      names. A probe is refused unless the config approves probing that recipient, apart
      from any class of data.

**Done when** these pass, against the sending fake of B3: a redirect to an origin that is
not the recipient or one of its upstreams is refused and nothing is sent to it; a redirect
to an approved upstream is followed without the credential; an upload address and a
download address outside the recipient's are each refused; a chain longer than the limit
is refused. Then: a model's download that is redirected off its catalogue's origins is refused,
and its request carried nothing but the address; a probe is refused by default, sent when
approved, carries no user data, and its redirect off the recipient is refused.
**Size:** medium.

### E4. The run's recipients

**Depends on:** E3.

- [ ] Each send is noted where it is made, at the `Sender`, in the run's place for facts:
      the origin that was reached and the classes of data. A run's `egress` list is those
      notes. A send the gate or a hop's check refused is in the record as refused.

**Done when** these pass: a run that sent a prompt and a derived image to one recipient
lists it once with both classes; a run whose request went on to an approved upstream lists
the upstream as reached, and an approved upstream that was not reached is not listed; a
request that failed at its second hop lists the first, which had the data; a run that
a process left after it had sent data is recovered as interrupted, with `egress` named in
`outcome.lost` and an empty list; a run whose only request was refused lists the refusal and
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
      use; holds as file locks. It edits nothing in `SPEC.md`: the two sentences named
      under "Decisions" are B0b's.
- [ ] The catalogue and the installation record are added to `whizbang-backends`, the
      crate B0b made, as types read strictly. Nothing installs yet.

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

**Depends on:** B0. Scripts only.

- [x] `sk_shape` in `scripts/scan.sh` finds a key of its family wherever it starts, with
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

**Depends on:** B0. Scripts only.

- [x] `scripts/tests/test-deny.sh`: in a throwaway copy of the workspace's manifests, a
      dependency from an unknown registry and one from a git source each fail
      `cargo deny check sources`.

**Done when** the test passes for both, passes on the tree as it is, needs no network,
ends with `finished`, and is run by `just test-scripts`.
**Carry-over:** the row on `cargo deny` (deferred from milestone 0). **Size:** small.

### X3. A pull request's title and body, scanned

**Depends on:** X1. Scripts and two skills.

- [x] One script scans the two files of a pull request's text for what is found today
      only by reading: a bare agent or model name in a sentence, a value of the repo's
      local `.env`, a path that starts with `~`, and an attribution line behind a
      leading mark. `kickoff` and `review-merge` use it in place of their own commands.

**Done when** its test passes: each of the four is refused, the fourth behind each mark
the carry-over row lists; a text with none passes; a refusal says where and never shows
the value; the test file ends with `finished`; the tests on the skills' exact lines pass
with the two changed lines.
**Carry-over:** the row on a pull request's title and body. **Size:** small.

### X4. A push made around the hooks

**Depends on:** X3. Scripts only.

- [x] A push made with `GIT_DIR` and `GIT_WORK_TREE` set from another folder is refused,
      or what it published is scanned before a merge: `review-merge`'s range scan takes
      every branch the pull request's commits are on and the branch's name. Which of the
      two is the builder's finding, written in the row.

**Done when** a test makes such a push to a throwaway remote and it is refused or its
material is found by the scan that follows; or the builder shows neither can be done from
a script, stops, and the gap goes to the owner to accept in a line of the row.
**Carry-over:** the row on a push around the hooks. **Size:** small.

### X5. A script edited while it runs

**Depends on:** X4. Scripts only.

- [x] `scripts/scan.sh` and the other long scripts run from a function read whole before
      it starts, or from a copy.

**Done when** a test edits the file during a run, at a barrier the test holds and not by
a sleep, and the run finishes as the program it started as; `scripts/tests/test-syntax.sh`
holds each long script to the form.
**Carry-over:** the row on a script edited while it runs. **Size:** small.

### X6. No `grep -q` after a pipe

**Depends on:** X5. Scripts only.

- [x] The sixteen lines under `scripts/` that end a pipe with `grep -q` send the output
      to `/dev/null` instead, and `scripts/tests/test-syntax.sh` refuses a new one.

**Done when** no script under `scripts/` has the pattern, a planted one fails the test,
and every script test passes on both runners.
**Carry-over:** the row on `grep -q` after a pipe. **Size:** small.

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

Every row of `docs/carry-over.md` whose owner names 1B, read again from `main` at the
tag `m1a`: twenty-five rows.

| Row | Task, or new owner |
|---|---|
| A pull request's title and body: four things found only by reading | X3 |
| A push made around the hooks | X4, or the owner's acceptance if no script can close it |
| A script edited while it runs | X5 |
| `grep -q` after a pipe, sixteen lines | X6 |
| The roster a call is resolved against and the backends an executor holds are given apart | I6 (one snapshot makes both), C4 (built from the config), W1 (the binary). The half that waits for a real backend stays with 1C |
| A run calls at most 256 actions: the cost of a publication is 1B's | Follows the row on what every publication costs, to milestone 2 |
| The secrets pattern misses a key directly after a hyphen, an underscore, a letter or a digit | X1 |
| "Never removed" has one limit left: `put` replaces a made name | I2 |
| The executor is built with no backends and the digest of no bytes | C4 (the shape and digest: C1; the binary: W1) |
| The run record's empty lists: `asked.settings`; `spend.requests` and settled spend; `egress` | B4 defines the elements; C5, S10 and E4 fill them. `egress` was owned by "the first backend that sends data": the gate is where a send is known, so 1B takes it, filled by fakes. `device` and `cached` stay with 1C |
| The ledger: the real one, the binary built with it, the token, a damaged reservation, the names' segments, a paid request with no bound | I2 (the token, the names), S2 (the real ledger), S4 (no bound), S6 and S12 (a damaged reservation: frozen, then corrected), W1 (the binary) |
| Settlement: a run that ends; one that did not; a child's share not given back; a run's own requests not debited | S11; S6, at the full amount unless every dispatch was settled, with I4b ending the run; S7; S5 and S7 |
| An admission that was visible and not confirmed takes its id | I4b ends it at the next open to write. Ending it at once in the process that made it: **milestone 2**, with the first door that keeps a process alive across runs; until then the limit stays, and the row says so |
| What every publication costs as ids and reservations accumulate | **Milestone 2, before request ids are used in volume** (the measurement at 100,000 ids, S13 of draft 5). 1B adds a ledger record to each admission and two to each paid request, and each of its publications has a case at the 64 MiB limit, where it is refused and nothing is released (I4, S5, S5b); the row gains that |
| Whether a failed call may have been charged | B1b (the stage on a fault), S5b (the settlement) |
| What a provider adapter said of itself: `pricing`, `recipient` | B1 and S4 (`pricing`); B2 and E2 (`recipient`). Image preparation stays with milestone 2, and a quote worked out from a request's shape (its tokens and images) went there with it (B1, ADR 0010) |
| What redaction does not find; quotes around a value; the variable named | C3 (the forms, UTF-16 among them, and JSON as JSON); C2 (quotes, the variable, the least length) |
| `max_spend_usd`, the cap of one call | S3. `set` stays with milestone 3 |
| Parallel installs of models sharing a file each download it | **1C, before a real download** (M4 of draft 5, with the removal of a shared file) |
| No test plants an unknown registry or git source for `cargo deny` | X2 |
| A model never shows as out of date (R2) | M1 (recorded), M6 (reported) |
| Uninstall signals a process id read from a file (R3) | M5d |
| A configured model that is not in the catalogue can read as ready (R4) | M6 |
| A hold is judged live by its process id alone (R5) | M5 |
| Untested cases around installs (R6) | M2b (a signal during a download), M5d (a removal racing an install), M6 (a reinstall between a call and its retry; a corrupt installation record) |

New rows this plan asks for, added by B0, each with the task that brings it in line
with what was built: a fuller record of what an interrupted run did, owner "none; reopen
on need" (I4b); `whizbang models list|get|remove` and their guide, 1C (M6b); `whizbang
backends status`, 1C (Z1); the real network client and the shipped catalogue, 1C (Z1).
And one that B0b added, which Z1's list asks for: what the worker's queue owes to
removal, 1C (M5b).
The row on what running an action does not do yet (milestone 2) keeps the bound on a
ledger being asked again; 1B adds none.

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
| 4 | 2026-11-13 | 6 | 2 | 4 | 1 |
| 5 | 2026-11-13 | 8 | 3 | 5 | 0 |
| 6, money only | 2026-11-13 | 10 | 3 | 4 | 0 |

No round followed the sixth: see "After round 6", below.

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

**Round 4, on draft 6, and what changed in draft 7.** The round was asked first whether
the recovery of a crashed run is now coherent. Its answer is no. It found that every
carry-over row is placed, that the cuts keep the exit line of `SPEC.md` section 16, and
that no task is clearly over size.

Applied in draft 7, because they do not touch recovery:

- Major: W1 could be wired before what it wires. It now needs every implementation.
- Major: B6 and S3 need the config's snapshot, which I1 did not show them. They follow I6,
  which shows a part the snapshot. B6 has an ADR of its own. `SPEC.md` is edited by base
  tasks only.
- Minor: the bounds on started runs are stated as ADR 0008 has them, and B3 no longer
  claims the fakes lack barriers.

**The findings on recovery, and the owner's decision of 2026-11-13.** The two blockers
and three of the majors were about recovery, as a blocker had been in every round. The
owner decided that 1B stops reconstructing what a crashed run did: the record of an
interrupted run is made from its admission alone and names the rest as lost; its money
is charged in full unless every dispatch is shown settled; a damaged record freezes one
reservation and stops nothing else; a person corrects by a new record; recovery runs
where a writer is handed out; and readers have a stated answer. Draft 8 applies that:

- B6 (the prepared call kept at admission), the durable send notes and the third state
  of `egress` are gone, with everything of S6b that rebuilt a record. Nothing is kept
  durably for recovery's sake.
- I4 is the one way to a writer, used by every writing command, with recovery before it
  and the states initializing, ready and failed. I4b is the interrupted record and its
  readers.
- S6 is the conservative settlement. S12 is the person's correction, `spend adjust`.
- I3 says what a failed wrapper and a backend's panic mean; S5b has the case of a first
  hop that was handed data and a second that was refused.

**Round 5, on draft 8, and what changed in draft 9.** The round was asked whether
recovery is now coherent and whether any path releases or under-counts money. It found
the admission-only recovery workable, and nested runs, deleted runs, abandoned runs, an
unconfirmed admission, a crash inside recovery, recovery twice and two openers at once
all coherent. Its answers were still "no" and "yes", for the findings below.

Applied in draft 9, each small:

- Blocker: "if recovery fails, nothing is published" was untrue where the publication
  was visible and not confirmed. I4 and the recovery section tell the two apart, and an
  open confirms what it finds.
- Major: a damaged admission cannot answer a repeat, since the fingerprint was in it.
  The id stays taken and is answered with the error of an unreadable record.
- Major: a reset and an adjustment had no stated relation. A reset closes the books.
- Major: B4's rule on nulls refused ordinary records. It is limited to fields lost.
- Major: S3 and S8 needed fields of the executor's that no base task added. I1 adds them.
- Major: the spec was not assigned the recovery decision. B0b makes those edits.

**Two blockers on money, each called structural, and the owner's decision.** They were
handed back with the proposals below; the owner accepted both on 2026-11-13, and draft
10 applies them as "The ledger's names and arithmetic", owned by I2 and S1.

1. *A damaged reservation may have no readable amount.* The amount of `reservation/1`
   is in its content (ADR 0007), and for a run that was deleted and pruned, with no
   request id, no admission is left to say it; and reading reservations fails as a whole
   when one cannot be read. So "frozen at its full amount" cannot always be done.
   Proposed: the name the ledger makes in every admission (I2, S2) carries the run and
   the amount in the name itself, which lives in the generation file and not in content;
   a dispatch's and a settlement's names carry their run and reservation the same way;
   and the fold takes a damaged entry as one frozen reservation, not as a failure of the
   whole reading. No fact about a run is reconstructed by this.
2. *The full-amount rule can charge less than what is known.* A request may settle over
   its bound (S5b). Reserve 10, dispatch two requests of 5, one settles at 12, crash:
   the full amount is 10, less than the one readable settlement. Proposed: what an
   interrupted reservation is charged is the larger of its full amount and the sum of
   its readable settlements with the bounds of its unsettled dispatches; a charge over
   its bound is debited from the allowance of its run and of each run above it, never
   below nothing, and a child whose charges passed its share gives nothing back; S1b
   and S7b gain that sequence and its nested form.

Both are in the ledger's records and arithmetic (S1, S1b, S5b, S6, S7, S7b, S11, S12)
and change no interface task but I2's naming.

**Round 6, on draft 10, money only, and what changed in draft 11.** It was asked whether
any path releases or under-counts money and whether the names and arithmetic cohere. It
found these conservative as written: admission against admission, settlement under, at
and over the bound before a later refund, cancellation, a refused end, the crash
boundaries, recovery twice and unconfirmed, and both orders of reset and adjust. Its
answers were still "yes" and "no".

Applied in draft 11, each small:

- Blocker: a child's failed settlement let its root close and release. A closing that
  releases needs a durable settlement for every dispatch; otherwise it is closed as
  interrupted (S11, S11b).
- Major: two dispatches of different bounds could take one place. A `place` name is the
  guard (the names' table, S5).
- Major: three rules were stated two ways. Frozen is never lowered by the all-settled
  exception; an open reservation counts the larger of its amount and what is known; a
  history the fold cannot place freezes one reservation (the arithmetic, S1b).

**Not applied: two structural blockers and two majors that hang on them, with what is
proposed.**

1. *Blocker: an overrun clamped at nothing forgets a debt.* Reserve 10, dispatch two of
   5, the first settles at 12 and the allowance stays at 0, the second settles at 0 and
   gives back 5: the run has 5 to spend after spending 12 against 10. Proposed: an
   allowance holds a signed balance and reports what can be spent as never below
   nothing; every return (an unused bound, a child's remainder, a share given back
   after a refused admission) first pays the debt. I2 owns it.
2. *Blocker: a damaged settlement loses its amount.* The charge is in the settlement's
   content, so damage makes the fold fall back to the dispatch's bound, which an overrun
   exceeded. Proposed: the settlement's name carries its charge
   (`spend/settled/<run>/<n>/<micros>`), with `spend/place` style guard
   `spend/settling/<run>/<n>` for uniqueness; an adjustment's floor uses those amounts.
3. *Major: an admission can straddle an overrun settlement.* It judges on a generation
   in which the settlement is not yet there, and no name of the two collides. Proposed:
   a settlement over its bound also makes the next `spend/turn/<n>`, so an admission
   that judged before it is refused by the store and asked again.
4. *Major: the grammar is not whole.* It leaves out the executor's
   `spend/reservations/<run>`, the `turn` names and the names of an allowance and a
   reset, and nothing durable ties a reservation to the reset it falls under. Proposed:
   I2's grammar lists every name a reader accepts apart from those `Step::make` may
   make; a reset is `spend/reset/<k>/<micros>`, and a `held` name carries its epoch,
   `spend/held/<k>/<run>/<micros>`.

**A recommendation with them.** Six rounds have each gone one level deeper into a
ledger that is being designed inside a plan. The findings are now of the kind a table
of histories run as tests finds in minutes and prose review finds one at a time. It is
proposed that the plan stop here on arithmetic: keep the invariants (never under; one
closing; damage freezes one reservation; names carry every amount the fold needs), make
I2 and S1 a design task whose ADR is reviewed together with a small executable model of
the fold and a generated table of histories, the six rounds' sequences among them, and
let that review, with code in hand, be where the arithmetic is settled.

**After round 6: the owner's decision of 2026-11-13, and draft 12.** The recommendation
above was accepted. The plan has had six rounds. Its scope, its order and every area but
the ledger's arithmetic are settled: no round's finding outside that area is open. The
arithmetic is deliberately not settled here. The plan keeps "The ledger's invariants";
the names and sums the rounds reviewed are in the appendix, marked as a starting point;
and task I2 decides them in an ADR that is reviewed, by the Codex CLI, together with an
executable model and a table of histories, the 38 sequences these rounds produced among
them. The four findings left open by round 6 are in the appendix and are histories 8, 12,
13, 19, 20, 30 and 31 of that table. No round was run on draft 12, by that decision.

**The final revision, draft 13, after the tag `m1a` (2026-11-14).** No round was run on
it: it changes no scope. What changed:

- D8, D9 and D13 are stated as fact, from `docs/plans/m1a-note.md` and ADR 0009, and B0
  is a short check of the carry-over rows.
- The carry-over rows were read again at the tag. Six are new to 1B: four tooling rows
  the exit check re-owned (X3 to X6, small tasks of the scripts strand), the roster and
  the held backends given apart (I6, C4, W1), and the publication cost under the limit of
  256 calls (with its row, to milestone 2).
- What 1A decided late is taken in: a run is handed the backend its call resolved and its
  tally records each request (I3); every list of a record is bounded (B4); a generation
  file has a limit, so each publication 1B adds has a case at it (I4, S5, S5b); the
  binary has one door (I4). The test rules follow `AGENTS.md`, "Threads and processes".

## Risks

| Risk | Effect | What we do |
|---|---|---|
| A store fills while 1B's publications add names | A settlement or a recovery is refused | Each has its case at the limit: nothing is released, the run is abandoned, and deleting still works (I4, S5, S5b) |
| An interface fixed early does not fit the strand that implements it | A base task is reopened and a strand waits | Each interface is the outline a review gave after three rounds of the tasks it serves; its task ships a fake that the strand's tests are first written against; a misfit is changed by a base task, never worked around in a strand |
| The ledger under-counts money on some path | The allowance is overspent without an error | Six rounds of prose review each found one; so I2 settles the arithmetic against an executable model, 38 named histories and 4,096 generated ones, each asserted against the invariants, and every later ledger task is held to the same table |
| Recovery's losses are felt | Someone needs to know what an interrupted run did | It is in no record, by decision; the carry-over line says who reopens it |
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

## Appendix: the ledger's names and arithmetic, a starting point for I2, not decided

This is where six review rounds left the detail. Nothing here binds: task I2 decides it,
held to "The ledger's invariants". It is kept so that the work is not lost.

**Open when the reviews stopped** (round 6), with what was proposed:

1. An overrun clamped at nothing forgets a debt (history 8). Proposed: an allowance holds
   a signed balance and reports what can be spent as never below nothing; every return
   pays the debt first.
2. A damaged settlement loses its amount (history 13). Proposed: a settlement's name
   carries its charge, `spend/settled/<run>/<n>/<micros>`, with a guard name
   `spend/settling/<run>/<n>` for uniqueness; an adjustment's floor uses those amounts.
3. An admission can straddle a settlement over its bound (history 12). Proposed: such a
   settlement also makes the next `spend/turn/<n>`, so an admission that judged before
   it is refused by the store and asked again.
4. The grammar is not whole (histories 30 and 31). Proposed: it lists every name a
   reader accepts, apart from those `Step::make` may make; a reset is
   `spend/reset/<k>/<micros>`, and a hold carries its epoch,
   `spend/held/<k>/<run>/<micros>`.

**Applied before they stopped,** and in the text below: a closing that releases needs a
durable settlement for every dispatch (history 22); a `place` name keeps two dispatches
from one place (history 3); frozen is never lowered by the all-settled exception, an open
reservation counts the larger of its amount and what is known, and a record the fold
cannot place freezes one reservation (histories 15, 16 and 17).

**The text as it stood in draft 11.**


Written after the fifth review round, from the two resolutions the owner accepted then,
and changed once after the sixth. It is no longer what any task is held to.

**What a name says.** A name lives in the generation file, not in content, so it is
there when content is damaged or pruned. The ledger's own names say what the money
needs:

| Name | Made | Says |
|---|---|---|
| `spend/held/<run>/<micros>` | By the ledger, in the admission's publication, for a run nothing called | That run's reservation and its amount |
| `spend/dispatched/<run>/<n>/<by>/<micros>` | By the meter, before a request leaves | The reservation it draws on (the root's run), its place `n` under it from 1, the run that sent it, and its bound |
| `spend/place/<run>/<n>` | By the meter, in the publication of dispatch `n` | Nothing but that place `n` under this reservation is taken: two dispatches that chose one place cannot both be published, whatever their bounds |
| `spend/settled/<run>/<n>` | By the meter, when the request ends | Which dispatch it settles; the charge is in its content |
| `spend/closed/<run>` | By the ledger, in the publication that ends the root's run, or by recovery | That the reservation is settled; the totals are in its content |
| `spend/frozen/<run>`, `spend/adjusted/<run>/<k>` | By recovery; by a person's command, `k` from 1 | That the reservation is frozen; its `k`th correction |

- **A strict grammar, parsed as untrusted.** A run is a `RunId` (ADR 0004); an amount
  and a count are decimal digits with no leading zero, at most 20 and 10 of them; a name
  is at most 200 characters (ADR 0007). A name under `spend/` that is not of one of
  these forms, to the character, is damage, and is shown escaped and cut.
- **Nothing secret is in a name:** a run's id is made by the executor, and the rest are
  numbers.
- **How it sits with ADR 0007.** The executor still makes `spend/reservations/<run>`,
  whose content is `reservation/1`: the request, the admission's digest, the amount, the
  start. That record remains the detail. The ledger's `held` name is the name ADR 0007
  says a ledger makes in the step, now with a fixed form, and it is what the fold counts
  from. Both are made in the one publication, so a generation has both or neither. The
  name that keeps two admissions from both taking the last of the allowance is the
  ledger's as before (`spend/turn/<n>`), and a dispatch's `place` name does the same
  under one reservation.
- **A damaged entry is one frozen reservation,** never a failure of the whole reading: a
  `held` name whose reservation record is missing, unreadable or says another amount (the
  larger of the two is taken); a dispatch or a settlement whose content cannot be read or
  does not agree with its name; a name of no known form that names a run. Reading
  reservations no longer fails as a whole when one cannot be read.

**What a reservation counts for,** in the fold, at any moment:

- *known against it* = the charges of its readable settlements, plus the bounds of its
  dispatches that have no readable settlement;
- *open* (no `closed` name): the larger of its amount and what is known against it;
- *closed by its run's end*: what is known against it, which then is its settlements
  alone, and unresolved ones at their bounds; what the amount held beyond that is
  released;
- *closed by recovery as interrupted*: the larger of its amount and what is known
  against it, unless every dispatch has a readable settlement, when it is as a run's end;
- *frozen*: the larger of its amount and what is known against it, whether or not every
  dispatch has a settlement: damage rules out the release, and nothing lowers it but an
  adjustment;
- *adjusted*: what the last adjustment says, which is never less than its readable
  settlements.

What the store has left is its allowance less the sum of those, and never reads below
nothing. So it is never more than the truth: every term is at least what was really
charged.

**A charge over its bound** is a fact, recorded and debited, not refused: the money is
spent. This holds for a run that ends by itself as for one that is interrupted.

- The settlement records the charge the answer gave, and that it was over.
- In memory, the excess is taken from the allowance of the run that sent the request and
  of each run above it, each down to nothing and no further. A run with nothing left
  sends no further paid request and makes no further paid call.
- A child gives back to its parent its share less what was charged or is unresolved
  under it, when that is over nothing; a child whose charges passed its share gives
  nothing back.
- In the ledger, an open reservation counts for the larger of its amount and what is
  known against it, so once the settlement is durable what is known has grown, and an admission made while the run still executes already sees
  the allowance reduced; and at its end it is closed at its settlements, which may be
  more than its amount.
- A later admission is judged against what is left after all of that. Where the overrun
  took the allowance to nothing, it is refused with `OverAllowance` until a reset.
