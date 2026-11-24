//! One connection, after its socket connected: TLS over it, and the limits that bound
//! every wait on it ([`super`], "Limits").
//!
//! The socket's own read and write limits are a short poll, so that no call blocks
//! longer than that; before each read, the cancellation, the limit between two reads
//! and the least rate are looked at, by the injected clock. A count of polls with
//! nothing beside the clock ends the wait too, so a clock that stands still, as a
//! test's may, cannot make it endless.
//!
//! **Only the answer counts.** The limit between two reads and the least rate count
//! the bytes of the answer, its head and its body, as TLS hands them over, and the
//! bytes of the request that leave: never records of TLS alone (a handshake's, a
//! session ticket's, a key update's), which a server whose certificate is good could
//! send for ever and so keep a download alive with none of the file.
//!
//! Every fault here is unresolved: the socket connected, so a byte of the request may
//! have left (`Fault::not_sent` is said only before that, in [`super::client`]).

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use rustls::ClientConnection;
use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::{Cancellation, Clock};

/// The span the least rate is held over: any whole minute, counted in whole seconds.
pub(crate) const RATE_WINDOW_SECONDS: u64 = 60;

/// How a connection's waits are bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timing {
    /// To connect, for all of a host's addresses together.
    pub(crate) connect: Duration,
    /// From one byte of the answer to the next, or from the request's last byte to the
    /// answer's first.
    pub(crate) between_reads: Duration,
    /// The least bytes a second, over any whole minute.
    pub(crate) least_bytes_per_second: u64,
    /// The socket's own limit for one call.
    pub(crate) poll: Duration,
    /// When the whole request must be over, by the injected clock, however its bytes
    /// come: a send's run's deadline. None for a download.
    pub(crate) until: Option<Instant>,
}

impl Timing {
    /// The polls with nothing in a row after which the limit between two reads is
    /// reached, whatever the clock says: each took one poll at least.
    fn polls_ceiling(&self) -> u128 {
        let poll = self.poll.as_millis().max(1);
        self.between_reads.as_millis().div_ceil(poll) + 1
    }
}

pub(crate) const CANCELLED: &str = "the download was cancelled";
pub(crate) const STALLED: &str = "nothing came from the server within the limit between two \
                                  reads";
pub(crate) const TOO_SLOW: &str = "the download kept under its least rate over a whole minute";
pub(crate) const PAST_DEADLINE: &str = "the run's deadline passed before the answer was whole";
pub(crate) const CUT: &str = "the connection ended before the answer was whole";
pub(crate) const BROKEN: &str = "the connection failed";
pub(crate) const UNTRUSTED: &str = "the server's certificate is not one the client trusts for \
                                    the address's host";
pub(crate) const TLS_FAILED: &str = "the TLS of the connection failed";
pub(crate) const TLS_ONLY: &str = "the server sent records of TLS and none of the answer, more \
                                   than 4096 times in a row";

/// The most reads off the socket in a row that bring no byte of the answer: far more
/// than a handshake and a head take. With the limit between two reads it ends a server
/// that sends records of TLS alone, under a clock that stands still too.
pub(crate) const UNANSWERED_LIMIT: u32 = 4096;

fn unresolved(kind: FaultKind, why: &'static str) -> Fault {
    Fault::new(kind, why).unresolved()
}

/// The bytes received in each of the last 61 seconds of a connection, by the clock: so
/// the bytes of any whole minute, counted in whole seconds, from the second 60 seconds
/// back to the one that is running.
#[derive(Debug, Clone)]
pub(crate) struct Rate {
    start: Instant,
    least_per_minute: u64,
    seconds: [u64; RATE_WINDOW_SECONDS as usize + 1],
    /// The second of the connection the last count went into.
    second: u64,
    /// The bytes in `seconds`.
    sum: u64,
}

impl Rate {
    pub(crate) fn new(start: Instant, least_bytes_per_second: u64) -> Rate {
        Rate {
            start,
            least_per_minute: least_bytes_per_second.saturating_mul(RATE_WINDOW_SECONDS),
            seconds: [0; RATE_WINDOW_SECONDS as usize + 1],
            second: 0,
            sum: 0,
        }
    }

    fn slot(second: u64) -> usize {
        // The remainder is under 61.
        (second % (RATE_WINDOW_SECONDS + 1)) as usize
    }

    /// Moves to the second `now` is in, emptying each second that falls out of the
    /// window: 61 at most, however far the clock moved.
    fn advance(&mut self, now: Instant) {
        let second = now.saturating_duration_since(self.start).as_secs();
        if second <= self.second {
            return;
        }
        if second - self.second > RATE_WINDOW_SECONDS {
            self.seconds = [0; RATE_WINDOW_SECONDS as usize + 1];
            self.sum = 0;
        } else {
            for passed in self.second + 1..=second {
                let slot = Rate::slot(passed);
                self.sum -= self.seconds[slot];
                self.seconds[slot] = 0;
            }
        }
        self.second = second;
    }

