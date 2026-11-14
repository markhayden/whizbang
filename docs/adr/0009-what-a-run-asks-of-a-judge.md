# 0009. What a run asks of its capabilities, and a judge held where its calls are dispatched

Status: accepted
Date: 2026-11-13
Amended: 2026-11-13, after the review of its first pull request, before it merged.
Added: "A provider's words are redacted whole"; the bound on a model's names and the
sum that keeps a record under its limit; what the executor does with a record over it;
the bytes of exchanges a run stores, and a turn that is not written; that what a run
froze binds every run under it, and binds a preparation. Corrected: the bound on what a
run stores, which left out the answer; what a stored digest gives away; that a redacted
exchange is not promised to be JSON; `RefusalKind` has 27 kinds.
Amended: 2026-11-13, at the exit check of checkpoint 1A (task A12). Added, under
"Evidence": evidence is kept exactly or not at all, and how much evidence one run can
hold. Changed: what remains of a record over the limit, which is nothing (ADR 0008,
"What a call and a run may hold"); a box whose entries are not all numbers makes no
call; and the sentence of the plan on the tool list, which is corrected there.
Spec: section 3, "The execution contract", "What a run publishes", "What a judge model may
call" and "What every run records"; section 4, "Backends"

## Context

`SPEC.md` section 3 says a run's record holds, for every capability request, the backend,
the model and its version, the device and whether a cache answered; the duration of each
step, tool call and capability request; and for a judge run every turn, what was sent and
received, kept whole, with token usage, excluded turns and the verdict. It says each run
freezes an explicit list of evidence tools the model may call and the bindings it may
read, enforced where calls are dispatched and not in the prompt; that diagnostic evidence
is always kept and can never be used as an output; and that secrets are redacted before
anything is written.

What there was to build it on, and what was left open, each on `docs/carry-over.md` for
task A11 of checkpoint 1A:

- The executor handed every run a handle on every backend it was given, whatever the
  run's call resolved (ADR 0006, ADR 0008). Requests were counted and recorded nowhere.
- The record's `models`, `time.durations` and `judge` lists could hold nothing (ADR 0005).
- An opaque part of a judge's conversation is kept whole in memory and must be sent back
  as it came (ADR 0006), and everything a run writes passes through redaction (ADR 0005),
  which would change a part that held a secret.
- The judge's types are made through checked constructors and are not deserialised (ADR
  0006), and the scripted run of task A11 reads its judge from data.
- An output port returns one of six artifact types (ADR 0002, ADR 0003). A verdict is
  none of them.
- A call from inside an action is a run of its own, and its outputs are committed by that
  run (ADR 0008). The plan for checkpoint 1A says of the scripted run: "Evidence a tool
  returns is kept as a diagnostic and can be read back by the same run. It cannot become
  an output of the action or be read by another run."

The code is in `crates/whizbang-core/src/capability/` (`mod.rs`, `judge.rs`,
`written.rs`), `crates/whizbang-core/src/contract/` (`context.rs`, `engine.rs`,
`journal.rs`, `nested.rs`, `record.rs`, `registry.rs`) and `crates/whizbang-squint/src/`
(`check.rs`, `view.rs`). The tests that hold it to this record are in
`crates/whizbang-core/tests/it/contract/asked.rs` and `scope.rs`,
`crates/whizbang-core/tests/it/capability/written.rs` and
`crates/whizbang-squint/tests/it/check.rs`.

## Decision

### An action is handed the backend its call resolved

An executor holds every backend of its build: `Capabilities` holds, for each capability,
the backends in the order they were given (`with_judge` adds one, where it replaced). When
a run starts, the executor hands its context, for each capability, the first backend it
holds that declares the model the run's prepared call resolved for that capability (ADR
0004, `Need::model`), compared by backend, model and version; and none for a capability
the call does not need. So a call that names the second of two judges is answered by the
second, and an action that declares no need is handed no handle
(`asked::an_action_is_handed_the_backend_its_call_resolved_and_no_other`). Prepare still
sends nothing: the test that said so is now of an executor that holds the fakes
(`prepare::prepare_sends_no_capability_request`).

What is not held, and is known:

- The roster prepare asks and the backends an executor holds are given apart, and nothing
  holds one to the other. A call that resolved a model no held backend declares is
  admitted, and its action finds no handle
  (`asked::known_a_call_that_resolved_a_model_the_executor_does_not_hold_is_handed_no_handle`).
  The binary's executor holds none. Both are the config's, at checkpoint 1B.
