# 0008. Executing a call: one commit step for outputs and the completed status

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, with the second pull request of task A7. Added: "Status, waiting,
and a run that was started", which is built. Changed: the paragraph that says which part
builds what; what this record does not decide yet.
Amended: 2026-11-13, in the same pull request, after its review. Added: "How many runs
are started at once" and a run whose thread could not be made, under "Status, waiting,
and a run that was started"; what a wait is told of a run that is deleted; when a wait
answers at the end of a run, and how long a run stays in the table for that; what keeps
a test from making threads without end. Changed: a progress message is cut as the other
texts are.
Amended: 2026-11-13, with the pull request that followed. Changed, under "What keeps a
test from making threads without end": how often the guard counts, when a guard under a
guard leaves the watching to the one above, what the Linux limit does when the user's
threads cannot be counted, that no group of lints is allowed, and what `just test-only`
takes.
Spec: section 3, "The execution contract", "What a run publishes", "Storage" and "What
every run records"

## Context

`SPEC.md` section 3 says an action's outputs are committed together when it completes;
that an action that fails or is cancelled commits no outputs; that outputs and the
completed status are committed in one step; that publication is atomic and survives a
crash; and that every run, whatever its outcome, writes one record.

What there was to build it on:

- The executor of ADR 0005 and ADR 0007: a call is prepared, admitted in one publication
  that makes its run, run, and ended in a second publication that adds the journal, the
  diagnostics and the record to the run (`Publication::close_run`) and, for a request
  with an id, makes the id's ending. A run is reported after that publication is durable.
- An action that handed back `Produced`, which held nothing: a completed run had no
  outputs, and nothing but `whizbang store import` put an artifact in a store.
- A panic in an action unwound through `Executor::submit`: the run stayed admitted, with
  no record, and a repeat of its request was told the run had not ended (ADR 0007).
- The store: an artifact is stored under the name `artifacts/<type>/<identity>`, outside
  runs, as its canonical file (ADR 0002); a publication is one replaced pointer, so
  everything in it becomes visible together or not at all; `Publication::put` replaces
  what a name held.
- Rascal's rule for its workspace (`crates/rascal-core/src/workspace/commit.rs`):
  a cancellation was looked at once, before the writer lock was taken, and a commit that
  held the lock ran to its end. The plan for checkpoint 1A says that rule is not carried
  over without being restated.

Task A7 of the plan for checkpoint 1A is delivered in parts, and this record is written
with them. **The first part, which this record came with, builds** the commit step and
what a panic does. **The second builds** a run's status and progress while it executes,
waiting for a run, and starting one without waiting. The rule for a cancellation and the
commit step, cancel itself, nested calls with their allowances, and what makes a parent
terminal are the later parts': each amends this record with the code that builds it, and
until then nothing here says how they work. Rascal's rule is restated with the
cancellation, in that amendment; nothing cancels a run yet.

The code is in `crates/whizbang-core/src/contract/` (`engine.rs`, `registry.rs`),
`crates/whizbang-core/src/artifact/stored.rs` and
`crates/whizbang-core/src/store/mod.rs`; the tests that hold it to this record are in
`crates/whizbang-core/tests/it/contract/commit.rs`, `store/runs.rs` and `store.rs`, and
`crates/whizbang/tests/it/runs.rs`.

## Decision

### What an action hands back

`Action::run` returns `Produced`: the artifacts the action made, each with the name of
the output port it is returned at (`Produced::nothing().with(port, artifact)`). Handing
an artifact back commits nothing, and an action has no other way to put an output in the
store: the store an action is handed reads only (`Context::store`).

What `{:?}` prints of a `Produced` is each output's type, port and identity, and never a
plane or a pixel.

### What is handed back is held to the ports the call returns

When an action returns `Produced`, the executor holds it to the output ports the
prepared call returns (ADR 0004: the ports of the descriptor whose condition holds for
the arguments the call gave), before anything is published:

- an artifact at a name that is no such port is refused, a port the descriptor declares
  for other arguments included;
- an artifact of another type than its port's is refused;
- a port with more artifacts than its count allows, or fewer than it requires, is
  refused.

