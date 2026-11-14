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

use crate::contract::{Backends, Capability, Estimate, ModelIdentity, Unavailable, text};

use super::{Price, Quote, Shown};

/// The rule of a name, as a refusal says it.
const NAME_RULE: &str = "a name is a lower-case letter, then lower-case letters, digits, _ or -, \
                         64 characters at most";

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
    price: Option<Price>,
}

impl Declaration {
    /// A declaration of this model with these features. A feature is a name, as an
    /// action's is, and is declared once.
    ///
    /// The backend, the model and the version are each 1 to [`MODEL_NAME_LIMIT`]
    /// printable ASCII characters with no space, no `"` and no `\`: each is written
    /// in a run's record once for every request the run sends (ADR 0009).
    pub fn new(model: ModelIdentity, features: &[&str]) -> Result<Declaration, DeclarationError> {
        let named = [
            ("backend", model.backend()),
            ("model", model.model()),
            ("version", model.version()),
        ];
        for (what, name) in named {
            let plain = |b: u8| b.is_ascii_graphic() && b != b'"' && b != b'\\';
            if name.is_empty() || name.len() > MODEL_NAME_LIMIT || !name.bytes().all(plain) {
                return Err(DeclarationError::Model { what });
            }
        }
        let mut declared = BTreeSet::new();
        for feature in features {
            let feature = (*feature).to_owned();
            if text::action_name(&feature).is_err() {
                return Err(DeclarationError::Name { feature });
            }
            if declared.contains(&feature) {
                return Err(DeclarationError::Twice { feature });
            }
            declared.insert(feature);
        }
        Ok(Declaration {
            model,
            features: declared,
            price: None,
        })
    }

    /// The same declaration, stating this price (ADR 0010). Stated again, the later
    /// price is the price.
    pub fn priced(mut self, price: Price) -> Declaration {
        self.price = Some(price);
        self
    }

    /// The price the backend states, when it states one.
    pub fn price(&self) -> Option<&Price> {
        self.price.as_ref()
    }

    /// What one request may cost under the price stated, when one is.
    pub fn quote(&self) -> Option<Quote> {
        self.price.as_ref().map(Price::quote)
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

/// The most characters of the name of a backend, of a model and of a model's version.
pub const MODEL_NAME_LIMIT: usize = 128;

/// Why a declaration could not be made.
#[derive(Clone, PartialEq, Eq)]
pub enum DeclarationError {
    /// The name of the backend, of the model or of its version (`what` says which) is
    /// not 1 to [`MODEL_NAME_LIMIT`] printable ASCII characters with no space, no `"`
    /// and no `\`. The name is not shown.
    Model { what: &'static str },
    /// The feature is not a name.
    Name { feature: String },
    /// The feature is declared twice.
    Twice { feature: String },
}

impl std::fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarationError::Model { what } => write!(
                f,
                "the name of the {what} is not 1 to {MODEL_NAME_LIMIT} printable ASCII \
                 characters with no space, no double quote and no backslash"
            ),
            DeclarationError::Name { feature } => {
                write!(
                    f,
                    "the feature {} is not a name: {NAME_RULE}",
                    Shown(feature)
                )
            }
            DeclarationError::Twice { feature } => {
                write!(f, "the feature {} is declared twice", Shown(feature))
            }
        }
    }
}

impl std::fmt::Debug for DeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, feature) = match self {
            DeclarationError::Model { what } => {
                return f.debug_struct("Model").field("what", what).finish();
            }
            DeclarationError::Name { feature } => ("Name", feature),
            DeclarationError::Twice { feature } => ("Twice", feature),
        };
        f.debug_struct(kind)
            .field("feature", &Shown(feature))
            .finish()
    }
}

impl std::error::Error for DeclarationError {}

/// Why a roster could not be built as asked.
#[derive(Clone, PartialEq, Eq)]
pub enum RosterError {
    /// The name is not one a call could choose: it does not keep the rule of a name.
    Name {
        capability: Capability,
        name: String,
    },
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
            RosterError::Name { capability, name } => write!(
                f,
                "a {} backend cannot be named {}: {NAME_RULE}",
                capability.name(),
                Shown(name)
            ),
            RosterError::Twice { capability, name } => {
                write!(f, "two {} backends are named {name}", capability.name())
            }
            RosterError::NotOffered { capability, name } => write!(
                f,
                "the default for {} is {}, which is not offered",
                capability.name(),
                Shown(name)
            ),
        }
    }
}

impl std::fmt::Debug for RosterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, capability, name) = match self {
            RosterError::Name { capability, name } => ("Name", capability, name),
            RosterError::Twice { capability, name } => ("Twice", capability, name),
            RosterError::NotOffered { capability, name } => ("NotOffered", capability, name),
        };
        f.debug_struct(kind)
            .field("capability", capability)
            .field("name", &Shown(name))
            .finish()
    }
}

impl std::error::Error for RosterError {}

/// Why nothing answers a capability for a call. A choice and a feature are what a
/// caller typed: a sentence and `{:?}` show each escaped, and its first 64 characters at
/// most.
#[derive(Clone, PartialEq, Eq)]
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

impl std::fmt::Display for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unresolved::NoDefault { capability } => write!(
                f,
                "no default backend is configured for {}, and the call chose none",
                capability.name()
            ),
            Unresolved::UnknownChoice { capability, choice } => write!(
                f,
                "no {} backend is named {}",
                capability.name(),
                Shown(choice)
            ),
            // The name is one the roster offers, and so a name; the feature is the
            // caller's.
            Unresolved::MissingFeature {
                capability,
                name,
                feature,
            } => write!(
                f,
                "the {} backend {name} does not declare the feature {}",
                capability.name(),
                Shown(feature)
            ),
        }
    }
}

impl std::fmt::Debug for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unresolved::NoDefault { capability } => f
                .debug_struct("NoDefault")
                .field("capability", capability)
                .finish(),
            Unresolved::UnknownChoice { capability, choice } => f
                .debug_struct("UnknownChoice")
                .field("capability", capability)
                .field("choice", &Shown(choice))
                .finish(),
            Unresolved::MissingFeature {
                capability,
                name,
                feature,
            } => f
                .debug_struct("MissingFeature")
                .field("capability", capability)
                .field("name", name)
                .field("feature", &Shown(feature))
                .finish(),
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
        if text::action_name(name).is_err() {
            return Err(RosterError::Name {
                capability,
                name: name.to_owned(),
            });
        }
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

    /// The most one request may cost when this model answers the capability: what the
    /// price its backend declares comes to ([`Quote::most`]; ADR 0010). An estimate
    /// only is the amount that is no bound. A backend that states no price, and a
    /// model that nothing offered for the capability declares, answer nothing, as
    /// before a declaration could state a price.
    ///
    /// The question names a model and not the backend a call chose, so two backends
    /// offered for one capability that declare one model are not told apart: the
    /// answer is the largest of theirs, and no bound when either has none.
    fn bound(&self, capability: Capability, model: &ModelIdentity) -> Estimate {
        self.offers
            .iter()
            .filter(|(offered, _, declaration)| {
                *offered == capability && declaration.model() == model
            })
            .filter_map(|(_, _, declaration)| declaration.quote())
            .map(|quote| quote.most().micros())
            .max()
            .map_or_else(Estimate::default, Estimate::of_micros)
    }
}
