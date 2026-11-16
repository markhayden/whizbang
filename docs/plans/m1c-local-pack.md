# Plan: checkpoint 1C, the local pack

Draft 3, 2026-11-15. From `SPEC.md` sections 4, 5, 10, 13, 16 and 17. This file is the
task list. Written from `main` at the tag `m1b` (commit 75f7dd6): the code, ADRs 0001 to
0019, `docs/plans/m1b-note.md` and the carry-over list as the exit check of 1B left it.

The plan of 1B grew past 3,000 lines and its length cost review time. This one states
each thing once. Where a task must decide a detail, the plan gives the constraint and the
task's ADR gives the detail.

## What this checkpoint delivers

The row of `SPEC.md` section 16: "The protocol is decided and its conformance suite
passes with fake models; the worker's queue, cancellation, restart and removal rules
pass; one runtime is chosen (section 17) and SAM 3 answers a phrase request on the Mac's
GPU."

1. **The protocol** between whizbang and the Python pack: an ADR, one codec for the local
   worker, and a conformance suite that the real pack passes with fake models, in the
   fast tier, on both CI runners, with no GPU and no third-party Python package.
2. **The worker:** one resident Python process, started and stopped by whizbang, with a
   bounded queue, cooperative cancellation, a restart at a deadline, and removal that
   drains.
3. **The real pieces 1B left:** a download client behind the checked downloader, a shipped
   catalogue with one entry, `whizbang models list|get|remove`, `whizbang backends
   status [--probe]`, `whizbang setup`, and the first real kind of backend, `pack`.
4. **One runtime, chosen by evidence,** and SAM 3 answering a phrase request on the
   owner's Mac, on its GPU, through the executor, with the device in the run's record.

Not in 1C: the `segment` action, point and box requests, Tiny, parity fixtures and the
mask tolerance (all 1D); experimental tools in the worker (milestone 2); the HTTP codec's
code and a remote worker (milestone 6); a real sender and prober (milestone 2); fill (milestones 3 and 8).

## What was looked up, and where

Checked on 2026-11-15 against public pages. Nothing here is from memory; what could not
be verified is under "Open questions".

| Fact | Source |
|---|---|
| SAM 3 is Meta's model for segmentation by concept: a short phrase or an image exemplar gives every matching instance. About 0.85 billion parameters | `https://github.com/facebookresearch/sam3`, `https://huggingface.co/facebook/sam3` |
| Its weights are at `facebook/sam3` on the Hugging Face Hub: `model.safetensors` 3.44 GB, `sam3.pt` 3.45 GB, a config, a processor config and tokenizer files | `https://huggingface.co/facebook/sam3/tree/main` |
| The repository is gated: "You need to agree to share your Hugging Face information to access this model", under Meta's privacy policy. A request is made in a browser, by a signed-in person, and is granted to one account. Many open discussions on the model's page ask for a rejected request to be reviewed, so a grant is not automatic for everyone | The model page, its discussions, and `https://huggingface.co/docs/hub/models-gated` |
| Licence: the "SAM License", last updated 2025-11-19. It grants use, copying, modification and distribution; a distribution carries a copy of the licence; it forbids military, nuclear and weapons uses and anything under ITAR; a publication acknowledges the use | `https://raw.githubusercontent.com/facebookresearch/sam3/main/LICENSE` |
| Meta's own package asks for Python 3.12, PyTorch 2.7 and "a CUDA-compatible GPU". Support for the Apple GPU is an open pull request (number 400), not merged | The repository's README and `https://github.com/facebookresearch/sam3/pull/400` |
| The `transformers` library has its own implementation. `Sam3Model` takes a phrase, and boxes as positive or negative exemplars; image features can be computed once and used for many phrases (`get_vision_features`, `vision_embeds`). `Sam3TrackerModel`, loaded from the same `facebook/sam3`, takes points and boxes and returns one object for each, with an IoU score. Its examples choose CUDA or the CPU; none names the Apple GPU | `https://huggingface.co/docs/transformers/model_doc/sam3`, `https://huggingface.co/docs/transformers/model_doc/sam3_tracker` |
| PyTorch's allocator for the Apple GPU is bounded by `PYTORCH_MPS_HIGH_WATERMARK_RATIO` (default 1.7), and `PYTORCH_ENABLE_MPS_FALLBACK=1` moves an unsupported operation to the CPU | `https://docs.pytorch.org/docs/2.15/mps_environment_variables.html` |
| A port to Apple's MLX exists (`mlx-community/sam3-image`, Apache-2.0, not gated): phrases and boxes, Apple silicon only | `https://huggingface.co/mlx-community/sam3-image` |
| ONNX Runtime has a Core ML execution provider with a listed set of supported operators | `https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html` |
| The Hub's client libraries send usage data unless `HF_HUB_DISABLE_TELEMETRY=1` or `DO_NOT_TRACK=1`, and make no call at all under `HF_HUB_OFFLINE=1` | `https://huggingface.co/docs/huggingface_hub/package_reference/environment_variables` |
| `uv.lock` is one lock file for every platform and is committed; extras are installed only when named | `https://docs.astral.sh/uv/concepts/projects/layout/`, `https://docs.astral.sh/uv/concepts/projects/sync/` |

From the frozen repos (reference, never authority; `docs/sources.toml`): rascal's pack
ran every model through ONNX Runtime on the CPU, spoke MCP over stdio through a library,
and took phrases only; its SAM 3 was its own ONNX export of `facebook/sam3`, hosted as a
release asset, 3,377 MB; its `tools/python/MODEL.md` says the owner accepted the model's
terms on 2026-11-05. Squint's pack client and server were hand-written, newline-delimited
JSON-RPC. Both installed the pack from an offline bundle with `uv sync --frozen`.

## Decisions this plan makes

**D1. The protocol is whizbang's own, not MCP, and the client that survives is squint's
kind.** `SPEC.md` section 17 replaces the pack over MCP and asks that one of the two old
clients survive for the pack. Rascal's use of a library does not: the library speaks
MCP. What survives is squint's hand-written, synchronous client: a thread that reads
and routes answers by request id, a cancel sent at a deadline, the worker's error
output read line by line, bounded and redacted, and a child stopped by closing its
input and then killing it. It is ported to the new frames with the tests that guard
those behaviours (task P3); its JSON-RPC and MCP parts are not. From rascal comes one
behaviour, as a test: the child's environment is cleared and given a short allow-list.
The server side of the choice is milestone 2's.

**D2. What the protocol is,** within section 4's constraints; task P1's ADR fixes the
bytes.

- Version: one name, `pack/1`. The first frame each way is a hello that states it. Any
  other version is refused: no ranges, no negotiation.
- Frame: a length, a JSON header of at most 64 KiB, then the payloads the header
  describes, each with its length. The local codec writes frames on the worker's
  standard input and output. The worker's error output is diagnostics only.
- Messages: `hello`, `request`, `result`, `event`, `cancel`, `error`, each with a
  request id. A request's kinds at 1C: `load`, `unload`, `segment`. A request can be
  sent in either direction, so that a tool in the worker can ask whizbang for a
  capability at milestone 2; at 1C the worker sends none and one from it is refused.
- A payload is raw samples, and its header entry states width, height, layout, data
  type, the colour and alpha rule, and the binding it belongs to. A segment request
  does not name its picture's binding today, so P1 amends ADR 0006: a request carries
  the identity of the binding its picture is under, and an answer's planes name the
  same one. No message holds a
  file path: the worker is told where the models directory is when it is started, and a
  `load` names a model and an installation's identity.
- Every bound is computed from the request before a byte is read: a reader refuses a
  frame over its bound without allocating for it. A request's bound alone is not
  enough: 64 queries of 64 instances of a large picture is hundreds of gigabytes of
  planes. So one result also has a ceiling of its own, 1 GiB of payload, and a result
  over it ends its call as `too_large`. And what a call may send beside its result is
  counted: 256 events and 64 KiB of diagnostics for one request; one more ends the
  call as malformed and restarts the worker.
- The HTTP codec is decided on paper in the same ADR (the same frames as a request
  body and a streamed response body) and built at milestone 6. The behavioural half of
  the conformance suite is written against a transport, so milestone 6 runs it again.

**D3. The conformance suite has two halves.** Fixed vectors, in `pack/conformance/`:
frames as bytes with what each parses to or why it is refused, read by a Rust test and
by a Python test, so the two codecs cannot drift. And a table of behavioural cases, run
by a Rust harness against the real pack started with fake models. The fake adapter is in
the pack, takes no third-party package, and answers by model name: `fake-ok` (a mask
computed from the request), `fake-wait` (blocks until cancelled), `fake-deaf` (blocks
and ignores cancel), `fake-exit` (the process ends inside a call), `fake-big` (answers
over its bound), `fake-bad` (a malformed payload). The shipped catalogue names none of
them.