A refusal ends the run as an error of the type `output`, with a sentence of fixed words,
the numbers, and the port's name as the action wrote it, escaped and cut at 64
characters. The secrets are taken out of that name first, while it is whole: a secret
that the cut went through, or that the escaping changed, would no longer be found, and
what was left of it would be reported and written
(`commit::a_secret_in_a_port_name_that_is_cut_is_taken_out_before_the_cut`). Nothing of
what was handed back is committed, the artifacts that were right
included (`commit::what_an_action_hands_back_is_held_to_the_ports_its_call_returns`).

What is not checked: that an output belongs to the image its port says it is on
(`OutputPort::on`), that an image is derived from the one its port names
(`derived_from`), that a mask was rebound (`rebound_onto`), and that an artifact's origin
names the action that handed it back. The descriptor states these and nothing holds an
action to them yet (`docs/carry-over.md`).

The outputs of a run are listed, in its outcome and in its record, in the order of the
descriptor's ports, and within a port in the order they were handed back.

### The commit step

The end of a run was one publication (ADR 0007). It still is, and for a run that
completed it is the commit step: **one publication** that

- gives the run a reference of its own to each output, among the run's names: the
  name `outputs/<port>/<n>`, where n is the output's place among those returned at
  that port, from 0, as a committed output, with the artifact's canonical file as
  content;
- gives each output its artifact's name outside runs, `artifacts/<type>/<identity>`
  (ADR 0002), as a committed output, with the same file, which is how a later call
  takes it as an input;
- adds the journal, the diagnostics and the record to the run, the record saying
  `completed` and listing every output by port, type and identity;
- for a request with an id, makes the id's ending.

The store's publication is one replaced pointer. So a reader sees a run that
completed, with its reference to every output it lists and every one of them under its
artifact's name, or a run that has not ended, with no reference to an output; there is
no generation in which the run holds its reference to one output without the other's,
an output without the status, or the status without an output. An artifact the store
held under its name before the run is there before and after, whatever the run does
("An output the store already holds", below). The completed outcome is returned after
that publication is durable, as every outcome is.

A run that ended any other way (an error, an invalid input, a panic, a refusal of what
was handed back) is ended by the same publication without the first two of the four: it
commits no output, and holds no reference to one.

Two tests hold this. One loses power before each call that changes the disk between the
moment a fake action hands back two artifacts and the moment its run is reported, under
the ways the test kit's disk can lose what was not yet durable: every name undone,
every name kept, every choice of which directories lose their names, and every choice
of which names outside `staging/` lose theirs, each with the names inside `staging/`
all lost and all kept. It then lets the next writer recover the store, and finds either
the run as it was admitted, with neither output and no reference to one, or both
outputs, each read whole and checked, with the run's two references and the completed
record that lists them; and finds a run reported as completed only in the second case
(`commit::a_run_crashed_inside_its_commit_reopens_with_neither_output_nor_status_or_with_all`).
The names inside `staging/` are not each tried apart from the others. The next writer
removes whatever staging holds before it reads anything else, so which of them a crash
kept is not expected to decide what it then finds; that is an argument, and the two
extremes are what is tested. The cost is why: up to about twenty names are undecided at
one step (with a request id, up to 16 for two outputs, and 20 where the store already
held the artifact), and every choice of which of them a crash keeps is two to the power
of that many crashes there, a million at twenty; trying every name apart took ten
seconds for the steps before that one alone.
The other refuses each of those calls in turn without a crash, and finds the same two
states, the second only where the publication became visible and its last sync failed
(`commit::a_commit_that_the_store_refuses_at_any_step_publishes_no_output_and_no_status`).

**The reference to an output** that the spec asks every record for is the output's
identity with its type, in the record's `outputs`: the artifact is found from it by its
name. The store's own references are the two above. `whizbang store list` counts the
run's among what the run holds
(`runs::a_run_that_committed_an_output_is_shown_and_listed_with_it`, in the binary's
tests), and deleting the run removes them. It does not remove the
artifact's name: that is outside runs, where an imported artifact is, and another run
may have taken it as an input. So an artifact that only a deleted run committed stays
in the store, and no command removes it
(`commit::deleting_a_run_removes_its_references_and_known_leaves_the_artifacts_it_committed`;
`docs/carry-over.md`).

