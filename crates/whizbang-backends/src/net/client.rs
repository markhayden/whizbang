//! The download client: one `GET` over TLS to one address, answered once ([`super`]).
//! It implements [`Downloader`] and follows nothing: an answer that points elsewhere is
//! handed back as [`Downloaded::Moved`], for the checked downloader around it to ask
//! its questions of. It is not public: [`super::downloader`] makes it inside a
//! [`CheckedDownloader`](whizbang_core::egress::CheckedDownloader), and nothing else
//! outside this crate can make one.

use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::Resumption;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::{Cancellation, Clock, Secrets};
use whizbang_core::egress::{Address, DownloadRequest, Downloaded, Downloader, NotAllowed};

use super::http;
use super::wire::{CANCELLED, Timing, Wire};

/// The most addresses of a host that are tried.
pub(crate) const ADDRESSES_LIMIT: usize = 8;

/// The bytes read from the connection at once, and so the most of one piece.
pub(crate) const PIECE_MAX_BYTES: usize = 64 * 1024;

/// How a socket is connected: to one address, within a limit. The real one is
/// `TcpStream::connect_timeout`; a test may hand another.
pub(crate) type Dial = Arc<dyn Fn(&SocketAddr, Duration) -> io::Result<TcpStream> + Send + Sync>;

pub(crate) const NOT_HTTPS: &str = "the address is not https, and the client speaks https only";
pub(crate) const NO_NAME: &str = "the address's host is not a name a certificate can be checked \
                                  against";
pub(crate) const NO_TLS: &str = "the client's TLS could not be set up";
pub(crate) const UNRESOLVED: &str = "the address's host could not be resolved to an address";
pub(crate) const CONNECT_TIMED_OUT: &str = "the server could not be connected to within the \
                                            limit to connect";
pub(crate) const NOT_CONNECTED: &str = "the server could not be connected to";
pub(crate) const CREDENTIAL: &str = "the server refused the request, which it answered with 401 \
                                     or 403: the credential is missing, wrong, or not granted \
                                     access";
pub(crate) const NOT_THERE: &str = "the server has no file at the address: 404 or 410";
pub(crate) const BUSY: &str = "the server could not answer now: 408, 429 or a 5xx";
pub(crate) const STATUS: &str = "the answer's status is one the download client does not act on";
pub(crate) const NOT_AN_ADDRESS: &str = "the answer points elsewhere, to an address the client \
                                         does not take";
pub(crate) const MOVED_SECRET: &str = "the answer points elsewhere, to an address that holds a \
                                       secret, and is not shown";

/// The client. What `{:?}` prints is its limits.
pub(crate) struct Client {
    /// None when TLS could not be set up, which every download then says.
    tls: Option<Arc<ClientConfig>>,
    timing: Timing,
    clock: Arc<dyn Clock>,
    secrets: Secrets,
    dial: Dial,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Client({:?})", self.timing)
    }
}

/// The client's TLS: TLS 1.3 and 1.2, the roots it is handed and no other, no session
/// resumed and no early data, and `http/1.1` the one protocol it offers.
fn tls(roots: RootCertStore) -> Option<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .ok()?;
    let mut config = builder.with_root_certificates(roots).with_no_client_auth();
    config.resumption = Resumption::disabled();
    config.enable_early_data = false;
    config.enable_sni = true;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Some(Arc::new(config))
}

/// The roots compiled in: Mozilla's list, as `webpki-roots` holds it.
fn compiled_roots() -> RootCertStore {
    RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    }
}

fn real_dial() -> Dial {
    Arc::new(TcpStream::connect_timeout)
}

impl Client {
    /// The client a download is made with: the compiled roots, the machine's clock, and
    /// the socket's own connect.
    pub(crate) fn new(timing: Timing, clock: Arc<dyn Clock>, secrets: Secrets) -> Client {
        Client {
            tls: tls(compiled_roots()),
            timing,
            clock,
            secrets,
            dial: real_dial(),
        }
    }

    /// A client that trusts these roots and no others, and connects by `dial`: for the
    /// crate's own tests, which hand it a root made in the test. It is compiled into
    /// the crate's tests and nowhere else, so no other code, the binary's included, can
    /// call it: a build that is not of these tests has no such function.
    #[cfg(test)]
    pub(crate) fn trusting(
        roots: RootCertStore,
        timing: Timing,
        clock: Arc<dyn Clock>,
        secrets: Secrets,
        dial: Option<Dial>,
    ) -> Client {
        Client {
            tls: tls(roots),
            timing,
            clock,
            secrets,
            dial: dial.unwrap_or_else(real_dial),
        }
    }

