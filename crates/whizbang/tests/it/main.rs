// Test helpers outside `#[test]` functions may unwrap; a failure there is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod admitted;
mod anthropic;
mod backends;
mod baseline;
mod budget;
mod bundle;
mod cache;
mod catalogue;
mod check;
mod checked;
mod cli;
mod coverage;
mod credential;
mod describe;
mod door;
mod edits;
mod escape;
mod evaluation;
mod evidence;
mod executor;
mod export;
mod fal;
mod fetch;
mod gate;
mod google;
mod guides;
mod import;
mod judges;
mod lab;
mod labs;
mod located;
mod loop_score;
mod loop_sheet;
mod marks;
mod matrix;
mod model_tier;
mod models;
mod nested;
mod openai;
mod pack;
mod pairs_page;
mod parity;
mod play;
mod play_bundles;
mod play_chain;
mod play_compare;
mod play_examples;
mod play_library;
mod play_models;
mod play_replay;
mod play_result;
mod prepare;
mod rascal;
mod recipe;
mod record;
mod release;
mod remote;
mod remote_parity;
mod replay;
mod runner;
mod runs;
mod runs_bundle;
mod segment;
mod select;
mod serve;
mod serve_http;
mod sets;
mod setup;
mod spend;
mod stats;
mod stopped;
mod store;
mod story;
mod table;
mod tool_file;
mod tool_records;
mod tool_worker;
mod transcript;
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