**D4. The queue is on the Rust side, and there is one supervisor in a process.** "The
worker" is a supervisor in `whizbang-backends` and the Python process it owns. The
binary's wiring makes the supervisor once; every backend the kind `pack` makes, under
every snapshot of the config, shares it. So one whizbang process has at most one
worker, as section 4 says. The supervisor holds the bounded
queue (16 waiting requests; the 17th is refused), sends the process one model call at a
time, owns the deadline and the restart, and is tested with the injected clock. The
Python process holds one resident model, one call, and a flag that a `cancel` sets. One
place for policy, and nothing in Python to bound but one call.

**D5. Removal reaches the queue two ways, and neither can be missed.** A request takes
its lease before it enters the queue (ADR 0015). A removal in the same process tells
the supervisor, which at once ends every queued request for that model as refused, lets
their holds go, and unloads the model if it is resident and idle. A removal in another
process cannot tell it, and a mark read now and then could be set and taken off between
two looks. So each removal that sets its mark also raises a count kept in the model's
folder, written under the directory's lock and never lowered. A queued request keeps
the count it was given its lease under, and the supervisor reads the count for each
queued request every 250 ms of the injected clock and when it takes one off the queue:
a request whose count has moved is ended as refused, whether or not the mark is still
there. So a removal refuses new requests (the lease, 1B), cancels queued ones within
that interval even while another call holds the worker, waits for the current call (its
hold), and answers busy at its deadline with the mark taken off (1B). One limit,
stated: an idle worker in another process keeps a removed model in memory until its
next request or its end; the lease refuses every request for it, so nothing is answered
from it.

**D6. A mark is live only while its remover is.** The remover holds a lock for as long
as its mark is set, and one question, `models::draining`, answers yes only for a mark
whose lock is held. A hold, a publication, the supervisor and `models list` all ask
that question and no other, so a mark whose remover is gone stops none of them. That closes the row on a remover that died, without a command. A
removal still takes no cancellation: `whizbang models remove --wait <seconds>` sets its
patience, and a remover that is interrupted no longer strands the model.

**D7. Who starts and stops the worker.** The supervisor, lazily, at the first request
for a local model; never at start-up, and never for a run that needs none. It stops the
worker when the supervisor is dropped: it closes the worker's input, waits by the clock,
then kills and waits. The worker itself exits when its input closes, so a whizbang
process that dies takes its worker with it. Every wait on a worker is finite, on the injected clock, whether or not a caller gave a
deadline: 60 s to start and say hello, 120 s for a `load`, a call's own deadline or else
the kind's limit for a call (a setting, 120 s by default), 5 s for a cancelled call to
stop, 30 s for an `unload`, 5 s to end after its input closes. Past any of them the worker is killed.
Whizbang still handles no signal: the row on
the store's lock and a child being started is closed by a test that the worker is handed
no descriptor of the store, and by a line accepting the moment for Ctrl-C.

**D8. The runtime is chosen by task R1 from these candidates,** in this order, and the
first that meets the bar is the one. The bar is section 17's: it runs a SAM 3 phrase
request on the Mac's GPU, and there is documented evidence that it supports point and
box requests and CUDA.

| Candidate | Phrase on the Apple GPU | Point and box, documented | CUDA, documented | Notes |
|---|---|---|---|---|
| A. PyTorch, through the `transformers` implementation | Not documented; R1 shows it or it fails | Yes: `Sam3TrackerModel` | Yes | Loads `model.safetensors` as Meta published it; no export step |
| B. ONNX Runtime with the Core ML provider | Unknown; needs a new export, and the provider's operator list may send parts to the CPU | No: the old export is phrase only | Yes, by its CUDA provider | Smallest install; the most new work |
| C. MLX port | Documented by its author | Boxes yes, points not stated | No | Fails the bar: it would need a second runtime on Linux |
| D. Meta's own package | No: CUDA is a stated prerequisite | Yes | Yes | Fails the bar until the open pull request is merged |

The recommendation is A, and the tasks are written for A alone: N5 installs A's files
and Y1 locks A's packages. If A fails on the Mac, R1 stops and says so. B is then a
change to this plan, with its own tasks (an export, its files and their hosting, a
lock) and the review rounds; so is any candidate not in this table.

"On the Mac's GPU" is shown with the CPU fallback off (`PYTORCH_ENABLE_MPS_FALLBACK`
unset) and the device the model reports. If A runs only with the fallback on, R1 records
which operations fell back and the owner decides whether that meets the bar.

**D9. The pack is installed by a plain `uv` install from a committed lock file;** offline
bundles are dropped. `whizbang setup` runs `uv sync --locked` on the pack, with the
extra for the chosen runtime, into an environment whizbang owns. The lock pins every
package by version and hash. A bundle of a GPU runtime is gigabytes for each platform
and one more thing to build and host, and nothing in the spec's user stories installs a
pack without a network: the hosted engine of story 1 runs no Python at all.

**D10. The pack's core takes no third-party package.** The protocol, the worker loop,
the fake adapter and their tests use the standard library only (`unittest`, as
`scripts/baseline/` does). So the fast tier and CI install nothing from a package index,
and "the real pack speaking the real protocol with fake models" (section 13) costs CI a
pinned Python and nothing else. The runtime and the SAM 3 adapter's imports are an
extra, installed only where a model runs.

**D11. Weights are downloaded as their maker published them, by whizbang's installer,**
from the Hub at a pinned revision, checked by size and hash. Whizbang hosts and
redistributes no model file, so the licence's distribution terms do not bind it; the
catalogue entry names the licence. Only `model.safetensors` and the small text files are
listed: `sam3.pt` is a pickle and is never downloaded or loaded. The worker loads from
the installation's folder with `HF_HUB_OFFLINE=1`, `HF_HUB_DISABLE_TELEMETRY=1` and
`DO_NOT_TRACK=1` set, so the runtime's libraries contact nobody.

**D12. A download's credential is named by the config, per origin,** read from the env
file like every key, known to redaction, sent to its own origin only and never to an
address a download is moved to (ADR 0012). The catalogue entry says that its origin
needs one. No agent reads the token: the owner writes it into the env file that git
ignores, whose values the secrets scan already searches for.

