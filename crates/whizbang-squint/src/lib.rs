//! Squint in whizbang (`SPEC.md` section 3): its Rust tools, its rules and its
//! checklist.
//!
//! It holds one evidence tool, [`view`], for flat images, as a function and as an
//! action; [`check`], the judge loop: a judge looks at images through the tools its run
//! froze, and concludes; the five use cases as one table, [`use_case`]; and the job a
//! check judges, [`job`].

pub mod check;
pub mod job;
pub mod use_case;
pub mod view;
