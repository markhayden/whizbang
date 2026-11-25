//! The client: one request over TLS to one address, answered once ([`super`]). It
//! implements [`Downloader`], a `GET`, and [`Sender`], a `POST`, and follows nothing:
//! an answer that points elsewhere is handed back as [`Downloaded::Moved`] or
//! [`Reply::moved`], for the gate around it to ask its questions of. It is not public:
//! [`super::downloader`] makes it inside a
//! [`CheckedDownloader`](whizbang_core::egress::CheckedDownloader), [`super::ways`]
//! makes it the sender of ways out with the check behind a send over it, and nothing
//! else outside this crate can make one.

use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::Resumption;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use whizbang_core::capability::{Bounds, Fault, FaultKind};
use whizbang_core::contract::{Cancellation, Clock, Secrets};
use whizbang_core::egress::Dispatched;
use whizbang_core::egress::{
    Address, CheckedFrames, Control, DownloadRequest, Downloaded, Downloader, Head, NotAllowed,
    Outgoing, PROBE_BODY_MAX_BYTES, ProbeRequest, Probed, Prober, REPLY_MAX_BYTES, Reply, Sender,
    Sent, Take,
};

use super::http;
use super::wire::{CANCELLED, Timing, Wire};

/// The most addresses of a host that are tried.
pub(crate) const ADDRESSES_LIMIT: usize = 8;

/// The bytes read from the connection at once, and so the most of one piece.
pub(crate) const PIECE_MAX_BYTES: usize = 64 * 1024;

/// How a socket is connected: to one address, within a limit. The real one is
/// `TcpStream::connect_timeout`; a test may hand another.
pub(crate) type Dial = Arc<dyn Fn(&SocketAddr, Duration) -> io::Result<TcpStream> + Send + Sync>;

/// How a host's name is resolved to its addresses. The real one is the system's
/// (`ToSocketAddrs`); a test may hand another, to show what does not resolve.
pub(crate) type Resolve = Arc<dyn Fn(&str, u16) -> io::Result<Vec<SocketAddr>> + Send + Sync>;

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
    resolve: Resolve,
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

fn real_resolve() -> Resolve {
    Arc::new(|host: &str, port: u16| {
        Ok((host, port)
            .to_socket_addrs()?
            .take(ADDRESSES_LIMIT)
            .collect())
    })
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
            resolve: real_resolve(),
        }
    }

    /// A client that trusts these roots and no others, and connects by `dial`: for the
    /// tests, which hand it a root made in the test. It is compiled into the crate's
    /// tests and, under the feature `fakes`, into the tests of the crates that turn it
    /// on ([`super::testing`]), and nowhere else: the binary that ships is built
    /// without `fakes`, so no source of it can call this.
    #[cfg(any(test, feature = "fakes"))]
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
            resolve: real_resolve(),
        }
    }

    /// The same, resolving names by `resolve`: for the tests that show what a control
    /// send never asks of the resolver.
    #[cfg(any(test, feature = "fakes"))]
    pub(crate) fn resolving(mut self, resolve: Resolve) -> Client {
        self.resolve = resolve;
        self
    }

    /// Connects to one of the host's addresses, within the limit to connect for them
    /// all. Every fault here says that nothing was sent.
    fn connect(
        &self,
        address: &Address,
        timing: Timing,
        cancellation: &Cancellation,
    ) -> Result<TcpStream, Fault> {
        let origin = address.origin();
        let not_sent = |kind, why| Fault::new(kind, why).not_sent();
        let addresses: Vec<SocketAddr> = (self.resolve)(origin.host(), origin.port())
            .map_err(|_| not_sent(FaultKind::Unavailable, UNRESOLVED))?
            .into_iter()
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
            let Some(left) = timing
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

/// An answer's head, read, and the bytes of the body that came with it.
struct Answered<'a> {
    wire: Wire<'a>,
    head: http::Head,
    /// The bytes after the head that came with it: the start of the body.
    rest: Vec<u8>,
}