    pub(crate) fn received(&mut self, now: Instant, bytes: u64) {
        self.advance(now);
        let slot = Rate::slot(self.second);
        self.seconds[slot] = self.seconds[slot].saturating_add(bytes);
        self.sum = self.sum.saturating_add(bytes);
    }

    /// Whether a whole minute has passed and the bytes of the last one are fewer than
    /// the least rate asks.
    pub(crate) fn under(&mut self, now: Instant) -> bool {
        self.advance(now);
        self.second >= RATE_WINDOW_SECONDS && self.sum < self.least_per_minute
    }
}

/// A connection: its socket and its TLS, and what bounds each wait on it.
pub(crate) struct Wire<'a> {
    socket: TcpStream,
    tls: ClientConnection,
    clock: &'a dyn Clock,
    timing: Timing,
    cancellation: &'a Cancellation,
    /// When a byte of the answer last came, or a byte of the request last left.
    last: Instant,
    /// The polls in a row that brought nothing off the socket.
    quiet: u128,
    /// The reads off the socket since the last byte of the answer.
    unanswered: u32,
    rate: Rate,
    /// The socket's other side ended it.
    ended: bool,
}

impl<'a> Wire<'a> {
    pub(crate) fn new(
        socket: TcpStream,
        tls: ClientConnection,
        clock: &'a dyn Clock,
        timing: Timing,
        cancellation: &'a Cancellation,
    ) -> Result<Wire<'a>, Fault> {
        let poll = Some(timing.poll.max(Duration::from_millis(1)));
        let set = socket
            .set_read_timeout(poll)
            .and_then(|()| socket.set_write_timeout(poll))
            .and_then(|()| socket.set_nodelay(true));
        if set.is_err() {
            return Err(unresolved(FaultKind::Unavailable, BROKEN));
        }
        let now = clock.now();
        Ok(Wire {
            socket,
            tls,
            clock,
            timing,
            cancellation,
            last: now,
            quiet: 0,
            unanswered: 0,
            rate: Rate::new(now, timing.least_bytes_per_second),
            ended: false,
        })
    }

    /// The cancellation, the limit between two reads and the least rate, before each
    /// wait.
    fn look(&mut self) -> Result<(), Fault> {
        if self.cancellation.is_cancelled() {
            return Err(unresolved(FaultKind::Cancelled, CANCELLED));
        }
        let now = self.clock.now();
        if self.timing.until.is_some_and(|until| now >= until) {
            return Err(unresolved(FaultKind::TimedOut, PAST_DEADLINE));
        }
        let quiet_for = now.saturating_duration_since(self.last);
        if quiet_for >= self.timing.between_reads || self.quiet > self.timing.polls_ceiling() {
            return Err(unresolved(FaultKind::TimedOut, STALLED));
        }
        if self.rate.under(now) {
            return Err(unresolved(FaultKind::TimedOut, TOO_SLOW));
        }
        if self.unanswered > UNANSWERED_LIMIT {
            return Err(unresolved(FaultKind::Malformed, TLS_ONLY));
        }
        Ok(())
    }

    /// Progress: bytes of the answer, as TLS hands them over (`received`), or bytes of
    /// the request that left (`received` is then nothing). Records of TLS that hold
    /// none of the answer, a handshake's, a ticket's or a key update's, are not
    /// progress: a server could send them for ever.
    fn moved(&mut self, received: u64) {
        let now = self.clock.now();
        self.last = now;
        self.quiet = 0;
        self.unanswered = 0;
        if received > 0 {
            self.rate.received(now, received);
        }
    }

    /// Writes what TLS holds to be written.
    fn flush(&mut self) -> Result<(), Fault> {
        while self.tls.wants_write() {
            self.look()?;
            match self.tls.write_tls(&mut self.socket) {
                Ok(0) => return Err(unresolved(FaultKind::Unavailable, CUT)),
                Ok(_) => self.moved(0),
                Err(error) if waited(&error) => self.quiet += 1,
                Err(_) => return Err(unresolved(FaultKind::Unavailable, BROKEN)),
            }
        }
        Ok(())
    }

    /// Reads what the socket has, once, and hands it to TLS.
    fn pull(&mut self) -> Result<(), Fault> {
        loop {
            self.look()?;
            match self.tls.read_tls(&mut self.socket) {
                Ok(0) => {
                    self.ended = true;
                    return self.process();
                }
                Ok(_) => {
                    // Bytes off the socket, which are not yet bytes of the answer.
                    self.quiet = 0;
                    self.unanswered = self.unanswered.saturating_add(1);
                    return self.process();
                }
                Err(error) if waited(&error) => self.quiet += 1,
                Err(_) => return Err(unresolved(FaultKind::Unavailable, BROKEN)),
            }
        }
    }

    fn process(&mut self) -> Result<(), Fault> {
        match self.tls.process_new_packets() {
            Ok(_) => Ok(()),
            Err(rustls::Error::InvalidCertificate(_)) => {
                Err(unresolved(FaultKind::Rejected, UNTRUSTED))
            }
            Err(_) => Err(unresolved(FaultKind::Malformed, TLS_FAILED)),
        }
    }

    /// The handshake: the server's certificate is checked against the roots and the
    /// address's host here, before a byte of the request is written.
    pub(crate) fn handshake(&mut self) -> Result<(), Fault> {
        while self.tls.is_handshaking() {
            if self.tls.wants_write() {
                self.flush()?;
            } else if self.ended {
                return Err(unresolved(FaultKind::Unavailable, CUT));
            } else {
                self.pull()?;
            }
        }
        self.flush()
    }

    /// Writes the request whole. TLS takes a bounded amount of it at once (64 KiB by
    /// default), so a larger request is handed over in turns, what TLS holds written
    /// to the socket between them, each wait bounded as every other is.
    pub(crate) fn send(&mut self, bytes: &[u8]) -> Result<(), Fault> {
        let mut rest = bytes;
        while !rest.is_empty() {
            match self.tls.writer().write(rest) {
                Ok(0) => self.flush()?,
                Ok(taken) => rest = &rest[taken..],
                Err(_) => return Err(unresolved(FaultKind::Unavailable, BROKEN)),
            }
        }
        self.flush()
    }

    /// From here on, the wait from the request's last byte to the answer's first and
    /// between two bytes of the answer is `between`: a request of frames waits as long
    /// as its model may work (ADR 0046), once its own bytes have left within the
    /// client's limits.
    pub(crate) fn waiting_for_answers(&mut self, between: Duration) {
        self.timing.between_reads = between;
        self.last = self.clock.now();
        self.quiet = 0;
    }

    /// Reads some of the answer into `into`: how many bytes, and none only when the
    /// server closed its side as TLS says to. A connection that ends otherwise is cut.
    pub(crate) fn read(&mut self, into: &mut [u8]) -> Result<usize, Fault> {
        loop {
            match self.tls.reader().read(into) {
                Ok(read) => {
                    if read > 0 {
                        self.moved(read as u64);
                    }
                    return Ok(read);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if self.ended {
                        return Err(unresolved(FaultKind::Unavailable, CUT));
                    }
                    self.pull()?;
                }
                Err(_) => return Err(unresolved(FaultKind::Unavailable, CUT)),
            }
        }
    }
}

