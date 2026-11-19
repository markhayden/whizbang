//! Sets, references, runs, statistics and gates (SPEC.md section 3).
//!
//! The comparison the baseline gate of milestone 2 uses; and the evaluation sets and the
//! matrices run over them (plan 2C, task E1); the measures, the paired statistics and the
//! gates (task E2).

pub mod baseline;
pub mod bootstrap;
pub mod gate;
mod json;
pub mod matrix;
pub mod metrics;
pub mod sets;
pub mod wilson;
