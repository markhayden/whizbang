//! The execution contract (`SPEC.md` section 3, ADR 0003), part one of task A5: the action
//! descriptor, the rule for the text a description prints, registration, and the static
//! description. Part two: preparing a call (ADR 0004). Part three: the run record, and
//! running an action (ADR 0005). And request ids and the admission of a run (ADR 0007). And the commit step, in
//! which a run's outputs and its completed status are published, a run's status
//! while it executes, and cancelling one (ADR 0008).

mod admit;
mod cancel;
mod commit;
mod describe;
mod descriptor;
mod execute;
mod fixtures;
mod prepare;
mod record;
mod redact;
mod registry;
mod run;
mod text;
mod watch;
