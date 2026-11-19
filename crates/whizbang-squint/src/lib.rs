//! Squint in whizbang (`SPEC.md` section 3): its Rust tools, its rules and its
//! checklist.
//!
//! It holds three evidence tools for flat images, each as a function and as an action:
//! [`view`], [`resize`] and [`align`]; [`check`], the judge loop: a judge looks at images
//! through the tools its run froze, after [`details`] has handed it what the tools
//! measured, and concludes; the five use cases as one table, [`use_case`]; and the job a
//! check judges, [`job`].

pub mod align;
pub mod check;
pub mod compare;
pub mod details;
pub mod job;
pub mod marks;
pub mod replay;
pub mod resize;
pub mod use_case;
pub mod view;