impl Client {
    /// One request to one address: the checks before anything leaves, the connection,
    /// the handshake, the request, and the head of the answer. `request` is written
    /// whole and then overwritten with zeros before it is let go, since it may hold a
    /// credential's value (`http::request` and `http::post` make it in one
    /// allocation, so no other copy of it is left behind).
    fn exchange<'a>(
        &'a self,
        address: &Address,
        request: Vec<u8>,
        timing: Timing,
        cancellation: &'a Cancellation,
    ) -> Result<Answered<'a>, Fault> {
        let mut request = Zeroed(request);
        let tls = self.before_connecting(address, cancellation)?;
        let socket = self.connect(address, timing, cancellation)?;

        // From here a byte may have left: every fault is unresolved.
        let mut wire = Wire::new(socket, tls, &*self.clock, timing, cancellation)?;
        wire.handshake()?;
        wire.send(&request.0)?;
        request.0.fill(0);
        let (head, rest) = answer_head(&mut wire)?;
        Ok(Answered { wire, head, rest })
    }

    /// What is looked at before the socket connects, when nothing has left, so each
    /// fault says so: the address is `https`, the request is not cancelled, the TLS was
    /// set up, and the host is a name a certificate is checked against. The connection's
    /// TLS, for that name and nothing else.
    fn before_connecting(
        &self,
        address: &Address,
        cancellation: &Cancellation,
    ) -> Result<ClientConnection, Fault> {
        let origin = address.origin();
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
        ClientConnection::new(config.clone(), name)
            .map_err(|_| Fault::new(FaultKind::Unavailable, NO_TLS).not_sent())
    }
}

/// The head of an answer, read off the wire up to its empty line, and the bytes of the
/// body that came with it.
fn answer_head(wire: &mut Wire<'_>) -> Result<(http::Head, Vec<u8>), Fault> {
    {
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
        let rest = head.split_off(end);
        let head = http::head(&head)?;
        Ok((head, rest))
    }
}

/// A request's bytes, overwritten with zeros when they are let go, on every path: they
/// may hold a credential's value.
struct Zeroed(Vec<u8>);

impl Drop for Zeroed {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&mut self.0);
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
        let written = http::request(address, request.credential());
        let Answered {
            mut wire,
            head: answer,
            rest,
        } = self.exchange(address, written, self.timing, cancellation)?;
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
        let mut buffer = vec![0; PIECE_MAX_BYTES];
        let mut handed: u64 = 0;
        let mut piece = rest;
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

/// How long past a request of frames' own wait its whole answer may last: the host ends
/// a request at its limit and answers it within seconds (the pack's front, 5), so this is
/// what is left for the bytes to come.
pub(crate) const WHOLE_ANSWER_GRACE: std::time::Duration = std::time::Duration::from_secs(30);

/// What the sender says of an answer whose body is over [`REPLY_MAX_BYTES`].
pub(crate) const REPLY_TOO_LARGE: &str = "the answer's body is over the most a reply holds";
/// What it says of a send whose run's deadline has passed before anything left.
pub(crate) const PAST_DEADLINE: &str = "the run's deadline passed before the request was sent";

impl Sender for Client {
    /// One `POST` of the request's pieces to its address ([`http::post`]), answered
    /// once: the answer's status and its body, of the length its one
    /// `Content-Length` states or in chunks ([`http::Chunks`]), at most
    /// [`REPLY_MAX_BYTES`], whatever the status;
    /// or, for `301`, `302`, `303`, `307` and `308`, the address its one `Location`
    /// names, which is not asked (ADR 0019, "Every hop": nothing is followed by the
    /// sender). The limit between two reads is the sender's; the whole request, its
    /// connection and every byte of its answer, is over by the run's deadline; the
    /// cancellation is the run's.
    fn send(&self, outgoing: &Outgoing, bounds: &Bounds) -> Result<Reply, Fault> {
        let mut timing = self.timing;
        if let Some(deadline) = bounds.deadline() {
            let left = deadline.saturating_duration_since(self.clock.now());
            if left.is_zero() {
                return Err(Fault::new(FaultKind::TimedOut, PAST_DEADLINE).not_sent());
            }
            timing.connect = timing.connect.min(left);
            timing.between_reads = timing.between_reads.min(left);
            // And the whole of it, however the answer's bytes come.
            timing.until = Some(self.clock.now() + left);
        }
        let address = outgoing.to();
        let written = http::post(outgoing);
        let cancellation = bounds.cancellation();
        let Answered {
            mut wire,
            head: answer,
            rest,
        } = self.exchange(address, written, timing, cancellation)?;
        if matches!(answer.status, 301 | 302 | 303 | 307 | 308) {
            let to = self.moved_to(address, answer.location()?)?;
            return Ok(Reply::moved(to));
        }
        let length = match answer.framing()? {
            http::Framing::Length(length) => length,
            http::Framing::Chunked => {
                let mut more = |buffer: &mut [u8]| wire.read(buffer);
                let body = http::Chunks::new(rest, &mut more).body(REPLY_MAX_BYTES)?;
                return Reply::answered(answer.status, body)
                    .map_err(|_| Fault::new(FaultKind::TooLarge, REPLY_TOO_LARGE).unresolved());
            }
        };
        let Some(length) = usize::try_from(length)
            .ok()
            .filter(|length| *length <= REPLY_MAX_BYTES)
        else {
            return Err(Fault::new(FaultKind::TooLarge, REPLY_TOO_LARGE).unresolved());
        };
        let mut body = rest;
        body.truncate(length);
        let mut buffer = vec![0; PIECE_MAX_BYTES];
        while body.len() < length {
            let read = wire.read(&mut buffer)?;
            if read == 0 {
                return Err(Fault::new(FaultKind::Malformed, super::wire::CUT).unresolved());
            }
            let room = length - body.len();
            body.extend_from_slice(&buffer[..read.min(room)]);
        }
        Reply::answered(answer.status, body)
            .map_err(|_| Fault::new(FaultKind::TooLarge, REPLY_TOO_LARGE).unresolved())
    }

