//! The network (`SPEC.md` section 10, "Egress"; plan 1C, task N1; ADR 0024): the
//! client, behind the checked downloader as the download client, and behind the
//! executor's checked send as the sender (plan 2A, task S1).
//!
//! **The two ways to it** are [`downloader`], which answers a
//! [`CheckedDownloader`](whizbang_core::egress::CheckedDownloader) held to the origins it
//! is handed (ADR 0019, "A download and a probe"), and [`ways`], which answers the
//! executor's [`Ways`] out, the client their sender and the check behind a send over
//! it, keeping out the root of the store (ADR 0019, "For later tasks"). The client
//! itself is not public, so nothing outside this crate can make one bare and ask an
//! address the gate did not. The client follows nothing: an answer that points
//! elsewhere comes back to the gate as an address, and the gate asks its questions of
//! it before anything is asked there.
//!
//! **A narrow HTTP/1.1, written here, that refuses what it does not handle.** Over TLS,
//! with the crate `rustls`, and nothing else beside it:
//!
//! - `https` only. A request to an `http` address is refused before anything is sent.
//! - A download is a `GET`; a send is a `POST` whose body is of known length, its
//!   request's pieces in order, a picture as the base64 of exactly its bytes. One
//!   request on one connection, which the client closes after its answer
//!   (`Connection: close`): no connection is kept or used again.
//! - The fields sent are fixed: `Host`, `User-Agent: whizbang`, `Accept: */*`,
//!   `Accept-Encoding: identity`, `Connection: close`, and the request's credential, in
//!   its own field, when it carries one; a send adds its request's own fields, which
//!   are constants of the code that sends, and its `Content-Length`. Nothing asks for
//!   compression.
//! - Nothing is read from the environment: no proxy variable, no key log, no setting.
//! - An answer is a status line of `HTTP/1.1` or `HTTP/1.0`, a head of fields, and a
//!   body of the length its one `Content-Length` states. A head is 64 KiB at most, a
//!   line of it 8 KiB, and it holds 100 fields at most.
//! - Refused, each in fixed words that quote nothing of what came: an interim status
//!   (`1xx`); a head over its bounds; a carriage return or a line feed that is not a
//!   line's ending; a folded line; a field that is not a name, a colon and a value; no
//!   `Content-Length`, two of them, or one that is not digits alone; any
//!   `Transfer-Encoding`, `chunked` included (chunked bodies are not handled); a
//!   `Content-Encoding` but `identity`; and a status the client does not act on. A
//!   connection that ends before the stated length is cut. Trailers cannot come: there
//!   is no chunked body.
//! - The statuses it acts on: `200`, whose body goes to the sink; `301`, `302`, `303`,
//!   `307` and `308`, whose one `Location`, a whole `http` or `https` address or a path
//!   at the same origin, comes back as
//!   [`Downloaded::Moved`](whizbang_core::egress::Downloaded); `401` and `403`, a
//!   credential's fault; `404` and `410`, refused; `408`, `429`, `500`, `502`, `503` and
//!   `504`, unavailable. Every other status is refused.
//! - A body is handed to the sink as it comes and stopped at the bytes the request
//!   asked for, or its stated length if that is less: never more is read.
//! - **A send** acts on every final status alike, handing back its status and its
//!   body, of its stated length and at most `REPLY_MAX_BYTES` (16 MiB): what a status
//!   means is the adapter's to say. A redirect's `Location` comes back as an address,
//!   not asked. Its wait for the answer is [`ANSWER_WAIT_LIMIT`], since a model
//!   answers once it has written the whole turn, and no later than the run's
//!   deadline; and it has no least rate.
//!
//! **TLS.** 1.3 and 1.2, nothing older. The server's certificate is checked by `rustls`
//! against the roots compiled in from `webpki-roots` and against the host of the
//! address the gate let through. There is no way to turn the check off, and the
//! platform's verifier is not used: it may ask a revocation service nobody approved.
//! No session is resumed and nothing is sent early: each connection is a full
//! handshake with a full check.
//!
//! **Limits** (plan 1C, "Rules": the network). To connect, 10 s for all of a host's
//! addresses together ([`CONNECT_LIMIT`]); from one byte of the answer to the next, or
//! from the request's last byte to the answer's first, 30 s ([`BETWEEN_READS_LIMIT`]);
//! and a least rate, which the maker gives: fewer bytes of the answer than that over
//! any whole minute, counted in whole seconds, ends the download as timed out. Records
//! of TLS that hold none of the answer (tickets, key updates) count toward neither,
//! and more than 4,096 reads of them in a row end the download. The socket's own limit
//! is a short poll, at which the cancellation is looked at too, so a cancellation is
//! seen within a poll, and by the limit between two reads at the latest. A host's name
//! is resolved by the system, which has its own limits: that one wait is not bounded
//! here.
//!
//! **A credential** is copied once, into the request's bytes, which are overwritten
//! with zeros once they are written.
//!
//! **What a fault says of how far a request got.** Before the socket connects (an
//! address refused, a name not resolved, a cancellation, a connection refused or out of
//! time) nothing left the process, and the fault says so (`Fault::not_sent`). After it,
//! a failed handshake included, every fault is unresolved.
//!
//! **No thread.** The client makes none: one call is one socket, on the caller's
//! thread, closed when the call returns.

