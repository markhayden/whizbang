//! The network (`SPEC.md` section 10, "Egress"; plan 1C, task N1; ADR 0024): the
//! download client behind the checked downloader.
//!
//! **The one way to it** is [`downloader`], which answers a
//! [`CheckedDownloader`](whizbang_core::egress::CheckedDownloader) held to the origins it
//! is handed (ADR 0019, "A download and a probe"). The client itself is not public, so
//! nothing outside this crate can make one bare and ask an address the gate did not.
//! The client follows nothing: an answer that points elsewhere comes back to the gate
//! as an address, and the gate asks its questions of it before anything is asked there.
//!
//! **A narrow HTTP/1.1, written here, that refuses what it does not handle.** Over TLS,
//! with the crate `rustls`, and nothing else beside it:
//!
//! - `https` only. A request to an `http` address is refused before anything is sent.
//! - `GET` only, one request on one connection, which the client closes after its
//!   answer (`Connection: close`): no connection is kept or used again.
//! - The fields sent are fixed: `Host`, `User-Agent: whizbang`, `Accept: */*`,
//!   `Accept-Encoding: identity`, `Connection: close`, and the request's credential, in
//!   its own field, when it carries one. Nothing asks for compression.
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

use std::sync::Arc;
use std::time::Duration;

use whizbang_core::config::DOWNLOAD_LEAST_KIB_LIMIT;
use whizbang_core::contract::{Secrets, SystemClock};
use whizbang_core::egress::{CheckedDownloader, NotAllowed, Origin};

use client::Client;
use wire::Timing;

/// The limit to connect, for all of a host's addresses together.
pub const CONNECT_LIMIT: Duration = Duration::from_secs(10);

/// The limit from one byte of the answer to the next, or from the request's last byte
/// to the answer's first.
pub const BETWEEN_READS_LIMIT: Duration = Duration::from_secs(30);

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
