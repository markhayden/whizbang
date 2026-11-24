# 0024. The download client: `rustls`, its roots compiled in, and a narrow HTTP/1.1 of its own

Status: accepted. Builds what ADR 0012 ("Three interfaces") and ADR 0019 ("A download and a
probe", "For later tasks") left to checkpoint 1C, and changes neither.
Date: 2026-11-16
Amended: 2026-11-16, after the security review of its pull request. Changed: only bytes of
the answer, and of the request, count toward the limit between two reads and the least
rate, and a ceiling of reads of TLS alone is added ("Limits"); the request's bytes are
zeroed once written; the end of a head is looked for in the new bytes only; the test
kit's listener serves 32 connections at most. Added: what a name resolves to, and the
tests of a name, a twenty-digit length and a server that sends TLS alone.
Amended: 2026-11-17, by plan 2A, task S1 (its first pull request). Added: the sender
("The sender"), the same client making one `POST`; nothing of the download changed.
Amended: 2026-11-17, by plan 2A, task S1 (its second pull request) and the security
review of the first. Added to "The sender": chunked answers, bounded; the run's deadline
held over the whole request; a request larger than TLS takes at once. Changed in "How a
test reaches a server on loopback": the test root and the client that trusts it are
compiled under the feature `fakes` too, for the tests of the crates that turn it on.
Amended: 2026-11-23, by plan m6, task R1 (ADR 0046, "The kind `remote`", "The client's way
of frames"). Added: a request of frames, a `POST` of the bytes a check let through whose
answer's head and body are handed to the caller as they arrive, its wait for the answer
the request's own; a control send that follows it to the address its connection reached,
resolving no name, within a deadline of its own; and the prober, a `GET` with no
credential that hands on its status and the start of its body. Chunked bodies are read
by a send, a request of frames and a probe; a download still refuses them.
Spec: section 10, "Egress"

## Context

Task N1 of checkpoint 1C (`docs/plans/m1c-local-pack.md`) puts a real `Downloader` behind
the checked downloader of ADR 0019: one blocking client over TLS, which follows nothing,
streams a file to its sink, stops at the bytes it was asked for, and has three limits (to
connect, between two reads, and a least rate). The plan's "Rules" say why its roots are
compiled in: the platform's verifier may ask a revocation service nobody approved. Nothing
in the build had a network client before this task: no crate of the lock file could speak
TLS, and no code under `crates/` opened a socket.

The client is the one place where whizbang's own process reaches a host off the machine,
so what it can be made to do, and what it cannot, is the decision.

## Decision

### Two crates, approved by the owner on 2026-11-16

- **`rustls`**, pinned exactly at `=0.23.46`, with its default features off and `ring`,
  `std` and `tls12` on, and nothing else: no `aws-lc-rs`, no logging, no compression of
  certificates, no platform verifier.
- **`webpki-roots`**, pinned exactly at `=1.0.12`: Mozilla's list of roots, as data.

Both from crates.io, and both are dependencies of `whizbang-backends` alone. The owner
approved exactly these two on 2026-11-16, with those features; no other crate is added.
HTTP/1.1 is written in `crates/whizbang-backends/src/net/` itself.

What the lock file gains with them, each from crates.io, with its licence as its
metadata states it:

| Crate | Version | Licence | Why it is there |
|---|---|---|---|
| `rustls` | 0.23.46 | Apache-2.0 OR ISC OR MIT | Asked for |
| `webpki-roots` | 1.0.12 | CDLA-Permissive-2.0 | Asked for |
| `rustls-webpki` | 0.103.17 | ISC | The certificate check, of `rustls` |
| `rustls-pki-types` | 1.16.0 | MIT OR Apache-2.0 | The types of keys and certificates |
| `ring` | 0.17.14 | Apache-2.0 AND ISC | The cryptography (`ring` feature) |
| `untrusted` | 0.9.0 | ISC | Of `ring` and `rustls-webpki` |
| `subtle` | 2.6.1 | BSD-3-Clause | Of `rustls` |
| `zeroize` | 1.9.1 | Apache-2.0 OR MIT | Of `rustls` and its key types |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 | Of `rustls` |
| `getrandom` | 0.2.17 | MIT OR Apache-2.0 | Of `ring` |
| `cc` | 1.11.2 | MIT OR Apache-2.0 | `ring`'s build: compiles its C and assembly with the system's compiler |
| `find-msvc-tools` | 0.1.15 | MIT OR Apache-2.0 | Of `cc`, for Windows |
| `shlex` | 2.0.1 | MIT OR Apache-2.0 | Of `cc` |
| `wasi` | 0.11.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Of `getrandom`, for WebAssembly only |
| `windows-sys` | 0.52.0 | MIT OR Apache-2.0 | Of `ring`, for Windows only (0.61.2 was already in the lock) |
| `windows-targets` and the eight `windows_*` crates | 0.52.6 | MIT OR Apache-2.0 | Of `windows-sys` 0.52, for Windows only |

**No build script fetches anything.** Four of them have one: `rustls`'s (it turns on a
language feature on a nightly compiler when `read_buf` is asked for, which it is not),
`ring`'s (it compiles the C and the assembly that ship in the crate with `cc`; the
programs that generate them, `perl` and `nasm`, are run only when the crate is built
from its git repository, not from crates.io), and the `windows_*` crates' (each prints
the folder of the import library it ships). None opens a connection or starts a program
that does. `cargo deny check licenses bans sources` passes; `deny.toml` already allowed
`CDLA-Permissive-2.0`, for this crate's data, and is not changed. Its one new note is a
warning, which the configuration allows: two versions of `windows-sys`.

