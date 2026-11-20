//! `stopped::…`: the ways out of the loop `remove_checked` that no test of its first
//! task reached (`docs/carry-over.md`, the row on its untested branches; plan m4, task
//! L2), on the fixture of `checked`: a child that ends cancelled when the loop was not;
//! a retry whose mask cannot be made; a check that hands back no verdict; a budget that
//! runs out inside a retry; a cancellation seen after the last call came back; and a
//! deadline that passes before the first fill.

use std::sync::Arc;
use std::time::{Duration, Instant};

use whizbang::wiring;
use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::record::{ATTEMPTS_NOT_ENDED, Decision, Status, stored};
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Argument, Cancel, Capability, Context, Count, Executor,
    Failure, InputPort, Kind, Outcome, PreparedCall, Produced, Registry, Request, Requirement,
    ResultKind, RunId, RunState, ValueType,
};
use whizbang_rascal::checked::{CHECK, NAME, RemoveChecked};
use whizbang_testkit::barrier::Barriers;

use crate::checked::{Looped, Setup, bound_of, looped, passes, remains};
use crate::located::{PLACE, remains_at};

/// The runs of the store that have no record yet, but for `own`, in the order they
/// were made: a run's id is the millisecond it was made in and a count that goes up
/// with each run.
fn running(world: &Looped, own: &RunId) -> Vec<RunId> {
    let reader = world.reader();
    let generation = reader.snapshot().unwrap();
    let mut running: Vec<(u64, u64)> = (generation.runs().keys())
        .filter(|id| id.as_str() != own.as_str())
        .filter(|id| {
            let read = stored(&generation, id, |entry| reader.read(entry));
            !matches!(read, Some(Ok(_)))
        })
        .map(|id| {
            let (at, count) = id.split_once('-').unwrap();
            (at.parse().unwrap(), count.parse().unwrap())
        })
        .collect();
    running.sort_by_key(|made| made.1);
    let id = |(at, count): (u64, u64)| RunId::parse(&format!("{at}-{count}")).unwrap();
    running.into_iter().map(id).collect()
}

#[test]
fn a_child_cancelled_when_the_loop_was_not_is_a_call_that_stopped() {
    // The fill of the second attempt is held with the filler, and its run, not the
    // loop's, is cancelled.
    let barriers = Barriers::new();
    let mut setup = Setup::answering(vec![remains(), passes()]);
    setup.holds = Some(barriers.clone());
    let world = Looped::new("stopped-child", &setup);
    let engine = world.engine();
    let Ok(Answer::Started(run)) = engine.start(&Request::new(world.call())) else {
        panic!("the loop was not started");
    };
    barriers.release("fill");
    barriers.arrived("fill", 2);
    // Two runs under the loop have not ended: the stretch that fills, and its step.
    // The stretch is cancelled, by its own id: the loop's signal stays unset.
    let under = running(&world, &run);
    assert_eq!(under.len(), 2, "{under:?}");
    assert_eq!(engine.cancel(&under[0]), Ok(Cancel::Accepted));
    barriers.release("fill");
    let waited = engine.wait(&run).unwrap();
    drop(engine);
    // The loop was not cancelled and its deadline did not pass: the call stopped, of
    // its own, and the retry leaves the attempt before it kept.
    let RunState::Ended(Outcome::Completed { .. }) = waited.state() else {
        panic!("{:?}", waited.state());
    };
    let record = world.record(run.as_str());
    assert_eq!(record.outcome().status, Status::Completed);
    let attempts = record.attempts().unwrap();
    assert_eq!(attempts.list.len(), 2);
    assert_eq!(
        attempts.list[1].unchecked.as_deref(),
        Some(
            "could not be filled: an error of the type cancelled: the call of remove_prompt \
             was cancelled"
        )
    );
    assert_eq!(attempts.kept, Some(1));
    assert_eq!(
        attempts.ended,
        "the retry could not be filled, and the attempts before it stand"
    );
    assert_eq!(
        record.verdict().map(|verdict| verdict.0),
        Some(Decision::Fail)
    );
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn a_retry_whose_mask_cannot_be_made_keeps_the_attempt_before_it() {
    // The judge names a place, and the segmenter fails when it is asked about the crop.
    let mut setup = Setup::answering(vec![remains_at(PLACE), passes()]);
    setup.inside_fails = true;
    let (world, finished) = looped("stopped-no-mask", &setup);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(finished.verdict().unwrap().decision(), Decision::Fail);
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 2);
    let why = attempts.list[1].unchecked.as_deref().unwrap();
    assert!(
        why.starts_with("could not make its mask: an error of the type segment: "),
        "{why}"
    );
    // The attempt lists the cut and the request that failed, made no fill, and says
    // nothing of crops it did not come to use.
    assert_eq!(attempts.list[1].runs.len(), 2);
    assert_eq!(attempts.list[1].changed, None);
    assert_eq!(attempts.kept, Some(1));
    assert_eq!(
        attempts.ended,
        "the retry could not make its mask, and the attempts before it stand"
    );
    assert_eq!(world.kit.requests(), (2, 1, 1));
    assert_eq!(world.kit.unplayed(), 1);
}

/// An action of the name of squint's checking recipe that completes and hands back no
/// verdict and no findings: what the shipped recipe never does.
struct Silent(ActionDescriptor);

impl Silent {
    fn new() -> Silent {
        let draft = ActionDescriptor::draft(
            CHECK,
            "1.0.0",
            Kind::Primitive,
            "Completes, and says nothing of what it was given.",
        )
        .argument(Argument::required("job", "The job.", ValueType::Text))
        .argument(Argument::optional("judge", "The judge.", ValueType::Text))
        .input(InputPort::variadic(
            "images",
            ArtifactType::Image,
            "The candidate and the source.",
            Count::between(1, 2),
        ))
        .results(ResultKind::Verdict)
        .results(ResultKind::Findings)
        .needs(Requirement::new(Capability::Judge).chosen_by("judge"))
        .concurrent(true);
        Silent(draft.build().unwrap())
    }
}