**A failure of the store in the commit step** is answered as ADR 0005 says of the end of
a run: with the store's failure, not with a finished run. The run stays as it was
admitted, and no output is committed; where the publication became visible and its last
sync failed, all of it is there and was not reported.

### An output the store already holds

An artifact's identity does not cover its origin (ADR 0002), and its file does. So two
runs can make one artifact with two origins: one name, two files. The store keeps the
first (ADR 0005, "The origin does not name the run or the models": which run made an
artifact is a query over records, and the stored origin is the first one's).

That is decided by the store, inside its publishing step, and not by the executor
looking first: `Publication::keep` gives a name content as an output unless the
generation it is published onto already holds the name as an output, in which case the
name keeps what it holds and the rest of the publication is published all the same. Of
two publications that each keep one name, the first to be published decides its
content, whatever each saw before
(`store::runs::a_name_that_is_kept_holds_the_first_output_it_was_given`;
`commit::an_output_the_store_already_holds_keeps_the_file_it_has`, which publishes the
other file between the moment the commit's content reached `objects/` and its publishing
step). A diagnostic reference under an artifact's name is not the artifact (ADR 0002)
and is replaced.

The run's record lists the output either way: the run produced that artifact. The file
it handed back, with its own origin, is what the run's own reference is to; it goes with
the run.

### What an action or a panic says is cut

A record is read back only when it is at most 4 MiB (ADR 0005, `RECORD_MAX_BYTES`), and
an executor that wrote a larger one would report a run whose repeat is then answered
"not a run record". So the texts an outcome takes from an action are bounded: an error's
type, its message, a panic's message and the reason a run was invalid are each kept to
their first 4,096 characters (`MESSAGE_LIMIT`), with three dots after them when there
were more. The secrets are taken out first, from the whole text, and looked for once
more after the cut. The outcome that is returned and the record hold the same cut text
(`commit::what_an_action_or_a_panic_says_is_cut_and_its_record_is_read_back`,
`commit::a_secret_across_the_place_a_message_is_cut_is_taken_out_before_the_cut`).

A record also lists every output. When the record of a run that handed back outputs
would be over the limit, which takes tens of thousands of them, the run ends as an error
of the type `output` that says how many there were and how large the record would have
been; none is committed, and the record that is written lists none
(`commit::a_run_whose_outputs_make_a_record_too_large_to_read_commits_none_of_them`).

What can still make a record too large to read back: an argument's text, which is
recorded as it was given, and the list of a run's diagnostics, when there are tens of
thousands of them. Neither is bounded yet (`docs/carry-over.md`).

### A panic in an action

A panic in `Action::run` is caught by the executor. The run ends as an error of the type
`panic`, through the same end publication as any other run: its journal and the
diagnostics it wrote before the panic are kept, its record is written, the id's ending
is made, and no output is committed. The message is `the action panicked: ` and the text
the panic carried, with the secrets taken out; a panic that carried no text says so.
`Executor::submit` returns the finished run, and the executor goes on to the next call
(`commit::a_panic_in_an_action_ends_its_run_as_an_error_with_its_record`).

What a panic carried is dropped by the executor, and a value can panic when it is
dropped. That second panic is caught as well, and the run ends as the first panic ends
it (`commit::a_panic_whose_payload_panics_when_it_is_dropped_ends_its_run_as_any_panic_does`).

What is not caught: a build whose panics abort the process, and a panic outside
`Action::run`, in the executor's own code.

**The panic hook is not the executor's.** The process's panic hook runs before the
executor catches anything, and the default hook prints the panic's text and the place in
the source it came from to stderr: the whole text, with no secret taken out and nothing
cut. None of that is written to the store, and the executor does not change the hook,
which belongs to the process. A door that runs actions must install its own hook, one
that prints nothing an action said (`docs/carry-over.md`).

### Status, waiting, and a run that was started

The code is in `crates/whizbang-core/src/contract/` (`engine.rs`, `live.rs`) and
`crates/whizbang-core/src/store/mod.rs`; the tests are in
`crates/whizbang-core/tests/it/contract/watch.rs` and `store/runs.rs`.

**Three more methods of the executor.** `start` takes a request as `request` does, and
answers once the run is admitted, durably, with `Answer::Started` and the run's id; the
action runs on a thread of its own, one for each started run. `status` takes a run's id
and says where the run is now, as a `RunState` (ADR 0007), with the progress events the
run has reported so far when it executes in this process. `wait` is `status` once
nothing in this process works on the run any more. `submit` and `request` still run the
action on the caller's thread and return when the run is over; a run they made is asked
after in the same way, by another caller.

| The run | `status` and `wait` answer |
|---|---|
| Ended, in the store | `Ended`, with the outcome, from its record; no events: they are in its journal |
| Executing in this process | `status`: `NotEnded { here: true }`, with its events so far. `wait`: waits, then answers as for a run that ended |
| Its end could not be written | `NotEnded { here: true }`, with its events; `wait` does not wait |
| Was to be started, and no thread could be made for it | `Ended`, as an error of the type `start`; the caller that started it was handed `CallError::NotStarted` |
| Admitted by a process that is gone | `NotEnded { here: false }`; `wait` does not wait: nothing will end it |
| Not in the store | The refusal `NoSuchRun`: it was never made, or it was deleted |

An executor that only reads a store says `here: false` of every run that has not ended,
as it does for a repeat. A run is asked after by any executor over the same store value:
the second caller in the first of these tests has an executor of its own
(`watch::a_second_caller_reads_the_status_and_the_progress_of_a_run_whose_action_is_busy`,
`watch::a_caller_that_starts_a_run_and_leaves_finds_it_again_by_its_id`,
`watch::waiting_for_a_run_that_no_process_will_end_returns_and_says_so`).

**One table of open runs, which is the store's.** The store already kept the runs its
writer made and has open (`Store::made_here`, ADR 0007). That table now holds, for each
run, what its maker attached to it (`Publication::attach`, `Store::attached`), and the
executor attaches the run's live state: its events, and whether anything still works on
it. There is no second table. The store's rule is the one rule: a run is in the table
from just before the rename that makes it visible, is taken out again if that rename
fails, and leaves only once the generation that closes or deletes it is visible
(`store::runs::what_is_attached_to_a_run_is_had_while_the_writer_has_the_run_open_and_no_longer`).
The publication that closes a run can ask that the run stay in the table past that,
until its closer lets it go (`Publication::keep_until_let_go`, `Store::let_go`): the
executor asks, for every run it ends, and lets the run go when it is done with it ("A
status and a wait at the end of a run", below). A deletion takes a run out whatever
was asked
(`store::runs::a_closed_run_that_was_kept_for_its_closer_stays_in_the_table_until_it_is_let_go`).

**How a status is answered: the store, the table, the store.**

1. The current generation is read. No such run: `NoSuchRun`. A run with its record:
   `Ended`.
2. Otherwise the run was admitted and had no record. The table is asked. Found: the run
   is open in this process.
3. Not found: the current generation is read again, and what it says is the answer; a
   run that still has no record is `NotEnded { here: false }`.

**Why the last look is conclusive.** A run this process made was in the table before it
was visible, so the first look could not have seen it before the table held it. It
leaves the table only after its closing or its deletion is visible. So when step 2 does
not find it, either this process never made it, and since one process writes a store
nobody is working on it; or it has left the table, and then its end or its deletion was
visible before step 2, and is in the generation step 3 reads. Step 3 cannot see a run
of this process that is still open.

Three tests stop at the moments this could go wrong. Straight after the rename that
makes an admission visible, a status says `here: true`
(`watch::a_status_asked_the_moment_a_run_becomes_visible_says_it_is_executing_here`).
Straight after the rename that makes an end visible, while the store still has the run
in its table, a status says `Ended`
(`watch::a_status_asked_the_moment_a_runs_end_becomes_visible_says_it_ended`). And a
status whose run ends between its first look and its look at the table is told `Ended`,
not that no process will end the run
(`watch::a_status_that_races_the_end_of_its_run_is_not_told_the_run_will_never_end`).

**What is held, and for how long.** A status takes the table's lock to read one entry,
and the run's own lock to copy its events or read one flag; neither is held across a
file-system call, and the store's publishing step holds neither while it writes. So a
status is answered while another run's end, or the same run's, is being published: a
test stops a publication after its pointer is replaced and before its last sync, and a
second thread reads the status of that run and of another that is executing
(`watch::a_second_caller_reads_the_status_of_a_run_while_its_end_is_being_published`).

**A status and a wait at the end of a run.** A run's end is visible in the store before
the executor is done with the run: the end publication's last sync comes after the
rename that shows it, and a started run's thread holds its place among the started runs
until that publication has returned. The two questions are answered differently there.

- `status` says what the store says: `Ended`, with the outcome, from the moment the end
  is visible.
- `wait` answers when the executor is done. Having found the end in the store, it looks
  for the run in the table; found there, it waits for the executor, and not found
  there, the executor is done. That is why the run is kept in the table past the
  visibility of its end. The executor lets go in a fixed order: the run's place, then
  the run's entry in the table, and only then are those who wait told. So whoever waited
  for a started run finds its place free, and a `start` straight after the `wait` is not
  refused for that run.

One test stops a run at its end publication's last sync, with a bound of one: a status
says `Ended`, a `start` is refused, a caller that waits is still waiting, and once the
run goes on the wait answers and the `start` after it is admitted
(`started::a_wait_for_a_started_run_answers_only_when_its_place_is_free`). The order of
letting go is held by a test of its own, which holds the lock a waiter is told under
while the executor lets go
(`contract::engine::tests::a_runs_place_is_free_before_whoever_waits_for_the_run_is_told`).
For a run that `submit` or `request` ran there is no place, and a `wait` by another
caller answers when that call has all but returned.

**A run that is deleted while someone waits.** A `wait` that began before the run was
deleted is waiting for the run's action, and answers when the action has come back and
the executor is done: with `NoSuchRun`, since the end of a deleted run is refused and
the run is not made again. A `wait` that begins after the deletion is visible answers at
once, with `NoSuchRun`. Deleting a run does not stop its action.

**A run whose end was refused stays in the table.** The store takes a run out of its
table of open runs when the generation that closes or deletes it is visible. A run
whose end publication was refused was never closed, so it stays there, with its live
state and its events, until it is deleted or the store value is dropped. That is what
lets `status` say `here: true` of it and show its events; what it costs is the memory
of those events for the life of the store value.

**How many runs are started at once.** Each started run has a thread, and a thread is
not free: a caller that started runs without end would take the process, and with it
the machine, down. So `start` is bounded.

- *The bound.* An executor starts a run only while fewer than its bound of started runs
  are live: 64 unless it was built otherwise (`Engine::with_started_bound`,
  `STARTED_RUNS_DEFAULT`). A bound is at least 1 and at most 256
  (`STARTED_RUNS_CEILING`); any other is refused when it is set (`BoundRefused`), so no
  one executor can be told to start thousands.
- *Where it is counted.* In the store value, beside its table of open runs
  (`Store::start_place`): the count is the store writer's, not an executor's. Two
  executors over one store value count the same runs, each against its own bound.
- *What that leaves.* The bound is of one store value. A process that opens several
  stores to write has a count for each, so it can hold as many started runs as it has
  stores, times the bound: with the ceiling, 256 for each. Nothing bounds how many
  stores a process opens. The binary opens one.
- *When a place is taken, and what a refusal leaves.* Before anything else: before the
  request's id is looked up, before the call is prepared, before admission. A request
  over the bound is refused with `Refusal::TooManyStarted`, which carries the bound and
  nothing the caller wrote, on the caller's thread and at once. No run was made,
  nothing was reserved, nothing was written to the store, and the request's id is not
  taken. One case is not refused: a request whose id was admitted before starts
  nothing, and is answered with the run it made, as always.
- *When a place is given back.* When the executor is done with the run, by a guard that
  is dropped however that comes about: the action came back, the action panicked, the
  end publication was refused, the admission failed after the place was taken, the
  thread could not be made. It is given back before whoever waits for the run is told
  that it is over, and a `wait` does not answer before that ("A status and a wait at the
  end of a run", above), so a caller that waited for a run can start the next.
- *What it does not cover.* `submit` and `request` run the action on the caller's own
  thread, take no place and are never refused for this: how many of those run at once
  is the caller's doing. A call from inside an action has no rule yet; nested calls get
  their own in the next part.

The tests hold two or three runs at barriers, never more
(`started::a_start_over_the_bound_is_refused_and_writes_nothing_and_a_run_that_ends_makes_room`,
`started::two_executors_over_one_store_count_the_same_started_runs`,
`started::a_bound_of_none_or_over_the_ceiling_is_refused_when_it_is_set`,
`started::a_started_run_whose_action_panics_gives_its_place_back`,
`started::a_started_run_whose_end_the_store_refuses_gives_its_place_back`,
`started::a_request_that_is_not_admitted_after_its_place_was_taken_gives_the_place_back`).

**What keeps a test from making threads without end.** The bound is the product's. The
tests of this repo are held three ways, because a probe written for the review of this
part parked threads until the system refused one, and the machine went down.

- *Before a thread is made.* A test makes a thread through `whizbang_testkit::threads`
  (`spawn`, `scope`), which counts the live threads it made in the process and panics,
  in the caller, at the 65th: nothing is asked of the system. `clippy.toml` refuses
  `std::thread::spawn`, `std::thread::scope` and a builder's `spawn` everywhere in the
  workspace, tests included, and is allowed in two places: that helper, and the
  executor's own spawner (`spawn_thread`), which the bound above holds. `just check`
  lints every target, so a test that makes a thread another way fails it
  (`scripts/tests/test-thread-lint.sh`, on a fixture). This is the stop that works
  against a loop: a loop of raw spawns makes tens of thousands of threads a second.
- *From outside, about three times a second.* Every recipe and script that runs tests runs them
  through `scripts/thread-guard.sh`, which counts the threads of every process under the
  command and kills the whole tree when one process holds more than 300, or when it can
  no longer count. It catches a process that grows slowly, within about a third of a
  second (0.2 seconds between two counts, and the count's own time). It cannot stop a
  tight loop in time, and it does not see a process whose parent has ended. A guard
  under a guard that is at least as strict leaves the watching to the one above; the
  variable that tells it so is believed only when the limit it states is not above the
  guard's own and it names a process the guard descends from that is running the same
  file.
- *By the system, on Linux.* The guard lowers the number of processes and threads the
  command's user may hold to what the user held and 4,096 more, and lowers nothing when
  it cannot count what the user holds. macOS has no limit of that kind that counts
  threads.
- *A group of lints is allowed nowhere.* An `allow` of `warnings`, `clippy::all` or
  `clippy::style` would silence the thread lint without naming it, so the lint test
  refuses one anywhere in the crates, on one line or several, and refuses the same in a
  manifest's lints, a second `clippy.toml`, and a cargo config that caps or allows lints.
- *Some of the tests.* `just test-only` runs without a prompt, so it takes a closed set of
  arguments (`scripts/test-only.sh`) and refuses any other before anything runs: nextest's
  own arguments can name a program to run.

A thread made by the product's own spawner in a test is a started run, and is held by
the bound, which the tests set to 1 or 2. A thread made by something a test depends on,
outside the workspace, is seen by the guard alone.

**A thread that cannot be made.** The thread is made with a name (`run <id>`) through
`std::thread::Builder`, which hands back the system's refusal where `std::thread::spawn`
panics. By then the run is admitted, durably. So the executor ends it there, on the
caller's thread, through the publication that ends any run: as an error of the type
`start` whose message says that the run was admitted and could not be started. The
caller is handed `CallError::NotStarted`, with the run and the system's sentence: not a
refusal, since it is no judgement of the call. Nothing waits for that run: a `wait`
answers `Ended`, and a repeat of the request is answered with the error. If that end
cannot be written either, the run stays admitted with nothing working on it, and a
`wait` says so and does not block. A test has the executor make its threads through a
function that fails; nothing is exhausted to show it
(`started::a_run_whose_thread_cannot_be_made_is_ended_as_an_error_and_nobody_waits_for_it`).

**Progress.** An action's progress events are kept in memory, and are read through
`status` while the run executes here. Each message is kept as the other texts of a run
are ("What an action or a panic says is cut"): the secrets taken out, then its first
4,096 characters, with three dots when there were more
(`started::a_progress_message_is_cut_as_every_text_a_run_keeps_is`). How many events a
run reports is not bounded. They are not published while the run executes: the journal
is durable with the run's end, as before, and a process that dies leaves none of it.
Both stay on `docs/carry-over.md`. The events of a run whose end the store refused stay
readable in the process that ran it. A message is shown as the action gave it apart
from that: a control character in it is passed through, and a door that prints events
escapes them (`docs/carry-over.md`).

**What is not built.** Nothing cancels a run, in its action or in its admission: that is
the next pull request, with the rule this record owes for a cancellation and the commit
step. A run whose admission became visible and was not confirmed has nothing behind it
and is still waited for: a `wait` for it does not return until the next pull request
marks it. A process that ends while a started run executes leaves the run admitted.
Nothing holds the generation a run reads from.

## Consequences

- An action returns artifacts. `Produced` is no longer made only as nothing, and is no
  longer `Eq`: an artifact holds numbers that are not.
- A completed run's outputs are in the store when the run is reported, and can be the
  inputs of the next call (`commit::an_output_one_run_committed_is_an_input_of_the_next`).
- The end publication of a completed run stages every output's file. An output is a
  whole canonical file held in memory from the moment the action hands it back to the
  end of the publication.
- The store has a third way to give a name outside runs content, `Publication::keep`,
  beside `put`, which replaces, and `create`, which refuses the publication.
- `whizbang store import` still looks at a snapshot and then puts (ADR 0002). It is the
  only writer of its store while it runs.
- A run whose action panicked has a record, and a repeat of its request is answered with
  the error. `RunState::NotEnded { here: true }` no longer covers a panic (ADR 0007);
  it still covers a run whose end could not be written.
- A run holds names that are committed outputs, where every name of a run was a
  diagnostic reference before. `runs show --diagnostic` refuses one, as it refused any
  output.
- Deleting a run leaves the artifacts it committed, under their names outside runs.
- Reversing this costs every action written against `Produced`, and the guarantee every
  caller reads a completed outcome by.

- `Executor` has three more methods, and `Answer` a third case, `Started`. `Engine` is
  cloned cheaply: a started run's thread holds one, with the store.
- `Refusal` has `NoSuchRun` and `TooManyStarted`; `CallError` has `NotStarted`.
- A `wait` for a run this process ended answers a little later than a `status` says
  `Ended`: when the executor is done with the run, not when its end is visible.
- A test makes its threads through the test kit, and `just check` refuses one that does
  not.
- A caller that starts runs handles `TooManyStarted`: it waits for one of its runs and
  asks again, under the same request id.
- A progress message over 4,096 characters is cut where it was kept whole.
- A run's events are kept after its end was published or refused, until the last holder
  of its live state lets go.

What this record does not decide yet: cancellation and its rule at the commit step;
cancel while a run executes; nested calls, allowances, and when a parent is terminal. They are the later parts of the same task, and amend this record.

## Alternatives

- **Let an action write its outputs to the store as it makes them, and mark the run
  completed at the end.** Turned down: a crash between two of those writes leaves an
  output of a run that never completed, which the spec forbids, and something would have
  to find and remove it.
- **Stage outputs in a publication the action holds.** Turned down: the action would
  hold the store's one way in, and a publication that an action abandoned or leaked
  would keep content from a prune for as long as the process lived.
- **The artifact's name alone, and no reference of the run's own.** Turned down: the
  store would then count a run that committed outputs as holding none, the file a run
  made with its own origin would be named by nothing when the store already held the
  artifact, and nothing of a run's outputs would go when the run is deleted.
- **The run's own reference alone.** Turned down: an artifact is taken as an input by
  its identity (ADR 0004), through its name outside runs. What `store delete` should do
  with that name is on `docs/carry-over.md`.
- **Refuse a commit whose artifact the store already holds with another origin.** Turned
  down: the run did its work and produced that artifact; which origin is stored is not
  its failure.
- **Let the executor look for the artifact first and skip the write.** Turned down: that
  is an answer from a snapshot, and another publication can come between the look and
  the publishing step.
- **Let a panic unwind,** as before. Turned down: the run then has no record, its
  diagnostics are lost, and its request is answered for good as a run that has not
  ended.