- A call with two needs of one capability is handed the backend of the first.
- Two backends that declare one model are one backend to this: the first is handed over.

### Every request is in the run's record

The handles a context makes share one tally (ADR 0006, "Accounting for requests"). It now
keeps each request it counts: the capability, the model the backend declares, when the
request was sent and when it was over, by the executor's clock, and how it ended:
`answered`, the kind of its fault, or `unwound` for a backend whose call panicked. A
request is counted after every check that sends nothing, as before, so a request the
handle refused is not one. An answer the handle refuses as malformed is a request that
ended `malformed`.

**A run sends at most 1,024 requests** (`REQUESTS_LIMIT`). One more is not sent: the
fault is `too_large`. So the lists below are whole and bounded
(`asked::a_run_sends_only_so_many_requests_and_its_record_lists_each_and_is_read_back`,
which lowers the bound through the executor's seam for tests, and
`asked::the_bound_on_a_runs_requests_is_the_one_its_record_and_the_schema_hold`).

In `run/2`, which these fills share until the tag (ADR 0005):

- **`models`** holds one element for each request, in the order they were sent:
  `capability`, `backend`, `model`, `version`, `device`, `cached` and `ended`. `device`
  and `cached` are always null: no response says which device answered or whether a cache
  did (ADR 0006), so neither is known, and null says so. They are there so that the first
  backend that reports them fills a field that exists (`docs/carry-over.md`).
- **`time.durations`** holds one element for each request, in order, and then one for
  each call of another action whose run ended, in the order of `steps`: `of` (`request`
  or `call`), `index` (its place in `models` or in `steps`), `start_ms` and `ms`. A call
  whose run was abandoned has none.

