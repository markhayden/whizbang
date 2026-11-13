//! The one way a test makes a thread. A process holds at most [`LIVE_LIMIT`] threads that
//! were made here at once, and the thread that would be one more is not made: the call
//! panics, in the caller, before anything is asked of the system.
//!
//! Why: a test that made threads until the system refused one took a machine down, twice.
//! A script that counts threads from outside (`scripts/thread-guard.sh`) cannot stop a
//! loop that makes tens of thousands of them a second; a count that is taken before each
//! thread is made can. `clippy.toml` refuses `std::thread::spawn`, `std::thread::scope`
//! and the builder's `spawn` everywhere in the workspace, so a test has no other way: the
//! functions here are where that lint is allowed, with the executor's own spawner, which
//! has its own bound (ADR 0008, "How many runs are started at once").
//!
//! [`spawn`] and [`scope`] stand in for `std::thread::spawn` and `std::thread::scope`,
//! and take what those take.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{JoinHandle, ScopedJoinHandle};

/// How many threads made here one process holds at once. The tests of the workspace
/// hold eight at most.
pub const LIVE_LIMIT: usize = 64;

/// The threads made here that have not ended, in this process.
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// How many threads made here are live now.
pub fn live() -> usize {
    LIVE.load(Ordering::SeqCst)
}

/// One live thread's place in the count: taken before the thread is made, and given
/// back when this is dropped, which the thread does when its work is over, by a return
/// or by a panic; or the caller, when the thread could not be made.
struct Counted {
    count: &'static AtomicUsize,
}

impl Counted {
    /// Takes a place, or panics when `limit` are taken: nothing was made.
    fn take(count: &'static AtomicUsize, limit: usize) -> Counted {
        let taken = |now: usize| (now < limit).then_some(now + 1);
        if count
            .try_update(Ordering::SeqCst, Ordering::SeqCst, taken)
            .is_err()
        {
            panic!(
                "a test holds {limit} live threads and asked for one more: no test needs \
                 that many, and none is made (whizbang_testkit::threads)"
            );
        }
        Counted { count }
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
    }
}

/// As `std::thread::spawn`, counted: panics, and makes nothing, when this process
/// already holds [`LIVE_LIMIT`] threads made here.
pub fn spawn<T, F>(work: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    spawn_counted(&LIVE, LIVE_LIMIT, work)
}

fn spawn_counted<T, F>(count: &'static AtomicUsize, limit: usize, work: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let counted = Counted::take(count, limit);
    // The one place a test's thread is made.
    #[allow(clippy::disallowed_methods)]
    std::thread::spawn(move || {
        let _counted = counted;
        work()
    })
}

/// As `std::thread::scope`: the threads made in it are counted with every other, and
/// are joined before it returns.
pub fn scope<'env, F, T>(work: F) -> T
where
    F: for<'scope> FnOnce(&Scope<'scope, 'env>) -> T,
{
    scope_counted(&LIVE, LIVE_LIMIT, work)
}

fn scope_counted<'env, F, T>(count: &'static AtomicUsize, limit: usize, work: F) -> T
where
    F: for<'scope> FnOnce(&Scope<'scope, 'env>) -> T,
{
    // The one place a test's scope is made; its threads are made below.
    #[allow(clippy::disallowed_methods)]
    std::thread::scope(|inner| {
        work(&Scope {
            inner,
            count,
            limit,
            env: PhantomData,
        })
    })
}

/// What [`scope`] hands its work: as `std::thread::Scope`.
pub struct Scope<'scope, 'env: 'scope> {
    inner: &'scope std::thread::Scope<'scope, 'env>,
    count: &'static AtomicUsize,
    limit: usize,
    env: PhantomData<&'env mut &'env ()>,
}

impl<'scope> Scope<'scope, '_> {
    /// As `std::thread::Scope::spawn`, counted as [`spawn`] is.
    pub fn spawn<F, T>(&self, work: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        let counted = Counted::take(self.count, self.limit);
        #[allow(clippy::disallowed_methods)]
        self.inner.spawn(move || {
            let _counted = counted;
            work()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::mpsc;

    use super::*;

    /// A count of this test's own, so that what the other tests of the process make
    /// does not move it.
    fn a_count() -> &'static AtomicUsize {
        Box::leak(Box::new(AtomicUsize::new(0)))
    }

    #[test]
    fn the_thread_over_the_limit_is_not_made_and_a_thread_that_ends_makes_room() {
        // A limit of three, and three threads parked: never more than that.
        let count = a_count();
        let (go, wait) = mpsc::channel::<()>();
        let wait = std::sync::Arc::new(std::sync::Mutex::new(wait));
        let park = || {
            let wait = wait.clone();
            spawn_counted(count, 3, move || wait.lock().unwrap().recv().is_err())
        };
        let parked = [park(), park(), park()];
        assert_eq!(count.load(Ordering::SeqCst), 3);

        // The fourth is refused in the caller, and its work never runs.
        let ran = std::sync::Arc::new(AtomicUsize::new(0));
        let refused = catch_unwind(AssertUnwindSafe(|| {
            let ran = ran.clone();
            spawn_counted(count, 3, move || ran.fetch_add(1, Ordering::SeqCst))
        }));
        let said = *refused
            .expect_err("a fourth thread was made")
            .downcast::<String>()
            .unwrap();
        assert!(
            said.starts_with("a test holds 3 live threads and asked for one more"),
            "{said}"
        );
        assert_eq!(count.load(Ordering::SeqCst), 3);

        // The three end, and their places are free: by a return, and by a panic.
        drop(go);
        for thread in parked {
            thread.join().unwrap();
        }
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        let panicked = spawn_counted(count, 3, || panic!("the work of a thread gave up"));
        assert!(panicked.join().is_err());
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_threads_of_a_scope_are_counted_and_the_one_over_the_limit_is_not_made() {
        let count = a_count();
        let seen = AtomicUsize::new(0);
        let (go, wait) = mpsc::channel::<()>();
        let wait = std::sync::Mutex::new(wait);
        let refused = catch_unwind(AssertUnwindSafe(|| {
            let (wait, seen) = (&wait, &seen);
            scope_counted(count, 2, move |scope| {
                // The two that are parked are let go when the channel ends, which the
                // unwinding of this closure brings about.
                let _go = go;
                scope.spawn(move || wait.lock().unwrap().recv().is_err());
                scope.spawn(move || wait.lock().unwrap().recv().is_err());
                seen.store(count.load(Ordering::SeqCst), Ordering::SeqCst);
                // The third is refused.
                scope.spawn(|| ());
            })
        }));
        assert!(refused.is_err(), "a third thread was made");
        assert_eq!(seen.load(Ordering::SeqCst), 2);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_limit_of_a_process_is_sixty_four_and_the_functions_count_in_one_place() {
        assert_eq!(LIVE_LIMIT, 64);
        let (go, wait) = mpsc::channel::<()>();
        let before = live();
        let parked = spawn(move || wait.recv().is_err());
        assert!(live() > before.saturating_sub(1));
        let within = scope(|scope| scope.spawn(live).join().unwrap());
        assert!(within >= 2, "{within}");
        drop(go);
        parked.join().unwrap();
    }
}
