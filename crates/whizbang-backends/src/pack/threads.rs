//! The threads the supervisor makes for its worker: three, one that reads the worker's
//! output, one that reads its error output, and one that writes its input, so that no
//! write waits outside the supervisor's clock (plan 1C, "Rules", "Threads in whizbang";
//! task P3b).
//!
//! This is the third place in the workspace that makes a thread, beside the executor's
//! spawner and the test kit's helper (`clippy.toml`): a process holds at most
//! [`THREADS_LIMIT`] made here at once, which is one worker's three (D4: one worker in a
//! process), and the one over it is not made. A test hands the supervisor the test
//! kit's spawner in place of this one, so that its threads are counted with every other
//! of the test's (`whizbang_testkit::worker`).

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;

/// What makes a thread of the supervisor: it is given the thread's name and its work,
/// and hands back the thread to be joined, or why it was not made.
pub type Spawn = fn(String, Box<dyn FnOnce() + Send + 'static>) -> io::Result<JoinHandle<()>>;

/// How many threads made here one process holds at once: one worker's three.
pub const THREADS_LIMIT: usize = 3;

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
pub(crate) fn spawn(
    name: String,
    work: Box<dyn FnOnce() + Send + 'static>,
) -> io::Result<JoinHandle<()>> {
    let place = Place::take()?;
    std::thread::Builder::new().name(name).spawn(move || {
        let _place = place;
        work();
    })
}

#[cfg(test)]
mod tests {
    //! The spawner itself: the one test that makes threads through it, never more than
    //! the two it allows, each parked on a channel of its own that this test ends.
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    use super::*;

    /// A thread parked until its channel is sent to or dropped.
    fn parked() -> (mpsc::Sender<()>, JoinHandle<()>) {
        let (tell, go) = mpsc::channel::<()>();
        let thread = spawn(
            "parked".to_owned(),
            Box::new(move || {
                let _ = go.recv();
            }),
        )
        .unwrap();
        (tell, thread)
    }

    #[test]
    fn three_places_are_taken_the_fourth_is_refused_at_once_and_a_place_comes_back_after_a_return_or_a_panic()
     {
        let (tell_first, first) = parked();
        let (tell_second, second) = parked();
        let (tell_third, third) = parked();
        assert_eq!(LIVE.load(Ordering::SeqCst), 3);

        // The fourth is refused in the caller, at once, waiting for nothing, and its work
        // never runs.
        let ran = Arc::new(AtomicBool::new(false));
        let seen = ran.clone();
        let asked = Instant::now();
        let fourth = spawn(
            "fourth".to_owned(),
            Box::new(move || seen.store(true, Ordering::SeqCst)),
        );
        assert!(fourth.is_err());
        assert!(asked.elapsed() < Duration::from_secs(1));
        assert_eq!(LIVE.load(Ordering::SeqCst), 3);

        // One returns and is joined: its place comes back, and one more is made.
        tell_first.send(()).unwrap();
        first.join().unwrap();
        assert_eq!(LIVE.load(Ordering::SeqCst), 2);

        // A thread whose work panics gives its place back too.
        let panicked = spawn(
            "panics".to_owned(),
            Box::new(|| panic!("the work gives up")),
        )
        .unwrap();
        assert!(panicked.join().is_err());
        assert_eq!(LIVE.load(Ordering::SeqCst), 2);

        drop(tell_second);
        second.join().unwrap();
        drop(tell_third);
        third.join().unwrap();
        assert_eq!(LIVE.load(Ordering::SeqCst), 0);
        assert!(!ran.load(Ordering::SeqCst));
    }
}
