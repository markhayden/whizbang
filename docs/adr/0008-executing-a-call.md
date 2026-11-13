# 0008. Executing a call: one commit step for outputs and the completed status

Status: accepted
Date: 2026-11-12
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
what a panic does. The rule for a cancellation and the commit step, status and cancel
while a run executes, nested calls with their allowances, and what makes a parent
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
characters. Nothing of what was handed back is committed, the artifacts that were right
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
extremes are what is tested. The cost is why: eleven names are undecided at one step,
which is two thousand crashes there, and trying every name apart took ten seconds for
the steps before that one alone.
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

### A panic in an action

A panic in `Action::run` is caught by the executor. The run ends as an error of the type
`panic`, through the same end publication as any other run: its journal and the
diagnostics it wrote before the panic are kept, its record is written, the id's ending
is made, and no output is committed. The message is `the action panicked: ` and the text
the panic carried, with the secrets taken out; a panic that carried no text says so.
`Executor::submit` returns the finished run, and the executor goes on to the next call
(`commit::a_panic_in_an_action_ends_its_run_as_an_error_with_its_record`).

What is not caught: a build whose panics abort the process, and a panic outside
`Action::run`, in the executor's own code. The process's panic hook still runs and
prints what it prints, the place of the panic among it; none of that is written to the
store.

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

What this record does not decide yet: cancellation and its rule at the commit step;
status, progress and cancel while a run executes; nested calls, allowances, and when a
parent is terminal. They are the later parts of the same task, and amend this record.

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
