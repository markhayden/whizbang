# 0046. The remote worker: a front that speaks `pack/1` over HTTP and supervises the worker beside a card

Status: accepted.
Date: 2026-11-23
Amended: 2026-11-24, with plan m6, task L1, item 2b, after the live round on Modal. Changed: an
answer of the request route sends its headers at once, before anything waits on the worker's
side; the identity of the files moves from the header `Whizbang-Model-Files` to an identity
record in the body, before the first frame; a failure before the check is the outcome record
alone in the same chunked body, with no `Whizbang-Unverified`; and the front sends a
keep-alive after every 10 seconds of nothing. Added: "The headers at once", with the
measurement that decided it, and the front's log of cancels and workers.
Spec: section 4 ("The model pack": one codec for the local worker and one for HTTP; one model,
one call at a time behind a bounded queue; cooperative cancellation; a restart fails everything
in flight and replays nothing; the device chosen at runtime), section 5 (segment, self-hosted,
milestone 6), section 6 ("Spend": GPU time on a self-hosted backend, as amended by this record),
section 9 (a remote backend reports the model files it runs by hash); plan
`docs/plans/m6-http-and-remote-hosting.md`, "The remote worker", task P1 and task R1 (the
client's section, "The kind `remote`"). Amends ADR 0020, "The HTTP codec, on paper", to "as built", and ADR 0024
with the client's way of frames, its control send and the prober.

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
  among them; a fifth is refused `busy` before anything of it is handed on. Each is held
  whole in memory: a request's image may be up to 200 MB (50,000,000 pixels of four bytes),
  so a full queue can hold a few GB beside the answer of the call in the worker, which is an
  input to the size of the front's container (task D1). A turn is the
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
  `--worker local` keeps its claims in the process; task D1's bridge uses a Modal `Dict`,
  through `front.StoreClaims`, whose put fails when the key is there and whose store, when it
  fails, is `Unreachable`.
- **A worker's side on another machine** (task D1): on the card's machine `front.relay` runs a
  job on a `LocalSide` on a thread of its own and yields what it hands on, so a front that
  goes away in the middle of a call leaves the call to end at its own limits; on the front's,
  `front.Bridged` is the side the front is given, over the host's call of the card's function
  and its control input, and a call whose items end before its outcome ends `unavailable`
  and unresolved. Neither names a host; `deploy/modal/sam3_host.py` hands them Modal's.
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
  every request waiting in that front's queue when the end is told (`unavailable`, not
  sent), and replays nothing; a request that reaches the queue after that is answered by
  the next worker (task R1 found that one which came while the failed turn was being
  cleaned up was failed with the queue, and fixed it,
  `test_front.Supervision.test_a_request_that_comes_after_a_restart_was_told_is_answered_and_not_failed_with_the_queue`).
  No worker is started by itself: the next request starts one; after 3 workers in a row that
  ended before they answered anything, none is started for 60 seconds, as the local
  supervisor's rule. A front whose queue another front's request shares with the same
  worker's side fails only its own waiting requests. A worker that dies between calls is
  replaced at the next call, which then runs, and the queue is not failed; only a death
  during a call fails the call and the queue. A cancel that comes before a request's
  frame is written ends it `cancelled`, not sent, and leaves the worker as it is: the
  worker's side takes the call as its dispatch's before the claim, so a cancel during the
  claim's round trip is kept; it keeps the last 64 cancels of a dispatch not running there
  yet, so one that reaches it before its call does is found when the call begins; and it
  hands a request to the worker's writer under the lock a cancel reads, so a `cancel` frame
  never goes ahead of its request.
- **Error output.** The worker's error output is read line by line by a thread of the worker's
  side, never left in the pipe; each line is cut to 4,096 bytes after the front's token and
  every value of a `--redact-env` variable is taken out of it (each read at up to 8,192 bytes,
  so a secret that begins inside what is kept is found whole), and the last 256 are kept.

### Its routes

| Route | Body | Answer |
|---|---|---|
| `POST /pack/1/request` | With `Content-Length` and `Whizbang-Dispatch`: whizbang's hello, then the `load` frame and the request frame, and nothing after | `200` and chunked, at once once the request is read whole ("The headers at once"), and a keep-alive. Once the request's turn has come and its files are checked: the identity record, then the worker's hello under whizbang's conversation id, the `load`'s events and its result or error, the request's events and its result or error (none when the load failed), each flushed as it comes; then the outcome record. A failure before the files were checked: the outcome record alone. A keep-alive after every 10 seconds of nothing else, anywhere before the outcome record |
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

