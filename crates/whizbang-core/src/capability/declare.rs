//! What a backend declares, and resolution (`SPEC.md` section 4; ADR 0006,
//! "Declarations" and "Resolution").
//!
//! A [`Roster`] holds the backends on offer for each capability, each under a name a
//! call can choose it by, and at most one default for each capability. Resolution is a
//! function of the roster and of what the call asks, and of nothing else: the call's
//! explicit choice, or else the default; then the features. Whatever was picked either
//! declares every feature needed or the resolution fails. No other backend is ever
//! looked for, so a model is never substituted.

use std::collections::BTreeSet;

use crate::contract::{Backends, Capability, ModelIdentity, Unavailable, text};

/// The features a request can need, by the names a descriptor and a declaration spell
/// them with. A request that holds what a feature is for needs that feature (ADR 0006).
pub mod feature {
    /// Segment: a request by a phrase.
    pub const PHRASE: &str = "phrase";
    /// Segment: a request by a point.
    pub const POINT: &str = "point";
    /// Segment: a request by a box.
    pub const BOX: &str = "box";
    /// Fill: a request with a prompt.
    pub const PROMPT: &str = "prompt";
    /// Judge: a conversation with an image in it.
    pub const IMAGE: &str = "image";
    /// Judge: a request that defines tools.
    pub const TOOL: &str = "tool";
}

/// What a backend declares of itself: the model that answers, and the features it
/// supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    model: ModelIdentity,
    features: BTreeSet<String>,
}

impl Declaration {
    /// A declaration of this model with these features. A feature is a name, as an
    /// action's is, and is declared once.
    pub fn new(model: ModelIdentity, features: &[&str]) -> Result<Declaration, String> {
        let mut declared = BTreeSet::new();
        for feature in features {
            text::action_name(feature).map_err(|why| format!("a feature: {why}"))?;
            if !declared.insert((*feature).to_owned()) {
                return Err(format!("the feature {feature} is declared twice"));
            }
        }
        Ok(Declaration {
            model,
            features: declared,
        })
    }

    /// The backend, the model and its version.
    pub fn model(&self) -> &ModelIdentity {
        &self.model
    }

    /// Whether the feature is declared.
    pub fn has(&self, feature: &str) -> bool {
        self.features.contains(feature)
    }

    /// The declared features, in the order of their names.
    pub fn features(&self) -> impl Iterator<Item = &str> {
        self.features.iter().map(String::as_str)
    }
}

/// Why a roster could not be built as asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RosterError {
    /// The name is not one a call could choose: it does not keep the rule of a name.
    Name { capability: Capability, why: String },
    /// The capability already has a backend of this name.
    Twice {
        capability: Capability,
        name: String,
    },
    /// The default names a backend that is not offered for the capability.
    NotOffered {
        capability: Capability,
        name: String,
    },
}

impl std::fmt::Display for RosterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RosterError::Name { capability, why } => {
                write!(f, "a {} backend: {why}", capability.name())
            }
            RosterError::Twice { capability, name } => {
                write!(f, "two {} backends are named {name}", capability.name())
            }
            RosterError::NotOffered { capability, name } => write!(
                f,
                "the default for {} is {name}, which is not offered",
                capability.name()
            ),
        }
    }
}

impl std::error::Error for RosterError {}

/// Why nothing answers a capability for a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unresolved {
    /// The call chose no backend, and no default is configured for the capability.
    NoDefault { capability: Capability },
    /// The call chose a backend that is not offered for the capability. The default is
    /// not used in its place.
    UnknownChoice {
        capability: Capability,
        choice: String,
    },
    /// The backend that was chosen, or the default, does not declare a feature the call
    /// needs: the first such feature, in the order they were needed. No other backend
    /// is used in its place.
    MissingFeature {
        capability: Capability,
        name: String,
        feature: String,
    },
}

/// How many characters of a refused choice are shown.
const SHOWN: usize = 64;

