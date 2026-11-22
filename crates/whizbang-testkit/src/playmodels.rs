//! A model the playground's Models screen can install in a test, and a download that a
//! test can hold half way (plan m5, task M1). Nothing is fetched from anywhere: the files
//! are a few kilobytes made here, served by [`FakeDownloader`] from memory, at an address
//! of the reserved `.example` domain.
//!
//! [`GatedDownloader`] serves each file as the fake does; while it is held, a download
//! after the first waits, until it is released or its cancellation is tripped (then it
//! ends as cancelled). So an install shows one file of its files done, and a Cancel or a
//! quit finds it going.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::{DownloadRequest, Downloaded, Downloader};

use crate::downloads::FakeDownloader;

/// The origin the model's files are served from.
pub const ORIGIN: &str = "https://models.example.org";
/// The model's name.
pub const MODEL: &str = "pocket";

/// The model's files, each a name and its bytes, in the order an install takes them.
pub fn files() -> Vec<(&'static str, Vec<u8>)> {
    let made = |name: &str, size: usize| -> Vec<u8> {
        format!("{MODEL}/{name}")
            .bytes()
            .cycle()
            .take(size)
            .collect()
    };
    vec![
        ("LICENSE", made("LICENSE", 600)),
        ("config.json", made("config.json", 900)),
        ("weights.bin", made("weights.bin", 4096)),
    ]
}

/// The address of one of the model's files.
pub fn address_of(file: &str) -> String {
    format!("{ORIGIN}/{MODEL}/{file}")
}

#[derive(Default)]
struct Gate {
    held: bool,
    /// Held whatever a download's cancellation says, until released: an install that is
    /// stopping and has not ended yet.
    deaf: bool,
    /// How many downloads have started.
    started: u64,
}

/// A download held half way while a test says so.
pub struct GatedDownloader {
    inner: FakeDownloader,
    gate: Mutex<Gate>,
    changed: Condvar,
}

impl GatedDownloader {
    /// Serving what this fake serves, not held.
    pub fn new(inner: FakeDownloader) -> GatedDownloader {
        GatedDownloader {
            inner,
            gate: Mutex::new(Gate::default()),
            changed: Condvar::new(),
        }
    }

    fn gate(&self) -> MutexGuard<'_, Gate> {
        self.gate.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// From now on, every download after the first waits.
    pub fn hold(&self) {
        self.gate().held = true;
        self.gate().started = 0;
        self.changed.notify_all();
    }

    /// As [`GatedDownloader::hold`], and a download waiting at the gate does not end when
    /// it is cancelled, until [`GatedDownloader::release`].
    pub fn hold_deaf(&self) {
        self.hold();
        self.gate().deaf = true;
    }

    /// Lets every waiting download go on.
    pub fn release(&self) {
        let mut gate = self.gate();
        gate.held = false;
        gate.deaf = false;
        drop(gate);
        self.changed.notify_all();
    }

    /// Whether a download is waiting at the gate now: one started after the first while
    /// it is held.
    pub fn waiting(&self) -> bool {
        let gate = self.gate();
        gate.held && gate.started > 1
    }
}

impl Downloader for GatedDownloader {
    fn download(
        &self,
        request: &DownloadRequest,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        let mut gate = self.gate();
        gate.started += 1;
        self.changed.notify_all();
        while gate.held && gate.started > 1 {
            if cancellation.is_cancelled() && !gate.deaf {
                return Err(Fault::new(
                    FaultKind::Cancelled,
                    "the download was cancelled",
                ));
            }
            gate = self
                .changed
                .wait_timeout(gate, Duration::from_millis(20))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        drop(gate);
        self.inner.download(request, cancellation, sink)
    }
}
