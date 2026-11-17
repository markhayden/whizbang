# Plan: checkpoint 1C, the local pack

Draft 5, 2026-11-15. From `SPEC.md` sections 4, 5, 10, 13, 16 and 17. This file is the
task list. Written from `main` at the tag `m1b` (commit 75f7dd6): the code, ADRs 0001 to
0019, `docs/plans/m1b-note.md` and the carry-over list as the exit check of 1B left it.
Draft 5 is the entry check's (task C0): its measurements, four follow-ups of the review,
and what the owner decided on 2026-11-15 ("Review rounds", at the end, lists them).

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
| The `transformers` library has its own implementation. `Sam3Model` takes a phrase, and boxes as positive or negative exemplars; image features can be computed once and used for many phrases (`get_vision_features`, `vision_embeds`). `Sam3TrackerModel`, loaded from the same `facebook/sam3`, takes points and boxes and returns one object for each, with an IoU score. The library's pages place the model with `device_map="auto"`; the model card's examples choose CUDA or the CPU; none names the Apple GPU | `https://huggingface.co/docs/transformers/model_doc/sam3`, `https://huggingface.co/docs/transformers/model_doc/sam3_tracker` |
| PyTorch's allocator for the Apple GPU is bounded by `PYTORCH_MPS_HIGH_WATERMARK_RATIO` (default 1.7), and `PYTORCH_ENABLE_MPS_FALLBACK=1` moves an unsupported operation to the CPU | `https://docs.pytorch.org/docs/2.15/mps_environment_variables.html` |
| A port to Apple's MLX exists (`mlx-community/sam3-image`, Apache-2.0, not gated): phrases and boxes, Apple silicon only | `https://huggingface.co/mlx-community/sam3-image` |
| ONNX Runtime has a Core ML execution provider with a listed set of supported operators | `https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html` |
| The Hub's client libraries send usage data unless `HF_HUB_DISABLE_TELEMETRY=1` or `DO_NOT_TRACK=1`, and make no call at all under `HF_HUB_OFFLINE=1` | `https://huggingface.co/docs/huggingface_hub/package_reference/environment_variables` |
| `uv.lock` is one lock file for every platform and is committed; extras are installed only when named | `https://docs.astral.sh/uv/concepts/projects/layout/`, `https://docs.astral.sh/uv/concepts/projects/sync/` |
| `uv` reads no configuration file under `--no-config` (`UV_NO_CONFIG`); `UV_CONCURRENT_DOWNLOADS`, `UV_CONCURRENT_INSTALLS` and `UV_CONCURRENT_BUILDS` bound what it does at once; `UV_OFFLINE` turns the network off; `UV_PROJECT_ENVIRONMENT` names the environment's folder; `tool.uv.package = false` means the project itself is neither built nor installed | `https://docs.astral.sh/uv/reference/environment/`, `https://docs.astral.sh/uv/concepts/projects/config/` |

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
those behaviours (tasks P3a and P3b); its JSON-RPC and MCP parts are not. From rascal comes one
behaviour, as a test: the child's environment is cleared and given a short allow-list.
The server side of the choice is milestone 2's.

**D2. What the protocol is,** within section 4's constraints; task P1's ADR fixes the
bytes.

- Version: one name, `pack/1`. The first frame each way is a hello that states it. Any
  other version is refused: no ranges, no negotiation.
- Frame: a length, a JSON header, then the payloads the header describes, each with
  its length. A header is 64 KiB at most, but for a result's, which names every
  instance: ADR 0006 allows 4,096 of them, each with a binding of 71 characters, so a
  result's header is bounded from its request, at 64 KiB and 512 bytes for each
  instance the request allows. The local codec writes frames on the worker's
  standard input and output. The worker's error output is diagnostics only.
- Messages: `hello`, `request`, `result`, `event`, `cancel`, `error`, each with a
  request id. A request's kinds at 1C: `load`, `unload`, `segment`. A model's
  settings travel in `load`: at most 32 names with scalar values, as the config
  allows. A request can be
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
  call as malformed and restarts the worker. What counts toward the 64 KiB: the text
  of every event and the diagnostics field of the result. Not counted there, because
  each has a bound of its own: an instance's score and predicted IoU, which are typed
  fields of its entry in the header; the payloads; and the worker's error output.
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
them. Faults of the worker itself, which no model can produce, are chosen by
`--fake-fault <name>`, taken only by the entry point `whizbang_pack.fakes`, which the
kind `pack` never starts: `no-hello`, `other-version`, `deaf-to-end-of-input` (always
with `--max-lifetime-s`), `error-output`, `flood`, `slow-load`, `slow-unload`.

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
folder, written under the directory's lock and never lowered. The hold a
lease takes returns the count it read under that lock, the lease hands it on in what
a backend is given for one request, and a queued request keeps it (P5a, P6b; ADRs 0017
and 0015 are amended), and the supervisor reads the count for each
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
whose lock is held. A hold, a publication, an install's own look at the mark where it asks its
cancellation, the supervisor and `models list` all ask that question and no other, so
a mark whose remover is gone stops none of them. The question is a try for a share of
the remover's lock, which the remover holds alone: two that ask at once each get a
share, and neither reads the other as a live remover. That closes the row on a remover that died, without a command. A
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
from the Hub at a pinned revision, checked by size and hash. In 1C whizbang hosts and
redistributes no model file: the weights are fetched as their maker publishes them, so
in 1C the licence's distribution terms do not bind it; the catalogue entry names the
licence. That is true of 1C only. The owner expects whizbang to host the model later,
so that a customer needs no Hugging Face account of their own (the owner's statement,
as the orchestrator passed it on, 2026-11-15); the owner's token is for testing and for
getting the model onto whizbang's side. Hosting is not designed and not built in 1C:
its carry-over row is milestone 4's and starts with reading the licence for what
redistribution requires. Only `model.safetensors` and the small text files are
listed: `sam3.pt` is a pickle and is never downloaded or loaded. The worker loads from
the installation's folder with `HF_HUB_OFFLINE=1`, `HF_HUB_DISABLE_TELEMETRY=1` and
`DO_NOT_TRACK=1` set, so the runtime's libraries contact nobody.

**D12. A download's credential is named by the config, per origin, and for the Hub it
is read from the sign-in the Hub's command-line tool already keeps on the machine.** The
owner chose this on 2026-11-15, in the owner's words "for now": whizbang reads the token
of that sign-in, and the owner writes it nowhere else. The config names, for an origin,
where its token is: that sign-in, or a variable of the env file, which stays as the way
for anyone without the sign-in and is read like every key. Whichever it is:

- It is read only when a download from that origin needs it, and not before: no other
  command, and no `get` of a model from another origin, opens the sign-in's store.
- It is read by the binary and by nothing else: no agent, no test and no script reads
  the owner's store, and no command prints it or takes it as an argument.
- It is told to redaction before a request is made, and is in no output, log, status
  record, run record or fault's sentence.
- It is sent to its own origin only, `huggingface.co`, and never to another origin (ADR
  0012; a hop back to the same origin carries it, as ADR 0019 has it).
- It is never handed to the worker: P3a's allow-list passes no variable that holds it,
  and the worker's environment points the Hub's libraries away from the sign-in's
  store, so the runtime does not read it either (open question 9).
- **A test cannot reach the owner's store.** The store's place is a value the binary's
  wiring is given. Only `main` gives the real one, by the one function that names it,
  and `just check` refuses a call of that function anywhere else. Every test gives a
  temporary folder that holds a made-up token; a test that starts the real binary
  starts it with a cleared environment whose home folder is a temporary one, so the
  real place cannot be reached from it. CI has no sign-in and needs none.

