//! Squint in whizbang (`SPEC.md` section 3): its Rust tools, and later its rules and its
//! checklist.
//!
//! So far it holds one evidence tool, [`view`], for flat images, as a function and as
//! an action; and [`check`], the smallest judge loop that proves the execution
//! contract: a judge looks at images through the tools its run froze, and concludes.

pub mod check;
pub mod view;
