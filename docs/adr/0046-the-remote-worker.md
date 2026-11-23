# 0046. The remote worker: a front that speaks `pack/1` over HTTP and supervises the worker beside a card

Status: accepted.
Date: 2026-11-23
Spec: section 4 ("The model pack": one codec for the local worker and one for HTTP; one model,
one call at a time behind a bounded queue; cooperative cancellation; a restart fails everything
in flight and replays nothing; the device chosen at runtime), section 5 (segment, self-hosted,
milestone 6), section 6 ("Spend": GPU time on a self-hosted backend, as amended by this record),
section 9 (a remote backend reports the model files it runs by hash); plan
`docs/plans/m6-http-and-remote-hosting.md`, "The remote worker" and task P1 (task R1 fills the
client's section). Amends ADR 0020, "The HTTP codec, on paper", to "as built".

## Context

Until milestone 6 the pack's worker runs only on the machine whizbang runs on, supervised by
the Rust supervisor over pipes (ADR 0020, "The worker"). User story 1 runs SAM 3 on a GPU host
whose app server has no GPU and no Python. ADR 0020 decided the HTTP codec on paper: the same
frames as a request body and a streamed response body. What it did not decide is who does the
supervisor's work on the host (the queue, the load before a call, the cancel's deadline, the
restart, the check of the files), how a host's answer names the files it came from, how a
worker's end and its error output, which are outside the frames, cross the network, and what
bounds the bill of one request on a host that bills by the second.

## Decision

### What bounds a request's bill on Modal

Read on 2026-11-23 from Modal's public documentation alone, with no account and no call. Each
limit, whether it holds across Modal's own restarts, and the overrun Modal states:

| Phase | Limit Modal enforces | Across Modal's restarts | Overrun Modal states | Source |
|---|---|---|---|---|
| Startup | `startup_timeout`, the container's startup (since client 1.1.4; before it, `timeout` covered startup too) | Per container: each restart starts a new container and a new startup | None stated | <https://modal.com/docs/guide/timeouts> |
| Execution | `timeout`, 1 second to 24 hours, 300 seconds by default; "It does not include scheduling time" | "per execution attempt"; it "will start new execution timeouts on each retry" (their example: a 100-second timeout and 3 retries run at least 400 seconds) | "they may run a handful of seconds longer" | <https://modal.com/docs/guide/timeouts> |
| Idle | `scaledown_window`, 2 seconds to 20 minutes, 60 seconds by default; "you will be billed for any resources used while the container is idle" | Per container | None stated; no absolute lifetime of a container is stated | <https://modal.com/docs/guide/cold-start> |
| A web endpoint's request | 150 seconds, then a 303 redirect to a result URL | Not stated | Not stated | <https://modal.com/docs/guide/webhook-timeouts> |
| Restarts after a crash | None: "For deployed Apps, container crashes will be retried indefinitely"; Modal "will reschedule the container and any work it was currently assigned"; the `retries` setting is not what governs it | Unbounded | Not applicable | <https://modal.com/docs/guide/retries> |
| Restarts after a preemption | None: "All Modal Functions are subject to preemption by default"; Modal will "restart it on the same input"; no count is stated, nor whether these restarts count against `retries` | Unbounded | Not applicable | <https://modal.com/docs/guide/preemption> |
| A card that is not preempted | Not available: "The `nonpreemptible` parameter is not supported for GPU Functions" | Not applicable | Not applicable | <https://modal.com/docs/guide/preemption> |
| `modal.Retries` | `max_retries`, with no stated range and no statement of which failures it covers | Not stated | Not stated | <https://modal.com/docs/reference/modal.Retries> |
| A workspace's budget and spend limit | A monthly cap on the whole workspace or one environment; at the spend limit Modal "stops workloads that would incur additional out-of-pocket charges" | Not per request | How fast workloads are stopped, and any overrun, are not stated | <https://modal.com/docs/guide/budgets> |

**Modal gives no bound for a request.** Each attempt has documented limits on its startup, its
execution and its idle time, but the number of attempts does not: a GPU function can always be
preempted, a preempted input is restarted with no stated count, and a crash of a deployed app
is retried without end, whatever `retries` says. Each new attempt bills a new startup.

**What whizbang bounds instead** (the orchestrator's decision of 2026-11-23, under the owner's
standing rule that cost controls are a convenience guard and not a hard production control:
keep work running and record the limit):

- **One attempt is bounded.** `host_limit_s` is the sum of the GPU function's
  `startup_timeout`, `timeout` and `scaledown_window`, each set in the deployment (task D1) and
  read back by `scripts/gpu-host.sh deploy`, which refuses when any of the three is missing or
  when their sum is not the config's `host_limit_s`.