mod client;
mod http;
mod wire;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use whizbang_core::config::DOWNLOAD_LEAST_KIB_LIMIT;
use whizbang_core::contract::{Secrets, SystemClock};
use whizbang_core::egress::{CheckedDownloader, NotAllowed, Origin, SendCheck, Ways};

use client::Client;
use wire::Timing;

/// The limit to connect, for all of a host's addresses together.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(10);

/// The limit from one byte of the answer to the next, or from the request's last byte
/// to the answer's first.
pub const BETWEEN_READS_LIMIT: Duration = Duration::from_secs(30);

/// A send's limit from the request's last byte to the answer's first, and from one
/// byte of the answer to the next: a model answers once it has written its whole turn.
pub const ANSWER_WAIT_LIMIT: Duration = Duration::from_secs(600);

/// The socket's own limit for one call: how soon a cancellation is seen when nothing
/// comes.
const POLL: Duration = Duration::from_millis(250);

/// The download client, made inside a checked downloader held to `origins` (at most
/// `DOWNLOAD_ORIGINS_LIMIT`), with this least rate in kibibytes a second (1 to
/// `DOWNLOAD_LEAST_KIB_LIMIT`; the config's `download_least_kib_per_second`, ADR 0011),
/// and told the executor's secrets, which an address a server points to is refused
/// for holding. The one public way to the client.
pub fn downloader(
    least_kib_per_second: u64,
    origins: &[Origin],
    secrets: &Secrets,
) -> Result<CheckedDownloader, NotAllowed> {
    if least_kib_per_second == 0 {
        return Err(NotAllowed::Empty);
    }
    if least_kib_per_second > DOWNLOAD_LEAST_KIB_LIMIT {
        return Err(NotAllowed::TooMany);
    }
    let timing = Timing {
        connect: CONNECT_LIMIT,
        between_reads: BETWEEN_READS_LIMIT,
        least_bytes_per_second: least_kib_per_second * 1024,
        poll: POLL,
    };
    let client = Client::new(timing, Arc::new(SystemClock), secrets.clone());
    CheckedDownloader::new(Arc::new(client), origins)
}

/// The executor's ways out (ADR 0015, "A policy before a send, and a checked send"):
/// the client as their sender, with [`ANSWER_WAIT_LIMIT`] and no least rate, and the
/// check behind a send over it, `SendCheck::new().keeping_out(root)` with the root of
/// the store the executor writes to (ADR 0019, "For later tasks"): as an absolute
/// path, and its canonical spelling too when that differs and can be had. The ways
/// approve no recipient: the policy and the credentials are a snapshot's, which the
/// executor puts behind this sender and this check. The client is told the
/// executor's secrets, which an address a server points to is refused for holding.
/// The one public way to the client as a sender.
///
/// Refused: a root that cannot be made absolute, and one of nothing or of nothing but
/// slashes (`NotAllowed::Empty`).
pub fn ways(store_root: &Path, secrets: &Secrets) -> Result<Ways, NotAllowed> {
    let absolute = std::path::absolute(store_root).map_err(|_| NotAllowed::Empty)?;
    let mut check = SendCheck::new().keeping_out(&absolute)?;
    if let Ok(canonical) = std::fs::canonicalize(&absolute)
        && canonical != absolute
    {
        check = check.keeping_out(&canonical)?;
    }
    let timing = Timing {
        connect: CONNECT_LIMIT,
        between_reads: ANSWER_WAIT_LIMIT,
        least_bytes_per_second: 0,
        poll: POLL,
    };
    let client = Client::new(timing, Arc::new(SystemClock), secrets.clone());
    Ok(Ways::through(Arc::new(client)).checked_by(Arc::new(check)))
}