    /// One request of frames (ADR 0046, "The kind `remote`"): a `POST` of the checked
    /// bytes with the request's fields and its credential, then its answer's head and a
    /// reader of its body, a piece at a time, handed to `take`. The connect, the
    /// handshake and the writing of the body keep the client's own limits
    /// ([`super::CONNECT_LIMIT`], [`super::BETWEEN_READS_LIMIT`]); the wait for the
    /// answer's first byte and between two of its bytes is the request's own
    /// ([`CheckedFrames::answer_within`]), the whole answer is over by that wait and
    /// [`WHOLE_ANSWER_GRACE`] from the request's start however it is kept alive, and
    /// nothing is waited for past the run's deadline. Nothing is followed: a redirect is an answer like any other, whose
    /// head `take` is handed. Once the socket connected, what it reached is named, so
    /// that a control send can follow the request.
    fn stream(&self, frames: &CheckedFrames, bounds: &Bounds, take: &mut Take<'_>) -> Sent {
        let now = self.clock.now();
        let mut answer_within = frames.answer_within();
        let mut timing = self.timing;
        timing.between_reads = super::BETWEEN_READS_LIMIT;
        // The whole answer is over by its own wait and a grace, whatever keeps it alive in
        // between: a host's keep-alives reset the wait between two reads, never this.
        timing.until = now.checked_add(answer_within.saturating_add(WHOLE_ANSWER_GRACE));
        if let Some(deadline) = bounds.deadline() {
            let left = deadline.saturating_duration_since(now);
            if left.is_zero() {
                return Sent::refused(Fault::new(FaultKind::TimedOut, PAST_DEADLINE).not_sent());
            }
            timing.connect = timing.connect.min(left);
            timing.between_reads = timing.between_reads.min(left);
            answer_within = answer_within.min(left);
            timing.until = Some(
                timing
                    .until
                    .map_or(now + left, |until| until.min(now + left)),
            );
        }
        let address = frames.to();
        let fields: Vec<(&str, &str)> = frames
            .fields()
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let request = Zeroed(http::post_bytes(
            address,
            &fields,
            frames.credential(),
            frames.bytes(),
        ));
        let cancellation = bounds.cancellation();
        let tls = match self.before_connecting(address, cancellation) {
            Ok(tls) => tls,
            Err(fault) => return Sent::refused(fault),
        };
        let socket = match self.connect(address, timing, cancellation) {
            Ok(socket) => socket,
            Err(fault) => return Sent::refused(fault),
        };
        let dispatched = socket
            .peer_addr()
            .ok()
            .map(|peer| Dispatched::reached(address.origin().clone(), peer));
        let mut left = false;
        let ended = self.streamed(
            socket,
            tls,
            timing,
            answer_within,
            request,
            cancellation,
            take,
            &mut left,
        );
        // A connection that failed before a byte of the request was written to it, in its
        // handshake say, handed the host nothing: its fault says nothing was sent.
        let ended = match ended {
            Err(fault) if !left => Err(Fault::new(fault.kind(), fault.sentence()).not_sent()),
            ended => ended,
        };
        Sent {
            dispatched,
            left,
            ended,
        }
    }