**The identity record** is marked as the outcome record is, then four bytes of length and the
document `{"files": "sha256:<digest>", "format": "pack-host/1"}` and nothing else; **a
keep-alive** is the mark and a length of 0, eight zero bytes, which no record has.

**The identity.** The digest of the identity record is the SHA-256, as 64
lower-case hex digits, of the lines `<file name> sha256:<64 hex digits>\n`, one for each file
of the installation's record, sorted by the bytes of the name: the files the worker's side
checked before the load that answered. A model that was resident answers with the digest of
the check before its load. Task R1 works out the same digest from the catalogue's entry and
refuses an answer whose identity record is missing before a frame, malformed or different.

### The headers at once

Added by the amendment of 2026-11-24. A web endpoint on Modal answers a request for at most
150 seconds, then redirects, and the kind follows no redirect. As first built, the front sent
an answer's headers only once the card's side had checked the files, which on a cold call
comes after the GPU container's start, the claim and the worker's hello. Measured in task L1
on an A10G: a whole call took 22 to 52 seconds cold as a rule and 2.6 seconds warm, the first
call after a new image 161 seconds, and one cold start about 5 minutes, in which two calls in
a row failed at 150.8 seconds on the redirect and the third took 129 seconds.

So the front sends the headers of every answer of the request route as soon as it has checked
the token and read the request whole, the same headers for every answer, before it waits on
anything of the worker's side; and with them a first keep-alive, a byte of the body, so a
proxy that holds headers until the body begins passes them on. What the headers named that is
known only later is in the body: the files' identity in the identity record, before the first
frame, and every failure in the outcome record, as before. The statuses decided before any
wait stay statuses (`400`, `401`, `404`, `405`, `411`, `413`, `431`, and `503` when the
request handlers are taken); a full queue is decided at once too, and is answered with the
headers and its outcome record (`busy`) as before. No fault changed its kind or its form.

While nothing else is written, the front writes a keep-alive after every `keep_alive` seconds,
10 by default (`--keep-alive-ms`, 1 to 20,000, for a test): under the client's limit between
two reads of an answer of frames, which is the request's own wait, `host_limit_s` (600 on
Modal) and never past the run's deadline, and under its 30 seconds for any other answer, so a
cold start up to the bridge's `startup_timeout` of 300 seconds trips neither. The client
passes over keep-alives anywhere before the outcome record, at most 4,096 of them in one
answer. A write of a keep-alive that fails ends the answer as a client gone, and cancels the
request. Tested by `pack/tests/test_front.py` (`Start`: the headers before a slow side, a
keep-alive while it is quiet, each fault after the headers) and by
`remote::wire::a_cold_host_keeps_its_answer_alive_past_the_clients_limit_between_two_reads`.

An answer of a front as first built (its identity in a header, its frames with no identity
record) is refused whole by this client, and an answer of this front by a client as first
built: the front and the client are deployed from one commit.

**The front's log.** On its error output, a line each, naming the dispatch id and never a
token, an image or a prompt: each cancel it is given and where it went (off the queue, or to
the worker's side); on the worker's side, each cancel frame written to a worker, and how long
after it the call ended and how; each worker started, and each ended, with how. On Modal both
sides' lines are in `modal app logs`, which is the evidence task L1, item 5 reads.

### Every phase of a connection has its deadline

One classifier thread holds every new connection in non-blocking mode under `selectors`, and
advances each one's TLS handshake, its request line and headers (at most 16 KiB) and, for a
cancel, its body (at most 4 KiB), only as its bytes arrive, so a ready connection is never
behind a stalled one. Each has **2 seconds** from its acceptance; at most **64** wait at once,
and a sixty-fifth closes the one that has waited longest. Once read, a connection is handed on
by its route: a request to one of **6** request handlers (a seventh answered `503`), a cancel
or a probe to one of **2** control handlers kept for them alone, whose answers are a few bytes.
So a cancel is answered within 2 seconds of its acceptance whatever else is stalled, inside the
client's control send of 5 seconds. A probe of the health route shares the 2 control handlers
with cancels, so a burst of probes can briefly answer a cancel `503`; the client's control send
treats that as a lost cancel, and the request is charged at its bound as for any other. A request's body is read at no less than 1 MiB a second
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

Built by task R1. Whizbang's side of the front: the client's way of frames, the kind and
its config, its identity check, its price, the probe with a body, and the conformance
suite over HTTP.

**The client's way of frames** (`whizbang_core::egress`, `frames.rs`; the network's in
`whizbang_backends::net`; ADR 0024 as amended). A backend sends a request whose body is
frames through the sender its bounds hand it (`Sender::send_frames`), as it sends pieces.

