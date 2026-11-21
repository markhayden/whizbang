# Plan: milestone 5, the playground

Draft 5, 2026-11-20, written from `main` at 722083fc (the merge of #259, ADR 0040, the words
the playground uses), after five outside review rounds ("Review rounds", at the end). Milestone 4 is closed (`docs/plans/m4-note.md`). Milestone 5 is
`SPEC.md` section 16, row 5, "Playground": "Every kept playground item in section 7 passes
its acceptance case in Chrome; the playground API's negative tests pass; old repos
archived". Beside the row it takes squint's row "Report viewer and export" (section 7, kept,
milestone 5), the Chrome tier of `ci` (sections 13 and 14), every carry-over row that names
milestone 5 (the table "Carry-over rows owned by milestone 5", below), and the by-hand check
of the loop that milestone 4 left to the playground (`docs/carry-over.md`, "Acceptance
checks moved here"). It ends with the note of milestone 5 and, on the owner's yes, the
archiving of the four old repos.

Milestone E's first slice (the default judge chosen again, the submission format, the loop's
retries on the owner's pictures) runs beside this milestone, as decided on 2026-11-20
(`docs/plans/m4-note.md`). It is not planned here; the two share no source file but
`docs/carry-over.md` (see "Shared files").

**Words this plan uses.**

- **The playground** is what a person sees in Chrome. **The host** is the part of the
  `whizbang` process that serves it: the playground API and the built interface, over
  loopback HTTP, in `whizbang-serve`. `whizbang play` starts the host.
- **The API** is the playground API of `SPEC.md` section 10, "Two HTTP surfaces". It is not
  the engine API of milestone 6 and shares no route, token or listener with it.
- **Kit, task, tool, model, run** are ADR 0040's words, and are the only ones the screens use.
  In code and in this plan's names of code, a task is a recipe, a task that uses another kit
  is a loop, a tool is a primitive, and a call of any of them is an action.
- **A draft** is a task with its inputs filled in and not yet run. **A result** is a run that
  ended. **Send to** hands a result's outputs to another task as that task's inputs, as a new
  draft.
- **A screen** is one of the seven designs of the Paper file "whizbang playground", page
  "Playground · hybrid" (Home; the kit workspace; Sent to Squint; Running; Result; History;
  Models). **A component** is one of the pieces on its page "Components".
- **A Chrome case** is one end-to-end test in `ui/e2e/`, run in Google Chrome against the
  host of a real build, with fake models (W1, item 1). Each is named here by its file and its
  title, as a "Done when" names a Rust test.
- **The fixture host** is that build: the host and the interface as they ship, with the test
  kit's fake segmenter, a scripted fill and a scripted judge in place of real models. It is an
  example target of the binary's crate, so it may use the test kit, a development dependency,
  and the binary itself never holds a fake.

## What the spec says, and where it is silent

**What it says.** Each of these binds the tasks below.

- Section 1: the playground is a door, `whizbang play`, "A person trying tools in a browser".
  Kits will grow: "Adding another must not require changes to existing kits or to the
  doors", and success criterion 5 says the same. The playground is a door, so a third kit
  must appear in it without a change to the interface's code.
- Section 3, "The playground": "The browser calls a host API served by whizbang; the host
  runs tools through the executor inside its own process. The only other process is the
  optional Python worker. Chrome is the only supported browser."
- Section 3, "Storage", "One writer": "While `whizbang play` or `whizbang serve` holds a
  store, command-line actions that write to it are refused until that process exits. Reads
  are always allowed."
- Section 3, "What every run records" and "Metrics from ordinary runs": "The playground shows
  the same figures in milestone 5" as `whizbang runs stats`.
- Section 3, "The execution contract": prepare "returns readiness for these exact arguments
  and a bounded estimate", with "the plan" of a loop; admit refuses a stale preparation;
  "Finding a run. A run can be found and read after its caller has disconnected".
- Section 6, "Readiness": what a build can do is static (`whizbang describe`), whether a call
  can run now is prepare, and reachability is an explicit probe. "Controls from
  table-manners" keeps "Per-call and per-choice readiness", model installs that are atomic,
  cancellable and attach to a running install, a removal that drains, and drops "Renders
  declared as commands for a host to run": "the playground shows reports itself".
- Section 7, "Playground": ten rows, all kept, all milestone 5, one deferred to milestone 7,
  one dropped. Squint's row "Report viewer and export": kept, milestone 5. These eleven kept
  rows are this plan's acceptance list ("Acceptance: every kept row, its task and its Chrome
  case").
- Section 10, "Two HTTP surfaces": the playground API listens on loopback only; a token made
  at launch and handed to the page; Host and Origin validated on every request; local
  controls (install models, change config) allowed; file paths from the caller only inside
  allowed folders; limits on request size, image dimensions and concurrent calls; tokens
  redacted; model and tool text escaped before display; report content shown in an isolated
  origin. Negative tests at this milestone: an unauthenticated call, a wrong Host, a
  cross-origin request, an oversized image.
- Section 10, "Egress": default deny; every run records where data went. The playground adds
  no recipient: it sends nothing off the machine itself, and the runs it starts go only where
  the config already approves.
- Section 12: `ui/` holds the playground interface.
- Section 13: the fast tier holds "UI type checks and unit tests"; the slow tier holds
  "Chrome end to end", run "on every pull request, from milestone 5". Section 14: `ci` runs
  "Chrome end to end, from milestone 5". The fast tier's limit of 300 seconds of test time
  stands, and is not raised without the owner.
- Section 16: the old repos "are archived publicly, read-only, with a pointer here, once
  milestone 5 ships and every kept item in section 7 has its replacement. They are never
  deleted." Section 15: changing an old repo "beyond what the freeze allows" is never done by
  an agent.

**Where it is silent, or where the design asks more than the engine has.** Each is settled in
"Choices this plan makes", is a task's item, or is an open question; none is settled
silently.

1. The spec does not say how the page gets its token, what a reopened tab does, or how the
   person stops the host. Settled in "The playground API".
2. The spec does not choose a front-end stack, say how the interface ships in the one binary,
   or where the Chrome tier runs inside the budgets. Settled in "Choices this plan makes";
   adding Node and npm packages is open question 1.
3. "Change config" is a local control the API may offer (section 10), but whizbang never
   writes a config: its reader is read-only, by design (`Cargo.toml`, the note on `toml`).
   This plan writes no config from the playground. It shows the config; the allowance is the
   store's ledger, which `spend reset` already sets (open question 2).
4. "File paths from the caller: inside allowed folders" names folders the spec does not
   list. This plan takes no path from the page at all: pictures arrive as bytes, results
   leave as downloads (stricter than the spec allows, and so inside it).
5. The design shows things the engine cannot back yet. They are findings, listed next; none
   is invented by a task.

## What the design shows that the engine cannot back yet

Read from the Paper file on 2026-11-20 against `main` at 722083fc. Each row is either left
out of the build, shown in the engine's own terms, or added by a named task item; the right
column says which. None changes the agreed design's structure.

| Screen element | What the engine has | What this plan does |
|---|---|---|
| Models: "Allowance · Monthly $10.00", "$2.48 spent this month"; History's footer "this month $2.48" | A store's allowance is one figure, set by `whizbang spend reset`, which closes the books (`crates/whizbang/src/spend.rs`); nothing is monthly | Shown as "since the last reset" with the reset's date; the playground's reset is `spend reset` (M1, item 4). A monthly allowance is open question 2 |
| Models: "Per run $1.00" as a control | The config's `spend.call_cap_usd`; whizbang never writes a config | Shown read-only, with the config file named (M1, item 4) |
| Models: "Local judge · Get · 3.4 GB"; the kit workspace's "Local · on your Mac · GET · 3.4 GB" judge choice | No local judge until milestone 8 (`SPEC.md` section 5) | Not shown. The choice list shows the judges the config offers and the catalogue's models only (W1, item 4) |
| Models: "LaMa · fills small areas · READY"; History's "Repair a scratch · lama · local" | No local fill until milestone 8 | Not shown |
| Models: "Depth · downloading" | No such catalogue entry; the shipped catalogue holds `sam3` and `tiny` (`crates/whizbang/src/catalogue.json`) | The list is the catalogue's; the design's row shows how an install in progress looks |
| Models: "downloading · 1.1 of 1.4 GB" and a bar | An install reports files done of files (`whizbang_backends::models::status`, `Status::files_done`), not bytes | Shown as files done of files, with each file's size from the catalogue; a byte count is not added in this milestone |
| Kit workspace: READY, NO KEY and GET · 3.4 GB on each choice; Models: KEY SET, NO KEY, NOT SET UP | Preparation answers for one choice at a time, and why a capability is not ready reaches a caller only as text (`contract::prepare::Missing::why`); the typed reasons below it (`config::offers::NotReady::Missing`, `capability::inventory::Unready`) are dropped on the way | The host prepares once per choice; `Missing` keeps its reason as a kind beside the text (K1, item 1), so the page never parses a sentence |
| Kit workspace: the plan block's lines "find · SAM3 local", "fill · $0.04 × ≤3", "judge · $0.002 × ≤3" | A preparation holds its total estimate and each need's capability and choice (`PreparedCall::estimate`, `needs`); a loop's `Plan` holds request counts and attempts; no bound per need or per step is handed out | Each need's bound and request count are handed out (K1, item 2); the block's lines are needs, by capability, not recipe steps |
| Running: the live step list ("find · SAM3 found 1 hose", "shape", "fill" running, "judge" waiting) across attempts, with the attempt divider | `Executor::status` gives text events; a recipe's step names are added to its record when the child ends, and the parent's record is published at its end; a loop's attempts are in its record only | Typed events: a step started and ended, with its name, its child run and its attempt, are published as the run goes (K1, item 3); "waiting" steps come from the recipe's own list of steps |
| Inputs that are lists | The description has no list type for an argument; a list is a variadic port, a list of artifacts (`InputPort::variadic`, `Count`) | Lists are variadic ports, shown as an ordered list of pictures; no list type for arguments is added |
| A location drawn on the canvas | A location is a `Location` artifact (`artifact/location.rs`: box, polygon, line, stroke) passed by its identity, so it must be in the store first | The host commits the drawn shape through the store's import path before the call that uses it (W1, item 3) |
| Models: "SAM3 · Get" when the model needs its maker's sign-in | `models get sam3` fetches with the user's own sign-in, from the env file (`docs/guides/models.md`) | Get works when the env file names the sign-in; otherwise the row says what is missing and links the guide, and nothing is asked for in the page (M1, item 2) |
| Home: the "Pixel · Make pictures · coming" card; the kit switcher's "Pixel · coming" | No third kit; adding one is the owner's ("Ask first", section 15) | Not shown: the kits listed are the description's (W1, item 2) |
| Kit cards: "6 tasks · 40 tools", a kit's colour, icon and tagline ("edit pictures") | The description names each action's kind but no kit, no title a person reads and no tagline | The description gains a kit and a title for each action and a short line for each kit (W1, item 2), so a new kit needs no change to the interface |
| Result: "Recorded", a folder of the run under the store with `result.png · mask.png · report.json` | The store is content-addressed; a run has no folder of named files | Shown as the run's id with "Export", which writes the files `whizbang runs export` and the artifact export already write, as a download (W3, item 5) |
| Run ids such as `r-8c21` | Run ids are longer (`RunId::minted`) | Shown whole in the mono face, cut in the middle where space is short, never shortened in what is copied |
| Running: a progress bar inside the step "fill" | A capability request reports no fraction done; a run's events are messages | The running step shows elapsed time and an indeterminate bar (U1, the step list component) |
| Kit workspace: "FAL flux-fill" in the plan | The fill backend is Bria's eraser through fal, named by the config | The plan block prints the backend and model the preparation resolved, never a fixed label (W1, item 5) |
| Home, ⌘K search and "Pictures": a library of the person's pictures with their file names | The store holds images by identity; `store import` publishes an artifact under a name that says it was imported (`import.rs`, `IMPORTED`), with no name a person gave it | The host imports through the same path with its own writer, and publishes beside the artifact, in the same generation, a small entry holding the file name it came with (W1, item 3) |
| Squint's "Check an edit" with Source, Edit and "What should have changed" as separate fields; "Send to" filling them | A use case takes `images` (a variadic port) and `job` (text holding a `job/1`, ADR 0028), whose images carry roles; the descriptor says nothing of roles | The host builds the job from the fields as `squint check` does (`Job::from_shorthand`), and Send to from a removal writes the job the loop writes (`whizbang_rascal::checked::job`) (W1, item 4; W2, item 3) |
| Home, "Continue": drafts | A draft is not a run, and the store holds runs | Drafts live in the browser's own storage for the host's fixed address (H1, item 2), and are a convenience: losing them loses no run |
| Result: "Difference" compare mode | No action makes a difference picture for display | Drawn in the page from the two pictures it already has; no action is added (U1, the compare component) |

## Choices this plan makes

Each is recorded in ADR 0041, "The playground host", written by H1 before code depends on
it, except the stack, which is ADR 0042, "The playground's interface", written by U1.

**The host is written by hand, in `whizbang-serve`, on the standard library.** Like the MCP
server (ADR 0031) and the download client (ADR 0024), whizbang speaks a narrow protocol of
its own rather than adding a web framework and an async runtime to a binary that has none:
HTTP/1.1 over `std::net::TcpListener`, requests with a `Content-Length` body only (no
chunked uploads, no upgrade, no keep-alive), one request per connection. The routes are
few and fixed. The launch token is read from the system's source of randomness through
`getrandom`, already in the lock file. The terminal's signals need code the workspace
forbids writing (`unsafe_code` is forbidden), so they are taken through `signal-hook`'s safe
iterator (MIT or Apache-2.0, with `signal-hook-registry`; `libc` is in the lock file
already): two new crates, the binary's alone, which is open question 1 (H1, items 2 and 6).

**Live progress is polled, not pushed.** The page asks for a run's state and the events
after the last one it has, every 250 ms while a run is open on screen
(`GET /api/runs/{id}?after=<n>`). The host answers from the executor's status in its own
process, which already holds the events of a run that executes there (ADR 0008, "Status,
waiting, and a run that was started"), asking only for the events after `n` (K1, item 4). A held connection per open run would cost a thread
each; a poll costs one short request. Reconnecting after a closed tab is the same request.

**The interface is TypeScript with Preact, built by Vite, tested by Vitest and Playwright.**

| Need | Choice | Why |
|---|---|---|
| Components a designer can move to Storybook later | Preact function components, each in its own file under `ui/src/components/`, taking props only and reading no application state | Preact has React's component model at about 4 KB; Storybook supports it; a component that takes props only moves as it is |
| Tokens | One file, `ui/src/tokens.css`, of CSS custom properties, the Paper file's names (`--color-panel`, `--text-md`, `--radius-md`), with the dark values under `:root` and the light values under `[data-theme="light"]` | Dark is the only theme shown (`<html data-theme="dark">` is fixed). The owner keeps the light tokens and a light board in the design so light can return; the file keeps them, and nothing else is built for light. A native app later can read the same names |
| Type checks and unit tests (fast tier) | `tsc --noEmit` and Vitest under `happy-dom` | Run in seconds with no browser; Vitest shares Vite's config |
| Chrome end to end (slow tier) | Playwright's test runner driving the runner's installed Google Chrome (`channel: "chrome"`) | No browser download in CI: both hosted images carry Chrome. Chrome only, as section 3 says; WebKit is dropped (section 17) |
| Build | Vite, to fixed file names (`index.html`, `app.js`, `app.css`, `fonts/…`, `brand/…`), no source maps, no hash in names | The host embeds a fixed list of files; the same sources give the same bytes, which item "How it ships" checks |

Rejected: plain TypeScript with no framework (seven screens and about thirty components with
state would grow a framework of its own); React (ten times the size for nothing this needs);
Svelte and Lit (a compiler or custom elements are a further thing to learn and to move to
Storybook); a CSS framework (the tokens already are the system).

**How it ships.** The built interface, `ui/dist/`, is committed, as the generated reference
is: `just ui-build` writes it and `just check` fails on a stale copy (`scripts/ui.sh check`,
like `just docs-check`). The host embeds each file with `include_bytes!` from a fixed list in
`crates/whizbang-serve/src/play/assets.rs`, and a test fails on a file in `ui/dist/` that the
list does not hold, or the reverse. So `cargo build`, `scripts/release-build.sh` and the
release archive need no Node; the archive still holds its five files, the interface being
inside the binary; and the bytes that ship are the bytes that were reviewed and scanned. The
fonts (Geist, Geist Mono and Bricolage Grotesque, each under the SIL Open Font Licence) are
files under `ui/public/fonts/`, served by the host; the page loads nothing from anywhere but
its own host, and its content security policy says so. Their licences, and Preact's and marked's (MIT),
join `THIRD-PARTY-LICENSES.md` through `scripts/third-party-licences.sh`, which gains the
packages of `ui/package-lock.json` that reach `ui/dist/` and the font files, and fails on one
whose licence is not on `deny.toml`'s list or, for a font alone, is not OFL-1.1. The outlined
wordmark is drawn from Roboto Slab (Apache-2.0), whose licence joins the file too.

**The brand.** The wordmark "whizbang" is committed as an SVG with its letters drawn as
outlines (the design sets it in Roboto Slab Black, so no font file is needed for it); the
one-colour mark as an SVG; the emblem and the crew illustration as WebP files, each at most
400 KB. They are the owner's own art, so committing them is the owner's to allow (open
question 3); they are not copied by this plan, and they go under `ui/public/brand/` in W3's
pull request, from files the owner hands over. Until then the Home screen shows the wordmark
and an empty frame where the art goes.

**Where the Chrome tier runs.** Not in the fast tier: the 300 seconds of test time are
untouched by it. `ci` gains two jobs, `e2e (macos-15)` and `e2e (ubuntu-22.04)`, beside
`check`, `scripts` and `release`, and the aggregate `ci` needs them. Each builds the fixture
host (an example target, cached by the same Rust cache), installs the UI's packages from
`ui/package-lock.json` with `npm ci` (cached), and runs `just e2e`: Playwright against the
fixture host, one worker, each case with a timeout of 30 seconds and the whole run held to
180 seconds of test time (`scripts/e2e.sh` reads the run's own report of each case's time,
prints the seconds by file, and fails above the limit, as `scripts/timing.sh` does for the
Rust tests).
Each job has a timeout of 15 minutes, as the others do. Pull-request CI stays under its 10
minutes because the job runs beside the others; the timing report of each pull request names
the job's seconds. On the owner's Mac `just e2e` runs the same cases with the Chrome
installed there.

**Fast-tier cost.** `ui-check` (the type check, the UI's unit tests, the freshness of
`ui/dist/` and its asset list) joins `just check`, `check-full` and `ci-without-scripts`, so
the `check` jobs run it on both runners; those jobs gain Node and `npm ci` as the `e2e` jobs
do. Its test seconds are counted with the Rust tests' by `scripts/timing.sh`, under the one
limit of 300 seconds (`SPEC.md` section 13), not beside it. The hosted macOS runner took 133
to 262 seconds before this milestone, so milestone 5 may add at most 25 seconds there in
all: the UI's unit tests at most 10, the Rust tests at most 15 (about 2 a pull request, 4
for H1 and K1). Each pull request reports its `just check` and CI test times by suite, and
stops and says so if a run passes about 250 seconds; then coverage is reviewed first, as
section 13 says, and the limit is not raised without the owner.
`scripts/tests/test-ci-jobs.sh` gains cases that a type error and a stale `ui/dist/` each
fail `ci-without-scripts`.

## The playground API

All under `http://127.0.0.1:<port>/`. The port is 7341 unless `whizbang play --port <n>`
says otherwise; it is fixed so that the page's origin is the same from one launch to the
next, which is what keeps a person's drafts (Home, "Continue") across a restart. A port in
use is refused with a sentence that names `--port`. The host never listens on any other
address, and has no flag that would.

**Who may call.** Every request passes these checks, in this order, before a route is
looked at; the first that fails answers and closes.

| Check | Refused with |
|---|---|
| The request line and headers fit in 16 KiB, one request per connection | 431, or 400 |
| `Host` is exactly `127.0.0.1:<port>` or `localhost:<port>` (a defence against a name that a hostile page points at loopback) | 421 |
| For `/api/…`: `Authorization: Bearer <token>`, compared in constant time with the launch token | 401 |
| For `/api/…`: an `Origin`, when sent, is exactly `http://127.0.0.1:<port>` or `http://localhost:<port>`; a request that changes anything (any method but `GET`) must send one | 403 |
| `Sec-Fetch-Site`, when sent, is `same-origin` or `none` | 403 |
| The body is at most 1 MiB for JSON and 64 MiB for a picture, and its declared type is the route's | 413, or 415 |
| A picture decodes inside the core's limits on pixels and dimensions (`DecodeLimits`, 50,000,000 pixels) | 413, naming the limit |
| Time: the request line and headers arrive within 5 seconds of the connection; a body at no less than 1 MiB a second after its first 5 seconds; the answer is taken by the client within 10 seconds. A connection past any of these is closed, so a client that trickles bytes or reads nothing holds a worker for seconds, not for ever | closed |

Connections are served by sixteen workers; the acceptor only accepts and hands over, and
never reads. The only requests that hold a worker for long are uploads, and at most two run
at once: a third is answered 429 as soon as its headers are read. Every other request is a
short read and a short answer. So a worker is held for at most the header deadline by a
client that trickles, for an upload's time by at most two uploads, and otherwise for
milliseconds, and polling, Stop and Quit find a free worker. In the page, the client of the
API sends at most two uploads at once and queues the rest, so of Chrome's six connections to
one host at least four are left for polls, Stop and Quit. A local process that opens many
connections and trickles can still slow the host for as long as it keeps doing so; a local
process can as well stop the host outright, and the playground does not defend against its
own user's other programs. **The calls' limit is not part of the guard**:
it applies to `POST /api/runs` alone, which answers 429 with `Retry-After` when four runs the
host started are still going (the stdio server's `CALLS_LIMIT`); every other route is
answered whatever is running. Model installs are not on the connection workers: the host runs
at most one install at a time on a thread of its own (M1), and a second asked for answers
that one is going, or attaches to it when it is the same model.

No response carries a header that allows another origin, ever. Every response carries
`Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`
and `Cross-Origin-Resource-Policy: same-origin`; the page itself carries a content security
policy of `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
img-src 'self' blob: data:; frame-src 'self'; object-src 'none'; base-uri 'none';
form-action 'none'; frame-ancestors 'none'`. Inline styles are allowed because a document
given to a frame by `srcdoc` inherits this policy, and a report and a record carry their
styles inline; scripts are not. Static files
(the interface) are served to any request that passes the Host check, since they are the
binary's own public code; everything about the person's store is under `/api/`.

**The token.** 32 bytes from the system's randomness, as 64 hex characters, made when the host
starts and never written to a log, an event, a record or an error. `whizbang play` opens
`http://127.0.0.1:<port>/#k=<token>` in Chrome (`open -a "Google Chrome"` on the Mac,
`google-chrome` on Linux; `--no-open` prints the address instead) and prints the address
without the token. The page reads the token from the address, keeps it in its own storage for
that origin, and removes it from the address bar at once. A token from an earlier launch is
refused (401), and the page then says to run `whizbang play` again, which opens the address
with the new one.

**A run's stated time.** Each run the host starts has a deadline, one hour by default
(`whizbang play --run-limit-s`), at which the host cancels it, as the stdio server does at its
stated time (ADR 0031); cancelling a run cancels its children (`SPEC.md` section 3). A
cancellation is cooperative: a run becomes terminal only when its children are and every
dispatched request is accounted for, so a run whose action ignores the cancellation keeps
going. The host does not pretend otherwise: such a run keeps its place among the four calls
(a place is never given back while its run is not over, so no threads pile up behind
released places), the page shows it as "stopping", and the way out is Quit, which waits the
grace and exits, leaving the run to be ended as interrupted by the next writer. Every shipped
action checks its cancellation between steps and requests, so this is the case of a bug or a
user's tool file, not of ordinary use.

**Reconnecting.** Closing the tab cancels nothing: runs belong to the host's process, not to
the page (`SPEC.md` section 7). Reopening the address, or running `whizbang play` again,
which finds the host already serving that store and opens its address instead of failing
(the host keeps its port, process id and token in `<store>/play.json`, readable by its owner
only, written when it starts and removed when it stops), loads the page, which asks
`GET /api/runs?open=1` and shows each run still going in "Continue" and in its kit, where it
polls as before.

**Stopping.** Three ways, one behaviour: "Quit" in the page (`POST /api/quit`), Ctrl-C or a
hang-up in the terminal (the host handles `SIGINT`, `SIGTERM` and `SIGHUP`, H1, item 6), and
the end of the process's standard input when it was started with `--stdin-quit` (for tests).
The host stops taking requests (503 to any that arrive) and starts the grace of ten seconds
(the stdio server's) first. Then it trips the cancellation of every run it started and of
the install going, if one is: each run was started with a `Cancellation` the host holds
(`Request::cancelled_by`), and tripping it never waits. `Executor::cancel` can wait (on a run
already in its final publication, `LiveRun::cancel`), so the host calls it, and waits for each
run, on a thread of its own and never on the thread that keeps the grace's time; at the end
of the grace the host goes on whatever is still waiting. Within the grace it waits for each
to end, so each run's record says it was cancelled and every dispatched request is
accounted for in the ledger, and the install is left as an install cancelled leaves it (the
earlier one in place); then it stops the pack's worker, removes `play.json`, drops its store
(which lets the writer's lock go) and exits 0.
A run that has not ended by the grace is left to be ended as interrupted by the next writer,
as any run of a dead process is (`SPEC.md` section 3), and the exit says how many. A second
Ctrl-C during the grace exits at once, the same way.

**Routes.** JSON unless said. `{id}` is a run's id; a picture is named by its identity.

| Route | What | Task |
|---|---|---|
| `GET /api/hello` | Version, the store's folder, the kits and tasks of the description with their arguments and ports, the backends' readiness (no probe), the ledger's allowance and what is left | H1, W1 |
| `POST /api/quit` | Stop, as above | H1 |
| `POST /api/pictures` (body: the picture's bytes, `?name=`) | Import into the store through the store's import path, with the name kept beside it in the same publication; answer its identity, size and name | W1 |
| `POST /api/locations` (body: a shape and the picture's identity) | Commit a drawn box, polygon (at most 64 points), line or stroke as a `Location` bound to that picture; answer its identity | W1 |
| `GET /api/pictures?after=&limit=` | The pictures this store's playground imported, newest first, with names | W1 |
| `GET /api/artifacts/{identity}` and `…/thumb?edge=` | An artifact's bytes for display: a picture as PNG; a mask drawn as a grey picture; a location as JSON. A thumbnail at most `edge` pixels on its long side, made on request and not stored | W1 |
| `POST /api/prepare` | Prepare a call: per choice, ready or not and its kind; each need's bound; for a loop, its plan; and a handle to the preparation, which the host keeps (at most 64, each for its hour) | W1 |
| `POST /api/runs` (body: the call, the handle, a request id the page made) | The call is sent to the executor with its request id, marked as needing a preparation, and with the preparation the handle names when the host still holds it. A repeated id answers the existing run before anything else is looked at (ADR 0007), so a retry after a dropped connection, an expired handle or a restart of the host answers the run it started. Otherwise a call without a fresh preparation of exactly these arguments is refused as stale before it is admitted, and the page prepares again | W1 |
| `GET /api/runs/{id}?after=<n>` | The run's state, its events after the `n`th, its steps and child runs so far, its spend so far against its reservation | W1 |
| `POST /api/runs/{id}/cancel` | Cancel ("Stop run"): trips the run's `Cancellation`, which the host holds, and answers "stopping" at once; the host's one canceller thread then calls `Executor::cancel` for it (which can wait while a run is in its final publication), once a run however often Stop is pressed, never on a connection worker | W1 |
| `GET /api/runs?kit=&task=&result=&words=&before=&limit=&open=` | History, newest first; `open=1` lists the runs still going | W3 |
| `GET /api/runs/{id}/record` | The run's record, its findings and verdict, its attempts, its cost split by capability and backend | W2 |
| `GET /api/runs/{id}/command` | The command line that runs the same call again, from the saved inputs and resolved settings | W2 |
| `GET /api/runs/{id}/report` | The report as one self-contained HTML document, for the isolated frame and for "Export" | W3 |
| `GET /api/runs/{id}/export` | The run's record and outputs as one archive download (`runs export` and the artifact export) | W3 |
| `GET /api/stats?…` | `runs stats` over the same filter as history | W3 |
| `GET /api/records/{name}` and its images | A tool's or task's record, `docs/reference/tools/<name>.md`, embedded in the binary | W3 |
| `GET /api/models`, `POST /api/models/{name}/get`, `POST /api/models/{name}/cancel`, `DELETE /api/models/{name}` | The catalogue's models with what is installed, an install's progress, cancel and removal | M1 |
| `GET /api/backends` | `backends status`, without a probe | M1 |
| `POST /api/spend/reset` | `spend reset`, with the amount typed, and the word `reset` typed again | M1 |

No route takes a file path. A picture is sent as bytes and kept in the store as a snapshot,
so moving or editing the original changes nothing; a result leaves as a download the browser
saves.

**Negative tests** (Rust, in the fast tier, against the host on a real loopback socket:
`crates/whizbang-serve/tests/it/play_guard.rs`), one each, from `SPEC.md` section 10 and
the table above: `an_api_call_without_the_token_is_refused_401_and_names_nothing`,
`an_api_call_with_a_token_of_an_earlier_launch_is_refused_401`,
`a_request_whose_host_is_not_loopback_at_the_port_is_refused_421`,
`a_request_from_another_origin_is_refused_403_and_carries_no_allow_origin_header`,
`a_change_without_an_origin_is_refused_403`,
`a_cross_site_fetch_metadata_header_is_refused_403`,
`a_picture_over_64_mib_is_refused_413_before_it_is_read_whole`,
`a_picture_over_the_pixel_limit_is_refused_413_naming_the_limit`,
`a_json_body_over_1_mib_or_deeper_than_the_limit_is_refused`,
`a_fifth_running_call_is_answered_429_and_polling_cancel_and_quit_are_still_answered`,
`a_client_that_trickles_its_headers_or_reads_nothing_is_closed_at_its_deadline`,
`a_third_upload_is_answered_429_after_its_headers`,
`two_stalled_uploads_and_ten_trickling_clients_leave_polling_cancel_and_quit_answered`,
`a_request_naming_a_path_in_any_argument_is_refused`, `the_token_is_in_no_log_event_record_or_error`,
`the_host_listens_on_loopback_only` (seventeen in all). And in Chrome, the same four of section 10 as a
hostile page would try them (U1's `security.spec.ts`).

## The screens, what each asks of the API, and what each reads of a run

| Screen | Calls | What of the description, the store and a run's record it shows |
|---|---|---|
| Home | `hello`, `pictures`, `runs?open=1`, `runs?limit=` | Kits and their tasks (kind, kit, title, inputs and output in words); open runs and drafts ("Continue"), each with its tag; the pictures library; the status bar's backends (readiness without a probe) and the allowance |
| Kit workspace (draft) | `hello`, `prepare` on every change (debounced 300 ms), `pictures`, `locations`, `artifacts` | The task's arguments and ports as typed inputs; each choice of a backend with READY, NO KEY, GET · size, from preparation (K1's kind) and the catalogue; the plan with each need's capability, backend, model, request count and bound (K1), and the total "up to $X"; the canvas: the picture and the location drawn; the tools the task uses, and each one's record (W3) |
| Sent to Squint | as the workspace, from a result's `record` | The sending run's id, task and time; which inputs came from it, tagged FROM RASCAL; "Back to the run" |
| Running | `runs/{id}` polled | Inputs locked; spent so far against the reservation (the ledger's records of this run and its children); the step list from the recipe's steps and K1's typed events (a step started and ended, its child run, its attempt), across attempts with their dividers; a judge's places drawn on the canvas from the findings of the check that made them, read from that child's record when it ends; "Stop run" |
| Result | `runs/{id}/record`, `artifacts`, `command`, `report` | The verdict or outcome tag; before and after (side by side, swipe, difference); the verdict card from the verdict and reasons; attempts with each one's outcome; the cost split by capability and backend, with free local work; the run's id and "Export"; Save picture (a download), Send to, Run again, Copy command |
| History | `runs?…`, `stats?…` | Each run's task, what was asked (its words and the picture's name), result tag, models, time, cost, when; filters by kit, result and words on screen, and every filter of `runs stats` (action, model, backend, use case, period) in the address; the figures of `runs stats` over the filter |
| A tool's or task's record | `records/{name}` | The page `docs/reference/tools/<name>.md` with its images, as the reference holds it (`SPEC.md` section 8: "the playground shows the same page") |
| Models | `models`, `backends`, `hello` | On this Mac: the catalogue's models, installed or not, sizes, an install in progress with its files done of its files, Get, Cancel, Remove; in the cloud: each backend the config offers, KEY SET or NO KEY or NOT SET UP, never a key; "How to add" linking the config guide; the allowance since the last reset and the call cap |

## Acceptance: every kept row, its task and its Chrome case

The eleven kept rows of `SPEC.md` section 7 that are milestone 5's: the playground's ten and
squint's "Report viewer and export". Each Chrome case runs against the fixture host, in a
new store, with fake models; each is named `<file>: <title>`.

| # | Row | Task | Chrome cases |
|---|---|---|---|
| 1 | Run both tools: typed inputs including lists and artifact bindings; drawing a location; readiness per choice; reviewing the bound cost before a paid run; progress, stop and reconnect | W1 | `run.spec.ts: a rascal task runs from a picture and words to a result`; `run.spec.ts: a squint task runs from two pictures and a brief to a verdict`; `inputs.spec.ts: a variadic port takes, orders and removes pictures within its count`; `inputs.spec.ts: an artifact port binds a picture from the library and from a drop`; `inputs.spec.ts: a box and a stroke drawn on the canvas become the location the task is given, and a click and a drag become segment's point and box`; `readiness.spec.ts: each choice shows ready, no key or get with its size, and a choice that is not ready can be picked and Run says what is missing`; `readiness.spec.ts: a paid run shows each need's bound and the total before Run, and Run is refused once the preparation is stale`; `run.spec.ts: progress shows each step as it starts and ends`; `run.spec.ts: the running step list shows a step running and an attempt started before the run ends`; `run.spec.ts: each of the five squint use cases runs from its form`; `run.spec.ts: stop ends the run as cancelled and nothing is committed`; `reconnect.spec.ts: a reloaded page shows the running run and follows it to its end` |
| 2 | Closing the tab does not cancel a run; reopening reconnects to it | W1 | `reconnect.spec.ts: a run started in a tab that is closed ends completed, and a new tab finds it in Continue and opens its result` |
| 3 | Before and after view of a source and its result | W2 | `compare.spec.ts: side by side, swipe and difference show the source and the result at one zoom` |
| 4 | Send a result to another action; rerun into an independent draft; hand one tool's output to the other | W2 | `send.spec.ts: send to squint opens check an edit with the source, the edit and the words tagged from rascal`; `send.spec.ts: back to the run returns to the sending result`; `send.spec.ts: run again opens a draft whose changes do not touch the result it came from`; `send.spec.ts: a result's mask sent to another rascal task binds as its mask`; `send.spec.ts: a removal sent to check an edit carries the job the loop writes` |
| 5 | Model installation from the interface | M1 | `models.spec.ts: get installs a model with its progress, and a second get attaches to the same install`; `models.spec.ts: cancel leaves the earlier install in place`; `models.spec.ts: remove of a model in use answers busy and the model stays`; `models.spec.ts: no key is shown for a backend without one and no key is ever in the page` |
| 6 | History that survives a restart; evidence that survives a reopen | W3 | `history.spec.ts: runs are listed after the host is stopped and started again, with their result, cost and time`; `history.spec.ts: filters by kit, result and words narrow the list and the figures`; `history.spec.ts: a finished run's evidence (steps, findings, places, pictures) is the same after a restart` |
| 7 | Reports for both tools shown in place and still readable after a restart; squint's exported as a document | W3 | `report.spec.ts: a rascal result's report shows its steps, masks and cost in place, its pictures decoded and its styles applied`; `report.spec.ts: a squint result's report shows its verdict, findings and the places on the picture in place`; `report.spec.ts: both reports read the same after a restart`; `report.spec.ts: export of a squint report downloads one document that opens alone, offline, and holds its pictures` |
| 8 | A saved run gives a usable command for a pipeline, from its saved inputs and resolved settings | W2 | `command.spec.ts: copy command gives a command that, run in a shell against the same store, makes the same call` (the case quits the host, runs the copied text with the binary, and compares the two runs' calls); `command.spec.ts: the command of a check sent from a removal carries its whole job` |
| 9 | Saved history owns a snapshot of its inputs; moving or editing the original file does not change it | W1 | `inputs.spec.ts: a picture changed on disk after it was sent leaves the run's input and result as they were` |
| 10 | Quit and terminal shutdown cancel and account for running work and release the store | H1 | `quit.spec.ts: quit with a run going cancels it, its record says cancelled, the ledger has no open reservation, and the store takes a writer at once`; `quit.spec.ts: an interrupt from the terminal does the same` |
| 11 | Squint: report viewer and export | W3 | as row 7's squint cases, and `report.spec.ts: model and tool text holding markup shows as text and runs nothing` and `report.spec.ts: the report frame cannot read the page's storage or call the api` |

Beside these, the API's negative tests ("The playground API") and the loop's three cases by
hand (E1), which `docs/carry-over.md` moved here: they are the orchestrator's, live.

Beside the section 7 rows, `SPEC.md` section 8 asks that the playground show each tool's
record: `records.spec.ts: a task's record opens from the workspace with its examples and
limits` (W3).

**Count.** 37 Chrome cases over the eleven rows, at least one for each row and each clause of
row 1; one for section 8's records (W3); one for a third kit on Home (W1); and
`shell.spec.ts`'s three and `security.spec.ts`'s four (U1): 46 in all. Seventeen Rust
negative tests of the API.

## How the work is split

Nine pull requests, where the owner asked for few and large, plus the owner-gated archiving.
Sizes are changed lines of source, tests and hand-written documents, by milestone 3's rate
(about 2,500 for a large pull request); `ui/dist/`, `ui/package-lock.json` and generated
reference are counted apart.

| Pull request | What | Builds on | Size |
|---|---|---|---|
| H1 | The host and its guard; `whizbang play`; the token; reconnect; quit and the terminal's signals; the store held for days (the carry-over rows); ADR 0041 | `main` | Large: about 2,400 (HTTP and the guard 700, play and the lifecycle 400, signals 150, the long-lived store's rows 450, tests 700) |
| K1 | What the screens stand on, in the core: readiness as a kind, each need's bound, typed events, a journal held to a bound, a call that must run under a preparation, the unconfirmed admission | `main` | Medium: about 1,800 (the kind 200, the bounds 150, events 450, the journal's bound 250, the mark and the admission 200, tests 550) |
| U1 | `ui/`: the stack, the tokens, every component with its unit tests, the app's shell and routes, the fixture host, `just ui-*` and `just e2e`, the two `e2e` jobs in CI; ADR 0042 | `main` | Large: about 2,500 (components 1,300, shell 250, the fixture host 250, scripts and CI 250, tests 450) |
| W1 | Run a task: the description's kits and titles, pictures and locations in, prepare, admit, the run polled, stop, reconnect; Home and the kit workspace and Running | H1, K1, U1 | Large: about 2,500 (API routes 700, description 250, screens 900, Chrome cases 650) |
| W2 | The result: compare, the verdict card, attempts, cost, Send to, Run again, Copy command | W1 | Large: about 2,000 (the command 450, send and the map of what goes where 350, screens 650, cases 550) |
| W3 | History, stats, reports and their export, the tool records in place; the brand on Home | W1 | Large: about 2,500 (history and stats 550, the report and its reader 800, records 200, screens 500, cases 450); the records' embedded files counted apart |
| M1 | Models: install, progress, cancel, remove, readiness and keys, the allowance | W1 | Medium: about 1,500 (routes 450, screen 450, cases 400, the guide 200) |
| E1 | The orchestrator's live runs: the loop's three cases by hand in the playground on the owner's Mac; the guide | all above | Medium: about 800 (the guide 400, the record 300, the script 100) |
| N1 | The note, the boxes, the carry-over rows, the spec's wording owed; then the archiving, on the owner's yes | E1 | Small: about 500, no code |

**Waves.** Five, each from `main` once what it builds on has landed.

1. **H1, K1 and U1** side by side. They share no source file: H1 is Rust in `whizbang-serve`,
   the store and the binary; K1 is the core's contract, journal and recipe runner and the
   loop's events; U1 is `ui/`, the `justfile`'s UI recipes, the fixture host and CI. The
   fixture host (U1) starts the host H1 builds, so U1's Chrome cases of wave 1 run against a
   stand-in route table of U1's own, `/api/hello` only, and switch to H1's host when U1 merges
   `main` after H1 (whichever lands second merges the other and reruns).
2. **W1**, when all three have landed.
3. **W2, W3 and M1** side by side, when W1 has landed. W2 owns the result screen, W3 history
   and reports, M1 the models screen; the shared files are below.
4. **E1**, when the rest have landed.
5. **N1**, then the archiving, on the owner's yes.

**Shared files, each with one owner in a wave.** `docs/carry-over.md`: H1 in wave 1 (U1 closes
the Chrome row only by a line N1 writes); W1 in wave 2; W3 in wave 3 (W2 and M1 hand their
lines to W3's builder through the orchestrator, or N1 writes them). `.github/workflows/ci.yml`
and the `justfile`: U1 alone in wave 1; nobody else changes them. The binary's
`main.rs`: H1 alone in wave 1 (it adds `play`); W2 makes its parser callable from the fixture
in wave 3, and W3 and M1 do not touch it. `ui/src/routes.ts` and
`ui/src/api.ts`: W1, and in wave 3 each task adds its own routes in its own file
(`ui/src/screens/<screen>/`), and the second and third to land merge `main`. The host's route
table, `crates/whizbang-serve/src/play/routes.rs`: H1, then W1, then in wave 3 each task adds
its routes in its own module (`play/result.rs`, `play/history.rs`, `play/models.rs`), each
registered by one line. The description (`contract/descriptor.rs`, `docs/reference/`): W1
alone; `just docs` is run again by the second to land of any wave. The store's code
(`crates/whizbang-core/src/store/`): H1 alone. The contract, the journal and the recipe
runner (`crates/whizbang-core/src/contract/`, `recipe/runner.rs`) and the loop's events
(`whizbang_rascal::checked`): K1 alone in wave 1, `contract/engine.rs` among them, so the
unconfirmed admission, which is there, is K1's (its item 6) and not H1's. `ui/dist/` is never merged by hand: it is
written again by `just ui-build` after merging `main`. This plan's own file: a task changes
only the lines of its own section, its box among them.

**Seams.** Each pull request names where it splits if it passes about 2,500 changed lines,
into a pull request of its own that lands before anything that builds on it: H1, the store
held for days (its item 7); K1, none; U1, the fixture host and CI (its items 5 and 6) before the
components; W1, the pictures library and Home (its item 3); W2, Copy command (its item 5);
W3, the report and its export (its items 3 to 5), then the records (item 7); M1, none. A split is said in the hand-back
and the owner is told the count. Nothing of a row is dropped to keep the count.

**Review rule.** Every pull request is reviewed whole by a reviewer that did not write it, as
`AGENTS.md` says, and merged by the orchestrator on green CI with `just scan-range` over
`base..head`. Beside that, each task names what its reviewer looks at hardest: H1, the guard's
order and that no route can be reached without it, the token's paths, and what a quit leaves
in the ledger; K1, that no door's words change and that a journal of the older format still
reads; U1, that the page loads nothing from off its host and that every component
takes props only; W1, that the page cannot call an action that skips preparation or
admission, and that the description's new fields keep `describe` static; W2, that the copied
command is exact and quotes every argument; W3, that the report's frame is isolated and that
no text from a model is put into the page as markup; M1, that no key, sign-in or token ever
reaches a response.

**Live runs are the orchestrator's.** No builder makes a provider call or loads a real model
in CI or in a Chrome case. The only live runs are E1's, on the owner's Mac.

**Spend.** The owner's standing allowance is 100 US dollars a milestone; milestone 5 has spent
nothing. Its live runs come to about 3 dollars ("Spend").

## H1. The host, its guard, `whizbang play`, and a store held for days (large)

### Goal

`whizbang play` starts a host that holds the store, serves the built interface and the API
behind the guard of "The playground API", follows runs a page started after the page is
gone, and stops cleanly from the page or the terminal. And the store, the executor and the
ledger are made fit to be held by one process for days, which is what the carry-over rows on
"before `whizbang serve` holds a store for days (milestone 5)" ask for. No screen yet.

1. **ADR 0041, "The playground host"**, written first: the choices of "Choices this plan
   makes" that concern the host, and "The playground API" whole (the checks and their order,
   the token, reconnecting, stopping, the routes as a table every task adds to). It names
   what it rejects: a web framework and an async runtime; a cookie for the token (a cookie
   rides on a forged request; a header does not); pushed events; a random port (it would
   lose the page's drafts at every launch).
2. **The listener and the guard** (`crates/whizbang-serve/src/play/`: `http.rs`, `guard.rs`,
   `routes.rs`, `assets.rs`). A request read with bounds on the line, the headers and the
   body before anything is held; the guard as one function every route goes through, so a
   route cannot be registered without it (the route table's type only takes guarded
   handlers); the response headers of "Who may call". Threads: one acceptor and a pool of
   sixteen connection workers, made by a spawner of the crate's own that counts them, the fifth
   place in the workspace that makes threads (`clippy.toml`,
   `scripts/tests/test-thread-lint.sh`). The token: 32 bytes through `getrandom`, held in a
   type whose `Debug` and `Display` print nothing, compared in constant time.
3. **`whizbang play`** (`crates/whizbang/src/play.rs`): `--store`, `--config`,
   `--env-file`, `--models-dir`, `--port`, `--no-open`, `--stdin-quit`, `--run-limit-s`. It
   builds the executor over the config, its env file and the models directory as `whizbang
   serve` does (`serve.rs`, `wiring::runner`), with one difference: `serve` asks for every
   backend (`wiring::Uses::Every`), so a config that names a key and no env file stops it
   before it starts; the host asks for none at start (`Uses::Nothing`), and a backend whose
   key is missing is simply not ready, which a preparation says with its kind (K1), so the
   page opens and says NO KEY, and work that needs no key runs. It opens the store to write;
   writes `play.json`; opens Chrome. A second `whizbang play` on a store held by a live host opens
   that host's address and exits 0; on a store held by anything else it is refused as today,
   naming the holder.
4. **The routes of this task**: `GET /api/hello` (without kits, which W1 adds) and
   `POST /api/quit`; and the static files, from `assets.rs` (a placeholder page until U1's
   bundle lands; the test of the list against `ui/dist/` arrives with it).
5. **Runs started by the host belong to it.** A run started through the API is started by
   `Executor::start` and kept in the host's table of its runs, as the stdio server keeps
   them; the API's status, cancel and wait reach it there; the page holds nothing a run
   needs. Started runs are bounded by the executor's bound and by four calls at once. This
   answers the carry-over row on a cancel from another process the way ADR 0031 did: one
   process, the host, owns every run it starts, and the row is closed with that line.
6. **Stopping**, as "Stopping" says, the same code for each of the three ways. The terminal's
   signals through `signal-hook`'s safe iterator (`signal_hook::iterator::Signals`), read on
   a thread of the host's counted spawner, in `crates/whizbang/src/play.rs` only, so the
   workspace keeps `unsafe_code` forbidden; no other command changes how it ends. Two
   signals in a row, and a signal while every worker is held by a stalled client, each
   quit as one does: the quit does not wait on a connection worker. The carry-over row on the store's lock ("whizbang handles no
   signal, so Ctrl-C ends a writer without dropping its store") gains a line: under
   `whizbang play` it does.
7. **A store held for days.** The rows that wait for "before `whizbang serve` holds a store
   for days (milestone 5)", each answered as its "Done when" asks, or narrowed with a line:
   - the store's locks: a writer notices that its `LOCK` is no longer the file at that name
     before it publishes, with a test; the network file system stays unsupported, said in
     the playground guide;
   - the store's two openings to lock (`Fs::share`, `Fs::try_alone`): a store whose folder
     is not owned by the user who opens it, or that its group or anyone can write, is refused
     when it is opened, to read or to write, before anything in it is read or locked, as a
     models directory is (`ModelsDir::open`), with a test of each, and the row is closed with
     that line;
   - a started run's thread that nobody waited for holds the writer until it is done: the
     host waits for every run it started before it drops its store (item 6), with a test
     that a quit followed by an open of the store to write is not refused;
   - an admission that became visible and was not confirmed durable: K1, item 6, which owns
     the executor's file in this wave;
   - every publication writes the whole generation again: measured with a store of 10,000
     runs (the owner's history of a year at about 30 runs a day) and the bound recorded; the
     row stays open for milestone 6 with the number if it holds;
   - a run's journal and diagnostics held in memory without a bound: K1, item 4, bounds
     them. A parent's wait for a child is not bounded by time: a run that ignores its
     cancellation keeps its place until Quit ("A run's stated time"), and the row is narrowed
     to that with a test of a child that never stops;
   - the deletion's sweep with a reader of another process: the test the row asks for.
8. **The playground guide's first part**, `docs/guides/playground.md`: starting, the address
   and the token, reopening, stopping, what the host holds, that the store is refused to other
   writers while it runs. Listed in `docs/guides/README.md` under "Written".

- [ ] Done when: the seventeen negative tests of "The playground API" pass;
  `play::hello_answers_the_version_and_the_store_behind_the_guard`,
  `play::a_second_play_on_a_held_store_opens_the_running_hosts_address`,
  `play::play_json_is_owner_only_and_gone_after_a_quit`,
  `play::a_quit_cancels_every_run_waits_and_releases_the_store`,
  `play::an_interrupt_signal_quits_as_the_page_does`,
  `play::a_run_that_outlives_the_grace_is_counted_and_ended_as_interrupted_by_the_next_writer`,
  `play::a_run_started_by_the_host_is_found_by_its_id_after_its_caller_is_gone`,
  `store::a_writer_whose_lock_file_was_replaced_refuses_to_publish`,
  `store::a_store_that_others_can_write_is_refused_when_it_is_opened`,
  `store::a_store_owned_by_another_user_is_refused_to_a_reader_and_a_writer`,
  `play::a_config_naming_a_key_with_no_env_file_opens_and_says_no_key`,
  `play::a_run_past_its_stated_time_is_cancelled_with_its_children`,
  `play::a_run_that_ignores_its_cancellation_keeps_its_place_and_quit_still_exits_after_the_grace`,
  `play::stop_pressed_often_while_a_run_is_in_its_final_publication_leaves_polling_and_quit_answered`,
  `play::two_interrupts_and_an_interrupt_with_every_worker_stalled_each_quit`,
  `play::a_quit_while_a_run_is_held_in_its_final_publication_exits_at_the_end_of_the_grace`,
  `store::volume::ten_thousand_runs_publish_within_the_measured_bound` (by hand, its number in
  the row), the sweep's test with a reader of another process, `scripts/tests/test-thread-lint.sh`
  with the fifth place, ADR 0041 merged, and the carry-over rows closed or narrowed with
  their lines.

**Keeping tests lean (H1).** The guard's tests make raw requests over a loopback socket with
no client library: a request is bytes and an answer is read to its status line and headers.
No test sleeps: a quit's grace is a controlled clock.

## K1. What the screens stand on, in the core (medium)

### Goal

Five things the screens need that the engine does not give today, and one carry-over row
in the same file, found when the design
was read against the code. They are the core's, so they are one task, beside H1 and U1, and
W1 builds on them; none changes what a run of any other door does.

1. **Why a capability is not ready, as a kind.** `contract::prepare::Missing` gains `kind`:
   `no_key`, `not_installed`, `out_of_date`, `damaged`, `not_in_catalogue`,
   `not_for_this_platform`, or `other`, carried up from `config::offers::NotReady` and
   `capability::inventory::Unready` where `why` is made today. Its words are unchanged, so
   every door prints what it printed.
2. **Each need's bound.** `PreparedCall::needs` hands out, for each `Need`, its request count
   and the bound of one request and of all of them, the figures preparation already
   multiplies into the estimate. A loop's `Plan` is unchanged. Nothing about admission
   changes: the estimate is still the one bound a call runs under.
3. **Typed events.** A run's journal line may carry a kind beside its message: a step
   started, a step ended (with its outcome and child run), an attempt started (with its
   number and what changed), a request sent to a capability and answered (backend, model,
   milliseconds). The recipe runner (`recipe/runner.rs`) and the loop (`whizbang_rascal::checked`)
   say them as they go, through the same `Events` sink, so they are published while the
   run executes, in this process, and kept in the journal at its end. A line without a kind
   is a message as today; the journal's format gains a version, and a reader of the older one
   reads it. This keeps squint's typed journal events in the shape whizbang needs, which
   answers that carry-over row (it is closed with this line, not dropped).
4. **A journal held to a bound.** A run's journal is published whole from memory at its end
   (ADR 0008, "Progress"), so its bound in memory is its bound on disk, and keeping fewer
   lines is a change to what `SPEC.md` section 3 says a run keeps ("progress events" among
   its diagnostics). This plan makes that change, and N1 writes it into the spec: a run keeps
   its first 4,096 progress messages and their first 1 MiB of text, and counts the rest
   without keeping them; and its typed events (item 3), each at most 512 bytes, up to 16,384,
   past which the run counts them too. Nothing kept is ever dropped later, so what a run
   kept is the same in memory, in the journal and after a restart. The journal's last line
   says how many of each were not kept. A sequence number is given to every event, kept or
   not, so a poll that asks for the events after `n` is answered with those kept and the
   count of those skipped, and the page says "n messages not kept" where the gap is.
   `Executor::status` gains a form that hands back only the events after a given sequence
   number, so a poll copies no more than is new. Diagnostics other than progress keep the
   bounds they have: a run's stored exchanges their 256 MiB, reserved before a request is
   sent (`capability/mod.rs`), and its other diagnostics their count; no new budget is shared
   between them, so nothing a paid request already sent can be refused for want of room.
   Today's actions say a few dozen messages a run, so no shipped run meets these bounds.

5. **A call that must run under a preparation.** `Request` gains a mark (`needing_preparation`)
   that the host sets on every call it sends. The executor resolves a repeated or conflicting
   request id first, as today; then, for a marked call with no expected preparation, or one
   that does not match, it refuses the call as stale (`Refusal::Stale`) before admission, so
   nothing is reserved or sent. An unmarked call is prepared and admitted as today.
6. **An admission that was visible and not confirmed** (the store's `Unacknowledged`, a
   carry-over row "before a door holds a process for days"): ended at once, as an error, by
   the executor that admitted it, so its request id answers a run that ended. In K1 because
   admission is in `contract/engine.rs`, which K1 alone changes in wave 1.

- [x] Done when: `admit::a_call_needing_a_preparation_without_one_is_refused_stale_and_reserves_nothing`,
  `admit::a_repeat_needing_a_preparation_is_answered_with_its_run_before_the_check`,
  `admit::an_unmarked_call_without_a_preparation_is_prepared_and_admitted_as_before`,
  `prepare::a_missing_capability_says_its_kind_beside_its_words`,
  `prepare::each_need_hands_out_its_request_count_and_bound_and_they_sum_to_the_estimate`,
  `runner::a_recipe_says_each_step_started_and_ended_while_it_runs`,
  `checked::the_loop_says_each_attempt_started_and_what_changed_while_it_runs`,
  `watch::a_second_caller_sees_a_step_started_before_the_parent_ends`,
  `journal::a_journal_of_the_older_format_is_read`,
  `journal::past_its_bound_a_run_keeps_its_first_messages_and_counts_the_rest`,
  `journal::typed_events_past_their_bound_are_counted_and_the_journal_says_so`,
  `watch::a_poll_across_events_not_kept_is_answered_with_the_count_skipped`,
  `watch::status_after_a_sequence_hands_back_only_the_newer_events`,
  `admit::an_unconfirmed_admission_is_ended_as_an_error_by_the_executor_that_made_it`, and the
  carry-over rows on typed events and on what a run holds in memory closed with their lines.

**Keeping tests lean (K1).** Scripted actions and the fake segmenter, through the production
path; no new picture.

## U1. `ui/`: the stack, the tokens, the components, the shell, the Chrome tier (large)

### Goal

The interface's foundation, whole and tested, before any screen is wired: so that W1 to M1
build screens out of finished components and add Chrome cases to a tier that already runs on
both runners.

1. **The stack**, ADR 0042, "The playground's interface": the table of "Choices this plan
   makes", why each, and what was rejected. Node is pinned in `scripts/tool-versions.txt`
   (the current long-term release, by exact version), `scripts/setup.sh` checks it as it
   checks the others, and `ui/package-lock.json` pins every package; `npm ci` installs.
   The packages: `preact` and `marked` (shipped); `vite`, `typescript`, `vitest`, `happy-dom`,
   `@preact/preset-vite`, `@playwright/test` (development only). Nothing else without a line
   in the ADR. Playwright downloads no browser (`PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`).
2. **The tokens**, `ui/src/tokens.css`, taken from the Paper file's tokens by their exact
   values (not from a screenshot): the dark set under `:root`, the light set under
   `[data-theme="light"]`; the type scale, weights, tracking, line heights, spacing and radii.
   A unit test reads the file and fails on a colour used by a component that is not a token.
   A contrast test computes each text and background pair the components use, in both themes,
   and fails under 4.5 to 1 for text under 18 px.
3. **The components** (`ui/src/components/`), one file each, props only, no request and no
   application state; each with a unit test of what it renders for each of its states:
   buttons (primary, secondary, quiet, danger; with a key hint); fields (text, number,
   picture, list of items); the choice list with readiness (READY, NO KEY, GET · size); the
   segmented control; tags (DRAFT, RUNNING, PASS, FAIL, UNSURE, NO KEY, INTERRUPTED,
   FREE · LOCAL, and a progress variant); the kit panel; the task row; the continue row; the
   picture tile and the drop target; the breadcrumb; the kit switcher menu; the Send to menu;
   the plan block (each need's capability, backend, request count and bound, and "up to
   $X"); the binding picker for any artifact type (a mask, a location, a band profile, a
   series or a plan, from the outputs of earlier runs of that type); the sent-from
   banner; the verdict card (pass, fail, unsure); the status bar; the canvas and its marks
   (box, mask overlay, point, stroke, judge flag with its words), with zoom and fit; the step
   list (done, failed, running with elapsed time, waiting; the attempt divider); the progress
   bar, the spend meter, the allowance bar and the cost table; compare (side by side, swipe,
   difference, one zoom for both); the history row; the model row. Every text a component is
   given is put in as text, never as markup (a test per component that takes text from a run:
   `<img onerror>` in a reason is shown as those characters).
4. **The shell**: the top bar (wordmark, breadcrumb "Kit ▾ / Task ▾", ⌘K search, History,
   Models, the allowance), the status bar, the routes (by the address's hash:
   `#/`, `#/k/<kit>/<task>`, `#/run/<id>`, `#/history`, `#/models`), the client of the API
   (`ui/src/api.ts`: the token, the poll, at most two uploads at once with the rest queued,
   a refusal shown as a sentence), the token's reading
   and removal from the address, the keyboard (⌘K, ⌘↵ for Run, ⌘S for Save, ⌘1 to ⌘9 for
   kits in the switcher's order, ⌘0 for Home), and an empty state for each screen.
5. **The fixture host**, `crates/whizbang/examples/play_fixture.rs`: the host of H1 with the
   binary's registry and wiring and, in place of models, the test kit's fake segmenter, a
   scripted fill and a scripted judge whose answers a case chooses by a header the fixture
   alone reads (`x-fixture-script`), refused by the real host. It prints its address and token
   on its first line, so Playwright's global setup starts one per worker in a new store. (Its
   command-line mode is W2's, item 5, after H1 has added `play` to `main.rs`.)
6. **The recipes and CI.** `just ui-build` (writes `ui/dist/`), `just ui-check` (type check,
   unit tests, freshness of `ui/dist/`, the asset list; part of `just check`), `just e2e`
   (builds the fixture host, runs Playwright, heavy). `.github/workflows/ci.yml` gains the jobs
   `e2e (macos-15)` and `e2e (ubuntu-22.04)` with `actions/setup-node` pinned by its commit,
   the Node of `tool-versions.txt`, a cache of npm's folder keyed by the lock file, and the
   Playwright report kept as an artifact on failure; the aggregate `ci` needs them.
   `scripts/tests/test-ci-jobs.sh` holds the new jobs to the recipes. `scripts/package.sh`
   is unchanged; `scripts/install-check.sh` gains a step that starts `whizbang play --no-open
   --stdin-quit` from the installed archive, fetches `/` and `/api/hello` with the printed
   token over loopback with the system's `curl`, and quits it.
7. **The first Chrome cases**: `shell.spec.ts: the page loads with the token, removes it from
   the address and shows Home`; `shell.spec.ts: a request from the page without the token is
   refused and the page says to run whizbang play again`; `shell.spec.ts: the page loads
   nothing from any origin but its host`; and `security.spec.ts`'s four, made from a second
   origin the test serves: a fetch of `/api/hello` without the token, a fetch with the token
   from the other origin, a request to the host under another name that resolves to loopback,
   and an upload of a picture over the pixel limit.

- [ ] Done when: the unit tests of every component and of the tokens and contrast pass under
  `just ui-check` in under 20 seconds; the seven Chrome cases pass under `just e2e` on the
  owner's Mac and in both `e2e` jobs; `scripts/tests/test-ci-jobs.sh` and
  `scripts/tests/test-setup.sh` pass with Node; the install check starts and quits the host
  from the archive on both runners; `THIRD-PARTY-LICENSES.md` holds Preact's, marked's and the fonts'
  licences, held by `scripts/tests/test-third-party-licences.sh`; ADR 0042 merged.

**As built (U1).** The record is ADR 0043, "The playground's interface": 0042 was taken by
milestone E's "Submission v2" first. In wave 1 the fixture host is the stand-in this task
names (item 5 and "Waves"): the interface the binary embeds, behind the guard's checks in
their order, with `GET /api/hello`, `POST /api/quit` and the upload's size and pixel checks;
it becomes H1's host, with the fakes, when the second of the two lands. The install check's
step (item 6) needs H1's `whizbang play`, and is added by whichever of the two lands second.

**Keeping tests lean (U1).** Component tests render to `happy-dom` and compare text and
attributes, never pixels. The Chrome cases use 64-by-64 pictures and scripted models; no case
waits on a fixed time, only on what the page shows.

## W1. Run a task: from Home to a running run and back (large)

### Goal

A person picks a kit and a task, gives it its inputs, sees what each choice needs and what
the run may cost, runs it, watches it, stops it, closes the tab and comes back to it. Rows 1,
2 and 9 of the acceptance table.

1. **The API's run routes**: `prepare`, `runs` (start), `runs/{id}` (poll), `runs/{id}/cancel`,
   `runs?open=1`, through the executor's own entry points (`Executor::prepare`,
   `Executor::start`, `status`, `cancel`) and nothing else, so a call from the page passes
   every check a call from the command line passes: preparation, admission, spend and the
   gate. A preparation is not sent to the page: the host keeps it under a handle (at most 64,
   each for its hour, the oldest dropped first) and the page sends the call, the handle and
   a request id it made. The host gives the executor the call with its request id, marked as
   one that must run under a preparation (K1, item 5), and with the preparation when it still
   holds one for exactly this call (`Request::expecting`). The executor's own order holds: a
   repeated id answers its run, and a different call under the same id is a conflict, first
   (ADR 0007), whether the handle has expired or the host has restarted since; then a call
   with no fresh preparation of exactly these arguments is refused as stale, before anything
   is admitted, reserved or sent, and the page prepares again. Without the mark an executor
   prepares and admits a call as it does today, so no other door changes.
2. **Kits and titles in the description.** Each action's descriptor gains `kit` (`rascal`,
   `squint`) and `title` (ADR 0040's words: "Remove what you name", "Remove, checked", "Check
   an edit"), and the description gains each kit's line ("edit pictures", "check pictures")
   and its order. A loop's descriptor names the kit it uses. Both are held to the rule for
   names and purposes (`scripts/docs.sh`, `contract/text.rs`). The description's version goes
   up; `just docs` rewrites the reference, whose page now shows them. A test adds a third kit
   of fake actions to a registry and shows it on Home, in the switcher and in search, with no
   change to `ui/`: `SPEC.md` section 1's rule for a door.
3. **Pictures and drawn locations in.** An artifact must be in the store before a call can
   name it (preparation resolves its inputs there), so neither comes in through an action.
   `POST /api/pictures` takes the bytes, held to 64 MiB and decoded inside `DecodeLimits`,
   and commits them as `whizbang store import` does (`import.rs`), through the host's own
   writer: `import::commit` is changed to take an open writer as well as a folder. In the same
   publication, so both are there or neither is, the host writes a small entry beside the
   artifact holding the person's file name (at most 200 characters, kept as given and shown
   escaped). A picture that fails to decode or to publish leaves nothing behind (the store's
   staging is cleared by the publication that fails, as today). `POST /api/locations` takes a
   shape drawn on the canvas (a box, a polygon of at most 64 points, a line or a stroke, in
   the picture's pixels) and the picture's identity, and commits a `Location` bound to that
   picture (ADR 0002) the same way. The picture is kept as the import keeps every picture:
   its canonical pixels as a PNG, with the source's metadata (EXIF, profiles, text) not kept
   (`import.rs`); the file name is the one thing kept beside it. `GET /api/pictures`
   lists the pictures the host imported, newest first, with their names. Home's "Pictures" and drop
   target, the workspace's picture field, and ⌘K's search over tasks, tools and pictures.
4. **The kit workspace, draft.** From a task's descriptor the left panel builds its inputs:
   each argument by its `ValueType` (text, number or integer with its bounds, boolean, a
   choice), each artifact port by its type and count (a picture from the library or a drop; a
   variadic port as an ordered list of pictures, held to its `Count`; a mask or a location
   from another run's outputs; a location drawn on the canvas as a box, a polygon or a
   stroke, by the canvas component's marks, and committed by item 3; `segment`'s point and
   box as its `kind` and `query`, filled from a click or a drag); each capability that an
   argument chooses (`Requirement::chosen_by`) a choice list, one preparation per choice,
   each shown READY, NO KEY or GET · size by K1's kind. Any other artifact port (a band
   profile, a series, a plan) takes an output of that type from an earlier run, by the
   binding picker. **Squint's tasks** take their pictures and job as `squint check` builds
   them: the form has the candidate, the source where the use case takes one, references in
   order, and the brief, and the host makes the `job/1` from them as `Job::from_shorthand`
   does (ADR 0028), so the five use cases each run from the form; a job file is not taken
   from the page. A choice that is not ready can be picked, and Run then says what
   is missing (the carry-over row "A missing model cannot be picked in the model list" is
   closed with this). The workspace lists the registry the executor runs
   (`wiring::registry_over`, with the config's recipe and tool files), not the shipped one
   alone, which is what `whizbang serve`'s tool list does today. The centre
   is the canvas with Original, Mask and Result as tabs where the task has them. The right
   panel lists the task's earlier runs and the tools it uses (from its recipe's steps).
5. **The plan and its cost.** Every change prepares again (debounced) and the plan block shows
   each need's capability, backend, model, request count and bound (K1, item 2), and the total
   bound, and, for a loop,
   its attempts as asked and as allowed (m4's plan of a call). Run is enabled only on a ready
   preparation; a paid run shows "up to $X" on the button's line before it can be pressed.
   The draft's command line is shown beside Run (the `>_` button), from W2's route once it
   lands; until then the button is hidden.
6. **Running.** The inputs lock; the spend meter shows what is spent of what was reserved;
   the step list shows the recipe's steps, each marked as K1's typed events say (waiting,
   running with its child run, done or failed), across a loop's attempts with their dividers,
   while the run is going; when a check's child run ends, its findings' places are drawn on
   the canvas; "Stop run" cancels; the result opens when the run ends.
7. **Home.** Wordmark, the intro with its three steps, Continue (runs still going, from
   `runs?open=1`, and drafts, from the page's own storage), the kits with their tasks, the
   pictures; the status bar with each backend's readiness from `hello`.
8. **Events shown escaped.** Every event's text the page shows is put in as text, and the API
   escapes control characters as the stdio server does (`escaped`): the carry-over row on a
   progress event's message is closed for the playground with a test.

- [ ] Done when: row 1's ten Chrome cases, row 2's and row 9's pass in both `e2e` jobs;
  `play::a_start_without_a_matching_fresh_preparation_is_refused_as_stale`,
  `play::a_start_sent_twice_with_one_request_id_answers_one_run`,
  `play::a_repeat_after_the_handle_expired_or_the_host_restarted_answers_its_run`,
  `play::a_conflicting_request_under_one_id_is_refused_as_a_conflict`,
  `play::a_repeat_is_answered_while_four_runs_are_going`,
  `play::a_preparation_made_under_another_config_is_refused_as_stale`,
  `play::the_first_import_into_a_new_store_keeps_the_picture_and_its_name_in_one_publication`,
  `play::a_picture_that_fails_to_decode_leaves_nothing_in_the_store`,
  `play::a_drawn_shape_is_committed_as_a_location_bound_to_its_picture`,
  `play::each_choice_is_prepared_and_answered_with_its_readiness_kind`,
  `run.spec.ts: each of the five squint use cases runs from its form`,
  `run.spec.ts: the running step list shows a step running and an attempt started before the run ends`,
  `play::an_event_message_with_an_escape_sequence_is_answered_escaped`,
  `describe::each_recipe_and_loop_names_its_kit_and_a_title_held_to_the_rule`,
  `describe::the_description_is_static_with_kits_and_titles`, `home.spec.ts: a third kit
  of fake actions shows on home, in the switcher and in search` (on a fixture registry), and
  `just docs` regenerated.

## W2. The result: compare, verdict, Send to, Run again, Copy command (large)

### Goal

What a finished run gives a person: see what changed, read what squint concluded, take the
result on to another task, try again from it, and take it away as a command. Rows 3, 4 and 8.

1. **`GET /api/runs/{id}/record`**: the record as the page needs it: outcome, verdict,
   reasons and findings with their places (K2), attempts (`run/6`), the cost split by
   capability and backend from the ledger's records of the run and its children (free local
   work shown as free), and the outputs with their types.
2. **The result screen**: the outcome or verdict tag; compare (side by side, swipe,
   difference) of the input picture and the output, and of each attempt's picture where the
   record holds one as a diagnostic, marked as such; the verdict card; "How it got there",
   each attempt and its outcome; the cost table; the run's id. "Save picture" downloads the
   output.
3. **Send to.** The descriptors say a port's artifact type but not its role, so Send to is two
   things. By type, for any task of any kit: an output fills each port of the other task
   that takes its type, when exactly one port does; where two or more do, the draft opens
   with none of them filled and the person chooses (no guess). By the job, for squint: a
   rascal result whose run had a phrase (`remove_prompt`, `remove_checked`) opens "Check an
   edit" (`source-fidelity`) with its input as the source, its output as the candidate, and
   the job the loop writes from the phrase (`whizbang_rascal::checked::job`); any other rascal
   result opens it with source and candidate and an empty brief. The new draft opens in the
   other kit with its inputs tagged FROM <KIT>, the sent-from banner, and "Back to the run".
4. **Run again** opens a new draft from the run's saved inputs and resolved settings; changing
   it changes nothing of the run it came from (the draft is a copy; the run is the store's).
5. **Copy command.** `GET /api/runs/{id}/command`: the `whizbang` command line that makes the
   same call, from the record's action, its arguments, and the settings it resolved, with each
   input named by its identity in the store, and the store, config, env file and models
   directory named as options by their paths (never a value from the env file); every
   argument quoted for a POSIX shell. A choice the run resolved from the config's default
   (a backend, a judge) is written as an explicit flag, so a later change of default does not
   change what the command calls. A config whose digest is no longer the one the run recorded
   is said above the command, as a comment line naming both digests: the command then runs
   under today's config, and the page says so. A squint check's whole job (requirements
   included, as `checked::job` writes them) cannot be said with `--source`, `--reference`
   and `--brief`, which make no requirements, and `--job` reads a file and pictures from
   paths: so `whizbang squint check` gains `--job-json <text>` with `--image <identity>` once
   for each of the job's images in its order, which takes a `job/1` as the stored run holds
   it. The fixture example gains a command-line mode: given `cli` as its first argument it is
   the binary's own parser and dispatch (`main.rs`'s commands, made callable from the
   example by this task, after H1 has added `play` there) over the fixture's fake wiring. The
   test stops the fixture host first (the store has one writer), runs the copied text with
   `whizbang` replaced by `play_fixture cli`, whose fakes are the host's, against the same
   store, and holds the two runs to the same call: the
   same action, the same arguments once each default is filled in, the same inputs, the same
   resolved settings and the same backend and model for each need. (Not the request
   fingerprint, which tells an omitted default from the same value written out.) Nothing renders a run as a command
   today, and each action's subcommand and flags are its own (`whizbang rascal
   remove-checked`, `whizbang squint check`), so the map from an action to its command lives
   beside the commands, in the binary (`crates/whizbang/src/command_of.rs`), with a test that
   every recipe and loop in the description has an entry and that each entry's flags parse
   with the command's own parser. A user's recipe file gives `whizbang rascal run <file>`
   with its inputs as `NAME=VALUE`.

- [ ] Done when: row 3's, row 4's and row 8's Chrome cases pass in both `e2e` jobs;
  `play::the_record_route_splits_cost_by_capability_and_backend_and_names_free_local_work`,
  `play::send_to_offers_each_task_whose_ports_a_results_outputs_fill`,
  `play::the_command_of_a_run_makes_the_same_call` (for a recipe, a loop, a squint use case
  sent from a removal, with its requirements, a primitive and a user's recipe file),
  `cli::squint_check_job_json_takes_a_stored_job_with_images_by_identity`,
  `cli::the_command_copied_from_a_mask_rect_run_makes_the_same_call_with_the_shipped_binary`
  (a task that needs no model, run by the release binary itself),
  `play::the_command_quotes_every_argument_for_a_posix_shell` (quotes, `$`, backticks, new
  lines),
  `play::the_command_names_a_default_choice_explicitly_and_says_when_the_config_changed`,
  `play::send_to_leaves_a_port_empty_when_two_ports_take_the_same_type`,
  `send.spec.ts: a removal sent to check an edit carries the job the loop writes`.

## W3. History, stats, reports and their export; the brand (large)

### Goal

Every run is findable and readable after a restart, each tool's report is shown in place and
squint's can be taken away as a document. Rows 6, 7 and 11, and `SPEC.md` section 3's
"the playground shows the same figures" as `runs stats`.

1. **History.** `GET /api/runs?…` over the store's runs, newest first, by pages: kit, task,
   result (pass, fail, unsure, done, cancelled, error, interrupted), words (the arguments'
   text and the pictures' names, matched without case); each row's thumbnail, task, what was
   asked, result tag, models, time, cost, when. Opening a row opens its result.
2. **Stats.** `stats::stats` returns the command's text today. W3 splits it: the choosing of
   runs (all of `stats::Filter`, which is action, model, backend, use case, matrix, since and
   until, and the page's kit, result and words, added to the filter) and a typed aggregation
   (`stats::Figures`) that both `runs stats` prints and `GET /api/stats` answers, so the two
   cannot differ: counts, outcome rates, verdict and unsure rates, retries and
   repairs, time as median and 95th percentile, cost, and the unsure verdicts by rule. **Cache
   hits** are not counted today (`runs stats` prints "cache hits: not recorded"); they are
   counted from each record's `decision_cache` note, for the command and the page alike.
3. **The report** of a run, one self-contained HTML document made by the host from the record
   (`crates/whizbang/src/report.rs`; nothing like it exists today, and the lab's pairs page,
   `pairs_page.rs`, is the precedent for its policy and its escaping): for a rascal task, its steps, masks over the picture,
   the result and its cost; for a squint task, its verdict and reasons, its checklist and
   findings with their places drawn on the pictures, each judge turn's text and its cost.
   `read_exchange` cannot give this: it stops at the first withheld part or missing picture
   and says neither where nor how long (`capability/written.rs`, `Unread`). So W3 adds a
   reader for display beside it, `written::for_display`, which reads a stored exchange into
   a list of parts, each text (whole, up to the turn's bound), a withheld part (its position,
   its adapter and its length, which the store keeps), or a picture (its digest, resolved from
   the store or said to be missing), and goes on to the answer; it builds no conversation
   that could be sent. A failed turn shows its error. The size the stored exchanges
   reach is measured on a check at its full budget of turns and recorded in the carry-over
   row, which then closes ("a stored exchange is read by the report", and the bound shown to
   be enough with a measurement). Pictures inline as `data:` addresses, at most
   2,048 pixels on the long side. Every text from a model, a tool or a person is escaped. The
   document carries its own policy (`default-src 'none'; img-src data:; style-src
   'unsafe-inline'`) and no script.
4. **Shown in place, isolated.** The result screen's "Report" tab shows the document in an
   `<iframe sandbox>` with no `allow-scripts` and no `allow-same-origin`, given its text by
   `srcdoc`, so it has an origin of its own and can reach neither the page nor the API
   (`SPEC.md` section 10, "report content shown in an isolated origin").
5. **Export.** "Export" downloads the report as `<task>-<run id>.html` (squint's exported as a
   document, `SPEC.md` section 7); "Export run" downloads the record and outputs as
   `runs export` and the artifact export write them.
6. **The brand.** On the owner's yes (open question 3), the wordmark, mark, emblem and crew
   art under `ui/public/brand/`, from the files the owner hands over, with a line in
   `ui/public/brand/README.md` that they are the owner's own art and not under the code's
   licence; Home shows them. Without the yes, Home keeps the drawn wordmark and an empty frame.
7. **The tool records in place.** `SPEC.md` section 8 says the playground shows each tool's
   and task's record. `GET /api/records/{name}` serves `docs/reference/tools/<name>.md` and its
   images (about 20 MB in all today, embedded in the binary from a list `just docs` writes,
   `crates/whizbang-serve/src/play/records.rs`, held to the folder by a test); the page renders
   the Markdown into the same isolated frame as a report (item 4), and links each tool and
   task to it from the workspace, the kit panel and search. The Markdown is turned into
   HTML in the page by `marked` (MIT), and shown only inside the isolated frame. A record's
   images are named by paths relative to it, which a frame with no origin of its own cannot
   fetch and could not send the token with: so the page fetches each image through the API,
   with the token, and puts it into the document as a `data:` address before it gives the
   document to the frame, as the report's pictures are. The binary grows by that much;
   the note reports the size.
8. **The carry-over rows of the report viewer**: the reference page shows each action's
   arguments, ports and needs (the row on `docs/reference/actions.md`), since the report and
   the workspace read the same fields; squint's typed journal events are answered by K1;
   `canary.rs` and the
   localisation of `metrics.rs` are dropped with a line (no report needs them); an artifact
   type's preview is answered by the canvas's marks.

- [ ] Done when: row 6's, row 7's and row 11's Chrome cases pass in both `e2e` jobs;
  `report::a_squint_report_holds_its_verdict_findings_places_and_turns_and_no_script`,
  `report::every_text_from_a_model_or_a_person_is_escaped`,
  `report::the_document_opens_alone_with_its_pictures_inline`,
  `play::history_filters_by_kit_task_result_and_words`,
  `play::stats_answers_what_runs_stats_answers_for_the_same_filter` (on a store holding
  repairs, a cached decision, unresolved spend, a loop and unsure verdicts),
  `stats::cache_hits_are_counted_from_the_records`,
  `report::a_withheld_part_a_missing_picture_and_a_failed_turn_each_show_as_such`,
  `written::one_exchange_with_text_a_withheld_part_a_missing_picture_and_an_answer_reads_whole_for_display`,
  `records.spec.ts: a task's record opens from the workspace with its examples and limits,
  its pictures decoded and its styles applied`,
  `play::the_embedded_records_are_the_reference_folder`, `just docs` regenerated
  with the arguments, ports and needs, and the rows closed with their lines.

## M1. Models: install, readiness, keys and the allowance (medium)

### Goal

A person can see what runs where, install a model, and see what a run may spend, from the
interface. Row 5.

1. **`GET /api/models`**: the shipped catalogue's models, each with its size, licence, what
   uses it, whether it is installed and whole, and an install in progress with its files done
   of its files (`Status::files_done`, `files`), from the models directory and its installer
   (`model_commands.rs`; `whizbang_backends::models::install`).
2. **Get, cancel, remove.** `POST …/get` starts the install the command starts
   (`install_or_attach_carrying`), on a thread of the host's own, one install at a time, with
   a `Cancellation` the host keeps,
   and a second get attaches to it; `…/cancel` cancels through that `Cancellation` (today only
   the request that started an install can cancel it, and the command line never does) and
   the earlier install is left in place; `DELETE` drains and removes, and answers busy when the drain times out (`SPEC.md`
   section 6). A model whose maker asks for a sign-in shows, when the env file names none,
   what is missing and the guide's section, and its Get is not offered: the page never asks
   for a secret.
3. **`GET /api/backends`**: each backend the config offers by capability, with KEY SET, NO
   KEY or NOT SET UP from the config and the env file, never the key or any part of it, and
   its price as the backend states its bound. There is no "Probe": the build has no real
   prober (`backend_status.rs`, `NoProber`), readiness needs none (`SPEC.md` section 6), and
   the backend that needs one first is milestone 6's GPU host; the carry-over row on a real
   prober is re-owned to 6 with that line.
4. **The allowance.** The ledger's allowance of the newest epoch, what is spent and reserved
   since the reset that opened it, with its date, and the config's call cap, read-only, with
   the config file named. "Reset" asks for the new amount and for the word `reset`, and calls
   `spend reset`, which is the owner's own command; the page says that a reset closes the
   books. (Open question 2 may turn this into a monthly allowance; then this item grows.)
5. **The guide's part on models and keys**, in `docs/guides/playground.md`, linked from "How
   to add".

- [ ] Done when: row 5's four Chrome cases pass in both `e2e` jobs;
  `play::no_response_of_any_route_holds_a_key_a_sign_in_or_the_token` (every route's answer
  scanned for the env file's values),
  `play::a_second_get_attaches_to_the_running_install`,
  `play::a_quit_during_an_install_cancels_it_and_leaves_the_earlier_install`,
  `play::status_cancel_and_quit_answer_with_four_runs_and_an_install_going`,
  `play::a_reset_from_the_page_needs_the_amount_and_the_word_and_closes_the_books`,
  `play::models_lists_only_the_catalogues_models_and_the_configs_backends`.

## E1. The loop by hand in the playground, and the guide (medium; the orchestrator's runs)

### Goal

The by-hand check milestone 4 left here, made live in the playground on the owner's Mac, and
the guide finished.

1. **The three cases** of `docs/carry-over.md`, "Acceptance checks moved here", in the
   playground: a clean pass, or a retry then a pass; a vague prompt showing its tries and its
   flag; an unset key leaving the run unchecked and saying why. On the owner's pictures where
   milestone E's slice has found one that retries; on milestone 3's otherwise. Each with its
   run id, settled spend and a screenshot of the result screen kept out of the repo; the
   record, `docs/results/m5-playground.md`, holds aggregates and run ids only.
2. **A quit with a paid run going**: one loop started and quit during its fill; the record says
   cancelled, `spend status` names nothing open, and the store takes a writer at once.
3. **The guide whole**: `docs/guides/playground.md` with every screen in words, each marked
   example by its tier.

- [ ] Done when: the three cases and the quit are in `docs/results/m5-playground.md` with
  their run ids and spend; the `pass` case has a pass (the carry-over line closed) or the
  record says it still owes one and why; the guide is whole.

## N1. The note; then the old repos archived, on the owner's yes (small)

1. **The note**, `docs/plans/m5-note.md`, in milestone 4's shape: the row's clauses each with
   its evidence (the Chrome cases by name, held by `scripts/tests/test-notes.sh`), the timing
   report by suite and by job, the spend, what moved to the carry-over list, and what a reader
   should not assume. The boxes of this plan ticked.
2. **The spec's wording owed**: section 3's "What every run records", Diagnostics, gains that
   progress events are kept up to a bound per run, with a count of those not kept (K1, item
   4, which these review rounds approved); section 7's playground rows unchanged; section 3 "The
   playground" gains one line naming the host's loopback API and `docs/adr/0041`; section 12's
   `ui/` line names `ui/dist/` as built and committed; section 13's slow tier says the Chrome
   tier runs in its own job beside the fast tier. Section 10's "inside allowed folders" stays:
   the playground takes no path, which is inside it.
3. **The archiving, on the owner's yes** (open question 4), after the note merges and the tag
   `m5` is made: for each of rascal, squint, wafflery and table-manners, the orchestrator
   checks that every kept row of section 7 that names it has its replacement (the note's
   table), sets the repository's description to a one-line pointer to whizbang, and archives
   it (`gh repo archive`), which makes it read-only and leaves it public. No file in an old
   repo is changed and no repo is deleted (`SPEC.md` sections 15 and 16). `docs/sources.toml`
   is unchanged. Success criterion 9 is then met and the note's last line says so.

- [ ] Done when: the note is merged, `m5` is tagged, and each of the four repos shows as
  archived with the pointer (or the owner's no is recorded in the note).

## Carry-over rows owned by milestone 5, and who takes each

| Row (by its subject in `docs/carry-over.md`) | What happens to it |
|---|---|
| The writer lock and a reader's hold are file locks; a store on a network file system | H1, item 7: a replaced `LOCK` is noticed before a publish; the network file system stays unsupported, in the guide |
| A store refused as held by the process that had just let it go | H1, item 7: the host waits for its runs before it drops the store |
| The store's two openings to lock (`Fs::share`, `Fs::try_alone`) | H1, item 7: a store another user owns, or others can write, is refused when opened |
| What running an action does not do yet (journals in memory, abandoned runs) | K1, item 4 bounds a run's journal and diagnostics; H1's stated time bounds a parent's wait; the rest (a panic outside an action) stays, narrowed with a line |
| A run is cancelled only in the process that made it | H1, item 5: one process, the host, owns every run it starts |
| An admission that was visible and not confirmed | K1, item 6 |
| Every publication writes the whole generation again | H1, item 7: measured at 10,000 runs; re-owned to 6 with the number if it holds |
| A progress event's message is not escaped | W1, item 8 |
| A run cannot be sent again from the store; nothing reads a stored exchange | W3, item 3: the report reads stored exchanges, withheld parts said as such, and the size measured; resending stays milestone 7's (replay) |
| The reference page prints name and purpose only | W3, item 8 |
| The deletion's sweep with a reader of another process | H1, item 7 |
| Squint's typed journal events | K1, item 3: typed events kept, in whizbang's shape |
| `canary.rs` and the localisation of `metrics.rs` | W3, item 8: dropped with a line |
| Whether an artifact type has a preview | W3, item 8: the canvas draws masks, locations and places; `describe` says so |
| A report page and its thumbnails (rascal's large results) | W3, items 3 and 1: the report's pictures at most 2,048 pixels, history's thumbnails made on request |
| `ci` has no Chrome end-to-end step | U1, item 6 |
| A missing model cannot be picked in the model list | W1, item 4 |
| A real prober for the playground's list of models | Re-owned to 6 by M1, item 3, with its line: readiness needs no probe, and the GPU host is the first backend that needs one |
| Squint job file: a picture given inline, and `expect` | Re-owned to 6, with this line: the playground sends pictures as uploads and builds jobs from its own fields, so no job file reaches it |
| The in-memory file system of the loop's tests | Re-owned to "the first task that wants it outside the binary's tests": no task of milestone 5 does |
| The child-run test that lists runs with no record | W1, item 1 adds a route that lists a run's children; the test reads them from it if W1 changes how runs are listed, otherwise re-owned |
| The Google rows of prices are introductory | E1, if its runs ask a Google judge on or after a row's date; otherwise each stays with the first live run on or after it |
| Notarisation of the Mac's binary | Untouched: the owner's. The playground is started from a terminal like every other command |

## Documentation that changes

- `docs/guides/playground.md`, new (H1, M1, E1), listed under "Written" in
  `docs/guides/README.md`, where the "Doors" row for the playground goes.
- `docs/guides/install.md`: one paragraph, Chrome is needed for the playground (U1).
- `docs/guides/config.md`: nothing new in the config; a line that the playground reads it and
  never writes it (M1).
- `docs/guides/models.md` and `docs/guides/backends.md`: a line each pointing to the Models
  screen (M1).
- `docs/reference/`: regenerated with kits, titles, arguments, ports and needs (W1, W3).
- `README.md`: the playground in the list of doors, with `whizbang play`, and its stale "State" paragraph brought up to date (W3).
- `docs/guides/config.md`: its stale line that nothing sends anything is corrected while M1 edits the page.
- `AGENTS.md`: `just ui-build`, `just ui-check`, `just e2e` in the commands table, Node in
  "On a fresh clone", and `just e2e` among the heavy commands (U1).
- `docs/adr/0041-the-playground-host.md` (H1) and `docs/adr/0042-the-playgrounds-interface.md`
  (U1).
- `SPEC.md`: N1's wording, item 2.

## Commit strategy

- One task is one branch and one pull request, tests first, reviewed whole by a reviewer that
  did not write it, merged by the orchestrator with a merge commit once `ci` (with its two
  `e2e` jobs from U1 on) and both scans pass. A builder pushes once, when the whole task is
  done, after one full `just check` on the finished tree and `just e2e` where the task
  touches `ui/` or the host.
- An ADR is in the pull request of the task that names it. `ui/dist/` and `docs/reference/`
  are always written by their recipe, never by hand, in the same commit as their sources.
- Every state of `main` between tasks passes `just check` and the `e2e` jobs; every door that
  worked still works: the playground adds a door and changes none.
- **Rollback checkpoints.** The tag `m4`, which exists. `m5-host`, made by the orchestrator on
  the second merge of wave 1: the host and the interface's foundation, no screen. `m5-run`, on
  W1's merge: a task runs end to end in the playground. `m5`, on N1's merge. A pushed tag is
  never moved.
- **If a task shows a decision here is wrong,** the decision is changed in this file first, in
  its own pull request; a change of scope or architecture gets the review rounds first. A test
  is never bent to fit.

| Tasks | To undo |
|---|---|
| H1 | Revert the merge: `whizbang play` is gone and no other command changed; the store's hardening is additive, and each of its tests stands alone |
| K1 | Revert the merge after W1: the kind, the bounds and the events are additions, and a journal of the new format written since is read by nothing older than K1 but the store's own deletion, which does not read it |
| U1 | Revert the merge: `ui/` and the `e2e` jobs are gone; `just check` loses `ui-check` |
| W1 | Revert the merge after W2, W3 and M1. The description's new fields go with it and `just docs` is run again; a store written since holds pictures and locations with their names beside them, which a build without W1 keeps and does not show |
| W2, W3, M1 | Revert the merge; each is a screen and its routes, and none changes what another reads |
| E1, N1 | Records and a note. An archived repo is unarchived by its owner on GitHub; nothing in it was changed |

## Spend

All on approved recipients, on the owner's Mac, by the orchestrator, each run's settled spend
in its results file and the total in the note.

| Step | What is called | Estimate |
|---|---|---|
| H1, K1, U1, W1, W2, W3, M1 | Nothing paid: fakes in every test and Chrome case | 0 |
| E1, item 1: three cases, each run twice | Six loops, up to three fills and three checks each | 1.00 |
| E1, item 2: a quit during a fill | One loop | 0.20 |
| The owner's own tries of the playground | About thirty runs | 1.50 |
| | **Total** | **about 3 dollars, of the 100** |

## Dependencies

```
H1 ──┐
K1 ──┼──► W1 ──┬──► W2 ──┐
U1 ──┘         ├──► W3 ──┼──► E1 ──► N1 ──► (owner's yes) archive
               └──► M1 ──┘
```

- H1, K1 and U1 before W1: W1 wires screens made of U1's components to H1's routes, and
  reads K1's kinds, bounds and events.
- W1 before W2, W3 and M1: each reads runs W1's routes start, and the description's kits.
- E1 after every screen: it is run in the finished playground.
- The archive after N1's merge and the tag `m5`, and only on the owner's yes.

## Risks

1. **A hand-written HTTP server is new attack surface.** Bounded by the guard's single path,
   loopback only, one request per connection, bounds and deadlines on what is read and
   written, at most two uploads at once, and seventeen negative tests; the reviewer of H1 reads the guard first. A framework would bring more
   code than it would save here.
2. **DNS rebinding and other pages on the same machine.** The Host check, the token in a
   header (never a cookie), the Origin and fetch-metadata checks, and no header that allows
   another origin. A local process that can read the person's files can read `play.json`; it
   is owner-only, and such a process could read the store directly anyway.
3. **Committing a built bundle.** It can go stale or hide a change. `just check` rebuilds and
   compares, the build has no source maps and fixed names, and the scans read it as they read
   every file. A minified bundle may trip the clean-room scan by chance; then the offending
   package is replaced or the word kept out by the build, and the owner is told.
4. **Node in the fast tier.** Every machine that runs `just check` needs the pinned Node, and
   npm's registry becomes an origin CI and developers fetch from (open question 1). The
   packages are few and pinned by the lock file; only Preact ships.
5. **The Chrome tier's time and flakiness.** Hosted runners vary; cases wait on what the page
   shows, never on a time; one worker; a retry is not allowed to turn a failure into a pass
   (Playwright's `retries: 0`). If the job passes about 8 minutes, cases are merged or moved
   before anything is raised.
6. **A store held for days is new.** The carry-over rows were written for this; H1 answers
   each, and the measurement at 10,000 runs says whether publication cost needs more before
   milestone 6.
7. **The description changes shape** (kits and titles). It is a static description and its
   version goes up once, in W1; every door reads it, and the reference is regenerated.
8. **What the design shows and the engine lacks** ("What the design shows…") may read to the
   owner as missing work. Each is a row there, and the questions below ask about the two that
   change behaviour.
9. **The binary grows** by the interface (well under 1 MB) and the tool records' images (about
   20 MB today). The note reports the size; if the owner finds it too much, the records'
   images can be served at a smaller size, which is a change to W3 alone.
10. **The budgets are estimates.** A pull request that passes about 2,500 changed lines splits
   at its named seam, and the owner is told the count.

## Open questions

Each can be answered yes or no. The recommended answer is the one the tasks assume.

1. **New dependencies.** I need your yes to pin Node (its current long-term release) as a
   tool `just check` needs; to use npm's registry for six development packages (Vite,
   TypeScript, Vitest, happy-dom, Preact's Vite preset, Playwright) and two that ship inside
   the binary, Preact and marked (both MIT, together about 50 KB); and to add one Rust crate
   to the binary, `signal-hook` (MIT or Apache-2.0, with its `signal-hook-registry`), so that
   Ctrl-C in the terminal cancels and accounts for running work. No to the first two means
   a page with no framework and no build; no to the third means Ctrl-C ends the host at once
   and its runs are ended as interrupted when the store is next opened, which misses one of
   the spec's kept rows. Recommended: yes. (U1, H1)
2. **The allowance as it is today.** I need your yes that the playground shows the allowance
   "since your last reset" and lets you reset it there (as `whizbang spend reset` does), and
   shows the per-run cap read-only from your config, instead of the design's monthly
   allowance and editable per-run cap. No means a task is added for a monthly allowance in
   the ledger and for writing the config. Recommended: yes. (M1)
3. **Your brand art in the repo.** I need your yes to commit the wordmark, the mark, the
   emblem and the crew illustration under `ui/` (the repo is public), from files you hand
   over, marked as your own art. No means Home shows the drawn wordmark and no picture.
   Recommended: yes. (W3)
4. **Archive the four old repos when milestone 5 ships.** I need your yes for the
   orchestrator to set each repo's description to a pointer to whizbang and archive it on
   GitHub (read-only, public, nothing deleted, no file changed). No means you archive them
   yourself, and the note records it when you have. Recommended: yes. (N1)

## What the owner is told, not asked

- **Nine pull requests**, six of them large; H1, K1 and U1 run side by side first.
- **The engine lacks some of the design**: a local judge and local fill (milestone 8), Pixel,
  a monthly allowance and an editable per-run cap. They are left out or shown in the engine's
  terms, listed in "What the design shows that the engine cannot back yet".
- **The playground takes no file path from the page**: pictures are uploaded and results are
  downloaded, which is stricter than the spec allows.
- **Chrome is needed** on the machine that runs `whizbang play`, and the Chrome tier runs in
  its own CI job on both runners, outside the fast tier's 300 seconds.
- **The tags `m5-host`, `m5-run` and `m5`** are made by the orchestrator as above.

## Review rounds

### Review round 1

On draft 1. Verdict: revise. Seventeen findings: 1 blocker, 15 majors, 1 minor. Each was
checked against the code; fifteen taken in full, one in part, one answered by re-owning.

1. **Blocker: unauthenticated clients can hold every connection worker.** Right: no
   deadline bounded how long a connection holds a worker. Taken: deadlines on headers, body
   and answer, with tests of trickling and silent clients.
2. **Major: control requests can be blocked by the work they control.** Right: the call
   limit sat in the guard, and installs ran on the connection pool. Taken: the limit applies
   to starting a run alone; installs run one at a time on a thread of their own and are
   cancelled by a quit.
3. **Major: the claimed per-run memory bound does not exist.** Right
   (`contract/journal.rs`). Taken: K1, item 4, bounds a run's journal; the parent's wait is
   settled in round 2.
4. **Major: signals through `libc` need unsafe code, which the workspace forbids.** Right.
   Taken: `signal-hook`'s safe iterator, two new crates, asked of the owner (open
   question 1).
5. **Major: building as `serve` does stops a no-key start.** Right (`Uses::Every`). Taken:
   the host asks for no backend at start; a missing key is a not-ready kind.
6. **Major: `play_import` cannot take uploaded bytes through a port.** Right: inputs must be
   in the store at preparation. Taken: the host imports through `import.rs` with its own
   writer, the name beside it in one publication, and a route for drawn locations.
7. **Major: preparation and status do not hand out per-step bounds or live steps.** Right.
   Taken: new task K1 (readiness kinds, each need's bound, typed events); the plan block
   shows needs, not recipe steps.
8. **Major: inputs and Send to assume roles the descriptors lack.** Right (a use case takes
   `images` and `job`). Taken: squint's form builds `job/1` as `Job::from_shorthand`; Send to
   by type when one port fits, by the loop's job from a removal; a picker for every artifact
   type; a case for each of the five use cases.
9. **Major: a preparation is not a wire type, and a stale rule can refuse a retry.** Right.
   Taken: the host keeps preparations under handles; a repeated request id is looked up
   first.
10. **Major: Copy command does not reproduce saved settings.** Right. Taken: defaults written
    as explicit flags, the env file named by path, a changed config said, the host stopped
    before the copied command runs.
11. **Major: `read_exchange` cannot read every stored exchange.** Right. Taken in draft 2 by a
    report reader, then replaced in round 2 by a display reader (round 2, finding 4); the
    size measurement assigned to W3.
12. **Major: stats coverage is incomplete and cache hits are not counted.** Right
    (`stats.rs`). Taken: every filter of `stats::Filter`, repairs, unsure by rule, and cache
    hits counted for the command and the page.
13. **Major: the playground never shows the tool records.** Right (`SPEC.md` section 8).
    Taken: W3, item 7, records embedded and shown in place, with a Chrome case.
14. **Major: the real prober is mapped but not planned.** Right (`NoProber`). Answered by
    re-owning, not building: readiness needs no probe, and milestone 6's GPU host is the
    first backend that needs one; "Probe" leaves the Models screen.
15. **Major: `ui-check` in `check` does not put it in CI, and the time budget is not
    combined.** Right (`ci-without-scripts`). Taken: `ui-check` in every check recipe and
    job, its seconds counted under the one 300-second limit, and a test that a type error or
    a stale bundle fails.
16. **Major: the store's openings check modes and not the owner.** Right. Taken: a store
    owned by another user is refused to a reader and a writer.
17. **Minor: a light theme nobody can reach.** Taken in part: no light screen and no light
    switch; the light tokens stay, and their contrast test, because the owner asked that the
    light set be kept so light can return.

### Review round 2

On draft 2. Verdict: revise. Nine findings: 1 blocker, 6 majors, 2 minors. All nine were
checked against the code and taken.

1. **Blocker: a missing preparation still lets a call run.** Right: without an expected
   preparation the executor prepares and admits (`contract/engine.rs`). Taken: K1, item 5, a
   request marked as needing a preparation is refused stale after the repeat and conflict
   lookup, before anything is reserved; other doors unchanged.
2. **Major: cancellation does not bound a parent's wait.** Right (`contract/live.rs`). Taken
   by saying so: a run that ignores its cancellation keeps its place until Quit; the
   carry-over row is narrowed to that, with a test.
3. **Major: reserved workers cannot be reserved before a request is read, and Chrome opens
   six connections to a host.** Right. Taken: sixteen workers, an acceptor that never reads,
   at most two uploads at once on the host and in the page.
4. **Major: `read_exchange` stops at the first withheld part.** Right
   (`capability/written.rs`). Taken: `written::for_display`, which keeps text, withheld
   parts and missing pictures in one list.
5. **Major: Copy command cannot say a whole job.** Right: shorthand makes no requirements,
   and a fingerprint tells an omitted default from a written one. Taken: `squint check
   --job-json` with `--image` by identity, and a same-call comparison.
6. **Major: record images cannot load in an isolated frame, and `srcdoc` inherits the
   page's policy.** Right. Taken: images fetched with the token and inlined as `data:`;
   the page's policy allows inline styles, not scripts.
7. **Major: the journal bound drops kept evidence.** Right (`SPEC.md` section 3). Taken: a
   run keeps its first events and counts the rest, never drops what it kept, and N1 writes
   the change into the spec.
8. **Minor: the import does not keep metadata.** Right (`import.rs`). Taken: the plan says
   so; the file name is the one thing kept beside a picture.
9. **Minor: `stats::stats` returns text.** Right. Taken: a typed aggregation,
   `stats::Figures`, that the command prints and the API answers.

### Review round 3

On draft 3. Verdict: revise, with no blocker. Four findings, all majors, all checked against
the code and taken.

1. **Major: shutdown can block in `Executor::cancel` before the grace starts.** Right
   (`LiveRun::cancel` waits in a final publication). Taken: the grace starts first, held
   cancellations are tripped without waiting, and the executor's cancel and waits run off
   the grace's thread, with a test.
2. **Major: a shared 256 MiB diagnostics budget competes with reserved exchange storage.**
   Right (`capability/mod.rs`). Taken: no new shared budget; only progress events get the
   keep-first bound.
3. **Major: the copied-command test has no fakes once the fixture host stops.** Right. Taken:
   a command-line mode of the fixture over the same fakes, and a shipped-binary case for
   `mask_rect`.
4. **Major: H1 and K1 both change `contract/engine.rs`.** Right. Taken: the unconfirmed
   admission is K1's, item 6.

### Review round 4

On draft 4. Verdict: revise, with no blocker. Two findings, both majors, both taken.

1. **Major: Stop through `Executor::cancel` can hold a connection worker.** Right. Taken: the
   route trips the held cancellation and answers "stopping"; one canceller thread calls the
   executor once a run, with a test of Stop pressed often during a final publication.
2. **Major: U1 and H1 both change `crates/whizbang/src/main.rs`.** Right. Taken: the fixture's
   command-line mode moves to W2, after H1 has added `play`.

### Review round 5

On draft 5. Verdict: approve. No blocker and no major; both fixes of round 4 checked against
the code.
