//! The plan of a call (ADR 0004, "A call's plan"): what preparation settles for an
//! action that plans, before the call is admitted.
//!
//! An action's descriptor states fixed needs, which apply by whether an argument is
//! given. An action that plans ([`Draft::plans`](super::Draft::plans)) is asked once
//! more, while its call is prepared ([`Action::plan`](super::Action::plan)): it is
//! shown the call's arguments, each capability the call could need with whether a
//! model answers it and the bound of one request to it, and the config's cap of one
//! call ([`Planning`]); and it states the plan of this call ([`Plan`]): how many
//! requests the call may send each capability, none for one it leaves out, and, for a
//! loop, how many attempts it may make and whether it checks, with the reason.
//!
//! The plan decides the needs preparation resolves and the estimate: a capability the
//! plan leaves out is not a need of the call, is not missing when nothing answers it,
//! and adds nothing to what is reserved. It is settled once: it is part of the
//! [`PreparedCall`](super::PreparedCall) and of its fingerprint, an action is handed it
//! with the prepared call, and admission, which prepares the call again, refuses a
//! preparation whose plan is no longer the one that would be made
//! ([`Stale::Plan`](super::Stale::Plan)).
//!
//! Planning has no side effects: an action is given nothing here to send with, to
//! reserve with or to read the store with.

use serde::{Deserialize, Serialize};

use super::{Capability, Estimate, Value};

/// The most attempts a plan may state, asked or allowed.
pub const PLAN_ATTEMPTS_MAX: u32 = 16;

/// The most characters of a plan's reason.
pub const PLAN_WHY_LIMIT: usize = 200;

/// One capability a call could need, as planning is shown it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Able {
    pub(super) capability: Capability,
    pub(super) requests: u64,
    pub(super) bound: Result<Estimate, String>,
}

impl Able {
    pub fn capability(&self) -> Capability {
        self.capability
    }

    /// How many requests the descriptor says one call may send it.
    pub fn requests(&self) -> u64 {
        self.requests
    }

    /// Whether a model answers it for this call.
    pub fn is_ready(&self) -> bool {
        self.bound.is_ok()
    }

    /// The bound of one request to the model that answers, when one does.
    pub fn bound(&self) -> Option<Estimate> {
        self.bound.as_ref().ok().copied()
    }

    /// Why nothing answers it, when nothing does.
    pub fn why_not(&self) -> Option<&str> {
        self.bound.as_ref().err().map(String::as_str)
    }
}

/// What an action is shown when it plans a call.
#[derive(Debug, Clone, PartialEq)]
pub struct Planning<'a> {
    pub(super) arguments: &'a [(String, Value)],
    pub(super) able: Vec<Able>,
    pub(super) cap: Option<Estimate>,
}

impl Planning<'_> {
    /// Every argument of the call that has a value, as the prepared call holds them.
    pub fn arguments(&self) -> &[(String, Value)] {
        self.arguments
    }

    /// The value of this argument, when it has one.
    pub fn argument(&self, name: &str) -> Option<&Value> {
        let mut arguments = self.arguments.iter();
        arguments.find_map(|(n, value)| (n == name).then_some(value))
    }

    /// Each capability the descriptor's needs name for this call, in their order.
    pub fn capabilities(&self) -> &[Able] {
        &self.able
    }

    /// This capability, when a need of the descriptor names it for this call.
    pub fn capability(&self, capability: Capability) -> Option<&Able> {
        self.able.iter().find(|able| able.capability == capability)
    }

    /// The most one call may reserve under the config (`spend.call_cap_usd`), when it
    /// sets one.
    pub fn cap(&self) -> Option<Estimate> {
        self.cap
    }
}

/// What a loop's plan states of its attempts: how many were asked, how many the plan
/// allows, the first included, and whether each is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempts {
    pub asked: u32,
    pub allowed: u32,
    pub checks: bool,
}

/// How many requests a planned call may send one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sends {
    pub capability: Capability,
    pub requests: u64,
}

/// The plan of one call. See the module.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    sends: Vec<Sends>,
    attempts: Option<Attempts>,
    why: Option<String>,
}