    /// Connects to one of the host's addresses, within the limit to connect for them
    /// all. Every fault here says that nothing was sent.
    fn connect(&self, address: &Address, cancellation: &Cancellation) -> Result<TcpStream, Fault> {
        let origin = address.origin();
        let not_sent = |kind, why| Fault::new(kind, why).not_sent();
        let addresses: Vec<SocketAddr> = (origin.host(), origin.port())
            .to_socket_addrs()
            .map_err(|_| not_sent(FaultKind::Unavailable, UNRESOLVED))?
            .take(ADDRESSES_LIMIT)
            .collect();
        if addresses.is_empty() {
            return Err(not_sent(FaultKind::Unavailable, UNRESOLVED));
        }
        let start = self.clock.now();
        let mut timed_out = false;
        for socket in &addresses {
            if cancellation.is_cancelled() {
                return Err(not_sent(FaultKind::Cancelled, CANCELLED));
            }
            let spent = self.clock.now().saturating_duration_since(start);
            let Some(left) = self
                .timing
                .connect
                .checked_sub(spent)
                .filter(|left| !left.is_zero())
            else {
                return Err(not_sent(FaultKind::TimedOut, CONNECT_TIMED_OUT));
            };
            match (self.dial)(socket, left) {
                Ok(stream) => return Ok(stream),
                Err(error) => {
                    timed_out = matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    );
                }
            }
        }
        if timed_out {
            Err(not_sent(FaultKind::TimedOut, CONNECT_TIMED_OUT))
        } else {
            Err(not_sent(FaultKind::Unavailable, NOT_CONNECTED))
        }
    }

    /// The address a redirect names: one whole, or a path at the same origin. Any other
    /// form is not taken.
    fn moved_to(&self, from: &Address, location: &str) -> Result<Address, Fault> {
        let scheme = location.split_once("://").map(|(scheme, _)| scheme);
        let text = match scheme {
            Some(scheme)
                if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") =>
            {
                location.to_owned()
            }
            None if location.starts_with('/') && !location.starts_with("//") => {
                format!("{}{location}", from.origin())
            }
            _ => return Err(Fault::new(FaultKind::Malformed, NOT_AN_ADDRESS).unresolved()),
        };
        Address::of(&text, &self.secrets).map_err(|refused| {
            let why = if refused == NotAllowed::Secret {
                MOVED_SECRET
            } else {
                NOT_AN_ADDRESS
            };
            Fault::new(FaultKind::Malformed, why).unresolved()
        })
    }
}

impl Downloader for Client {
    fn download(
        &self,
        request: &DownloadRequest,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        let address = request.from();
        let origin = address.origin();
        // Before the socket connects nothing has left, so each fault says so.
        if origin.scheme() != "https" {
            return Err(Fault::new(FaultKind::Rejected, NOT_HTTPS).not_sent());
        }
        if cancellation.is_cancelled() {
            return Err(Fault::new(FaultKind::Cancelled, CANCELLED).not_sent());
        }
        let Some(config) = &self.tls else {
            return Err(Fault::new(FaultKind::Unavailable, NO_TLS).not_sent());
        };
        // The certificate is checked against the host of the address the gate let
        // through, and nothing else.
        let Ok(name) = ServerName::try_from(origin.host().to_owned()) else {
            return Err(Fault::new(FaultKind::Rejected, NO_NAME).not_sent());
        };
        let Ok(tls) = ClientConnection::new(config.clone(), name) else {
            return Err(Fault::new(FaultKind::Unavailable, NO_TLS).not_sent());
        };
        let socket = self.connect(address, cancellation)?;

        // From here a byte may have left: every fault is unresolved.
        let mut wire = Wire::new(socket, tls, &*self.clock, self.timing, cancellation)?;
        wire.handshake()?;
        // The request holds the credential's value: it is written, and then its bytes
        // are overwritten with zeros before they are let go (`http::request` makes it
        // in one allocation, so no other copy of it is left behind).
        let mut written = http::request(address, request.credential());
        let sent = wire.send(&written);
        written.fill(0);
        std::hint::black_box(&mut written);
        drop(written);
        sent?;

        let mut buffer = vec![0; PIECE_MAX_BYTES];
        let mut head = Vec::new();
        // How much of the head was looked at for its end already.
        let mut scanned = 0;
        let end = loop {
            if let Some(end) = http::end_of_head(&head, scanned)? {
                break end;
            }
            let read = wire.read(&mut buffer)?;
            if read == 0 {
                return Err(Fault::new(FaultKind::Malformed, super::wire::CUT).unresolved());
            }
            scanned = head.len();
            head.extend_from_slice(&buffer[..read]);
        };
        let answer = http::head(&head[..end])?;
        let answered = |kind, why| Err(Fault::new(kind, why).unresolved());
        match answer.status {
            200 => {}
            301 | 302 | 303 | 307 | 308 => {
                let to = self.moved_to(address, answer.location()?)?;
                return Ok(Downloaded::Moved(to));
            }
            401 | 403 => return answered(FaultKind::Credential, CREDENTIAL),
            404 | 410 => return answered(FaultKind::Rejected, NOT_THERE),
            408 | 429 | 500 | 502 | 503 | 504 => return answered(FaultKind::Unavailable, BUSY),
            _ => return answered(FaultKind::Malformed, STATUS),
        }

        // The body: its stated length, and never more than the request asked for.
        let wanted = answer.length()?.min(request.most_bytes());
        let mut handed: u64 = 0;
        let mut piece = head.split_off(end);
        loop {
            let room = usize::try_from(wanted - handed).unwrap_or(usize::MAX);
            piece.truncate(room);
            if !piece.is_empty() {
                handed += piece.len() as u64;
                if !sink(&piece) {
                    return Ok(Downloaded::Bytes(handed));
                }
            }
            if handed == wanted {
                return Ok(Downloaded::Bytes(handed));
            }
            // Between two pieces.
            if cancellation.is_cancelled() {
                return Err(Fault::new(FaultKind::Cancelled, CANCELLED).unresolved());
            }
            let read = wire.read(&mut buffer)?;
            if read == 0 {
                return Err(Fault::new(FaultKind::Malformed, super::wire::CUT).unresolved());
            }
            piece.clear();
            piece.extend_from_slice(&buffer[..read]);
        }
    }
}
