//! The part of a store's recovery that ends the runs a dead process left (ADR 0015,
//! "The interrupted record, and what readers say of it"; the plan of checkpoint 1B,
//! "What a crashed run is recovered to").
//!
//! It reconstructs nothing. A run that was admitted and has no record, and that this
//! process does not have open, is ended as an error of the kind `interrupted`, with
//! a record made from its admission alone ([`RunRecord::interrupted`]); and where
//! its request came with an id, the id's ending is made, so that a repeat is
//! answered. The money is not this part's: a reservation is closed by the ledger's
//! own part of the recovery.

use super::engine::WHIZBANG_VERSION;
use super::record::{Admitted, Left, RECORD_NAME, RunRecord, Status};
use super::recover::{Recovering, RecoveryFailed, RecoveryPart};
use super::request::{self, ADMITTED_NAME, Admission, Ending};
use super::run::RunId;

/// Ends every run that was admitted, has no record and is not open in this process,
/// as an error of the kind `interrupted` (ADR 0013, "What is lost").
///
/// **Which runs.** A run with a name `admitted` and no name `record`, under an id
/// this build gives a run. A run that has its record ended, by itself or by an
/// earlier recovery, and is left as it is: so done twice this makes nothing. A run
/// with neither name was made by other hands, and is left too.
///
/// **From what.** Its admission, when that reads as the admission of this run; and
/// otherwise from its name alone, with the admission named as lost. An admission
/// that cannot be read is never a failure of the recovery: a store with one damaged
/// run would then have no writer at all.
///
/// **The request's ending** is made for a run whose admission was read and names a
/// request id, when the id's own record is that same admission and the id has no
/// ending. A run whose admission could not be read has none made: the id was in the
/// admission. Its id stays taken, and a request under it is answered with the
/// error of a record that is not read (ADR 0007).
#[derive(Debug, Clone, Copy, Default)]
pub struct Interrupted;

impl RecoveryPart for Interrupted {
    fn name(&self) -> &'static str {
        "interrupted"
    }

    fn recover(&self, step: &mut Recovering<'_, '_>) -> Result<(), RecoveryFailed> {
        let generation = step.generation().clone();
        for (id, held) in generation.runs() {
            let Some(admitted) = held.get(ADMITTED_NAME) else {
                continue;
            };
            if held.get(RECORD_NAME).is_some() || step.open_here(id) {
                continue;
            }
            // A run under an id this build does not give was made by other hands:
            // no record of this build names it.
            let Ok(run) = RunId::parse(id) else {
                continue;
            };
            let read = request::admitted(&generation, id, |entry| step.read(entry));
            let admission = read.and_then(Result::ok);
            let end_ms = step.now_ms();
            let from = |admission: Option<&Admission>| {
                RunRecord::interrupted(Left {
                    run: &run,
                    whizbang: WHIZBANG_VERSION,
                    admitted: admission.map(|admission| Admitted {
                        request: admission.request(),
                        parent: admission.parent(),
                        action: admission.action(),
                        version: admission.version(),
                        reserved_micros: admission.reserved_micros(),
                        start_ms: admission.start_ms(),
                    }),
                    end_ms,
                })
            };
            // An admission that reads and would make a record its own reader
            // refuses is no better than one that does not read.
            let (record, admission) = match from(admission.as_ref()) {
                Some(record) => (record, admission),
                None => match from(None) {
                    Some(record) => (record, None),
                    None => continue,
                },
            };
            step.make_in(id, RECORD_NAME, &record.to_bytes())?;
            // The id's ending, where the id's record is this run's own admission:
            // the two were published as one content (ADR 0007).
            let Some(request) = admission.as_ref().and_then(Admission::request) else {
                continue;
            };
            let own = generation
                .get(&request.admission_name())
                .is_some_and(|entry| entry.digest == admitted.digest);
            let name = request.ending_name();
            if own && generation.get(&name).is_none() {
                let ending = Ending::new(run.clone(), request.clone(), Status::Error, end_ms);
                step.make(&name, &ending.to_bytes())?;
            }
        }
        Ok(())
    }
}
