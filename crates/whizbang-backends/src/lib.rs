//! Implementations of the capability interfaces (`SPEC.md` section 3): backend kinds,
//! the pack client, provider adapters, and model installations.
//!
//! It holds the models directory ([`models`]; ADR 0017): the catalogue, the record of
//! an installation and its identity, the installer, the holds, removal, the inventory
//! and the lease, each against a download that a caller gives; [`pack`], the pack
//! protocol's codec and its worker (ADR 0020); [`net`], the download client and the
//! sender (ADR 0024); and the judge of a service, the kind [`service`] and its three
//! providers' adapters, [`anthropic`], [`google`] and [`openai`]; and, under the same
//! kind, the filler of [`fal`]. It depends on the
//! core and on nothing else in the workspace, and, of crates outside it, on `rustls`
//! and `webpki-roots` for the download client's TLS, and `image` to scale what fal's
//! filler sends and is answered.

pub mod anthropic;
pub mod fal;
pub mod google;
pub mod models;
pub mod net;
pub mod openai;
pub mod pack;
pub mod service;
