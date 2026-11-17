//! A listener on loopback for a network client's tests (plan 1C, "Rules": the network).
//! It listens on `127.0.0.1` alone, at a port the system chooses, serves a fixed number
//! of connections one after another on one thread of its own, and counts them. Nothing
//! it does leaves the machine.
//!
//! Every wait is bounded. Its thread is made through [`threads`](crate::threads). Each
//! connection it serves has read and write limits ([`WAIT`]), so a connection whose
//! client says nothing ends. It is joined on every path: [`Listener::finish`] waits for
//! the connections it was told to serve; dropping it, on a return or in a panic, sets a
//! stop and wakes an accept that is blocked with a connection of its own to its own
//! port, which is not served and not counted, and joins the thread.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::threads;

/// The most connections one listener serves.
pub const CONNECTIONS_LIMIT: usize = 32;

/// How long a connection the listener serves waits for one read or one write, and how
/// long the stop's own connection waits to connect.
pub const WAIT: Duration = Duration::from_secs(5);

/// A listener on loopback, serving a fixed number of connections on its own thread.
/// What `{:?}` prints is its port and how many connections it has accepted.
pub struct Listener {
    address: SocketAddr,
    /// Held until the thread is joined, so the port is this listener's until then and
    /// the stop's connection reaches nothing else.
    _held: Arc<TcpListener>,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// A listener that accepts at most `connections` connections, from 1 to
    /// [`CONNECTIONS_LIMIT`], and hands each to `serve` in turn, on its thread, with its
    /// read and write limits set and its number among them, from 0. Panics when
    /// `connections` is out of its range or the port cannot be had, before any thread
    /// is made.
    pub fn serve<F>(connections: usize, mut serve: F) -> Listener
    where
        F: FnMut(usize, TcpStream) + Send + 'static,
    {
        assert!(
            (1..=CONNECTIONS_LIMIT).contains(&connections),
            "a listener serves 1 to {CONNECTIONS_LIMIT} connections"
        );
        let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap_or_else(|error| panic!("no port on loopback: {error}"));
        let address = socket
            .local_addr()
            .unwrap_or_else(|error| panic!("the listener has no address: {error}"));
        let socket = Arc::new(socket);
        let stop = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(AtomicUsize::new(0));
        let thread = {
            let (socket, stop, accepted) = (socket.clone(), stop.clone(), accepted.clone());
            threads::spawn(move || {
                for number in 0..connections {
                    let Ok((stream, _)) = socket.accept() else {
                        return;
                    };
                    // The stop's own connection is not served and not counted.
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    accepted.fetch_add(1, Ordering::SeqCst);
                    if limited(&stream).is_ok() {
                        serve(number, stream);
                    }
                }
            })
        };
        Listener {
            address,
            _held: socket,
            stop,
            accepted,
            thread: Some(thread),
        }
    }

    /// Where it listens: `127.0.0.1` and its port.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn port(&self) -> u16 {
        self.address.port()
    }

    /// How many connections it has accepted so far, the stop's own not counted.
    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    /// Stops it, joins its thread, and answers how many connections it accepted. A
    /// connection that was still waiting to be accepted when it stopped is neither
    /// served nor counted, so a test finishes it once its connections were answered.
    /// A panic of `serve`, such as a failed assertion, is a panic here.
    pub fn finish(mut self) -> usize {
        if let Err(panic) = self.stop_and_join() {
            std::panic::resume_unwind(panic);
        }
        self.accepted()
    }

    fn stop_and_join(&mut self) -> std::thread::Result<()> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        self.stop.store(true, Ordering::SeqCst);
        if !thread.is_finished() {
            // An accept that is blocked is woken by a connection of the listener's own.
            // The port is still this listener's (`_held`), so the
            // connection reaches nothing else; when the thread is already past its
            // last accept, it waits in the queue and is closed with the socket.
            let _ = TcpStream::connect_timeout(&self.address, WAIT);
        }
        thread.join()
    }
}

/// Sets a served connection's limits.
fn limited(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(WAIT))?;
    stream.set_write_timeout(Some(WAIT))
}

impl Drop for Listener {
    fn drop(&mut self) {
        // A panic of `serve` is not raised a second time while a test unwinds.
        let _ = self.stop_and_join();
    }
}

impl std::fmt::Debug for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Listener(port {}, {} accepted)",
            self.address.port(),
            self.accepted()
        )
    }
}