The catalogue entry says that its origin needs a token. The secrets scan does not know
this token's value, since it is in no env file: what it has is its pattern for a token
of the Hub's shape (`scripts/scan.sh`), and N5 records fields, never raw output.

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
| A worker a test started | Killed and waited for on every path, a panic included | One guard type in the test kit (task P3a), whose drop kills and waits; `just check` refuses a test that starts the pack another way |
| A worker nobody waits for | Ends by itself: at the end of its input, whatever it is doing then (inside a `load`, or a call that ignores `cancel`: the reading thread ends the process and waits for nothing); and at `--max-lifetime-s`, which the test kit's guard passes (120) and which the kind `pack` passes from its setting `max_lifetime_s`, set by every test that starts a worker through the binary's wiring (P6a, P6b, P7a, P7b, R2b). `whizbang setup` is built before the kind (Y1 needs P3a only), so it takes no setting: the worker it starts for its hello check is given a fixed lifetime, 120, by `setup` itself | The worker; a test of each (P2, P6a, Y1) |
| `uv` | One at a time, started only by `whizbang setup` and `just test-pack` (and once by hand for Y1's lock), and on CI by the step that installs the pinned Python, which the job's time limit bounds (`timeout-minutes`, 15). **Reworded with the task** (by C1b): the words "and on CI by the step … 15" are added; the row named only `setup`, `just test-pack` and the lock, and C1b's CI step starts `uv` too. And by the test kit's fixture, `uv python find` alone, offline, once in a test process that starts a worker, so at most two at a time (the group `pack-workers`), killed and waited for at 60 s. **Reworded with the task** (by P3a): that sentence is added; the fixture finds the pinned Python as `just test-pack` does (P3a, "Built with the task"). A cleared environment with P3a's allow-list; no user's configuration (`--no-config`); `UV_CONCURRENT_DOWNLOADS=4`, `UV_CONCURRENT_INSTALLS=4`, `UV_CONCURRENT_BUILDS=1`. Killed and waited for on every path out of `setup`. 60 s with no extra, which is offline; 30 minutes with one. The stand-in for a `uv` that never ends, which the test of the limit starts, ends itself: after 120 s and not before, so that it outlasts the limit the test passes and still ends when the test is killed and nothing is left to kill it. The Mac's install with an extra, and the making of the lock, run through `scripts/heavy.sh` with the thread limit at 200, and the peak is in the hand-back | Y1: `setup::a_uv_that_does_not_end_within_its_limit_is_killed_and_waited_for`, `setup::uv_is_given_the_allow_list_and_no_users_config` |
| The pack's Python tests | Run under `scripts/thread-guard.sh`. The runner ends itself at a stated limit, 60 s, by the standard library's `faulthandler.dump_traceback_later(…, exit=True)`. A thread a test starts is a daemon thread, so no test's thread keeps the runner alive | C1b |
| Threads in the worker | Three of the worker's own (reader, call, main). The runtime's pools are held to 8 threads each, before the runtime is imported: by the environment the supervisor sets (`OMP_NUM_THREADS`, `MKL_NUM_THREADS`, `VECLIB_MAXIMUM_THREADS`, `TOKENIZERS_PARALLELISM=false`) and by the adapter's own calls to the runtime; R1 lists every pool the chosen runtime has, from its documentation | The worker reports its count in `hello` and after each call; over 64 with a fake model fails the conformance suite |
| Threads in the worker, real model | 200 at most, enforced while it runs | R1's script and `just test-models` run under `scripts/thread-guard.sh` with its limit set to 200: it counts the command and everything it started, during the load and the call, and kills on the first count over. Both also run under the watch on resident memory of the row below. The evidence record holds the peak it saw |
| Threads in whizbang | Three for each worker (its output, its error output, its input), through the supervisor. **Reworded with the task** (by P3b): this cell read "Two for each worker (its output, its error output)". A write to a worker that stops reading waits until the worker reads or ends, and its lifetime may be a year; the workspace forbids the `unsafe` a write with a time limit would need, so a third thread writes the input and the supervisor waits for it on the clock (D7) | The supervisor's tests count them; `pack::threads` holds a process to three |
| GPU memory | The allocator's high watermark, set by the supervisor below 1.0 of the device's recommended maximum; R1 starts at 0.5 and writes the value it ends with | The environment the supervisor sets; the evidence record's peak |
| The worker's resident memory, on the CPU path too | Measured, and capped only in the model tier, by one watch that three things run under: R1's script, `just test-models` and `scripts/model-evidence.sh`. It reads the resident size of the command and of everything it started, beside their threads, and kills them over 24 GiB. R1 writes the watch, since R1 is the first to load the model, and R2b's recipe and script use the same one. In ordinary use it is not capped, because nothing in the standard library caps a process's memory in a way macOS is documented to hold to; what bounds it is one resident model, one call, and the ceilings on a picture and a result. If R1 finds a cap that holds, the supervisor sets it | R1 (the search, the peak), R2b |
| Time, a worker | The limits of D7, each finite with no caller's deadline | The supervisor; a test of each |
| Memory, one result | 1 GiB of payload (D2), refused before it is allocated; a result's header bounded from its request; the adapter cuts candidates to the most instances asked before it makes a full-size plane | P1, R2a |
| Memory, beside a result | 256 events and 64 KiB of diagnostics for one request (D2) | P1, P4 |
| Time, fast tier | A call's deadline comes from the injected clock; one test 30 s, the suite 180 s, as now (300 s on GitHub's hosted macOS runner since the owner's decision of 2026-11-17: `SPEC.md` section 13). The pack's Python tests: 10 s in all | `scripts/timing.sh`; for the pack's Python tests, their runner, `pack/tests/run.py`. **Reworded with the task** (by C1b): this cell read "`scripts/timing.sh`" alone. That script times the Rust tests; the pack's Python tests are run by `just test-pack`, and their runner is what holds them to 10 s, failing a run over it and saying how long it took |
| Time, model tier | Load 120 s, one request 60 s, the recipe 10 minutes | The supervisor's deadlines; the recipe |
| Disk | One download of about 3.5 GB into a models directory outside the repo; one environment for the runtime, its size measured by R1 | Tasks N5 and Y1 only |
| Network | No test of the fast tier leaves the machine. The client's tests use a listener on loopback, made through the test kit, that serves a fixed number of connections. The client has a limit to connect (10 s), a limit between two reads (30 s), and a least rate for a download, a setting whose default is 256 KiB a second: under it over any whole minute ends the download as timed out. A test passes its own limits, under a second. Root certificates are compiled in: the platform's verifier may ask a revocation service nobody approved | Task N1 |

What kills a worker, in full: the end of its input; the supervisor at a deadline or when
it is dropped; its own lifetime limit in a test; the test kit's guard; and, in the model
tier only, the watches on threads and on resident memory. Nothing else signals a
process, and nothing kills a process whizbang did not start.

## Origins, and what needs the owner

Nothing was contacted while this plan was written beyond reading the public pages above.
Each origin below is new, carries no user data, and needs the owner's approval before
the task named. What the owner has approved since is under the table.

| Origin | What for | First used by | Where |
|---|---|---|---|
| The Python package index (`pypi.org`, and `files.pythonhosted.org` for the files) | Resolving and installing the runtime extra: the lock file, then `whizbang setup` | Y1 | The owner's Mac; never CI |
| `github.com` (`/astral-sh/python-build-standalone/releases/download/…`), and `release-assets.githubusercontent.com`, where GitHub serves a release's files from (named by C1b) | `uv` installing the Python the pack pins, where none is there | C1b | Both CI runners; the owner's Mac only if the pinned Python is ever missing there |
| `github.com` (`/astral-sh/uv/releases/download/0.12.23/…`), and `release-assets.githubusercontent.com`: where the CI action, `taiki-e/install-action` at a pinned commit, fetches `uv` from (named by C1b) | `uv` on the runners. On the Mac the installed `uv` is the pinned version, so nothing is contacted | C1b | Both CI runners only |
| `huggingface.co`, and the host it moves a file download to | The SAM 3 files, with the owner's token to the first origin only. Also, outside whizbang: N5's builder reads the file listing of the pinned revision from the Hub's public API, for the large file's hash; that reading is part of this approval | N5 | The owner's Mac; never CI |
| `crates.io` | The download client's crates | N1 | Already used for every dependency; named for completeness |

Also the owner's, before N5: that the account behind the token has been granted access
to `facebook/sam3` (the old repo says the terms were accepted on 2026-11-05; the grant
shares the account's name and address with the model's maker), and the token, which
whizbang reads from the Hub's command-line sign-in on the Mac (D12).

### What the owner approved, and what is still asked

The orchestrator asked the owner in one message, as C0's last box says, and passed the
answer on. Given by the owner on 2026-11-15 (the owner's local date):

| Approved | For | Where | Needed before |
|---|---|---|---|
| Downloading a pinned Python from GitHub's releases | `uv` installing the Python the pack pins | The owner's Mac and both CI runners | C1b |
| Downloading the runtime's Python packages from `pypi.org` and `files.pythonhosted.org` | The lock file, then `whizbang setup` with the extra | The owner's Mac; never CI | Y1 |
| Downloading the SAM 3 files from Hugging Face, with the owner's token sent to Hugging Face | `whizbang models get sam3`, `whizbang models pin`, and N5's reading of the pinned revision's file listing | The owner's Mac; never CI | N5 |

Given by the owner on 2026-11-16 (the owner's local date), in the owner's word "yes", to
the hosts as C1b named them:

| Approved | For | Where | Needed before |
|---|---|---|---|
| `github.com`, for release downloads under `/astral-sh/python-build-standalone/releases/download/…` (the Python) and `/astral-sh/uv/releases/download/0.12.23/…` (`uv`, on CI), and `release-assets.githubusercontent.com`, where those downloads are redirected | `uv` installing the Python the pack pins; the CI action installing `uv` | Both CI runners (`macos-15`, `ubuntu-22.04`); the owner's Mac only if the pinned Python is ever missing there | C1b |

Given by the owner on 2026-11-16 (the owner's local date), as the orchestrator passed it
on, ahead of the host being known: "when it comes in i am good with the hugging face
stuff. no need to approve". Recorded by N5:

| Approved | For | Where | Needed before |
|---|---|---|---|
| The Hub's own storage host, the one it moves a file download to: named by N5 as `us.aws.cdn.hf.co` (under the Hub's domain `hf.co`), found by `whizbang models pin` refusing the move and printing its origin alone | Downloading the SAM 3 files the Hub moves there; no token is sent to it (ADR 0012) | The owner's Mac; never CI | N5 |

Nothing else. Without a mirror named, `uv` 0.12.23 tries its maker's own host,
`releases.astral.sh`, before GitHub (its source, `crates/uv-python/src/downloads.rs`), and
that host is not approved. So every command that installs a Python names GitHub's releases
as its mirror, by `--mirror` or by the variable `UV_PYTHON_INSTALL_MIRROR`, set to
`https://github.com/astral-sh/python-build-standalone/releases/download`; given so, `uv`
uses that address alone. `just check` refuses a tracked line that installs a Python
without it (`scripts/tests/test-pack.sh`). No package index is contacted for C1b: the pack
has no dependency and `tool.uv.package = false`, and `just test-pack` only finds the
installed Python, offline.

The owner also said, on 2026-11-15:

- Access to `facebook/sam3` is already granted on the owner's account. This is the
  owner's statement; N5 finds out for real, and its fifth credential test says what a
  refusal looks like.
- The token is read from the Hub's command-line sign-in already on the Mac, "for now"
  (D12).
- Whizbang is expected to host the model later, for customers (D11). Not in 1C.

One host is still unnamed, and is named to the owner before its first contact. The
approvals above do not cover a host by a name nobody has read (C1b's two were named and
approved on 2026-11-16, above):

- The host the Hub moves a file download to, its storage: N5 names it to the owner
  before the entry lists it as an origin (open question 2). **Named by N5:**
  `us.aws.cdn.hf.co`, under the owner's approval of 2026-11-16 above. A second package index
  for Linux with CUDA, if a lock needs one, is Y1's to name the same way (open
  question 5).

**Spend: nothing.** Every model call is local, the downloads are free, and no task makes
a paid call. A task that would spend stops and goes to the owner.

**Disk:** about 3.5 GB of weights and the runtime's environment, of about 130 GB free.

## Open questions

Each is settled by the task named, from documentation or by the owner; none blocks C0.

1. Has the owner's account access to `facebook/sam3` today? The owner says it has
   (2026-11-15); N5 finds out for real.
   **Settled by N5 (2026-11-17): yes.** With the token of the owner's sign-in, read by
   whizbang, the Hub served every listed file of the pinned commit, the large one
   included; nothing was refused as a credential's fault.
2. Which host does the Hub move a file download to? The entry's origins must name it.
   A refusal cannot tell: ADR 0019's refusals quote nothing of an address. N5 takes it
   from the Hub's documentation, or `whizbang models pin` prints the origin, and only
   the origin, of a hop it refused; it is added only after the owner approves it by
   name. And the Hub's listing marks the large file as held in its Xet storage (as the
   reviewer of this plan read it; not verified here). Whether a plain request for the
   file's address still serves the whole file is the first thing N5 finds out; if it
   does not, N5 stops, since an installer that speaks another transfer is a change to
   this plan.
   Read for C0b's pull request, 2026-11-15, on the Hub's page of environment variables
   (`https://huggingface.co/docs/huggingface_hub/package_reference/environment_variables`):
   under a deprecated variable it says that "the Hugging Face Hub is fully powered by
   the Xet storage backend" and that "all file transfers go through the `hf-xet` binary
   package"; it also has `HF_HUB_DISABLE_XET`, which turns that package off in the Hub's
   own library. All of that is about the Hub's own client. It says neither that a plain
   request for a file's address is served whole nor that it is refused, so the question
   is live, and it stays the first thing N5 finds out.
   **Settled by N5 (2026-11-17), through whizbang's own client, by `whizbang models
   pin`.** A `GET` of `model.safetensors` at the pinned commit on `huggingface.co`,
   with the token, is moved, and the address it is moved to is at
   `https://us.aws.cdn.hf.co`: a first pin whose draft named the Hub alone refused that
   move and printed that origin and nothing else of the address. With that origin
   added, the same narrow client (HTTP/1.1 only, no range, no chunked body, a
   `Location` of at most 2,048 bytes with no `@`, no `#` and no IPv6 host) followed the
   one move and was served the whole file, 3,439,938,512 bytes, as one body of a stated
   `Content-Length`: no other move, no transfer coding, no range and no protocol of
   Xet's was needed. The small files are served by `huggingface.co` itself. So the
   client needs no change, and the plan none. The host was approved by the owner on
   2026-11-16, ahead of its name ("Origins").
3. Does candidate A run SAM 3 on the Apple GPU with the fallback off? (R1.)
4. Which Python does the runtime's wheel support? The pack pins the newest that it
   does. (Y1; the pack's core runs on any since 3.12.)
   And can `uv sync --locked --offline` check a lock that holds the runtime's extra
   when only the core is installed? Not verified. Y1 finds out before it merges; if it
   cannot, the fast tier's `setup` tests install from the lock as it is, and the match
   of the lock with the pack's own file is held by a test that reads both.
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
9. Where the Hub's command-line tool keeps the token of a sign-in, which variables move
   that place, and which variables keep the Hub's libraries in the worker from reading
   it. Not verified here: nothing was read for it when D12 changed. Each is taken from
   the Hub's documentation of its environment variables, the page "What was looked
   up" already cites: the first two by N4, written in its amendment to ADR 0011, and
   the third by P3a, which sets the worker's environment. If the
   sign-in keeps its token somewhere a file read cannot reach (a system keychain), N4
   stops: the env-file key is then the only way, and the owner is told.
   What that page says, read for C0b's pull request on 2026-11-15
   (`https://huggingface.co/docs/huggingface_hub/package_reference/environment_variables`;
   the page only: no token file was looked at, and no place on a machine):
   - The token is a plain file. `HF_HOME` is where the Hub's library keeps its data,
     "in particular, your token and the cache", and defaults to the folder
     `.cache/huggingface` in the user's home folder.
     `HF_TOKEN_PATH` is where the token is stored and defaults to `$HF_HOME/token`.
   - Three variables move that place: `HF_TOKEN_PATH`; `HF_HOME`; and `XDG_CACHE_HOME`,
     "used only when `HF_HOME` is not set", which makes the home
     `$XDG_CACHE_HOME/huggingface`.
   - `HF_TOKEN`, if set, "will overwrite the token stored on the machine" for the Hub's
     own tools; `HUGGING_FACE_HUB_TOKEN` is its deprecated name and still works. N4's
     reader reads neither (N4, its second box).
   - The page names no keychain. It says the variables are read when the library is
     imported, so one set afterwards is not taken into account.
   - For the third part the page names no variable that stops a library reading the
     token. It has `HF_HUB_DISABLE_IMPLICIT_TOKEN`, under which a token is sent only for
     "write-access" calls, and `HF_HUB_OFFLINE`, under which no call is made to the Hub;
     and the place itself can be pointed at a folder that holds no token, by the
     variables above.
   Still N4's: which of the three variables its reader follows, in its amendment to
   ADR 0011. **Settled by N4** (ADR 0011, "A download's token"): all three, in the
   page's order, `HF_TOKEN_PATH`, then `HF_HOME`, then `XDG_CACHE_HOME`, then the home
   folder; a variable set to a path that is not absolute names no place. The page was
   read again for N4, and the Hub's source of the function that writes the token
   (`utils/_auth.py`), which writes it for its owner alone (600); no token file was
   looked at. Still P3a's: which variables the worker's environment sets. **Settled by
   P3a:** all four places (`HF_TOKEN_PATH`, `HF_HOME`, `XDG_CACHE_HOME` and the home
   folder) are one empty folder of the worker's own, and `HF_TOKEN_PATH` a file in it
   that is not there (P3a, "Built with the task").

## Strands and order

Two strands run side by side after C1a and join at P6a.

| Strand | Tasks | Owns |
|---|---|---|
| Base | C0, C0b, C1a, C1b, then P6a, P6b, P7a, P7b, R1, R2a, R2b, Z1 | Root files; `crates/whizbang/src/`; the record's schema; `.github/`; the justfile |
| Protocol and worker | P1, P2, P3a, P3b, P4, P5b | `crates/whizbang-backends/src/pack/`; `pack/`; the test kit's guard |
| Network and models | N1 to N5, P5a, Y1 | `crates/whizbang-backends/src/net/` and `models/`; the catalogue; `whizbang models`, `backends`, `setup` |

| Task | Size | Needs |
|---|---|---|
| C0 Entry check | S | the tag `m1b` |
| C0b The macOS job inside its budget | S | C0 |
| C1a Rust roots | S | C0 |
| C1b Python in the fast tier | S | C0 |
| P1 The protocol: ADR, codec, vectors | M | C1a |
| P2 The Python worker with fake models | M | P1, C1b |
| P3a The supervisor's process: start, hello, stop; the guard | M | P2 |
| P3b One call, and the conformance table | M | P3a |
| P4 Queue, cancellation, deadline, restart | M | P3b |
| P5a A mark is live only while its remover is; the count of removals | M | C1a |
| P5b Removal against the queue | S | P4, P5a |
| N1 The download client behind the checked downloader | M | C1a |
| N2 A models directory that is its owner's; a lock asked for until a time | S | C1a |
| N3a The catalogue's second version, the fifth reason, where the directory is | S | N2 |
| N3b `whizbang models list`, `get`, `remove` | M | N1, N3a |
| N3c One way for a test to start the real binary | S | C1a |
| N4 A download's credential | M | N3c |
| N5 The shipped catalogue, and SAM 3 installed on the Mac | M | N4 |
| Y1 `whizbang setup`, and the lock file | M | P3a |
| P6a The kind `pack`: a local segmenter through the executor | M | P5b, N3b |
| P6b The installation a call is answered from | S | P6a |
| P7a `run/4`: the device and the cache | S | P6b |
| P7b `whizbang backends status` | S | P7a |
| R1 The runtime, chosen | M | N5, Y1 |
| R2a The adapter `sam3` | S | R1, P6a |
| R2b SAM 3 answers a phrase on the Mac's GPU | M | R2a, P7b |
| Z1 Exit check | S | all |

Twenty-seven tasks, one pull request each.

**What can be built with no approval from the owner:** C0; C0b, since C0 chose an
option that changes no coverage and no budget; C1a, P1, P5a, N1, N2, N3a, N3b, N3c and N4.
These contact nothing new. Everything from P2 on runs the pack's Python and waits for
C1b's origins: P2, P3a, P3b, P4, P5b, P6a, P6b, P7a and P7b. Y1 waits for the package
index as well; N5 for the Hub, the access and the sign-in; R1, R2a and R2b for all of
them. The origins themselves were approved on 2026-11-15 ("What the owner approved,
and what is still asked"); C1b's hosts were named and approved on 2026-11-16; N5 still
waits for the naming of its unnamed host, and Y1 for nothing more unless its lock needs a
second index.

## Commit strategy

- One task is one branch and one pull request, built tests first, reviewed by a reviewer
  that did not write it, merged by the orchestrator with a merge commit once `ci` and
  both scans pass.
- An ADR is in the pull request of the task that names it. Ported behaviour names its
  source in the commit message: the repo, the path, the pinned commit.
- **Until P6a merges the binary registers no kind,** so no config can start a worker.
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
| C1a, C1b, P1, P2 | Revert the later tasks that need it, then the merge. `pack/` and the vectors go together |
| P3a, P3b, P4, P5a, P5b | Revert the merge; the binary never started a worker before P6a |
| N1 to N4 | Revert the merge. A models directory a test wrote is in a temporary folder |
| N5 | Revert the merge: the catalogue is empty again. The installed files stay in the owner's models directory until `whizbang models remove sam3`, or the folder is deleted by the owner |
| Y1 | Revert the merge. The environment `whizbang setup` made is one folder, named in the task's hand-back |
| P6a, P6b, P7a, P7b | Revert the merge: the binary registers no kind. A store that holds a `run/4` record is discarded with P7a; none is kept before the tag |
| R1, R2a, R2b | Revert the merge; the ADR and the evidence record go with it |
| Z1 | The note reverts; the tag stays and a corrected checkpoint is `m1c.1` |

## Tasks

A "Done when" is a list of tests by the name each will have; the name is the clause. A
builder that finds a clause cannot be shown as worded rewords it here in the form
**Reworded with the task**, quoting the old words and the reason.

### C0. Entry check

**Depends on:** the tag `m1b`. **Reads:** `docs/plans/m1b-note.md`, `docs/carry-over.md`,
the job logs of the last twenty runs of `ci` on `main`.

- [x] `main` at the tag: CI green on both runners, `just check` and `just scan` pass,
      and each file, type and function this plan names is there. A line that no longer
      fits is corrected here.
- [x] Every carry-over row that names 1C has the task or the owner that "Carry-over rows
      and their tasks" gives it; the new rows it asks for are added.
- [x] **The macOS job, measured before anything is changed.** From the logs: for each of
      twenty runs, the time of each recipe inside `just ci` (the debug build and lint,
      the tests, the advisories, the release build of the smoke, the example that must
      not compile, the scan), and whether the build cache was restored. Then the
      coverage review section 13 asks for: the ten slowest tests on that runner and what
      each guards.
- [x] One option is chosen from the measurements and written here for C0b:
      (a) the cache: restore it on pull requests from `main`'s, if runs are building
      from nothing; (b) the release build of the smoke and the example that must not
      compile run on the Linux runner only, if the second build is what costs; (c) the
      heaviest crash sweeps move to a tier that runs on `main` and before a tag, if
      test time is what costs, each named with what still guards its behaviour on a
      pull request; (d) the owner changes the budget; (e) the script tests run in a job
      of their own on the macOS runner, beside the job that builds and tests, if the
      script tests are what costs. Options (c) and (d) are put to the owner before C0b;
      (a), (b) and (e) change no coverage on any commit that merges.
      **Reworded with the task:** the box offered "(a) … (b) … (c) … (d)" and ended
      "(a) and (b) change no coverage on any commit that merges". The measurements
      show that what costs is none of the four: it is the script tests, which the box
      did not name. So (e) is added, and it is the one chosen.
- [x] The owner is asked, in one message, for what "Origins, and what needs the owner"
      lists, with the task each is needed before.
      **Reworded with the task:** nothing in the words changed; who did it did. The
      orchestrator asked, since only the orchestrator speaks with the owner, and passed
      the answer on while this task was open: "What the owner approved, and what is
      still asked", under "Origins", holds it.

**Done when** the measurements are in this file under this task, the rows are re-owned,
and `just check` passes. **Size:** small. Documents only.

#### What the check found

`main` is the tag `m1b` and one merge, of this plan (pull request #139), which changed
no other file. At the tag, run 43795459690 of `ci` was green on both runners, and so was run
43803956633 on the plan's merge, where the macOS job took 10 min 6 s, 4 min 36 s of it
the script tests; `just
check` (1,998 tests, 76.6 s of tests on the owner's Mac) and `just scan` pass on the
plan's merge. Every file, type, function, test and ADR section a task names as already
there was looked up and is there, and every path a "Reads" line gives in an old repo is
there at its commit in `docs/sources.toml`. Two things did not fit, and neither changes
what is built:

- P3a's "Reads" line called squint's `minimal.rs` and `full.rs` the tests of
  `crates/squint-tools/src/mcp/mod.rs`. They are, and they are in that folder, as test
  modules of the client (`src/mcp/minimal.rs`, `src/mcp/full.rs`), not under the
  crate's `tests/`. The line now says so.
- Two of the five new rows this plan asked C0 to add were already on the carry-over
  list, under "Evaluations": other runtimes for local models, and holding several
  models with calls side by side. C0 brought those two rows up to date and added no
  second copy. It added the other three, and a fourth the owner's decision on hosting
  asks for (D11).

The thirty rows that named 1C each have their task or their new owner. Six left 1C, as
the table says: the filler's row to milestone 3; installs that share a file to 1D; the
sender and a real prober, a backend that sends with no credential, a recipient compared
with its upstreams, and the encoder for sending, to milestone 2.

#### The macOS job, measured

Read through `gh` from the logs of finished runs of `ci`, nothing pushed to find it
out: the twenty runs on `main` before the plan's merge (commits 5cc7d96 to 75f7dd6, 15
and 16 November in UTC), and the twenty latest finished runs on pull requests. Seconds,
from the time stamps of the log's lines; a recipe ends where the next begins. "Before"
is everything before `just ci` starts: the checkout, the toolchain, the cache and the
tools. "Lint" is the format check, clippy on a restored cache, the crate graph and the
licences. "Test build" is `scripts/timing.sh` up to the first test, and "tests" is the
test time nextest reports, which is what the limit of 180 s is held against.

`check (macos-15)` on `main`, newest first:

| Commit | Cache restored (its step) | Before | Lint | Script tests | Reference check | Test build | Tests | Advisories | Release build of the smoke | Example that must not compile | Scan | Scan of the range (its step) | Canary | The job |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 75f7dd6 | full (6 s) | 22 | 24 | 259 | 16 | 42 | 83.0 | 1 | 26 | 8 | 12 | 14 | 30 | 9:02 |
| c42b854 | full (5 s) | 20 | 23 | 251 | 13 | 40 | 116.9 | 2 | 28 | 10 | 15 | 19 | 31 | 9:31 |
| d725b43 | full (3 s) | 16 | 15 | 233 | 17 | 45 | 112.1 | 2 | 27 | 11 | 14 | 16 | 30 | 9:02 |
| c9ecf9c | partial (4 s) | 16 | 22 | 224 | 13 | 44 | 99.0 | 1 | 29 | 15 | 15 | 23 | 30 | 9:08 |
| 392fe50 | partial (7 s) | 20 | 24 | 269 | 18 | 45 | 118.7 | 1 | 30 | 14 | 13 | 15 | 30 | 10:14 |
| 8025977 | full (5 s) | 22 | 25 | 261 | 18 | 55 | 128.4 | 1 | 31 | 20 | 17 | 18 | 30 | 10:32 |
| 5caa3e2 | full (7 s) | 18 | 15 | 223 | 17 | 42 | 77.5 | 1 | 22 | 11 | 12 | 13 | 30 | 8:06 |
| 7873c2a | full (9 s) | 30 | 42 | 317 | 17 | 55 | 129.5 | 1 | 37 | 19 | 17 | 18 | 30 | 11:56 |
| 6808ee8 | full (8 s) | 33 | 21 | 273 | 17 | 53 | 111.1 | 1 | 29 | 16 | 15 | 17 | 30 | 10:20 |
| 44cfe7b | full (6 s) | 22 | 28 | 298 | 19 | 61 | 128.3 | 2 | 33 | 17 | 16 | 23 | 30 | 11:21 |
| 0181cf6 | full (5 s) | 21 | 24 | 256 | 16 | 55 | 91.2 | 1 | 28 | 13 | 14 | 20 | 31 | 9:34 |
| fd2b650 | full (6 s) | 19 | 24 | 261 | 23 | 59 | 114.5 | 2 | 32 | 15 | 15 | 49 | 30 | 10:48 |
| 1d56085 | full (12 s) | 25 | 13 | 196 | 10 | 29 | 71.0 | 1 | 20 | 10 | 12 | 32 | 31 | 7:35 |
| b1297e3 | full (11 s) | 33 | 23 | 288 | 14 | 61 | 100.2 | 1 | 23 | 13 | 12 | 18 | 30 | 10:21 |
| 3bb7c29 | full (9 s) | 23 | 38 | 275 | 13 | 43 | 118.9 | 1 | 26 | 16 | 14 | 34 | 30 | 10:37 |
| aa4bcb4 | full (8 s) | 24 | 26 | 308 | 21 | 65 | 142.2 | 1 | 36 | 24 | 18 | 55 | 30 | 12:36 |
| 53d53ff | full (7 s) | 19 | 18 | 210 | 17 | 48 | 113.0 | 1 | 34 | 15 | 13 | 14 | 30 | 8:58 |
| fa6ff68 | full (7 s) | 22 | 19 | 316 | 16 | 61 | 117.8 | 1 | 30 | 19 | 16 | 31 | 30 | 11:25 |
| ef29ccc | full (7 s) | 25 | 26 | 272 | 17 | 48 | 120.9 | 1 | 31 | 17 | 16 | 20 | 30 | 10:28 |
| 5cc7d96 | full (5 s) | 20 | 26 | 295 | 16 | 48 | 129.3 | 2 | 25 | 13 | 13 | 21 | 30 | 10:41 |

The same in short, least, median and most, with the pull requests' runs and the Linux
job beside it:

| Part | macOS, `main` (20) | macOS, pull requests (20) | Linux, `main` (20) |
|---|---|---|---|
| Before `just ci` | 16, 22, 33 | 15, 20, 27 | 13, 19, 22 |
| Lint | 13, 24, 42 | 16, 20, 32 | 11, 16, 19 |
| **Script tests** | **196, 265, 317** | **181, 256, 334** | 53, 106, 118 |
| Reference check | 10, 17, 23 | 11, 17, 27 | 6, 9, 10 |
| Test build | 29, 48, 65 | 30, 46, 80 | 24, 34, 43 |
| Tests | 71, 116, 142 | 72, 109, 149 | 41, 68, 89 |
| Advisories | 1, 1, 2 | 1, 1, 3 | 0, 1, 1 |
| Release build of the smoke | 20, 29, 37 | 20, 32, 38 | 16, 22, 26 |
| Example that must not compile | 8, 15, 24 | 8, 14, 23 | 6, 11, 13 |
| Scan | 12, 14, 18 | 11, 15, 19 | 1, 2, 3 |
| Scan of the range | 13, 20, 55 | 13, 18, 71 | 2, 4, 7 |
| Canary (macOS), real timeout (Linux) | 30 | 30 | 30 |
| The job | 7:35, 10:20, 12:36 | 6:57, 9:38, 13:32 | 3:27, 5:17, 6:15 |
| Jobs over 10 minutes | 12 of 20 | 8 of 20 | none |

The Linux column leaves the two runs that failed (commits 6808ee8 and 7873c2a, the fault
`docs/plans/m1b-note.md` describes) out of the parts after the tests, where they stopped.
Two of the pull requests' runs are a second attempt: the first failed on the macOS
runner on test time alone, every test passing, at 180.0 s and at 194.9 s.

What the numbers say:

- **The cache is restored on every run.** All forty macOS jobs restored it, on pull
  requests as on `main`, thirty-six from the exact key and four from an older one, in 3
  to 12 seconds. No run built from nothing: clippy took 12 to 38 seconds. Option (a)
  has nothing to gain.
- **The script tests are the largest part of the job, and nobody had named them.** On
  the macOS runner `scripts/tests/run.sh` takes 3 to 5 and a half minutes, 44 of every
  100 seconds of the job on `main`, more than the test build and the fast tests
  together. On Linux the same tests take a minute and three quarters. Three of them are
  most of it, by median on macOS and on Linux: `test-scan.sh` 83 s and 24 s,
  `test-skills.sh` 60 s and 24 s, `test-thread-guard.sh` 28 s and 14 s; then
  `test-rules.sh` and `test-hooks.sh` 17 s each, `test-syntax.sh` 15 s, and thirteen
  more that share about 50 s. They start thousands of small processes, which that
  runner is slow at.
- **The second build is not what costs.** The release build of the smoke and the example
  that must not compile are 30 to 60 seconds together, 44 at the median. With both off
  the macOS job, six of the twenty runs on `main` and four of the pull requests' twenty
  would still have been over 10 minutes. Option (b) alone does not meet the budget.
- **Test time is not what costs the 10 minutes.** With the test time halved, four runs
  on `main` and two on pull requests would still have been over. Option (c) would give
  up coverage on pull requests and not meet the budget either.
- **With the script tests off the job, every one of the forty is under.** The longest
  would have been 7 min 28 s on `main` and 7 min 58 s on a pull request.

The ten slowest tests on the macOS runner, as the ten that were most often among a
run's ten slowest over the twenty runs on `main`, with the least and the most each
took:

| Test | Seconds | What it guards |
|---|---|---|
| `nested::a_run_calls_as_many_actions_as_its_record_lists_and_one_more_is_refused` | 6.5 to 13.3 | A run calls 256 actions and the 257th is refused. Its time is 514 publications, each writing the whole generation: the carry-over row on what a publication costs, milestone 2's |
| `reserve::a_crash_at_any_event_of_an_admission_through_the_real_ledger_leaves_all_of_it_or_none`, its six parts | 3.7 to 11.3 each | An admission through the real ledger, crashed before every step and with every way a disk loses what was not durable, leaves the whole admission or none of it. The only crash sweep the spend strand has |
| `crashes::a_crash_at_any_event_of_a_repair_leaves_it_as_it_was_not_installed_or_whole`, part 3 of 4 | 4.0 to 11.6 | A repair of a model, crashed at any event, leaves the installation as it was, not installed, or whole |
| `crashes::a_crash_at_any_event_of_an_install_leaves_the_previous_or_the_new_whole`, part 3 of 4 | 3.7 to 11.3 | An install over a previous one, crashed at any event, leaves the previous installation or the new one, whole: "the previous install left in place on failure" of `SPEC.md` section 6 |
| `execute::many_runs_from_two_executors_at_one_instant_are_all_kept` | 4.2 to 7.7 | Two executors that write many runs at one instant lose none: the store's one-writer rule under contention |

None is near the limit of 30 seconds for one test: the slowest single test in the twenty
runs took 15.6 s. Eight of the ten are crash sweeps, which already run two at a time.
Each guards a rule the spec states, and none has a cheaper test that guards the same
thing on a pull request, so the review finds nothing to cut and nothing to move.

#### The option chosen for C0b

**(e): the script tests run in a job of their own on the macOS runner, beside the job
that builds and tests.** Every commit still runs every script test and every recipe of
`just ci` on both runners, so no coverage moves and no budget changes; `just check` on
a developer's machine is unchanged. The job that builds and tests loses 3 to 5 and a
half minutes and comes to 8 minutes at most by these runs. The new job is the script
tests and a checkout, about 6 minutes at most: as far as C0 read them, none of the
script tests builds the workspace, which C0b confirms before it leaves the toolchain or
the cache out of that job. The Linux job is inside its budget and stays one job. The
aggregate `ci` needs the new job too, so that a merge still waits for it.

What (e) does not fix, and C0b does not touch: **the fast tier's test time on the macOS
runner against its limit of 180 seconds.** On unchanged code it was 71 to 149 seconds
in these forty runs, and two more attempts failed at 180.0 and 194.9 seconds with every
test passing. A job with fewer steps before the tests may run them a little steadier,
and may not. Moving a sweep to another tier, (c), or changing the limit, (d), is the
owner's, and C0 recommends neither on these numbers: the tests are not what breaks the
10 minutes, and each slow test guards something. The carry-over row keeps this half
for the owner, and Z1's timing table says what the job and the tests take afterwards.

### C0b. The macOS job inside its budget

**Depends on:** C0.

- [x] The option C0 chose, and nothing else: (e), the script tests in a job of their own
      on the macOS runner ("The option chosen for C0b", under C0). Every script test
      and every recipe of `just ci` still runs on both runners for every commit; `just
      check` is unchanged; the aggregate `ci` needs the new job; `AGENTS.md` and the
      `kickoff` skill name the jobs a builder waits for.

**Done when**, before the merge, five runs of `check (macos-15)` in a row on the branch's
head (the same commit, run again) each take under 10 minutes, and so does the new job
in each, with the test time of each in the hand-back; or the owner's line on the budget
is in `SPEC.md` section 13 and the carry-over row. What the jobs take on `main`
afterwards is Z1's, in the note's timing table. No limit in `scripts/timing.sh` or
`.config/nextest.toml` is raised by this task.
**Reworded with the task** (by C0, which chose the option): the clause read "five runs
of `check (macos-15)` in a row … each take under 10 minutes". The option chosen adds a
job on that runner, and a budget met by one job while the other is over it would not be
met, so the clause now holds both.
**Reworded with the task** (by C0b): the clause reads "five runs of `check (macos-15)`
in a row on the branch's head (the same commit, run again)". The five runs are of
commit d7d3adb, which holds every change this task makes to CI, the justfile, the
script tests and the skills. The head that merges is later, by documents and a merge:
a commit that writes those runs down, here and in the carry-over row, since a commit
cannot hold the times of its own runs; a merge of `main` at 4c2f6c2, which had moved by
two tasks (C1a and N2) and brought eleven fast tests; and a line of N3c's sizing. The
run on that head is in the pull request, a sixth sample and the only one with `main`'s
new tests in it. The times of the five are under "What the runs showed", and in the
hand-back.
**Carry-over:** the row on pull-request CI on the macOS runner, whose half on the 10
minutes this closes; its half on the fast tier's test time stays, for the owner.
**Size:** small.

#### What C0b built

- `check (macos-15)` runs `just ci-without-scripts`: every recipe `just ci` runs, in
  its order, but `test-scripts`. The search canary stays a step of that job.
- A new job, `scripts (macos-15)`, runs `just ci-scripts`: `deny`, then `test-scripts`.
  It runs on every pull request and every push to `main`, as `check` does, and no step
  of it has a condition.
- `check (ubuntu-22.04)` runs `just ci` whole, as before. `just check` and `just ci`
  are as they were, so a developer's machine runs what it ran.
- The aggregate `ci` needs both jobs and tests the result of each on a line of its own.
- `scripts/tests/test-ci-jobs.sh` holds all of it, each rule shown to fail on a copy
  with one thing changed: the two halves are `just ci`, recipe for recipe, so a recipe
  added to `check` or to `ci` and not to the half that runs it on macOS fails `just
  check`; the new job is set up line for line as `check` is; the aggregate needs
  every job; nothing may continue on error; and `AGENTS.md` names the jobs.
  `scripts/tests/test-skills.sh` holds the `kickoff` and `review-merge` skills to the
  four jobs.

**Linux stays one job.** The box says the macOS runner, and the measurements agree:
over C0's twenty runs the Linux job took 3 min 27 s to 6 min 15 s with the script tests
inside it at 53 to 118 seconds. A second job there would take a minute or two off a
job already inside its budget, and cost a second set-up and a second cache.

**The protection of `main` needs no change.** Read through the API on 2026-11-15,
changing nothing: it requires one check, `ci`, and names neither `check` job. `ci`
needs the new job, so a red script test on macOS still stops a merge. The repository
has no rule set beside it.

**The new job contacts nothing the other does not.** Its set-up is the other job's, line
for line: the same checkout, the same toolchain step, the same cache action and the
same pinned tools from the same action, each at the version `check` uses. `just deny`
is the one step that may fetch, the workspace's crates from `crates.io` when the cache
did not hold them, as it does in `check`.

#### What the runs showed

Run 43809732794 of `ci` on this task's pull request (#142), on commit d7d3adb, run five
times: the first run and four reruns of it, one after another, between 05:21 and 05:55
on 16 November in UTC. A job's time is from its start to its end; the script tests' is
from the start of `scripts/tests/run.sh` to its last test; the test time is what
nextest reports, in seconds, which is what the limit of 180 s is held against.

| Attempt | `check (macos-15)` | Its test time | `scripts (macos-15)` | The script tests in it | `check (ubuntu-22.04)` | Its test time | The script tests in it | `ci` |
|---|---|---|---|---|---|---|---|---|
| 1 | 4:18 | 80.3 | 5:42 | 304 s | 5:20 | 66.1 | 92 s | 0:04 |
| 2 | 4:35 | 87.2 | 4:38 | 253 s | 6:12 | 86.8 | 118 s | 0:04 |
| 3 | 5:12 | 106.3 | 5:01 | 276 s | 6:12 | 84.9 | 118 s | 0:02 |
| 4 | 5:38 | 119.1 | 4:36 | 247 s | 3:35 | 39.2 | 58 s | 0:03 |
| 5 | 4:50 | 90.3 | 4:04 | 223 s | 6:15 | 86.7 | 119 s | 0:03 |

Every job was green in every attempt, and none was run again for a failure. All twenty
script tests ran and passed on each runner each time: in `scripts (macos-15)` on
macOS, inside `check (ubuntu-22.04)` on Linux. The fast tier ran its 1,998 tests on
both, as a pull request merges them with `main` at e207510.

Beside C0's tables:

- **`check (macos-15)`** took 6 min 57 s to 13 min 32 s over C0's twenty runs on pull
  requests, 9 min 38 s at the median, and eight of the twenty were over 10 minutes. In
  these five it took 4 min 18 s to 5 min 38 s. C0 expected 8 minutes at most.
- **`scripts (macos-15)`** took 4 min 4 s to 5 min 42 s; C0 expected about 6 at most.
  The longest was the first, the only attempt with no cache of its own: the job was
  new, so none had been saved. `deny` took 3 seconds there and `test-deny.sh` passed,
  so the cache is a convenience, and `deny` before the tests is what that test needs.
- **The script tests take what they took:** 223 to 304 seconds here, 181 to 334 in
  C0's twenty. Nothing was made faster. They run beside the build and the fast tests
  and no longer before them, so what a pull request waits for on that runner is the
  longer of two jobs: 4 min 38 s to 5 min 42 s in these five.
- **Linux is where it was:** 3 min 35 s to 6 min 15 s, with its script tests at 58 to
  119 seconds; C0 had 3 min 27 s to 6 min 15 s, and 53 to 118.

What this shows, and what it does not. Each of the two macOS jobs was under 10 minutes
in each of five runs of one commit, with more than four minutes to spare every time.
That is five samples, of one commit, in about half an hour of one evening, on hosted runners
whose speed moved by half on unchanged code inside those five (the test time on macOS
went from 80 to 119 seconds). It is not a measurement of `main`: what the jobs take
there is Z1's to report, in the note's timing table, and the budget's row stays open
until then on that half.

**The test time on the macOS runner is as it was, and is the owner's.** It was 80.3 to
119.1 seconds in these five, against the limit of 180. None was near the limit, which
says little: C0 saw 71 to 149 on unchanged code, and two attempts that failed at 180.0
and 194.9 with every test passing. This task moved no test, changed no limit, and did
nothing to that time. Whether a job with fewer steps before the tests runs them
steadier, as C0 wondered, five runs cannot say.

#### What each script test needs

C0 read that none of the script tests builds the workspace. That holds, and the job is
still not a checkout alone. Read test by test, at e207510, twenty files with the new
one:

| Script test | What it needs beyond bash, git and the usual tools of a runner |
|---|---|
| `test-deny.sh` | `cargo-deny`; `just` (it runs `just deny` in a copy of the manifests); `rustup` and the pinned toolchain (`cargo metadata`, under `cargo deny`); and the workspace's crates already in cargo's home, because it runs with no way to the network. In `just check` the recipe `deny` runs before the script tests and puts them there. So `ci-scripts` is `deny`, then the tests, and the job has the cache |
| `test-crate-graph.sh` | cargo of the pinned toolchain (`cargo metadata --no-deps`, which fetches nothing); `jq` |
| `test-thread-lint.sh` | clippy of the pinned toolchain, on a fixture crate with no dependency, built into `target/script-tests`: the one thing a script test compiles |
| `test-test-only.sh`, `test-rules.sh` | `just`: one dry run of `just test-only`, and `just scan-range` with planted arguments. Nothing is built or tested |
| `test-baseline.sh`, `test-thread-guard.sh` | `python3`, 3.9 or later, the runner's own |
| `test-docs.sh`, `test-timing.sh`, `test-compile-fail.sh` | `jq`, the runner's own. Where one needs cargo it brings a stand-in |
| `test-scan.sh` | `perl`, the runner's own |
| `test-setup.sh`, `test-hooks.sh` | Nothing more: each brings stand-ins for `just` and cargo |
| `test-timeout.sh` | Nothing more in the script tests, where it reads the config. Its real run, with `cargo-nextest`, is `just test-timeout`, which stays a step of the Linux job |
| `test-ci-jobs.sh`, `test-heavy.sh`, `test-notes.sh`, `test-skills.sh`, `test-syntax.sh`, `test-worktree.sh` | Nothing more |

So the new job needs the toolchain (cargo, clippy, `rustup`), `just`, `cargo-deny` and
the crates in cargo's home. Two things it is given and may not need, so that the two
set-ups stay one text: the repository's whole history, which no script test was found
to read (those that walk commits make a repository of their own), and `cargo-nextest`,
which no script test runs outside an opt-in part and which comes with the one line of
pinned tools.

### C1a. Rust roots

**Depends on:** C0. It contacts nothing and needs no approval.

- [x] The module roots `pack/` and `net/` in `whizbang-backends`, and the subcommands
      `models`, `backends` and `setup` as stubs that say they are not built.
- [x] The folder `pack/conformance/vectors/`, empty but for a line saying what it holds.

**Done when** `just check` passes; each stub says it is not built and exits with an
error; `whizbang describe` and the generated reference are unchanged. **Size:** small.

### C1b. Python in the fast tier

**Depends on:** C0. **Needs the owner:** the origins for a pinned Python, and for `uv`
on the runners, each named in the request.

- [x] `pack/`: a `pyproject.toml` with no dependency and `tool.uv.package = false`, so
      that nothing is built and no build backend is fetched from an index; the pinned
      Python version; an empty package `whizbang_pack` and one `unittest` test.
      Python 3.12.15, in `pack/.python-version` (`requires-python = "==3.12.*"`): inside
      the ranges PyTorch, `transformers` and Meta's `sam3` package document, and already
      installed by `uv` on the Mac. The test:
      `test_package.PackageTest.test_the_package_imports_from_the_pack_and_runs_on_the_pinned_python`,
      run by `just test-pack`.
- [x] `uv` joins `scripts/tool-versions.txt` at the version the Mac has; `just setup`
      checks it. `just test-pack` runs the pack's tests with the pinned Python, offline,
      under the two rows of "Rules" on `uv` and on the pack's tests, and joins `just
      check`; `AGENTS.md` and `SPEC.md` section 11 list it.
      `uv` 0.12.23; `scripts/tests/test-setup.sh` ("uv at its pin", "uv at another
      version", "no uv"). In `scripts/tests/test-pack.sh`: "under the guard" (offline, no
      configuration, the three bounds, the allow-list), "uv does not end" (killed and
      waited for at its limit, 60 s, lowered to 1 for the test), "a test that blocks"
      (the runner's 60 s), "a thread left behind" (daemon threads), "outside the guard"
      and "a guard named by hand". `scripts/tests/test-ci-jobs.sh` holds `test-pack` to
      `check` and to the macOS half of `ci`.
      The row on `uv` is held as far as `just test-pack` starts it: P3a's allow-list does
      not exist yet, so the script hands on PATH, HOME, TMPDIR and
      `UV_PYTHON_INSTALL_DIR` and nothing else; P3a and Y1 hold `whizbang setup` to the
      rest of the row.
- [x] CI: one step installs `uv` at its pinned version, by an action pinned to a commit,
      with its cache. Its time on each runner is in the hand-back.
      `taiki-e/install-action` at commit `ab49d3c` (v2.90.5), already the workflow's,
      with `fallback: none`; then a step installs the pinned Python from GitHub's
      releases only, into a folder that `Swatinem/rust-cache` keeps, keyed by the pin
      (`scripts/tests/test-pack.sh` holds each). On the first CI run of the pull request
      (commit `a5181b8`), with nothing cached: the action, four tools with `uv`, 2 s on
      macOS and 3 s on Linux; the Python step, downloaded, 2 s on macOS and 1 s on Linux;
      test time 76.5 s and 87.9 s. Both under 30 seconds, as "Done when" asks.

**Done when** `just check` passes on both runners with the new step under 30 seconds,
and these pass in `scripts/tests/test-pack.sh`: the runner of the pack's tests ends
itself at its limit, shown with a test that blocks; `just test-pack` refuses to run
outside `scripts/thread-guard.sh`; `uv` is started offline, with no user's configuration
and the three bounds. **Size:** small.

### P1. The protocol: its ADR, the Rust codec, the vectors

**Depends on:** C1a. **Reads:** `SPEC.md` section 4; ADR 0006, "Segment" and "Faults";
ADR 0002 for a plane's layout; rascal `crates/rascal-mcp/src/client/mod.rs` (the size
bound on an answer) and `crates/rascal-mask/src/filler/mod.rs` (`answer_cap`); squint
`crates/squint-tools/src/mcp/mod.rs`.

- [x] ADR 0020, the pack protocol: D2 in bytes. The hello; the frame; each message and
      its fields; a payload's entry; every bound and how it is computed from a request;
      what a refusal says; the HTTP codec on paper; what `pack/2` would mean.
- [x] `whizbang_backends::pack::proto`: frames written and read, strictly. Segment's
      request and response in it: an image, the queries, the most instances; instances
      with a plane, a box and a score. The settings rascal's adapters took (a threshold,
      a padding, an outline rule, a least cover) are a model's settings in the config,
      sent in `load` and not fields of a request; an instance's predicted IoU is a
      typed field of its entry, and its other diagnostics go in the result's
      diagnostics; the order of an answer is fixed here: best score first, ties
      by the query, then by the box's top left.
- [x] The amendment to ADR 0006 of D2: `SegmentRequest` carries its picture's binding,
      and the handle refuses an answer whose planes name another.
- [x] `pack/conformance/vectors/`: each message as bytes, and each refusal.

**Done when** these pass, in `crates/whizbang-backends/tests/it/pack/proto.rs`, and the
last in the core's `tests/it/capability/segment.rs`:

- `proto::every_vector_parses_to_what_it_states_and_is_written_back_byte_for_byte`
- `proto::a_hello_of_another_version_is_refused_and_says_both_versions`
- `proto::a_header_is_strict_json_in_strict_utf8_and_each_edge_of_it_has_one_reason`
- `proto::a_header_or_a_payload_over_its_bound_is_refused_before_it_is_read`
- `proto::a_results_header_is_bounded_from_its_request_and_holds_the_most_instances_adr_0006_allows`
- `proto::a_load_carries_at_most_32_settings_and_one_more_is_refused`
- `proto::a_result_over_one_gibibyte_of_payload_is_refused_whatever_its_request_would_allow`
- `proto::an_unknown_field_a_missing_field_and_an_unknown_message_are_each_refused`
- `proto::a_payload_states_its_dimensions_layout_type_colour_alpha_and_binding_and_one_that_disagrees_with_its_length_is_refused`
- `proto::two_requests_of_the_same_pixels_under_two_bindings_are_two_requests_and_each_answer_names_its_own`
- `proto::the_257th_event_of_a_request_and_diagnostics_past_their_bound_are_refused_without_being_kept`
- `proto::no_message_has_a_field_that_holds_a_path`
- `proto::an_answer_is_in_the_fixed_order_whatever_order_it_was_built_in`
- `proto::a_frame_cut_at_any_byte_is_an_error_and_never_a_shorter_message`
- `segment::an_answer_whose_planes_name_another_binding_is_refused_as_malformed`

**Carry-over:** what a segmenter adapter took and gave (settings, diagnostics, order);
the bound on what is read off a wire. **Size:** medium.
**Built with the task:** C1a's test of the vectors folder, which held it to one file of
one line, is now `pack::the_vectors_folder_holds_its_readme_and_one_file_for_each_vector_and_nothing_else`.
The hello and each result carry the worker's device and its count of threads from the
start, since `pack/1` is not edited later: "Rules" (the count in `hello` and after each
call) and P7a (the device in `hello` and in each result) use those fields and add none.
A setting's text may hold no `/` and no `\` on the wire, which the config allows, so
P6a refuses such a setting where the config is read. Both carry-over rows keep a part:
what a run keeps of a result's diagnostics and predicted IoU is P3b's, with the
supervisor that first reads a worker through the codec.
Notes for P2, from what the Rust codec decides (ADR 0020, "What a reader takes as JSON"):

- Decode the header as strict UTF-8 before parsing it: a lone surrogate, encoded or
  escaped, is `not_json`.
- Refuse NaN, the infinities, a number outside a double's range, duplicate keys, JSON
  nested 128 deep and a byte-order mark explicitly, as `not_json`: Python's `json`
  accepts each.
- `True` passes a Python check for an `int`, and `-0` parses as the int 0: refuse both
  where a whole number is asked (`bad_value`).
- The worker's writer sorts instances into the fixed order, whatever order an adapter
  gives them in.
- It cleans an adapter's texts (diagnostics, events, a fault's sentence) to the
  characters a fault's sentence may hold before it writes them.
- It cuts diagnostics at about 63 KiB, so a result never meets its header's bound.

### P2. The Python worker, with fake models

**Depends on:** P1, C1b. **Reads:** ADR 0020; rascal `tools/python/src/rascal_tools/server.py`
and `models/adapters.py` (the adapter's interface and its fakes, as reference).

- [x] `whizbang_pack.proto`: the same frames, standard library only.
- [x] `whizbang_pack.worker`: reads frames on one thread, runs one call at a time on
      another, answers `cancel` by setting the call's flag, and exits at the end of
      its input, whatever the call is doing, and at `--max-lifetime-s`. One resident model: a `load` of another unloads
      the first. It is given the models directory as an argument and opens an
      installation by the model's name and the identity a `load` names.
- [x] The adapter interface (`load`, `segment`, `unload`, the features it declares, its
      revision), the six fake models of D3, and the entry point `whizbang_pack.fakes`
      with its faults.

**Done when** these pass under `just test-pack`, in `pack/tests/`:

- `test_proto.EveryVector.test_parses_and_is_written_back_byte_for_byte`
- `test_proto.Refusals.test_each_refusal_vector_is_refused_for_its_stated_reason`
- `test_worker.Hello.test_another_version_is_refused_and_the_worker_exits`
- `test_worker.Calls.test_one_call_runs_at_a_time_and_a_second_model_unloads_the_first`
- `test_worker.Cancel.test_a_cancel_ends_a_waiting_call_as_cancelled`
- `test_worker.Ends.test_the_worker_exits_at_the_end_of_its_input_and_at_its_lifetime`
- `test_worker.Ends.test_the_worker_exits_at_the_end_of_its_input_inside_a_call_that_ignores_cancel`
- `test_worker.Fakes.test_the_worker_entry_point_refuses_a_fault_and_the_fakes_entry_point_takes_each`
- `test_worker.Imports.test_the_core_imports_nothing_outside_the_standard_library`
- `test_worker.Threads.test_the_worker_holds_three_threads_with_a_fake_model`

The tests drive the worker in-process over pipes, and start one child process at a
time, four times in all, each killed and waited for in the test's cleanup.
**Reworded with the task** (from P2's review): "three times in all" is now "four times
in all". The review asked that the `python -m` stanza of each entry point be run in a
test. The three children of `test_worker.Ends` now start `whizbang_pack.worker` as a
program, and one more,
`test_worker.Fakes.test_the_fakes_entry_point_runs_as_a_program_with_its_fault`, starts
`whizbang_pack.fakes` with a fault.
**Size:** medium.
**Built with the task:** the codec was written from ADR 0020 and the vectors alone, not
from the Rust codec. Each of P1's notes has a test of its own in `test_proto.StrictJson`
and `test_proto.Writing` (strict UTF-8 before `json`; a byte-order mark, NaN, the
infinities, numbers past a double, whole numbers past 4,300 digits, a field twice,
nesting 128 deep, `-0`, `true` and `false` as whole numbers; fractions as the bits of
their double; the writer's total order, its cleaning and its cut of diagnostics at
63 KiB). `test_worker.Threads` counts the worker's own threads in this process (its
reader and its call, beside the thread that started it, and none started by a call);
the count of three a process reports is checked in a child, by the first test of
`test_worker.Ends`. Where the two readers first named different reasons for one frame
that breaks two rules, the Python reader now names the Rust reader's: the Rust codec,
its bytes and its reasons did not change. ADR 0020 was amended on 2026-11-17 to say the
Rust order where it was silent or said otherwise ("The steps in full"). Ten refusal
vectors hold it, each reason the Rust reader's, confirmed by its own vector test
(`test_proto.Refusals.test_a_frame_that_breaks_rules_of_two_steps_is_named_as_whizbangs_reader_names_it`).
The 62 vectors of P1 are unchanged. ADR 0006 cleans a sentence by putting `?` for a
character it may not hold, and `?` is no character of a text of the protocol (ADR 0020,
as amended), so the writer puts a space, then drops white space at the ends again; a
text that is then empty is `no reason was given` for a sentence and `no text was given`
for an event. The reader counts nesting in one pass over the header, linear in its
length (`test_proto.StrictJson.test_the_nesting_is_counted_in_one_pass_whatever_the_header_holds`).
The worker ends at its lifetime and at the end of its input without waiting on its
error output (`test_worker.Ends.test_the_lifetime_ends_the_worker_though_its_error_output_is_blocked`).
**Notes for P3a:** the command line is
`python -m whizbang_pack.worker --models-dir <absolute folder> [--max-lifetime-s <whole seconds>]`,
and `python -m whizbang_pack.fakes` takes the same and `--fake-fault <name>`; neither is
installed (`tool.uv.package = false`), so the supervisor puts `pack/src` on the path the
way it starts Python (the tests use `-I -B -c` with the path as an argument, and run each
module as a program). Exit codes:
0 at the end of the input or at whizbang's error that names the conversation, 2 at a
refused frame or an output that cannot be written (and at a command line refused before
anything starts), 3 at the lifetime, 4 from `fake-exit`. The worker answers a refused
frame with an `error` of the conversation's id (or, before a hello, the refused header's
own id when it has a valid one): `too_large` for `header_too_large`, `over_bound` and
`too_large`, `malformed` for every other reason. A request that comes while a call
runs is answered `unavailable`. The worker moves its standard output to a copy before
anything runs and points the standard output at its error output, so that a library
that prints cannot write into the frames. It reads no variable of its environment;
what the supervisor sets for the Hub's libraries is P3a's. `error-output` writes 300
lines, every tenth over 5,000 bytes, and nothing in them is a secret: what the fault
prints for the redaction tests of P3a and P3b (a marker, a host, a made-up token) is
theirs to add, since P2 names none. The supervisor reads the worker's error output
without stopping until it closes, and drops what is past its bound (256 lines of 4,096
bytes) rather than stopping to read. Otherwise a pipe that fills blocks any thread of the
worker that prints, and a call stalls until its deadline. The worker does not bound what a
library prints, and it never waits on its error output when it ends.
**Notes for P3a and P3b:** a text of the protocol that the Rust side makes from words it
was handed (a fault's sentence it sends, diagnostics) is cleaned by a cleaner of the
protocol: a space for a character the text may not hold, as the Python writer does. It
is never cleaned by the core's `fault::clean`, which puts `?`, a character the reader
refuses (ADR 0020, "A text of the protocol", as amended).

### P3a. The supervisor's process: start, hello, stop; the guard

**Depends on:** P2. **Reads:** `crates/whizbang-core/src/store/fs.rs`, "What these locks
are, and are not"; the carry-over row on the store's locks; squint
`crates/squint-tools/src/mcp/mod.rs` and its test modules beside it, `minimal.rs` and
`full.rs` (the client that survives, D1: what is ported and what is not is in the commit message);
rascal `crates/rascal-mcp/src/client/env.rs` (the environment's allow-list).

- [x] `whizbang_backends::pack::Worker`: started from the launch it is given, with a
      cleared environment and an allow-list (no variable whose name ends in `_KEY`,
      `_TOKEN` or `_SECRET`, and none the config names as a key), the thread limits and
      the offline variables of D11 set, and with them the variables that keep the
      Hub's libraries from reading the token of a sign-in (D12; open question 9); a hello each way; stopped as D7 says. Its error
      output is read by line, 256 lines of 4,096 bytes kept, redacted.
      **Reworded with the task:** "started from a command line it is given" is now
      "started from the launch it is given". The pack is not installed, so the
      supervisor makes the command line itself from a `Launch` (the interpreter by its
      absolute path, the pack's source, the entry point, the models directory and the
      lifetime), so that `--max-lifetime-s` is always passed and the interpreter is
      never looked for on `PATH`; only a test's fault is added to it.
- [x] `whizbang_testkit::worker`: the guard of "Rules", and the pack with fake models
      and fake faults as a fixture.

**Done when** these pass, in `crates/whizbang-backends/tests/it/pack/worker.rs`:

- `worker::a_worker_is_started_at_the_first_request_and_not_before`
- `worker::a_worker_that_says_no_hello_within_its_limit_is_killed_and_the_request_ends_as_unavailable`
- `worker::a_worker_of_another_protocol_version_is_refused_at_the_hello_and_is_not_left_running`
- `worker::the_worker_is_handed_no_descriptor_but_its_three_and_none_of_the_store`
- `worker::the_workers_environment_holds_the_allow_list_and_no_key`
- `worker::the_workers_environment_points_the_hubs_libraries_away_from_the_sign_ins_store`
- `worker::a_dropped_supervisor_closes_the_input_and_the_worker_is_gone_when_the_drop_returns`
- `worker::a_worker_that_does_not_end_at_the_end_of_its_input_is_killed_at_the_deadline_and_waited_for`
- `worker::the_error_output_kept_is_bounded_and_a_secret_in_it_is_redacted`
- `worker::the_guard_kills_and_waits_for_its_worker_when_its_test_panics`

**Carry-over:** the child's copy of the store's lock (closed by the fourth and sixth
tests and the line of D7). **Size:** medium.
**Built with the task:**

- **How the interpreter is found.** The supervisor is given it, as an absolute path, and
  never looks on `PATH`: one that is not an absolute path to a file is refused before
  anything starts, `unavailable`, "the pack's Python is not installed; run whizbang
  setup". The binary starts no worker until P6a; Y1 makes the environment the kind
  then takes its interpreter from. In tests the fixture finds the pinned Python as
  `scripts/test-pack.sh` does: `uv python find`, offline, with no user's configuration
  and downloads of a Python off, in a cleared environment, at most 60 seconds, killed
  and waited for past them, once in a test process.
  **Reworded with the task** (by P3a), in the row of "Rules" on `uv`: "started only by
  `whizbang setup` and `just test-pack`" now names the test kit's fixture too, which
  starts `uv python find` and nothing else; and "one at a time" is at most two, the
  group `pack-workers`. The row named neither, and the fixture needs the one Python the
  pack pins without taking one from `PATH`.
- **The command line** is `<python> -I -B -c <boot> <source> <module> --models-dir
  <folder> --max-lifetime-s <seconds>`, then a test's `--fake-fault`. The boot
  (`pack::worker::BOOT`) closes every descriptor above the three, which a descriptor
  that whoever started whizbang handed it without closing at a program's start would
  otherwise reach; every file whizbang opens is closed there already, and nothing runs
  between the start of the child and the start of its program.
- **The environment** (open question 9, P3a's part): cleared; `LANG`, `LC_ALL` and
  `LC_CTYPE` of the parent's, but for a key's name; then `HOME`, `TMPDIR`,
  `XDG_CACHE_HOME` and `HF_HOME` set to one empty folder the supervisor makes for each
  worker (its owner's only, in the system's temporary folder) and removes once the
  worker has been waited for, and `HF_TOKEN_PATH` to a file in it that is not there; so
  no library in the worker finds the sign-in's store by a home, a cache or the Hub's
  own place. `HF_HUB_OFFLINE`, `HF_HUB_DISABLE_TELEMETRY`,
  `HF_HUB_DISABLE_IMPLICIT_TOKEN` and `DO_NOT_TRACK` are `1`; `PATH` is `/usr/bin:/bin`;
  the pools are held at 8 (`OMP_NUM_THREADS`, `MKL_NUM_THREADS`,
  `OPENBLAS_NUM_THREADS`, `VECLIB_MAXIMUM_THREADS`) and `TOKENIZERS_PARALLELISM` is
  `false`; the watermark of "Rules" starts at 0.5, for R1 to change. No token, home
  folder, proxy or Python variable of the parent's passes.
- **Exit codes** name how a worker that ended before its hello ended, in its fault: 0,
  2 and 3 each shown (`worker::a_worker_that_ends_before_its_hello_is_named_by_its_exit_code`,
  and the dropped supervisor's 0); 4 is from inside a call, which is P4's to show.
- **Threads:** two for each worker, made by a spawner of the supervisor's own
  (`pack::threads`, a process holds two at most: one worker), the third place in the
  workspace allowed to make a thread (`clippy.toml`, ADR 0008 as amended,
  `scripts/tests/test-thread-lint.sh`). A test hands the supervisor the test kit's,
  so its threads count toward the test's 64 (the dropped supervisor's test counts
  them). Both readers are joined on every path: each ends at the end of its pipe,
  which the end of the process closes. A process the worker started that outlives it
  holds the pipes open: the readers are given 5 s (`END_LIMIT`, on the clock) and let
  go past it, still holding their places, no worker is started until they end (a
  fault that says so, at once), and the kept error output says so last
  (`worker::a_process_the_worker_started_that_holds_its_streams_open_keeps_neither_stop_nor_drop_waiting`,
  with the fault `grandchild` of the fakes, whose process lives two seconds). Such a
  process is not killed: killing the worker's group of processes takes `libc` or
  `unsafe`, which the workspace forbids, or the system's `kill` program, which the
  builder's permission to run was refused; the carry-over row on processes a worker
  starts keeps it, and R1's "Done when" asks about the runtime's.
- **Every path out of a start ends the worker,** a panic among them: from the moment
  its process is started it is held by a value whose drop kills it, waits for it, lets
  its readers end within 5 s of the machine's time and removes its folder
  (`worker::a_panic_between_a_workers_start_and_its_end_leaves_no_process_no_child_unwaited_and_no_folder`,
  with a clock whose pause panics). The kill on the drop's path is
  `worker::a_dropped_supervisor_kills_a_worker_deaf_to_the_end_of_its_input_at_the_deadline`.
- **The boot** imports what it uses of the standard library before the pack's source
  is on the path, so the pack cannot stand in for it, and closes the descriptors above
  the three up to the process's limit of open files (a million at most).
- **The guard:** `Fixture::guard` holds the supervisor a test makes from the fixture
  (the test kit does not depend on `whizbang-backends`); its drop kills the worker,
  waits for it, and fails the test if `ps` lists a child of the test's process that
  runs the pack. The tests that start one are the group `pack-workers`, two at a time.
  "`just check` refuses a test that starts the pack another way" is
  `worker::no_test_starts_the_pack_but_through_the_test_kits_guard`, a fast test that
  reads the crates' sources.
- **The fakes:** `error-output` now also writes its environment and its descriptors,
  a line each, and last a marker, a host and a made-up token: the fourth, fifth, sixth
  and ninth tests read them, and P3b's test of a fault reads the last three.
- **Not built here:** the cleaner of a text of the protocol. P3a writes none (its hello
  holds no text, and its faults are whizbang's, not the protocol's); P3b builds it.
- Five tests more than "Done when" names:
  `worker::a_python_that_is_not_there_or_not_absolute_is_refused_and_says_to_run_setup`,
  the one on exit codes, and the three above on a process the worker starts, a panic
  and the drop's kill; and a unit test of the spawner
  (`pack::threads::tests::two_places_are_taken_the_third_is_refused_at_once_and_a_place_comes_back_after_a_return_or_a_panic`).

**Notes for P3b and P4:** the output's reader hands each frame on a channel and ends
at the first refusal or the end; it reads with an `Open` of its own, which P3b
replaces with the one the writer notes requests in. That channel is not bounded: a
worker that writes frames faster than they are taken grows it, so P3b bounds it (a
channel of a fixed size, the reader waiting on it). The error output is complete only
once the worker has ended (`Worker::error_output` after `stop` or `kill`): the reader
may still be behind when a frame arrives. `Worker::folder` is the worker's folder while
it runs.

**Notes for P6a and Y1:** `Launch::source`, the pack's source the worker runs, is
taken from the installation `setup` made, never from a path compiled into the binary
(the test kit's fixture takes the repository's, which no shipped binary has).

### P3b. One call, and the conformance table

**Depends on:** P3a.

- [x] One request and its result through the supervisor; a worker's failure made into a
      fault; the error output handed on with the failure, for a run's diagnostics.
      **Reworded with the task:** "the error output handed to a run's diagnostics" is
      now "the error output handed on with the failure, for a run's diagnostics". No
      backend can reach its run's diagnostics: a `Segmenter` is handed a request and
      its bounds, and answers a `SegmentResponse`, which holds instances and a charge
      (ADR 0006); only an action writes a run's diagnostics. So the supervisor hands
      the error output, redacted and whole, to its caller with each failure
      (`Failed::error_output`), with the worker's own sentence and a result's
      diagnostics and events beside it, and the way from a backend to its run's
      diagnostics is a row of `docs/carry-over.md` (1D, with the `segment` action).
- [x] The behavioural half of the conformance suite, as a table of cases over a
      transport, with the cases this task can show.

**Done when** these pass, in `crates/whizbang-backends/tests/it/pack/`:

- `conformance::the_real_pack_with_a_fake_model_answers_a_segment_request_whose_masks_are_those_the_request_determines`
- `conformance::a_result_over_the_bound_its_request_gives_is_refused_before_it_is_built_and_the_call_ends_as_too_large`
- `conformance::a_malformed_payload_ends_the_call_as_malformed_and_the_worker_is_restarted`
- `conformance::every_case_of_the_table_names_the_fake_model_or_the_fake_fault_that_produces_it`
- `worker::a_fault_made_from_a_workers_failure_holds_none_of_the_marker_the_host_and_the_token_its_error_output_held`
- `worker::a_request_that_could_not_be_written_is_not_sent_and_one_written_and_unanswered_is_unresolved`

**Carry-over:** a fault's sentence; `not_sent` only when no byte left. **Size:** medium.
**Notes from P1** for the supervisor, which the codec leaves to it:

- Refuse a `request` frame from a worker before its payload is read: the codec reads
  one whole (up to 200 MB of pixels) before the supervisor answers it `unsupported`.
- A request the codec refuses to write (`header_too_large`, a batch of phrases full of
  characters written as two or six) is a fault before anything is sent.

**Built with the task:**

- **One call.** `Worker::call` takes a request (`proto::Ask`) and hands back its result
  (`Called`: the device, the threads, the result's diagnostics, the text of each event,
  and the answer as the wire held it, each instance's predicted IoU in it) or its
  failure (`Failed`: the fault; the worker's own sentence when it ended the request
  with an `error`; how the worker ended, when the failure ended it; the error output as
  kept). Ids are `r` and a count. Each kind has a limit on the clock, past which the
  worker is killed and the call is `timed_out`: 120 s for a `load`, 30 s for an
  `unload`, 120 s for a `segment` (the kind's limit; a call's own deadline and
  cancellation are P4's, which tests these limits). The worker is started at the first
  call; a failure that ends it leaves the next call to start another.
- **How far a request got** (the carry-over row on `not_sent`): a request the codec
  would refuse is a fault before a worker is started or told anything, `not_sent`,
  `too_large` for `header_too_large`, `over_bound` or `too_large` and `rejected`
  otherwise (`proto::writable`, the writer's checks without a byte written). A request
  is written through a count of the bytes the pipe took: none taken is `not_sent`, any
  is `unresolved` unless an answer settles it. A worker's `error` for the request is
  `answered`. Every fault whose request was written and not answered (a refused frame,
  the end of the worker, a time limit) is `unresolved`.
- **A fault's sentence** (the carry-over row on an adapter's keeping of the rule): fixed
  words and typed fields (a fault's name, a refusal's reason, an exit code, a limit in
  seconds), never a word the worker wrote. The worker's own sentence is handed on as
  `Failed::said`, a text of the protocol, never as the fault's
  (`worker::a_fault_made_from_a_workers_failure_holds_none_of_the_marker_the_host_and_the_token_its_error_output_held`).
- **What a refusal becomes** (ADR 0020, "What a refusal says", amended): `too_large`
  for `header_too_large`, `too_large`, and an `over_bound` of a count of a result's
  instances, for one query or in all (`Refused::counts_instances`); `malformed` for
  every other reason, an `over_bound` of events or text among them. A worker whose
  frame was refused is sent an `error` with the conversation's id, its sentence the
  refusal's cleaned as a text of the protocol, and is given 5 s to end before it is
  killed; so is one that answered its hello with something else, or with another
  version (P3a's hello now tells it so).
- **The protocol's cleaner** (P2's note): `proto::clean` and `proto::sentence` put a
  space for a character a text of the protocol may not hold, drop white space at the
  ends, cut, and never put a `?`; an empty sentence is `no reason was given`. Every
  text whizbang writes to a worker goes through it
  (`proto::the_protocols_cleaner_puts_a_space_for_what_a_text_may_not_hold_and_never_a_question_mark`,
  and through the real pack
  `worker::a_text_whizbang_sends_is_cleaned_as_a_text_of_the_protocol_and_the_worker_takes_it`,
  whose worker reads the refusal of its hello as a frame of `pack/1` and ends at it).
- **A request from a worker** (P1's note) is refused before a byte of its payload is
  read: `proto::hear` reads as `proto::read` does, but hands a request back by its id
  once its header is held to every rule, and reads nothing more
  (`proto::a_request_heard_by_a_side_asked_nothing_is_handed_back_by_its_id_before_its_payload_is_read`).
  The supervisor answers it `unsupported`, as ADR 0020 says, and, since the input is no
  longer at a frame's start, ends the conversation; the call is `malformed`
  (`worker::a_request_from_the_worker_is_refused_before_its_payload_is_read_and_answered_as_unsupported`,
  with a new fault of the fakes, `asks`, which sends a request's header and its
  payload's length and never its samples, so that a reader that went on would wait).
- **The frames' channel is bounded** (P3a's note), decided here: a channel of 16 places
  (`FRAMES_WAITING`); the reader waits while it is full and reads nothing more. So a
  worker that writes frames faster than they are taken holds this process to 16 frames,
  each a header of 64 KiB at most but for the one result of the one call, which its
  request bounds. When the worker is killed the channel is let go, so a reader waiting
  on it ends. The requests whizbang writes reach the reader as notes, sent before each
  request is written and taken once the first byte of the next frame is there
  (`pack::worker::tests::the_reader_of_the_output_reads_no_further_ahead_than_the_frames_that_may_wait`).
- **The error output handed on is whole** (P3a's note): a failure that ends the worker
  takes the error output after the worker's end, which waits for its reader within 5 s;
  the fault `error-output` now writes its marker, host and token again at the start of
  each call; before a segment of `fake-exit` it first closes its frames' output, and
  writes them half a second later, the end of its input not ending it meanwhile, just
  before `fake-exit` ends the process. The failure holds them, the token redacted, which
  a supervisor that took the error output when the frames ended would not (a mutant
  that did so failed the test). No fault's sentence is made from the error output.
- **What a run keeps of a result's predicted IoU and diagnostics** (the carry-over row
  P1 gave this task), decided: the core's `Instance` and `SegmentResponse` stay as they
  are, so what an action is answered with does not depend on which backend answered.
  Both are diagnostics, and the supervisor hands them on with each call (`Called`);
  a run keeps them, and the error output, once a backend can hand diagnostics to its
  run, which no backend can today. That way, one for every kind, is the `segment`
  action's at 1D: `docs/carry-over.md` keeps the row, re-owned.
- **The table** (`conformance::TABLE`) names each case, what produces it (a fake model,
  a fake fault, or both), the task that shows it and its test. Every fake model and
  every fake fault of the pack produces a case of it, the names read from the pack's
  own source; every test it names is named in this plan, and the tests of P3a and P3b
  are there. P4's cases name P4's tests. The transport is a trait of the test
  (`Transport`), carried by the supervisor here and by the HTTP codec at milestone 6.
  The tests of `pack::conformance::` start workers, and join the group `pack-workers`
  (`.config/nextest.toml`).
- **Every write to the worker is on the clock** (from the review of this pull request,
  which found a request written on the caller's thread waiting for a worker that stopped
  reading until that worker's lifetime ended, never `timed_out`). A third thread of each
  worker writes its input (`write_input`, made through `Spawn`; `pack::threads` now
  holds a process to three, and "Rules", "Threads in whizbang" is reworded). The
  supervisor hands it each frame on a channel of 4 places and waits on the clock: for a
  request within the call's limit, after which the worker is killed, the write fails and
  the writer ends, the call `timed_out` (`unresolved` when the pipe took a byte,
  `not_sent` when it took none); for every other frame (the hello, the `error` that ends
  a conversation, the answer to a worker's request) within 5 s, after which the
  worker's end follows as it would. Closing the input is letting go of that channel: the
  writer writes what waits and ends, and its end closes the pipe. Two new faults of the
  fakes, `deaf-after-hello` (reads nothing after its hello) and `garbage-after-hello`
  (the same, and writes a frame whizbang refuses):
  `worker::a_request_the_worker_does_not_read_ends_at_the_calls_limit_as_timed_out_and_leaves_nothing_behind`
  and `worker::a_word_to_a_worker_whose_input_is_full_waits_no_longer_than_its_limit`,
  each with a request of 4 MB, on a clock that rushes through the limits, ending within
  10 s of the machine's time with no process, reader or writer left.
- **What `{:?}` prints** of `Called` and `Failed` is no word the worker wrote: counts,
  the fault, how the worker ended, and whether the worker gave a sentence (held in the
  fault test above).
- Eight tests more than "Done when" names, each named above: two of the cleaner, two of
  a request from a worker, two of a write the worker does not take, the unit test of the
  reader, and
  `proto::a_refusal_of_a_count_of_a_results_instances_says_so_and_no_other_does`, which
  also holds the count of 4,097 instances in all to `too_large`.

**Notes for P4:** `Worker::call` waits on one call with `until`, at the limits above;
the deadline of a call, `cancel`, the queue and a restart's fault are P4's to add
around it. A refusal of a flood of events already ends the call as `malformed` and the
worker with it (an `over_bound` that counts no instances). The case table names P4's
tests for `fake-wait`, `fake-deaf`, `flood`, `slow-load`, `slow-unload` and the exit
inside a call; each joins `TABLE` as shown once P4 builds it.

### P4. Queue, cancellation, deadline, restart

**Depends on:** P3b. **Reads:** `SPEC.md` section 4, "The worker"; ADR 0008,
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

**Depends on:** C1a. **Reads:** ADR 0017, "Removal", "Holds" and "Three kinds of lock";
`models/remove.rs`, `models/hold.rs`.

- [x] D6 and the count of D5, by an amendment to ADR 0017: the remover's lock and its
      place in the order of locks; `models::draining` as the one question, a try for a
      share; the count, where it is written and how it is read; a hold returns the
      count it read.
- [x] 1B's tests that plant a mark with no remover are rewritten: such a mark is now a
      dead remover's. Each is named in the pull request with what it shows now.

**Done when** these pass (`tests/it/models/remove.rs`):

- `remove::a_hold_that_meets_the_mark_of_a_remover_that_was_killed_is_given`
- `remove::a_hold_that_meets_the_mark_of_a_live_remover_is_refused_as_draining`
- `remove::a_publication_an_installs_own_look_and_a_listing_take_the_mark_of_a_dead_remover_as_no_mark`
- `remove::two_that_ask_at_once_whether_a_dead_removers_mark_is_live_both_answer_no`
- `holds::a_hold_returns_the_count_of_removals_it_read_under_the_directorys_lock`
- `remove::each_removal_that_sets_its_mark_raises_the_count_and_a_removal_that_timed_out_leaves_it_raised`
- `remove::a_crash_at_any_event_of_a_removal_leaves_the_model_installed_or_not_installed`
  (1B's sweep, still passing with the new lock and the count)

**Carry-over:** the mark left by a remover that died; a removal that takes no
cancellation (a line: the patience is enough, since a dead remover no longer strands
the model). **Size:** medium.

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

**Depends on:** C1a. **Reads:** ADR 0012, "Three interfaces"; ADR 0019, "Every hop", "A
download and a probe", "For later tasks"; `egress/ways.rs`, `chains.rs`, `hops.rs`.

- [x] One blocking HTTP client over TLS in `whizbang_backends::net`, by two crates the
      owner approved on 2026-11-16, `rustls` (pinned `=0.23.46`, default features off,
      `ring`, `std` and `tls12` on) and `webpki-roots` (pinned `=1.0.12`), with
      HTTP/1.1 written in `whizbang_backends::net` itself (ADR 0024); `just deny`
      accepts every crate they bring, and `deny.toml` is not changed. It implements
      `Downloader`, the one way out 1C uses; a real `Sender` and a real `Prober` wait for the first thing that sends (milestone 2), since a
      sender shaped with no provider to send to would be a guess. It follows nothing:
      an answer that points elsewhere comes back as `Moved`. A download is streamed to
      its sink and stopped at the bytes it was asked for, and has the limits of
      "Rules": to connect, between two reads, and a least rate, which its maker
      gives it (N3a's amendment to ADR 0011 names the setting). Its root
      certificates are compiled in.
      **Reworded with the task:** the box read "by a crate that `just deny` accepts;
      any licence it adds to `deny.toml` is named in the pull request". The owner
      approved exactly two crates, by name and with their features, and no HTTP
      crate: an HTTP crate brings more dependencies, reads proxy variables and follows
      redirects unless each is turned off, and gives no clean way to bound the time
      between two reads or to know whether a byte left (ADR 0024, "Alternatives").
      `CDLA-Permissive-2.0`, the licence of `webpki-roots`, was already allowed.
- [x] In the binary the client is made only inside a `CheckedDownloader`. The one
      public way to it is `whizbang_backends::net::downloader`, which answers a
      `CheckedDownloader`, and the client is not public; nothing in the binary makes
      one yet (task N3b's `models get` will)
      (`public::the_only_public_way_to_the_client_is_a_checked_downloader`).

**Done when** these pass (`crates/whizbang-backends/src/net/tests/`, against the loopback
listener of "Rules"):

- `client::a_download_is_streamed_to_its_sink_and_stopped_at_the_bytes_it_was_asked_for`
- `client::an_answer_that_points_elsewhere_is_handed_back_as_moved_and_nothing_is_asked_of_the_second_address`
- `client::a_credential_is_sent_as_its_header_to_its_own_origin_and_a_request_without_one_sends_none`
- `client::a_fault_holds_nothing_of_the_body_the_headers_or_the_address_that_answered`
- `client::a_request_that_never_connected_is_not_sent_and_one_cut_after_it_was_written_is_unresolved`
- `client::a_cancellation_is_looked_at_between_pieces`
- `client::a_peer_that_accepts_and_sends_nothing_ends_the_request_as_timed_out_at_the_limit_between_reads`
- `client::a_cancellation_set_while_no_byte_arrives_ends_the_request_by_that_limit_at_the_latest`
- `client::a_peer_that_sends_a_byte_now_and_then_ends_the_download_as_timed_out_by_the_least_rate`
- `client::the_client_trusts_the_roots_compiled_into_it_and_asks_no_other_service`
- `client::a_download_through_the_checked_downloader_that_is_moved_off_its_entrys_origins_is_refused`
- `client::the_installer_installs_a_model_of_two_files_from_the_listener_through_the_checked_downloader`

**Reworded with the task:** the line above read "(`tests/it/net/`, against the loopback
listener of "Rules")". The client trusts only the roots compiled into it, and a test's
server needs a root of the test's own: the constructor that takes one is
`#[cfg(test)]`, so that no other code, the binary's included, can call it, and a test
under `tests/it/` cannot reach it either. So the tests are inside the crate, in
`src/net/tests/` (module `net::tests::client`), and `tests/it/net/` holds what is
public (`public::the_only_public_way_to_the_client_is_a_checked_downloader`,
`public::through_the_public_way_an_address_off_its_origins_or_not_https_asks_nothing`).
**Added with the task**, for the owner's safety properties (ADR 0024):
`client::the_answers_the_subset_does_not_handle_are_each_refused_by_name`,
`client::the_client_speaks_tls_1_3_and_1_2_and_each_connection_is_a_full_handshake`,
`client::the_limit_to_connect_is_for_all_of_a_hosts_addresses_and_a_connect_out_of_its_time_is_not_sent`,
`client::under_a_clock_that_stands_still_a_mute_peer_ends_by_the_ceiling_of_polls`,
`client::the_client_makes_no_thread_and_one_socket_for_each_request`, the least rate's
meter at its bound (`wire::tests::…`), and the test kit's listener (`loopback::…`).

**Carry-over:** the real network client, for downloads; a fault's sentence and
`not_sent`, for this adapter. **Size:** medium.

### N2. A models directory that is its owner's; a lock asked for until a time

**Depends on:** C1a. **Reads:** ADR 0017, "Three kinds of lock", "A folder of the layout
that is a link", "Limits"; `models/directory.rs`; `store/fs.rs`.

- [x] D13: a models directory is made for its owner only, and one that another can
      write is refused when it is opened, with a refusal that says how to fix it.
- [x] The directory's lock is asked for until a time, by an amendment to ADR 0017,
      "Three kinds of lock": tries paced by the injected clock for 5 seconds, then
      busy. The 20,000 tries stay beside it as a ceiling, so that a clock that stands
      still, as a test's does, cannot make the tries endless. A lock that blocks
      without an end is not used: a lease and a removal's `--wait` must each end.

**Done when** these pass (`tests/it/models/directory.rs`):

- `directory::a_new_models_directory_is_readable_and_writable_by_its_owner_only`
- `directory::a_models_directory_that_its_group_or_anyone_can_write_is_refused_and_nothing_in_it_is_read`
- `directory::two_publications_at_once_both_end_and_neither_is_refused_as_busy`
- `directory::a_directory_whose_lock_is_held_past_five_seconds_of_the_clock_answers_busy_and_nothing_waits_longer`
- `directory::under_a_clock_that_stands_still_a_held_lock_answers_busy_at_the_ceiling_of_tries`

**Carry-over:** the lock by 20,000 tries; the two rows on a look and then an act, each
accepted in a line that names the first two tests. **Size:** small.

### N3a. The catalogue's second version, the fifth reason, where the directory is

**Depends on:** N2. **Reads:** ADR 0017, "The catalogue", "The inventory"; ADR 0015, "An
inventory"; ADR 0011 on the `models` table.

- [x] `catalogue/2`, by an amendment to ADR 0017: an entry gains a title and the
      platforms it runs on. `catalogue/1` is no longer read: no catalogue was shipped.
- [x] `Unready` gains a fifth reason, by an amendment to ADR 0015: this build has no
      adapter for the model. The inventory gives it where it said out of date.
- [x] Where the directory is: a flag, then the config's `[models] dir` (an amendment to
      ADR 0011), then the platform's data folder. The same amendment names the
      setting for a download's least rate.
- [x] The binary's wiring is given the shipped catalogue, which is empty until N5
      (`Over::installed`), and makes the inventory of it, told the secrets the
      executor is told, in the models directory it opens.
      **Reworded with the task:** the box read "The binary's wiring is given the
      inventory of the shipped catalogue, which is empty until N5 (`Over::installed`),
      told the secrets the executor is told." An inventory is over a models
      directory, and one is opened only by the opening that asks whose it is, which
      makes the folder where it is not there. Handed an inventory of the empty
      catalogue, every command that runs an action would open, and make, a models
      directory, in the data folder of whoever runs it and of whoever runs the
      tests, to be told of every model what the inventory that knows none says
      already: that it is not in the catalogue. So the wiring is given the catalogue
      and the place, and makes the inventory itself, which is also what holds its
      secrets to the executor's; and under a catalogue that holds no model it opens
      nothing (ADR 0017, "Where the directory is"). A test gives the same wiring a
      catalogue with a model, and the six tests below go through it.

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

With the task, from the review of N2 (ADR 0017, "A models directory is its owner's
alone"): who whizbang runs as is read from a pair of sockets and a failure to learn
it has its own refusal; the test that no product code names `ModelsDir::at` reads
every product crate; and the real file system is shown to say that a folder of
another user is not this user's. For N3c and N5: under the shipped catalogue, which
holds no model, no command opens a models directory or asks where the platform's
is, so none of the starts of the binary N3c moves can reach it today;
`wired::with_the_catalogue_it_ships_the_binary_opens_and_makes_no_models_directory`
holds that and fails on the day the catalogue is given an entry, by when N3c has
given every start a cleared environment and a temporary home
(`docs/carry-over.md`). This task's own start of the binary, in `wired.rs`, clears
the environment already.

### N3b. `whizbang models list`, `get` and `remove`

**Depends on:** N1, N3a. **Reads:** the carry-over row on the commands (it names what
they call); ADR 0017, "Attaching and cancelling", "Removal".

- [x] The three commands over `whizbang_backends::models`, and `docs/guides/models.md`.
      `list` prints, for each entry: its name, title, licence, size, platforms and
      state. `get` installs or attaches, waits between tries by the clock, and prints
      what the status record shows. `remove --wait <seconds>` drains.

**Done when** these pass (`crates/whizbang/tests/it/models.rs`, with a test's catalogue
and the fake downloader through the binary's own functions, `whizbang::model_commands`,
and the real binary under the catalogue it ships):

**Reworded with the task** (by N3b): the line above read "…through the binary's own
function, and `list` as the real binary". The real binary lists the catalogue it
ships, which holds no model until N5, and a test cannot hand it another: a catalogue
taken from a file or a variable would be a way to download from origins no shipped
entry names. So `list` of a catalogue with models is shown through the binary's own
function, the one `main` calls, and the real binary is started for `list`, `get` and
`remove` under the shipped catalogue
(`models::under_the_catalogue_it_ships_no_models_command_resolves_or_makes_a_models_directory`).

- `models::list_names_each_model_of_the_catalogue_with_its_title_licence_size_platforms_and_state`
- `models::get_installs_a_model_and_list_then_says_it_is_installed`
- `models::a_second_get_attaches_to_the_first_waits_by_the_clock_and_ends_with_the_same_installation`
- `models::remove_drains_and_answers_busy_at_its_wait_with_the_model_still_installed`
- `models::a_model_whose_adapter_this_build_lacks_is_listed_with_that_reason`
- `models::a_model_this_platform_cannot_run_is_listed_as_that_and_get_refuses_it`
- `models::a_model_whose_layout_has_a_link_is_listed_as_having_one`
- `models::a_secret_the_config_names_is_in_nothing_the_commands_print`

**Added with the task**, for five follow-ups of earlier reviews that were on its path:
`models::under_the_catalogue_it_ships_no_models_command_resolves_or_makes_a_models_directory`;
`models::a_relative_models_dir_is_refused_before_anything_is_made` (a relative
`--models-dir` made a folder in the working folder and was then refused);
`models::get_makes_the_download_client_with_the_configs_least_rate_or_the_default`
(the carry-over row on the least rate);
`wired::two_configs_that_differ_only_in_where_the_models_directory_is_repeat_the_same_run`
and `digest::the_models_table_is_in_no_digest` (the `models` table had come into the
config's digest with N3a; ADR 0011 is amended);
`inventory::a_model_whose_entry_does_not_name_this_platform_is_not_ready_for_that_reason_installed_or_not`
(`Unready` gains a sixth reason, that the model does not run on this platform; ADR
0015 is amended). `list` asks `models::draining` of each model and says "being
removed" of a live remover's mark, shown inside
`models::remove_drains_and_answers_busy_at_its_wait_with_the_model_still_installed`.
**For N5:** `get` installs only a model whose adapter is in the binary's table of
adapters (`wiring::adapters`), whose revision is part of an installation's identity;
the table N5 writes, which names `sam3` as not yet shipped, must still give `get` that
revision, or `get sam3` is refused as having no adapter.

**Carry-over:** the `models` commands; how one who attaches is paced; what `models
list` says of a link. **Size:** medium.

### N3c. One way for a test to start the real binary

**Depends on:** C1a. **Reads:** D12, "A test cannot reach the owner's store"; the nine
files of `crates/whizbang/tests/it/` that start the binary.

- [x] The test kit has the one function that starts the real binary, with a cleared
      environment and a temporary home; `just check` refuses a test that starts it
      another way, and the tests that start it today are moved to that function.
      The function is given the binary's path, which only the binary's own tests have
      (`CARGO_BIN_EXE_whizbang`), and hands back the command, for a test to add its
      arguments, its variables and its input to.

**Done when** these pass: in the test kit,
`binary::the_command_has_a_cleared_environment_and_a_temporary_home`; in
`scripts/tests/`, `just check` refuses the binary's path anywhere in a test but as that
function's argument, shown on planted files; and every test that starts the binary
today passes through the function.
**Why a task of its own** (sized by C0b's pull request, from `main` at 4c2f6c2): the
binary is started by `Command::new(env!("CARGO_BIN_EXE_whizbang"))` in nineteen places
in nine files (`describe.rs`, `escape.rs`, `import.rs`, `rascal.rs`, `runs.rs`,
`spend.rs`, `store.rs`, `stubs.rs`, `wired.rs`), most of them a helper many tests call,
and one more hands the path to a shell script (`describe.rs`); N3b adds `models.rs`.
None clears the environment today, so each is looked at for a variable it relied on
without saying so. That is a small task, and it is not N4's: N4 is already medium,
and its own files are the credential's.
**Reworded with the task** (by N3c): "**Depends on:** N3b", and "N3b" in the task
table, are now C1a. A review found that the first path worked out from a home folder
comes with N3a (where the models directory is when nobody says), so the guard has to
be there before N3a's tests start the binary, not after N3b's; N3c was built before
both and needs neither. "N3b adds `models.rs`" above then reads: the tests N3a and N3b
add start the binary through the function from the first.
**What was built** (the task's choices; `crates/whizbang-testkit/src/binary.rs`,
`scripts/check-binary-start.sh`):

- The function is `whizbang_testkit::binary::command`. The started program is given
  six variables and no other: `HOME`; `XDG_DATA_HOME`, `XDG_CONFIG_HOME`,
  `XDG_CACHE_HOME` and `XDG_STATE_HOME` inside that home; and `TMPDIR` beside it. All
  are in one folder made for the call, readable and writable by its owner alone, under
  a name that is refused when anything is already there, and removed with the command.
  No `PATH`: every test that starts the binary passes without one.
- The test that hands the binary's path to a shell script
  (`describe::a_write_that_fails_is_exit_3_with_the_reason`) runs the script through
  the function's command (`under_shell`): the same environment, the path in
  `WHIZBANG`. The check has no exception.
- The check is `scripts/check-binary-start.sh`, run by its script test
  (`scripts/tests/test-binary-start.sh`), which is part of `just check`. It reads text.
  Its top says what it refuses and what it does not catch.
- No test had relied on the environment it ran in: all passed unchanged in meaning.
- A test file of a crate that starts the binary may not clear the environment again
  or take a variable from it (`.env_clear(`, `.env_remove(`). N3a's test that started
  the binary with its own cleared environment and home
  (`wired::with_the_catalogue_it_ships_the_binary_opens_and_makes_no_models_directory`)
  starts it through the function, and looks for nothing made in the function's home.
  The texts `wired::only_main_reads_where_the_platforms_data_folder_is` searches the
  binary's source for are in a data file, `environment-reads.txt`, which the check
  does not read; the test requires all five.

**Shown by:** `binary::the_command_has_a_cleared_environment_and_a_temporary_home`
(the test kit: a process with a planted variable calls the function, and the program
it starts lists its whole environment); `scripts/tests/test-binary-start.sh` (the
workspace passes, with twenty-two places counted: the twenty above, the one N3a
added in `wired.rs` and the one N3b added in `models.rs`; each other way is refused on planted files); and the tests of `crates/whizbang/tests/it/` that start the binary, each
through the function, which the check counts.
**Size:** small.

### N4. A download's credential

**Depends on:** N3c. **Reads:** ADR 0012, "An address, and a credential"; ADR 0019, "A
download and a probe"; ADR 0011 on keys from the env file.

- [x] D12, by amendments to ADRs 0011, 0012 and 0017: the config's key for an origin's
      token, which names where the token is (the Hub's command-line sign-in, or a
      variable of the env file); an entry's statement that its origin needs one; the
      installer attaching it to a request for that origin.
- [x] The reader of the sign-in's store: one function names the real place, from the
      Hub's documentation (open question 9), and only `main` calls it, which `just
      check` holds; the wiring is given the place as a value. The token is read when a
      download from its origin is about to be asked and not before, told to redaction
      before the request is made, and kept nowhere after the install ends.
      A variable `HF_TOKEN` in the binary's environment, or `HUGGING_FACE_HUB_TOKEN`,
      its older name, is not read: the Hub's own tools let it take the place of the
      stored file (open question 9), but the owner chose the sign-in, and whizbang's
      own other way in is the env-file key. The binary's tests of this start it through
      the test kit's one function, with a cleared environment and a temporary home,
      which is N3c's.

**Done when** these pass (`tests/it/models/credential.rs`, `tests/it/config/`, and the
last two in `crates/whizbang/tests/it/models.rs`), each with a made-up token in a temporary
folder:

- `credential::a_download_from_an_origin_the_config_names_a_token_for_carries_it_and_one_from_another_origin_does_not`
- `credential::a_download_moved_to_another_origin_is_asked_there_with_no_credential`
- `credential::an_entry_whose_origin_needs_a_token_the_config_does_not_name_is_refused_before_anything_is_asked_and_says_how_to_sign_in_or_which_key_to_set`
- `credential::the_token_is_in_no_status_record_no_refusal_and_nothing_get_prints`
- `credential::an_origin_that_answers_that_access_is_refused_ends_the_install_with_a_sentence_that_says_to_ask_for_access`
- `credential::a_token_of_the_sign_in_is_read_from_the_place_the_wiring_was_given_and_sent_to_its_origin_only`
- `credential::the_sign_ins_store_is_not_opened_by_a_command_that_downloads_nothing_from_its_origin`
- `credential::a_sign_in_that_is_not_there_or_cannot_be_read_ends_the_install_before_anything_is_asked_and_its_refusal_holds_no_path`
- `credential::a_token_of_the_sign_in_is_redacted_from_a_fault_made_after_it_was_read`
- `models::the_real_binary_started_with_a_temporary_home_reads_the_sign_in_there_and_no_other`
- `models::the_real_binary_started_with_hf_token_set_reads_the_sign_in_and_never_the_variable`

and in `scripts/tests/`: `just check` refuses a call of the function that names the
real place of the sign-in's store anywhere but `main`.

**Reworded with the task** (by C0, with the owner's decision of 2026-11-15, D12): the
third test's name ended "…and_says_which_key_to_set"; a missing token is now a missing
sign-in or a missing key, so its refusal says both. Five tests and the script test are
added for the sign-in. What keeps the worker from the sign-in's store is P3a's, with
the worker's environment.
**Added with C0b's pull request** (two follow-ups of the review of C0's): what the
reader does with `HF_TOKEN`, with the eleventh test; and that the binary's tests start
it through the test kit's one function, which is a task of its own, N3c, since it
moves tests N4 does not otherwise touch.
**Reworded with the task** (by N4), three things:

- "only `main` calls it, which `just check` holds" now reads: only `main` names it,
  and hands it on uncalled to the models commands, which ask it only for `get` of a
  model that needs the sign-in's token; `just check` refuses its name anywhere else
  and a call of it in `main` too (`scripts/check-sign-in.sh`, shown on planted files
  by `scripts/tests/test-sign-in.sh`). It is `main`'s `hub_sign_in_token_file`, the
  pattern of `platforms_models_dir`: a function `main` called would read the sign-in
  for every command.
- "(`tests/it/models/credential.rs`, `tests/it/config/`, …)" now reads
  `crates/whizbang/tests/it/credential.rs` for the nine `credential::` tests: they go
  through the binary's own `get`, which is the binary crate's, so they are in its
  tests; and `crates/whizbang-core/tests/it/config/file.rs` for the config's key
  (`file::a_downloads_token_is_named_for_an_origin_as_the_sign_in_or_a_variable_and_the_sign_in_for_the_hub_alone`).
- The last two tests could not be shown as worded: the real binary reaches the reader
  only through a shipped model that needs a token, and the shipped catalogue holds none
  until N5, which also cannot be handed another (N3b's rewording). Each is now two
  halves. The first is a reading of the binary's source: `main`'s one function hands
  `HF_TOKEN_PATH`, `HF_HOME`, `XDG_CACHE_HOME` and `HOME`, in that order, to
  `sign_in::token_file` and reads nothing else for it, and no source of the binary
  names `"HF_TOKEN"` or `HUGGING_FACE_HUB_TOKEN`; with the place's order shown on
  made-up folders
  (`sign_in::tests::the_place_is_hf_token_path_then_hf_home_then_xdg_cache_home_then_the_home_folder`)
  and the reading shown through `get` (`credential::`). The second starts the real
  binary, through the test kit's function, for each shipped model that needs the
  sign-in's token: with a temporary home and nothing there, and with a file there that
  others can read, each refused as that before anything is asked; and with `HF_TOKEN`
  and `HUGGING_FACE_HUB_TOKEN` set and no sign-in, refused as not there. That half
  runs for no model until N5's entry, and N5 shows it running (`docs/carry-over.md`,
  the row on the real binary's reading of the sign-in).

**Decided with the task** (ADR 0011, "A download's token"): the reader follows all
three of the Hub's variables that move the place, and not only the default, since the
Hub's tool writes the sign-in where they say; a variable set to a relative path names
no place. A token file that others can read or write is refused, not read and warned
of: the Hub's tool writes it 600. The file's one trailing line break is taken off;
anything else that is not printable ASCII is refused. `get` of a model installed
already reads the token too, since whether anything is downloaded is known only under
the model's lock.
**Changed after the security review of N4** (two findings and four follow-ups): the
reader looks at the name before it opens, refuses anything but a regular file, and
opens without waiting
(`credential::a_pipe_at_the_sign_ins_place_is_refused_before_it_is_opened_and_nothing_waits`);
the opened file must be this user's
(`sign_in::tests::a_token_file_that_is_not_this_users_or_that_others_may_read_or_write_is_refused`);
its folder must not be one others can write (a case of
`credential::a_sign_in_that_is_not_there_or_cannot_be_read_…`); a file at the same host
on another port carries nothing
(`credential::a_file_at_the_same_host_on_another_port_is_asked_for_with_no_credential`);
the gate and the real client on loopback carry no credential to another allowed
origin after a move
(`client::a_credentialed_download_moved_to_another_allowed_origin_is_asked_there_with_no_credential`);
`scripts/check-sign-in.sh` holds the handing on to the models commands' call. And for
N5, the coordinator's decision: no built-in default; the owner writes the
`[[models.tokens]]` entry, and the refusal prints the three lines to add.
**Added with the task**, for follow-ups of the reviews of N3b and N3c:
`models::list_of_an_empty_catalogue_and_get_or_remove_of_a_model_it_does_not_hold_never_ask_where_the_platforms_directory_is`
(a platform that counts each ask); the guide's "Limits" gains the unbounded wait of
one attached to another install and a build for a platform no catalogue names; the
documentation of `Installations::on` and its line in ADR 0017 no longer say it is for
a test; and the guide says where `get` takes the token from.
**Carry-over:** a credential for a download that needs one. **Size:** small, now
medium.

### N5. The shipped catalogue, and SAM 3 installed on the Mac

**Depends on:** N4. **Needs the owner:** the Hub's storage host approved by name
(the Hub itself was approved on 2026-11-15); access to the model, which the owner says is
granted; the Hub's command-line sign-in on the Mac, which the owner has. **Runs on:** the owner's Mac, for its last box.

- [x] The shipped catalogue, built into the binary, with one entry, `sam3`: the files
      of D11, each `from` an address at a pinned revision of `facebook/sam3`, the
      licence as its maker names it, the features this build's adapter gives (`phrase`).
      The adapter's revision is fixed here, in the binary's table of adapters, as the
      one R2a will ship under: it is part of an installation's identity (ADR 0017), so
      a revision changed later is a second download of 3.44 GB. Until R2a merges the
      pack has no such adapter and a `load` of `sam3` would be refused as unsupported.
      So the table says of each adapter whether this build's pack ships it, and N5
      enters `sam3` as not yet shipped: in that window the inventory gives the model
      N3a's fifth reason, this build has no adapter for it. `models list` shows it as
      installed and not ready for that reason, and a call for it is refused when it is
      prepared, before a worker is started. Nothing calls the model ready while a load
      of it would be refused. The identity does not depend on the mark, so R2a changes
      it and nothing is downloaded again.
- [x] How the sizes and hashes were fixed, written in the entry's comment and the ADR:
      the large file's from the Hub's public listing of the pinned revision, which
      states its SHA-256 (read by the builder, outside whizbang: "Origins"); the small
      files' from the bytes fetched once through the checked downloader by
      `whizbang models pin <draft entry>`, a developer's command that prints each file's
      size and hash, and the origin alone of a hop it refused, and installs nothing.
      The second is trust on first use, stated as a limit. `SPEC.md` section 11
      gains the command.
- [x] On the Mac: `whizbang models get sam3`, by the owner or with the owner's leave,
      into a models directory outside the repo. Nothing but whizbang downloads a file.
      The token is read by the binary from the sign-in (D12), and is never printed,
      passed as an argument or read by an agent: no agent opens, lists or prints the
      sign-in's store, before or after.

**Done when** these pass in the fast tier:

- `catalogue::the_shipped_catalogue_parses_names_sam3_and_no_fake_model`
- `catalogue::every_file_of_the_shipped_entry_is_at_an_origin_the_entry_names_and_at_one_pinned_revision`
- `catalogue::the_shipped_entry_names_no_pickle`
- `models::pin_prints_the_size_and_hash_of_each_file_it_fetched_through_the_checked_downloader_and_installs_nothing`
- `inventory::a_model_whose_adapter_the_binarys_table_names_as_not_yet_shipped_is_not_ready_for_want_of_an_adapter_installed_or_not`
- `catalogue::every_adapter_the_binarys_table_names_as_shipped_is_an_adapter_the_pack_has`

and, on the Mac, recorded in the pull request as fields (the installation's identity,
the state `models list` gives, the bytes installed, the time), never as a command's raw
output, which holds absolute paths: `sam3` is installed; a second `get` installs
nothing; `models list` gives it as installed and not ready, for want of an adapter; the
secrets scan passes over the working tree, which holds no token.
**Reworded with the task** (by C0, with the owner's decision of 2026-11-15, D12): the
last clause read "the secrets scan passes over the working tree with the token in the
env file". The token is in the sign-in's store and in no env file, so the scan cannot
search for its value; the clause now says what the scan can show. Two tests are added
for the window before R2a, a follow-up of the plan's review.
**Reworded with the task** (by N5): the second box read "the large file's from the Hub's
public listing of the pinned revision, which states its SHA-256 (read by the builder,
outside whizbang: "Origins"); the small files' from the bytes fetched once …". The
listing was read once, without a token, as approved: for this gated repository the Hub
masks the large file's `lfs.oid` and its Xet hash, each as 64 asterisks, to a reader
without a token, so it could not give the hash. Every file's hash, the large file's
included, was then fixed by `whizbang models pin`, trust on first use, with the
orchestrator's leave; the listing gave every file's size, which `pin` agreed with. The
first `get` downloaded the files a second time and held each to its size and hash. "The
entry's comment" is the documentation of `wiring::catalogue` and ADR 0017, "The shipped
catalogue": a catalogue is strict JSON, which has no comment.
**What N5 decided and built** (ADR 0017, "The shipped catalogue"; ADR 0019's amendment):

- The entry: commit `3c879f39826c281e95690f02c7821c4de09afae7` of `facebook/sam3`;
  `model.safetensors`, `config.json`, `processor_config.json`, `tokenizer.json`,
  `tokenizer_config.json`, `special_tokens_map.json`, `vocab.json`, `merges.txt` and
  `LICENSE`; origins `https://huggingface.co` and `https://us.aws.cdn.hf.co`; the
  licence `SAM License`, the first line of its maker's licence; the platform
  `macos-arm64` alone, the one 1C installs on (Linux comes with the runtime on Linux,
  1D, under a new revision); entry revision `2026-11.1`; adapter `sam3` at revision
  `1`, not yet shipped.
- `whizbang models pin <draft>` (`model_commands::pin`, `models::pin_file`): through the
  checked downloader, which can now be told the origin of an address it refused
  (`CheckedDownloader::noting_refused`,
  `chains::a_checked_download_that_notes_refusals_is_told_the_origin_alone_of_each_address_it_refused`).
- A build with no kind of backend opens no models directory for a command that builds
  an executor: with the first entry, `rascal grow` under a config would otherwise have
  made one (ADR 0017's amendment;
  `wired::with_the_catalogue_it_ships_the_binary_opens_and_makes_no_models_directory`,
  rewritten to what is now true, and failing on the day a kind is registered).
  `models::under_the_catalogue_it_ships_no_models_command_resolves_or_makes_a_models_directory`
  is now
  `models::under_the_catalogue_it_ships_no_models_command_makes_a_models_directory_or_asks_anything_of_any_address`:
  `list` of `sam3` does work out where the directory is.
- The carry-over rows N5 owned: the two real-binary token tests
  (`models::the_real_binary_started_with_a_temporary_home_reads_the_sign_in_there_and_no_other`,
  `models::the_real_binary_started_with_hf_token_set_reads_the_sign_in_and_never_the_variable`)
  now start the binary for `sam3` on macOS, each refused before anything is asked and
  before the models directory is made; on Linux the entry's platform is refused first,
  so they run there for no model. The resolver's wait is accepted in a line, with what
  bounds it on each platform. The narrow client is enough for the Hub (open question
  2). N4's follow-up on the open that does not wait is held by
  `sign_in::tests::a_fifo_opened_by_the_open_that_does_not_wait_is_opened_at_once_and_judged_no_regular_file`.

**On the Mac** (2026-11-17, the release build of this branch, through `scripts/heavy.sh`
with the thread limit at 200, under a config that names the sign-in and a models
directory in the builder's scratch folder; fields only):

| Field | Value |
|---|---|
| Installation's identity | `sha256:37ab46c2f87b600f4654dd972ce85a27b1024b4fc92901b11464bd6fa9664d32` |
| Bytes installed | 3,445,003,826, in nine files |
| Time of the first `get` | 1,220 s |
| A second `get` | The same identity, in 2 s; nothing downloaded |
| `models list` | `installed, not ready: this build has no adapter for it` |
| Hosts whizbang asked | `huggingface.co` (every file, with the token) and `us.aws.cdn.hf.co` (the large file, moved there; no token): the entry's two origins, which its checked downloader holds it to |

**Carry-over:** the shipped catalogue. **Size:** medium.

### Y1. `whizbang setup`, and the lock file

**Depends on:** P3a. **Needs the owner:** the package index approved. **Reads:** rascal
`crates/rascal-mcp/src/provision.rs` and `scripts/build-bundle.sh` (what a bundle was,
as reference for what is dropped).

- [ ] ADR 0021, installing the pack: D9 and D10; where the environment is; what `setup`
      contacts; why bundles are dropped.
- [ ] The pack's extra for candidate A, and `pack/uv.lock`, made on the Mac.
- [ ] `whizbang setup [--extra <name>]`: `uv sync --locked` of the pack into the
      environment, by `uv` at its pinned version, under the row of "Rules" on `uv`, and a check that the worker it made
      says hello. `setup` starts that worker itself, with `--max-lifetime-s 120`: the
      kind and its setting `max_lifetime_s` come later, with P6a. The kind `pack` finds
      the worker there. `docs/guides/setup.md` says
      what it installs and every origin it contacts.

**Done when** these pass (`crates/whizbang/tests/it/setup.rs`; all are in the fast
tier, install no package and run offline, the limit's with a stand-in for `uv` that
never answers and ends itself after 120 s and not before, since nothing is left to kill
it if the test is killed):

- `setup::setup_with_no_extra_makes_an_environment_offline_whose_worker_says_hello`
- `setup::setup_refuses_a_lock_file_that_does_not_match_the_pack_and_changes_nothing`
- `setup::a_second_setup_changes_nothing`
- `setup::the_lock_file_pins_every_package_of_every_extra_by_version_and_hash`
- `setup::the_lock_file_names_no_index_but_the_approved_one`
- `setup::a_uv_that_does_not_end_within_its_limit_is_killed_and_waited_for`
- `setup::uv_is_given_the_allow_list_and_no_users_config`
- `setup::the_worker_setup_starts_for_its_hello_is_given_a_lifetime_of_120_seconds_and_is_gone_when_setup_ends`

and, on the Mac, recorded in the pull request as fields (how it ended, the
environment's size, the time, the peak of threads), never as raw output: `whizbang
setup --extra` for the runtime ends well. **Carry-over:** offline bundles, dropped in a
line. **Size:** medium.

### P6a. The kind `pack`: a local segmenter through the executor

**Depends on:** P5b, N3b. **Reads:** ADR 0011, "Kinds"; ADR 0006, "Segment"; the
carry-over rows on the roster and the backends, and on the old configs.

- [ ] `kind = "pack"` in `wiring::kinds`: a `Segmenter` over the one supervisor of D4. It
      declares no recipient and no price, and the features the catalogue's entry states.
      Its settings in the config are the model's (the threshold and the rest of P1),
      the limit for a call and `max_lifetime_s`, by an amendment to ADR 0011; a setting
      it does not know is refused.
- [ ] One walk of the config makes the roster and the backends in the binary.

**Done when** these pass (`crates/whizbang/tests/it/pack.rs`, through the binary's
wiring, with a test action that asks for the segment capability and the pack's fake
models in a test's catalogue):

- `pack::a_call_to_a_local_model_is_answered_by_the_worker_and_its_record_names_the_backend_the_model_and_the_installation`
- `pack::two_actions_in_one_process_that_use_one_model_cause_one_load`
- `pack::two_backends_of_the_kind_under_two_names_and_two_snapshots_of_the_config_share_one_worker`
- `pack::a_run_that_needs_no_local_model_starts_no_worker`
- `pack::a_worker_started_through_the_wiring_is_given_the_lifetime_its_config_sets`
- `pack::a_model_the_config_resolves_and_no_backend_holds_is_refused_at_admission`
- `pack::a_setting_the_kind_does_not_know_is_refused_when_the_config_is_read`
- `pack::a_local_call_reserves_nothing_and_adds_no_name_under_spend`
- `pack::the_shipped_binary_offers_no_fake_model_and_starts_no_entry_point_that_takes_a_fault`

**Carry-over:** the roster and the backends from one thing; the settings of the first
real kind. **Size:** medium.

### P6b. The installation a call is answered from

**Depends on:** P6a. **Reads:** ADR 0015, "An inventory", the lease, "A wrapper around
each request"; ADR 0017, "Holds", "The lease".

- [ ] D14: the files are verified before `load`. A `load` names the installation in the
      declaration the call was handed under, never the one the pointer names.
- [ ] The count of D5 reaches the queue: the lease hands on the count its hold returned
      in what a backend is given for one request, by an amendment to ADR 0015.

**Done when** these pass (`crates/whizbang/tests/it/pack.rs`):

- `pack::after_a_reinstall_between_two_requests_of_a_run_the_first_is_answered_from_the_old_installations_files_and_the_second_is_refused_at_the_lease`
- `pack::a_file_damaged_after_it_was_published_is_not_loaded_and_the_model_is_reported_as_damaged`
- `pack::a_request_enters_the_queue_with_the_count_its_own_hold_was_given`
- `pack::through_the_wiring_a_removal_from_a_second_process_cancels_a_queued_request_and_not_one_leased_after_it`

**Carry-over:** a local backend loading by the identity it was handed under; unverified
files of an installation. **Size:** small.

### P7a. `run/4`: the device and the cache

**Depends on:** P6b. **Reads:** ADR 0005 and ADR 0013 on a record's versions;
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

- [ ] Candidate A, tried by one script in `pack/`, run once by hand through
      `scripts/heavy.sh` and `scripts/thread-guard.sh` with its limit at 200: load from
      the installation's folder, one phrase on one picture, twice. If A fails the task
      stops (D8). The script runs outside the supervisor, so it brings the
      supervisor's bounds itself: a cleared environment with P3a's allow-list; the
      offline and no-telemetry variables of D11; the variables that point the Hub's
      libraries away from the sign-in's store, the same ones P3a's worker environment
      sets (D12; open question 9), set before anything of the Hub's is imported, since
      its libraries read them at import; the thread limits of "Rules"; the
      watermark, starting at 0.5; and its own end at 10 minutes, by
      `faulthandler.dump_traceback_later(600, exit=True)`. It runs under the watch on
      resident memory of "Rules", which this task writes as a script beside the
      thread guard and which kills over 24 GiB; `scripts/tests/` holds the watch to
      that with a stand-in that grows. It places the model on the
      device itself, by name, and uses no `device_map="auto"`.
- [ ] ADR 0022, the first runtime: each candidate against the bar, with the page that
      shows point, box and CUDA support; what was measured (the device, load time, the
      first and second request, peak memory, threads, the environment's size); every
      thread pool the runtime has and what holds each; the
      licences of the runtime's packages from their metadata; what the choice rules out
      (nothing: a second runtime is another adapter); the watermark chosen.
- [ ] `SPEC.md` section 17's rows on the runtime, the pack over MCP, the two clients and
      offline bundles say what was decided. Wording only.

**Done when** ADR 0022 names one runtime with its numbers and its sources, the script's
numbers are in the pull request as fields (never its raw output, which holds absolute
paths), and `just check` passes. A fast test holds the ADR to the
lock: `setup::the_runtime_the_adr_names_is_the_one_the_lock_pins`. And ADR 0022 says
whether the runtime starts processes of its own (compilers, loaders, pools of
processes), from its documentation and from the run: if it does, each ends with the
worker, or the supervisor kills them with it, with a test; the supervisor of P3a
kills only the worker, and lets go of readers that such a process keeps waiting.
**Added with P3a's pull request,** from its review: a process the worker starts that
outlives it holds the worker's streams open (`docs/carry-over.md`).
**Size:** medium.

### R2a. The adapter `sam3`

**Depends on:** R1, P6a (the settings it reads arrive in `load`).

- [ ] The adapter `sam3` in the pack, on the chosen runtime: image features computed
      once for a batch; a phrase for each query; the settings of P6a; the answer in the
      fixed order. It declares `phrase` only, under the revision N5 fixed, and the binary's
      table now names it as shipped, so the inventory calls an installed `sam3` ready. It asks its runtime for the GPU, falls
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
- `test_sam3.Revision.test_the_adapters_revision_is_the_one_the_binarys_table_names`

and, in the fast tier, N5's
`catalogue::every_adapter_the_binarys_table_names_as_shipped_is_an_adapter_the_pack_has`
now covers `sam3`.

**Size:** small.

### R2b. SAM 3 answers a phrase on the Mac's GPU

**Depends on:** R2a, P7b. **Runs on:** the owner's Mac.

- [ ] `just test-models` and `scripts/model-evidence.sh` (D15), and the fixture picture
      with a loose box drawn by hand around the thing its phrase names, committed
      with no metadata (no EXIF, no profile, no text chunk; a test holds it). The recipe is
      heavy, runs under `scripts/thread-guard.sh` with its limit at 200 and under R1's
      watch on resident memory (24 GiB), as the evidence script does, refuses without
      a models directory named in the local environment, and is listed in `AGENTS.md`.
- [ ] The catalogue's entry and the adapter's revision are unchanged from N5, so
      nothing is downloaded again. `docs/reference/` is unchanged: no action is added
      in 1C.

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
| The store's locks: a child's copy while the worker is started | P3a; the moment for Ctrl-C accepted in a line (D7) |
| Pull-request CI on the macOS runner over its budget | C0 (measured); C0b (the job's 10 minutes, by five runs of one commit; Z1 reports `main`). The fast tier's test time there against its 180 s: **the owner**. **Settled by the owner on 2026-11-17**: the limit on GitHub's hosted macOS runner is 300 s, because its test time varies about two-fold on unchanged code; Linux CI and the owner's Mac keep 180 s, and one test keeps 30 s (`scripts/timing.sh`, `SPEC.md` section 13) |
| A file opened by a look and then an opening; a lock file opened plainly | N2: accepted in a line for a models directory (D13). **The store's half: milestone 5, with the row on the store's locks.** **Reworded with the task** (by N3a, from the review of N2): the cell read "N2: accepted in a line (D13)". D13 is about a models directory, and the row's two openings to lock that are the store's own (a reader's share of a generation file, a prune's try at one) are in no models directory: nothing asks whose a store's folder is. So the row stays open for those two, re-owned as the row itself now says |
| The run record: `device` and `cached` | P7a (`run/4`) |
| What a segmenter adapter took and gave | P1 (the order, diagnostics, settings in `load`), P6a (settings in the config) |
| What a filler was sent beside an image and one plane | **Milestone 3, with the first fill backend.** 1C has none |
| An adapter's keeping of the rule for a fault's sentence | P3b (the worker), N1 (the client) |
| Nothing bounds what a backend builds; nothing cuts off one past its deadline | P1 (the bound), P4 (the deadline) |
| Whether a failed call may have been charged: `not_sent` | P3b, N1 |
| The roster and the backends given apart | P6a |
| The models directory's lock by 20,000 tries | N2 (a time in place of a count) |
| A folder of the layout that is a link | N2 (accepted in a line), N3b (`list` says so) |
| Nothing reads an installation's files before they are loaded | P6b (D14) |
| A model whose adapter this build lacks | N3a (the reason), N3b (the command) |
| Parallel installs of models sharing a file | **1D, with the second catalogue entry.** 1C ships one entry, so nothing can share |
| What a catalogue entry and a models directory held in rascal | N3a (title, platforms, where the directory is), N4 (the credential). The graph's external data: dropped in a line by R1 when the runtime is A, which loads no graph |
| `whizbang models list`, `get`, `remove` and their guide | N3b |
| `runs show` cuts a model's version | P7a |
| One who attaches is paced by its caller | N3b |
| `whizbang backends status` | P7b (a remote probe by the test kit's prober; a real one with milestone 2's sender) |
| The real network client and the shipped catalogue | N1 (downloads), N3a, N3b, N5. **The sender, with `SendCheck::new().keeping_out(root)` over it in the binary, and a real prober: milestone 2, with the first provider's kind.** Nothing in 1C sends, and the row itself says a provider's sending follows there |
| A local backend loads the installation it was handed under | P6b |
| A backend made from a config sends with no credential | **Milestone 2, with the first provider's kind.** The kind `pack` sends nothing to a recipient |
| A recipient is equal only with its upstreams | **Milestone 2,** for the same reason |
| The encoder for sending stores its rows | **Milestone 2, before the first picture sent to a provider.** 1C sends no picture off the machine |
| What the worker's queue owes to removal | P5b |
| A removal is ended by its deadline alone | P5a (a line) |
| The mark of a remover that died | P5a (D6), P5b (a request answered after it) |
| What the old configs held: the first real kind's settings | P6a; the judge loop's stay with milestone 2 |
| Speed of local models (R12) | R2b measures a first and a second request; 1D the rest |

New rows C0 adds: the HTTP codec's code and its conformance run (milestone 6); a request
from the worker to whizbang, for tools beside the model (milestone 2); holding several
models and running calls side by side (none; reopen on need, as section 4 defers it); a
second runtime (none; on a measured need, as section 17 says); a user's own weights
and a user's own adapter with the conformance test they are given (1D, with the
`add-adapter` skill). C0 added the first, the second and the last; the third and the
fourth were already rows under "Evaluations", which C0 brought up to date. And one
more, from the owner's decision of 2026-11-15: hosting the model's files for customers
(milestone 4, "The loop and a release", the first milestone at which someone who is not
the owner installs whizbang; its "done when" starts with reading the licence). With
C0b's pull request the row also says what changes on whizbang's side when it hosts the
files: N4's refusals that say to sign in or to ask for access, and the models guide.

## Risks

| Risk | Effect | What we do |
|---|---|---|
| Candidate A does not run SAM 3 on the Apple GPU | The last clause of the row is not met | R1 runs before the adapter is written and needs only N5 and Y1, so it is known early; the tag `m1c-fakes` holds everything else. B is then a change to this plan, and the owner is told |
| It runs only with operations falling back to the CPU | "On the GPU" is arguable | D8: the fallback is off for the evidence; with it on, the operations are listed and the owner decides |
| Access to the model is refused, or the sign-in is not there or cannot be read as a file | N5, R1, R2a and R2b wait | Asked at C0: the owner says access is granted and the sign-in is on the Mac. Needed at N5; both strands up to P7b need neither. The env-file key is the other way in (D12) |
| The Hub moves a download to a host nobody approved | The installer refuses, as built | Open question 2: the host is named to the owner before it is added |
| A worker outlives its test or its parent | Processes pile up on the owner's machine | Four things end a worker ("Rules"), each with a test; a worker is started lazily and one at a time |
| The runtime starts more threads than a machine should hold | The machine is slowed or worse | The pools are set before the runtime is imported; the count is read after each call and fails the run over 200 |
| The protocol is fixed before the real adapter exists | A second version inside one checkpoint | The protocol carries planes and requests ADR 0006 already fixed; R2a changes the adapter, not the frames; a misfit is `pack/2`, never an edit |
| Python in the fast tier slows `just check` and CI | The budgets C0b just met are lost | D10: no package is installed; the pack's tests have 10 seconds; C1b reports the step's time |
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

The pull request's independent review then blocked draft 3 on three things, all text: a
worker started through the binary's wiring had no lifetime limit, `uv` and the pack's
Python tests were unbounded, and seven tests named faults that no fake could produce.
Draft 4 fixes the three and folds in its sixteen follow-ups: C1, P3 and P6 are each
split in two, and P5a is medium.

Draft 5 is the entry check's, task C0, and had no review round of its own beyond the
review of C0's pull request. It holds three kinds of change:

- **What C0 measured and chose** (under C0): the tables, option (e) added to C0's box
  and chosen, and C0b's "Done when" holding the new job too.
- **Four follow-ups of the review of this plan's pull request:** Y1's stand-in for a
  `uv` that never ends now ends itself; R1's script and `just test-models` run under
  the same watch on resident memory as the evidence script; `setup` passes its hello
  check a fixed lifetime, since Y1 is built before the kind; and in the window between
  N5 and R2a the inventory calls `sam3` not ready, for want of an adapter.
- **What the owner decided on 2026-11-15,** as the orchestrator passed it on. These
  are the owner's own decisions, so they took no review round: the three origins
  approved ("What the owner approved, and what is still asked"); access to the model,
  as the owner's statement; the token read from the Hub's command-line sign-in, "for
  now" (D12, N4, N5, P3a's environment, open question 9); and hosting the model for
  customers as the owner's intent, not built in 1C (D11, and its carry-over row).

C0b's pull request added to draft 5, with no round of its own beyond its review: what
C0b built and measured (under C0b); and four follow-ups of the review of C0's pull
request, with two readings of the Hub's documentation. The follow-ups: one way for a
test to start the real binary, as a task of its own (N3c, which N4 now needs); what
N4's reader does with `HF_TOKEN`; R1's script setting the variables that point away
from the sign-in, as P3a's worker environment does; and the hosting row saying what
changes in N4's refusals and the models guide. The readings: open question 9 (where
the token is and which variables move it) and open question 2 (transfers through the
Hub's Xet client).