### The one way to it

`whizbang_backends::net::downloader(least_kib_per_second, origins, secrets)` answers a
`CheckedDownloader` held to `origins`, with the client inside it. It is the only public
item of the module beside the two limits, and the client, its constructors, its wire and
its HTTP are `pub(crate)`: no code outside the crate, the binary included, can make the
client bare and ask an address the gate did not
(`public::the_only_public_way_to_the_client_is_a_checked_downloader`, which also reads
the module's sources for every `pub` item). The client follows nothing: an answer that
points elsewhere comes back as `Downloaded::Moved`, and the gate asks its questions of
the address before anything is asked there
(`client::an_answer_that_points_elsewhere_is_handed_back_as_moved_and_nothing_is_asked_of_the_second_address`,
`client::a_download_through_the_checked_downloader_that_is_moved_off_its_entrys_origins_is_refused`,
where a second listener off the entry's origins sees no connection at all).

### A narrow HTTP/1.1 that refuses what it does not handle

The module's top (`net/mod.rs`) states it whole. In short:

- `https` only; an `http` address is refused before anything is sent.
- `GET` only. The fields sent are fixed: `Host`, `User-Agent: whizbang`, `Accept: */*`,
  `Accept-Encoding: identity`, `Connection: close`, and the credential's own field when
  the request carries one (`http::tests::the_request_is_a_get_with_fixed_fields_and_the_credential_alone_beside_them`,
  `client::a_credential_is_sent_as_its_header_to_its_own_origin_and_a_request_without_one_sends_none`).
  Nothing is read from the environment: no proxy variable, no key log.
- One request on one connection, closed after its answer; nothing is kept or used again.
- An answer is a status line, a head of at most 64 KiB, 100 fields and 8 KiB a line, and a
  body by its one `Content-Length`. **Chunked bodies are not handled:** any
  `Transfer-Encoding` is refused by name. No compression is asked for or accepted. No
  trailers can come.
- Refused, each in fixed words, each with a test of exact bytes
  (`client::the_answers_the_subset_does_not_handle_are_each_refused_by_name`): an interim
  `1xx` (a `103` before a `200` included); no length; a length that is not digits alone
  (`3, 3`); two `Content-Length` fields; a length beside a transfer coding; a chunked
  body; a compressed one; a line over 8 KiB; a bare carriage return; a bare line feed; a
  folded line; more than 100 fields; a status line that is not HTTP/1.1 or 1.0; a `206`
  and a `304`, statuses the client does not act on; and a body cut before its length.
- It acts on `200` (the body), `301`, `302`, `303`, `307` and `308` (the one `Location`,
  a whole `http` or `https` address or a path at the same origin, made an `Address` with
  the executor's secrets), `401` and `403` (`credential`), `404` and `410` (`rejected`),
  and `408`, `429`, `500`, `502`, `503` and `504` (`unavailable`). Every other status is
  refused as `malformed`.
- A body goes to the sink as it comes, in pieces of at most 64 KiB, and stops at the
  bytes the request asked for or its stated length, whichever is less; nothing more is
  read (`client::a_download_is_streamed_to_its_sink_and_stopped_at_the_bytes_it_was_asked_for`).
- **A fault quotes nothing** of an address, a field, a reason phrase or a body: every
  sentence is fixed words, and an address a server points to that holds a secret is
  refused without being shown
  (`client::a_fault_holds_nothing_of_the_body_the_headers_or_the_address_that_answered`).

### TLS

- TLS 1.3 and 1.2, nothing older (`client::the_client_speaks_tls_1_3_and_1_2_and_each_connection_is_a_full_handshake`).
- The server's certificate is checked by `rustls` against the roots compiled in and
  against the host of the address the gate let through. **There is no option to turn the
  check off:** no code of the module names `rustls`'s `dangerous` configuration, and a
  test reads the sources for it. The platform's verifier is not used, and the lock file
  holds no crate that would reach it or the platform's roots
  (`client::the_client_trusts_the_roots_compiled_into_it_and_asks_no_other_service`,
  which also shows the client the crate makes refusing a server whose root is the
  test's, and a certificate the test's root trusts but for another host, both in the
  handshake, before a byte of the request).
- **Session tickets and early data are off.** No session is resumed
  (`Resumption::disabled()`), so each connection is a full handshake with a full check
  of the certificate; and nothing is sent before the handshake ends
  (`enable_early_data = false`), so no request can be replayed. What this costs is one
  handshake for each file, against downloads of gigabytes. A server's tickets are
  ignored. Kept on, a resumed session would skip the certificate's check on that
  connection and tie two downloads together for the server, for nothing whizbang needs.
- ALPN offers `http/1.1` alone.
- **A name is checked, then resolved by the system, unfiltered.** The gate checks a
  name against the entry's origins; the client then has the system resolve it, and
  connects to whatever comes back, which may be a loopback or a private address. What
  stands between is the check of the certificate against the name the gate let through:
  the name, not the address it resolved to, is what is checked and sent in the
  handshake. A test asks `localhost`, from the hosts file: a certificate for that name
  is taken and the server is told the name, and a certificate for `127.0.0.1` alone,
  where the name leads, is refused
  (`client::a_name_is_checked_against_a_certificate_for_that_name_and_asked_for_by_it`).
  The client puts no bound on how long resolving takes; whether a name that resolves to
  a private address is refused, and how long its resolving may take, are the
  installer's command's to decide (tasks N3b and N5).

### How a test reaches a server on loopback

**Changed by task S1 of plan 2A, its second pull request:** the root, the server's
certificate (`net::testing::Pki`), `Client::trusting` and the executor's ways out over
a client that trusts the test's root (`net::testing::ways_trusting`) are compiled into
this crate's tests and, under the feature `fakes`, into the tests of the crates that
turn it on, which the binary's tests need to reach a judge on loopback through its
wiring (`wiring::Over::trusting`, under `fakes` too). As `Launch::testing` is: the
binary that ships, `just smoke`'s and a release build, is built without `fakes`
(`scripts/check-features.sh` fails a build of `-p whizbang` that turns it on), so no
source of it can name any of this. What follows is as N1 wrote it.

The client trusts the compiled roots and no other, so a test's server needs a root the
client is handed. `Client::trusting(roots, …)` takes one, and is `#[cfg(test)]`: it is
compiled into this crate's own tests and into nothing else, so no other code can call it,
the binary's included, since a build that is not of those tests has no such function
(the release build of the binary, which `just smoke` makes, would fail to compile a call
of it). So the client's tests are inside the crate (`src/net/tests/`, named `client::…`
here), not under `tests/it/net/`, where they could not reach it; `tests/it/net/` holds
what is public (`public::…`).

The root and the server's certificate are made in the test, from fixed made-up seeds, as
Ed25519 keys whose PKCS#8 is the seed, signed with `rustls`'s own signing
(`rustls::crypto::ring::sign::any_eddsa_type`), with a few lines of DER written in the
test. No key file is committed, and no crate is added for it.

The servers are the test kit's listener on loopback (`whizbang_testkit::loopback`): it
binds `127.0.0.1` alone, serves a fixed number of connections (32 at most) on one thread
made through `whizbang_testkit::threads`, sets a 5-second limit on each read and write
of a connection it serves, and is joined on every path: `finish` waits for its thread,
and a drop, on a return or in a panic, sets a stop and wakes a blocked accept with a
connection of its own to its own port, which it does not serve or count. It holds its
socket until the thread is joined, so that connection reaches nothing else
(`loopback::…`, in the test kit's tests). Nothing in this task contacts a host off the
machine.

### Limits

- **To connect:** 10 s for all of a host's addresses together, each tried with the time
  left, by the injected clock; 8 addresses at most.
- **Between two reads:** 30 s from one byte of the answer to the next, or from the
  request's last byte to the answer's first (or, before that, from the connection or
  the handshake's last byte written).
- **A least rate,** a setting (ADR 0011's `download_least_kib_per_second`, 256 KiB a
  second when the config gives none; the command that makes the client is task N3b's):
  fewer bytes than that over any whole minute, counted in whole seconds, ends the
  download as `timed_out`. The bytes counted are the answer's, its head and its body,
  as TLS hands them over, from the moment the socket connected.
- **Only the answer counts.** Records of TLS that hold none of the answer (the
  handshake's, a session ticket's, a key update's) count toward neither limit: a server
  whose certificate is good could send them for ever, and `rustls` puts no bound on how
  many tickets of TLS 1.3 it is sent. And more than 4,096 reads of them in a row end the
  download as `malformed`, so that a clock that stands still cannot keep it alive
  either (`client::a_server_that_sends_tls_records_and_none_of_the_answer_ends_by_the_limit_between_reads`,
  where the server sends key updates and nothing else).
- **A response's size** is what the caller states (`DownloadRequest::most_bytes`). A
  length of more than 19 digits is refused, not read as the most there is.
- **The end of a head** is looked for in the bytes that came since the last look, and
  the three before them, so that a head that comes a byte at a time is not looked over
  again at each byte
  (`http::tests::the_end_of_a_head_that_comes_a_byte_at_a_time_is_found_from_where_the_last_look_stopped`).

**A credential is held once.** The request is made in one allocation of its exact size,
so its value is copied once, into those bytes, which are overwritten with zeros once they
are written. `rustls` encrypts what it is handed into records of its own, and the
caller's `Credential` keeps the value as long as the caller holds it.

The socket's own read and write limit is a poll, 250 ms; each time one comes back with
nothing, the cancellation, the limit between two reads and the least rate are looked
at by the injected clock. A count of polls with nothing is a second limit beside the
clock, so a clock that stands still, as a test's may, cannot make a wait endless. A
cancellation is seen within a poll, between two pieces of a body, and by the limit
between two reads at the latest. Each limit has a test at its bound with no real
sleeping: the least rate by a meter given exactly the rate for a minute and one byte a
second less (`wire::tests::the_least_rate_is_kept_by_a_minute_of_exactly_that_many_bytes_and_not_by_one_less`,
`wire::tests::the_least_rate_is_held_over_any_whole_minute_and_not_over_the_whole_download`)
and by a server that sends a byte for every ten seconds of a clock it moves itself
(`client::a_peer_that_sends_a_byte_now_and_then_ends_the_download_as_timed_out_by_the_least_rate`);
between two reads by a clock that moves a second each time it is read
(`client::a_peer_that_accepts_and_sends_nothing_ends_the_request_as_timed_out_at_the_limit_between_reads`,
and `client::under_a_clock_that_stands_still_a_mute_peer_ends_by_the_ceiling_of_polls`);
a cancellation while nothing comes
(`client::a_cancellation_set_while_no_byte_arrives_ends_the_request_by_that_limit_at_the_latest`,
`client::a_cancellation_is_looked_at_between_pieces`); to connect, by a connect handed
in by the test that takes the whole time it is given
(`client::the_limit_to_connect_is_for_all_of_a_hosts_addresses_and_a_connect_out_of_its_time_is_not_sent`).

### How far a request got

`Fault::not_sent` is said only before the socket connects: an address refused (not
`https`, or a host no certificate can name), a cancellation before connecting, a name
not resolved, a connect refused or out of time. After it, every fault is unresolved,
a failed handshake included
(`client::a_request_that_never_connected_is_not_sent_and_one_cut_after_it_was_written_is_unresolved`).

### No thread, and one socket for each request

The client makes no thread and starts no process: one call is one socket, on the
caller's thread, closed when the call returns. Shown by a test that hands each piece to a
sink that checks it is on the caller's thread, counts one connection for each request
(a download moved once is two), finds `Connection: close` in each, and reads the
module's sources for a thread or a process
(`client::the_client_makes_no_thread_and_one_socket_for_each_request`); clippy refuses a
thread made anywhere in the workspace but in two allowed places (`clippy.toml`).

### Through the installer

`client::the_installer_installs_a_model_of_two_files_from_the_listener_through_the_checked_downloader`
installs an entry of two files from a listener, through `CheckedDownloader::new(client,
entry.origins())`, one file moved once within the origin and followed by the gate, into a
models directory opened by the opening that asks whose it is.

### The sender

Plan 2A, task S1, its first pull request. The same client implements `Sender`
(ADR 0012), over the same TLS, connection and limits of polls, with the same
statement of how far a request got (`not_sent` only before its socket connects).

- **The one way to it** is `whizbang_backends::net::ways(store_root, secrets)`, which
  answers the executor's `Ways` with the client as their sender and
  `SendCheck::new().keeping_out(root)` over it: the root as an absolute path, and its
  canonical spelling too when that differs (ADR 0019, "For later tasks"). The ways
  approve no recipient; a run sends under its snapshot's policy. `wiring::runner` gives
  them to the binary's executor, over the root of the store it writes to
  (`wired::the_binarys_sender_refuses_a_file_from_inside_the_store_root`). No bare
  sender is public (`public::the_only_public_way_to_the_client_is_a_checked_downloader`,
  which lists every public item of the module).
- **One `POST` with a body of known length.** The head is the `GET`'s fixed fields, the
  request's own fields (`Outgoing::with_fields`: constants of the code that sends, at
  most 8, none of the fields the client writes itself), the credential's field when the
  executor attached one, and `Content-Length`. The body is the request's pieces in
  order, nothing between them: a piece made by `Piece::picture` (`Piece::is_picture`)
  as the base64 of exactly its bytes, the bytes the check behind a send looked at, and
  every other piece as it is. Head and body are made in one allocation of their exact
  size and zeroed once written, as the `GET` is
  (`http::tests::a_post_is_its_head_its_own_fields_the_credential_its_length_and_its_pieces_in_order`,
  `sender::a_post_carries_its_body_once_and_follows_no_redirect`).
- **Every final status is answered** with its status and its body, by its one
  `Content-Length`, at most `REPLY_MAX_BYTES` (16 MiB); what a status means is the
  adapter's to say. A `301`, `302`, `303`, `307` or `308` comes back as `Reply::moved`
  with the address of its `Location`, which is not asked: a second listener sees no
  connection (`sender::a_post_carries_its_body_once_and_follows_no_redirect`). The same
  refusals as a download's head hold: no interim status, no transfer coding, chunked
  included, no compression.
- **Its limits.** To connect, `CONNECT_LIMIT`; from the request's last byte to the
  answer's first, and between two bytes of the answer, `ANSWER_WAIT_LIMIT` (600 s),
  since a model's answer comes whole once it has written its turn; each no later than
  the run's deadline, and a deadline that has passed before the socket connects is
  `not_sent`. No least rate: a model may write for minutes before the first byte.
- **How far a request got** (`sender::a_request_that_never_connected_is_not_sent_and_one_cut_after_writing_is_unresolved`):
  refused to connect, not `https`, cancelled, or past its deadline, `not_sent`, and the
  listener sees nothing; connected, written whole and cut with no answer, unresolved.
- **The run's deadline is held over the whole request** (second pull request, after the
  security review of the first): its connection and every byte of its answer, however
  slowly they come, not only each wait between two reads
  (`sender::the_whole_answer_is_over_by_the_runs_deadline_however_slowly_its_bytes_come`).
- **A chunked answer is read, bounded** (second pull request): a `Transfer-Encoding` of
  `chunked` alone, with no `Content-Length`. Each chunk's size line is hex digits, eight
  at most, then any extensions after a `;`, which are not read, in 256 bytes at most;
  each chunk ends with a line ending; the body is held to `REPLY_MAX_BYTES` as a whole;
  the trailer is 16 lines at most, each within the head's 8 KiB, and not read. Refused,
  each in fixed words: a size line that is not that, a chunk that does not end with a
  line ending, a body over the bound, a trailer over its bound, any other transfer
  coding, chunks beside a length, and a connection that ends before the last chunk
  (`sender::a_chunked_answer_is_read_whole_and_one_the_reader_does_not_take_is_refused_by_name`,
  with exact bytes). A download still refuses every transfer coding.
- **A request larger than TLS takes at once** (64 KiB) is handed over in turns, what TLS
  holds written to the socket between them, each wait bounded as every other is
  (`anthropic::each_picture_in_the_sent_body_decodes_to_the_bytes_of_its_checked_piece`,
  whose request holds pictures of hundreds of kilobytes).

## Consequences

- The binary can now download, through the gate only. Nothing in it makes the downloader
  yet: `whizbang models get` is task N3b's, which gives it the config's least rate.
- `ring` needs a C compiler where whizbang is built. Both CI runners and the Mac have one.
- The build is longer by `ring` and `rustls`.
- **What this subset may not be enough for with the real model host** (task N5 finds out
  before it starts, and the plan already makes the first of these the first thing it
  does):
  - **The Hub's storage redirect.** A file of `facebook/sam3` is moved to the Hub's
    storage host, and its large file is held in its Xet storage. Whether a plain `GET` of
    the address it is moved to serves the whole file in one body with a
    `Content-Length` is not known. If that host answers only in ranges, or in a protocol
    of its own, this client cannot fetch it.
  - **Range requests.** None is made: a download that fails starts again from its first
    byte, and a `206` is refused. A 3.4 GB file over a connection that breaks is
    downloaded whole again.
  - **Chunked bodies.** Refused. A server or a proxy that sends the file, or an error,
    chunked cannot be read.
  - **HTTP/2 only.** ALPN offers `http/1.1` alone; a server that speaks only HTTP/2 ends
    the handshake.
  - **A `103 Early Hints` before the answer** is refused, as every `1xx` is.
  - **A redirect's address** must be an address ADR 0012 takes: at most 2,048 bytes,
    printable ASCII, no `@` and no fragment, a host of ASCII letters, digits and hyphens.
    A signed storage address longer than that, or holding an `@`, is refused as
    `malformed`.
  - **A host's name is resolved by the system,** whose wait has its own limits and is
    not bounded here, to whatever addresses it answers ("TLS").

## Alternatives

- **An HTTP crate** (one of the blocking clients over `rustls`). Turned down: more
  dependencies; proxy variables read from the environment and redirect-following, each on
  by default and to be turned off and held off; and no clean way to bound the time
  between two reads or to know whether a byte left before a fault, which `not_sent`
  needs. The subset whizbang needs is a few hundred lines it can test byte by byte.
- **The platform's verifier, or the platform's roots.** Turned down by the plan: the
  verifier may ask a revocation service nobody approved, and the platform's roots are
  read from the machine, so what is trusted would differ from machine to machine.
- **`aws-lc-rs`, `rustls`'s default provider.** Turned down: a larger build (it needs
  CMake, and NASM on Windows) for nothing `ring` does not do here.
- **Chunked bodies, written and tested.** Turned down for now: the files the plan names
  come with a length, and a reader of chunks is another set of bounds to hold. Refused
  by name until a host needs it.
- **Sessions resumed and early data.** Turned down: see "TLS".
- **The test's root reached by a cargo feature,** on through a development dependency of
  the crate on itself. Turned down: it adds an edge to the crate graph of `SPEC.md`
  section 3, and the feature would be on in every build of the workspace's tests, the
  binary's included. `#[cfg(test)]` is narrower: the crate's own tests and nothing else.
