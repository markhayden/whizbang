//! Squint in whizbang (`SPEC.md` section 3): its Rust tools, and later its rules and its
//! checklist.
//!
//! It holds three evidence tools for flat images, each as a function and as an action:
//! [`view`], [`resize`] and [`align`]; and [`check`], the judge loop: a judge looks at
//! images through the tools its run froze, after [`details`] has handed it what the tools
//! measured, and concludes.

pub mod align;
pub mod check;
pub mod details;
pub mod resize;
pub mod source_fidelity;
pub mod view;
