//! A fake of the sending interface (ADR 0012): it sends nothing anywhere. Each request
//! it is handed is answered with the next reply of a script, which may be a redirect,
//! an answer that names an upload address or a download address, or a fault; and it
//! keeps every request, with the address it was for and the headers it would have
//! carried, in the order they came.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use whizbang_core::capability::{Bounds, Fault, FaultKind};
use whizbang_core::egress::{
    Address, Classes, Denied, Hop as Asked, HopCheck, Origin, Outgoing, Recipient, Reply, Sender,
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
