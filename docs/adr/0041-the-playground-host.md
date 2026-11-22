# 0041. The playground host

Status: accepted.
Date: 2026-11-21
Spec: sections 1, 3, 7 and 10; plan m5 (`docs/plans/m5-the-playground.md`), task H1;
ADR 0008, ADR 0015, ADR 0031

## Context

Milestone 5 adds the playground, `whizbang play`: a page in Chrome that calls an API the
`whizbang` process serves. It is the first part of whizbang that listens on a socket, and the
first process meant to hold a store for days. Section 10 asks that the playground API listen
on loopback only, hand the page a token made at launch, validate Host and Origin on every
request, bound request size, image dimensions and concurrent calls, and keep tokens out of
logs, results and errors. Section 7 asks that closing the tab cancel nothing, that reopening
reconnect, and that a quit or a signal from the terminal cancel and account for running work
and let the store go. This ADR records how the host does each, and is the table of routes
every later task of milestone 5 adds to.

## Decision

**The host is written by hand, in `whizbang-serve`** (`crates/whizbang-serve/src/play/`), on
the standard library, as the stdio server (ADR 0031) and the download client (ADR 0024) are:
HTTP/1.1 over `std::net::TcpListener`, a request with a `Content-Length` body or none (no
chunked body, no upgrade, no keep-alive), one request per connection, a fixed table of
routes. It adds no crate to that library. The launch token is read from the system's
randomness through `getrandom`, which the shipped binary already holds (through `ring`).
The terminal's signals are taken in the binary alone (`crates/whizbang/src/play.rs`)
through `signal-hook`'s safe iterator, approved by the owner with the plan's open question 1,
so the workspace keeps `unsafe_code` forbidden.

**Where it listens.** `127.0.0.1:<port>`, 7341 unless `--port` says otherwise, and no other
address: there is no flag for one. The port is fixed so that the page's origin is the same
from one launch to the next, which keeps its drafts. A port in use is refused with a sentence
that names `--port`. `--port 0` asks the system for a free port; it is for tests and the
fixture host, and the address printed names the port taken.

**Who may call.** Every request goes through one function, `guard::admit`, before a route is
looked at, and the route table takes only handlers that are given what that function
returns (`Guarded`, which nothing else can make). The checks, in this order; the first that
fails answers and closes:

| Check | Refused with |
|---|---|
| The request line and headers fit in 16 KiB, read within 5 seconds of the connection | 431, or 400, or closed |
| `Host` is exactly `127.0.0.1:<port>` or `localhost:<port>` | 421 |
| For `/api/…`: `Authorization: Bearer <token>`, compared in constant time | 401 |
| For `/api/…`: an `Origin`, when sent, is exactly `http://127.0.0.1:<port>` or `http://localhost:<port>`; any method but `GET` must send one | 403 |
| `Sec-Fetch-Site`, when sent, is `same-origin` or `none` | 403 |
| The route takes no other field in the address; the body is at most 1 MiB for JSON (nested at most 16 deep, the stdio server's bound, and naming no field the route does not take) and 64 MiB for a picture, and its declared type is the route's | 400, or 413, or 415 |
| A picture decodes inside the core's `DecodeLimits` | 413, naming the limit |
| A body arrives at no less than 1 MiB a second after its first 5 seconds; an answer is taken within 10 seconds | closed |

One request from another site is let through the `Sec-Fetch-Site` check: the browser
going to a static page as a top-level document (a `GET` outside `/api/`, with
`Sec-Fetch-Mode: navigate` and `Sec-Fetch-Dest: document`), which is how Chrome arrives from
the opener page, a file it calls another site. It loads only the binary's public files, and
the page acts on nothing in its address but the token after `#k=`. Nothing under `/api/` is
taken from another site.

A refusal names the check and nothing the request held. Static files (the interface) are
served to any request that passes the Host check: they are the binary's own public code, and
everything of the person's store is under `/api/`. No response carries a header that allows
another origin. Every response carries `Cache-Control: no-store`, `X-Content-Type-Options:
nosniff`, `Referrer-Policy: no-referrer` and `Cross-Origin-Resource-Policy: same-origin`; the
page carries the content security policy of the plan ("The playground API").

**Threads.** One acceptor, which accepts and hands over and never reads; sixteen connection
workers; one keeper, which holds each run's stated time and the grace of a quit; one
canceller, which calls `Executor::cancel`, the one call of the executor that can wait. Each is
made by the host's own counted spawner (`play::threads`), the fifth place in the workspace
that makes threads (`clippy.toml`), which refuses the 45th live thread it would make in a
process: two hosts' twenty each (the twentieth is a host's one install, plan m5, task M1:
an install's thread wakes the keeper when it ends, and the next install waits for it to
end before its own is made, so a host never holds two), and room for the binary's readers of its signals and its
input and the waiter of a Chrome started on Linux. At most two
uploads hold a worker at once: a third is answered 429 as soon as its headers are read. Every
other request is a short read and a short answer, so polls, Stop and Quit find a free worker. A
worker that panics loses its connection, not itself. The workers are not joined when the
host stops: once the acceptor has ended, each ends after the connection it holds, which its
deadlines bound, and none holds the executor by then. The answer's deadline holds for the
whole answer, written in pieces, not for each write. A picture's size is read from its
header by the guard; it is decoded only by the route that keeps it.

