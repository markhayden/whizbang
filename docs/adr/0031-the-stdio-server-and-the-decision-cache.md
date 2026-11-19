# 0031. The stdio server and the decision cache

Status: accepted.
Date: 2026-11-18
Spec: sections 1, 3, 9 and 17; plan m2c (`docs/plans/m2c-milestone-2.md`), task M1;
ADR 0008, ADR 0009, ADR 0020

## Context

Milestone 2 owes a door for a local agent: `whizbang serve`, MCP over stdio, offering each
action as a tool, and a job asked again unchanged answered from the decision cache. Section
17 left the server's library to be chosen here: squint's `rmcp`, or a server written by hand
on the frames of the protocol. It is the first process of whizbang that lives across runs, so
three things a command could leave to its own exit become this ADR's: how long a run is
waited for, what is said while it runs, and whether installed files checked once are still
the files loaded.

## Decision

**The server is written by hand, in the crate `whizbang-serve`**, on JSON-RPC 2.0 frames,
one message to a line, speaking the revisions 2025-06-18, 2025-03-26 and 2024-11-05
(`PROTOCOL_VERSIONS`). It depends on the core and `serde_json`, both already in the binary.
It adds no crate to the binary and no licence to `deny.toml`. `rmcp` would bring an async
runtime and its macros to a binary that has none, for four methods: `initialize`, `ping`,
`tools/list` and `tools/call`, and two notifications, `notifications/cancelled` and
`notifications/progress`. The binary hands the server the executor and a `Door` (the tools,
how a call of one becomes a call of an action, what is said of a run that ended, and what to
stop at the end). No MCP type leaves the crate.

**What is read.** One line at most 1 MiB (`MESSAGE_LIMIT`); a longer line is read to its end
and dropped without being held. Nesting at most 16 deep (`DEPTH_LIMIT`), counted before the
line is parsed. Request ids and progress tokens are whole numbers or strings of at most 128
characters. Nothing a client sent is said back but its id and its token; an argument the
tool does not take is refused without naming it.

**Threads.** One, the reader of the input, made by a spawner of the crate's own that holds a
process to two (`READERS_LIMIT`): the fourth place in the workspace that makes a thread
(`clippy.toml`, `scripts/tests/test-thread-lint.sh`). The reader hands messages over a
channel of eight, so a client that writes faster than the server takes is made to wait. Runs
are started by the executor's entry point (`Executor::start`), whose bound on started runs
holds; the server holds at most four calls at once (`CALLS_LIMIT`) and answers one more
that it is busy. The four counts the calls on the server's list: a run the server gave up on, once the
grace after its stated time has passed, is answered and dropped from the list though its
action may still be running, so more than four may then run, and only the executor's bound
on started runs holds them (`serve::a_run_given_up_after_its_grace_no_longer_counts_toward_the_limit_of_calls`).

**A call is a call of an action.** It is prepared, admitted, reserved for and sent through
the gate exactly as from the command line, since it goes through the same entry point; the
executor is built as `squint check`'s, over the config, its env file and the models
directory, with every backend in use, so a config that names a credential is not served
without its env file. `check` is offered in the shape a caller of a judge asks in: the use
case, the images by their identities with their roles and labels, the brief, the judge.

**A run that waits gives up after a stated time.** Each run is started with a deadline at the
server's stated time (`--wait-s`, 900 by default); at it the server cancels the run, and
answers the call that it gave up, once the run ended or after a grace of ten seconds.

**Events are published while a run executes.** Between messages the server looks at each
run every 50 ms (`Executor::status`) and sends each event it has not sent as
`notifications/progress` under the call's token; at a run's end the rest come from its
journal. Every text the server sends has its control characters escaped (`escaped`).

**Cancellation and the end of the input.** `notifications/cancelled` cancels the run of that
call, which is then not answered. At the end of the input every run is cancelled and waited
for, up to the grace, and the door stops the pack's one worker.

**Installed files are verified before each load** (the carry-over row on files verified
once in a process). The pack's supervisor asks the kind, in the request's turn and right
before each `load` (`Supervisor::call_checked`), to verify the installation's files; so a
load after an unload or a restart of the worker looks again. A model that stays resident is
not loaded and not looked at. The record of a full check (plan 2A, task R1) keeps an
unchanged file from being read whole again.

**The decision cache** (section 9) lives in `check`, behind a flag its registry is built
with: on when the config has `[cache]` (with `decisions = false` it is off), for `squint
check` and `serve`; evaluation builds its registry without it, and a run that names its cell
of a matrix never looks. The key is worked out before anything is measured or sent, as the
SHA-256 of canonical JSON (`decision-cache/1`): the job without where its pictures came from,
every argument but `cell`, each image's identity and binding in order, the rules' version,
the system text and the checklist as the judge is shown them and the fixed instructions, each
tool the run may call with its revision, the judge's identity (backend, model, and a version
that names the image preparation), the config's digest, the limits of the loop and the
versions of `check` and its crate. A run that concludes keeps the material whole as a
diagnostic named by the key, only when every turn of its judge named the model that answered
(`Turn::reported_model`, read from each provider's answer); a hit reads that material back
and compares it whole before it answers, with the earlier run's verdict, reasons and
decisions, and notes `decision_cache = hit` and `answered_by_run` in its record. A hit is
admitted like any run: its reservation is taken and settled at nothing.

## Consequences

The server is ours to keep to the protocol as it moves; a revision with methods it lacks
means code here. A process now lives for days: a run abandoned by its action (one that
ignores its cancel) is given up on after the grace and left to the executor. Cancelling a
run from a second process is still not possible (the carry-over row, owed by task F1). The
cache spends nothing and is invisible to evaluation; a hit returns a verdict a provider
gave under a model name it reported then, which the earlier run's record keeps.

## Alternatives

- `rmcp` at a pinned version: an async runtime and a macro crate in the binary for four
  methods, and its own reading of the input, whose bounds are not ours.
- The cache keyed by the door, before the executor: a forged earlier verdict could then be
  handed to `check` as an argument; in the action, the key and the read are the action's.
- A cache for every judge: section 9 caches only what a backend that reports its identity
  answered.