**A model's names are bounded where they are declared.** The backend, the model and
the version of a declaration are each 1 to 128 printable ASCII characters with no
space, no `"` and no `\` (`MODEL_NAME_LIMIT`; `Declaration::new` refuses any other, and
does not show it): each is written once for every request. In the record each has the
secrets taken out while it is whole and is then cut to 128 characters, so a name that
redaction lengthened is still one the reader takes
(`asked::the_names_of_a_backend_a_model_and_a_version_are_bounded_where_they_are_declared`,
`asked::a_secret_and_a_long_text_in_a_models_names_are_redacted_and_then_cut_in_the_record`).

**The sum.** One request adds at most 1,211 bytes to a record: its model with three
names of 128 bytes, its duration, its turn with an id of 128 bytes and four counts of
twenty digits, and its exchange among the diagnostics, with times of twenty digits and
indexes of four. A test measures 1,157 with the names, the id and the counts at their
longest and the times as a test's clock gives them, and holds it under 1,211. So 1,024
requests are 1,240,064 bytes. The other lists the executor fills are bounded too: 256
steps of at most 256 bytes, 65,536; 256 refusals of 512 characters, at six bytes for a
character that JSON escapes, 819,200; and an error's type, its message and a verdict's
reason, each 4,099 characters at six bytes, 73,782. Together 2,198,582 bytes. A count
made apart from this one, with every field at the schema's own maximum, comes to
2,364,470. Either leaves 1.8 megabytes of the 4 MiB for what the executor does not bound
(`asked::a_record_of_as_many_requests_as_a_run_sends_is_under_the_limit_by_arithmetic`).

**A record over the limit is not published as it is.** What the executor does not
bound can still take a record over: an argument's text, and the number of diagnostics
an action wrote. When the record would be over `RECORD_MAX_BYTES`, the run ends as an
error, of the type `output` when it handed back artifacts (ADR 0008) and `record` when
it did not; its record leaves out `models`, `time.durations` and `judge`, and the
message says how many requests there were. The exchanges are kept, and listed
(`asked::a_record_that_would_be_over_the_limit_is_not_published_as_it_is`). Nothing
remains of the case in which the record was still over the limit without them: an
argument's text, a call's inputs and a run's diagnostics are bounded where they come
in, and with them the sum above is under the limit (ADR 0008, "What a call and a run
may hold"). That test now lowers the limit through a seam for tests to reach this
answer at all.

### A provider's words are redacted whole

Three texts of a provider are cleaned and cut when a backend makes them (ADR 0006): a
fault's sentence at 240 characters, another stop reason's word at 64, and the provider's
id at 128. Redaction finds a secret by its whole value. So a secret that a cut went
through, or that cleaning changed a character of, was no longer found by the time
anything was written, and part of it was stored.

A fault and a turn therefore keep the words they were made of, as they were given,
until a handle has looked. A handle that an executor's context made knows its run's
secrets, and makes each again before it hands it on or keeps it: the secrets are taken
out of the whole words, and only then are they cleaned and cut. When what was shaped
still holds something redaction finds, as cleaning can make it, none of the words is
kept: the sentence is `the reason is not kept`, the word and the id `not kept`. Words
over a mebibyte are not kept to be searched, and are shown the same way. This is done
to every fault of every capability, the handle's own included, and to every turn
(`asked::a_secret_that_a_cut_of_a_providers_words_goes_through_is_stored_nowhere_and_told_to_no_action`,
`asked::a_fault_of_any_capability_is_told_to_an_action_with_the_secrets_taken_out_whole`,
`asked::words_that_cleaning_turns_into_a_secret_or_that_are_too_long_to_search_are_not_kept`).

- **The action is handed the redacted words too.** It could copy a fault's sentence
  into its own failure, and a cut fragment of a secret would then be in the record.
  A turn's parts are not cut, and are handed to the action as they came: what is
  stored of them is redacted where it is stored.
- **The marker is cleaned with the rest:** where a secret stood, a fault's sentence
  and a provider's id say `?redacted?`.
- **A handle made by other code** knows no secrets and changes nothing.
- **Cut after the last redaction.** Wherever a bounded text is redacted when a record
  is made, it is cut afterwards: a model's names and the provider's id at 128
  characters. A text an outcome keeps (ADR 0008) is redacted, cut at 4,096 characters
  and marked with three dots; when the dots complete a secret they are left off, so
  what is kept holds none and is never over 4,099 characters
  (`asked::an_id_that_redaction_lengthens_is_cut_again_and_its_record_is_read_back`,
  `asked::a_text_whose_cut_and_three_dots_complete_a_secret_is_kept_without_the_dots`).

What was looked at for the same order, and how each stands: a tool call's id and name
and its arguments are never cut, and are redacted as bytes in the exchange; a refusal's
sentence is redacted and then cut, and a call any of whose own texts holds a secret is
recorded with fixed words (ADR 0008); a name given to `Context::offered` goes the same
way; a diagnostic's name that holds a secret is refused; a verdict's reason and an
error's message go through the rule above. The errors of the reader of scripts and
exchanges quote what was read and are not redacted: nothing stores them, and a reader
has no secrets to look for.

### What is kept of an exchange with a judge

A handle that a context made keeps every exchange with a judge itself, when the request
is over. An action does nothing for it and cannot leave it out.

**The whole exchange is a diagnostic of the run**, named `exchange-` and the request's
number among the run's requests, from 1, in four digits. It is one JSON document,
`judge-exchange/1`: the system prompt, every message that was sent, the tools, the output
limit, and the turn or the fault that came back. The executor writes these names; an
action that writes a diagnostic whose name starts with `exchange-` is refused
(`asked::an_action_cannot_write_a_diagnostic_under_an_exchanges_name`). It is written
through the run's diagnostics, so its bytes pass through redaction like any other
(`asked::a_secret_in_what_a_judge_was_sent_or_answered_is_in_nothing_the_run_stores`).

What "whole" does not cover:

- **Pixels.** An image is written as its size and its canonical pixel digest. The images
  of a run are its inputs, which the store holds, and what its tools returned.
- **An opaque part's body** (below).
- **A secret:** where one stood, the stored text says `[redacted]`. Redaction is of
  the document's bytes (ADR 0005), so a stored exchange is not promised to be valid
  JSON or to read back: a secret that begins or ends inside a JSON escape can break
  the string it stood in (ADR 0005, "Redaction").
- **A turn the run has no room for, or would not take** (below).

**How much a run stores.** A request may hold 64 MiB of text and the turn that answers
it 64 MiB more, and writing either as JSON can multiply it: an exchange can be several
hundred mebibytes as written. Two bounds hold what is stored, as it is stored, with the
secrets taken out:

- one exchange is at most 160 MiB (`EXCHANGE_MAX_BYTES`), which is what its reader
  reads: none is written that its own reader would refuse;
- a run stores at most 256 MiB of exchanges in all (`EXCHANGES_MAX_BYTES`).

What was sent is written before it is sent. When that, with room for an answer that is
not written, would pass either bound, the request is not sent: the fault is
`too_large`, and the request is in the record as one that ended so, with a turn that
names no exchange. Such a request was counted before it was refused: it is one of the
1,024 a run may send, and is listed in `models`, though no backend was asked.

Room is taken before anything is stored, in one step with the look at what is left:
a request takes room for what it sends before it sends, takes room for the whole
exchange when its answer has come, and gives back what it did not use. So requests
that are open at once cannot together store more than the run may
(`asked::two_requests_at_once_cannot_both_take_the_last_of_what_a_run_may_store`). When the answer comes and the whole would pass a bound, the turn is
handed to the action all the same, and the exchange is stored with the turn not
written (`turn_withheld`): how many parts it had, how many bytes of text, the SHA-256
digest of that text, and why it stopped. When even that does not fit, as when redaction
lengthened what was sent, nothing is stored and the record's turn names no exchange
(`asked::a_request_whose_exchange_the_run_has_no_room_for_is_not_sent_and_the_record_says_so`,
`asked::an_answer_the_run_has_no_room_for_is_kept_as_its_sizes_and_the_action_still_has_it`).

**A turn the action will not take.** `JudgeHandle::turns_within(bytes)` narrows what a
handle takes: a turn with more text is the fault `too_large`, the request is recorded
as ended so, and the exchange is stored with the turn not written. A loop sets it to
what it will read, so that a turn it would refuse is not kept either
(`asked::a_turn_over_the_limit_its_handle_was_given_is_a_fault_and_is_stored_as_its_sizes`).

So the most a run stores of its exchanges is 256 MiB. All of it is held in memory until
the run ends, with, while a request is open, what was sent written once more.

**The record's `judge` group is a bounded summary.** It is a list, as `steps` is, each
entry under the word that says what it is:

- `turn`: one for each request to a judge, in order. `request` is its place in `models`;
  `exchange` is the name of the diagnostic that holds the whole of it, or null when it
  could not be kept; `max_output_tokens`; `messages`, how many were sent; `stop`, why the
  turn ended as a word (`end_turn`, `tool_use`, `max_tokens`, `refusal`, `other`), or null
  when no turn came back, in which case the fault is in `models`; `tool_calls`, how many
  the turn made; `usage`, the four token counts, or null; `provider_id`; and `excluded`.
  A request whose backend's call panicked kept nothing: its turn is there with no
  exchange, no stop and zeroes, and `models` says `unwound`
  (`asked::a_request_whose_backend_panicked_is_in_a_record_that_is_read_back`).
- `verdict`: at most one, the last entry (below).

A turn's entry is a few hundred bytes whatever the conversation held, and there are at
most 1,024, so a long conversation cannot make a record that is refused on reading
(`RECORD_MAX_BYTES`, ADR 0005). The tool calls themselves are in the exchange, and what
became of each is in `steps` and `outcome.refusals`.

**An excluded turn.** Whether a turn enters the conversation that goes on is the
harness's decision, and the handle cannot know it. The loop says so:
`JudgeHandle::exclude_last` marks the answer to the last request the run sent a judge, with
the reason, in the baseline format's words (`docs/baseline/transcript.md`): `truncated`,
`refusal`, `no_call`. The turn is whole in its exchange either way. It is for a loop that
sends one request at a time; two loops in one run would mark each other's. It marks
the entry of the last request whatever that came to: one that ended in a fault is
marked too, though no turn came back to leave out.

The schema and the types refuse the same records, with what the schema cannot say (that
there is one duration for each request, in order; that a turn is of a request to a judge;
that an exchange is a diagnostic the run holds; that a verdict is last, single, and of a
completed run) held by the types alone
(`asked::what_is_no_model_no_duration_and_no_judge_entry_is_refused_by_the_types_and_by_the_schema`,
`asked::what_the_schema_cannot_say_of_the_requests_and_the_judge_the_types_still_refuse`).
The test kit's validator gained `oneOf` for the two shapes of an entry.

### An opaque part's body is never stored

It must be sent back byte for byte (ADR 0006), and nothing unredacted may be stored (ADR
0005). Redacting the body and storing the result would store something that is neither
the part nor safe to send. So the body is not written at all: what a run stores of an
opaque part is its adapter, the SHA-256 digest of its body and its length in bytes
(`opaque_withheld`). In memory the part stays whole, and the judge is sent it back as it
came (`asked::an_opaque_parts_body_is_sent_back_whole_and_is_never_stored`).

**What the digest gives away.** It is a plain SHA-256 of the body, with no key. A body
may hold a secret, and whoever can read the store and guess the whole body can confirm
the guess by the digest. A body is long and a provider's own, so a guess of all of it
is not expected; the digest of a turn that was not written is of the same kind. A keyed
digest is not used because nothing would hold the key that the store's reader does not.

What follows: a stored exchange that holds an opaque part cannot be read back into a
conversation and sent again, and reading says so (`Unread::Withheld`). A run that is
interrupted cannot be resumed from the store with a provider that needs such parts back.
Nothing resumes a run today.

### A turn and an exchange as data

`whizbang_core::capability::written` writes and reads two documents. The judge's types
gain no `Deserialize`: what is read is a private mirror of the document, and every value
is then made by the constructor that checks it (`Turn::new`, `Message::new`,
`JudgeRequest::new`, `ToolCall::new`, `Opaque::new`, `Fault::new`). So data that breaks a
bound is refused when it is read, with the type's own refusal
(`written::data_that_breaks_a_bound_of_a_turn_is_refused_when_it_is_read`).

- **`judge-script/1`:** the answers a scripted judge plays, in order, each a turn or a
  fault. `FakeJudge::from_script` reads one. At most 16 MiB and 1,024 entries.
- **`judge-exchange/1`:** one request and its answer, as above. At most 160 MiB is read.

Reading is strict: the bytes are counted before they are parsed; the kind and version are
looked at first, by a pass that keeps nothing else of the document and keeps the kind
only when it is a text of at most 64 bytes; a field that is not known, a field left out (one that may be null may
not be absent) and anything after the document are refused. A document is not held to one
way of being written, as a record is: a script is written by hand.

The writer chooses how an opaque part is written (`Keep`): whole, for a script, which
must hand the part to a loop as a provider would; or as a digest, for what is stored. An
exchange written whole is read back equal to what was written, an opaque part, two calls
in one turn and a cut-off turn included
(`written::an_exchange_is_written_and_read_back_as_the_request_and_the_turn_it_was`).

This is not the baseline's transcript format (`docs/baseline/transcript.md`), which
describes a whole decision for comparison. Milestone 2 converts between them.

### A verdict is not an artifact

None of the six artifact types is a verdict, and a seventh type would give a verdict an
identity, a canonical file and a name under which another call takes it as an input, none
of which anything needs. A verdict is what a run concluded. So it is in the run's record,
where the spec's "Judge runs" group already puts it:

- An action hands one back beside its artifacts: `Produced::with_verdict(Verdict)`, a
  decision (`pass`, `fail`, `unsure`) and a reason.
- The executor writes it as the last entry of the record's `judge` group, in the
  publication that ends the run, when the run completed, and for no other outcome. So the
  verdict and the completed status are durable together, and a run that failed or was
  cancelled has none, whatever its action handed back
  (`asked::a_verdict_is_written_with_a_run_that_completed_and_with_no_other`). The reason
  has the secrets taken out and is then cut as an error's message is (ADR 0008).
- A caller reads it from the record (`RunRecord::verdict`). `Outcome::Completed` does not
  carry it.

What is not decided, and is milestone 2's with squint's checklist and submission format:
the shape of a real verdict (dispositions, findings, locations); whether an outcome
carries it; and a descriptor that says which actions answer with one. Until then any
action may hand one back, and nothing but `check` does (`docs/carry-over.md`).

### A run freezes what its calls may be

`Context::freeze_calls(names)` says which actions the run may call. From then on the
executor in the run's context, where every call is dispatched, refuses:

- a call of any other action, registered or not: `Refusal::NotOffered`. The two are not
  told apart, so a refusal does not say what the build holds;
- a call that gives as an input an artifact that is neither an input of the run's own
  call nor an output that a run it called committed: `Refusal::NotReadable`. Whether the
  store holds such an artifact is not looked at, so an identity that was guessed and one
  that names another run's artifact are refused alike.

Both are refusals of a nested call like any other: the call is refused before it is
prepared or admitted, so no run is made, and each is in the record of the run that made
the call (`outcome.refusals`, ADR 0008), with kinds `not_offered` and `not_readable`.

**A name can be held to the list without a call.** `Context::offered(name)` answers as
the executor would answer a call of that action: nothing when the run froze nothing or
the name is one it froze, and otherwise `Refusal::NotOffered`, which it records in the
run's record in the same list, in order, with the same sentence. A loop that dispatches
what a model names asks it first and makes no call of a refused name
(`scope::a_name_is_held_to_what_a_run_froze_without_a_call_and_the_refusal_is_in_its_record`).

**By construction.** What a run may read is not a list an action keeps and compares
against. The executor builds it from two things only it holds: the inputs of the run's
prepared call, fixed at admission, and the outputs in the outcomes of the runs this run
called, added when each child's end is published. An action has no way to add to either.
**It is said once:** a second `freeze_calls` changes nothing and says so
(`scope::a_run_that_froze_its_calls_calls_no_other_action_and_each_refusal_is_in_its_record`,
`scope::a_run_that_froze_its_calls_gives_a_call_only_what_it_was_given_or_its_own_calls_committed`).

**A preparation is held to it as well.** `prepare` through the run's executor is
refused as the call would be, with the same refusal and the same sentence, and
otherwise is the executor's own: so it says nothing of whether an action is one of the
build, or whether the store holds an artifact, that the call would not. It is no call,
and is not recorded. And a call to start a run is held to the list before it takes a
place among the started runs, so a full count does not answer in the list's place
(`scope::a_preparation_from_inside_a_run_that_froze_its_calls_says_no_more_than_a_call_would`,
`scope::a_call_to_start_what_a_run_did_not_freeze_is_told_so_though_no_place_is_free`).

**What a run froze binds every run under it.** A run that a frozen run called, and
every run under that one, calls only actions that are on the list of each run above it
that froze one, and its own when it froze one; and from the first frozen run down,
each gives a call only what it was given itself or what its own calls committed. So a
listed action cannot be used to call, or to read, what the run that listed it could
not. A list of its own narrows what a run may call and widens nothing. What this
costs: an action that is put on a list must have every action it calls on that list
too (`scope::what_a_run_froze_binds_every_run_under_it`).

A run that freezes nothing, under no run that did, calls as before. Only the outputs of
a run's own children are readable, not those of runs further down. Freezing binds the calls an action makes
through the executor; what the action itself reads through `Context::store` is not held
to it: the action is this build's code, and the judge is not.

### `check`, and what a judge may do

`whizbang-squint` gains two actions.

**`view`** is the function of task A10 behind a descriptor: an image at `image`, an
optional second at `also`, the box as `left`, `top`, `right` and `bottom`, and the crops
at `crops`. A request the function refuses as the caller's mistake ends the run as
invalid, with the function's sentence. A box that reaches past the image is refused, as
squint refused it. The binding of a crop onto its source is not returned: a port returns
artifacts (`docs/carry-over.md`).

**`check`** is the smallest judge loop that proves the contract. It takes one to eight
images, the tools by name (`tools`, by default `view`), an optional choice of judge, and
an output limit. It needs a judge with `image` and `tool`. It returns no artifact, and
concludes with a verdict. Squint's rules, checklist and submission format are not in it:
the judge ends its turn with `pass`, `fail` or `unsure` on the first line of its text.

- **Frozen when the run starts.** The names the call gave are held against the tools on
  offer (`Toolbox`) once, and handed to `freeze_calls`. Nothing reads the toolbox again.
  The names are an argument, so they were fixed at admission and are in the record; a
  tool withdrawn after that does not leave a run that has started, and the next run sees
  the change
  (`check::changing_the_tool_list_after_admission_has_no_effect_on_the_run`). Between a
  run's admission and its action's first step, a tool that is withdrawn makes the run
  invalid: it is never run with a tool that was not on offer when it started.
- **A name that is not on the list is refused twice over.** First by the loop, before
  any call is made of it: it asks `Context::offered`, which holds the name to what the
  run froze and records the refusal, and nothing is prepared, admitted or run. So a
  model that names an action of the build that runs with nothing in its call gets no
  call made of it
  (`check::an_action_that_runs_with_no_arguments_and_is_not_on_the_list_is_refused_before_any_call`).
  Then by the executor, which stands behind the loop: were the loop to hand such a
  call over, the executor refuses it before it is admitted, as it refuses any call the
  run did not freeze. A call of a listed tool is made from the judge's arguments. An
  image is named by its identity, as the judge was told it; the executor decides
  whether the run may read it. Arguments that are not the tool's make no call: a key
  the tool does not have, and a `bbox` that is not four entries, each a number (an
  entry that is none is not skipped, or five entries of which four are numbers would
  be a box). A box that reaches past the image is a call, which the executor refuses
  when it prepares it, with the refusal in the record
  (`check::arguments_that_are_not_a_views_make_no_call_and_a_box_past_the_image_is_refused`).
- **A cut-off turn** is left out of the conversation, whatever it holds, marked
  `truncated`, and asked for once more with four times the output limit
  (`RETRY_FACTOR`). A second one fails the run. No call inside one is dispatched.
- **Bounds.** At most 16 requests to the judge (`TURNS_LIMIT`), and at most 32 tool calls
  dispatched (`CALLS_LIMIT`, far under the executor's 256): a call past that is answered
  with an error and makes no run. A turn of more than 256 KiB of text is not taken
  (`TURN_TEXT_LIMIT`, given to the handle with `turns_within`): it is a fault, the run
  fails, and what is stored of the turn is its sizes.
- **A judge is not trusted.** A tool that is not on the list, arguments that are not the
  tool's, an identity that was guessed or is another run's, an id an earlier call had, a
  turn that ends without a verdict or with calls unanswered, a stop reason the loop does
  not know, a refusal, and a judge that never concludes: each is an error result to the
  judge or a typed failure of the run, and none is a panic
  (`check::what_a_judge_that_is_not_trusted_answers_is_a_typed_outcome_and_never_a_panic`,
  `check::a_judge_that_never_concludes_is_stopped_at_the_loops_bounds`). A tool result
  for no call cannot be made: a result answers a call of the turn it follows, and a
  script that holds one in a turn is refused when it is read.

### Evidence

A tool's result is the committed output of the tool's own run: `view` is called through
the executor, a nested call is a run of its own, and a completed child keeps what it
committed whatever its parent comes to (ADR 0008; `SPEC.md` section 3, "What a run
publishes"). The plan's sentence was written before that was decided, and the spec comes
first. What is built:

- **Kept as a diagnostic.** `check` writes each crop a call returned among its own
  diagnostics (`evidence-001`, ...), as the image's canonical file. It is kept whatever
  the run comes to: after a failure on the second turn and after an accepted cancellation
  the store, opened anew, holds the run's terminal status, no verdict, no committed
  output and no completed status, and the evidence with every other diagnostic written
  before the run ended (`check::a_failure_on_turn_two`,
  `check::a_cancelled_check_has_no_verdict_and_keeps_its_evidence`).
- **Read back by the same run.** A crop a call of this run returned is readable in later
  turns of this run: the executor takes it as the input of a later call
  (`check::look_then_look_closer_at_part_of_the_first_crop_then_decide`).
- **Never an output of the action.** `check` has no output port, so the executor would
  refuse an artifact it handed back (ADR 0008), and a diagnostic reference is refused by
  the store wherever an output is asked for (ADR 0005).
- **Not read by another run's judge.** A second run of `check` whose judge names the
  crop's identity is refused: it was not given that image and no call of its own returned
  it, though the store holds it
  (`check::evidence_is_no_output_of_check_and_another_runs_judge_cannot_read_it`).

- **Kept exactly, or not at all.** Evidence is an image's canonical file, and a
  diagnostic's bytes pass through redaction, which works on bytes whatever they are
  (ADR 0005). A secret the executor was told of can stand in an image file by chance,
  eight bytes of compressed pixels, and taking it out would leave under the evidence's
  name a file that is no image. So evidence is written with `Diagnostics::write_exact`:
  kept as the bytes that were given, or refused, with nothing kept and nothing of the
  secret shown, when redaction would change one of them. `check` then fails, as an
  error of the type `evidence`: a run does not go on with evidence it could not keep.
  The crop itself is unchanged in the store, as the committed output of the tool's own
  run: an artifact is stored as its bytes, and is never redacted
  (`execute::bytes_written_exactly_are_kept_as_they_are_or_not_at_all`,
  `check::evidence_that_holds_a_secrets_bytes_is_not_kept_altered_and_the_run_fails`).
- **How much evidence one run holds.** At most 32 tool calls are dispatched
  (`CALLS_LIMIT`), each returns at most two crops, and a crop is at most 1,024 pixels
  on its longer side (`view::MAX_SIDE`): 64 files of at most 1,024 by 1,024 pixels at
  four bytes, 4 MiB of pixels each before the PNG's own framing and the artifact's
  record around it, so about 270 MiB at the very most, held in memory until the run
  ends, as every diagnostic is. That is under the 1,024 diagnostics a run may write
  (ADR 0008).

**What is not held, and why it is accepted.** The crop is also an image artifact in the
store, under its identity, as the output of the `view` run that cut it. A caller of the
executor that is no judge, a door or a recipe, takes it as an input by that identity, as
it takes any artifact
(`check::known_a_crop_that_a_view_run_committed_is_an_artifact_a_caller_of_the_executor_may_take`).
A store has one owner and no boundary between the runs of its callers; the boundary the
spec draws is around the judge model, and that one holds. A crop is a function of an
image and a box, and holds nothing of what the judge said. Making a tool's result
invisible to every other run would need a third kind of reference, an output of a child
that only its parent reads, in the store and in the commit step; it is on
`docs/carry-over.md` for milestone 2, with the tools a judge may call.

## Consequences

- An action reaches the model its call resolved. A test that hands an executor a backend
  now gives it a roster that resolves that backend, and an action that asks a capability
  declares the need.
- `Capabilities::with_segment`, `with_fill` and `with_judge` add a backend where they
  replaced one.
- A run's record says what it asked of which model and how long each request took, with
  no code in the action. A handle made by other code records nothing, as it counts
  nothing.
- `Refusal` has `NotOffered` and `NotReadable`; `RefusalKind` has 27 kinds.
- A backend's declaration is refused when one of its three names is over 128
  characters or is not plain ASCII. A fault and a turn hold the words they were made
  of until a handle has looked, and two are equal whatever of those they still hold.
- `Produced` holds a verdict beside its artifacts.
- A diagnostic name that starts with `exchange-` is the executor's.
- Each exchange holds the whole request, so what a run keeps grows with the square of
  its turns, up to the 256 MiB a run stores. `check` bounds its turns and the text of a
  turn, and a turn over that is not stored. Diagnostics are held in memory until the
  run ends, as before (`docs/carry-over.md`).
- A run that sends all 1,024 requests to a judge ends with 1,024 exchanges to publish:
  that took six seconds on the test kit's disk when it was measured, and no test runs it
  at that size any more.
- A stored conversation with an opaque part cannot be sent again.
- Reversing the record's groups costs every stored record after the tag; reversing the
  frozen calls costs `check` and every loop after it.

## Alternatives

- **Hand an action every backend and let it choose.** Turned down: which model answers
  would then be the action's decision, and the record of what the call resolved would
  not be what ran.
- **Refuse, at admission, a call whose resolved model no held backend declares.** Turned
  down for now: every executor built without backends, the binary's among them, would
  refuse every call that needs a capability, where today such a call is not ready for
  want of a roster. It comes with the config.
- **Let the action report its judge turns to the record.** Turned down: a loop that
  forgot one, or failed before it reported, would leave a request with no turn, and the
  failed turn is the one a reader most needs.
- **Put every turn whole in the record.** Turned down: a record is read whole, under four
  mebibytes, on every status and every repeat.
- **Write an exchange against the one before it,** storing only what was added. Turned
  down for now: it saves the square, and makes one exchange unreadable without all those
  before it. The loop's bounds are enough at this size (`docs/carry-over.md`).
- **Store an opaque body redacted, and mark it as changed.** Turned down: what is stored
  would be sent back to a provider as its own part by whatever resumes a run, and a
  signed part that was changed is refused, or worse, accepted.
- **Store an opaque body unredacted.** Turned down by the spec.
- **A verdict as a seventh artifact type.** Turned down above.
- **A verdict as a location or a series.** Turned down: a type is never taken for
  another (ADR 0002).
- **Keep the list of readable images in the loop,** as a set the loop checks. Turned
  down: the check would be the loop's to forget, and a refusal would be recorded only if
  the loop recorded it.
- **Call `view` as a function from inside `check`,** with no nested run. Turned down: a
  tool reaches a tool only through the executor (`SPEC.md` section 3), and the record's
  steps and refusals would have to be written by the loop.
- **Have a tool's run commit nothing and hand its crop back some other way.** Turned
  down for now: see "Evidence".
- **Freeze the tool list in the descriptor,** before admission. Turned down: which tools
  a run names is the call's, and which are on offer is the machine's state, which a
  descriptor does not hold (ADR 0003).