    /// A control send, to the address the request it follows reached: no name is
    /// resolved. The certificate is checked against the host of the control's own
    /// address, which is the request's origin. Its bounds' deadline holds from the
    /// connect to the last byte. The status of its answer, whose body is not read.
    fn control(
        &self,
        dispatched: &Dispatched,
        control: &Control,
        bounds: &Bounds,
    ) -> Result<u16, Fault> {
        let address = control.to();
        if address.origin() != dispatched.origin() {
            return Err(Fault::new(FaultKind::Rejected, ELSEWHERE).not_sent());
        }
        let now = self.clock.now();
        let Some(deadline) = bounds.deadline() else {
            return Err(Fault::new(FaultKind::Rejected, NO_DEADLINE).not_sent());
        };
        let left = deadline.saturating_duration_since(now);
        if left.is_zero() {
            return Err(Fault::new(FaultKind::TimedOut, PAST_DEADLINE).not_sent());
        }
        let timing = Timing {
            connect: self.timing.connect.min(left),
            between_reads: left,
            least_bytes_per_second: 0,
            poll: self.timing.poll,
            until: Some(now + left),
        };
        let fields: Vec<(&str, &str)> = control
            .fields()
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let mut request = Zeroed(http::post_bytes(
            address,
            &fields,
            control.credential(),
            control.body(),
        ));
        let cancellation = bounds.cancellation();
        let tls = self.before_connecting(address, cancellation)?;
        let socket = (self.dial)(&dispatched.peer(), timing.connect).map_err(|error| {
            let kind = match error.kind() {
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => FaultKind::TimedOut,
                _ => FaultKind::Unavailable,
            };
            Fault::new(kind, NOT_CONNECTED).not_sent()
        })?;
        let mut wire = Wire::new(socket, tls, &*self.clock, timing, cancellation)?;
        wire.handshake()?;
        wire.send(&request.0)?;
        request.0.fill(0);
        let (head, _) = answer_head(&mut wire)?;
        Ok(head.status)
    }
}

/// What a control send answers when its address is at another origin than the request
/// it follows.
pub(crate) const ELSEWHERE: &str = "a control send goes only to the origin of the request it \
                                    follows";
/// What it answers when it was given no deadline: its bounds are made for it.
pub(crate) const NO_DEADLINE: &str = "a control send is made under bounds of its own, with a \
                                      deadline, and these have none";

impl Client {
    /// The rest of a request of frames, once its socket connected: the handshake, the
    /// request, the answer's head and its body, handed to `take`. A fault of the wire
    /// under `take` is the request's end, whatever `take` answered. `left` is set once a
    /// byte of the request was written to the socket.
    #[allow(clippy::too_many_arguments)]
    fn streamed(
        &self,
        socket: TcpStream,
        tls: ClientConnection,
        timing: Timing,
        answer_within: Duration,
        mut request: Zeroed,
        cancellation: &Cancellation,
        take: &mut Take<'_>,
        left: &mut bool,
    ) -> Result<(), Fault> {
        let mut wire = Wire::new(socket, tls, &*self.clock, timing, cancellation)?;
        wire.handshake()?;
        let sent = wire.send(&request.0);
        *left = wire.request_left();
        sent?;
        request.0.fill(0);
        wire.waiting_for_answers(answer_within);
        let (head, rest) = answer_head(&mut wire)?;
        let framing = match head.framing() {
            Ok(framing) => framing,
            // An answer whose body is framed in no way the client reads is handed on
            // with no body: its head is what its caller acts on.
            Err(_) => http::Framing::Length(0),
        };
        let handed = Head::new(head.status, head.fields().to_vec());
        let mut body = BodyReader {
            wire: &mut wire,
            framing: match framing {
                http::Framing::Chunked => Framing::Chunked(http::Dechunk::new(rest)),
                http::Framing::Length(length) => Framing::Length {
                    held: rest,
                    at: 0,
                    left: length,
                },
            },
            fault: None,
        };
        let taken = take(&handed, &mut body);
        match body.fault.take() {
            Some(fault) => Err(fault),
            None => taken,
        }
    }
}

