//! The one thread the server makes: the reader of its input, which waits on the input
//! while the server's own loop looks after the runs it started (ADR 0031, "Threads").
//!
//! This is the fourth place in the workspace that makes a thread, beside the executor's
//! spawner, the spawner of the pack's supervisor and the test kit's helper
//! (`clippy.toml`): a process holds at most [`READERS_LIMIT`] made here at once, and the
//! one over it is not made. A server makes one, and it ends at the end of the input.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;

/// How many readers one process holds at once: one server's, and room for the reader
/// of a server that ended while its input stayed open.
pub const READERS_LIMIT: usize = 2;

static LIVE: AtomicUsize = AtomicUsize::new(0);

/// A place in the count, given back when the thread's work is over, by a return or a
/// panic, or when the thread could not be made.
struct Place;

impl Place {
    fn take() -> io::Result<Place> {
        let taken = |now: usize| (now < READERS_LIMIT).then_some(now + 1);
        match LIVE.try_update(Ordering::SeqCst, Ordering::SeqCst, taken) {
            Ok(_) => Ok(Place),
            Err(_) => Err(io::Error::other(
                "the process holds as many readers of a server's input as it may",
            )),
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The reader's thread, counted: refused, and nothing made, when the process already
/// holds [`READERS_LIMIT`] made here. The lint against making a thread is allowed here,
/// and its bound is the count (`clippy.toml`).
#[allow(clippy::disallowed_methods)]
pub(crate) fn spawn(work: Box<dyn FnOnce() + Send + 'static>) -> io::Result<JoinHandle<()>> {
    let place = Place::take()?;
    std::thread::Builder::new()
        .name("whizbang-serve-reader".to_owned())
        .spawn(move || {
            let _place = place;
            work();
        })
}
