# 0045. The engine API

Status: accepted.
Date: 2026-11-23
Spec: sections 1, 3, 7 and 10; plan m6 (`docs/plans/m6-http-and-remote-hosting.md`), task H1;
ADR 0007, ADR 0008, ADR 0031, ADR 0041

## Context

Section 1 names a door for "a hosted app that uses whizbang as its engine": the same server
as `whizbang serve`, over HTTP. User story 1 is its first caller: an app server with no GPU and
no Python uploads a picture, calls two or three actions, and downloads the result. Section 10
gives the engine API the rules of both HTTP surfaces (a bearer token, Host and Origin checked,
limits on request size, image dimensions and concurrent calls, tokens redacted) and its own:
loopback by default and another address only with a flag, TLS at a reverse proxy off
loopback, no local control, and no file path from the caller, "images arrive as bytes or by
reference to an upload". Section 3 asks that a request id outlive a restart, that a run be
found after its caller has gone, and that status and cancel answer while a run executes.

The playground (ADR 0041) is the first HTTP surface. This ADR records the second: what it
speaks, who may call it, how it holds its runs, and what it shares with the first.

## Decision

**It is written by hand, in `whizbang-serve`** (`crates/whizbang-serve/src/engine/`), on the
standard library, as the stdio server and the playground are. It shares with the playground
the code that reads an HTTP/1.1 request under its bounds and deadlines and writes an answer
(`crates/whizbang-serve/src/http.rs`, moved out of `play/` with no change of behaviour), to
which it adds an answer written as a stream, a piece at a time, each piece within its
deadline. It shares no route, no token, no guard and no listener with the playground. It adds
no crate.

**MCP's streamable HTTP transport, on one path.** `POST /mcp` takes one JSON-RPC message as
its body. One revision is offered, 2025-06-18: the latest the stdio server speaks, and the
first whose transport carries its version in a header. `initialize` answers 2025-06-18
whatever the client asks; every other request carries `MCP-Protocol-Version: 2025-06-18` or is
refused 400, and so is a batch, which that revision does not have. `initialize`, `ping`,
`tools/list` and `tools/call` are answered; any other method is answered with the protocol's
error for an unknown method. A notification and a client's answer are taken, 202, with no
body; `notifications/cancelled` cancels nothing, since a server that keeps no session cannot
tell one client's JSON-RPC ids from another's, and MCP lets a receiver ignore it. `GET /mcp`
is 405. The server makes no `Mcp-Session-Id`.

A `tools/call` is answered as an event stream (`text/event-stream`): the header `Whizbang-Run`
names the run before the body; then each of the run's progress messages, as
`notifications/progress` under the call's `progressToken` when it gave one and none when it
gave none, as over stdio; then the result, whose `_meta` names the run again
(`whizbang/run`); then the stream ends. A call refused before it made a run is answered as one
JSON body. The tools, and how a call of one becomes a call of an action, are the stdio
server's door (`whizbang_serve::Door`, `whizbang::serving::Actions`), unchanged: one
definition of the actions serves both, as section 3 asks.

**Plain routes beside it**, behind the same guard and token:

| Route | Takes | Answers |
|---|---|---|
| `POST /v1/uploads` | A picture's bytes, `image/png` or `image/jpeg`; or a mask as a grey PNG with `?mask_of=<image identity>` | `{"image": "sha256:…"}` or `{"mask": "sha256:…"}`, imported as `whizbang store import` imports one, the file's metadata not kept |
| `GET /v1/artifacts/<identity>` | Nothing | A picture as PNG, a mask as its grey PNG; 404 for one the store does not hold |
| `GET /v1/runs/<run id>` | Nothing | The run's state, and once it has ended its outcome and its outputs' identities, whether it ran here or not |
| `POST /v1/runs/<run id>/cancel` | Nothing | 202 and `stopping`; 200 and its end when it has ended, whoever started it, since a cancel then changes nothing (so a run this server started and no longer remembers is answered from the store); 409 for a run going that this server did not start, since one process owns every run it starts (ADR 0031); 404 for a run the store does not hold |

A call names its pictures by identity, as over stdio. No path is taken anywhere: any string in
any argument of any tool that reads as a path (`/` or `~` first, a drive letter, `file:`, a
`..` part) is refused 400.