impl Plan {
    /// A plan that changes nothing yet: every capability is sent what the descriptor
    /// states.
    pub fn new() -> Plan {
        Plan::default()
    }

    /// The same, with this capability sent this many requests by the call, in place of
    /// what the descriptor states. None leaves the capability out: it is no need of
    /// the call, and nothing is reserved for it.
    pub fn sending(mut self, capability: Capability, requests: u64) -> Plan {
        self.sends.push(Sends {
            capability,
            requests,
        });
        self
    }

    /// The same, stating a loop's attempts: how many were asked, how many the plan
    /// allows, and whether each is checked.
    pub fn attempts(mut self, asked: u32, allowed: u32, checks: bool) -> Plan {
        self.attempts = Some(Attempts {
            asked,
            allowed,
            checks,
        });
        self
    }

    /// The same, with the reason for what it allows, in one line.
    pub fn because(mut self, why: &str) -> Plan {
        self.why = Some(why.to_owned());
        self
    }

    /// What it states of each capability it names, in the order it named them.
    pub fn sends(&self) -> &[Sends] {
        &self.sends
    }

    /// How many requests it says the call may send this capability, when it names it.
    pub fn requests(&self, capability: Capability) -> Option<u64> {
        let mut sends = self.sends.iter();
        sends.find_map(|s| (s.capability == capability).then_some(s.requests))
    }

    /// What it states of a loop's attempts, when it states any.
    pub fn stated(&self) -> Option<Attempts> {
        self.attempts
    }

    /// Its reason, when it gives one.
    pub fn why(&self) -> Option<&str> {
        self.why.as_deref()
    }

    /// The plan in one line, as a door prints it before the call runs.
    pub fn line(&self) -> String {
        let mut line = String::from("plan:");
        if let Some(attempts) = self.attempts {
            line.push_str(&format!(
                " {} of {} attempts asked, {}",
                attempts.allowed,
                attempts.asked,
                if attempts.checks {
                    "checked"
                } else {
                    "unchecked"
                }
            ));
        }
        for sends in &self.sends {
            let sep = if line.ends_with(':') { " " } else { "; " };
            line.push_str(&format!(
                "{sep}{} {}",
                sends.capability.name(),
                sends.requests
            ));
        }
        if let Some(why) = &self.why {
            let sep = if line.ends_with(':') { " " } else { "; " };
            line.push_str(&format!("{sep}{why}"));
        }
        line
    }

    /// What is wrong with the plan, for a call whose needs name these capabilities:
    /// none when it is one a call can be prepared with.
    pub(super) fn fault(&self, named: &[Capability]) -> Option<String> {
        for (at, sends) in self.sends.iter().enumerate() {
            let name = sends.capability.name();
            if !named.contains(&sends.capability) {
                return Some(format!("it names {name}, which is no need of this call"));
            }
            if self.sends[..at]
                .iter()
                .any(|s| s.capability == sends.capability)
            {
                return Some(format!("it names {name} twice"));
            }
        }
        if let Some(attempts) = self.attempts
            && !(1..=attempts.asked).contains(&attempts.allowed)
        {
            return Some("it allows no attempt, or more attempts than were asked".to_owned());
        }
        if self.attempts.is_some_and(|a| a.asked > PLAN_ATTEMPTS_MAX) {
            return Some(format!("it states more than {PLAN_ATTEMPTS_MAX} attempts"));
        }
        self.why_fault().map(str::to_owned)
    }

    fn why_fault(&self) -> Option<&'static str> {
        let why = self.why.as_deref()?;
        let fits = (1..=PLAN_WHY_LIMIT).contains(&why.chars().count());
        (!fits || why.chars().any(char::is_control) || why.trim() != why).then_some(
            "its reason is not one line of 1 to 200 characters with no space at either end",
        )
    }

    /// Whether a record may hold it: what [`Plan::fault`] holds that does not depend
    /// on the call.
    pub(super) fn holds(&self) -> bool {
        let named: Vec<Capability> = self.sends.iter().map(|s| s.capability).collect();
        self.fault(&named).is_none()
    }
}
