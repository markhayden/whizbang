//! The execution contract (`SPEC.md` section 3, ADR 0003), part one of task A5: the action
//! descriptor, the rule for the text a description prints, registration, and the static
//! description. Part two: preparing a call (ADR 0004). Part three: the run record, and
//! running an action (ADR 0005). And request ids and the admission of a run (ADR 0007). And the commit step, in
//! which a run's outputs and its completed status are published, a run's status
//! while it executes, cancelling one, and nested calls (ADR 0008). And what a run asks
//! of its capabilities, and the calls a run froze (ADR 0009).

mod admit;
mod asked;
mod cancel;
mod commit;
mod describe;
mod descriptor;
mod execute;
mod fixtures;
mod nested;
mod prepare;
mod record;
mod record3;
mod redact;
mod registry;
mod run;
mod scope;
mod started;
mod text;
mod watch;