**The token.** 32 bytes as 64 hex digits, made when the host starts, held in a type whose
`Debug` and `Display` print nothing, and never written to a log, an event, a record or an
error, and never on a command line, which other users of the machine can read: `whizbang
play` writes an owner-only page in the store's folder, `play-open.html`, that sends a browser
on to `http://127.0.0.1:<port>/#k=<token>`, opens that page in Chrome (in a process group of
its own, so a Ctrl-C meant for whizbang does not reach the browser), removes it when it
stops, and prints the address without the token. With `--no-open` it prints the address
with the token, which the person opens. The page keeps the token in its own storage for its origin and takes it out of
the address bar. A token of an earlier launch is refused, 401.

**Runs belong to the host.** A run the API starts is started by `Executor::start` with a
`Cancellation` and a deadline the host holds, and kept in the host's table of its runs; status,
cancel and the list of runs still going reach it there, so a page that closed holds nothing a
run needs, and a page that opens again finds it (`GET /api/runs?open=1`). One process, the
host, owns every run it starts, as ADR 0031 has it for the stdio server. At most four runs the
host started are going at once (the stdio server's `CALLS_LIMIT`), a start being admitted
among them; a fifth start is answered 429 with `Retry-After`, and no other route is held to
that limit. A place is given back once its run is seen to have ended, by the keeper or by the
route that counts or lists the places, whichever looks first. A start names a request id of
the page's making, and a repeated id is answered with its run (or, for another call, as a
conflict) before anything else is looked at; then the start is marked as needing a
preparation (`Request::needing_preparation`) and sent with the preparation the page's handle
names, when the host still holds it (at most 64, each for an hour). A quit that comes while a
start is being admitted trips its cancellation too, and a run that admission then makes is
cancelled and waited for as the others are. Two limits of the repeat's look-up, accepted: the host remembers the ids
of the runs it started (the newest 4,096, by the action and a digest of the arguments as the
page sent them); an id it does not remember that the store holds an admission under (one
of an earlier launch) is answered with the run that admission names, by the id alone,
before a place is taken and whatever is running (plan m5, task W1, its review): the host
holds no copy of what was asked then, and making the call again under a config changed
since would not give the call the id was given for, so another call sent under such an id
is answered with that run too, not as a conflict. And the host compares the arguments as
sent, not as filled in, so a retry in the same launch that writes out a default the first
request left out is answered as a conflict.

