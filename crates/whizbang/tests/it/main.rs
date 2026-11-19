// Test helpers outside `#[test]` functions may unwrap; a failure there is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod anthropic;
mod backends;
mod baseline;
mod cache;
mod catalogue;
mod check;
mod cli;
mod coverage;
mod credential;
mod describe;
mod escape;
mod evidence;
mod executor;
mod fetch;
mod gate;
mod google;
mod import;
mod matrix;
mod model_tier;
mod models;
mod openai;
mod pack;
mod parity;
mod prepare;
mod rascal;
mod record;
mod runs;
mod segment;
mod select;
mod serve;
mod setup;
mod spend;
mod store;
mod tool_file;
mod tool_worker;
mod wired;
mod writer;

use std::sync::Arc;

use whizbang::wiring::Over;
use whizbang_core::artifact::Contents;
use whizbang_core::contract::{Engine, Registry};
use whizbang_core::store::Store;

/// The binary's executor that runs actions, as a command that names no config builds
/// it: over these actions and this store, and nothing else.
fn runner(registry: Registry, store: Arc<Store>) -> Engine {
    whizbang::wiring::runner(registry, store, &Over::nothing()).expect("the executor builds")
}

/// The binary's executor that only prepares, built the same way over a read of a store.
fn preparer(registry: Registry, store: Arc<dyn Contents + Send + Sync>) -> Engine {
    whizbang::wiring::executor(registry, store, &Over::nothing()).expect("the executor builds")
}