- **The check sees typed parts, not opaque bytes.** The request (`Frames`) holds a body of
  the backend's (`FrameBody`), which states what it is made of, each text as a piece of
  its class and the picture as its canonical pixels (`Piece::pixels`), and writes its
  bytes from that same value. The kind `remote`'s body is whizbang's hello, the `load` and
  the call, written by `pack/1`'s own codec; its texts are the conversation's id, the
  model, the installation, each setting and each query, as prompts, and its picture is
  the call's image as source images. A call whose image is not a picture of the core (a
  raster whose samples are not its size's) is refused before anything leaves, never sent
  with the image left out of the parts. The checked sender asks of the parts what it asks
  of a send's pieces (the recipient's origin and upstreams, the policy for every class
  they hold, the check of every part and field); a picture's pixels are passed as
  pixels, whatever their bytes spell, as the encoder's PNG is. Only then are the bytes
  written, once, into `CheckedFrames`, which only a check makes (`CheckedFrames::checked`,
  and the checked sender's own) and which is all the network's sender of frames takes.
  The request is in the run's record before anything leaves, taken back only when
  nothing did.
- **The answer is read as it arrives.** The network's sender writes a `POST` of the checked
  bytes with the request's own fields (the kind's `Whizbang-Dispatch`) and its credential,
  and hands the caller the answer's head and a reader of its body, chunked or of a stated
  length, which reads off the wire only as the caller asks: the kind reads it through
  `pack/1`'s codec, so every length is checked before its bytes, and no answer is held
  whole by the client. A fault of the wire under the reader (a timeout, a cancel, a cut)
  is the request's end, whatever the reader made of it.
- **Two limits, not one.** The connect, the handshake and the writing of the body keep the
  client's own limits (`CONNECT_LIMIT`, `BETWEEN_READS_LIMIT`); the wait for the answer's
  first byte and between two of its bytes is the request's own, which the kind sets to
  `host_limit_s`, one attempt, and never past the run's deadline. A host that stalls is
  found then, and is charged at its bound either way. No heartbeat is added.
- **It follows nothing.** A redirect is an answer like another; the kind refuses it, and
  nothing is asked of the address it names.
- **The control send.** Once its socket connected, a request of frames names what it
  reached (`Dispatched`: the origin and the address of its peer), and the checked sender
  keeps the last 64 it dispatched, each with the credential it carried. A control send
  (`Sender::control`, a `Control` of at most 4 KiB) is allowed only to the origin of one of
  them, carries that request's credential and none a backend brings, holds no user data,
  is held to the check of its address and fields, and is noted in the run's record with no
  class. It is made under bounds of its own, `Bounds::for_cleanup` (a cancellation signal
  of its own and a deadline of 5 seconds from now, the run's way out and record), the one
  place a backend is given bounds that are not its run's, since the run's signal is set
  or its deadline spent and the checked sender refuses to send under either. The network
  connects it to the peer the request reached, resolving no name, checks the same name in
  the certificate, and holds it to its deadline from the connect to the last byte of the
  answer's head; the body of the answer is not read. The check is shown the control's
  body as a part of no class, and its fields. Under those bounds the checked sender sends a
  control and nothing else: a send or a request of frames under them is refused, so no data
  of the run escapes its signal or its deadline through them; and a time past what the clock
  holds is no time, never no deadline.

**The kind's config** (`whizbang_backends::remote`), under `[backends.segment]` alone: a
remote fill or a remote judge is refused, naming the capability (milestone 8). `url` is
the front's base address and is `https`, else refused; the routes are appended to it.
`token_env` names the front's token, which goes as `Authorization: Bearer <token>`, bound to
the url's origin; a `key_env` or a `provider` is refused. `model` is a model the catalogue
holds, under an adapter of this build. Settings: `price_microusd_per_second` and
`host_limit_s`, whole numbers from 1, both or neither; and the model's own settings the
kind `pack` takes (`threshold`, `pad`, `min_cover`, `outline`, `device`), each sent in the
`load`. The binary registers the kind over the catalogue it ships and its table of
adapters.

**Its price.** `Price::by_time(price_microusd_per_second, TimeLimit::of_host(ms))` with `ms`
`host_limit_s` × 1,000 × 2 (the allowance of attempts above), the multiplications checked:
one setting without the other, a zero, or a product that does not fit is refused when the
config is read, naming the setting. The plan's example is 300 × 600 × 2 = 360,000
millionths, 0.36 dollars. A table that names neither makes a backend with no price; it
names a recipient, so it has no bound, and every call to it is refused before anything is
sent (ADR 0012). A request refused before anything left is settled at nothing; every
other ends unresolved and is charged at its bound, whatever the outcome record says of
its stage.

**Its identity.** The kind works out the digest above from the files of the installation
`whizbang models get` would make of the catalogue's entry under this build's adapter, and
holds the answer to it before anything of the answer is used: a frame before the identity
record, or an identity record malformed, of another digest or out of its place, refuses the
answer whole, and nothing of it is kept. An answer with no identity record is read as its
outcome record alone, which must be a failure. The backend declares its model at the version `files-` and
the digest's 64 hex digits: not the digest's own text, which a version is taken for an
installation's identity in, and a local model's. So a run's record names the model of the
kind `remote` at the files asked, and the device the worker named, never from a cache; the
digest the answer reported and the request's dispatch id are said beside the response, in
the run's diagnostic of `segment`. The record's version is unchanged.

**Its dispatch id** is 128 bits of the system's randomness (`getrandom`), as 32 lower-case
hex digits, new for each request: whizbang retries none itself. Every failure of a request
leads its sentence with it (`dispatch <id>: …`), and the action `segment` keeps a failure's
kind and sentence in its run's diagnostic (`failed <kind>: …`), so a request charged at its
bound and ended by a timeout or a cancel, whose run keeps no reason, is still found by its id
at the host.

**Its cancel.** When the run's signal is set or the request's wait or deadline passes
before the answer was read whole, the stream is closed (which the front takes as a cancel
too) and whizbang's hello and a `cancel` of the call follow it to `/pack/1/cancel`, naming
the dispatch, through the control send. A host that does not answer it is let go of at its
deadline. A request whose answer was read whole, a cancelled one among them, is not
followed.

**The probe with a body.** `Probed::Reached` carries the start of the answer's body, at most
4 KiB, which the checked prober holds to that bound before it hands it on, refusing a
longer one whole. The network's prober is a `GET` with no credential (a request that
brings one is refused), no body and no user data, which follows nothing; the binary hands
it to `whizbang backends status --probe` in place of the prober that contacted nothing.
For a backend of the kind `remote` it probes `<url>/pack/1/health` and says whether the
host holds the model the config asks: an installation of that model whose files' digest
is the version the backend declares. A backend whose approval does not say `probe = true`
is not probed.