**D13. A models directory is its owner's alone.** Whizbang makes it readable and
writable by its owner only, and refuses one that anyone else can write. With that, the
two rows on a look followed by an act (a folder swapped for a link; a pipe under a
file's name) are accepted in a line each: the only one who could make the swap is the
owner. Where the directory is, when nobody says: the platform's data folder for
whizbang; a config key and a flag override it.

**D14. An installation's files are verified when a model is loaded,** by the Rust side,
once for each installation in a process, before `load` is sent. A damaged file is not
loaded and the model is reported as damaged. The cost, hashing a few gigabytes, is
measured in the task and written in the ADR.

**D15. Model-tier evidence is a committed record, made by a script, checked by a
reviewer on the same Mac.** No model is in CI. `just test-models` runs the model tier
under the heavy lock; `scripts/model-evidence.sh` writes `docs/results/m1c-local-pack.md`
and its numbers file: the commit, the lock file's digest, the installation's identity,
the device, the times, the worker's peak thread count and memory, and for each request
the count of instances and the digest of each mask. The reviewer of that pull request
runs `just test-models` on the owner's Mac and names the head it ran on.

**D16. No spend machinery.** A local model is free: the kind declares no price and no
recipient, a call to it is admitted without the ledger (1B), and nothing in 1C adds to
the spend code.

## Rules every task's tests follow

Rules 3 to 10 of the 1B plan stand (stored bytes are not trusted; a secret is never
written; a document says what the head does; no sleeps; threads only through the test
kit; every loop that starts something is bounded; nothing is run to find a limit; tests
are run one way). A resident worker and a GPU runtime add these.

| What | Bound | Held by |
|---|---|---|
| Worker processes | One in a whizbang process (D4). A test file starts at most one at a time; the tests that start one are a group in `.config/nextest.toml`, `pack-workers`, two at a time | The supervisor; the group |
| A worker a test started | Killed and waited for on every path, a panic included | One guard type in the test kit (task P3), whose drop kills and waits; `just check` refuses a test that starts the pack another way |
| A worker nobody waits for | Ends by itself: at the end of its input, and at `--max-lifetime-s`, which every test passes (120) | The worker; a test of each |
| Threads in the worker | Three of the worker's own (reader, call, main). The runtime's pools are held to 8 threads each, before the runtime is imported: by the environment the supervisor sets (`OMP_NUM_THREADS`, `MKL_NUM_THREADS`, `VECLIB_MAXIMUM_THREADS`, `TOKENIZERS_PARALLELISM=false`) and by the adapter's own calls to the runtime; R1 lists every pool the chosen runtime has, from its documentation | The worker reports its count in `hello` and after each call; over 64 with a fake model fails the conformance suite |
| Threads in the worker, real model | 200 at most, enforced while it runs | R1's script and `just test-models` run under `scripts/thread-guard.sh` with its limit set to 200: it counts the command and everything it started, during the load and the call, and kills on the first count over. The evidence record holds the peak it saw |
| Threads in whizbang | Two for each worker (its output, its error output), through the supervisor | The supervisor's tests count them |
| GPU memory | The allocator's high watermark, set by the supervisor below 1.0 of the device's recommended maximum; the value is R1's | The environment the supervisor sets; the evidence record's peak |
| Time, a worker | The limits of D7, each finite with no caller's deadline | The supervisor; a test of each |
| Memory, one result | 1 GiB of payload (D2), refused before it is allocated; the adapter cuts candidates to the most instances asked before it makes a full-size plane | P1, R2a |
| Memory, beside a result | 256 events and 64 KiB of diagnostics for one request (D2) | P1, P4 |
| Time, fast tier | A call's deadline comes from the injected clock; one test 30 s, the suite 180 s, as now. The pack's Python tests: 10 s in all | `scripts/timing.sh` |
| Time, model tier | Load 120 s, one request 60 s, the recipe 10 minutes | The supervisor's deadlines; the recipe |
| Disk | One download of about 3.5 GB into a models directory outside the repo; one environment for the runtime, its size measured by R1 | Tasks N5 and Y1 only |
| Network | No test of the fast tier leaves the machine. The client's tests use a listener on loopback, made through the test kit, that serves a fixed number of connections. The client has a limit to connect (10 s), a limit between two reads (30 s), and a least rate for a download: under 256 KiB a second over any whole minute ends it as timed out. A test passes its own limits, under a second | Task N1 |

What kills a worker, in full: the end of its input; the supervisor at a deadline or when
it is dropped; its own lifetime limit in a test; and the test kit's guard. Nothing else
signals a process, and nothing kills a process whizbang did not start.

## Origins, and what needs the owner

Nothing was contacted while this plan was written beyond reading the public pages above.
Each origin below is new, carries no user data, and needs the owner's approval before
the task named.

| Origin | What for | First used by | Where |
|---|---|---|---|
| The Python package index (`pypi.org`, and `files.pythonhosted.org` for the files) | Resolving and installing the runtime extra: the lock file, then `whizbang setup` | Y1 | The owner's Mac; never CI |
| GitHub release downloads for a pinned Python build | `uv` installing the Python the pack pins, where none is there | C1 | The owner's Mac and both CI runners |
| `huggingface.co`, and the host it moves a file download to | The SAM 3 files, with the owner's token to the first origin only | N5 | The owner's Mac; never CI |
| `crates.io` | The network client's crates | N1 | Already used for every dependency; named for completeness |

Also the owner's, before N5: that the account behind the token has been granted access
to `facebook/sam3` (the old repo says the terms were accepted on 2026-11-05; the grant
shares the account's name and address with the model's maker), and the token written
into the env file by the owner.

**Spend: nothing.** Every model call is local, the downloads are free, and no task makes
a paid call. A task that would spend stops and goes to the owner.

**Disk:** about 3.5 GB of weights and the runtime's environment, of about 130 GB free.

## Open questions

Each is settled by the task named, from documentation or by the owner; none blocks C0.

1. Has the owner's account access to `facebook/sam3` today? (Owner, before N5.)
2. Which host does the Hub move a file download to? The entry's origins must name it.
   N5 takes it from the Hub's documentation, or from the refusal of its first attempt,
   and adds it only after the owner approves it by name.
3. Does candidate A run SAM 3 on the Apple GPU with the fallback off? (R1.)
4. Which Python does the runtime's wheel support? The pack pins the newest that it
   does. (Y1; the pack's core runs on any since 3.12.)
5. Does a lock that covers Linux with CUDA need a second package index? If it does, it
   is named to the owner before Y1 commits the lock; 1C installs on the Mac only.
6. The licences of the runtime's packages, read from their metadata, not from memory.
   (R1, in the ADR.)
7. SAM 3.1 (`facebook/sam3.1`, 2026-03-27) is a newer set of weights, for tracking
   several objects in video. This plan installs `facebook/sam3`, which the spec names
   and the library documents. (Owner, if 3.1 is wanted; a catalogue entry later.)
8. A fixture picture for the model tier that may be committed to a public repo. R2b uses
   an example picture already in `docs/reference/tools/` if one holds a thing with a
   name; otherwise it asks the owner for one.

## Strands and order

Two strands run side by side after C1 and join at P6.

| Strand | Tasks | Owns |
|---|---|---|
| Base | C0, C0b, C1, then P6, P7a, P7b, R1, R2a, R2b, Z1 | Root files; `crates/whizbang/src/`; the record's schema; `.github/`; the justfile |
| Protocol and worker | P1 to P4, P5b | `crates/whizbang-backends/src/pack/`; `pack/`; the test kit's guard |
| Network and models | N1 to N5, P5a, Y1 | `crates/whizbang-backends/src/net/` and `models/`; the catalogue; `whizbang models`, `backends`, `setup` |

| Task | Size | Needs |
|---|---|---|
| C0 Entry check | S | the tag `m1b` |
| C0b The macOS job inside its budget | S | C0 |
| C1 Roots, and Python in the fast tier | S | C0 |
| P1 The protocol: ADR, codec, vectors | M | C1 |
| P2 The Python worker with fake models | M | P1 |
| P3 The supervisor: start, hello, one call, stop | M | P2 |
| P4 Queue, cancellation, deadline, restart | M | P3 |
| P5a A mark is live only while its remover is; the count of removals | S | C1 |
| P5b Removal against the queue | S | P4, P5a |
| N1 The download client behind the checked downloader | M | C1 |
| N2 A models directory that is its owner's; a lock asked for until a time | S | C1 |
| N3a The catalogue's second version, the fifth reason, where the directory is | S | N2 |
| N3b `whizbang models list`, `get`, `remove` | M | N1, N3a |
| N4 A download's credential | S | N3b |
| N5 The shipped catalogue, and SAM 3 installed on the Mac | M | N4 |
| Y1 `whizbang setup`, and the lock file | M | P3 |
| P6 The kind `pack`: a local segmenter through the executor | M | P5b, N3b |
| P7a `run/4`: the device and the cache | S | P6 |
| P7b `whizbang backends status` | S | P7a |
| R1 The runtime, chosen | M | N5, Y1 |
| R2a The adapter `sam3` | S | R1 |
| R2b SAM 3 answers a phrase on the Mac's GPU | M | R2a, P7b |
| Z1 Exit check | S | all |

Twenty-three tasks, one pull request each.

## Commit strategy

- One task is one branch and one pull request, built tests first, reviewed by a reviewer
  that did not write it, merged by the orchestrator with a merge commit once `ci` and
  both scans pass.
- An ADR is in the pull request of the task that names it. Ported behaviour names its
  source in the commit message: the repo, the path, the pinned commit.
- **Until P6 merges the binary registers no kind,** so no config can start a worker.
  Until N5 merges the shipped catalogue is empty, so `models get` installs nothing.
  Every state of `main` between tasks passes `just check`.
- **Rollback checkpoints.** The tag `m1b` is the first. Two tags are added on the way,
  each a state worth returning to: `m1c-fakes` on the merge of P7b (the protocol, the
  worker, the commands and the kind, all against fakes; nothing downloaded), and `m1c`
  at the exit check.
- **If a task shows a decision here is wrong,** the decision is changed in this file
  first, in its own pull request; a change of scope or architecture gets the review
  rounds first. A test is never bent to fit.

| Tasks | To undo |
|---|---|
| C0b | Revert the merge: CI is as at `m1b` |
| C1, P1, P2 | Revert the later tasks that need it, then the merge. `pack/` and the vectors go together |
| P3, P4, P5a, P5b | Revert the merge; the binary never started a worker before P6 |
| N1 to N4 | Revert the merge. A models directory a test wrote is in a temporary folder |
| N5 | Revert the merge: the catalogue is empty again. The installed files stay in the owner's models directory until `whizbang models remove sam3`, or the folder is deleted by the owner |
| Y1 | Revert the merge. The environment `whizbang setup` made is one folder, named in the task's hand-back |
| P6, P7a, P7b | Revert the merge: the binary registers no kind. A store that holds a `run/4` record is discarded with P7a; none is kept before the tag |
| R1, R2a, R2b | Revert the merge; the ADR and the evidence record go with it |
| Z1 | The note reverts; the tag stays and a corrected checkpoint is `m1c.1` |

## Tasks

A "Done when" is a list of tests by the name each will have; the name is the clause. A
builder that finds a clause cannot be shown as worded rewords it here in the form
**Reworded with the task**, quoting the old words and the reason.

### C0. Entry check

**Depends on:** the tag `m1b`. **Reads:** `docs/plans/m1b-note.md`, `docs/carry-over.md`,
the job logs of the last twenty runs of `ci` on `main`.

- [ ] `main` at the tag: CI green on both runners, `just check` and `just scan` pass,
      and each file, type and function this plan names is there. A line that no longer
      fits is corrected here.
- [ ] Every carry-over row that names 1C has the task or the owner that "Carry-over rows
      and their tasks" gives it; the new rows it asks for are added.
- [ ] **The macOS job, measured before anything is changed.** From the logs: for each of
      twenty runs, the time of each recipe inside `just ci` (the debug build and lint,
      the tests, the advisories, the release build of the smoke, the example that must
      not compile, the scan), and whether the build cache was restored. Then the
      coverage review section 13 asks for: the ten slowest tests on that runner and what
      each guards.
- [ ] One option is chosen from the measurements and written here for C0b:
      (a) the cache: restore it on pull requests from `main`'s, if runs are building
      from nothing; (b) the release build of the smoke and the example that must not
      compile run on the Linux runner only, if the second build is what costs; (c) the
      heaviest crash sweeps move to a tier that runs on `main` and before a tag, if
      test time is what costs, each named with what still guards its behaviour on a
      pull request; (d) the owner changes the budget. Options (c) and (d) are put to
      the owner before C0b; (a) and (b) change no coverage on any commit that merges.
- [ ] The owner is asked, in one message, for what "Origins, and what needs the owner"
      lists, with the task each is needed before.

**Done when** the measurements are in this file under this task, the rows are re-owned,
and `just check` passes. **Size:** small. Documents only.

### C0b. The macOS job inside its budget

**Depends on:** C0.

- [ ] The option C0 chose, and nothing else.

**Done when**, before the merge, five runs of `check (macos-15)` in a row on the branch's
head (the same commit, run again) each take under 10 minutes, with the test time of
each in the hand-back; or the owner's line on the budget is in `SPEC.md` section 13 and
the carry-over row. What the job takes on `main` afterwards is Z1's, in the note's
timing table. No limit in `scripts/timing.sh` or `.config/nextest.toml` is raised by
this task.
**Carry-over:** the row on pull-request CI on the macOS runner. **Size:** small.

### C1. Roots, and Python in the fast tier

**Depends on:** C0. **Needs the owner:** the origin for a pinned Python.

- [ ] `pack/`: a `pyproject.toml` with no dependency, the pinned Python version, an
      empty package `whizbang_pack` and one `unittest` test.
- [ ] `uv` joins `scripts/tool-versions.txt`; `just setup` checks it. `just test-pack`
      runs the pack's tests with the pinned Python, offline, and joins `just check`;
      `AGENTS.md` and `SPEC.md` section 11 list it.
- [ ] CI: one step installs `uv` at its pinned version, by an action pinned to a commit,
      with its cache. Its time on each runner is in the hand-back.
- [ ] The module roots `pack/` and `net/` in `whizbang-backends`, and the subcommands
      `models`, `backends` and `setup` as stubs that say they are not built.

**Done when** `just check` passes on both runners with the new step under 30 seconds;
`whizbang describe` and the generated reference are unchanged. **Size:** small.

### P1. The protocol: its ADR, the Rust codec, the vectors

**Depends on:** C1. **Reads:** `SPEC.md` section 4; ADR 0006, "Segment" and "Faults";
ADR 0002 for a plane's layout; rascal `crates/rascal-mcp/src/client/mod.rs` (the size
bound on an answer) and `crates/rascal-mask/src/filler/mod.rs` (`answer_cap`); squint
`crates/squint-tools/src/mcp/mod.rs`.

- [ ] ADR 0020, the pack protocol: D2 in bytes. The hello; the frame; each message and
      its fields; a payload's entry; every bound and how it is computed from a request;
      what a refusal says; the HTTP codec on paper; what `pack/2` would mean.
- [ ] `whizbang_backends::pack::proto`: frames written and read, strictly. Segment's
      request and response in it: an image, the queries, the most instances; instances
      with a plane, a box and a score. The settings rascal's adapters took (a threshold,
      a padding, an outline rule, a least cover) are a model's settings in the config,
      not fields of a request; an instance's diagnostics and predicted IoU go in the
      result's diagnostics; the order of an answer is fixed here: best score first, ties
      by the query, then by the box's top left.