impl std::fmt::Display for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unresolved::NoDefault { capability } => write!(
                f,
                "no default backend is configured for {}, and the call chose none",
                capability.name()
            ),
            Unresolved::UnknownChoice { capability, choice } => {
                // The choice is what a caller typed: shown escaped, and its start only.
                let count = choice.chars().count();
                let start: String = choice.chars().take(SHOWN).collect();
                write!(f, "no {} backend is named {start:?}", capability.name())?;
                if count > SHOWN {
                    write!(f, "... ({count} characters)")?;
                }
                Ok(())
            }
            Unresolved::MissingFeature {
                capability,
                name,
                feature,
            } => write!(
                f,
                "the {} backend {name} does not declare the feature {feature}",
                capability.name()
            ),
        }
    }
}

impl std::error::Error for Unresolved {}

/// The backend a resolution picked: the name it is offered under, and its declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved<'a> {
    name: &'a str,
    declaration: &'a Declaration,
}

impl<'a> Resolved<'a> {
    /// The name the backend is offered under.
    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn declaration(&self) -> &'a Declaration {
        self.declaration
    }
}

/// The backends on offer: for each capability, backends by name, and at most one
/// default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roster {
    offers: Vec<(Capability, String, Declaration)>,
    defaults: Vec<(Capability, String)>,
}

impl Roster {
    /// A roster that offers nothing.
    pub fn new() -> Roster {
        Roster::default()
    }

    /// The same roster, also offering this backend for the capability, under a name a
    /// call can choose it by. A name keeps the rule of an action's name, and names one
    /// backend of a capability.
    pub fn offer(
        mut self,
        capability: Capability,
        name: &str,
        declaration: Declaration,
    ) -> Result<Roster, RosterError> {
        text::action_name(name).map_err(|why| RosterError::Name { capability, why })?;
        if self.find(capability, name).is_some() {
            return Err(RosterError::Twice {
                capability,
                name: name.to_owned(),
            });
        }
        self.offers.push((capability, name.to_owned(), declaration));
        Ok(self)
    }

    /// The same roster, with this backend as the capability's default: the one that
    /// answers a call that chose none. It is already offered. Setting a default again
    /// replaces it.
    pub fn default_for(
        mut self,
        capability: Capability,
        name: &str,
    ) -> Result<Roster, RosterError> {
        if self.find(capability, name).is_none() {
            return Err(RosterError::NotOffered {
                capability,
                name: name.to_owned(),
            });
        }
        self.defaults.retain(|(other, _)| *other != capability);
        self.defaults.push((capability, name.to_owned()));
        Ok(self)
    }

    fn find(&self, capability: Capability, name: &str) -> Option<Resolved<'_>> {
        self.offers
            .iter()
            .find(|(offered, offer, _)| *offered == capability && offer == name)
            .map(|(_, name, declaration)| Resolved { name, declaration })
    }

    /// Which backend answers the capability for a call: the one the call chose, or else
    /// the default; and it declares every feature needed. Nothing else is tried.
    pub fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<Resolved<'_>, Unresolved> {
        let picked = match choice {
            Some(choice) => {
                self.find(capability, choice)
                    .ok_or_else(|| Unresolved::UnknownChoice {
                        capability,
                        choice: choice.to_owned(),
                    })?
            }
            None => self
                .defaults
                .iter()
                .find(|(other, _)| *other == capability)
                .and_then(|(_, name)| self.find(capability, name))
                .ok_or(Unresolved::NoDefault { capability })?,
        };
        match features
            .iter()
            .find(|feature| !picked.declaration.has(feature))
        {
            Some(feature) => Err(Unresolved::MissingFeature {
                capability,
                name: picked.name.to_owned(),
                feature: feature.clone(),
            }),
            None => Ok(picked),
        }
    }
}

impl Backends for Roster {
    fn resolve(
        &self,
        capability: Capability,
        choice: Option<&str>,
        features: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        match Roster::resolve(self, capability, choice, features) {
            Ok(resolved) => Ok(resolved.declaration.model().clone()),
            Err(unresolved) => Err(Unavailable::new(&unresolved.to_string())),
        }
    }
}