**The conformance suite over HTTP.** The behavioural half is restated over an atomic
request, a `load` and its call together: the local transport sends each through the
supervisor's queue as a `Request`, and the transport over HTTP through the kind's own
production HTTPS client to the fake host, the pack's own front (`python -m
whizbang_pack.front`, the same Python and the same bytes) serving HTTPS on `127.0.0.1:0`
with a certificate of the net client's test PKI. Its worker's side is named by
`--worker fake_host:side` (`crates/whizbang-backends/tests/it/remote/fake_host.py`): the
pack's own `LocalSide`, one for each fake fault a request's model names
(`<fault>--<fake model>`), each starting the pack's fakes with that fault and with limits
the fast tier can wait for. `--worker local` starts only the pack's worker, which takes
no fault: the faults need a side of the test's, made as task D1's Modal bridge makes its
own. A case that observes the worker observes it through what the front reports: the
error the request is answered with, and the outcome record (the end and the error
output). Every case, its local form and its form over HTTP:

| Case | Local form | Form over HTTP |
|---|---|---|
| A result names the device of the model that answered it | `conformance::a_result_names_the_device_of_the_model_that_answered_it_and_the_hello_the_workers_own` | `conformance::the_cases_of_the_fake_models_pass_over_http`: `fake-mps` answers `mps` |
| A segment request is answered with the masks the request determines | `conformance::the_real_pack_with_a_fake_model_answers_a_segment_request_whose_masks_are_those_the_request_determines` | `conformance::the_cases_of_the_fake_models_pass_over_http`: the same masks, in the fixed order |
| A result over its request's bound is refused, as too_large | `conformance::a_result_over_the_bound_its_request_gives_is_refused_before_it_is_built_and_the_call_ends_as_too_large` | `conformance::the_cases_of_the_fake_models_pass_over_http`: the front refuses it, ends the worker and says `too_large`; the codec's half, refused before the planes are read, is the same codec on both sides |
| A payload whose length disagrees with its entry ends the call as malformed, and the next call starts another worker | `conformance::a_malformed_payload_ends_the_call_as_malformed_and_the_worker_is_restarted` | `conformance::the_cases_of_the_fake_models_pass_over_http`: `malformed`, the worker's end in the record, the next request answered |
| A worker that ends inside a call ends it as unavailable and unresolved | `worker::a_request_that_could_not_be_written_is_not_sent_and_one_written_and_unanswered_is_unresolved` | `conformance::the_cases_of_the_fake_models_pass_over_http`: `unavailable`, unresolved, exit code 4 in the record |
| A fault made from a worker's failure quotes nothing of its error output, which is handed on whole | `worker::a_fault_made_from_a_workers_failure_holds_none_of_the_marker_the_host_and_the_token_its_error_output_held` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: the lines cross in the record, the front's secrets taken out (`--redact-env`), and the fault quotes none |
| A request from the worker is answered unsupported before its payload is read | `worker::a_request_from_the_worker_is_refused_before_its_payload_is_read_and_answered_as_unsupported` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: the call ends and the worker is killed, nothing of its request crossing; the front waits for the payload and ends the call at its limit, where the local supervisor refuses before reading it (a row of `docs/carry-over.md`) |
| A refusal whizbang sends is a text the worker's reader takes | `worker::a_text_whizbang_sends_is_cleaned_as_a_text_of_the_protocol_and_the_worker_takes_it` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: the front refuses the worker of another version at its hello, `malformed`, and kills it |
| A request the worker does not read ends at the call's limit as timed_out | `worker::a_request_the_worker_does_not_read_ends_at_the_calls_limit_as_timed_out_and_leaves_nothing_behind` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: `timed_out` at the front's limit of a call, the worker killed |
| A word whizbang writes behind a full input waits no longer than its limit | `worker::a_word_to_a_worker_whose_input_is_full_waits_no_longer_than_its_limit` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: the frame the front refuses ends the request at once, `malformed`, the worker killed |
| A worker of another version is refused at its hello | `worker::a_worker_of_another_protocol_version_is_refused_at_the_hello_and_is_not_left_running` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: `malformed`, killed |
| A worker that says no hello is killed at its limit | `worker::a_worker_that_says_no_hello_within_its_limit_is_killed_and_the_request_ends_as_unavailable` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: killed at the front's limit of a hello; the front says `timed_out` where the local supervisor says `unavailable` |
| A worker that does not end at the end of its input is killed at the deadline | `worker::a_worker_that_does_not_end_at_the_end_of_its_input_is_killed_at_the_deadline_and_waited_for` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: its end is in the record, and the next request is answered |
| A process the worker started keeps neither stop nor drop waiting | `worker::a_process_the_worker_started_that_holds_its_streams_open_keeps_neither_stop_nor_drop_waiting` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: the request ends within the front's limit, and the next is answered |
| A cancelled call that stops ends as cancelled | `queue::a_cancelled_call_that_stops_ends_as_cancelled_and_the_worker_lives_on` | `conformance::the_cases_of_cancels_and_limits_pass_over_http`: the run's signal closes the stream and sends the cancel; `cancelled`, charged at its bound |
| A cancelled call that does not stop has its worker restarted | `queue::a_cancelled_call_that_has_not_stopped_at_its_deadline_has_its_worker_restarted` | `conformance::the_cases_of_cancels_and_limits_pass_over_http`: `cancelled`; once the front's side has ended the call, the next request starts another worker, which answers |
| A worker that floods events has its call ended | `queue::a_worker_that_floods_events_has_its_call_ended_as_malformed_and_is_restarted` | `conformance::the_cases_of_the_workers_faults_pass_over_http`: ended and killed; the front says `too_large` of the event over its bound where the local supervisor says `malformed` |
| A load or an unload past its limit has its worker killed | `queue::a_load_or_an_unload_that_does_not_end_within_its_limit_has_its_worker_killed` | `conformance::the_cases_of_cancels_and_limits_pass_over_http`: `timed_out`, killed, for a load and for the unload a second model's load makes |
| A worker that exits inside a call fails what was in flight, and the next request starts another | `queue::a_worker_that_exits_inside_a_call_fails_what_was_in_flight_and_the_next_request_starts_another` | `conformance::the_cases_of_the_fake_models_pass_over_http`: as the case of a worker that ends inside a call, and the next request answered |

None is dropped, and none passes by a load made up for it. The three tests over HTTP share
a host each and take about 6 seconds of the fast tier's 300 together.

### What this record leaves

Tiny on a remote host (its entry works there unchanged; nothing asks for it yet); a remote fill
or judge (milestone 8); several models resident on one host (section 4, deferred).

### ADR 0020, "The HTTP codec, on paper", as built

The frames are the same bytes, as decided, with two changes made by building it. The `POST`'s
body holds the `load` frame between the hello and the request, since the request is taken
whole; and the answer holds the `load`'s answer before the request's. Beside the frames, and
outside them, are the dispatch id header, the identity record, the keep-alives and the
outcome record, which ADR 0020 lets a record add around the frames. A cancel is a second `POST` that names the dispatch; the
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