**Who may call.** Every request passes these checks in this order; the first that fails
answers and closes. One function holds them (`engine::guard`), and the route table takes only
handlers given what it returns, as ADR 0041's does.

| Check | Refused with |
|---|---|
| The line and headers fit in 16 KiB, read within 2 seconds of the connection's acceptance | 431, 400, or closed |
| `Host` is one of the server's names at its port: `127.0.0.1`, `localhost` and `[::1]` on loopback, and each `--host-name` | 421 |
| `Authorization: Bearer <token>`, compared in constant time | 401, naming nothing |
| An `Origin`, when sent, is one listed by `--allow-origin`; none is listed by default; a request with no `Origin`, a server's own client, is taken | 403, with no allow-origin header |
| The path is a route; no field in the address but `mask_of`; JSON at most 1 MiB and 16 deep; a picture at most 64 MiB, of one of the two types | 404, 405, 400, 413 or 415 |
| A picture's bytes are of the type it was sent as (a PNG's signature, a JPEG's start; a mask a PNG); it is inside the pixel limit (25,000,000 by default, `--pixel-limit`), read from its header first; a mask is exactly its image's size | 415; 413, naming the limit; 400 |
| No string argument reads as a path; every artifact argument is an identity | 400 |
| A new run is started only while fewer than 4 run (`--calls`, 1 to 4: each run has a stream handler of its own, so the server refuses more at start); at most 2 uploads are read at once | 429, after the headers |
| A body arrives at no less than 1 MiB a second after the headers' 2 seconds; a plain answer is taken within 10 seconds; a stream's next piece within 30 | closed |

The browser's `Sec-Fetch-Site` rule of the playground is not applied: the caller is a
server, not a page. Every answer carries the playground's four protective headers and no
header that allows another origin.

**Reading is apart from serving.** The acceptor hands each connection to a pool of 16
readers, which read its line and headers and, for every route but an upload, its whole body,
and know its route, its method, its tool and its request id before any handler is taken. At
most 16 connections are read at once; a seventeenth makes the acceptor shut the one that has
waited longest, which ends that reader's blocked read. An upload is handed on after its
headers. Then a request goes to one of 12 handlers held to their shares: 4 for streams, 2 for
uploads, 4 for the run routes alone, 2 for every other short answer; so a stalled stream or
upload never takes the run routes' place. Uploads past 2 are refused 429 by the reader.

**One stream a run.** A `tools/call` that names a request id is looked up first, on the
reader, by the executor's durable, fingerprinted lookup, before any handler or place is taken:

- an id never admitted needs a place, and is 429 at capacity, making no run and no
  reservation;
- an id admitted under another request is a conflict, answered at once;
- a repeat of a run that has ended is answered as one JSON body, by a short handler, with the
  result the run's own call was given;
- a repeat that arrives while its run is being started waits, within the head's time, for
  the handler that starts it, and follows it as below; it is never told the run was given up;
- a repeat of a run going here takes over that run's stream: the older connection is closed,
  the run is not touched, and the new one is sent the rest of its progress and its result;
  so 4 stream handlers are never more than the 4 runs, and an app that reconnects at
  capacity is answered at once.

The lookup is the executor's, through the request id's durable admission, never the server's
table: after a restart, after the table let the run go, and after the run was pruned, the
same request is answered with its run, and a changed request under the same id is a conflict
(section 3). A closed stream cancels nothing. Two clients that use the same JSON-RPC id each
get their own run: a run is never found by a JSON-RPC id.

**Runs belong to the server.** One process owns every run it starts, in a table no
connection owns: at most `--calls` going, and 4,096 ended runs remembered, the oldest let go
first; a run let go of is still read from the store by its id. A run's stream handler holds
it for its whole life: it sends its progress when someone follows it, cancels it at its stated
time (`--wait-s`, 900 by default), and answers that it gave up once the grace has passed
after that. A run that ignores its cancellation keeps its place until it ends. No connection
and no watch of a time calls a cancel that can block: `Executor::cancel` can wait for a run's
final publication, so the cancel route and a stated time trip the run's held signal at once
and answer `stopping`, and one canceller thread calls the executor's cancel, once for a run
however often it is asked, and its wait at a stop.

