//! One connection, after its socket connected: TLS over it, and the limits that bound
//! every wait on it ([`super`], "Limits").
//!
//! The socket's own read and write limits are a short poll, so that no call blocks
//! longer than that; each time one returns with nothing, the cancellation, the limit
//! between two reads and the least rate are looked at, by the injected clock. A count of
//! polls with nothing beside the clock ends the wait too, so a clock that stands still,
//! as a test's may, cannot make it endless.
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
    /// From one byte to the next, either way.
    pub(crate) between_reads: Duration,
    /// The least bytes a second, over any whole minute.
    pub(crate) least_bytes_per_second: u64,
    /// The socket's own limit for one call.
    pub(crate) poll: Duration,
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
pub(crate) const CUT: &str = "the connection ended before the answer was whole";
pub(crate) const BROKEN: &str = "the connection failed";
pub(crate) const UNTRUSTED: &str = "the server's certificate is not one the client trusts for \
                                    the address's host";
pub(crate) const TLS_FAILED: &str = "the TLS of the connection failed";

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
    /// When a byte last moved, either way.
    last: Instant,
    /// The polls in a row that moved nothing.
    quiet: u128,
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
        let quiet_for = now.saturating_duration_since(self.last);
        if quiet_for >= self.timing.between_reads || self.quiet > self.timing.polls_ceiling() {
            return Err(unresolved(FaultKind::TimedOut, STALLED));
        }
        if self.rate.under(now) {
            return Err(unresolved(FaultKind::TimedOut, TOO_SLOW));
        }
        Ok(())
    }

    fn moved(&mut self, received: u64) {
        let now = self.clock.now();
        self.last = now;
        self.quiet = 0;
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
                Ok(received) => {
                    self.moved(received as u64);
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

    /// Writes the request whole.
    pub(crate) fn send(&mut self, bytes: &[u8]) -> Result<(), Fault> {
        if self.tls.writer().write_all(bytes).is_err() {
            return Err(unresolved(FaultKind::Unavailable, BROKEN));
        }
        self.flush()
    }

    /// Reads some of the answer into `into`: how many bytes, and none only when the
    /// server closed its side as TLS says to. A connection that ends otherwise is cut.
    pub(crate) fn read(&mut self, into: &mut [u8]) -> Result<usize, Fault> {
        loop {
            match self.tls.reader().read(into) {
                Ok(read) => return Ok(read),
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
        };
        assert_eq!(timing.polls_ceiling(), 121);
    }
}