impl Action for Silent {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        Ok(Produced::nothing())
    }
}

#[test]
fn a_check_that_hands_back_no_verdict_leaves_its_attempt_unchecked() {
    let world = Looped::new("stopped-silent", &Setup::answering(vec![passes()]));
    // Every action of the build, with the check's recipe replaced and the loop built
    // over that.
    let build = wiring::registry_over(&world.over, false).unwrap();
    let mut registry = Registry::new();
    for descriptor in build.descriptors() {
        let name = descriptor.name();
        if name != CHECK && name != NAME {
            registry.register(build.get(name).unwrap().clone()).unwrap();
        }
    }
    registry.register(Arc::new(Silent::new())).unwrap();
    let over = RemoveChecked::new(&registry, &registry).unwrap();
    registry.register(Arc::new(over)).unwrap();
    let engine = world.engine_of(registry);
    let finished = engine.submit(&world.call()).unwrap();
    drop(engine);
    // The fill is handed back, unchecked, and the attempts end: nothing allows a retry.
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert!(finished.verdict().is_none() && finished.findings().is_none());
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 1);
    assert_eq!(
        attempts.list[0].unchecked.as_deref(),
        Some("the check handed back no verdict")
    );
    assert_eq!(attempts.list[0].runs.len(), 4);
    assert_eq!(attempts.kept, Some(1));
    assert_eq!(attempts.ended, "the check of attempt 1 could not run");
    assert_eq!(world.kit.requests(), (1, 1, 0));
}

#[test]
fn a_budget_that_runs_out_inside_a_retry_keeps_the_attempt_before_it() {
    // The segmenter charges far over its bound for the crop of the retry, which puts
    // the loop in debt: the next call of the retry is refused for want of allowance.
    let mut setup = Setup::answering(vec![remains_at(PLACE), passes()]);
    setup.inside = vec![[4, 4, 10, 10]];
    setup.inside_charges = Some(bound_of(3) * 2);
    let (world, finished) = looped("stopped-budget", &setup);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(finished.verdict().unwrap().decision(), Decision::Fail);
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 2);
    let why = attempts.list[1].unchecked.as_deref().unwrap();
    assert!(why.contains(" was not called: it may cost up to "), "{why}");
    assert_eq!(attempts.kept, Some(1));
    assert_eq!(attempts.ended, "the budget ran out after attempt 1");
    // No second fill was asked, and the refusal is in the loop's record.
    assert_eq!(world.kit.requests(), (2, 1, 1));
    let record = world.record(finished.run().as_str());
    assert_eq!(record.outcome().refusals.len(), 1);
    assert_eq!(world.kit.unplayed(), 1);
}

#[test]
fn a_loop_cancelled_as_its_check_ends_commits_nothing_and_keeps_no_attempt() {
    // The loop is cancelled while its last call is ending: the judge has answered.
    let barriers = Barriers::new();
    let mut setup = Setup::answering(vec![passes()]);
    setup.holds = Some(barriers.clone());
    let world = Looped::new("stopped-late", &setup);
    let engine = Arc::new(world.engine());
    let Ok(Answer::Started(run)) = engine.start(&Request::new(world.call())) else {
        panic!("the loop was not started");
    };
    barriers.arrived("fill", 1);
    let (cancels, cancelled) = (engine.clone(), run.clone());
    world.kit.after_judge(move || {
        let _ = cancels.cancel(&cancelled);
    });
    barriers.release("fill");
    let waited = engine.wait(&run).unwrap();
    assert_eq!(waited.state(), &RunState::Ended(Outcome::Cancelled));
    drop(engine);
    // Nothing is committed, and the record says that no attempt was kept: the fill
    // completed and is listed, with why it has no verdict.
    let record = world.record(run.as_str());
    assert!(record.outputs().is_empty());
    let attempts = record.attempts().unwrap();
    assert_eq!(attempts.list.len(), 1);
    assert_eq!(
        attempts.list[0].unchecked.as_deref(),
        Some("the run was stopped before its check ended")
    );
    assert_eq!(
        (attempts.kept, attempts.ended.as_str()),
        (None, ATTEMPTS_NOT_ENDED)
    );
    assert_eq!(world.kit.requests(), (1, 1, 1));
}

#[test]
fn a_deadline_that_passes_before_the_first_fill_is_the_loops_error() {
    // The first fill is answered only once the run's deadline has passed.
    let mut setup = Setup::answering(vec![passes()]);
    setup.fill_waits_at = Some(1);
    let world = Looped::new("stopped-deadline", &setup);
    let engine = world.engine();
    let request = Request::new(world.call()).until(Instant::now() + Duration::from_millis(300));
    let Ok(Answer::Ran(finished)) = engine.request(&request) else {
        panic!("the loop did not run");
    };
    drop(engine);
    // No fill completed: the loop ends as an error of the type `deadline`, hands back
    // nothing and keeps no attempt.
    let Outcome::Error { kind, message } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert_eq!(
        (kind.as_str(), message.as_str()),
        ("deadline", "the loop stopped: its deadline passed")
    );
    let record = world.record(finished.run().as_str());
    assert!(record.outputs().is_empty());
    assert!(
        record
            .attempts()
            .is_none_or(|attempts| attempts.kept.is_none())
    );
    assert_eq!(world.kit.requests().2, 0);
}
