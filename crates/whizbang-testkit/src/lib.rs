//! Fakes of the core's interfaces. A development dependency only: nothing that ships
//! depends on this crate.

pub mod around;
pub mod barrier;
pub mod binary;
pub mod capabilities;
pub mod downloads;
pub mod echo;
pub mod fs_shim;
pub mod inventory;
pub mod kinds;
pub mod ledger;
pub mod loopback;
pub mod part;
pub mod recover;
pub mod schema;
pub mod scratch;
pub mod sending;
pub mod source;
pub mod spend;
pub mod threads;
pub mod worker;
