//! A fake of the sending interface (ADR 0012): it sends nothing anywhere. Each request
//! it is handed is answered with the next reply of a script, which may be a redirect,
//! an answer that names an upload address or a download address, or a fault; and it
//! keeps every request, with the address it was for and the headers it would have
//! carried, in the order they came. And a fake of the probing interface, scripted the
//! same way: it probes nothing.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::capability::{Bounds, Fault, FaultKind};
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::{
    Address, Classes, Denied, Hop as Asked, HopCheck, Origin, Outgoing, ProbeRequest, Probed,
    Prober, Recipient, Reply, Sender,
};

/// One request the fake was handed: what would have gone to one address. What `{:?}`
/// prints is what an [`Outgoing`] prints: no byte of a piece and no credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    outgoing: Outgoing,
}

impl Hop {
    /// The address the request was for.
    pub fn to(&self) -> &Address {
        self.outgoing.to()
    }

    /// The whole of what was to be sent.
    pub fn outgoing(&self) -> &Outgoing {
        &self.outgoing
    }

    /// The headers the request would have carried, each a name and a value: the
    /// credential's, when the request holds one, and no other.
    pub fn headers(&self) -> Vec<(String, String)> {
        self.outgoing
            .credential()
            .map(|credential| {
                (
                    credential.header().to_owned(),
                    credential.expose().to_owned(),
                )
            })
            .into_iter()
            .collect()
    }
}

/// What is left of the script, and what was handed over, behind one lock.
struct Played {
    script: VecDeque<Result<Reply, Fault>>,
    hops: Vec<Hop>,
}

/// A fake sender: it plays scripted hops. It does not look at what it is handed: a
/// request the script's writer did not expect is answered with the next entry like any
/// other, and a test that cares reads what the fake kept. Past the end of its script
/// it answers with the fault `malformed`, and never panics.
///
/// It may be shared across threads. Keeping a request and playing the next entry are
/// one step, so the request kept in place `n` is the one answered with entry `n`.
pub struct FakeSender {
    played: Mutex<Played>,
}

impl FakeSender {
    /// A sender that answers with these, in order.
    pub fn scripted(script: Vec<Result<Reply, Fault>>) -> FakeSender {
        FakeSender {
            played: Mutex::new(Played {
                script: script.into(),
                hops: Vec::new(),
            }),
        }
    }

    fn played(&self) -> MutexGuard<'_, Played> {
        self.played.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every request it was handed, in order.
    pub fn hops(&self) -> Vec<Hop> {
        self.played().hops.clone()
    }

    /// How many requests it was handed, answered or not.
    pub fn requests(&self) -> u64 {
        self.played().hops.len() as u64
    }

    /// How many entries of the script have not been played.
    pub fn remaining(&self) -> usize {
        self.played().script.len()
    }
}

impl std::fmt::Debug for FakeSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let played = self.played();
        write!(
            f,
            "FakeSender({} hops, {} replies left)",
            played.hops.len(),
            played.script.len()
        )
    }
}

impl Sender for FakeSender {
    fn send(&self, outgoing: &Outgoing, _: &Bounds) -> Result<Reply, Fault> {
        let mut played = self.played();
        played.hops.push(Hop {
            outgoing: outgoing.clone(),
        });
        played.script.pop_front().unwrap_or_else(|| {
            // The request was handed over and nothing settles it: the fake says so.
            Err(
                Fault::new(FaultKind::Malformed, "the scripted hops have no reply left")
                    .unresolved(),
            )
        })
    }
}

/// One probe the fake was handed. What `{:?}` prints is what a [`ProbeRequest`]
/// prints: the address's origin and the size of its rest, and no credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    request: ProbeRequest,
}

impl Probe {
    /// The address the probe was for.
    pub fn to(&self) -> &Address {
        self.request.to()
    }

    /// The whole request: an address and a credential are all a probe has.
    pub fn request(&self) -> &ProbeRequest {
        &self.request
    }

    /// The headers the probe would have carried, each a name and a value: the
    /// credential's, when the request holds one, and no other.
    pub fn headers(&self) -> Vec<(String, String)> {
        self.request
            .credential()
            .map(|credential| {
                (
                    credential.header().to_owned(),
                    credential.expose().to_owned(),
                )
            })
            .into_iter()
            .collect()
    }
}

/// What is left of a prober's script, and what it was handed, behind one lock.
struct Probing {
    script: VecDeque<Result<Probed, Fault>>,
    probes: Vec<Probe>,
}