- [ ] The amendment to ADR 0006 of D2: `SegmentRequest` carries its picture's binding,
      and the handle refuses an answer whose planes name another.
- [ ] `pack/conformance/vectors/`: each message as bytes, and each refusal.

**Done when** these pass, in `crates/whizbang-backends/tests/it/pack/proto.rs`:

- `proto::every_vector_parses_to_what_it_states_and_is_written_back_byte_for_byte`
- `proto::a_hello_of_another_version_is_refused_and_says_both_versions`
- `proto::a_header_or_a_payload_over_its_bound_is_refused_before_it_is_read`
- `proto::a_result_over_one_gibibyte_of_payload_is_refused_whatever_its_request_would_allow`
- `proto::an_unknown_field_a_missing_field_and_an_unknown_message_are_each_refused`
- `proto::a_payload_states_its_dimensions_layout_type_colour_alpha_and_binding_and_one_that_disagrees_with_its_length_is_refused`
- `proto::two_requests_of_the_same_pixels_under_two_bindings_are_two_requests_and_each_answer_names_its_own`
- `proto::the_257th_event_of_a_request_and_diagnostics_past_their_bound_are_refused_without_being_kept`
- `proto::no_message_has_a_field_that_holds_a_path`
- `proto::an_answer_is_in_the_fixed_order_whatever_order_it_was_built_in`
- `proto::a_frame_cut_at_any_byte_is_an_error_and_never_a_shorter_message`

**Carry-over:** what a segmenter adapter took and gave (settings, diagnostics, order);
the bound on what is read off a wire. **Size:** medium.

### P2. The Python worker, with fake models

