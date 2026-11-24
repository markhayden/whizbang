//! The MCP server over stdio (`SPEC.md` sections 1 and 3; plan m2c, task M1; ADR 0031):
//! the door a local agent uses. It speaks the protocol's frames by hand, JSON-RPC 2.0
//! one message to a line, and offers each action as a tool. What it runs, it runs
//! through the executor it is given, by the executor's one entry point, so a call of a
//! tool passes every check a call from the command line passes: preparation, admission
//! and spend, and the gate on what leaves the machine. The MCP types stay in this crate.
//!
//! - [`frame`]: a message read from the input, bounded in its size and its nesting.
//! - [`tools`]: an action's descriptor as a tool, and a tool's arguments as a call.
//! - [`serve`]: the loop: what a client asks, the runs it started, progress, a stated
//!   time for each run, cancellation, and the end of the input.
//! - [`play`]: the playground host over loopback HTTP (plan m5, task H1; ADR 0041): its
//!   guard, its routes, the runs it started, and stopping.
//! - [`engine`]: the engine API, MCP over HTTP with plain routes for pictures and runs
//!   (plan m6, task H1; ADR 0045): its guard, its readers and handlers, its runs, stopping.
//! - [`http`]: the HTTP/1.1 both surfaces read and write.

pub mod engine;
pub mod frame;
pub mod http;
pub mod play;
mod server;
mod threads;
pub mod tools;

pub use server::{CALLS_LIMIT, Door, Limits, PROTOCOL_VERSIONS, Said, Tool, escaped, serve};
pub use threads::READERS_LIMIT;