**Where it listens.** `whizbang serve --http <addr>`. A loopback address needs nothing more.
Any other is refused at start unless `--off-loopback` is given with at least one
`--host-name`: off loopback the server stands behind a reverse proxy that ends TLS, which it
cannot see, so the flag is the operator's statement, and the start line says it was made.
Built-in TLS is deferred (section 10).

**The token.** `--token-env <NAME>` or `--token-file <path>`, exactly one, on loopback too: at
least 32 characters of printable ASCII, without spaces. A token file its group or anyone can
read, or another user's, is refused at start. It is read once; a changed token needs a
restart. It is held in a type whose `Debug` and `Display` print nothing, and is in no answer,
log, event, record or error.

**No local control.** The tools are the description's actions and nothing else: no model
install or removal, no spend reset, no config, no store command. `tools/list` holds none, a
call naming one is refused as an unknown tool, and no route reaches one.

**Stopping.** `SIGTERM`, `SIGINT` and `SIGHUP` stop the server as the playground's Quit does:
it takes nothing new, every run is cancelled and waited for up to the grace, the ledger is
accounted by those waits, and the store is let go; a second signal ends the grace. The
binary's reader of signals is the playground's (`signal-hook`'s safe iterator), so whizbang
now handles a signal under two commands, `play` and `serve --http`.

**Threads.** The engine's own counted spawner (`engine::threads`, threads named
`whizbang-engine-*`), the sixth place in the workspace that makes threads, refuses the 33rd
live thread it would make in a process: the acceptor, 16 readers, 12 handlers, the
canceller, the binary's reader of signals, and one place kept for the hourly collection of
uploads no run took (plan m6, task D1). The stdio server's spawner and the playground's are
unchanged. A test that starts the engine in its own process stays far inside the test kit's
bound.

**The store.** The server holds it as the one writer for as long as it runs, as the
playground does: its executor's writer, through which uploads are imported too.

## What it leaves

- **A start in progress at a stop.** A second signal ends the grace at once, but a handler
  that is inside `Executor::start` finishes that start (its preparation and the durable
  publication of its admission, bounded by the store's own writing) before the server's
  threads are joined; the run it makes is cancelled with the rest. Nothing waits on a run's
  action.

- **A flood of new connections before any token is shown.** More than 16 connections in
  2 seconds that never finish their headers keep each other from being read. Off loopback the
  server stands behind the operator's proxy, which is where such a flood is met.
- **Built-in TLS**, deferred by section 10.
- **Sessions, a push channel (SSE on `GET`), and batches.** Each is state to bound and expire,
  or a held connection per client, for nothing the stateless form lacks.
- **More than one token**, and a token changed without a restart.
- **A picture given inline in a job**, and squint's `expect` (`docs/carry-over.md`, "Two things
  of a squint job file"): pictures arrive as uploads alone, so one byte bound governs every
  picture that arrives, and an upload's identity is what the server answered, so there is
  nothing to expect.

## Consequences

- The binary gains a door and changes none: `serve` without `--http` is the stdio server,
  unchanged, and the playground's own guard and tests are unchanged.
- The playground's reading of HTTP lives in `whizbang-serve/src/http.rs`, for both surfaces.
- Reversing it removes `serve --http` and `engine/`; the HTTP reader can move back into
  `play/`.

## Alternatives

- **A library** (`rmcp`, a web framework, an async runtime). Rejected for the reasons of ADR
  0031 and ADR 0041: more code than the server has, for a few fixed routes.
- **Offering 2025-03-26 or 2024-11-05 over HTTP.** Rejected: 2024-11-05 had another HTTP
  transport, and 2025-03-26 asks a receiver to take batches, which hold a connection for
  several runs at once and need rules of their own for capacity and reconnecting.
- **Pictures inline in a call.** Rejected: a JSON-RPC message is held to 1 MiB (ADR 0031), and
  a picture is bigger; an upload has its own bound, its own handlers and its own count.
- **Following a run by its JSON-RPC id.** Rejected: a stateless server cannot tell one client's
  ids from another's; a run is found by its run id or its request id.