/// A fake prober: it plays scripted answers, as [`FakeSender`] plays replies. It does
/// not look at what it is handed, nor at the cancellation signal: a probe the
/// script's writer did not expect is answered with the next entry like any other, and
/// a test that cares reads what the fake kept. It follows nothing. Past the end of
/// its script it answers with the fault `malformed`, and never panics.
///
/// It may be shared across threads. Keeping a probe and playing the next entry are
/// one step, so the probe kept in place `n` is the one answered with entry `n`.
pub struct FakeProber {
    probing: Mutex<Probing>,
}

impl FakeProber {
    /// A prober that answers with these, in order.
    pub fn scripted(script: Vec<Result<Probed, Fault>>) -> FakeProber {
        FakeProber {
            probing: Mutex::new(Probing {
                script: script.into(),
                probes: Vec::new(),
            }),
        }
    }

    fn probing(&self) -> MutexGuard<'_, Probing> {
        self.probing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every probe it was handed, in order.
    pub fn probes(&self) -> Vec<Probe> {
        self.probing().probes.clone()
    }

    /// How many probes it was handed, answered or not.
    pub fn requests(&self) -> u64 {
        self.probing().probes.len() as u64
    }

    /// How many entries of the script have not been played.
    pub fn remaining(&self) -> usize {
        self.probing().script.len()
    }
}

impl std::fmt::Debug for FakeProber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let probing = self.probing();
        write!(
            f,
            "FakeProber({} probes, {} answers left)",
            probing.probes.len(),
            probing.script.len()
        )
    }
}

impl Prober for FakeProber {
    fn probe(&self, request: &ProbeRequest, _: &Cancellation) -> Result<Probed, Fault> {
        let mut probing = self.probing();
        probing.probes.push(Probe {
            request: request.clone(),
        });
        probing.script.pop_front().unwrap_or_else(|| {
            // The probe was handed over and nothing settles it: the fake says so.
            Err(Fault::new(
                FaultKind::Malformed,
                "the scripted probes have no answer left",
            )
            .unresolved())
        })
    }
}

/// What a fake check was shown of one address, before anything was handed to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Looked {
    /// The recipient the backend declared.
    pub recipient: Recipient,
    /// The origin of the address the request was for.
    pub origin: Origin,
    /// The classes of data the request holds.
    pub needs: Classes,
    /// How many answers in a row, just before, pointed elsewhere.
    pub moves: u32,
    /// How many answers pointed elsewhere, up to now, for this backend of this run.
    pub total: u32,
}

/// What a test has a fake check do while it is asked.
type Hook = Box<dyn Fn() + Send + Sync>;

/// A fake of the check every address goes through before data is handed to it (ADR
/// 0015): it approves every address, or refuses every one with a sentence, as a test
/// tells it to, keeps what it was shown each time, and can run a test's hook while it
/// is asked. It knows no policy and asks none.
pub struct FakeCheck {
    refusing: Option<&'static str>,
    looked: Mutex<Vec<Looked>>,
    hook: Mutex<Option<Hook>>,
}

impl FakeCheck {
    /// A check that approves every address it is shown.
    pub fn approving() -> FakeCheck {
        FakeCheck {
            refusing: None,
            looked: Mutex::new(Vec::new()),
            hook: Mutex::new(None),
        }
    }

    /// A check that refuses every address it is shown, with this sentence.
    pub fn refusing(why: &'static str) -> FakeCheck {
        FakeCheck {
            refusing: Some(why),
            ..FakeCheck::approving()
        }
    }

    /// Runs this each time the check is asked from here on, after it has kept what it
    /// was shown and before it answers.
    pub fn when_asked(&self, hook: impl Fn() + Send + Sync + 'static) {
        let mut held = self.hook.lock().unwrap_or_else(PoisonError::into_inner);
        *held = Some(Box::new(hook));
    }

    /// What it was shown, each time it was asked, in order.
    pub fn looked(&self) -> Vec<Looked> {
        let looked = self.looked.lock().unwrap_or_else(PoisonError::into_inner);
        looked.clone()
    }
}

impl std::fmt::Debug for FakeCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let answer = if self.refusing.is_some() {
            "refusing"
        } else {
            "approving"
        };
        write!(
            f,
            "FakeCheck({answer}, asked {} times)",
            self.looked().len()
        )
    }
}

impl HopCheck for FakeCheck {
    fn look(&self, hop: &Asked<'_>) -> Result<(), Denied> {
        {
            let mut looked = self.looked.lock().unwrap_or_else(PoisonError::into_inner);
            looked.push(Looked {
                recipient: hop.recipient().clone(),
                origin: hop.origin().clone(),
                needs: hop.needs(),
                moves: hop.moves(),
                total: hop.total_moves(),
            });
        }
        // The hook is run with no lock of the fake held but its own.
        let hook = self.hook.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(hook) = hook.as_ref() {
            hook();
        }
        match self.refusing {
            Some(why) => Err(Denied::because(why)),
            None => Ok(()),
        }
    }
}