/// Whether a socket's call came back with nothing at its limit.
fn waited(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn the_least_rate_is_kept_by_a_minute_of_exactly_that_many_bytes_and_not_by_one_less() {
        let start = Instant::now();
        for (each_second, under) in [(1024, false), (1023, true)] {
            let mut rate = Rate::new(start, 1024);
            // Nothing is judged before a whole minute.
            for second in 1..RATE_WINDOW_SECONDS {
                rate.received(start + SECOND * u32::try_from(second).unwrap(), each_second);
                assert!(!rate.under(start + SECOND * u32::try_from(second).unwrap()));
            }
            rate.received(start + SECOND * 60, each_second);
            assert_eq!(
                rate.under(start + SECOND * 60),
                under,
                "{each_second} a second"
            );
        }
    }

    #[test]
    fn the_least_rate_is_held_over_any_whole_minute_and_not_over_the_whole_download() {
        let start = Instant::now();
        let mut rate = Rate::new(start, 1024);
        // A fast first minute banks nothing for the second.
        rate.received(start, 1024 * 600);
        assert!(!rate.under(start + SECOND * 60));
        rate.received(start + SECOND * 61, 1024 * 59);
        assert!(rate.under(start + SECOND * 61));
        // A clock that jumps far empties the window at once.
        let mut rate = Rate::new(start, 1);
        rate.received(start, 1_000_000);
        assert!(rate.under(start + SECOND * 10_000));
    }

    #[test]
    fn the_ceiling_of_quiet_polls_is_the_limit_between_two_reads_in_polls() {
        let timing = Timing {
            connect: SECOND,
            between_reads: Duration::from_millis(30_000),
            least_bytes_per_second: 1,
            poll: Duration::from_millis(250),
            until: None,
        };
        assert_eq!(timing.polls_ceiling(), 121);
    }
}
