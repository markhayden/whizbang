//! Implementations of the capability interfaces (`SPEC.md` section 3): backend kinds,
//! the pack client, provider adapters, and model installations.
//!
//! So far it holds the first of the models directory ([`models`]; ADR 0017): the
//! names of a model and of its files, and what a refusal says. Nothing installs yet:
//! the plan of checkpoint 1B (`docs/plans/`) builds the catalogue, the installer, the
//! holds, removal and the inventory on these. It depends on the core and on nothing else in the
//! workspace.

pub mod models;