**A run's stated time.** Each run the host starts has a stated time, one hour unless
`--run-limit-s` says otherwise, at which the keeper trips its cancellation and hands it to the
canceller; cancelling a run cancels its children. The run's own deadline is a grace later, a
backstop for an action that reads its deadline and not its cancellation, so that the two do
not race and a run stopped at its stated time ends as cancelled. A cancellation is cooperative: a run that
ignores it keeps its place among the four, the page shows it as stopping, and Quit is the way
out.

**Stop.** `POST /api/runs/{id}/cancel` trips the run's held `Cancellation`, answers
"stopping" at once, and queues the run for the canceller, once a run however often Stop is
pressed. No connection worker ever calls `Executor::cancel`, which waits while a run is in
its final publication.

**Stopping the host.** Three ways, one code: `POST /api/quit`, `SIGINT`, `SIGTERM` or `SIGHUP`
in the terminal, and the end of standard input under `--stdin-quit`. The host first stops
taking requests (503 to any that arrive) and starts the grace of ten seconds; then it trips
every held cancellation, which never waits, and queues each run for the canceller. The keeper,
which holds the grace's time, only reads each run's status, which answers during a final
publication. The host waits within the grace for every run to end, and for every thread the executor
started (the runs those runs called among them) to let go of the store
(`Executor::on_threads`), then stops the pack's
worker, removes `play.json`, drops its store, which lets the writer's lock go, and exits 0. A
run that has not ended by then is left to be ended as interrupted by the next writer, and the
exit says how many. A second signal, or a second quit, during the grace ends the grace at
once, and the same steps follow.