/// How a body being read is framed.
enum Framing {
    Chunked(http::Dechunk),
    Length { held: Vec<u8>, at: usize, left: u64 },
}

/// The body of an answer to a request of frames, read off the wire as its reader asks:
/// never ahead of it. A fault of the wire is kept, and the read answers an error that
/// says nothing more; the request then ends in the fault kept.
struct BodyReader<'w, 'a> {
    wire: &'w mut Wire<'a>,
    framing: Framing,
    fault: Option<Fault>,
}

impl io::Read for BodyReader<'_, '_> {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        if self.fault.is_some() {
            return Err(io::Error::other("the connection failed"));
        }
        let wire = &mut *self.wire;
        let read = match &mut self.framing {
            Framing::Chunked(chunks) => {
                let mut more = |buffer: &mut [u8]| wire.read(buffer);
                chunks.read(into, &mut more)
            }
            Framing::Length { held, at, left } => {
                if *left == 0 || into.is_empty() {
                    Ok(0)
                } else if *at < held.len() {
                    let most = usize::try_from(*left).unwrap_or(usize::MAX);
                    let here = (held.len() - *at).min(into.len()).min(most);
                    into[..here].copy_from_slice(&held[*at..*at + here]);
                    *at += here;
                    *left -= here as u64;
                    Ok(here)
                } else {
                    let most = usize::try_from(*left).unwrap_or(usize::MAX).min(into.len());
                    match wire.read(&mut into[..most]) {
                        Ok(0) => {
                            Err(Fault::new(FaultKind::Malformed, super::wire::CUT).unresolved())
                        }
                        Ok(read) => {
                            *left -= read as u64;
                            Ok(read)
                        }
                        Err(fault) => Err(fault),
                    }
                }
            }
        };
        read.map_err(|fault| {
            self.fault = Some(fault);
            io::Error::other("the connection failed")
        })
    }
}

impl Prober for Client {
    /// A probe: a `GET` of the address with no credential, no body and no user data,
    /// answered with its status and the start of its body, at most one byte more than
    /// [`PROBE_BODY_MAX_BYTES`], which the checked prober refuses whole. Nothing is
    /// followed: an answer that points elsewhere comes back as the address, not asked.
    fn probe(&self, request: &ProbeRequest, cancellation: &Cancellation) -> Result<Probed, Fault> {
        if request.credential().is_some() {
            return Err(Fault::new(FaultKind::Rejected, PROBE_CREDENTIAL).not_sent());
        }
        let address = request.to();
        let written = http::request(address, None);
        let Answered {
            mut wire,
            head,
            rest,
        } = self.exchange(address, written, self.timing, cancellation)?;
        if matches!(head.status, 301 | 302 | 303 | 307 | 308) {
            return Ok(Probed::Moved(self.moved_to(address, head.location()?)?));
        }
        let most = PROBE_BODY_MAX_BYTES + 1;
        let mut body = Vec::new();
        let mut buffer = vec![0; PIECE_MAX_BYTES];
        match head.framing() {
            Ok(http::Framing::Length(length)) => {
                let wanted = usize::try_from(length).unwrap_or(usize::MAX).min(most);
                body = rest;
                body.truncate(wanted);
                while body.len() < wanted {
                    let read = wire.read(&mut buffer)?;
                    if read == 0 {
                        return Err(Fault::new(FaultKind::Malformed, super::wire::CUT).unresolved());
                    }
                    let room = wanted - body.len();
                    body.extend_from_slice(&buffer[..read.min(room)]);
                }
            }
            Ok(http::Framing::Chunked) => {
                let mut chunks = http::Dechunk::new(rest);
                let mut more = |into: &mut [u8]| wire.read(into);
                while body.len() < most {
                    let room = most - body.len();
                    let read = chunks.read(&mut buffer[..room.min(PIECE_MAX_BYTES)], &mut more)?;
                    if read == 0 {
                        break;
                    }
                    body.extend_from_slice(&buffer[..read]);
                }
            }
            // A head that frames no body the client reads is reached with none.
            Err(_) => {}
        }
        Ok(Probed::Reached {
            status: head.status,
            body,
        })
    }
}

/// What a probe that came with a credential is refused with: a probe carries none.
pub(crate) const PROBE_CREDENTIAL: &str = "a probe carries no credential, and this one came \
                                           with one";
