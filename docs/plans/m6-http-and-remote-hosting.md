# Plan: milestone 6, HTTP and remote hosting

Draft 27, 2026-11-23, written from `main` at 9a2d397f (the tag `m5`, the merge of #283, the
note of milestone 5) and brought up to 77bc1597 (the merge of #284, CI's limits as the owner
decided on 2026-11-22), with the owner's answers of 2026-11-23 to its four questions, after twenty-five outside review rounds ("Review rounds", at the end); amended the same day
for the owner's later decision 5, which replaces its yes to hosting SAM 3's files.
Milestone 6 is `SPEC.md` section 16, row 6, "HTTP and remote hosting": "User story 1 runs:
the engine API with its negative tests, SAM 3 on a GPU host with parity inside the 1D
tolerance, Bria through fal". Beside the row it takes section 7's "MCP over HTTP" (new,
milestone 6), section 5's cell "segment, self-hosted" (milestone 6), success criteria 3 and
8 of section 18, every carry-over row that names milestone 6 (the table "Carry-over rows
owned by milestone 6", below), and what `docs/plans/m5-note.md` says milestone 6 owes: the
engine API with its negative tests, SAM 3 on a GPU host within the 1D tolerance, a real
prober, and the rows of a store and an executor held for longer than the playground holds
them. Its first task is the entry check that the note of milestone 5 left: the two fast tests
that time out under load and the Chrome case that failed once.

Milestone E keeps running beside this milestone, as decided on 2026-11-20
(`docs/plans/m4-note.md`). It is not planned here; the two share no source file but
`docs/carry-over.md` ("Shared files").

**Words this plan uses.**

- **The engine API** is the second HTTP surface of `SPEC.md` section 10, "Two HTTP
  surfaces": `whizbang serve --http <addr>`, the MCP server of ADR 0031 spoken over HTTP, with
  plain routes beside it for pictures in and out and for a run by its id. It is not the
  playground API and shares no route, token, guard or listener with it; it shares the code
  that reads and writes HTTP/1.1 (H1, item 2).
- **The app** is user story 1's hosted image editor: a program that calls the engine API. It
  writes no model code. In tests the app is a test's own client; live, it is a short script
  of the orchestrator's (L1).
- **The GPU host** is where SAM 3 runs off the owner's Mac. It has two parts: **the front**,
  a small HTTP server in the pack that speaks `pack/1` over HTTP (ADR 0020, "The HTTP codec,
  on paper") and needs no card, and **the worker** behind it, the pack's own worker with SAM 3
  on an NVIDIA card. Whizbang reaches the front through a new backend kind, **`remote`**. The
  first GPU host is Modal ("What the owner decided"); nothing in the code names Modal but its
  deployment folder (D1).
- **The fake host** is the real front, run on loopback in a test with the pack's fake models
  behind it: the same Python, the same bytes, no card and no weights.
- **Live** means a real provider or a real card: only the orchestrator runs it, on the owner's
  Mac (L1). No builder makes a live call, and nothing in CI does.

## What the owner decided

1. **The GPU host is Modal** (the owner's decision of 2026-11-22, as the orchestrator passed
   it on). Modal is an approved recipient for this milestone, and its spend counts against the
   milestone's 100 dollars. The origins it brings, each written exactly into the approvals
   once it is known:

   | Origin | Who sends to it | What it receives |
   |---|---|---|
   | `https://<workspace>--whizbang-sam3-front.modal.run`, the front's endpoint, its exact name read from the deploy's output (Modal names a web endpoint by the workspace, the app and the function) | Whizbang, through the kind `remote`, under the config's `egress` table | `source_images` and `prompts` (the picture, and the words, point or box of a segment request), and the token, bound to that origin |
   | `https://api.modal.com`, Modal's control plane, as its command-line client is configured by default (confirmed by the orchestrator at the first deploy, and any other address the client uses written into the record) | Modal's own command-line client on the owner's Mac, run by `scripts/gpu-host.sh` (D1), never by whizbang | The deployment's sources (the pack and the lock, from a clean checkout), SAM 3's files for the volume (decision 4), and the account's token, which the client reads itself |

   Whizbang's own egress table gains the front's origin alone; the control plane is the
   client's traffic, like `git` to GitHub, and is named here so that the owner has approved
   it. The account, its payment method and its token are the owner's (decision 2). Tasks that
   use fakes come first; nothing needs the account before L1.
2. **The Modal account and its tokens: yes** (the owner's answer of 2026-11-23). The owner
   sets up the workspace for whizbang and its payment method; runs `modal token new` once on
   the Mac, which stores Modal's token in Modal's own file in the owner's home folder, where
   the orchestrator's commands use it through Modal's client and no agent opens it; and makes a
   random token of at least 32 characters for the front, puts it in the whizbang env file as
   `SEGMENT_TOKEN`, and gives the same value to Modal as the secret `whizbang-front`, typed by
   the owner. The owner is told when L1 needs them ("What the owner still does").
3. **PyTorch's Linux CUDA build in the host's image, and there alone: yes** (2026-11-23).
   The host's image installs PyTorch's Linux CUDA build and the NVIDIA CUDA packages it pulls
   in, under NVIDIA's own licences, from pypi.org and files.pythonhosted.org; not in the
   release archive, not in the Mac's lock, and no local Linux models for users (D1, item 1).
4. **SAM 3's files copied from the owner's Mac into a Modal volume: yes** (2026-11-23). The
   host needs no Hugging Face sign-in, and the owner's token never leaves the Mac (D1, item 3).
5. **SAM 3's files hosted for customers: no, for now** (the owner's decision of 2026-11-23,
   which replaces the yes given earlier the same day). Whizbang hosts no copy of SAM 3's files.
   A customer who runs SAM 3 locally downloads it from the maker's page on Hugging Face, as
   today, with their own sign-in. A copy of whizbang's own may be added later if that becomes
   a barrier: the carry-over row "Hosting the model's files for customers" stays open, with no
   milestone yet. The owner considers the licence and distribution question settled and does
   not reopen it. A customer who does not want to run SAM 3 locally hosts their own with the
   repo's GPU-host deployment (D1) and points the kind `remote` at it; whizbang's own Modal
   deployment (decisions 1, 2 and 4) is for testing. No whizbang account on Hugging Face, no
   upload, and no change to SAM 3's catalogue entry (W1).

## What the spec says, and where it is silent

**What it says.** Each of these binds the tasks below.

- Section 1, the doors: "The same server over HTTP (`whizbang serve --http`)", for "A hosted
  app that uses whizbang as its engine". User story 1: "An app lets people upload an image,
  name or click a thing to remove, and download the result. The app server runs `whizbang
  serve --http` with no GPU and no Python. Segmentation runs on SAM 3 on a GPU host; the fill
  runs on Bria through fal. The app calls two or three actions and writes no model code."
- Section 3: "MCP types stay inside `serve`"; the execution contract (request ids that
  outlive a restart; "Finding a run" after its caller has disconnected; status and cancel
  responsive while a run executes; "A parent becomes terminal only when its descendants are
  terminal and every dispatched request is accounted for"); "One writer".
- Section 4: the config example of `[backends.segment] kind = "remote"`, with `url`,
  `token_env` and `model`; "Credentials: named in config and given only to the backend that
  needs them"; the pack's protocol has "one codec for the local worker and one for HTTP, each
  the simplest that meets the bounds"; the worker holds one model and runs one call at a
  time behind a bounded queue, cancellation is cooperative and a worker that does not stop is
  restarted, "A restart fails everything in flight … Nothing that had been dispatched is
  replayed"; "Device chosen at runtime: GPU when available, CPU as the fallback, recorded in
  the run".
- Section 5: segment, self-hosted, milestone 6. "Platforms": the runtime extra is the Mac's
  alone (ADR 0021, amended), and the owner deferred a Linux runtime because production runs
  models on an external GPU host (`docs/carry-over.md`, "Linux runtime extra").
- Section 6, Spend: "GPU time on a self-hosted backend has a configured price per second. Its
  bound must come from a limit the host itself enforces, or a fixed maximum charge; a timeout
  on whizbang's side is not a bound." "A timeout does not prove a remote host stopped
  billing, so it is charged at its bound." Readiness: reachability is an explicit probe.
- Section 7, "Doors and releases": "MCP over HTTP | New | Milestone 6".
- Section 9: "A remote backend reports the model files it runs by hash. A run records the
  requested identity and the reported one. Results from a backend that cannot report its
  identity are not cached durably."
- Section 10, the engine API: loopback by default, another address needs a flag; a bearer
  token from the environment or a file; Origin refused unless listed; TLS required off
  loopback, at a reverse proxy, built-in TLS deferred; local controls not exposed; file paths
  from the caller refused, "images arrive as bytes or by reference to an upload". Both
  surfaces: limits on request size, image dimensions and concurrent calls; tokens redacted.
  Negative tests: "an unauthenticated call, a wrong Host, a cross-origin request, an oversized
  image, and for the engine API a path". Egress: default deny, every hop checked, credentials
  never forwarded, metadata removed before sending, every run records where data went; "one
  GPU host for SAM 3 from milestone 6. Adding a recipient is the owner's decision."
- Section 13: the fast tier holds "the real pack speaking the real protocol with fake
  models"; the model tier "Real models through the pack"; one test 30 seconds, all 300; "a
  budget is not raised without the owner".
- Section 18, item 3: "The segment conformance fixtures give the same instances, inside the
  tolerance recorded at checkpoint 1D, from the local worker and from a remote one." Item 8:
  "User story 1 runs end to end."
- ADR 0027: the tolerance is an IoU of 0.99 for each pair in order, scores within 0.01, the
  same counts. ADR 0020: the HTTP codec on paper; and "A new field … is another version",
  while "sections on what the supervisor does around the frames" may be added without one.
  ADR 0010: `Price::by_time(micros_per_second, TimeLimit)` with `TimeLimit::of_host(ms)` is a
  bound of price times limit; with `own_timeout` an estimate only. ADR 0012: a backend that
  names a recipient and no price has no bound and is refused. ADR 0019: a probe carries no
  credential and no user data.

**Where it is silent, or where the code is short of it.** Each is settled in "Choices this
plan makes", is a task's item, or is an open question; none is settled silently.

1. How MCP is carried over HTTP, how a picture reaches a call, how a result leaves, and how a
   run is found and cancelled once its caller has gone. The stdio server names pictures by
   store identity only (`crates/whizbang-serve/src/tools.rs`, `identity_schema`), and it ties a
   cancel to its one client's JSON-RPC ids (`server.rs`). Settled in "The engine API".
2. No backend kind `remote` exists: the binary registers `pack` and `service` alone
   (`crates/whizbang/src/wiring.rs`, `kinds()`), though the config already reads `url` and
   `token_env`. No config key states a price per second. Settled in "The remote worker".
3. The network client reads an answer whole, up to 16 MiB (`egress/ways.rs`,
   `REPLY_MAX_BYTES`), and its sender's body is pieces with pictures as base64; ADR 0020 asks
   for a body of frames and a streamed answer. R1, item 2.
4. A probe answers a status alone (`Probed::Reached { status }`), and the build has no real
   prober (`backend_status.rs`, `NoProber`). R1, item 5.
5. The worker's hello names no model file (`pack/proto.rs`, the hello's fields are `type`,
   `id`, `version`, `role`, `device`, `threads`, and a new field is a new version). Settled in
   "The remote worker", "Its identity".
6. The queue, the load before a call, the cancel's deadline and the restart are the Rust
   supervisor's (ADR 0020, "The worker"); the Python worker refuses a request while busy
   (`worker.py`). A front on another machine must do the supervisor's part there. P1.
7. The adapter chooses the Apple GPU or the CPU (`adapter.py`, `device_name`); it never asks
   for CUDA. P1, item 4.
8. The GPU host runs Linux with CUDA, and the pack's lock holds the runtime for the Mac alone.
   A host's image is not a user's install, and it pulls NVIDIA's packages. Decision 3;
   D1.
9. Where the host's copy of SAM 3's files comes from. The owner's Hugging Face sign-in is for
   testing only; whizbang hosts no copy for customers, who download from the maker or host
   their own. Decisions 4 and 5; D1.
10. What a store held by a server for months needs. The carry-over rows say: a generation file
   that reaches 64 MiB at about 39,000 runs, a table of abandoned runs with no bound, a
   parent's wait with no bound, file systems on which locks do not hold, uploads no run took.
   S1.

## Choices this plan makes

Each is recorded in an ADR written before code depends on it: ADR 0045, "The engine API"
(H1); ADR 0046, "The remote worker" (P1 writes it; R1 adds its client's section), which
amends ADR 0020's "The HTTP codec, on paper" to "as built" and ADR 0024 with the client's
new way; ADR 0047, "The generation in buckets" (S1).

### The engine API

**MCP's streamable HTTP transport, on one path, written by hand.** `POST /mcp` takes one
JSON-RPC message as its body. **One revision is offered over HTTP, 2025-06-18**, the latest the
stdio server speaks and the first whose transport carries its version in a header: 2024-11-05
had another HTTP transport, and 2025-03-26 asks a receiver to take batches, which would hold a
connection for several runs at once and need rules of their own for capacity and reconnecting;
an app that is written for the engine API speaks the one revision. `initialize` answers
2025-06-18 whatever the client asks, as MCP lets a server, and every request after it carries
`MCP-Protocol-Version: 2025-06-18` or is refused 400 (one with no header is not taken as
2025-03-26, which is not offered), so a batch, which 2025-06-18 does not have, is refused 400
too. `initialize`, `ping`, `tools/list` and `tools/call` are answered. A `tools/call` is
answered as an event stream (`text/event-stream`): each progress event as the run goes, as
`notifications/progress` under the call's `progressToken` when it gave one and not at all when
it gave none (as over stdio), then the result, then the stream ends. Whether or not a token was
given, the run's id is in the answer's header `Whizbang-Run`, sent before the body, and in the
result's `_meta`, so a caller can find and cancel the run once the stream is gone. Every other
request is answered as one JSON body.
`notifications/cancelled` is taken and answered 202 and cancels nothing: a stateless server
cannot tell one client's JSON-RPC ids from another's, and MCP lets a receiver ignore it; a
run is cancelled by its id (below). `GET /mcp` is answered 405. The server makes no
`Mcp-Session-Id`. The door, the tools and how a call becomes a call of an action are the
stdio server's (ADR 0031, the `Door`; `whizbang::serving::Actions`), unchanged: one definition
of the actions serves both, as section 3 asks. Rejected: a library (`rmcp`, a web framework,
an async runtime) for the reasons ADR 0031 and ADR 0041 gave; sessions, which would be state
to bound and expire for nothing the stateless form lacks; SSE on `GET`, which needs a held
connection per client.

**Plain routes beside MCP.** A JSON-RPC message is held to 1 MiB (ADR 0031), and a picture is
bigger; and a run must be found and cancelled after its stream has gone. Behind the same
guard and token:

| Route | Takes | Answers |
|---|---|---|
| `POST /v1/uploads` | A picture's bytes (`image/png` or `image/jpeg`, the two the core decodes; any other type, WebP among them, refused 415), or a mask as a grey PNG with `?mask_of=<image identity>` | `{"image": "sha256:…"}` or `{"mask": "sha256:…"}`: the identity a call names. Imported through the playground's path (`import.rs`); the picture's metadata is not kept |
| `GET /v1/artifacts/<identity>` | Nothing | The artifact's bytes: a picture as PNG, a mask as its grey PNG; 404 for one the store does not hold |
| `GET /v1/runs/<run id>` | Nothing | The run's state, its outcome and its outputs' identities, as `whizbang runs show --json` gives them, whether it is running here or ended |
| `POST /v1/runs/<run id>/cancel` | Nothing | 202 and `stopping`, or the run's end if it has one. A run this server did not start is answered 409: one process owns every run it starts (ADR 0031) |

A call names its pictures by identity, as over stdio: "by reference to an upload" (section 10).
No path is taken anywhere: any string argument that reads as a path (`/` or `~` first, a drive
letter, `file:`, a `..` part) is refused 400, the playground's rule applied to every argument
of every tool.

**What user story 1 calls.** Named: an upload, `remove_prompt` with the picture and the words,
a download of its result. Clicked: an upload, `segment` with `kind = "point"`, the click as
`query` and `max_instances = 1`, so its one output is the best mask; then `fill` with the
picture and that mask; a download. (`remove` takes a location, not a mask:
`crates/whizbang-rascal/recipes/remove.toml`. `fill` paints the mask as `segment` found it,
without `remove`'s growing and feathering; a task that shapes a given mask first is not in the
build, and is proposed as a carry-over row for milestone E by D1 rather than built here.) Two or
three actions; no model code (`docs/guides/engine-api.md`, D1).

**Who may call** (ADR 0045; the checks in this order, the first that fails answers and
closes):

| Check | Refused with |
|---|---|
| The line and headers fit in 16 KiB, read within 5 seconds | 431, 400 or closed |
| `Host` is one of the server's names at its port: `127.0.0.1`, `localhost` and `[::1]` on loopback, and each `--host-name` | 421 |
| `Authorization: Bearer <token>`, compared in constant time | 401, naming nothing |
| An `Origin`, when sent, is one listed by `--allow-origin`; none is listed by default. A request with no `Origin` (a server's own client) is taken | 403, with no allow-origin header |
| The path is one of the routes; no field in the address but `mask_of`; JSON at most 1 MiB and 16 deep; a picture at most 64 MiB, of one of the two types | 404, 400, 413 or 415 |
| A picture decodes inside the engine's pixel limit (25,000,000 by default, `--pixel-limit`); a mask is exactly its image's size | 413, naming the limit |
| No string argument reads as a path; every artifact argument is an identity | 400 |
| A new run is started only while fewer than 4 run (`--calls`); at most 2 uploads are read at once | 429, after the headers |
| A body arrives at no less than 1 MiB a second after its first 5 seconds; a plain answer is taken within 10 seconds; a stream's next event within 30 | closed |

The limit of 4 counts runs started, not streams: following a run again (a repeat by request
id, looked up before any handler is taken, which takes over that run's one stream) and the run
routes are answered whatever it is, so an app reconnects at capacity. The
browser's `Sec-Fetch-Site` rule of the playground is not applied: the caller is a server, not
a page. The token is never in an answer, a log, an event, a record or an error.

**Where it listens.** `--http <addr>`. A loopback address needs nothing else. Any other is
refused at start unless `--off-loopback` is given with at least one `--host-name`; the guide
says that off loopback the server stands behind a reverse proxy that ends TLS (section 10;
built-in TLS deferred). The server cannot see the proxy, so the flag is the operator's
statement, and the start line says so.

**The token.** `--token-env <NAME>` or `--token-file <path>`, exactly one, on loopback too. A
token is at least 32 characters of printable ASCII. A token file its group or anyone can read,
or another user's, is refused at start. Read once; a changed token needs a restart.

**Local controls are not exposed.** The tools are the description's actions, as over stdio,
and nothing else: no model install or removal, no spend reset, no config, no store command.
`tools/list` holds none, a call naming one is refused as an unknown tool, and no plain route
reaches one.

**Runs, reconnecting and stopping.** One process owns every run it starts, in a table of its
runs that is not tied to any connection, held to the executor's bound on started runs and to
4,096 ended runs remembered, oldest let go first (the playground's `REQUESTS_REMEMBERED`); a
run let go of is still read from the store by `GET /v1/runs/<id>`. Each run has a stated time
(`--wait-s`, 900 by default), at which it is cancelled and given up on after the grace, as
over stdio. A `tools/call` with a `request_id` whose run exists follows that run, so an app
whose stream dropped calls again with the same id and gets the run, never a second fill
(section 3); a closed stream cancels nothing. A repeat is looked up through the executor's
own request-id lookup, with its durable fingerprint, before readiness or preparation
(`contract/engine.rs`), never through the server's table: so after a restart, after the table
let the run go, and after the run was pruned, the same request is answered with its run and a
changed request under the same id is a conflict (section 3), unlike the playground, which
answers a forgotten id by the id alone (ADR 0041). `SIGTERM`, `SIGINT` and `SIGHUP` stop the server
as the playground's Quit does: every run cancelled and waited for up to the grace, the ledger
accounted, the store dropped (`signal-hook`, already in the binary).

**The store.** The server holds it as the one writer for as long as it runs. Uploads that no
run took are removed once older than `--uploads-kept-h` (24 by default), through the store's
collection (S1, item 7), once an hour (D1, item 5).

### The remote worker

**The front supervises the worker, on the host, in Python.** `python -m whizbang_pack.front
--listen <addr> --models-dir <dir> --token-env <NAME> --worker <how>` serves `pack/1` over
HTTP and does there what the Rust supervisor does on the Mac (ADR 0020, "The worker"): it
starts the pack's worker as a child process over the same pipes and frames; holds a bounded
queue of 4 requests in front of it and refuses a fifth as `busy`; takes each request whole, as
the local supervisor's queue does (`pack/queue.rs`, `Request`: the model, its installation, its
settings and the call), and when the resident model, installation or settings differ from the
request's, unloads and loads with the request's settings before the call, inside that one turn
of the queue, so no other caller's load comes between; one request at a time; sends a cancel on, and after the cancel's deadline kills and
restarts the worker; on the worker's death or restart, fails the request in flight and every
queued one with a clear error, and replays nothing; and verifies the installation's files
against the hashes of its record before each load, as `Supervisor::call_checked` does. The
pack gains no dependency (`dependencies = []` stays: `socket`, `selectors`, `ssl`,
`subprocess`, `hmac`, `hashlib`; as built, ADR 0046). `--worker local` runs the child on the same machine; `--worker <module:function>`
names a callable that runs it elsewhere and carries the same frames, which is how D1's Modal
deployment keeps the front on a machine with no card and the worker on one with a card.
The front speaks HTTPS itself when given `--tls-cert` and `--tls-key` (the standard library's
`ssl`), which a host of one's own and the tests use, or plain HTTP behind a host's own TLS
proxy, which Modal's endpoint is; the kind `remote` takes an `https` address only, as the
client does (`net/client.rs` refuses any other before it connects). Rejected: a Rust front
(whizbang's Linux binary on the host for nothing the Python lacks); Modal's decorators inside
the pack (the front must run on any host).

**Its routes** (ADR 0046):

| Route | Body | Answer |
|---|---|---|
| `POST /pack/1/request` | whizbang's hello, then the `load` frame (model, installation, settings) and the `request` frame of one queued request, in that order, with a `Content-Length`: frames `pack/1` already has, in the order the local supervisor sends them | Streamed: the worker's hello, the request's events as they happen, then its `result` or `error`; the body then ends. Then, always, **the outcome record** (below). A body that ends without its outcome record is `truncated`. The header `Whizbang-Model-Files` names the files the answer came from (below) |
| `POST /pack/1/cancel` | whizbang's hello, then a `cancel` frame, with the header `Whizbang-Dispatch` naming the request to cancel | 204. The front cancels the request of that dispatch id alone, whether queued in its own queue or running beside the worker (which is asked when the front holds no entry, as after the front was replaced); one that has ended, or was never seen, touches nothing. The `pack/1` id in the frame is never used to find it |
| `GET /pack/1/health` | None | 200 and a small JSON document, `pack-host/1`: the protocol, the models the volume holds with each file's SHA-256 as verified, the device the worker last reported. Needs no token, starts no worker and wakes no card |

**The outcome record.** What the local supervisor knows beside the frames (why a call
failed, the stage it reached, a worker's end and its exit status, and the worker's last lines
of error output, which are outside the frames, ADR 0020) is told over HTTP by one record after
the last frame of every answer, around the frames and not in them, so `pack/1` is unchanged:

- **Its head**: four bytes of length, then a JSON document of `pack-host/1` of at most 4 KiB:
  `outcome` (`complete` or `failed`), and for a failure its `kind`, one to one with the core's
  `FaultKind` (`capability/fault.rs`: `cancelled`, `unsupported`, `credential`, `rejected`,
  `unavailable`, `timed_out`, `malformed`, `too_large`), so a bad or oversized frame the front's
  supervision caught is told as the local supervisor tells it; apart from it, `admission`, the
  front's own reason when it never handed the request on (`busy`, `replay`, `not_after`,
  `claim_unreachable`), or null; its `stage` (how far the request got toward the worker);
  `ended`, null while the worker lives on, or one of `{"exited": <code>}`, `"killed"` and
  `"signalled"`, as `pack/worker.rs`'s `Ended` has them; and the count of error-output lines
  that follow; a `complete` outcome carries the count too, since a worker that lives on may
  have written error output.
- **Its lines**: that many lines of the worker's error output, each as four bytes of length and
  the line's raw bytes, at most 256 lines of at most 4,096 bytes, the supervisor's own bounds
  (`pack/worker.rs`), so up to about 1 MiB and no escaping; each line is redacted of the front's
  token and of every secret the front was given before it is written. Every length is read and
  checked before its bytes, and one over its bound ends the answer as `truncated`.

The kind `remote` reads it under those bounds and gives back the same `Failed` the local
supervisor gives (its kind, stage, `ended` and `error_output`), so a worker that ended and a
stream that broke are told apart: the first has its record, the second has none and is
`truncated`. **What is charged is not read from the record's stage**: the stage is the
worker's, and is kept as a diagnostic; for the ledger, a request whose body began to leave for
the host is charged at its bound whatever the record says (the host may have started a machine
and billed it), and only a request refused before anything left (the client's own refusals,
which are `NotSent` today) is settled at nothing.

**Its identity.** The hello of `pack/1` is not changed (a new field would be `pack/2`); the
identity is in the HTTP around the frames, which ADR 0020 lets this record add. The front
answers each request with `Whizbang-Model-Files: sha256:<digest>`, the digest of the sorted
list of the model's file names and hashes it verified before the load that answered; the kind
`remote` works out the same digest from the catalogue's entry for the model asked and refuses
an answer whose digest is missing, malformed or different, before anything of it is used, and
an answer so refused is never cached (section 9). **A failure before the files were verified**
(the front's own refusal, `busy`, a replay, a stale not-after, an unreachable claim store, or a
check of the files that failed) carries no such header and no frame: it is answered with the
header `Whizbang-Unverified: failure` and the outcome record alone, which the kind takes only
as a failure, bounded and redacted as every record is; such an answer can never be a result,
is never cached, and is charged as every sent request is (at its bound). Any frame in an
answer without a verified identity refuses the whole answer. The run records the identity asked, the
digest reported and the device.

**Its device.** The adapter's `auto` asks for CUDA first where the runtime has it, then the
Apple GPU, then the CPU; `cpu` still forces the CPU; on CUDA the adapter runs in full precision
with TF32 off (`torch.backends.cuda.matmul.allow_tf32 = False`,
`torch.backends.cudnn.allow_tf32 = False`), and the worker reports `cuda` and the card's name.

**The kind `remote`** (`whizbang_backends::remote`), for `segment` in this milestone:

```toml
[backends.segment]
kind = "remote"
url = "https://gpu-host.example"
token_env = "SEGMENT_TOKEN"
model = "sam3"

[backends.segment.settings]
price_microusd_per_second = 300
host_limit_s = 600
```

- `url` is the front's base address; the routes are appended to it (`<url>/pack/1/request`,
  `<url>/pack/1/cancel`, `<url>/pack/1/health`), so a front served under a path is named with
  that path and no more.
- The recipient is the origin of `url`, which the `egress` table must approve with
  `source_images` and `prompts`; a point or a box is a prompt. The credential is the token,
  bound to that origin and sent nowhere else.
- **Its price** is `Price::by_time(price_microusd_per_second, TimeLimit::of_host(ms))`, where
  `ms` is `host_limit_s` times 1,000 times the attempt allowance of **2** (below), converted
  with checked multiplications: zero, or a value that overflows, is refused when the config is
  read, naming the setting. With the example, a request's bound is 300 × 600 × 2 = 360,000
  millionths, 0.36 dollars
  (`remote::the_examples_bound_is_360000_microusd_and_an_unknown_end_settles_at_it`).
- **What `host_limit_s` must cover, and how it was settled.** ADR 0046's section "What bounds
  a request's bill on Modal" (P1, item 1) read Modal's own documentation on paper, first, and
  found that **Modal gives no bound for a request**: each attempt's startup, execution and idle
  time have documented limits, but a GPU function can always be preempted and is then started
  again on the same input with no stated count, and a crash of a deployed app is retried
  without end. So, by the orchestrator's decision of 2026-11-23, under the owner's standing rule
  that cost controls are a convenience guard and not a hard production control (keep work
  running, record the limit), **whizbang bounds one attempt**: `host_limit_s` is the sum of the
  GPU function's `startup_timeout`, `timeout` and `scaledown_window`, D1's deployment sets the
  three, and `scripts/gpu-host.sh deploy` reads them back and refuses when any is missing or
  their sum differs from the config's. The price covers **2 attempts** (whizbang's, and one
  restart of the host's). The bridge asks for no retry (`retries` unset or 0) and claims its
  dispatch id before any model work, so a delivery after the claim is answered from the claim
  and runs no model. **The residual**, restarts after a crash or a preemption beyond the
  allowance, crash loops at a container's start among them, is not bounded by whizbang; ADR 0046
  records it as the known limit, and the owner's Modal workspace spend limit, set at D1's
  setup (suggested 50 dollars a month), is the backstop. A timeout, a dropped stream or an
  unknown end is charged at the bound.
- **Nothing dispatched is replayed by the host.** The id of a `pack/1` request is unique only
  among one sender's open requests (ADR 0020; the supervisor's `r1`, `r2`), so it is not what
  the host keys on, and nor is a run's id, which is the time and a count of one process
  (`contract/run.rs`, `RunId::minted`), so two processes can mint the same one. The kind
  `remote` mints a **dispatch id** for each request it sends: 128 bits from the system's
  randomness (`getrandom`, already in the lock file, a new dependency of `whizbang-backends`
  alone), written as hex, sent in the header `Whizbang-Dispatch` and kept in the run's record
  beside the request; a retry of whizbang's own is a new dispatch with a new id, and the host's
  own redelivery carries the same one. The front stamps each request at its receipt with a **not-after** time, the receipt
  plus `host_limit_s`, and hands both to the bridge. Before any model work the bridge
  **claims** the dispatch id in the host's own small store (on Modal, a `Dict`) by an atomic
  put that fails if the key is there, and ends the request with an error naming the replay if
  the claim fails, if the store cannot be reached (failing closed), or if the not-after has
  passed. Markers live far longer than a request may (a `Dict`'s entries last days without use,
  a request at most `host_limit_s`), so a marker gone stale can belong only to a request whose
  not-after has passed, which is refused anyway.
- **Cancellation**: when the run's signal is set, or its deadline passes, the backend closes
  the stream and sends the `cancel` through a checked control send of its own (R1, item 3): it
  is allowed only for a request already dispatched to that origin, carries the same credential
  bound to the same origin and no user data, and has its own short deadline (5 seconds) and a
  signal of its own, because the run's are already set or spent and the checked sender refuses
  to send under either (`net/client.rs`). A host that does not answer it is let go of at that
  deadline; the request is charged at its bound in any case.
- **No Python on the app's side.** The binary starts the pack's worker only for a `pack`
  backend; a config whose segment is `remote` and whose fill is fal starts none (D1, item 4).
- This, the call cap and the store's allowance are the whole cost control. Nothing else is
  built.

## Acceptance: the row's clauses, each with its task and its evidence

| Clause | Task | Shown by |
|---|---|---|
| The engine API | H1 | `serve_http::*` in the fast tier: each revision's exchange, a call streamed, a repeat by request id after a dropped stream, a cancel by run id, uploads and downloads, a signal's stop |
| Its negative tests | H1 | `engine_guard::*`: the five of section 10 (unauthenticated, wrong Host, cross-origin, oversized image, a path) and the rest of "Who may call" |
| SAM 3 on a GPU host | P1, R1, D1, L1 | `test_front` (the front's supervision); the conformance suite over HTTP against the fake host; `remote::*`; L1's live runs, every one reporting `cuda` |
| With parity inside the 1D tolerance | L1 | `remote_parity::sam3_answers_each_fixture_inside_the_tolerance_from_a_remote_worker`, three rounds of each fixture, every pair's IoU at least 0.99 and its scores within 0.01, `docs/results/m6-remote-sam3.md` |
| Bria through fal | D1, L1 | The fill of user story 1 through the engine API: fal's fake in the fast tier, live in L1 |
| User story 1 runs | D1, L1 | `story::a_hosted_editor_uploads_names_and_clicks_and_downloads_with_no_local_worker` (fast tier); the live run (`docs/results/m6-user-story-1.md`) |
| Section 18, item 3 | R1, L1 | The conformance suite's behavioural half over HTTP; L1's parity |

## How the work is split

Nine pull requests. Sizes are changed lines of source, tests and hand-written documents, by
milestone 3's rate (about 2,500 for a large one); generated reference and lock files apart.

| Pull request | What | Builds on | Size |
|---|---|---|---|
| A0 | The entry check: the two fast tests that time out under load, the Chrome case that failed once | `main` | Small: about 300 |
| P1 | The pack's side: the front and its supervision, `pack-host/1`, CUDA in the adapter; ADR 0046 | A0 | Large: about 2,000 (front 500, supervision 600, device 150, tests 750) |
| S1 | A store and an executor held for months: the carry-over rows; ADR 0047 | A0 | Large: about 2,400 (buckets 900, the executor's rows 450, collection 250, the rest 150, tests 650) |
| H1 | The engine API: `whizbang serve --http`, its guard, its routes, its negative tests; ADR 0045 | A0; merged after S1 | Large: about 2,500 (shared HTTP 300, guard 400, MCP over HTTP 600, routes 350, lifecycle 250, tests 600) |
| R1 | Whizbang's side: the client's frames way, the kind `remote`, its identity and price, the probe with a body, the real prober, the conformance suite over HTTP | P1 | Large: about 2,300 (client 450, kind 650, probe and prober 350, tests 850) |
| D1 | The GPU host's deployment and lock, the hourly collection, user story 1 on fakes, the guides | R1, H1, S1 | Medium: about 1,600 |
| W1 | Tiny's licence text; the catalogue's revisions and the fixtures they stale | P1 | Small: about 400 |
| L1 | The orchestrator's live round: the host up, parity, user story 1 | D1, and the owner's setup of decision 2 | Medium: about 600 |
| N1 | The note, the boxes, the carry-over rows, the spec's wording owed | L1, and W1's box | Small: about 500, no code |

**Waves.** Each from `main` once what it builds on has landed.

1. **A0** alone: the entry check, before anything else changes the fast tier.
2. **P1, S1 and H1** side by side, with one order: **H1 merges after S1**, so that
   `whizbang serve --http` never holds a store before the store refuses a file system whose
   locks do not hold (`docs/carry-over.md`, the row on the store's locks: "before `whizbang
   serve --http` holds a store"). H1 is built beside S1 and merges `main` once S1 has landed. P1 is the pack alone (`pack/src/whizbang_pack/front.py`, `adapter.py`, `sam3.py`,
   `pack/tests/`). S1 is the core's store and contract (`crates/whizbang-core/src/store/`,
   `contract/`) and the binary's `store.rs`. H1 is `whizbang-serve` and the binary's
   `serve.rs`, `serving.rs` and `main.rs`; its tests use the test kit's fake segmenter.
3. **R1 and W1**, when P1 has landed (W1 needs P1's candidate mode and comes after P1's
   fixtures; it may land later, at any time before N1). R1: `whizbang-backends` (`net/`, a new `remote/`), the core's
   `egress/ways.rs` and `egress/chains.rs` (the probe's body), and in the binary only
   `wiring.rs`'s `kinds()` and `backend_status.rs`.
4. **D1**, when R1, H1 and S1 have landed.
5. **L1**, when D1 has landed and the owner's setup of decision 2 is done. W1 changes `tiny`'s
   entry and fixtures alone, not SAM 3's, which L1's parity measures against.
6. **N1**.

**Shared files, each with one owner in a wave.** `docs/carry-over.md`: each task changes only
the rows the table "Carry-over rows owned by milestone 6" gives it; the second and third to
land in a wave merge `main`; N1 writes the rest. `crates/whizbang/src/main.rs`: H1 alone
(S1's flag is on `store prune`, whose arguments are in `store.rs`). `wiring.rs`: S1 in wave 2
(the inventory's secrets, its item 6), R1 in wave 3. `crates/whizbang/examples/`: H1 adds
`engine_fixture.rs`; D1 adds the remote kind to it.
`crates/whizbang-core/src/egress/`: R1 alone; S1 touches `store/` and `contract/` only. The
playground's `crates/whizbang-serve/src/play/http.rs`: H1 alone, which moves its generic part
to `crates/whizbang-serve/src/http.rs` for both surfaces. `pack/`: P1 alone in wave 2; R1 only
reads it. `catalogue.json` and `fixtures/parity/segment/`: P1 in wave 2 (its fixtures), W1 in
wave 3, after it. The `justfile` and `.github/workflows/`: A0 in wave 1;
then R1 alone, if the fake host's tests need a recipe. `docs/reference/`: whoever changes the
description runs `just docs`; the second to land runs it again. This plan's own file: a task
changes only its own section and box.

**Seams.** Each pull request names where it splits if it passes about 2,500 changed lines: P1,
the device (item 4); S1, the generation in buckets (item 1), which then lands first; H1, the
plain routes (item 5); R1, the real prober (item 5); D1, none. A split is said in the
hand-back and the owner is told the count. Nothing of a row is dropped to keep the count.

**Review rule.** Every pull request is reviewed whole by a reviewer that did not write it, and
merged by the orchestrator on green CI with `just scan-range` over `base..head`. What each
reviewer reads hardest: P1, that every duty of the Rust supervisor has its Python twin with a
test, and that the front holds every bound of `pack/1` before it holds a body; S1, that a
store of the old format still reads, that nothing a run uses is collected, and that no parent
becomes terminal before its children; H1, the guard's order, that no route is reached past it,
that no local control is reachable, and that the token is in nothing the server writes; R1,
that the token goes to the backend's origin alone, that an answer is held to `pack/1`'s bounds
before it is held in memory, that no identity means no cached answer, and that no price means
no run; D1, that no secret, account name or private path is in a tracked file; W1, that
`tiny`'s licence file is pinned by the hash of the text the binary carries and that its
fixtures and evidence moved with its revision.

**Live runs are the orchestrator's.** No builder makes a provider call, loads a real model, or
deploys to a host. The orchestrator makes two kinds: P1's model tier on the owner's Mac, which
spends nothing (item 5 there), and L1's.

**Spend.** The allowance is 100 US dollars; milestone 6 has spent nothing. Its live runs come
to about 8.50 dollars ("Spend"), Modal's among them.

## A0. The entry check (small)

### Goal

The fast tier and the Chrome tier are as trustworthy at the start of milestone 6 as their
limits say, before wide pull requests add to them (`docs/carry-over.md`, "Tests that time out
or fail at random at the end of milestone 5").

1. **The two fast tests that time out under load.**
   `models::crashes::a_crash_at_any_event_of_a_first_install_leaves_it_not_installed_or_whole_part_3_of_4`
   (and its sibling `…after_a_refused_step_of_an_install_leaves_the_previous_or_the_new_whole_part_3_of_4`)
   and `contract::execute::many_runs_from_two_executors_at_one_instant_are_all_kept`: each timed
   alone and under a full `just check` on the owner's Mac, five times each, and on both CI
   runners from their reports; then each is split into parts, or made cheaper, so that each
   part finishes inside a third of the 30 seconds under that load. The 30-second limit of one
   test stands (`SPEC.md` section 13): no test's limit is raised, and if a test cannot be
   brought inside it without testing less, A0 stops and asks the owner. A split part of a sweep
   joins `crash-sweeps`.
2. **The Chrome case.** `compare.spec.ts: two pictures wider than half the canvas both fit
   side by side, at one zoom` waits until both pictures are decoded and laid out, by
   Playwright's polling `expect`, never a time. It is then run 20 times in a row on the macOS
   runner: one commit adds a step to `e2e (macos-15, shard 1)` that runs that case alone with
   `--repeat-each 20` and no retry, inside that shard's 120 seconds (the Chrome tier's limit is
   240 in all and 120 a shard in CI, the owner's decision of 2026-11-22, `scripts/e2e.sh`), its run is named in the pull request, and the next commit
   takes the step out before the review.
3. **The timing report** of the pull request names each suite's seconds before and after.

- [x] Done when: no part of either fast test passes 10 seconds alone or under a full `just
  check` on the owner's Mac in five runs, with no limit raised; the Chrome case passed 20 runs
  in a row on `macos-15` (the run named); the carry-over row is closed with the numbers.

## P1. The pack's side: the front, its supervision, CUDA (large)

### Goal

The pack can run SAM 3 on a machine other than the one whizbang runs on, behind a front that
speaks `pack/1` over HTTP and keeps every rule section 4 gives the worker, and the adapter uses
an NVIDIA card where there is one.

1. **ADR 0046, "The remote worker"**, written first: "The remote worker" of "Choices this plan
   makes" whole (the front and its supervision, the routes, `pack-host/1`, the identity header,
   the device, the kind, its price and what `host_limit_s` must cover), ADR 0020's HTTP codec
   moved from "on paper" to "as built" with a section on the front's supervision, and the
   client's section left for R1 to fill. Its first section, written and reviewed before the
   rest, is **"What bounds a request's bill on Modal"**, from Modal's public documentation
   alone (no account, no call): each limit Modal enforces on a function's billed time (startup,
   execution, idle), whether each holds across Modal's own retries after a crash and its
   preemptions, whether a card that is not preempted can be asked for, and the overrun Modal
   states for each, each with its citation and the date it was read. It ends in one of two
   sentences: "a request's bill is bounded by the sum of these limits", with the deployment's
   settings that make it so; or "Modal gives no bound for a request", naming the phase. **It
   ended in the second** (read 2026-11-23: restarts after a crash or a preemption have no
   stated count), and the orchestrator decided the same day, in place of refusing every call,
   that whizbang bounds one attempt, prices 2, and records the residual ("The kind `remote`",
   above); the section says so, and `SPEC.md` section 6 is amended with it. It names what it leaves: Tiny on a remote host (its
   entry works there unchanged; nothing asks for it yet), a remote fill or judge (milestone 8),
   several models resident on a host (section 4, deferred).
2. **The front** (`pack/src/whizbang_pack/front.py`), on the standard library's `socket` and
   `selectors` (as built: a classifier thread reads every new connection, then 8 handler
   threads, 6 for requests and 2 for control, take them, ADR 0046): the three routes; the bearer token read once from the variable named and compared
   with `hmac.compare_digest`; a body is parsed as it is read, through `pack/1`'s own reader
   (`proto.py`) over a stream held to the body's `Content-Length` and to its deadline, so each
   frame's header length and each payload's length is checked before their bytes are read, as
   ADR 0020 asks over HTTP too, and a frame over its bound closes the connection with the rest
   unread; the answer streamed in chunks, flushed
   after each frame; the answer's status and headers sent once the request's turn has come and
   its files are verified, and nothing added to the worker's frames, so `pack/1`'s budgets of
   events and diagnostics are the worker's alone; a client gone mid-answer cancels its
   request; the identity header; the
   health document, which reads the volume's installation records and the hashes they hold,
   and starts no worker. **Every phase of a connection has its deadline**, as the playground's
   host has (`play/http.rs`): a TLS handshake and the line and headers (at most 16 KiB) within
   2 seconds of the connection's acceptance in all (as built; the classifier below), a body at no less than 1 MiB a second after its first 5, each frame of
   an answer taken within 30 seconds, and the connection closed at any of them. **Reading a
   request is apart from serving it, and never waits on a stalled peer**: one classifier
   thread holds every new connection in non-blocking mode under the standard library's
   `selectors`, and advances each one's TLS handshake and its line and headers only as its
   bytes arrive, so a ready connection is never behind a stalled one. Each has 2 seconds from
   its acceptance; at most 64 wait at once, and a sixty-fifth closes the one that has waited
   longest. A control route's body (a cancel's hello and frame, at most 4 KiB) is read by the
   classifier too, within the same 2 seconds, so only a whole, checked control message is
   handed on. Once read, it is handed on by its route: a request route to one of 6 request
   handlers (a seventh answered 503), a cancel or a probe to one of 2 control handlers kept for
   them alone, each of whose answers is a few bytes written within 1 second. So a cancel is answered within 2 seconds of its acceptance whatever else is
   stalled, inside the control send's 5 (R1). What this does not stop, a flood of more than 64
   new connections in 2 seconds, is a line in ADR 0046: the front stands behind the host's own
   proxy, and a cancel lost so leaves its request charged at its bound, as any lost cancel
   does.
3. **The front's supervision**, as "The remote worker" says: the child worker over pipes, the
   queue of 4, each request taken whole (its `load` and its call) with a load whenever the
   model, installation or settings differ, the cancel and its deadline, the kill and restart,
   the failure of everything in flight on a death or a restart with nothing replayed, and the
   check of the files before each load. `--worker local` and `--worker <module:function>`, the
   second given the frames in and handing the frames out, so that a test (and D1's deployment)
   can put the worker elsewhere. **Whether to load is decided where the worker is**: the front
   hands the whole queued request (its `load` and its call) to the side that holds the worker,
   and that side compares the request's model, installation and settings with what its own
   worker process holds, keyed to that process as the local supervisor keys residency to the
   worker's process id (`pack/queue.rs`), so a worker replaced on the card's side is loaded
   again for the next request and a front that stayed never skips the load. The second claims each request's dispatch id before any model
   work and refuses a replay, a stale not-after or an unreachable claim store, as "The remote
   worker" says. **Each request is known by its dispatch id, never by a sender's `pack/1`
   id**, and **a call is admitted, and named, beside the worker, not in the front**: the side
   that holds the worker (in process under `--worker local`, the bridge on the card under
   Modal) holds the one lock of ordinary calls and gives the worker each call under an id of
   its own (a count of that side's, with the worker's process id, so no front's restart can
   make one twice); a second ordinary call while one runs is refused `busy` at once and never
   takes the control input. **That side also holds which dispatch id its running call is**, so
   ownership outlives any front: the front finds a cancel, a client gone mid-answer and a
   cancel's deadline by the dispatch id alone, in its own queue first, and when it holds no
   entry for that id (a front started after the one that took the request) it hands the cancel
   to the worker's side through the control input, which cancels its running call only if that
   call is the dispatch named; a late cancel, after the call ended or its worker was replaced,
   finds nothing there either, touches nothing and starts no model work. **A cancel reaches the running call by a path of its own**: the side that
   holds the worker takes a control input beside the call (on Modal, a second input of the same
   one container, which D1 allows), which writes the `cancel` frame on the worker's pipe at
   once; at the cancel's deadline that side kills and restarts its worker, failing what was in
   flight, and replays nothing; the front holds the queue, so nothing waits on the card's side. `--tls-cert` and `--tls-key`.
4. **CUDA in the adapter**, in `adapter.py`'s `device_name` and one setting beside it, so that
   `sam3.py` and Tiny's adapter change as little as can be: `auto` takes CUDA, then the Apple
   GPU, then the CPU; `cpu` forces the CPU; on CUDA, full precision with TF32 off; the worker
   reports `cuda` and the card's name. The Mac's answers are unchanged.
5. **The model tier, by the orchestrator, before P1's last check** (no spend: the Mac's GPU).
   The fast tests hold committed evidence to the digests of the adapters' sources
   (`crates/whizbang/tests/it/evidence.rs`, `sam3.py` and `owlv2_sam2.py` among them), and ADR
   0027 makes the parity fixtures stale when an adapter's revision moves. So for each record
   whose sources P1 changed, the orchestrator runs its own runner on P1's branch on the owner's
   Mac (`scripts/heavy.sh model-evidence scripts/model-evidence.sh` and `scripts/test-models.sh
   parity` and `coverage`, under the memory watch), hands the builder the files they write, and
   the builder commits them with the sources in one commit. The runners refuse a checkout with
   changes, and the sources cannot be committed first, since the evidence test fails on them;
   so P1 gives each runner **a candidate mode**, `--candidate`: it measures the staged tree,
   exported from the index (`git write-tree`, then `git archive` of that tree into a new
   folder, which creates no commit), refuses unstaged changes and untracked files under
   `pack/` or `crates/`, and records that tree's id and the digests of the sources it measured,
   so that the record, committed with exactly those sources, is the record of that commit's
   tree (`scripts/tests/test-model-evidence.sh` gains the cases: a staged change measured, an
   unstaged one refused, the tree id recorded). **The candidate is what runs**: the mode
   builds the binary from the exported tree and installs that tree's pack with it (`whizbang
   setup --extra torch`) into a new models directory of its own, the installed SAM 3 and Tiny
   linked in from the owner's checked installation, before any model test; and it refuses to
   measure unless the pack's sources the worker launches from are, file by file, the digests it
   records (a case of the script test: a pack installed from other sources is refused); the parity measurement of ADR 0027 is taken again if a revision
   moved, and must give the same tolerance or better. P1's files for this: the records under
   `docs/results/` that name those digests and `fixtures/parity/segment/`.
6. **The tests** (`pack/tests/test_front.py`, `test_adapter_device.py`, under `just
   test-pack`'s 60 seconds, the fake models and a fake runtime for the device):
   `test_front.Routes.test_a_request_streams_the_hello_its_events_and_its_result`,
   `test_front.Routes.test_without_the_token_every_route_but_health_is_refused`,
   `test_front.Routes.test_a_body_over_its_bound_is_refused_unread`,
   `test_front.Routes.test_health_needs_no_token_starts_no_worker_and_names_the_hashes`,
   `test_front.Routes.test_a_client_gone_mid_answer_cancels_its_request`,
   `test_front.Identity.test_each_answer_names_the_digest_of_the_files_it_came_from`,
   `test_front.Outcome.test_every_answer_ends_with_its_outcome_record_and_a_worker_end_carries_its_status_stage_and_redacted_error_output`,
   `test_front.Supervision.test_a_fifth_request_is_refused_busy`,
   `test_front.Supervision.test_a_request_for_another_model_unloads_the_first_and_loads_it`,
   `test_front.Supervision.test_an_ignored_cancel_restarts_the_worker_at_its_deadline`,
   `test_front.Supervision.test_a_hung_load_is_ended_at_its_deadline_and_the_worker_restarted`,
   `test_front.Supervision.test_a_worker_that_dies_fails_the_request_in_flight_and_the_queue_and_replays_nothing`,
   `test_front.Supervision.test_after_a_restart_the_next_request_is_answered`,
   `test_front.Supervision.test_a_changed_file_is_refused_at_the_next_load`,
   `test_front.Supervision.test_two_clients_with_other_settings_each_get_a_load_with_their_own`,
   `test_front.Supervision.test_after_a_restart_a_request_is_loaded_with_its_own_settings`,
   `test_front.Bridge.test_a_request_the_bridge_has_begun_is_refused_when_it_comes_again`,
   `test_front.Bridge.test_two_clients_whose_pack_ids_are_the_same_have_other_dispatch_ids_and_both_run`,
   `test_front.Cancel.test_with_two_clients_using_r1_a_cancel_of_the_queued_one_leaves_the_running_one`,
   `test_front.Cancel.test_a_late_cancel_after_the_call_ended_or_after_its_worker_was_replaced_touches_nothing`,
   `test_front.Bridge.test_a_new_front_while_the_old_ones_call_is_blocked_has_its_call_refused_busy_its_cancel_reach_the_old_call_by_dispatch_id_and_the_next_call_answered`,
   `test_front.Bridge.test_the_cancel_route_of_a_replaced_front_reaches_the_old_call_and_a_late_one_starts_no_model_work`,
   `test_front.Bridge.test_a_client_restarted_with_its_ids_counted_from_one_again_is_not_taken_for_a_replay`,
   `test_front.Bridge.test_of_two_deliveries_of_one_dispatch_at_once_one_runs_and_one_is_refused`,
   `test_front.Bridge.test_a_request_past_its_not_after_is_refused_whatever_the_claim_store_holds`,
   `test_front.Bridge.test_an_unreachable_claim_store_refuses_the_request_before_any_model_work`,
   `test_front.Bridge.test_a_cancel_reaches_a_call_blocked_in_the_bridge_through_its_control_input_and_an_ignored_one_restarts_the_worker_at_its_deadline`,
   `test_front.Tls.test_the_front_serves_https_with_the_cert_it_is_given`,
   `test_front.Deadlines.test_incomplete_headers_a_trickling_body_a_stalled_handshake_and_a_client_that_reads_nothing_are_each_closed`,
   `test_front.Deadlines.test_with_every_request_handler_taken_and_sixty_four_connections_stalled_in_their_handshake_or_headers_a_cancel_made_after_them_is_answered_within_2_seconds_of_its_acceptance`,
   `test_front.Deadlines.test_a_ready_connection_is_read_while_others_are_stalled`,
   `test_front.Deadlines.test_two_cancels_stalled_in_their_bodies_leave_a_whole_third_cancel_to_reach_the_worker_within_2_seconds`,
   `test_front.Start.test_the_headers_follow_the_check_of_the_files_and_a_failed_check_is_an_error_answer_with_no_frame`,
   `test_front.Framing.test_a_frame_header_over_its_bound_inside_a_permitted_body_is_refused_with_the_rest_unread`,
   `test_front.Framing.test_a_payload_length_that_disagrees_with_its_header_is_refused_before_its_samples_are_read`,
   `test_front.Bridge.test_a_gpu_worker_replaced_under_a_front_that_stays_loads_again_for_the_next_request_and_replays_nothing`,
   `test_adapter_device.Device.test_auto_takes_cuda_then_mps_then_cpu`,
   `test_adapter_device.Device.test_cpu_forces_the_cpu_where_cuda_is_there`,
   `test_adapter_device.Device.test_on_cuda_tf32_is_off_and_the_card_is_reported`.

- [ ] Done when: those tests pass under `just test-pack`, with the pack's other tests
  unchanged; the evidence records and fixtures of item 5 are written again by their runners and
  the fast tests that hold them pass; ADR 0046 is merged, its section on Modal's bound ending
  in one of its two sentences, with its client's section marked as R1's.

## S1. A store and an executor held for months (large)

### Goal

`whizbang serve --http` holds one store and one executor for as long as the app runs: weeks,
thousands of runs a day. Every carry-over row written for "before `whizbang serve --http` holds
a store" is answered as its "Done when" asks, or narrowed with a line.

1. **The generation in buckets** (carry-over, "every publication writes the whole
   generation"; ADR 0047, written first). Every name of a namespace that grows with use moves
   out of the one generation file into 256 bucket files, each an object of the store named by
   its digest: the names by run, the request ids' names, the ledger's names (several for each
   paid attempt, `spend/name.rs`, which no prune removes, as section 3 says), and the names of
   imports and outputs outside runs. A name's bucket is the first byte of the SHA-256 of the key
   it belongs to (its run id, request id, attempt or artifact name), so its lookup and its rules
   of retention are as before. The generation file keeps the store's few fixed names and the
   256 digests, so it no longer grows with the store. A publication writes the buckets it
   changed and the generation file. A store of the present format is read
   as it is, and its first publication by a writer of this build writes the new format whole,
   once; a build before S1 refuses the new format by its version, naming it, rather than
   misread it. Recovery, prune and the removal of older generations (ADR 0036) hold for bucket
   objects as for every other, and **a publication removes the bucket objects it superseded**
   that no held generation and no publication in progress names, as ADR 0036's writer removes
   older generation files, so the store keeps the buckets of the current generation and of the
   held ones and no others; a prune is not needed for it.
   `store::buckets::a_thousand_publications_leave_only_the_current_and_held_generations_buckets`
   measures the bucket bytes kept after 1,000 publications, with an older generation held and
   then let go. Measured by hand,
   `store::volume::a_hundred_thousand_runs_publish_within_the_measured_bound`, on a store made
   as the engine API leaves one: each run with its request id, an upload, its outputs, its
   record and journal, and a paid attempt's ledger names; the time of one publication and the
   bytes written at 10,000 and 100,000 such runs, and the size of the generation file at both,
   which must be the same; recorded in the row. The test of 2,000 ids in CI is kept, and
   `store::buckets::the_generation_file_is_the_same_size_at_100_and_at_2000_paid_runs` holds the
   flat root in CI.
2. **A parent's wait, and a panic** (carry-over, "What running an action does not do yet", the
   part left). **A parent's wait inside the core is not given a time, with a line:** the
   contract makes a parent terminal only when its descendants are terminal and every dispatched
   request is accounted for (section 3; `contract/engine.rs`), so a parent whose child never
   comes back cannot end without breaking that. What a door gives up is its caller's wait: the
   stdio server and the engine API cancel at the stated time and answer the caller after the
   grace (ADR 0031), and the family keeps its place, counted under the executor's bound on
   started runs. That bound is what keeps a server from growing:
   `execute::a_server_whose_runs_never_return_refuses_new_starts_at_its_bound_and_grows_no_further`.
   The process's end leaves such a family to the next open, which ends it interrupted, as
   today. **A panic** in the executor's own code is caught at one protected boundary that both
   ways of running an admitted run pass through, the synchronous one (`Engine::execute`, then
   `end`, which a child called from inside a parent's action takes, `contract/nested.rs`) and
   the started one (`execute_and_let_go`, which calls `run_and_end` itself and bypasses both,
   `contract/engine.rs`), around the shared `run_and_end`, keeping `execute_and_let_go`'s order
   of letting the executor go and telling the waiters; and not only at a started thread's edge, so a child's panic is caught before it unwinds into its parent's
   `catch_unwind` around `Action::run`. **What the catch does depends on how far the run's
   ending had got**, which the run keeps where the unwinding cannot lose it: before its ending
   is visible, the run's descendants are cancelled and waited for and its requests accounted as
   for any failed run, and it ends as an error of the kind `panicked` with its record, and only
   then does its parent see a failed child; once its ending is visible and not confirmed
   durable, it is left as such an ending is today (item 3, the marker), never told as
   confirmed; once its ending is confirmed durable, the committed outcome stands, and the catch
   only finishes what was left (the unused allowance returned once, the parent and the waiters
   told), never settling again, in a build
   whose panics unwind, which the release profile's are:
   `execute::a_panic_in_the_executors_code_ends_the_run_as_panicked_after_its_children`,
   `execute::a_panic_outside_a_synchronous_childs_action_ends_the_child_as_panicked_before_its_parent_sees_it`,
   `execute::a_panic_in_a_run_entered_through_start_ends_it_as_panicked_with_its_children_and_requests_accounted_before_its_waiter_is_told`,
   and, each through both ways in, a panic injected after the ending is visible and before it
   is confirmed (`execute::a_panic_between_visible_and_durable_leaves_the_ending_unconfirmed`)
   and after it is durable and before the parent is told
   (`execute::a_panic_after_a_durable_ending_keeps_the_committed_outcome_and_tells_the_parent_once`).
3. **Abandoned runs held to a bound** (carry-over, "A run whose end became visible and was not
   confirmed is not let go of"). The heavy part of such a run (its live state, its events, what
   it kept of its calls) is let go of once it is abandoned; a small marker is kept in its
   place, the run's id and which of the two it is (an admission not confirmed, or an ending not
   confirmed), so that a wait or a cancel of it still answers as today, never as a confirmed
   end. Markers are held to 4,096; a store whose writes failed that often is not healthy, and
   past the bound the executor refuses new admissions, naming the cause and saying to restart.
   The next open of the store reads what the store holds, as today.
   `store::an_abandoned_runs_marker_answers_wait_and_cancel_as_unconfirmed_after_its_state_is_dropped`
   (both kinds), `store::past_4096_markers_new_admissions_are_refused_naming_the_cause`,
   `interrupted::after_a_marker_the_next_open_reads_what_the_store_holds` (both kinds).
4. **Locks that do not hold** (carry-over, "The writer lock and a reader's hold are file
   locks", the part left). **The policy:** a store, and a models directory, live on a local file
   system; a network one is not supported, since its locks may hold on one machine alone and
   its readers' holds and prune's lock differ (`store/fs.rs`). **It is enforced in two ways,
   neither needing a system call beyond the standard library's:** (a) when a store is opened,
   the mount that holds its folder is looked up in the system's own table of mounts (on Linux
   `/proc/self/mountinfo`; on macOS the output of `/sbin/mount`, read as text), and a store on a
   file system of a network type (`nfs`, `nfs4`, `cifs`, `smbfs`, `afpfs`, `webdav`, `9p`,
   `ceph`, `glusterfs`, `fuse.sshfs`, and any `fuse.` type the table cannot name further) is
   refused with `Error::NetworkFileSystem`, naming the type; a table that cannot be read refuses
   the store to a writer, saying so; (b) after a writer's lock is taken, the same lock is asked
   through a second opening of `LOCK`, and where it is granted the store is refused with
   `Error::LocksDoNotHold`. A file system that holds locks on one machine alone and is not
   recognised by type is the line's: such a store is unsupported, as the guides say.
   `store::a_store_on_a_mount_of_a_network_type_is_refused_naming_the_type` (each type, from
   mount-table fixtures of both systems), `store::a_local_mount_is_taken`,
   `store::an_unreadable_mount_table_refuses_a_writer`,
   `store::a_file_system_that_grants_a_second_writers_lock_is_refused` (a file system of the
   test kit's that grants every lock).
5. **Masks from a caller** (carry-over, "A mask costs its three planes in memory"). Decided,
   with a line: a mask that arrives through the engine API is held to its image's size and to
   the engine's pixel limit (25,000,000 by default: 75 MB of planes), and at most two uploads
   are read at once (H1), so a server's memory for masks being read is at most 150 MB by
   default; the core's limit for other doors is unchanged.
6. **The models directory read once** (carry-over, "Where the models directory is … is read
   once"). The directory is kept, with a line: a server is restarted for a changed `[models]
   dir`, as the playground is; the engine API's guide says so, and
   `serve_http::a_changed_models_dir_is_seen_after_a_restart_and_not_before` (H1's file) holds
   it. **The secrets are told at every ask:** the source reads the env file again at each
   snapshot (`config/source.rs`), while the inventory keeps the secrets it was made with
   (`wiring.rs`); so the snapshot now hands the inventory its secrets with the offers
   (`Offers::over`), and a key added to the env file after the start is kept out of a model's
   name at the next ask as one given at the start is.
   `wired::a_key_added_to_the_env_file_after_the_start_is_refused_as_a_models_name_at_the_next_ask`
   and `wired::a_key_given_at_the_start_is_refused_as_a_models_name`, with made-up keys.
7. **The store's collection of what no run uses** (carry-over, "A shape drawn on a picture …
   and a picture sent to it are kept as an import keeps an artifact"). `whizbang store prune
   --unused-imports-older-than <hours>` removes imported artifacts (the playground's pictures
   and locations, the engine API's uploads) that no run took and that are older than the age,
   in one publication; and `Store::collect_unused_imports` does the same for a writer that
   holds the store, which the engine API calls (D1, item 5).
8. **A lookup that admits nothing** (for H1's repeats at capacity): `Executor::look_up`, public,
   given a request's id and the request, answers `Missing`, `Repeat(run)` or `Conflict`, by the
   same durable, fingerprinted lookup `start` makes first (`contract/request.rs`), with its
   checks of secrets, and admits, reserves and starts nothing. H1 calls it before it takes a
   handler. `request::look_up_admits_nothing_and_answers_missing_repeat_and_conflict_after_a_restart_and_a_prune`.

- [ ] Done when: `store::buckets::a_store_of_the_old_format_reads_and_its_first_publication_writes_the_new`,
  `store::buckets::a_publication_writes_only_the_buckets_it_changed`,
  `store::buckets::the_generation_file_is_the_same_size_at_100_and_at_2000_paid_runs`,
  `store::buckets::a_thousand_publications_leave_only_the_current_and_held_generations_buckets`,
  `store::buckets::a_build_before_buckets_refuses_the_new_format_by_its_version`,
  `store::buckets::a_crash_at_any_step_of_the_first_bucket_publication_leaves_one_whole_generation`
  (a committed fixture of the present format, written by the present build before S1 and kept
  under `fixtures/store/`, holding artifacts, request ids and ledger names, not one made by the
  new writer; the crash swept through every step of the first publication, the pointer's and the
  removal of what it superseded among them, as the store's crash sweeps do; after each, the
  store opens to one whole generation, every request id and ledger name is there, and what a
  held snapshot names is kept),
  `store::volume::a_hundred_thousand_runs_publish_within_the_measured_bound` (by hand, its
  numbers in the row), the tests of items 2 to 4,
  `store::collect::an_import_a_run_took_stays_and_one_none_took_past_its_age_goes` and
  `store::collect::an_import_taken_by_a_run_started_during_the_collection_stays` and item 8's
  test pass; ADR
  0047 is merged; each row is closed or narrowed with its line.

## H1. The engine API: `whizbang serve --http` (large)

### Goal

A hosted app can call whizbang's actions over HTTP, upload its pictures, download its results
and find or cancel its runs, and nothing that is not that app can do anything at all; and the
server holds its store and its runs as the stdio server does, for as long as it runs.

1. **ADR 0045, "The engine API"**, written first: "The engine API" of "Choices this plan makes"
   whole, with the routes, the revisions offered and the checks in their order. It names what
   it rejects (above) and what it leaves: built-in TLS (deferred), sessions, a push channel,
   more than one token, a picture given inline in a job (carry-over, "Two things of a squint
   job file", below).
2. **The HTTP both surfaces read** (`crates/whizbang-serve/src/http.rs`): the part of
   `play/http.rs` that reads a request under its bounds and deadlines and writes an answer,
   moved with no change of behaviour; and one addition, an answer written as a stream, a frame
   at a time, each within its deadline. Each surface keeps its own guard, token, routes and
   listener. The playground's seventeen negative tests run unchanged.
3. **The guard** (`crates/whizbang-serve/src/engine/guard.rs`): the checks of "Who may call",
   one function every route goes through, the route table taking only guarded handlers, as ADR
   0041 does; the token in a type whose `Debug` and `Display` print nothing. **Reading a
   request is apart from serving it**, so that nothing stalled before its route is known can
   take a handler: the acceptor hands each new connection to a pool of 16 readers, which read
   its line and headers and, for the run routes and every `POST /mcp`, its whole body (at most
   1 MiB, the JSON limit), parse it and know its method, its tool and its request id, within 2
   seconds of its acceptance for the line and headers and, for the body, at no less than 1 MiB
   a second after that, so a whole message of 1 MiB is read within 3 seconds of its acceptance
   (an upload's body is not read here: an upload is handed to an upload handler after its
   headers); at most 16 connections are being read at
   once, and a seventeenth closes the one that has waited longest (its socket shut from the
   acceptor's side, which ends the reader's blocked read). Only then is it handed on by its
   route, to 12 handlers held to their shares: 4 for streams, 2 for uploads, 4 kept for the run
   routes and cancels alone, 2 for other short requests, so a stalled stream or upload never
   takes the run routes' place either. **One stream a run**: a `tools/call` that names a
   request id is looked up first, through `Executor::look_up` (S1, item 8), before any handler
   is taken, so a fresh id at capacity makes no run and no reservation; a repeat
   of a run that has ended is answered as one JSON body by a short handler, and a repeat of a
   running run takes over that run's stream, the older stream closed and the run untouched, so
   the 4 stream handlers are never more than the 4 runs and an app that reconnects at capacity
   is answered at once. **The engine's own spawner** (`crates/whizbang-serve/src/engine/threads.rs`,
   threads named `whizbang-engine-*`): the stdio server's spawner holds 2 and the
   playground's 44, and neither changes; the engine's holds 32 (the acceptor, 16 readers, 12
   handlers, the canceller, the hourly collection of D1 and the signals' reader), a sixth place
   in `scripts/tests/test-thread-lint.sh` with its limit, so a test that starts the engine in
   process stays inside the test kit's 64. What this does not stop, a flood of more than 16 new
   connections in 2 seconds before any token is shown, is a line in ADR 0045: off loopback the
   server stands behind the operator's proxy.
4. **MCP over HTTP** (`engine/mcp.rs`): `POST /mcp`, the one revision and its header,
   the stdio server's `Door`, message limits and methods, answered as JSON or as a stream; the
   run's id first, then its progress, then its result; the stated time and its grace; a repeat
   by request id followed to its run. The part of the stdio server's loop (`server.rs`, `call`
   and `look`) that starts, follows and ends a run is shared through a table of runs that no
   connection owns; the stdio server's tests run unchanged. **No connection and no watch of a
   deadline calls a cancel that can block**: `Executor::cancel` can wait for a run's final
   publication (`contract/live.rs`), so, as the playground does (ADR 0041; `play/mod.rs`), the
   cancel route and a deadline trip the run's held signal at once and answer `stopping`, and
   one canceller thread of the counted spawner calls the executor's cancel once a run, a second
   ask for the same run being folded into the first; a quit's grace starts before any such
   wait.
5. **The plain routes** (`engine/pictures.rs`, `engine/runs.rs`): uploads through the import
   path, a mask held to its image's size; downloads by identity; a run's state and its cancel
   by run id.
6. **`whizbang serve --http`** (`crates/whizbang/src/serve.rs`): `--http <addr>`,
   `--off-loopback`, `--host-name` (repeated), `--allow-origin` (repeated), `--token-env` or
   `--token-file`, `--calls`, `--pixel-limit`, `--uploads-kept-h`, and the existing `--wait-s`,
   `--store`, `--config`, `--env-file`, `--models-dir`. The executor is built as `serve` builds
   it, with every backend in use. `serve` without `--http` is the stdio server, unchanged. The
   start writes one line to the error output: the address, the host names, whether off loopback
   was stated, never the token. Signals stop it as "Runs, reconnecting and stopping" says.
7. **The negative tests** (`crates/whizbang-serve/tests/it/engine_guard.rs`, raw requests on a
   real loopback socket, no client library, no sleep):
   `engine_guard::a_call_without_the_token_is_refused_401_and_names_nothing`,
   `engine_guard::a_call_with_a_wrong_token_is_refused_401`,
   `engine_guard::a_request_whose_host_is_not_one_of_the_servers_names_is_refused_421`,
   `engine_guard::a_request_from_an_origin_not_listed_is_refused_403_with_no_allow_origin_header`,
   `engine_guard::a_listed_origin_is_answered_and_a_request_with_no_origin_is_taken`,
   `engine_guard::an_upload_over_64_mib_is_refused_413_before_it_is_read_whole`,
   `engine_guard::an_upload_over_the_pixel_limit_is_refused_413_naming_the_limit`,
   `engine_guard::an_upload_of_webp_or_another_type_is_refused_415`,
   `engine_guard::a_mask_not_of_its_images_size_is_refused`,
   `engine_guard::a_path_in_any_argument_of_any_tool_is_refused_400`,
   `engine_guard::a_json_body_over_1_mib_or_deeper_than_the_limit_is_refused`,
   `engine_guard::a_fifth_new_run_is_answered_429_and_a_repeat_run_routes_and_downloads_are_still_answered`,
   `engine_guard::a_third_upload_is_answered_429_after_its_headers`,
   `engine_guard::stalled_streams_and_uploads_and_trickling_clients_leave_the_run_routes_answered`,
   `engine_guard::with_every_reader_held_by_unauthenticated_incomplete_headers_a_whole_status_and_a_cancel_are_answered_within_2_seconds_of_their_acceptance`,
   `engine_guard::with_four_streams_open_a_repeat_of_one_takes_over_its_stream_at_once_and_dispatches_and_spends_nothing_more`,
   `engine_guard::a_repeat_sent_as_headers_then_body_with_four_streams_open_and_calls_stalled_in_their_bodies_is_reattached_and_status_and_cancel_answered`,
   `engine_guard::a_repeat_of_an_ended_run_at_capacity_is_answered_as_one_body`,
   `engine_guard::the_whole_engine_saturated_holds_no_more_than_its_32_threads`,
   `engine_guard::no_local_control_is_listed_callable_or_routed`,
   `engine_guard::a_request_with_no_protocol_version_another_one_or_a_batch_is_refused_400`,
   `engine_guard::a_cancel_of_a_run_this_server_did_not_start_is_answered_409`,
   `engine_guard::a_run_held_in_its_final_publication_leaves_repeated_cancels_another_runs_status_and_deadline_and_a_signals_grace_answered`,
   `engine_guard::the_token_is_in_no_answer_log_event_record_or_error`.
   And of the start (`crates/whizbang/tests/it/serve_http.rs`, the real binary):
   `serve_http::an_address_off_loopback_is_refused_without_off_loopback_and_a_host_name`,
   `serve_http::a_token_file_others_can_read_or_a_short_token_is_refused_at_start`,
   `serve_http::with_neither_token_flag_or_with_both_the_start_is_refused`,
   `serve_http::a_store_on_a_mount_of_a_network_type_is_refused_before_anything_listens`.
8. **What it does, against fakes** (`serve_http.rs`, against **the engine fixture**,
   `crates/whizbang/examples/engine_fixture.rs`, as `play_fixture.rs` is for the playground:
   the engine API's production code and the binary's wiring, with the test kit's fake
   segmenter, fal's fake fill and the test PKI's root handed in, in process; the shipped binary
   holds no fake and no way to trust another root. The tests of the start, of the flags, of the
   token and of the signals above run the shipped binary itself):
   `serve_http::the_offered_revision_initializes_lists_and_calls_to_its_end`,
   `serve_http::a_call_with_a_progress_token_streams_its_progress_and_its_result_and_names_its_run`,
   `serve_http::a_call_without_a_progress_token_streams_no_progress_and_still_names_its_run`,
   `serve_http::an_initialize_asking_another_revision_is_answered_with_2025_06_18`,
   `serve_http::a_repeat_after_a_restart_after_the_table_let_it_go_and_after_its_run_was_pruned_dispatches_nothing_more`,
   `serve_http::the_same_request_id_with_other_arguments_is_a_conflict_after_a_restart_and_after_a_prune`,
   `serve_http::a_repeat_is_answered_at_capacity_and_when_its_backend_is_no_longer_ready`,
   `serve_http::a_call_repeated_by_request_id_after_its_stream_dropped_follows_the_same_run_and_spends_once`,
   `serve_http::a_run_whose_stream_dropped_is_read_and_cancelled_by_its_id_and_commits_nothing`,
   `serve_http::two_clients_using_the_same_json_rpc_id_each_get_their_own_run`,
   `serve_http::an_upload_is_named_by_a_call_and_its_result_is_downloaded`,
   `serve_http::a_signal_cancels_every_run_accounts_the_ledger_and_releases_the_store` (the
   shipped binary),
   `serve_http::a_changed_models_dir_is_seen_after_a_restart_and_not_before` (S1, item 6).
9. **The engine API's guide's first part** (`docs/guides/engine-api.md`, the row already in
   `docs/guides/README.md`): starting it, the token, off loopback behind a proxy, the routes,
   calling a tool, reconnecting, cancelling, stopping; each example marked with its tier.

- [ ] Done when: the twenty-four negative tests and the four of the start pass; the thirteen of
  item 8 pass; the playground's seventeen and the stdio server's tests pass unchanged; ADR 0045
  is merged; the guide's first part is in.

**Keeping tests lean (H1).** The fast tier gains at most 12 seconds on the hosted macOS runner
for H1, measured in the pull request; the binary's tests share one started server where a case
allows it.

## R1. Whizbang's side: the kind `remote`, the probe, conformance over HTTP (large)

### Goal

Whizbang asks the front for SAM 3 under the gate, the ledger and the record exactly as it asks
the local worker, refuses an answer it cannot tie to the catalogue's files, and can ask the
host whether it is there without sending it anything.

1. **ADR 0046's client section**: the frames way, the kind, the probe's body.
2. **The client's frames way** (`whizbang_backends::net`): one new public way, a `POST` whose
   body is bytes the caller made (the frames), of a stated length and a declared set of
   classes, and whose answer is handed to the caller as it arrives, a chunk at a time, through
   a sink the caller gives. **Two limits, not one**: the connect, the handshake and the
   writing of the body keep the client's own limits (`CONNECT_LIMIT`, `BETWEEN_READS_LIMIT`);
   the wait for the answer's first byte and between its frames, which is model work (a queue,
   a cold worker's start and hello, the check of the files, a silent load, a silent call), is
   held only to the request's own deadline, which the kind `remote` sets from `host_limit_s`.
   A host that stalls is found at that deadline rather than sooner, and is charged at its bound
   either way (section 6), so nothing is lost but time; no heartbeat is added to the frames. It follows no redirect, takes the credential for its
   origin alone, and sends through the gate as the sender does (ADR 0019). **The send check
   sees typed parts, not opaque bytes:** the way takes a `CheckedFrames` value, which only the
   check can make, from the request's typed parts: each text (the phrase, the settings, the
   names) checked as a text piece is checked today, refusing a path or the store's root
   (`egress/hops.rs`), and the picture as the canonical image piece it already is, which the
   check passes as pixels (`SendCheck` exempts canonical images); the frames on the wire are
   written by `pack/1`'s codec from that value alone, so the bytes sent are the bytes checked,
   with the classes they carry. Beside it, **a
   checked control send** for the cancel: allowed only to the origin of a request this way has
   already dispatched, with that request's credential and no user data (no class), under a
   short deadline and a cancellation signal of its own made by a new constructor in the core
   (`capability/mod.rs`, `Bounds::for_cleanup`, the one place a backend gets bounds that are not
   its run's), and recorded in the run's egress like any send. It resolves no name: the
   client resolves outside its bounds (`net/mod.rs`), so the control send connects to the peer
   address the request's own connection reached, with the same TLS name and the same origin
   for its credential, and its 5 seconds hold from the connect to the last byte. ADR 0024 is amended with both,
   and the stale line of `net/mod.rs` that says chunked answers are refused is corrected.
3. **The kind `remote`** (`crates/whizbang-backends/src/remote/`), registered for `segment` by
   the binary's `kinds()`, as "The remote worker" says: an `https` address only; the hello, the
   `load` frame with the model, installation and settings, and the `request` frame of
   `pack/1`'s own codec (`pack/proto.rs`, unchanged), and the `cancel` through the control
   send; the answer's frames read through the same
   codec under its bounds as they arrive, the identity header checked against the catalogue's
   digest before anything is used (or, without one, the answer taken only as a failure-only
   answer, "Its identity"), the price from the two settings with the checked conversion,
   the device and both identities recorded. A config that gives `remote` for `fill` or `judge`
   is refused, naming the capability; one that gives one price setting without the other, zero,
   or a value that overflows is refused when read.
4. **The conformance suite over HTTP** (carry-over, "The HTTP codec of the pack's protocol"):
   the behavioural half is first restated over an atomic request, since the HTTP route takes a
   load and its call together and answers the call alone. The suite's `Transport`
   (`crates/whizbang-backends/tests/it/pack/conformance.rs`, `Ask::Load` beside the calls)
   becomes one of requests, each a load and its call, as the production route takes them; a
   case that asks a load's failure sends a load that fails with its `segment` and holds that
   the answer is the load's error and that the `segment` never ran; and the local transport sends each through the supervisor's queue as a `Request`
   is sent today; a case that observes the worker (a load that failed, a worker ended and
   replaced after bad output) observes it, over HTTP, through what the front reports of its
   worker: the error the request is answered with, naming the load or the worker's end, and the
   worker's generation in `pack-host/1`, which counts each start, and the outcome record, so the
   suite's cases of a worker's exit and its error output (`tests/it/pack/worker.rs`: the
   `Unavailable` at `Stage::Unresolved`, the exit status kept, the last lines of error output
   handed back with the token taken out) pass through the production HTTPS client as they pass
   locally. ADR 0046 holds a table of
   every behavioural case, its local form and its HTTP form; none is dropped, none passes by a
   load made up for it, and a case that cannot be stated over HTTP stops R1 and is said. Then
   the remote kind's transport runs every case against the fake host (the real front, `--worker
   local`, the pack's fake models, serving HTTPS on `127.0.0.1:0` with a certificate of the test
   PKI the net client's tests already use, `net/testing.rs`, whose root the test hands the
   client), through the production HTTPS path. The
   front runs under `scripts/thread-guard.sh` as the pack's other real-process tests do, and its
   seconds count under the fast tier's 300 (at most 8 in all; the cases share one host).
5. **The probe's body, and the real prober** (carry-over, "The real network client and the
   shipped catalogue", what is left). `Probed::Reached` gains a body of at most 4 KiB, held to
   that bound by the checked prober before it is handed on (`egress/ways.rs`,
   `egress/chains.rs`, where `CheckedProber` is; `FakeProber` with it). `Prober` is implemented on the net client: a
   `GET` with no credential, no body and no user data, no redirect followed. The binary hands it
   to `whizbang backends status --probe` in place of `NoProber`; for a `remote` backend it
   probes `<url>/pack/1/health`, reads `pack-host/1`, and says whether the host holds the model
   the config asks, by the catalogue's hashes. A backend whose approval does not say
   `probe = true` is not probed, as today.
6. **The guide's part**: `docs/guides/backends.md` gains the kind `remote` and its settings,
   `docs/guides/config.md` its table and an `egress` example; both say that the token is a
   secret of both sides and that a missing price refuses every call.

- [ ] Done when: every case of the conformance suite's behavioural half passes over the local
  pipe and over HTTP against the fake host (`conformance::*_over_http`);
  `remote::a_call_is_sent_through_the_gate_with_its_token_bound_to_its_origin`,
  `remote::a_remote_backend_without_both_price_settings_has_no_bound_and_is_refused`,
  `remote::a_zero_or_overflowing_host_limit_is_refused_when_the_config_is_read`,
  `remote::the_examples_bound_is_360000_microusd_and_an_unknown_end_settles_at_it`,
  `remote::an_answer_with_no_malformed_or_another_identity_is_refused_and_not_kept` (three
  cases), `remote::a_stream_cut_before_its_result_is_truncated_and_charged_at_its_bound`,
  `remote::a_cold_start_a_silent_load_and_a_silent_call_past_30_seconds_complete_inside_the_requests_deadline`,
  `remote::a_host_silent_past_the_requests_deadline_is_timed_out_and_charged_at_its_bound`,
  `remote::a_cancel_sends_the_cancel_frame_and_closes_the_stream`,
  `remote::a_cancel_is_sent_after_the_runs_signal_is_set_and_after_its_deadline_has_passed`,
  `remote::a_host_that_never_answers_the_cancel_is_let_go_of_at_its_own_deadline`,
  `remote::the_cancel_resolves_no_name_and_ends_in_its_deadline_under_a_resolver_that_stalls`,
  `remote::the_cancel_ends_in_its_deadline_on_a_stalled_connect_handshake_or_answer`,
  `remote::the_control_send_is_refused_for_a_request_not_dispatched_or_another_origin`,
  `remote::a_load_frame_carries_the_settings_of_the_call`,
  `remote::a_worker_that_ended_is_told_by_its_outcome_record_with_its_status_stage_and_error_output_and_no_secret`,
  `remote::a_failed_check_of_the_files_and_a_busy_refusal_cross_as_failure_only_answers_with_their_kind_and_are_charged_at_the_bound`,
  `remote::an_answer_with_frames_and_no_verified_identity_is_refused_whole`,
  `remote::a_halt_a_timeout_and_a_cancel_with_the_same_status_and_stage_are_told_apart_by_their_kind`,
  `remote::a_malformed_and_an_oversized_answer_caught_by_the_front_cross_as_malformed_and_too_large`,
  `remote::a_worker_exited_killed_and_signalled_each_crosses_as_itself`,
  `remote::a_worker_error_that_leaves_the_worker_alive_hands_back_its_error_output`,
  `remote::the_long_error_output_fixture_crosses_whole_and_a_line_or_count_over_its_bound_is_refused_before_it_is_read`,
  `remote::a_host_that_failed_its_start_or_was_cancelled_before_the_worker_saw_the_request_is_charged_at_its_bound`,
  `remote::a_body_that_ends_without_its_outcome_record_is_truncated`,
  `remote::two_processes_with_their_own_stores_at_one_clock_mint_other_dispatch_ids_and_both_run_against_the_fake_host`,
  `remote::the_dispatch_id_is_in_the_runs_record_and_a_cancel_names_it`,
  `remote::pixels_that_spell_a_path_are_sent_and_a_path_in_the_phrase_or_a_setting_is_refused_before_anything_leaves`,
  `remote::the_stores_root_in_any_text_is_refused_before_anything_leaves`,
  `remote::a_plain_http_address_is_refused_and_a_certificate_of_another_root_fails_the_call`,
  `remote::a_redirect_from_the_host_is_refused_and_nothing_follows_it`,
  `remote::an_answer_past_a_bound_of_pack_1_is_refused_before_it_is_held`,
  `remote::the_run_records_the_identity_asked_the_one_reported_and_the_device`,
  `prober::a_probe_carries_no_credential_and_no_body_and_follows_no_redirect`,
  `checked::a_probes_body_past_4_kib_is_refused_by_the_checked_prober`,
  `backends::status_with_probe_reads_the_hosts_document_and_names_the_model_it_holds` pass; ADR
  0046's client section is merged; the two carry-over rows are closed with their lines.

## D1. The GPU host's deployment, and user story 1 on fakes (medium)

### Goal

Everything the orchestrator needs to put SAM 3 on Modal and run user story 1 is in the repo and
tested without an account; nothing in it is a secret, an account name or a private path.

1. **The host's image, locked** (decision 3).
   `deploy/gpu-host/requirements.txt`: the pack's runtime for Linux on x86-64 with CUDA
   (`torch`, `torchvision` and `transformers` at the versions the Mac extra pins, and what they
   pull, NVIDIA's CUDA packages among them), written by `uv pip compile --python-platform
   x86_64-unknown-linux-gnu --python-version 3.12 --generate-hashes`, every file pinned by its
   hash, from `pypi.org` and `files.pythonhosted.org` alone. It builds the host's image on the
   host's side and nothing else: `pack/uv.lock`, the extra's markers, `whizbang setup`, the
   release archive and `THIRD-PARTY-LICENSES.md` are unchanged, and
   `setup::the_extra_is_for_the_mac_alone_and_the_lock_names_no_nvidia_or_cuda_package` still
   passes. `deploy/gpu-host/LICENSES.md` names each package's licence as its metadata states it.
2. **Modal's deployment** (`deploy/modal/sam3_host.py`): an image of the pinned Python, the
   locked requirements and the pack's sources from the checkout; a volume for the models
   directory; a CPU function that serves the front (`--worker` naming the bridge below), its
   health route among it, so that the token is checked and a probe answered with no card
   allocated, held to exactly one container that takes many inputs at once (the front's own
   connections), so a cancel and a probe reach the one front that holds the queue and are never
   queued behind a running request; and one GPU function, the bridge, that runs the pack's
   worker in its container, keeps the model resident there between calls, claims each dispatch
   id in a Modal `Dict` before any model work, and carries frames in and out, with no retry, at
   most one container, which takes two inputs at once, the call and a control input for its
   cancel (the worker still runs one call at a time), and its limits (`startup_timeout`,
   `timeout` and `scaledown_window`, and no `retries`) set from one table at the top of the file
   whose sum is the config's `host_limit_s`, the bound of one attempt, as ADR 0046's section on
   Modal's bound says. **No two generations of
   the app run at once**: a host's rolling deploy keeps old containers on the inputs they hold
   while new traffic goes to new ones, so the bridge's lock and its running dispatch would be
   two; so a new deploy is only ever made from a stopped app (item 3: `deploy` proceeds only when
   Modal's own list of apps says the app is stopped, which an app scaled to zero containers is
   not, and no container of it is left, or, for the first deploy, when that list of the
   workspace's environment, read whole, holds no app of that name; any other state, or one it
   cannot read whole, refuses; `down`
   stops the app, so it takes no new input, then waits until no container of it is left), and the claim
   store is a named `Dict` outside the app, which a deploy does not replace, so a redelivery
   after a new deploy still finds its claim. Within one deployment, **when the front is
   replaced** (a crash), its streams end and each request in them is `truncated`
   and charged at its bound on whizbang's side; the bridge's call it had begun runs on at most
   to its own limits, inside that bound, and its answer is dropped. The bridge is shown against
   fakes in `pack/tests/test_front.py` (`Bridge`) with a stand-in for Modal's call, and
   `test_front.Bridge.test_a_front_replaced_mid_call_leaves_the_bridges_call_to_end_at_its_limit`
   is added here, as D1's, with
   `test_front.Bridge.test_under_one_call_and_one_control_input_at_a_time_a_cancel_is_not_queued_behind_the_call`,
   whose stand-in for Modal admits exactly the inputs the deployment allows, and
   `test_front.Bridge.test_two_generations_never_overlap_a_deploy_waits_for_the_old_one_to_stop_and_a_redelivery_after_it_finds_its_claim`,
   whose stand-in routes new inputs to a new generation while the old one holds a blocked call;
   `scripts/tests/test-gpu-host.sh` holds, with Modal's command faked, that `deploy` refuses an
   app that is deployed with no container running, one with a container left, and a state it
   cannot read; that it proceeds for an app listed as stopped with no container, leaving the
   claim store as it was, and for a first deploy into an environment that holds no app of that
   name; and that `down` fails while a container is left. The front's token is a Modal secret named by the variable the config names.
   The workspace, the token's value and the endpoint are not in the file: they come from the
   orchestrator's environment at deploy time. The card is a 24 GB one, by its name in the file.
   The front's function is CPU-billed and counted in the price per second.
3. **The orchestrator's script** (`scripts/gpu-host.sh`, not on the allowed list, so it asks
   first): `deploy`, from a clean checkout of a named commit, refused unless Modal's list of apps
   says the app is stopped and no container of it is left, or, the first time, that the
   workspace's environment holds no app of that name (an app deployed with no container
   running is not stopped; a list it cannot read whole refuses; it says to run `down`), which then
   reads the deployed
   limits back and refuses when any of the three is missing or their sum differs from the
   config's `host_limit_s`;
   `weights`, which copies SAM 3's installed files from the models directory on the Mac into
   the host's volume after `whizbang models verify sam3` has checked every file against the
   catalogue, with no Hugging Face sign-in
   on the host (decision 4); `probe`, which is `whizbang backends status --probe` under the
   config it is given; `down`, which stops the app and waits, to a stated limit, until Modal
   lists no container of it, and fails if one is left. It prints no token and no account, reads no
   env file itself (it hands `--env-file` to whizbang), never reads Modal's token file, and
   refuses a checkout with changes. `scripts/tests/test-gpu-host.sh` checks its arguments and
   refusals with Modal's command faked; it never contacts a host.
4. **User story 1 on fakes** (`crates/whizbang/tests/it/story.rs`, the engine fixture of H1,
   item 8, with the kind `remote` wired as the binary wires it and pointed at the fake host over
   HTTPS with the test root, and fal's fake for fill, on loopback):
   `story::a_hosted_editor_uploads_names_and_clicks_and_downloads_with_no_local_worker`: the app
   uploads a picture, calls `remove_prompt` with words and downloads the result; uploads again,
   calls `segment` with a point and one instance and `fill` with its mask, downloads that; and the test holds
   that whizbang started no pack worker, that each run's record names the front for segment and
   fal for fill with their classes, and that the ledger booked the fill's price and the
   segment's bound and settlement.
5. **The hourly collection of uploads**: `whizbang serve --http` calls
   `Store::collect_unused_imports` once an hour on a thread of the counted spawner, with the age
   `--uploads-kept-h` gives;
   `serve_http::an_upload_no_run_took_is_collected_after_its_age_and_one_a_run_took_stays`, on a
   controlled clock.
6. **The guides**: `docs/guides/gpu-host.md`, new, listed in `docs/guides/README.md`, written
   for a customer who deploys a host of their own, with their own Modal account (or another
   machine) and their own tokens, and points the kind `remote` at it (decision 5), not only for
   whizbang's testing deployment: the front and the worker, a host of one's own (the front with
   `--worker local` on any Linux machine with a card and the requirements), Modal's deployment
   under the reader's own workspace, secret and config, the price and what `host_limit_s` must
   cover, the workspace's spend limit as the backstop for ADR 0046's residual, the token, the probe, where the host's copy of SAM 3's files comes from (the reader's
   own download from the maker's page with their own sign-in, copied to the host as `weights`
   does), and what a customer is told of SAM 3's licence before its files are used; `docs/guides/engine-api.md` finished with user story 1 as its worked example,
   in `curl`, marked by tier. `README.md` names the engine API among the doors; `AGENTS.md`
   gains `scripts/gpu-host.sh` (the orchestrator's alone) and `parity-remote`. The remote
   parity module (empty of runs until L1) and its selector are D1's, with their script test.
7. **The Linux runtime row** (carry-over, "Linux runtime extra"): narrowed with a line: the GPU
   host's image runs the pack on Linux with CUDA from its own lock (decision 3); local
   models on Linux for a user stay deferred, as the owner decided.
8. **A row for milestone E**: a task that removes a thing from a mask a caller gives (the
   mask grown, closed and feathered as `remove` shapes a location's, then filled), so a clicked
   removal is shaped as a named one is; opened in `docs/carry-over.md` with milestone E as its
   owner, built only if E's evaluation of clicked removals asks for it.

- [ ] Done when: the story's test, the collection's test and `scripts/tests/test-gpu-host.sh`
  pass; the setup test of the Mac's extra passes unchanged; the deployment and the lock are in
  the guides are in;
  a scan of the tree finds no secret, account or private path in `deploy/`.

## W1. Tiny's licence text, and the catalogue's revisions (small)

### Goal

Tiny's installation carries its licence's text (carry-over, "The Apache-2.0 licence text
itself"). SAM 3's files are not hosted by whizbang (decision 5): its catalogue entry, its origin
and its download from the maker with the customer's own sign-in stay as they are.

1. **Tiny's licence text**: the catalogue's entry for `tiny` gains a
   `LICENSE` file whose text is the Apache-2.0 text the binary carries (whizbang's own `LICENSE`
   is that licence, byte for byte), pinned by its hash, written by the installer beside the
   model's files. No origin is added; the row's "from an approved origin" is changed, with a
   line, to "from the binary", which needs none.
2. **What a customer is told** of SAM 3's licence: its terms are accepted on the maker's page,
   with the customer's own sign-in, before the download, as today, and the copy the catalogue
   pins (`LICENSE`) is kept beside the files; `docs/guides/models.md` gains the acknowledgement
   a publication owes (the carry-over row's reading, section 1.b.ii).
3. **The catalogue's revisions, and what they stale.** Each change to an entry moves its
   revision (`models/catalogue.rs`), and a moved revision stales the parity fixtures of that
   model (`parity::the_committed_fixtures_name_the_revisions_the_catalogue_holds`; ADR 0027) and
   any evidence record that names it. So item 1 moves `tiny`'s revision, and SAM 3's is
   unchanged. Before the commit that moves it, the orchestrator installs the entry so changed on
   the owner's Mac, runs the model tier's `parity` and the evidence runner for `tiny` in P1's
   candidate mode, which installs the candidate's own pack and binary first, and hands the
   builder the files, which go in the same commit as the entry; the tolerance measured again
   must be ADR 0027's or better. No spend: the Mac's GPU.

- [ ] Done when: Tiny's installation holds its licence's text, pinned
  (`models::tiny_installs_its_licence_text_from_the_binary_pinned_by_its_hash`), with its
  revision moved and its fixtures and evidence written again in the same commit; SAM 3's
  catalogue entry is unchanged; and `docs/guides/models.md` names the acknowledgement.

## L1. The live round: SAM 3 on the GPU host, parity, user story 1 (medium; the orchestrator's)

### Goal

The row's live clauses, made from the owner's Mac against Modal and fal, with their records.

1. **Before anything is spent**: on or after 2026-11-30 or 2026-12-31, if the carry-over row on
   the Google rows of prices still has that date's row open, the orchestrator reads that price
   from the provider's page and changes the build's row, its date and ADR 0026 with it, in L1's
   pull request (the row: the first live run on or after each date, milestone E's or 6's).
2. **The host up**, once the owner's setup of decision 2 is done: `scripts/gpu-host.sh deploy`,
   `weights` and `probe` from a clean checkout of `main`; the card's and the front's prices per
   second read from Modal's pages on the day, and the limits as ADR 0046's section on Modal's
   bound set them, checked again against those pages on the day; written into a config kept
   outside the repo, with the front's exact
   origin approved in its `egress` table (`source_images`, `prompts`, `probe = true`). Every
   address Modal's client used is named in the record.
3. **Parity, three times** (a module of its own, `crates/whizbang/tests/it/remote_parity.rs`,
   `remote_parity::sam3_answers_each_fixture_inside_the_tolerance_from_a_remote_worker`, ignored
   in the fast tier and selected only by `scripts/test-models.sh parity-remote`, which refuses
   unless `WHIZBANG_REMOTE_CONFIG` names a config whose segment is `remote`; the default
   selection, `parity` and the local leg of a release never name the module, which D1 writes
   with the selector and holds with
   `scripts/tests/test-models-selection.sh`: the default, `parity` and the local leg's
   selections hold no remote case): each of SAM 3's fixtures asked three times of the host, each answer
   against the committed one, every pair's IoU at least 0.99 and its scores within 0.01 with the
   same counts, and every answer reporting `cuda`. The record, `docs/results/m6-remote-sam3.md`,
   names the card, the driver and CUDA the worker reported, the least IoU and widest score gap
   of each fixture, the time of a call cold and warm, and what each round cost against what
   Modal billed. An answer outside the tolerance, or one not on `cuda`, fails the round: the
   record says by how much and where, the owner is told, and L1 is not done; the fix is in the
   adapter's settings on CUDA (a pull request of its own, then the round again), never in the
   tolerance.
4. **User story 1, live.** `whizbang serve --http 127.0.0.1:<port>` on the owner's Mac, in a new
   home with no pack installed and an empty models directory, a new store, the remote SAM 3 and
   Bria through fal; the app is a short `curl` script outside the repo. Three of milestone 3's
   pictures, each named and each clicked: six removals (the clicked ones two runs each, `segment` then `fill`), each with its run id, its settled spend and
   the recipients its record names. The record, `docs/results/m6-user-story-1.md`, holds
   aggregates and run ids, and the evidence test holds it to those
   (`evidence::the_records_of_milestone_6_hold_only_aggregates_and_run_ids`).
5. **A cancel of a running call on the card**: one `segment` cancelled from whizbang while the
   card computes; the record says cancelled, the host's log shows the cancel frame reached the
   worker within the cancel's 5 seconds (or the worker restarted at its deadline), and the
   request is charged at its bound.
6. **The probe and a stopped host**: `backends status --probe` with the host up names the model
   it holds, and repeated probes allocate no card (Modal's usage page shows none for them); with
   the host down (`gpu-host.sh down`), a call of user story 1 ends as an error naming the host,
   and nothing falls back to another backend (section 10).
7. **Then `gpu-host.sh down`**, so that nothing bills between rounds.

- [ ] Done when: both records are merged with their runs; parity is inside the tolerance on
  every fixture in each of three rounds, every answer on `cuda`; the six removals of user story 1 (nine runs of actions)
  completed with no local worker; the cancel of item 5 reached the card's worker; the spend
  is in the records.

## N1. The note (small)

1. **The note**, `docs/plans/m6-note.md`, in milestone 5's shape: the row's clauses each with
   its evidence (the tests by name, held by `scripts/tests/test-notes.sh`), the timing report by
   suite and by job, the spend (Modal's and fal's), what moved to the carry-over list, and what
   a reader should not assume (the tolerance met on one card; one GPU host; the app's server
   behind a proxy it cannot see). The boxes of this plan ticked.
2. **The spec's wording owed**: section 1's door row names `docs/guides/engine-api.md`; section
   4's example config gains the two price settings of the kind `remote`; section 5's
   "Platforms" says that a GPU host runs the pack on Linux with CUDA from its own lock and that
   local Linux models stay deferred; section 10's "Approved recipients at first" names Modal;
   section 11's `whizbang serve [--http <addr>]` gains the flags H1 built. Each brings the spec
   in line with reviewed pull requests.
3. **The tag `m6`**, by the orchestrator, on the note's merge, and only when L1's and W1's
   boxes are ticked. Neither waits on an upload: whizbang hosts no files (decision 5).

- [ ] Done when: the note is merged and `m6` is tagged.

## Carry-over rows owned by milestone 6, and who takes each

| Row (by its subject in `docs/carry-over.md`) | What happens to it |
|---|---|
| Tests that time out or fail at random at the end of milestone 5 | A0 |
| The HTTP codec of the pack's protocol | P1 and R1: built, and the behavioural half passes over it |
| The real network client and the shipped catalogue: what is left, a real prober | R1, item 5 |
| Every publication writes the whole generation again | S1, item 1 |
| What running an action does not do yet: a parent's wait inside the core, a panic outside an action | S1, item 2: the wait kept with a line (the contract forbids a terminal parent before its children) and bounded by the started-runs bound, with a test; the panic caught |
| A run whose end became visible and was not confirmed is not let go of | S1, item 3 |
| The writer lock and a reader's hold are file locks: a file system on which they do not hold | S1, item 4 |
| A mask costs its three planes in memory | S1, item 5: decided with a line |
| Where the models directory is, read once | S1, item 6: kept with a line and a test |
| A drawn location or a picture that no run took | S1, item 7, and D1, item 5 |
| Two things of a squint job file: a picture given inline, and `expect` | H1, closed with a line: the engine API takes pictures as uploads alone, so a job names them by identity and one byte bound governs every picture that arrives; `expect` guards a file a caller read itself, and an upload's identity is what the server answered, so there is nothing to expect. Neither is taken, by `squint check --job` either |
| Hosting the model's files for customers | Not taken: re-owned by this plan's amendment to no milestone yet, open until local download from the maker becomes a barrier (decision 5, 2026-11-23) |
| The Apache-2.0 licence text itself, with Tiny's installation | W1, item 1 |
| Linux runtime extra (deferred by the owner) | D1, item 7: narrowed with a line |
| The Google rows of prices are introductory | L1, item 1, if its run is on or after a row's date and the row is still open |
| Separate publishing to crates.io | Untouched |

## Documentation that changes

- `docs/guides/engine-api.md`, new (H1, D1); `docs/guides/gpu-host.md`, new (D1); both under
  "Written" in `docs/guides/README.md`.
- `docs/guides/backends.md` and `docs/guides/config.md`: the kind `remote`, its settings and its
  egress (R1); `docs/guides/serve.md`: a line pointing to the engine API's guide (H1).
- `docs/guides/models.md`: the acknowledgement a publication owes, W1, item 2; nothing on
  hosting (decision 5).
- `docs/adr/0045-the-engine-api.md` (H1), `docs/adr/0046-the-remote-worker.md` (P1 and R1, with
  the amendments to ADR 0020 and ADR 0024), `docs/adr/0047-the-generation-in-buckets.md` (S1).
- `docs/reference/`: regenerated if the description changes (not expected).
- `README.md` and `AGENTS.md`: D1, item 6.
- `SPEC.md`: N1's wording, item 2.

## Commit strategy

- One task is one branch and one pull request, tests first, reviewed whole by a reviewer that
  did not write it, merged by the orchestrator with a merge commit once `ci` and both scans
  pass (the sixteen jobs: `check`, `sweeps` and `release` on each runner, `scripts` and `e2e` in
  two shards on each, `secrets`, and the aggregate `ci` that needs the other fifteen). A builder pushes once,
  when the whole task is done, after one full `just check` on the
  finished tree, and `just e2e` where it touches the playground's host (H1, item 2). A0 pushes
  twice, by design (item 2).
- An ADR is in the pull request of the task that names it, its first commit.
- Every state of `main` between tasks passes `just check`; every door that worked still works:
  the engine API adds a door and changes none.
- **Rollback checkpoints.** The tag `m5`, which exists. `m6-engine`, made by the orchestrator
  when P1, S1, H1 and R1 have all landed: the engine API and the remote kind, on fakes, with the
  long-lived store. `m6`, on N1's merge. A pushed tag is never moved.
- **If a task shows a decision here is wrong,** the decision is changed in this file first, in
  its own pull request; a change of scope or architecture gets the review rounds first. A test
  is never bent to fit, and the tolerance of ADR 0027 is not changed by this milestone.

| Tasks | To undo |
|---|---|
| A0 | Revert the merge: the tests are joined again, and the case reads at once |
| P1 | Revert the merge after R1 and D1: the front and the CUDA choice are gone; the Mac's behaviour never changed |
| S1 | Revert with care: a store a writer of S1 has published is in the new format, which a build before S1 refuses by its version. So a revert keeps the reader and writer of buckets and drops the rest, or such a store is exported and imported. ADR 0047 says this |
| H1 | Revert the merge after D1: `serve --http` is gone; the stdio server and the playground are as at `m5`, the shared HTTP reader back in `play/` |
| R1 | Revert the merge after D1: the kind `remote` and the prober are gone, `NoProber` is back, the probe has no body; a config naming `remote` is refused as an unknown kind |
| D1, W1 | Revert the merge; the deployment, the lock and the guides go. W1's revert takes `tiny`'s licence file back with its revision and its fixtures |
| L1, N1 | Records and a note. The host is stopped by `gpu-host.sh down`, and the app deleted in Modal's console by the owner |

## Spend

All on approved recipients (Modal and fal), by the orchestrator, from the owner's Mac, each
run's settled spend in its results file and the total in the note. Modal's prices are an
assumption until L1 reads them on the day: at most 0.0003 dollars a second for a 24 GB card and
the front's CPU together, about 1.10 an hour. A request's bound at the example's 600 seconds is
0.18 dollars an attempt and 0.36 a call, at the allowance of 2 attempts (ADR 0046); what is
billed is less, but for the residual ADR 0046 records (restarts past the allowance).

| Step | What is called | Estimate |
|---|---|---|
| A0, P1, S1, H1, R1, D1, W1 | Nothing paid: fakes in every test | 0 |
| L1, item 2: deploy, weights, probes, three cold calls | The image built on Modal's side; probes on the CPU front | 0.50 |
| L1, item 3: parity, three rounds | 9 requests, a cold start each round, about 15 minutes of card time with idle | 0.50 |
| L1, item 4: user story 1 | 6 removals (9 runs of actions), each a segment on the host and a fill at 0.04 through fal | 1.00 |
| Getting the host right | Cold starts, failed tries and reruns, up to 3 hours of card time | 3.50 |
| A second parity round after a fix, if one is needed | As item 3 | 1.00 |
| The owner's own tries of the engine API | About thirty runs | 2.00 |
| | **Total** | **about 8.50 dollars, of the 100** |

If the live spend passes 40 dollars, the orchestrator stops, says why in the record and tells
the owner. Nothing else is built to hold spend.

## Dependencies

```
A0 ──┬──► P1 ──┬──► R1 ──┐
     │         └──► W1 ──┼─────────────────────┐
     ├──► S1 ──┬─────────┤                     ▼
     │         ▼         ├──► D1 ──► L1 ─────► N1
     └──► H1 (merges after S1) ──┘   ▲
              owner's setup of decision 2
```

- A0 before everything: the fast tier's flakes are fixed before wide pull requests measure
  against it.
- P1 before R1: R1's conformance over HTTP runs against P1's front.
- S1 before H1's merge: the engine API holds a store only once the store refuses a file system
  whose locks do not hold.
- R1, H1 and S1 before D1: user story 1 on fakes needs the kind and the engine API, and the
  hourly collection needs S1's API.
- P1 before W1: W1 changes `tiny`'s catalogue entry, which stales the parity fixtures and the
  evidence P1 wrote again; it writes them again itself, by P1's candidate mode.
- L1 after D1 and the owner's setup of decision 2. L1 does not wait for W1: W1 moves `tiny`'s
  revision and fixtures alone, and SAM 3's entry, which L1's parity measures against, is
  unchanged (decision 5). N1 waits for W1's box and L1's; nothing waits on an upload.

## Risks

1. **Parity on a CUDA card may not reach 0.99.** The tolerance was measured on one Mac's two
   devices. Full precision and TF32 off are used; if an answer is still outside, L1 fails and
   the fix is in the adapter, never the tolerance. This is the milestone's largest unknown, and
   it is met last.
2. **A host's limits do not bound its bill.** Modal bills startup and idle time beside a call's
   own time, and runs an input again after a crash or a preemption with no stated count (P1,
   item 1, settled on paper on 2026-11-23). Whizbang bounds one attempt from documented limits,
   prices an allowance of 2, the deploy refuses a missing limit or a mismatch, the bridge asks
   for no retry and answers a replayed request from its claim, and the record compares the
   ledger with Modal's bill. The residual, restarts past the allowance, is recorded in ADR 0046
   as the known limit; the owner's workspace spend limit is the backstop.
3. **The host's own front may cut, buffer or redirect a long answer.** A redirect is refused and
   recorded; a buffered answer only delays the events; a cut answer is `truncated` and charged at
   its bound. L1 measures cold calls before user story 1.
4. **The front does the supervisor's work a second time, in Python.** Two codes for one set of
   rules may drift. Each duty has a named test on each side, and the conformance suite runs over
   both transports.
5. **New attack surface off loopback.** Bounded by one token, a Host list, no Origin by default,
   no path, no local control, the limits, and the guard's single path; TLS at a proxy the
   operator states. The reviewer of H1 reads the guard first.
6. **A new store format.** S1's buckets change what is on disk; a revert is costly. The old
   format is read, the new one is written only by a writer, and the refusal by version is
   tested. If S1 runs long, its item 1 is the seam and lands first.
7. **NVIDIA's packages in the host's image.** Under NVIDIA's own licences; built on the host's
   side, from the host's lock; nothing of it ships in whizbang's archive. The owner decides
   (decision 3).
8. **The licence of SAM 3.** Whizbang hosts no copy of its files (decision 5): a customer
   downloads them from the maker's page with their own sign-in, or hosts their own SAM 3 on a
   GPU host of their own. The owner considers the licence and distribution question settled;
   the carry-over row's reading stays where it is, for when hosting is taken up.
9. **The fast tier's time.** P1, R1 and H1 add real processes and a server. Each pull request
   reports its seconds by suite and stops past about 250 of the 300; then coverage is reviewed
   first, as section 13 says.
10. **The budgets are estimates.** A pull request that passes about 2,500 changed lines splits
   at its named seam, and the owner is told the count.

## Open questions

None. The owner answered the four of draft 26 on 2026-11-23; they are decisions 2 to 5 under
"What the owner decided". No agent ever reads, prints or copies the Modal token, the front's
token or a Hugging Face token.

## What the owner still does

Not questions: steps the owner has agreed to take, each before the task that needs it, and
the orchestrator says when.

1. Before L1: the Modal workspace and its payment method, `modal token new` on the Mac, and
   `SEGMENT_TOKEN` in the env file with the same value as Modal's secret `whizbang-front`
   (decision 2).
2. At D1's setup, with the workspace: a spend limit on the Modal workspace, suggested at 50
   dollars a month, the backstop for the residual ADR 0046 records (restarts past the
   allowance of 2 attempts), and a line of `docs/guides/gpu-host.md` (D1).

## What the owner is told, not asked

- **Modal is the GPU host**, as you decided on 2026-11-22; the origins it brings are in "What
  the owner decided", and its spend counts against this milestone's 100 dollars. Your four
  answers of 2026-11-23 are decisions 2 to 5 there, decision 5 as you changed it later that
  day: whizbang hosts no copy of SAM 3's files for now.
- **Nine pull requests**, four of them large; P1, S1 and H1 run side by side after the entry
  check.
- **About 8.50 dollars of live spend**, with a stop at 40.
- **The engine API is stateless MCP over HTTP** with plain routes for pictures and runs; it
  takes no path, offers no local control, and needs a token on loopback too.
- **No Python and no GPU on the app's server**: shown in the fast tier and live.
- **The store's format changes** (S1) so that a server can hold it for months; older builds
  refuse a store a newer one has written.
- **CI as you decided on 2026-11-22, and as #284 made it**, is what every pull request of this
  plan meets: the Chrome tier held to 240 seconds in all and 120 a shard; the script tests in
  two shards on each runner, under the unchanged budget of 10 minutes; sixteen jobs in all,
  fifteen and the aggregate `ci` that needs them, which the skills `kickoff` and `review-merge`
  name. This
  plan changes none of them; a task that would pass a limit stops and says so.
- **The tags `m6-engine` and `m6`** are made by the orchestrator as above.

## Review rounds

### Review round 1

On draft 1. Verdict: revise. Thirteen findings: 1 blocker, 12 majors. Each was checked against
the code and taken.

1. **Blocker: the billing bound was not established, and the health route would wake a card.**
   Right: Modal bills startup and idle beside execution. Taken: `host_limit_s` is the sum of
   every limit the host enforces plus a margin, with no retry and one container, the deploy
   refuses a mismatch, and a phase with no limit means no bound; the front and its health route
   run on a CPU function and the worker on the GPU one.
2. **Major: seconds passed to a milliseconds API.** Right (`price.rs`). Taken: a checked
   conversion, zero and overflow refused, and a numerical test of the example.
3. **Major: hashes cannot ride in the hello, and a probe answers a status alone.** Right
   (`proto.rs`, `ways.rs`). Taken: the identity in an HTTP header around the frames, `pack/1`
   unchanged; `Probed::Reached` gains a bounded body (R1); missing, malformed and other
   identities tested.
4. **Major: the adapter never asks for CUDA.** Right (`adapter.py`). Taken: P1, item 4, and L1
   fails any answer not on `cuda`.
5. **Major: the queue, loads and restarts are the Rust supervisor's.** Right (ADR 0020,
   `worker.py`). Taken: the front supervises a child worker, every duty with a test; the pack's
   side became P1 and whizbang's side R1.
6. **Major: a terminal parent with live children breaks the contract.** Right (`SPEC.md` section
   3). Taken: the parent's wait is kept with a line; doors give up their caller's wait; the
   started-runs bound holds a server, with a test; a panic ends a run only after its children.
7. **Major: evicting an abandoned run loses what is unconfirmed.** Right (`engine.rs`,
   `interrupted.rs`). Taken: a small marker kept in place of the heavy state, bounded, and new
   admissions refused past the bound.
8. **Major: H1 needed S1's collection.** Right. Taken: the hourly collection moved to D1, which
   builds on S1.
9. **Major: cancel tied to a stream and to ambiguous ids.** Right (`server.rs`). Taken: a table
   of runs no connection owns, `GET /v1/runs/<id>` and its cancel by run id, a repeat answered
   at capacity, and a test of two clients with one JSON-RPC id.
10. **Major: the revisions offered and the header rule disagreed.** Right. Taken: 2025-06-18 and
   2025-03-26 alone over HTTP, a missing header taken as 2025-03-26, a test of each revision's
   whole exchange.
11. **Major: failed parity could still close the milestone.** Right. Taken: L1's box needs
   parity inside the tolerance, and `m6` waits for it.
12. **Major: the Google price rows were left out.** Right (the row: the first live run on or
   after each date). Taken: L1, item 1.
13. **Major: A0 could raise a test's limit.** Right (`SPEC.md` section 13). Taken: no limit is
   raised; A0 splits or speeds up the tests, or stops and asks the owner.

Beside the round, the orchestrator passed on the owner's decision of 2026-11-22 that the GPU
host is Modal: draft 1's open question on the host became "What the owner decided", with the
origins named, and the account and its tokens became open question 1.

### Review round 2

On draft 2. Verdict: revise. Eight findings: 1 blocker, 7 majors; round 1's fixes for its
findings 2, 3, 6 to 9 and 11 to 13 were found to hold. Each was checked against the code and
taken.

1. **Blocker: Modal's limits were not shown to bound a request, and the host may run an input
   again.** Right (Modal's own retries after a crash and its preemptions). Taken: the bound is
   settled on paper first, in ADR 0046's opening section, from Modal's documentation alone;
   only documented limits count, no margin from a measurement; if a phase has none, the kind
   declares no bound on Modal, D1 and L1 wait and the owner is asked; the bridge refuses a
   request it has begun before.
2. **Major: a request frame carries no settings.** Right (`pack/proto.rs`, `pack/queue.rs`).
   Taken: the body is the `load` frame and the `request` frame of one queued request, taken
   whole in one turn of the front's queue, a load whenever model, installation or settings
   differ; tests of two clients with other settings, and after a restart.
3. **Major: the CPU front's containers and concurrency were not defined.** Right. Taken: one
   front container taking many inputs, so a cancel and a probe never wait behind a request;
   what a replaced front leaves is defined, with a test against fakes.
4. **Major: a cancel cannot be sent under the run's spent signal or deadline.** Right
   (`net/client.rs`, `capability/mod.rs`). Taken: a checked control send with its own short
   deadline and signal, from a new core constructor, allowed only to a dispatched request's
   origin, with tests after the signal, after the deadline and against a host that never
   answers.
5. **Major: the ledger's names would still grow the generation file.** Right
   (`contract/ledger.rs`, `spend/name.rs`). Taken: every namespace that grows (runs, request
   ids, the ledger, imports and outputs) goes into the buckets; the measurement uses a store
   as the engine API leaves one, and a CI test holds the root flat.
6. **Major: changing the adapters stales the committed evidence.** Right
   (`tests/it/evidence.rs`). Taken: the change kept to `adapter.py` where it can be, and P1,
   item 5: the orchestrator runs the model tier on the Mac (no spend) and the records and
   fixtures are written again by their runners in P1.
7. **Major: 2025-03-26 requires batches, and progress needs a token.** Right. Taken: bounded
   batches under 2025-03-26, refused under 2025-06-18; progress only under a token; the run's
   id in a header and the result's `_meta` either way; tests of each.
8. **Major: the client speaks HTTPS only, and the fake host was plain HTTP.** Right
   (`net/client.rs`). Taken: the front serves HTTPS when given a certificate; the tests use the
   net client's test PKI through the production path, with plain-address and wrong-root cases.

### Review round 3

On draft 3. Verdict: revise, with no blocker. Six majors, each checked against the code and
taken.

1. **Major: superseded bucket objects were never reclaimed.** Right (`store/mod.rs`: older
   generation files go at publication, objects at a prune). Taken: a publication removes the
   buckets it superseded that nothing holds, with a test of 1,000 publications and a held
   generation.
2. **Major: the cancel's 5 seconds did not cover name resolution.** Right (`net/client.rs`,
   `net/mod.rs`). Taken: the control send resolves nothing and connects to the peer the request
   reached, with the same TLS name and origin; tests with a stalled resolver and a stalled
   connect, handshake and answer.
3. **Major: stalled connections could take every handler of the front.** Right. Taken:
   deadlines on every phase, two of the eight connections kept for cancel and health, and tests
   that a cancel and a probe are answered with six requests stalled.
4. **Major: the shipped binary cannot trust the test root.** Right (`net/client.rs`,
   `wiring.rs`). Taken: an engine fixture beside the playground's, with the production code and
   the test root handed in; the shipped binary keeps no way to trust another root and is tested
   for its start, flags, token and signals.
5. **Major: two openings of `LOCK` on one machine cannot see locks that hold on one machine
   alone.** Right (`store/fs.rs`). Taken: the policy is a local file system; a store on a mount
   of a network type is refused by the system's own table of mounts, and the two-openings check
   stays beside it; tests from mount-table fixtures of both systems.
6. **Major: the inventory's secrets were left out of the row.** Right (`config/source.rs`,
   `wiring.rs`). Taken: the snapshot hands the inventory its secrets at every ask, with tests
   of a key given at the start and one added after it.

### Review round 4

On draft 4. Verdict: revise, with no blocker. Six majors, each checked against the code and
taken.

1. **Major: opaque frames lose the send check's two protections.** Right (`egress/hops.rs`,
   ADR 0019). Taken: the frames way takes a value only the check makes from typed parts, texts
   checked as texts and the picture as a canonical image, and the codec writes the wire from
   it; tests that path-like pixels pass and a path or the store's root in a text is refused.
2. **Major: a dropped batch lost its runs.** Right (`tools.rs`: `request_id` optional). Taken:
   every call in a batch carries a request id, `GET /v1/requests/<id>` answers the run, and a
   test recovers both runs of a dropped batch, cancels one and dispatches nothing more.
3. **Major: the front's control handlers were not reserved before classification.** Right.
   Taken: classifiers apart from handlers, with a bound on waiting connections and a 2-second
   classification, and a test with six requests and sixteen header-stalled connections.
4. **Major: the evidence runners refuse the change they must measure.** Right
   (`scripts/model-evidence.sh`, `tests/it/evidence.rs`). Taken: a candidate mode that measures
   the staged tree without a commit and records its tree id, with its script tests, in P1.
5. **Major: W1's catalogue changes stale the fixtures and nobody refreshes them.** Right
   (`models/catalogue.rs`, `tests/it/parity.rs`). Taken: W1 after P1, owning each revision it
   moves and the fixtures and evidence it stales, written again in the same commit.
6. **Major: a remote test under `parity::` enters the local model tier.** Right
   (`scripts/test-models.sh`). Taken: a module of its own, `remote_parity`, selected only by
   `parity-remote` with its config checked, and a script test that the default, `parity` and
   the local leg hold no remote case.

### Review round 5

On draft 5. Verdict: revise, with no blocker. Three majors, each checked against the code and
taken.

1. **Major: the candidate mode could measure the pack already installed.** Right
   (`tests/it/model_tier.rs`, `pack.rs`: the worker runs the installed pack). Taken: the mode
   builds the candidate's binary and installs its pack into a models directory of its own
   before measuring, and refuses when the launched sources are not the recorded digests.
2. **Major: blocking classifiers could hold a cancel behind stalled connections.** Right.
   Taken: one non-blocking classifier under `selectors`, 2 seconds from acceptance, 64 waiting
   at most, and a test of a cancel made after every slot is taken, timed from its acceptance;
   the flood it does not stop is a line in ADR 0046.
3. **Major: durable repeats and conflicts were not tested through HTTP.** Right (`SPEC.md`
   section 3, `contract/engine.rs`, ADR 0041). Taken: repeats go through the executor's
   fingerprinted lookup, with tests after a restart, after the table lets a run go and after a
   prune, a conflict for changed arguments, and a repeat at capacity and with its backend no
   longer ready.

### Review round 6

On draft 6. Verdict: revise, with no blocker. Two majors, both checked against the code and
taken (in draft 8: the edit that made draft 7 failed, so round 7 read draft 6 again).

1. **Major: 30 seconds between reads would end a request that waits, loads or computes.**
   Right (`net/mod.rs`, `pack/worker.rs`, `sam3.py`). Taken: the front sends a heartbeat event,
   a kind `pack/1` already has, at least every 10 seconds while a request is queued, verified,
   loading or computing; the whole request is held to its own deadline from `host_limit_s`;
   tests cover a long queue, a silent load and call, and a host that is truly stalled.
2. **Major: two cancels stalled in their bodies could take both control handlers.** Right.
   Taken: the classifier reads a control route's small body too, within its 2 seconds, so only
   a whole control message reaches a handler, whose answer is written within 1 second; a test
   stalls two cancel bodies and sends a third.

### Review round 7

On draft 6 again (above). Verdict: revise, with no blocker. Four majors: round 6's two, again,
and two more, both checked against the code and taken.

3. **Major: reading a whole body before parsing breaks `pack/1`'s rule that a bound is read
   before its bytes.** Right (ADR 0020, `proto.py`). Taken: the front parses as it reads,
   through the pack's own reader over a stream held to the body's length and deadline, with
   tests of an oversized frame header and a payload length that disagrees, the rest unread.
4. **Major: a GPU worker replaced under a front that stays could be asked without a load.**
   Right (`pack/queue.rs`, `worker.py`). Taken: whether to load is decided on the worker's side,
   keyed to its own process, with a test that replaces the card's worker under a front that
   stays.

### Review round 8

On draft 8. Verdict: revise, with no blocker. Three majors and a minor, each checked against
the code and taken.

1. **Major: heartbeats would spend `pack/1`'s budget of events and diagnostics.** Right
   (`pack/proto.rs`, `worker.py`). Taken, with finding 2, by taking round 6's other way:
   no heartbeat; the frames way keeps the client's limits for the connect, the handshake and
   the body, and waits for model work (first byte and between frames) only to the request's own
   deadline from `host_limit_s`. A stalled host is found at that deadline and charged at its
   bound, which section 6 charges a stall in any case.
2. **Major: nothing could cover the wait for the first byte.** Right (`net/mod.rs`,
   `pack/worker.rs`). Taken as above, with tests of a cold start, a silent load and a silent
   call past 30 seconds, and of a host silent past its deadline; the front sends its headers
   once the files are checked, and a failed check is an error answer with no frame.
3. **Major: the shared lifecycle calls a cancel that can block.** Right (`server.rs`,
   `contract/live.rs`). Taken: the playground's way, a held signal tripped at once and one
   canceller thread, with a test of a run held in its final publication.
4. **Minor: `/pack` twice in the probe's address.** Taken: `url` is the front's base and the
   routes are appended.

### Review round 9

On draft 9. Verdict: revise, with no blocker. Two majors and two minors, each checked against
the code and taken.

1. **Major: `remove` takes a location, not a mask, so the clicked flow could not run.** Right
   (`recipes/remove.toml`). Taken: clicked is `segment` with a point and one instance, then
   `fill` with its mask (`fill.rs`); a task that shapes a given mask first is proposed as a row
   for milestone E, not built; D1's test, the guide and L1 follow.
2. **Major: the engine API's handlers could be taken by connections stalled before their
   route is known.** Right (`play/http.rs`, `play/mod.rs`). Taken: a pool of readers apart from
   the handlers, 2 seconds from acceptance, the longest waiting closed past 32, handlers kept
   for the run routes, and a test with every reader held by incomplete headers.
3. **Minor: `models list` does not check every file.** Taken: `models verify sam3`.
4. **Minor: `CheckedProber` is in `egress/chains.rs`.** Taken.

### Review round 10

On draft 10. Verdict: revise, with no blocker. Two majors, both checked against the code and
taken.

1. **Major: the replay guard keyed on ids that are unique only per sender, in a store whose
   entries expire.** Right (ADR 0020, `pack/worker.rs`). Taken: a dispatch id of the run's
   random id and the attempt, a not-after stamped at the front's receipt, an atomic claim that
   fails closed, and tests of two clients, a restarted client, two deliveries at once and a
   stale not-after.
2. **Major: a cancel had no path to a call blocked on the card.** Right (`worker.py`; one input
   a container by default). Taken: the card's container takes a control input beside the call,
   which writes the cancel frame at once and restarts the worker at its deadline; a test whose
   stand-in admits exactly those inputs, and a live cancel in L1.

### Review round 11

On draft 11. Verdict: revise, with no blocker. Three majors, each checked against the code and
taken.

1. **Major: a run's id is a time and a count, not random.** Right (`contract/run.rs`). Taken:
   the kind mints a 128-bit dispatch id from the system's randomness for each request, kept in
   the record, with a test of two processes with their own stores at one clock.
2. **Major: a cancel found its request by a sender's `pack/1` id.** Right (`pack/worker.rs`,
   `worker.py`). Taken: the cancel names the dispatch id; the front gives the worker ids of its
   own per worker generation and finds every cancel by dispatch id; tests of two clients using
   `r1` and of a late cancel.
3. **Major: H1 could merge before S1's check of the file system.** Right (the carry-over row on
   the store's locks). Taken: H1 merges after S1, and a test that a store on a network mount is
   refused before anything listens.

### Review round 12

On draft 12. Verdict: revise, with no blocker. Two majors, both checked against the code and
taken.

1. **Major: the pools exceeded every spawner's limit.** Right (`threads.rs`,
   `play/threads.rs`, `test-thread-lint.sh`). Taken: the engine's own spawner of 32 threads,
   16 readers and 12 handlers among them, a sixth place in the lint, the other two unchanged,
   and a test of the whole engine saturated.
2. **Major: four stream handlers left no room for a repeat at capacity.** Right
   (`contract/engine.rs` answers it; the handlers did not). Taken: one stream a run, a repeat
   looked up before any handler and taking over its run's stream, an ended run answered as
   one body; tests of both at capacity.

### Review round 13

On draft 13. Verdict: revise, with no blocker. Three majors, each checked against the code and
taken.

1. **Major: batches had no place in the handlers' shares.** Right. Taken by narrowing: one
   revision over HTTP, 2025-06-18, which has no batches; its header is required, and a request
   without it, of another revision, or a batch, is refused 400. The batch items of rounds 2 and
   4 (and the route `GET /v1/requests/<id>`) are gone with it.
2. **Major: a front replaced while its worker survived could name or admit a second call.**
   Right (`pack/queue.rs`, `worker.py`). Taken: the lock of ordinary calls and the worker's ids
   are held beside the worker, a second call refused `busy` without the control input, with a
   test of a new front while the old call is blocked.
3. **Major: the suite's `Transport` cannot be the HTTP route as it stands.** Right
   (`tests/it/pack/conformance.rs`). Taken: the behavioural half restated over atomic requests
   for both transports, the worker observed over HTTP through the answer's error and the worker's
   generation in `pack-host/1`, every case kept in a table in ADR 0046.

### Review round 14

On draft 14. Verdict: revise, with no blocker. Three majors and a minor, each checked against the
code and taken.

1. **Major: no public lookup admits nothing.** Right (`contract/engine.rs`,
   `contract/request.rs`). Taken: S1, item 8, `Executor::look_up`, which H1 calls before any
   handler, with a test after a restart and a prune.
2. **Major: a load alone is not a request the route takes.** Right (`pack/queue.rs`). Taken: a
   load's failure is a failing load with its `segment`, the answer the load's error and the
   `segment` never run.
3. **Major: the tag could come before W1.** Right. Taken: W1 merges before L1 whatever the
   answer to open question 4; N1 and the tag wait for W1's box with a definite answer; a yes
   after L1 is its own pull request and L1's parity is run again.
4. **Minor: "the two revisions" in H1.** Taken.

### Review round 15

On draft 15. Verdict: revise, with no blocker. One major, checked against the code and taken.

1. **Major: what the supervisor knows beside the frames could not cross HTTP.** Right
   (`pack/worker.rs`, `Failed`; ADR 0020: error output is outside the frames). Taken: an
   outcome record of `pack-host/1` ends every answer, with a worker's status, stage and redacted
   error output; a body without it is `truncated`; the suite's cases of a worker's exit and its
   error output pass through the production HTTPS client.

### Review round 16

On draft 16. Verdict: revise, with no blocker. Three majors, all on round 15's outcome record,
each checked against the code and taken.

1. **Major: copying the worker's stage could settle a paid request at nothing.** Right
   (`pack/worker.rs`, `spend/meter.rs`). Taken: the stage is a diagnostic; a request whose body
   began to leave is charged at its bound, and only the client's own refusals before sending are
   settled at nothing; tests of a failed start and an early cancel on the host.
2. **Major: the record could not rebuild every `Failed`.** Right (`pack/worker.rs`). Taken: a
   typed cause beside the stage and an optional exit status, error output on a complete answer
   too; tests of a halt, a timeout and a cancel told apart, and of an error that leaves the
   worker alive.
3. **Major: 64 KiB could not hold the error output the supervisor keeps.** Right
   (`pack/worker.rs`: 256 lines of 4,096 bytes). Taken: a head of at most 4 KiB, then the lines
   raw and length-prefixed at the supervisor's own bounds, each length checked before its bytes;
   a test with the long-error-output fixture.

### Review round 17

On draft 17. Verdict: revise, with no blocker. Two majors, both checked against the code and
taken.

1. **Major: a call's method and request id are in its body, which the readers did not read.**
   Right (`server.rs`, `play/http.rs`). Taken: the readers read and parse every `POST /mcp`
   body before any handler is taken, under the body's rate after the headers' 2 seconds; a test
   of a repeat sent in two parts with four streams open and calls stalled in their bodies.
2. **Major: the record's causes were not the core's kinds, and `ended` lost two of three.**
   Right (`capability/fault.rs`, `pack/worker.rs`). Taken: `kind` one to one with `FaultKind`,
   the front's own reasons apart as `admission`, and `ended` tagged as `Ended` is; tests of a
   malformed and an oversized answer, and of each way a worker ends.

### Review round 18

On draft 18. Verdict: revise, with no blocker. Two majors and two minors, each checked against
the code and taken.

1. **Major: a replaced front dropped the cancel of the old front's call.** Right. Taken: the
   worker's side holds which dispatch its running call is; a front with no entry hands the
   cancel there, which acts only on that dispatch; a test through the cancel route.
2. **Major: a synchronous child's panic would not meet a catch at a started thread's edge.**
   Right (`contract/nested.rs`, `contract/engine.rs`). Taken: the catch is in
   `Engine::execute`, for every admitted run, with a test of a synchronous child.
3. **Minor: "and finds a cancel" twice.** Taken.
4. **Minor: L1's six removals are nine runs.** Taken.

### Review round 19

On draft 19. Verdict: revise, with no blocker. One major, checked against the code and taken.

1. **Major: a started run does not pass through `Engine::execute`.** Right
   (`contract/engine.rs`: `execute_and_let_go` calls `run_and_end` itself). Taken: the catch is
   around the shared `run_and_end`, the boundary both ways pass through, with `execute_and_let_go`'s
   order kept, and a test of a run entered through `Executor::start` beside the synchronous one.

### Review round 20

On draft 20. Verdict: revise, with no blocker. One major, checked against the code and taken.

1. **Major: a catch around all of `run_and_end` could turn a committed run into a failed one.**
   Right (`store/mod.rs`, `contract/engine.rs`). Taken: the catch acts by how far the ending
   had got (before visible: `panicked`; visible and unconfirmed: left as item 3 leaves it;
   durable: the outcome stands and only the rest is finished, once), with a test at each
   boundary through both ways in.

### Review round 21

On draft 21. Verdict: revise, with no blocker. One major, checked against Modal's documented
deploys and taken.

1. **Major: a rolling deploy runs two generations at once, so the bridge's lock and its running
   dispatch could be two.** Taken: a deploy is made only from a stopped app (`deploy` refuses
   while a container runs; `down` waits until none is left), the claim store is a named `Dict`
   outside the app, and a test whose stand-in has two generations.

### Review round 22

On draft 22. Verdict: revise, with no blocker. One major, taken.

1. **Major: no container running is not a stopped app.** Right: an app scaled to zero still
   takes inputs. Taken: `deploy` proceeds only on Modal's own word that the app is stopped and
   no container is left, refusing any other or unreadable state; `down` stops the app first;
   script tests of each state.

### Review round 23

On draft 23. Verdict: revise, with no blocker. One major, taken.

1. **Major: the guard refused the first deploy, since a new app is not listed as stopped.**
   Taken: a first deploy proceeds when the environment's list, read whole, holds no app of that
   name; an unreadable or partial list still refuses; a script test of it.

### Review round 24

On draft 24. Verdict: revise, with no blocker. Three majors, each checked against the code and
taken.

1. **Major: a failure before the files were verified cannot carry the identity every answer
   needs.** Right (`pack/worker.rs`, `Failed`). Taken: a failure-only answer, marked, with the
   outcome record alone, never a result, never cached, charged at the bound; any frame without
   a verified identity refuses the whole answer; tests of each.
2. **Major: the first bucket publication had no crash test.** Right (`tests/it/store.rs`:
   `with_old` makes its store with the present writer). Taken: a committed fixture of the
   present format, and a crash swept through every step of its first bucket publication.
3. **Major: WebP was promised and nothing decodes it.** Right (`play/guard.rs`,
   `whizbang-core/Cargo.toml`). Taken: uploads are PNG and JPEG, the two the core decodes;
   WebP is refused 415, with a test.

### Review round 25

On draft 25. Verdict: approve. No blocker and no major; the fixes of the earlier rounds checked
against the code, and later fixes found to supersede earlier ones where they say so. One minor:
the spend table's "6 runs" of user story 1 are nine runs of actions; taken. The reviewer noted
that the bound of a request on Modal stays a gate to be met on paper (P1, item 1) before any
deployment or live run, which this approval does not settle.