**A fixture's script.** A request carrying `x-fixture-script` is refused, 400, by the
real host (the door's `Play::script`); the fixture host of the Chrome cases reads it to
steer its fake backends (plan m5, tasks U1 and W1). No page of the playground sends it.

**Reconnecting.** The host writes `<store>/play.json` when it starts, readable by its owner
only, with its port, its process id and the token, and removes it when it stops. A second
`whizbang play` on a store held by a live host reads it, asks that host's `/api/hello` with
its token, and on an answer naming the same store opens (or prints) its address and exits 0;
on a store held by anything else it is refused as today, naming the holder.

**Routes.** JSON unless said. H1 serves the first six; each later task adds its own rows in
a module of its own, whose table `play/routes.rs` lists by one line (`TABLES`).

| Route | What | Task |
|---|---|---|
| `GET /` and the interface's files | The built interface, embedded (`play/assets.rs`) | H1, U1 |
| `GET /api/hello` | Version, the store's folder, the backends' readiness (no probe), the allowance and what is left | H1, then W1 adds the kits |
| `POST /api/quit` | Stop, as above | H1 |
| `POST /api/prepare` | A preparation of a call, kept under a handle when it is ready; its readiness and what is missing by kind, each need's bound, a loop's attempts, and each choice of a backend prepared on its own | H1 (the handle), W1 (per choice, needs, plan) |
| `POST /api/runs` | Start a call with its handle and a request id | H1, W1 |
| `GET /api/runs/{id}?after=<n>` | The run's state and its events after the `n`th, every text escaped (from its journal once it has ended), what was asked, what it spent of what it reserved, its outputs and its verdict | H1, W1 |
| `POST /api/runs/{id}/cancel` | Stop a run | H1 |
| `GET /api/runs?open=1` | The runs the host started that are still going | H1 |
| `GET /api/runs?kit=&task=&result=&words=&action=&model=&backend=&use_case=&matrix=&since=&until=&output=&before=&limit=` | History: the runs of the store that nothing nested, newest first, at most 200 a page after the run `before` names, under `runs stats`'s filter and the page's; each with its task, words, pictures' names, result, models, time, cost, start, a picture and its outputs (port, type, identity). `output=<type>` keeps the runs that made an output of that artifact type: what the workspace's binding picker offers a mask, a location or any other port, from the store, so a reload or a restart keeps it. Read from the store's records once a generation, kept, and read again only when the store has published since (`play/history.rs`) | W3, the m5 gaps |
| `GET /api/stats?` the same filter | The figures of `runs stats` (`stats::Figures`, the one aggregation the command prints) as JSON, with the command's text | W3 |
| `GET /api/runs/{id}/report` | The run's report: one HTML document with its pictures inline as `data:` and no script, served as a download (`Content-Disposition: attachment`) under `Content-Security-Policy: default-src 'none'; img-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; sandbox`, which its own `<meta>` repeats but for `sandbox`; the page shows it in `<iframe sandbox="">` by `srcdoc` | W3 |
| `GET /api/runs/{id}/export` | The run's archive, a tar file: its record, its report, each output's canonical file and a PNG of each picture or mask, under the same headers | W3 |
| `GET /api/records`, `/api/records/{name}`, `/api/records/{name}/{file}` | The names of the tool records, a record's Markdown, and its pictures: `docs/reference/tools/` embedded from a fixed list held to the folder by a test (`play/records.rs`) | W3 |
| `POST /api/pictures?name=` | A picture's bytes, decoded once, decoded within `DecodeLimits` by the route (the guard reads only its header's size) and kept as `store import --picture` keeps one, through the host's writer, with its name beside it under `play/pictures/` in the same publication | W1 |
| `GET /api/pictures?after=&limit=` | The pictures the playground kept, newest first, with their names | W1 |
| `POST /api/locations` | A box, a polygon (at most 64 points), a stroke or a line drawn on a picture, committed as a location bound to it | W1 |
| `GET /api/artifacts/{id}` and `…/thumb?edge=` | An artifact for display: a picture as its PNG, a mask as a grey picture, a location as JSON; a thumbnail made on request; at most two thumbnails and two whole artifacts made at once (one more is answered 429 with `Retry-After`, which the page waits out), the newest 64 thumbnails kept in memory | W1 |
| `GET /api/runs/{id}/record` | A run that ended, as the result screen reads it: outcome, verdict with its reasons and places, a loop's attempts with each one's picture (a diagnostic), the cost of the run and every run it called by capability and the config's name of the backend (free local work said as free), its inputs and outputs, the drafts Send to opens and the draft Run again opens; every text escaped; 404 for no run, 409 for one with no record yet | W2 |
| `GET /api/runs/{id}/command` | The `whizbang` command that makes the same call, from the binary's map beside its commands (`command_of.rs`): inputs by identity, files by absolute path, defaults written out, each flag with its value as one word, held to the command line's own parser, every word quoted for a POSIX shell, and a line `: '…'` when the config's digest, or a recipe file's, is no longer the run's; 422 when no command line says the call, or its words hold a control or format character other than a new line or a tab | W2 |
| the rest of the plan's table | Models, backends, spend | M1 |

No route takes a file path.

## Consequences

- The playground needs no web framework, no async runtime and no crate in `whizbang-serve`
  beside `getrandom`, already shipped; the binary gains `signal-hook` and
  `signal-hook-registry`.
- Only `whizbang play` handles a signal; no other command changes how it ends. The
  carry-over row on the store's lock gains that line.
- A store held for days is made safer in the same task: a writer notices that its `LOCK`
  was replaced before it publishes, and a store whose folder is another user's, or that its
  group or anyone may write, is refused when it is opened, to read or to write.
- Reversing it removes `whizbang play` and the module; nothing else reads it.

## Alternatives

- **A web framework and an async runtime.** Rejected: more code than the host has, for a
  handful of fixed routes on loopback, in a binary that has neither.
- **The token in a cookie.** Rejected: a cookie rides on a forged request from another
  page; a header set by the page's own script does not.
- **Events pushed over a held connection.** Rejected: a held connection costs a worker for
  as long as a run is open; a poll every 250 ms costs one short request.
- **A random port.** Rejected: the page's origin would change at every launch and its
  drafts would be lost. `--port 0` stays, for tests.
- **Calling `Executor::cancel` from the route or on the grace's thread.** Rejected (the
  plan's review rounds 3 and 4): it waits while a run is in its final publication.
