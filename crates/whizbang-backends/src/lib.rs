//! Implementations of the capability interfaces (`SPEC.md` section 3): backend kinds,
//! the pack client, provider adapters, and model installations.
//!
//! So far it holds the models directory ([`models`]; ADR 0017): the catalogue, the
//! record of an installation and its identity, the installer, the holds, removal,
//! the inventory and the lease, each against a download that a caller gives. No
//! backend kind, no pack client and no provider adapter is here yet: [`pack`] holds
//! the pack protocol's codec (ADR 0020) and [`net`] is a root only, where the plan of
//! checkpoint 1C builds the pack client and the download client. It depends on the core and on nothing else in the workspace.

pub mod models;
pub mod net;
pub mod pack;