- **The price covers a declared allowance of attempts.** The kind `remote` prices a call at
  its price per second times `host_limit_s` times an attempt allowance of **2**: the attempt
  whizbang made and one restart of Modal's. So the example of the plan, 300 millionths of a
  dollar a second and `host_limit_s = 600`, is a bound of 300 × 600 × 2 = 360,000 millionths,
  0.36 dollars, for one call (task R1 builds it).
- **No retry is asked for.** The bridge's function sets no `retries`, or sets it to 0.
- **The claim comes first.** The bridge claims the call's dispatch id in the host's own store
  (on Modal, a `Dict`) as its first act on an input, before any model work. A delivery of the
  same dispatch after the claim is answered from the claim, as a replay, and the model does not
  run again; so a restart after the claim bills its container's startup and idle time and no
  second execution.
- **The residual, recorded as the known limit.** Restarts after a crash or a preemption beyond
  the allowance, crash loops at a container's start among them (a crash before the claim is
  made, as in a model's load inside the container's start), are not bounded by whizbang. A
  ledger can then hold less than Modal bills.
- **The backstop is the owner's.** The owner sets a spend limit on the Modal workspace when
  the host is set up (task D1's checklist, suggested at 50 dollars a month, and a line in
  `docs/guides/gpu-host.md`). It is a monthly cap of the workspace, not a bound of a request,
  and Modal states neither how fast it stops work nor any overrun.

`SPEC.md` section 6 is amended with this: a remote host's bound may be per attempt, with a
declared allowance of attempts, the residual recorded in that host's record.

### The front, and what it supervises

`python -m whizbang_pack.front --listen <host:port> --models-dir <dir> --token-env <NAME>
[--worker local | --worker <module:function>] [--host-limit-s <s>] [--tls-cert <file>
--tls-key <file>] [--redact-env <NAME>]...` serves `pack/1` over HTTP
(`pack/src/whizbang_pack/front.py`). It uses the standard library alone (`socket`,
`selectors`, `ssl`, `subprocess`, `hmac`, `hashlib`, `threading`): the pack's
`dependencies = []` stays. It does on the host what the Rust supervisor does on the Mac.

- **Two sides.** The front holds the connections and the queue. The **worker's side** holds
  the worker process: in the front's own process under `--worker local`
  (`whizbang_pack.front.LocalSide`), or wherever a callable named by `--worker
  <module:function>` puts it (task D1's Modal bridge runs a `LocalSide` in the GPU container).
  The callable is given the models directory and the front's options and returns an object
  with two methods: `call(job, emit)`, which runs one whole queued request and hands what it
  makes to `emit`, and `cancel(dispatch)`, the control input. A job is plain data (the dispatch
  id, the not-after time, and the bytes of the `load` and the request frames), so it crosses to
  another machine as it is.
- **The queue.** At most **4** requests are held by a front, the one with the worker's side
  among them; a fifth is refused `busy` before anything of it is handed on. A turn is the
  front of the queue; one request at a time is handed to the worker's side.
- **A request is taken whole:** whizbang's hello, the `load` frame (model, installation,
  settings) and the request frame, as the local supervisor's queue holds a request
  (`pack/queue.rs`, `Request`).
- **Whether to load is decided where the worker is.** The worker's side compares the
  request's model, installation and settings with what its own worker process holds, keyed to
  that process's id, as the local supervisor keys residency. When they differ it unloads what
  is resident and loads the request's, inside the one turn, so no other caller's load comes
  between. A worker replaced on the card's side is loaded again for the next request.
- **The files are checked before each load.** The worker's side reads the installation's
  record (ADR 0017), holds it to the identity the `load` names, and hashes every file it names
  against the record's size and SHA-256, as `Supervisor::call_checked` does. A file that
  differs refuses the load. An installation with no record cannot be checked and is refused.
- **One ordinary call at a time, admitted beside the worker.** The worker's side holds the one
  lock of ordinary calls; a second while one runs is answered `busy` at once and never takes
  the control input. It gives the worker each call under an id of its own, `<process id>.<count>`,
  so no restart of a front can make one twice, and maps the worker's frames back to the ids the
  request carried.
- **Each request is known by its dispatch id, never by a sender's `pack/1` id.** The header
  `Whizbang-Dispatch` holds 32 lower-case hex digits that the kind `remote` minted from the
  system's randomness. The worker's side holds which dispatch its running call is, so a
  cancel finds it whichever front sends it.
