//! The threads the supervisor makes for its worker: two, one that reads the worker's
//! output and one that reads its error output (plan 1C, "Rules", "Threads in whizbang").
//!
//! This is the third place in the workspace that makes a thread, beside the executor's
//! spawner and the test kit's helper (`clippy.toml`): a process holds at most
//! [`THREADS_LIMIT`] made here at once, which is one worker's two (D4: one worker in a
//! process), and the one over it is not made. A test hands the supervisor the test
//! kit's spawner in place of this one, so that its threads are counted with every other
//! of the test's (`whizbang_testkit::worker`).

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;

/// What makes a thread of the supervisor: it is given the thread's name and its work,
/// and hands back the thread to be joined, or why it was not made.
pub type Spawn = fn(String, Box<dyn FnOnce() + Send + 'static>) -> io::Result<JoinHandle<()>>;

/// How many threads made here one process holds at once: one worker's two.
pub const THREADS_LIMIT: usize = 2;

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
                "the process holds the threads of one worker already",
            )),
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A thread of the system with this name, counted: refused, and nothing made, when the
/// process already holds [`THREADS_LIMIT`] made here. The lint against making a thread
/// is allowed here, and its bound is the count (`clippy.toml`).
#[allow(clippy::disallowed_methods)]
pub fn spawn(name: String, work: Box<dyn FnOnce() + Send + 'static>) -> io::Result<JoinHandle<()>> {
    let place = Place::take()?;
    std::thread::Builder::new().name(name).spawn(move || {
        let _place = place;
        work();
    })
}