**Depends on:** P1. **Reads:** ADR 0020; rascal `tools/python/src/rascal_tools/server.py`
and `models/adapters.py` (the adapter's interface and its fakes, as reference).

- [ ] `whizbang_pack.proto`: the same frames, standard library only.
- [ ] `whizbang_pack.worker`: reads frames on one thread, runs one call at a time on
      another, answers `cancel` by setting the call's flag, exits at the end of its
      input and at `--max-lifetime-s`. One resident model: a `load` of another unloads
      the first. It is given the models directory as an argument and opens an
      installation by the model's name and the identity a `load` names.
- [ ] The adapter interface (`load`, `segment`, `unload`, the features it declares, its
      revision) and the six fake models of D3.

**Done when** these pass under `just test-pack`, in `pack/tests/`:

- `test_proto.EveryVector.test_parses_and_is_written_back_byte_for_byte`
- `test_proto.Refusals.test_each_refusal_vector_is_refused_for_its_stated_reason`
- `test_worker.Hello.test_another_version_is_refused_and_the_worker_exits`
- `test_worker.Calls.test_one_call_runs_at_a_time_and_a_second_model_unloads_the_first`
- `test_worker.Cancel.test_a_cancel_ends_a_waiting_call_as_cancelled`
- `test_worker.Ends.test_the_worker_exits_at_the_end_of_its_input_and_at_its_lifetime`
- `test_worker.Imports.test_the_core_imports_nothing_outside_the_standard_library`
- `test_worker.Threads.test_the_worker_holds_three_threads_with_a_fake_model`

The tests drive the worker in-process over pipes, and start one child process, once,
for the two tests of how it ends. **Size:** medium.

### P3. The supervisor: start, hello, one call, stop

**Depends on:** P2. **Reads:** `crates/whizbang-core/src/store/fs.rs`, "What these locks
are, and are not"; the carry-over row on the store's locks; squint
`crates/squint-tools/src/mcp/mod.rs` and its tests `minimal.rs` and `full.rs` (the
client that survives, D1: what is ported and what is not is in the commit message);
rascal `crates/rascal-mcp/src/client/env.rs` (the environment's allow-list).

- [ ] `whizbang_backends::pack::Worker`: started from a command line it is given, with a
      cleared environment and an allow-list (no variable whose name ends in `_KEY`,
      `_TOKEN` or `_SECRET`, and none the config names as a key), the thread limits and
      the offline variables of D11 set; a hello each way; one request and its result;
      stopped as D7 says. Its error output is read by line, 256 lines of 4,096 bytes
      kept, redacted, and handed to a run's diagnostics.
- [ ] `whizbang_testkit::worker`: the guard of "Rules", and the pack with fake models as
      a fixture.
- [ ] The behavioural half of the conformance suite, as a table of cases over a
      transport, with the cases this task can show.

**Done when** these pass, in `crates/whizbang-backends/tests/it/pack/`:

- `conformance::the_real_pack_with_a_fake_model_answers_a_segment_request_whose_masks_are_those_the_request_determines`
- `conformance::a_worker_of_another_protocol_version_is_refused_at_the_hello_and_is_not_left_running`
- `conformance::a_result_over_the_bound_its_request_gives_is_refused_before_it_is_built_and_the_call_ends_as_too_large`
- `conformance::a_malformed_payload_ends_the_call_as_malformed_and_the_worker_is_restarted`
- `worker::a_worker_is_started_at_the_first_request_and_not_before`
- `worker::a_worker_that_says_no_hello_within_its_limit_is_killed_and_the_request_ends_as_unavailable`
- `worker::the_worker_is_handed_no_descriptor_but_its_three_and_none_of_the_store`
- `worker::the_workers_environment_holds_the_allow_list_and_no_key`
- `worker::a_dropped_supervisor_closes_the_input_and_the_worker_is_gone_when_the_drop_returns`
- `worker::a_worker_that_does_not_end_at_the_end_of_its_input_is_killed_at_the_deadline_and_waited_for`
- `worker::a_fault_made_from_a_workers_failure_holds_none_of_the_marker_the_host_and_the_token_its_error_output_held`
- `worker::a_request_that_could_not_be_written_is_not_sent_and_one_written_and_unanswered_is_unresolved`
- `worker::the_error_output_kept_is_bounded_and_a_secret_in_it_is_redacted`

**Carry-over:** the child's copy of the store's lock (closed by the second and fourth
tests and the line of D7); a fault's sentence; `not_sent` only when no byte left.
**Size:** medium.

### P4. Queue, cancellation, deadline, restart

**Depends on:** P3. **Reads:** `SPEC.md` section 4, "The worker"; ADR 0008,
"Cancellation".

- [ ] The supervisor's queue of D4, with one call in the worker at a time. A request for
      another model waits for the current call; then the old model is unloaded.
- [ ] A cancelled call is sent `cancel`; one that has not ended by its deadline has its
      worker killed. A call past its own deadline ends as `timed_out` at the worker's
      boundary. A restart, and a worker that dies, end the current call and every
      queued request with a fault that says so; none is sent again. The next request
      starts a new worker; nothing starts one in a loop.
- [ ] ADR 0020 gains "The worker": the queue's size, the states, what each fault says.

**Done when** these pass, in `tests/it/pack/queue.rs`, each on the injected clock and
with no sleep:

- `queue::the_seventeenth_waiting_request_is_refused_as_unavailable_and_not_held`
- `queue::requests_run_one_at_a_time_in_the_order_they_came`
- `queue::a_request_for_another_model_waits_for_the_current_call_and_the_first_model_is_then_unloaded`
- `queue::two_requests_for_one_model_cause_one_load`
- `queue::a_cancelled_call_that_stops_ends_as_cancelled_and_the_worker_lives_on`
- `queue::a_cancelled_call_that_has_not_stopped_at_its_deadline_has_its_worker_restarted`
- `queue::a_call_past_its_deadline_ends_as_timed_out_and_its_worker_is_restarted`
- `queue::a_call_with_no_deadline_is_cut_off_at_the_kinds_limit_and_a_cancelled_one_five_seconds_after_its_cancel`
- `queue::a_load_or_an_unload_that_does_not_end_within_its_limit_has_its_worker_killed`
- `queue::a_worker_that_floods_events_has_its_call_ended_as_malformed_and_is_restarted`
- `queue::a_restart_ends_the_current_call_and_every_queued_request_each_with_a_fault_that_says_the_worker_was_restarted`
- `queue::a_worker_that_exits_inside_a_call_fails_what_was_in_flight_and_the_next_request_starts_another`
- `queue::nothing_that_was_dispatched_is_sent_again_after_a_restart`
- `queue::a_queued_request_that_is_cancelled_leaves_the_queue_and_is_never_sent`

**Carry-over:** a request past its deadline is cut off at the worker's boundary.
**Size:** medium.

### P5a. A mark is live only while its remover is; the count of removals

**Depends on:** C1. **Reads:** ADR 0017, "Removal", "Holds" and "Three kinds of lock";
`models/remove.rs`, `models/hold.rs`.

- [ ] D6 and the count of D5, by an amendment to ADR 0017: the remover's lock and its
      place in the order of locks; `models::draining` as the one question; the count,
      where it is written and how it is read.

**Done when** these pass (`tests/it/models/remove.rs`):

- `remove::a_hold_that_meets_the_mark_of_a_remover_that_was_killed_is_given`
- `remove::a_hold_that_meets_the_mark_of_a_live_remover_is_refused_as_draining`
- `remove::a_publication_and_a_listing_take_the_mark_of_a_dead_remover_as_no_mark`
- `remove::each_removal_that_sets_its_mark_raises_the_count_and_a_removal_that_timed_out_leaves_it_raised`
- `remove::a_crash_at_any_event_of_a_removal_leaves_the_model_installed_or_not_installed`
  (1B's sweep, still passing with the new lock and the count)

**Carry-over:** the mark left by a remover that died; a removal that takes no
cancellation (a line: the patience is enough, since a dead remover no longer strands
the model). **Size:** small.

### P5b. Removal against the queue

**Depends on:** P4, P5a.

- [ ] D5: the supervisor is told of a removal in its process, and reads the count for
      queued requests by the clock and when it takes one off the queue.

**Done when** these pass (`tests/it/pack/removal.rs`):

- `removal::a_removal_refuses_new_requests_cancels_the_queued_ones_and_waits_for_the_current_call`
- `removal::a_removal_from_a_second_process_cancels_the_queued_requests_within_the_interval_while_another_call_holds_the_worker`
- `removal::a_removal_that_began_and_timed_out_between_two_looks_still_cancels_the_requests_queued_before_it`
- `removal::a_removal_whose_wait_times_out_answers_busy_and_the_model_takes_requests_again`
- `removal::a_removal_in_this_process_unloads_the_model_that_was_resident_and_idle`
- `removal::after_a_remover_was_killed_with_its_mark_set_a_request_for_the_model_is_answered_by_the_worker`

**Carry-over:** what the worker's queue owes to removal. **Size:** small.

### N1. The download client behind the checked downloader

**Depends on:** C1. **Reads:** ADR 0012, "Three interfaces"; ADR 0019, "Every hop", "A
download and a probe", "For later tasks"; `egress/ways.rs`, `chains.rs`, `hops.rs`.

- [ ] One blocking HTTP client over TLS in `whizbang_backends::net`, by a crate that
      `just deny` accepts; any licence it adds to `deny.toml` is named in the pull
      request. It implements `Downloader`, the one way out 1C uses; a real `Sender`
      and a real `Prober` wait for the first thing that sends (milestone 2), since a
      sender shaped with no provider to send to would be a guess. It follows nothing:
      an answer that points elsewhere comes back as `Moved`. A download is streamed to
      its sink and stopped at the bytes it was asked for, and has the limits of
      "Rules": to connect, between two reads, and a least rate.
- [ ] In the binary the client is made only inside a `CheckedDownloader`.

**Done when** these pass (`tests/it/net/`, against the loopback listener of "Rules"):

- `client::a_download_is_streamed_to_its_sink_and_stopped_at_the_bytes_it_was_asked_for`
- `client::an_answer_that_points_elsewhere_is_handed_back_as_moved_and_nothing_is_asked_of_the_second_address`
- `client::a_credential_is_sent_as_its_header_to_its_own_origin_and_a_request_without_one_sends_none`
- `client::a_fault_holds_nothing_of_the_body_the_headers_or_the_address_that_answered`
- `client::a_request_that_never_connected_is_not_sent_and_one_cut_after_it_was_written_is_unresolved`
- `client::a_cancellation_is_looked_at_between_pieces`
- `client::a_peer_that_accepts_and_sends_nothing_ends_the_request_as_timed_out_at_the_limit_between_reads`
- `client::a_cancellation_set_while_no_byte_arrives_ends_the_request_by_that_limit_at_the_latest`
- `client::a_peer_that_sends_a_byte_now_and_then_ends_the_download_as_timed_out_by_the_least_rate`
- `client::a_download_through_the_checked_downloader_that_is_moved_off_its_entrys_origins_is_refused`
- `client::the_installer_installs_a_model_of_two_files_from_the_listener_through_the_checked_downloader`

**Carry-over:** the real network client, for downloads; a fault's sentence and
`not_sent`, for this adapter. **Size:** medium.

### N2. A models directory that is its owner's; a lock asked for until a time

**Depends on:** C1. **Reads:** ADR 0017, "Three kinds of lock", "A folder of the layout
that is a link", "Limits"; `models/directory.rs`; `store/fs.rs`.

- [ ] D13: a models directory is made for its owner only, and one that another can
      write is refused when it is opened, with a refusal that says how to fix it.
- [ ] The directory's lock is asked for until a time, not a count: tries paced by the
      injected clock for 5 seconds, then busy. The 20,000 tries go. A lock that blocks
      without an end is not used: a lease and a removal's `--wait` must each end.

**Done when** these pass (`tests/it/models/directory.rs`):

- `directory::a_new_models_directory_is_readable_and_writable_by_its_owner_only`
- `directory::a_models_directory_that_its_group_or_anyone_can_write_is_refused_and_nothing_in_it_is_read`
- `directory::two_publications_at_once_both_end_and_neither_is_refused_as_busy`
- `directory::a_directory_whose_lock_is_held_past_five_seconds_of_the_clock_answers_busy_and_nothing_waits_longer`

**Carry-over:** the lock by 20,000 tries; the two rows on a look and then an act, each
accepted in a line that names the first two tests. **Size:** small.

### N3a. The catalogue's second version, the fifth reason, where the directory is

**Depends on:** N2. **Reads:** ADR 0017, "The catalogue", "The inventory"; ADR 0015, "An
inventory"; ADR 0011 on the `models` table.

- [ ] `catalogue/2`, by an amendment to ADR 0017: an entry gains a title and the
      platforms it runs on. `catalogue/1` is no longer read: no catalogue was shipped.
- [ ] `Unready` gains a fifth reason, by an amendment to ADR 0015: this build has no
      adapter for the model. The inventory gives it where it said out of date.
- [ ] Where the directory is: a flag, then the config's `[models] dir` (an amendment to
      ADR 0011), then the platform's data folder.
- [ ] The binary's wiring is given the inventory of the shipped catalogue, which is
      empty until N5 (`Over::installed`), told the secrets the executor is told.

**Done when** these pass (`tests/it/models/`, `crates/whizbang/tests/it/wired.rs`):

- `catalogue::an_entry_of_the_second_version_holds_a_title_and_its_platforms_and_the_first_version_is_refused`
- `inventory::a_model_whose_adapter_this_build_lacks_is_not_ready_for_that_reason_installed_or_not`
- `wired::a_call_for_a_model_whose_adapter_this_build_lacks_is_refused_when_prepared_with_that_reason`
- `wired::the_models_directory_is_the_flags_then_the_configs_then_the_platforms`
- `wired::under_the_binarys_wiring_a_call_for_an_installed_model_is_prepared_under_its_identity_and_holds_a_lease`
- `wired::a_secret_the_executor_is_told_of_is_refused_as_a_models_name_by_the_binarys_inventory`

**Carry-over:** the adapter this build lacks (the reason); a model's title, platforms
and where the directory is; one set of secrets for the inventory and the executor.
**Size:** small.

### N3b. `whizbang models list`, `get` and `remove`

**Depends on:** N1, N3a. **Reads:** the carry-over row on the commands (it names what
they call); ADR 0017, "Attaching and cancelling", "Removal".

- [ ] The three commands over `whizbang_backends::models`, and `docs/guides/models.md`.
      `list` prints, for each entry: its name, title, licence, size, platforms and
      state. `get` installs or attaches, waits between tries by the clock, and prints
      what the status record shows. `remove --wait <seconds>` drains.

**Done when** these pass (`crates/whizbang/tests/it/models.rs`, with a test's catalogue
and the fake downloader through the binary's own function, and `list` as the real
binary):

- `models::list_names_each_model_of_the_catalogue_with_its_title_licence_size_platforms_and_state`
- `models::get_installs_a_model_and_list_then_says_it_is_installed`
- `models::a_second_get_attaches_to_the_first_waits_by_the_clock_and_ends_with_the_same_installation`
- `models::remove_drains_and_answers_busy_at_its_wait_with_the_model_still_installed`
- `models::a_model_whose_adapter_this_build_lacks_is_listed_with_that_reason`
- `models::a_model_this_platform_cannot_run_is_listed_as_that_and_get_refuses_it`
- `models::a_model_whose_layout_has_a_link_is_listed_as_having_one`
- `models::a_secret_the_config_names_is_in_nothing_the_commands_print`

**Carry-over:** the `models` commands; how one who attaches is paced; what `models
list` says of a link. **Size:** medium.

### N4. A download's credential

**Depends on:** N3b. **Reads:** ADR 0012, "An address, and a credential"; ADR 0019, "A
download and a probe"; ADR 0011 on keys from the env file.

- [ ] D12, by amendments to ADRs 0011, 0012 and 0017: the config's key for an origin's
      token; an entry's statement that its origin needs one; the installer attaching it
      to a request for that origin.

**Done when** these pass (`tests/it/models/credential.rs`, `tests/it/config/`):

- `credential::a_download_from_an_origin_the_config_names_a_token_for_carries_it_and_one_from_another_origin_does_not`
- `credential::a_download_moved_to_another_origin_is_asked_there_with_no_credential`
- `credential::an_entry_whose_origin_needs_a_token_the_config_does_not_name_is_refused_before_anything_is_asked_and_says_which_key_to_set`
- `credential::the_token_is_in_no_status_record_no_refusal_and_nothing_get_prints`
- `credential::an_origin_that_answers_that_access_is_refused_ends_the_install_with_a_sentence_that_says_to_ask_for_access`

**Carry-over:** a credential for a download that needs one. **Size:** small.

### N5. The shipped catalogue, and SAM 3 installed on the Mac

**Depends on:** N4. **Needs the owner:** the Hub's origins approved; access to the
model; the token in the env file. **Runs on:** the owner's Mac, for its last box.

- [ ] The shipped catalogue, built into the binary, with one entry, `sam3`: the files
      of D11, each `from` an address at a pinned revision of `facebook/sam3`, the
      licence as its maker names it, the features this build's adapter gives (`phrase`).
- [ ] How the sizes and hashes were fixed, written in the entry's comment and the ADR:
      the large file's from the Hub's own listing, which states its SHA-256; the small
      files' from the bytes fetched once through the checked downloader by
      `whizbang models pin <draft entry>`, a developer's command that prints each file's
      size and hash and installs nothing. The second is trust on
      first use, stated as a limit.
- [ ] On the Mac: `whizbang models get sam3`, by the owner or with the owner's leave,
      into a models directory outside the repo. Nothing but whizbang downloads a file.
      The token is never printed, passed as an argument or read by an agent.

**Done when** these pass in the fast tier:

- `catalogue::the_shipped_catalogue_parses_names_sam3_and_no_fake_model`
- `catalogue::every_file_of_the_shipped_entry_is_at_an_origin_the_entry_names_and_at_one_pinned_revision`
- `catalogue::the_shipped_entry_names_no_pickle`
- `models::pin_prints_the_size_and_hash_of_each_file_it_fetched_through_the_checked_downloader_and_installs_nothing`

and, on the Mac, recorded in the pull request with the installation's identity:
`whizbang models list` says `sam3` is installed; a second `get` installs nothing; the
secrets scan passes over the working tree with the token in the env file.
**Carry-over:** the shipped catalogue. **Size:** medium.

### Y1. `whizbang setup`, and the lock file

**Depends on:** P3. **Needs the owner:** the package index approved. **Reads:** rascal
`crates/rascal-mcp/src/provision.rs` and `scripts/build-bundle.sh` (what a bundle was,
as reference for what is dropped).

- [ ] ADR 0021, installing the pack: D9 and D10; where the environment is; what `setup`
      contacts; why bundles are dropped.
- [ ] The pack's extra for candidate A, and `pack/uv.lock`, made on the Mac.
- [ ] `whizbang setup [--extra <name>]`: `uv sync --locked` of the pack into the
      environment, by `uv` at its pinned version, and a check that the worker it made
      says hello. The kind `pack` finds the worker there. `docs/guides/setup.md` says
      what it installs and every origin it contacts.

**Done when** these pass (`crates/whizbang/tests/it/setup.rs`; the first three install
no package and run offline, in the fast tier):

- `setup::setup_with_no_extra_makes_an_environment_offline_whose_worker_says_hello`
- `setup::setup_refuses_a_lock_file_that_does_not_match_the_pack_and_changes_nothing`
- `setup::a_second_setup_changes_nothing`
- `setup::the_lock_file_pins_every_package_of_every_extra_by_version_and_hash`

and, on the Mac, recorded in the pull request: `whizbang setup --extra` for the runtime
ends well, with the environment's size. **Carry-over:** offline bundles, dropped in a
line. **Size:** medium.

### P6. The kind `pack`: a local segmenter through the executor

**Depends on:** P5b, N3b. **Reads:** ADR 0011, "Kinds"; ADR 0015, "An inventory", the
lease; ADR 0006, "Segment"; the carry-over rows on the roster and the backends, and on
the old configs.

- [ ] `kind = "pack"` in `wiring::kinds`: a `Segmenter` over the supervisor. It declares
      no recipient and no price, and the features the catalogue's entry states. Its
      settings in the config are the model's (the threshold and the rest of P1), by an
      amendment to ADR 0011; a setting it does not know is refused.
- [ ] D14: the files are verified before `load`. A `load` names the installation in the
      declaration the call was handed under, never the one the pointer names.
- [ ] One walk of the config makes the roster and the backends in the binary.

**Done when** these pass (`crates/whizbang/tests/it/pack.rs`, through the binary's
wiring, with a test action that asks for the segment capability and the pack's fake
models in a test's catalogue):

- `pack::a_call_to_a_local_model_is_answered_by_the_worker_and_its_record_names_the_backend_the_model_and_the_installation`
- `pack::two_actions_in_one_process_that_use_one_model_cause_one_load`
- `pack::two_backends_of_the_kind_under_two_names_and_two_snapshots_of_the_config_share_one_worker`
- `pack::a_run_that_needs_no_local_model_starts_no_worker`
- `pack::after_a_reinstall_between_two_requests_of_a_run_the_first_is_answered_from_the_old_installations_files_and_the_second_is_refused_at_the_lease`
- `pack::a_file_damaged_after_it_was_published_is_not_loaded_and_the_model_is_reported_as_damaged`
- `pack::a_model_the_config_resolves_and_no_backend_holds_is_refused_at_admission`
- `pack::a_setting_the_kind_does_not_know_is_refused_when_the_config_is_read`
- `pack::a_local_call_reserves_nothing_and_adds_no_name_under_spend`
- `pack::the_shipped_binary_offers_no_fake_model`

**Carry-over:** a local backend loading by the identity it was handed under; unverified
files of an installation; the roster and the backends from one thing; the settings of
the first real kind. **Size:** medium.

### P7a. `run/4`: the device and the cache

**Depends on:** P6. **Reads:** ADR 0005 and ADR 0013 on a record's versions;
`contract/record.rs` (`Model`).

- [ ] A capability's response says which device answered and whether a cache did (an
      amendment to ADR 0006). The worker states its device in `hello` and in each
      result; the local kind has no cache and says so. `run/4`: its schema, its fixed
      vectors, and what a reader does with a `run/3` record.
- [ ] `whizbang runs show` prints an installation's identity whole.

**Done when** these pass (`crates/whizbang-core/tests/it/`, `crates/whizbang/tests/it/`):

- `record::a_run_4_record_holds_the_device_and_whether_a_cache_answered_for_each_request`
- `record::a_run_3_record_is_still_read_and_says_its_device_is_not_known`
- `pack::the_record_of_a_local_call_names_the_device_the_worker_reported`
- `runs::show_prints_an_installations_identity_whole_and_two_installations_are_told_apart`

**Carry-over:** `device` and `cached`; the `runs show` cut. **Size:** small.

### P7b. `whizbang backends status`

**Depends on:** P7a. **Reads:** `SPEC.md` section 6, "Readiness"; ADR 0019, "A
download and a probe".

- [ ] Each configured backend with its kind, its model and whether its model is ready,
      with no network and no worker. `--probe` starts the worker of a local backend and
      prints its hello (the protocol version, the device). A remote backend is
      probed only through `CheckedProber`, shown with the test kit's prober: no real
      prober is in the build until something remote is (milestone 2). `docs/guides/backends.md`.

**Done when** these pass (`crates/whizbang/tests/it/backends.rs`):

- `backends::status_lists_each_configured_backend_and_starts_no_worker`
- `backends::status_with_probe_prints_the_hello_of_a_local_backends_worker_and_the_worker_is_gone_when_the_command_ends`
- `backends::status_with_probe_asks_a_prober_of_a_remote_backend_only_when_the_config_approves_probing_it`

Tag `m1c-fakes` on the merge. **Carry-over:** `backends status`. **Size:** small.

### R1. The runtime, chosen

**Depends on:** N5, Y1. **Runs on:** the owner's Mac. **Reads:** D8 and the pages of
"What was looked up".

- [ ] Candidate A, tried by one script in `pack/` that is run once by hand under
      `scripts/heavy.sh`, with the thread limits and the watermark of "Rules" and a
      10-minute limit: load from the installation's folder, one phrase on one picture,
      twice. If A fails the task stops (D8).
- [ ] ADR 0022, the first runtime: each candidate against the bar, with the page that
      shows point, box and CUDA support; what was measured (the device, load time, the
      first and second request, peak memory, threads, the environment's size); every
      thread pool the runtime has and what holds each; the
      licences of the runtime's packages from their metadata; what the choice rules out
      (nothing: a second runtime is another adapter); the watermark chosen.
- [ ] `SPEC.md` section 17's rows on the runtime, the pack over MCP, the two clients and
      offline bundles say what was decided. Wording only.

**Done when** ADR 0022 names one runtime with its numbers and its sources, the script's
output is in the pull request, and `just check` passes. A fast test holds the ADR to the
lock: `setup::the_runtime_the_adr_names_is_the_one_the_lock_pins`.
**Size:** medium.

### R2a. The adapter `sam3`

**Depends on:** R1.

- [ ] The adapter `sam3` in the pack, on the chosen runtime: image features computed
      once for a batch; a phrase for each query; the settings of P6; the answer in the
      fixed order. It declares `phrase` only. It asks its runtime for the GPU, falls
      back to the CPU, and reports the device it got.
- [ ] Its tests run in CI against a stand-in for the runtime that the tests give it: no
      runtime is imported.

**Done when** these pass under `just test-pack`:

- `test_sam3.Phrase.test_features_are_computed_once_for_a_batch_of_phrases`
- `test_sam3.Phrase.test_instances_under_the_threshold_are_left_out_and_the_rest_are_in_the_fixed_order`
- `test_sam3.Phrase.test_a_point_or_a_box_is_refused_as_unsupported`
- `test_sam3.Phrase.test_candidates_are_cut_to_the_most_instances_asked_before_a_full_size_plane_is_made`
- `test_sam3.Device.test_the_device_reported_is_the_one_the_runtime_gave`
- `test_sam3.Device.test_with_no_gpu_the_adapter_runs_on_the_cpu_and_says_so`

**Size:** small.

### R2b. SAM 3 answers a phrase on the Mac's GPU

**Depends on:** R2a, P7b. **Runs on:** the owner's Mac.

- [ ] `just test-models` and `scripts/model-evidence.sh` (D15), and the fixture picture
      with a loose box drawn by hand around the thing its phrase names. The recipe is
      heavy, runs under `scripts/thread-guard.sh` with its limit at 200, refuses without
      a models directory named in the local environment, and is listed in `AGENTS.md`.
- [ ] The catalogue's entry and the adapter's revision say what R2a built.
      `docs/reference/` is unchanged: no action is added in 1C.

**Done when** these pass under `just test-models` on the Mac, through the binary's
wiring with the installed model:

- `model_tier::sam3_answers_a_phrase_on_the_gpu_with_an_instance_inside_the_fixtures_box_and_the_record_names_the_device`
- `model_tier::a_second_request_for_the_same_model_loads_nothing`

with `docs/results/m1c-local-pack.md` and its numbers file committed, and the
reviewer's own run named in its review. The evidence script times the load, the first
request and the second, three runs each, and reports them without asserting an order:
that is the measurement the carry-over row on the speed of local models asks of 1C.
In the fast tier, `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`
fails when the record is stale. **Size:** medium.

### Z1. Exit check

- [ ] Each clause of the row for 1C in `SPEC.md` section 16 is matched to a named test
      in `docs/plans/m1c-note.md`, which `scripts/tests/test-notes.sh` reads. The clause on the Mac's GPU is matched to the model-tier test and the
      evidence record.
- [ ] Every carry-over row that named 1C is gone or has the owner the table below gives.
- [ ] The note: what shipped; what C0 changed; the decisions the owner made; the timing
      table by suite on both runners and the Mac, the pack's tests among them; what was
      spent (nothing); what was downloaded and from where; what a reader should not
      assume; what 1D starts from.
- [ ] Tag `m1c`.

**Size:** small.

## Which clauses only the owner's Mac can show

| Clause | Shown by | Where the evidence is |
|---|---|---|
| The real files install from the real host | N5 | Its pull request: the identity, and `models list` |
| The runtime installs from the lock | Y1 | Its pull request: the environment's size |
| One runtime runs SAM 3 on the GPU | R1 | ADR 0022 |
| SAM 3 answers a phrase on the Mac's GPU, through the executor | R2b, `model_tier::…` | `docs/results/m1c-local-pack.md`, and the reviewer's run |

Everything else in the checkpoint is shown in the fast tier on both runners.

## Carry-over rows and their tasks

Every row of `docs/carry-over.md` whose owner names 1C: thirty.

| Row | Task, or new owner |
|---|---|
| The store's locks: a child's copy while the worker is started | P3; the moment for Ctrl-C accepted in a line (D7) |
| Pull-request CI on the macOS runner over its budget | C0, C0b |
| A file opened by a look and then an opening; a lock file opened plainly | N2: accepted in a line (D13) |
| The run record: `device` and `cached` | P7a (`run/4`) |
| What a segmenter adapter took and gave | P1 (the order, diagnostics), P6 (settings) |
| What a filler was sent beside an image and one plane | **Milestone 3, with the first fill backend.** 1C has none |
| An adapter's keeping of the rule for a fault's sentence | P3 (the worker), N1 (the client) |
| Nothing bounds what a backend builds; nothing cuts off one past its deadline | P1 (the bound), P4 (the deadline) |
| Whether a failed call may have been charged: `not_sent` | P3, N1 |
| The roster and the backends given apart | P6 |
| The models directory's lock by 20,000 tries | N2 (a time in place of a count) |
| A folder of the layout that is a link | N2 (accepted in a line), N3b (`list` says so) |
| Nothing reads an installation's files before they are loaded | P6 (D14) |
| A model whose adapter this build lacks | N3a (the reason), N3b (the command) |
| Parallel installs of models sharing a file | **1D, with the second catalogue entry.** 1C ships one entry, so nothing can share |
| What a catalogue entry and a models directory held in rascal | N3a (title, platforms, where the directory is), N4 (the credential). The graph's external data: dropped in a line by R1 when the runtime is A, which loads no graph |
| `whizbang models list`, `get`, `remove` and their guide | N3b |
| `runs show` cuts a model's version | P7a |
| One who attaches is paced by its caller | N3b |
| `whizbang backends status` | P7b (a remote probe by the test kit's prober; a real one with milestone 2's sender) |
| The real network client and the shipped catalogue | N1 (downloads), N3a, N3b, N5. **The sender, with `SendCheck::new().keeping_out(root)` over it in the binary, and a real prober: milestone 2, with the first provider's kind.** Nothing in 1C sends, and the row itself says a provider's sending follows there |
| A local backend loads the installation it was handed under | P6 |
| A backend made from a config sends with no credential | **Milestone 2, with the first provider's kind.** The kind `pack` sends nothing to a recipient |
| A recipient is equal only with its upstreams | **Milestone 2,** for the same reason |
| The encoder for sending stores its rows | **Milestone 2, before the first picture sent to a provider.** 1C sends no picture off the machine |
| What the worker's queue owes to removal | P5b |
| A removal is ended by its deadline alone | P5a (a line) |
| The mark of a remover that died | P5a (D6), P5b (a request answered after it) |
| What the old configs held: the first real kind's settings | P6; the judge loop's stay with milestone 2 |
| Speed of local models (R12) | R2b measures a first and a second request; 1D the rest |

New rows C0 adds: the HTTP codec's code and its conformance run (milestone 6); a request
from the worker to whizbang, for tools beside the model (milestone 2); holding several
models and running calls side by side (none; reopen on need, as section 4 defers it); a
second runtime (none; on a measured need, as section 17 says); a user's own weights
and a user's own adapter with the conformance test they are given (1D, with the
`add-adapter` skill).

## Risks

| Risk | Effect | What we do |
|---|---|---|
| Candidate A does not run SAM 3 on the Apple GPU | The last clause of the row is not met | R1 runs before the adapter is written and needs only N5 and Y1, so it is known early; the tag `m1c-fakes` holds everything else. B is then a change to this plan, and the owner is told |
| It runs only with operations falling back to the CPU | "On the GPU" is arguable | D8: the fallback is off for the evidence; with it on, the operations are listed and the owner decides |
| Access to the model is refused, or the token is not there | N5, R1, R2a and R2b wait | Asked at C0, needed at N5; both strands up to P7b need neither |
| The Hub moves a download to a host nobody approved | The installer refuses, as built | Open question 2: the host is named to the owner before it is added |
| A worker outlives its test or its parent | Processes pile up on the owner's machine | Four things end a worker ("Rules"), each with a test; a worker is started lazily and one at a time |
| The runtime starts more threads than a machine should hold | The machine is slowed or worse | The pools are set before the runtime is imported; the count is read after each call and fails the run over 200 |
| The protocol is fixed before the real adapter exists | A second version inside one checkpoint | The protocol carries planes and requests ADR 0006 already fixed; R2a changes the adapter, not the frames; a misfit is `pack/2`, never an edit |
| Python in the fast tier slows `just check` and CI | The budgets C0b just met are lost | D10: no package is installed; the pack's tests have 10 seconds; C1 reports the step's time |
| The macOS budget cannot be met without losing coverage | C0b has no honest fix | Options (c) and (d) are the owner's |
| A network client's crates fail `just deny` | N1 waits | The crate is chosen for it; a licence added is named in the pull request |
| Hashing gigabytes at each first load is slow | A slow first request | D14 measures it; once for each installation in a process |
| Two builders edit one file | A conflict | The strands' ownership; ADR amendments are additive sections |

## What 1D starts from

`main` at the tag `m1c`: `pack/1` and its conformance suite; a worker with its queue; the
kind `pack`; SAM 3 installed by whizbang and answering phrases on the Mac through one
chosen runtime, with `run/4` naming the device; the evidence record and its script. 1D
adds point and box to the adapter (the library's second class over the same files, if
the runtime is A), the `segment` action, Tiny as a second entry and adapter, parity
fixtures with a tolerance, timings, the runtime on Linux (CUDA, or the CPU), and the
`add-adapter` skill.

## Review rounds

Recorded as `AGENTS.md`, "Working with the owner", requires. Each round was one
read-only run of the independent reviewing model over this repository at the branch's
head, asked for: what the spec's row or sections 4, 5 and 17 ask that no task delivers;
a carry-over row for 1C with no task; a task that cannot be built as written or whose
"Done when" cannot be tested; a contradiction with the spec or an ADR; a machine-safety
or public-repo risk the plan does not bound; a task that is too large. The second round
was also asked what it would cut.

| Round | Date | Draft | Blockers | Majors | Minors |
|---|---|---|---|---|---|
| 1 | 2026-11-15 | 1 | 1 | 8 | 2 |
| 2 | 2026-11-15 | 2 | 1 | 5 | 2 |

Every finding of round 1 was fixed in draft 2, and every finding of round 2 in draft 3,
the cuts it proposed among them: the real sender and prober moved to milestone 2, P5
and R2 were each split, and two quotas were lowered or dropped. No round followed the
second.