- **The claim.** Before any model work the worker's side refuses a request whose not-after has
  passed (the front stamps it at receipt: the receipt plus `--host-limit-s`), then claims its
  dispatch id in its claim store by an atomic put that fails when the key is there. A claim
  that fails is a replay; a store that cannot be reached refuses the request (failing closed).
  `--worker local` keeps its claims in the process; task D1's bridge uses a Modal `Dict`.
- **The cancel.** A cancel names a dispatch. The front looks in its own queue first: a waiting
  request is taken off and ended `cancelled`, not sent. When the front holds no entry for it,
  or the request is with the worker's side, the cancel goes to the worker's side, which writes
  `cancel` on the worker's pipe at once only if its running call is that dispatch; a late one
  touches nothing and starts no model work. At the cancel's deadline, 5 seconds, the worker's
  side kills the worker; the call ends `cancelled`.
- **Limits of each call,** as the local supervisor's (`pack/worker.rs`): a hello within 60
  seconds, an `unload` within 30, a `load` within 120, a call within 120, a cancel within 5,
  an end within 5. Past one the worker is killed.
- **A restart** is the end of the worker, by any cause. It fails the request in flight and
  every request waiting in that front's queue (`unavailable`, not sent), and replays nothing.
  No worker is started by itself: the next request starts one; after 3 workers in a row that
  ended before they answered anything, none is started for 60 seconds, as the local
  supervisor's rule. A front whose queue another front's request shares with the same
  worker's side fails only its own waiting requests. A cancel that comes before a request's
  frame is written ends it `cancelled`, not sent, and leaves the worker as it is.
- **Error output.** The worker's error output is read line by line by a thread of the worker's
  side, never left in the pipe; each line is cut to 4,096 bytes after the front's token and
  every value of a `--redact-env` variable is taken out of it (each read at up to 8,192 bytes,
  so a secret that begins inside what is kept is found whole), and the last 256 are kept.

### Its routes

| Route | Body | Answer |
|---|---|---|
| `POST /pack/1/request` | With `Content-Length` and `Whizbang-Dispatch`: whizbang's hello, then the `load` frame and the request frame, and nothing after | `200`. Once the request's turn has come and its files are checked: `Whizbang-Model-Files: sha256:<digest>`, then, chunked, the worker's hello under whizbang's conversation id, the `load`'s events and its result or error, the request's events and its result or error (none when the load failed), each frame flushed as it comes; then the outcome record. A failure before the files were checked: `Whizbang-Unverified: failure` and the outcome record alone |
| `POST /pack/1/cancel` | With `Content-Length` of at most 4 KiB and `Whizbang-Dispatch`: whizbang's hello, then a `cancel` frame | `204`, whether or not anything was found. The id in the frame is never used to find a request |
| `GET /pack/1/health` | None | `200` and a JSON document of `pack-host/1`: the protocol, each installation in the models directory with each file's SHA-256 as its record states it and whether this front has checked it, the device the worker last reported. Needs no token, starts no worker and wakes no card |

Every answer closes its connection. Other answers: `401` without the bearer token (compared with
`hmac.compare_digest`), naming nothing; `404` for another path; `405` for another method; `400`
for a missing or malformed `Whizbang-Dispatch`, a body that is not its frames, or bytes after
them; `411` without `Content-Length` (a chunked request body is not taken); `413` for a length
over its bound; `503` when the request handlers are all taken.

