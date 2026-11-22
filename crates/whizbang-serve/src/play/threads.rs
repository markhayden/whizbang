//! The threads of the playground host (ADR 0041, "Threads"): its acceptor, its sixteen
//! connection workers, its keeper, its canceller and its one install, and the binary's reader of the
//! terminal's signals and of its input.
//!
//! This is the fifth place in the workspace that makes a thread, beside the executor's
//! spawner, the spawner of the pack's supervisor, the stdio server's reader and the test
//! kit's helper (`clippy.toml`): a process holds at most [`THREADS_LIMIT`] made here at
//! once, and the one over it is not made.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;

/// How many threads made here one process holds at once: two hosts' twenty each (one
/// host's acceptor, sixteen workers, keeper, canceller and its one install, task M1),
/// and the binary's readers of its signals and its input and the waiter of a Chrome
/// started on Linux.
pub const THREADS_LIMIT: usize = 44;

static LIVE: AtomicUsize = AtomicUsize::new(0);

/// A place in the count, given back when the thread's work is over, by a return or a
/// panic, or when the thread could not be made.
struct Place;

impl Place {
    fn take() -> io::Result<Place> {
        let taken = |now: usize| (now < THREADS_LIMIT).then_some(now + 1);
        match LIVE.try_update(Ordering::SeqCst, Ordering::SeqCst, taken) {
            Ok(_) => Ok(Place),
            Err(_) => Err(io::Error::other(
                "the process holds as many threads of the playground host as it may",
            )),
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How many threads made here are live now.
pub fn live() -> usize {
    LIVE.load(Ordering::SeqCst)
}

/// A thread of the host, or of the binary that runs it, counted: refused, and nothing
/// made, when the process already holds [`THREADS_LIMIT`] made here. The lint against
/// making a thread is allowed here, and its bound is the count (`clippy.toml`).
#[allow(clippy::disallowed_methods)]
pub fn spawn(name: &str, work: Box<dyn FnOnce() + Send + 'static>) -> io::Result<JoinHandle<()>> {
    let place = Place::take()?;
    std::thread::Builder::new()
        .name(format!("whizbang-play-{name}"))
        .spawn(move || {
            let _place = place;
            work();
        })
}
