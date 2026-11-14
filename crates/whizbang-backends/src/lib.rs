//! Implementations of the capability interfaces (`SPEC.md` section 3): backend kinds,
//! the pack client, provider adapters, and model installations.
//!
//! So far it holds the first of the models directory ([`models`]; ADR 0017): the
//! catalogue of the models that can be installed. Nothing installs yet: the plan of
//! checkpoint 1B (`docs/plans/`) builds the record of an installation, the installer,
//! the holds, removal and the inventory on it. It depends on the core and on nothing else in the
//! workspace.

pub mod models;