**The outcome record** ends every answer of the request route, around the frames and not in
them, so `pack/1` is unchanged: four zero bytes, its mark (no frame begins with them, since a
header's length is never 0, so a reader tells the record from a frame by them), then four
bytes of length and a JSON document of at most 4 KiB,
`{"format": "pack-host/1", "outcome": "complete" | "failed", "kind": <one of the eight faults,
or null>, "admission": "busy" | "replay" | "not_after" | "claim_unreachable" | null, "stage":
"not_sent" | "unresolved" | "answered", "ended": null | {"exited": <code>} | "killed" |
"signalled", "lines": <count>}`; then that many lines of the worker's error output, each four
bytes of length and the line's bytes, at most 256 of at most 4,096 bytes. The lines are those
the worker wrote during the request's turn, redacted. A body that ends without its outcome
record, or with a length over its bound, is `truncated`. The stage is the worker's and is a
diagnostic: what is charged is never read from it (task R1).

**The identity.** The digest of the `Whizbang-Model-Files` header is the SHA-256, as 64
lower-case hex digits, of the lines `<file name> sha256:<64 hex digits>\n`, one for each file
of the installation's record, sorted by the bytes of the name: the files the worker's side
checked before the load that answered. A model that was resident answers with the digest of
the check before its load. Task R1 works out the same digest from the catalogue's entry and
refuses an answer whose header is missing, malformed or different.

### Every phase of a connection has its deadline

One classifier thread holds every new connection in non-blocking mode under `selectors`, and
advances each one's TLS handshake, its request line and headers (at most 16 KiB) and, for a
cancel, its body (at most 4 KiB), only as its bytes arrive, so a ready connection is never
behind a stalled one. Each has **2 seconds** from its acceptance; at most **64** wait at once,
and a sixty-fifth closes the one that has waited longest. Once read, a connection is handed on
by its route: a request to one of **6** request handlers (a seventh answered `503`), a cancel
or a probe to one of **2** control handlers kept for them alone, whose answers are a few bytes.
So a cancel is answered within 2 seconds of its acceptance whatever else is stalled, inside the
client's control send of 5 seconds. A request's body is read at no less than 1 MiB a second
after its first 5 seconds, frame by frame through `pack/1`'s own reader, so each header's
length and each payload's length is checked before their bytes are read and a frame over its
bound closes the connection with the rest unread. Each write of an answer must be taken within
30 seconds. A client gone at any point of its request (a write that fails, or the end of its
connection seen while it waits) cancels it.

What this does not stop: a flood of more than 64 new connections in 2 seconds can push a cancel
out. The front stands behind the host's own proxy, and a cancel lost so leaves its request
charged at its bound, as any lost cancel does.

### TLS

With `--tls-cert` and `--tls-key` the front speaks HTTPS itself (the standard library's `ssl`),
for a host of one's own and the tests. Without them it speaks plain HTTP, for a host whose own
proxy ends TLS, as Modal's endpoint does. The kind `remote` takes an `https` address only.

### Its device

The adapter's `auto` asks for CUDA first where the runtime has it, then the Apple GPU, then the
CPU; `cpu` still forces the CPU (`adapter.device_name`). On CUDA the adapter runs in full
precision with TF32 off (`torch.backends.cuda.matmul.allow_tf32 = False`,
`torch.backends.cudnn.allow_tf32 = False`). A result names `cuda`, and the worker adds the
card's name to the diagnostics of a `load` that put the model on it. The adapters of SAM 3 and
Tiny are unchanged: both ask `device_name`, so the Mac's answers and the records that name
those adapters' sources are as they were.

### The kind `remote`

Task R1 fills this section: the client's way of frames, the kind's config, its identity check,
its price from `host_limit_s` and the attempt allowance, the probe with a body, and the
conformance suite over HTTP.

### What this record leaves

Tiny on a remote host (its entry works there unchanged; nothing asks for it yet); a remote fill
or judge (milestone 8); several models resident on one host (section 4, deferred).

### ADR 0020, "The HTTP codec, on paper", as built

The frames are the same bytes, as decided, with two changes made by building it. The `POST`'s
body holds the `load` frame between the hello and the request, since the request is taken
whole; and the answer holds the `load`'s answer before the request's. Beside the frames, and
outside them, are the dispatch id and identity headers and the outcome record, which ADR 0020
lets a record add around the frames. A cancel is a second `POST` that names the dispatch; the
end of the first connection asks the same. No byte of `pack/1` changed, and its vectors are as
they were.

## Consequences

- The pack can run on a machine whizbang does not run on, and its supervision there is held by
  `pack/tests/test_front.py`, test by test against the duties of the Rust supervisor.
- Two codes keep one set of rules: the Rust supervisor and the front. Each duty has a test on
  each side, and the conformance suite runs over both transports (task R1).
- The bound of a remote call is per attempt, times 2. A call that Modal restarts more than once
  can cost more than its ledger says; the workspace's spend limit is the backstop. Changing the
  allowance is a change to this record and to the kind's price.
- Reversing it means removing the front and the kind `remote`; nothing local depends on them.

## Alternatives

- **A Rust front** (whizbang's Linux binary on the host): turned down; it would ship a Linux
  build to the host for nothing the Python front lacks.
- **Modal's decorators inside the pack**: turned down; the front must run on any host. Only
  task D1's deployment folder names Modal.
- **`http.server`'s threading server**: turned down when built; a thread per connection that
  waits for headers cannot keep a cancel inside 2 seconds while 64 connections stall, so a
  classifier under `selectors` reads every connection's headers instead.
- **Refusing every call on Modal for want of a documented bound per request**: what the plan
  said to do on this finding; replaced by the orchestrator's decision above, a bound per
  attempt with an allowance of 2 and the residual recorded.
