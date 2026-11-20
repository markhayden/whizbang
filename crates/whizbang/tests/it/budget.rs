//! `budget::…`: the loop's one reservation (plan m4, task L1, item 7; ADR 0039, "One
//! budget"), through the binary's wiring with the real ledger and the meter, on the
//! fixture of `checked`: the plan fitted to the cap of one call, what is reserved and
//! by whom, what comes back between attempts, and what happens when it runs out.

use std::sync::atomic::Ordering;

use whizbang_core::contract::record::{Decision, RunRecord, stored};
use whizbang_core::contract::{
    CallError, Capability, DESCENDANTS_LIMIT, Executor, Outcome, Refusal, RefusalKind, Request,
    Stale,
};

use crate::checked::{
    CHECK_BOUND, FILL_BOUND, JUDGE_BOUND, Looped, SEGMENT_BOUND, Setup, bound_of, calls, looped,
    passes, remains,
};

/// Every run the store holds a record of, by its id.
fn records(world: &Looped) -> Vec<(String, RunRecord)> {
    let reader = world.reader();
    let generation = reader.snapshot().unwrap();
    (generation.runs().keys())
        .map(|id| {
            let record = stored(&generation, id, |entry| reader.read(entry));
            (id.clone(), record.unwrap().unwrap())
        })
        .collect()
}

/// What every request of every run of the store settled, together.
fn settled(world: &Looped) -> u64 {
    let records = records(world);
    let paid = records
        .iter()
        .flat_map(|(_, record)| record.spend().requests.iter());
    paid.map(|paid| paid.settled_micros()).sum()
}

/// A cap of one call of this many millionths of a US dollar, as a config writes it.
fn cap(micros: u64) -> Option<String> {
    Some(format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000))
}

#[test]
fn the_loop_reserves_once_for_its_plan_and_each_child_draws_on_it() {
    let (world, finished) = looped("budget-once", &Setup::answering(vec![remains(), passes()]));
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    // The loop's run reserved the bound of its plan: three attempts, each a fill and a
    // check at its bound, and the segmenter once.
    let own = world.record(finished.run().as_str());
    assert_eq!(own.spend().reserved_micros, Some(bound_of(3)));
    // The ledger holds one reservation, the loop's: no run under it reserved anything.
    // Each of the 25 runs more drew its share from the loop's allowance, and its
    // record says how much that share was.
    let all = records(&world);
    assert_eq!(all.len(), 26);
    let reader = world.reader();
    let generation = reader.snapshot().unwrap();
    let books = whizbang_core::spend::fold::fold(&generation, |entry| reader.read(entry));
    let reserved: Vec<&str> = books
        .reservations()
        .keys()
        .map(|run| run.as_str())
        .collect();
    assert_eq!(reserved, [finished.run().as_str()]);
    // What was dispatched under it names the five runs that sent a request.
    assert_eq!(books.runs().len(), 5);
    let shares = (all.iter()).filter(|(id, _)| id != finished.run().as_str());
    let largest = shares
        .filter_map(|(_, record)| record.spend().reserved_micros)
        .max();
    assert_eq!(largest, Some(FILL_BOUND));
    // The requests were paid by the runs that sent them: one of the segmenter, two of
    // the filler, two of the judge; and all of it is within what was reserved.
    let paid: usize = all
        .iter()
        .map(|(_, record)| record.spend().requests.len())
        .sum();
    assert_eq!(paid, 5);
    assert_eq!(
        settled(&world),
        SEGMENT_BOUND + 2 * (FILL_BOUND + JUDGE_BOUND)
    );
    assert!(settled(&world) <= bound_of(3));
}

#[test]
fn what_a_child_did_not_spend_is_back_before_the_next_attempt() {
    // A check may take sixteen turns and takes one: fifteen turns' worth of its share
    // is back with the loop when it ends, and the record says what each attempt
    // settled of the loop's allowance, measured when the attempt ended.
    let mut setup = Setup::answering(vec![remains(), passes()]);
    setup.fill_charges = Some(10_000);
    let (world, finished) = looped("budget-back", &setup);
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 2);
    assert_eq!(
        attempts.list[0].settled_micros,
        SEGMENT_BOUND + 10_000 + JUDGE_BOUND
    );
    assert_eq!(attempts.list[1].settled_micros, 10_000 + JUDGE_BOUND);
    assert!(attempts.list[0].settled_micros < SEGMENT_BOUND + FILL_BOUND + CHECK_BOUND);
    // So a reservation of exactly two attempts at their bound covers both.
    let mut setup = Setup::answering(vec![remains(), passes()]);
    setup.cap = cap(bound_of(2));
    setup.allowance = bound_of(2);
    let (world, finished) = looped("budget-exact", &setup);
    assert_eq!(world.attempts(&finished).list.len(), 2);
    assert_eq!(finished.verdict().unwrap().decision(), Decision::Pass);
}

#[test]
fn the_plan_is_the_most_attempts_the_call_cap_covers_and_says_asked_and_allowed() {
    // The cap covers two attempts at their bound, and not three.
    let mut setup = Setup::answering(vec![remains(); 3]);
    setup.cap = cap(bound_of(3) - 1);
    let world = Looped::new("budget-cap-two", &setup);
    let engine = world.engine();
    let prepared = engine.prepare(&world.call()).unwrap();
    let plan = prepared.plan().unwrap();
    let stated = plan.stated().unwrap();
    assert_eq!((stated.asked, stated.allowed, stated.checks), (3, 2, true));
    assert_eq!(
        plan.why(),
        Some("the call cap covers 2 of the 3 attempts asked")
    );
    assert_eq!(
        plan.line(),
        "plan: 2 of 3 attempts asked, checked; fill 2; judge 32; the call cap covers 2 of \
         the 3 attempts asked"
    );
    assert_eq!(prepared.estimate().micros(), bound_of(2));
    // It runs two, though the judge would send it on: nothing is paid past the plan.
    let finished = engine.submit(&world.call()).unwrap();
    drop(engine);
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 2);
    assert_eq!(attempts.ended, "the last attempt the plan allows");
    assert_eq!(world.kit.requests(), (1, 2, 2));
    let record = world.record(finished.run().as_str());
    assert_eq!(record.spend().reserved_micros, Some(bound_of(2)));
    assert!(settled(&world) <= bound_of(2));
    // A cap that covers all three changes nothing, and gives no reason.
    let mut setup = Setup::answering(Vec::new());
    setup.cap = cap(bound_of(3));
    let world = Looped::new("budget-cap-three", &setup);
    let prepared = world.engine().prepare(&world.call()).unwrap();
    let plan = prepared.plan().unwrap();
    assert_eq!(plan.stated().unwrap().allowed, 3);
    assert_eq!(plan.why(), None);
    // And a call that asks two is planned for two, whatever the cap would cover.
    let two = crate::checked::asking(&world, 2);
    let prepared = world.engine().prepare(&two).unwrap();
    assert_eq!(prepared.estimate().micros(), bound_of(2));
}

#[test]
fn a_cap_that_covers_no_check_plans_one_attempt_unchecked() {
    // The cap covers a removal, and not a removal with its check.
    let mut setup = Setup::answering(vec![passes()]);
    setup.cap = cap(bound_of(1) - 1);
    let world = Looped::new("budget-cap-none", &setup);
    let engine = world.engine();
    let prepared = engine.prepare(&world.call()).unwrap();
    let plan = prepared.plan().unwrap();
    let stated = plan.stated().unwrap();
    assert_eq!((stated.asked, stated.allowed, stated.checks), (3, 1, false));
    assert_eq!(plan.why(), Some("the call cap does not cover a check"));
    assert_eq!(prepared.estimate().micros(), SEGMENT_BOUND + FILL_BOUND);
    assert!(
        prepared
            .needs()
            .iter()
            .all(|need| need.capability() != Capability::Judge)
    );
    let finished = engine.submit(&world.call()).unwrap();
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(world.kit.requests(), (1, 1, 0));
    let attempts = world.attempts(&finished);
    assert_eq!(
        attempts.list[0].unchecked.as_deref(),
        Some("the call cap does not cover a check")
    );
    // With the check required, the same call is planned checked and refused as over
    // the cap, as any call over its cap is.
    let required = world
        .call()
        .argument("check", whizbang_core::contract::Value::text("required"));
    let refused = engine.submit(&required).unwrap_err();
    let CallError::Refused(Refusal::ByPart { kind, .. }) = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(*kind, RefusalKind::OverAllowance);
    drop(engine);
    // A cap that does not cover one removal refuses the call.
    let mut setup = Setup::answering(Vec::new());
    setup.cap = cap(SEGMENT_BOUND + FILL_BOUND - 1);
    let world = Looped::new("budget-cap-nothing", &setup);
    let refused = world.engine().submit(&world.call()).unwrap_err();
    let CallError::Refused(Refusal::ByPart { kind, .. }) = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(*kind, RefusalKind::OverAllowance);
    assert_eq!(world.kit.requests(), (0, 0, 0));
}

#[test]
fn a_judges_key_that_goes_between_prepare_and_admit_is_a_stale_preparation() {
    let world = Looped::new("budget-stale", &Setup::answering(vec![passes()]));
    let prepared = world.engine().prepare(&world.call()).unwrap();
    assert!(prepared.plan().unwrap().stated().unwrap().checks);
    // The judge is no longer ready: the plan that would be made now is one unchecked
    // pass. The preparation is refused, and no plan is fitted at admission.
    world.kit.judge_ready.store(false, Ordering::SeqCst);
    let now = world.engine().prepare(&world.call()).unwrap();
    assert!(!now.plan().unwrap().stated().unwrap().checks);
    let request = Request::new(world.call()).expecting(&prepared);
    let refused = world.engine().request(&request).unwrap_err();
    let CallError::Refused(Refusal::Stale { why, .. }) = &refused else {
        panic!("{refused:?}");
    };
    assert!(
        matches!(why, Stale::Plan | Stale::Model { .. } | Stale::Config),
        "{why:?}"
    );
    assert_eq!(world.kit.requests(), (0, 0, 0));
    // And the other way: a judge that appears after an unchecked plan was prepared.
    world.kit.judge_ready.store(true, Ordering::SeqCst);
    let request = Request::new(world.call()).expecting(&now);
    let refused = world.engine().request(&request).unwrap_err();
    assert!(
        matches!(refused, CallError::Refused(Refusal::Stale { .. })),
        "{refused:?}"
    );
    assert_eq!(world.kit.requests(), (0, 0, 0));
}

#[test]
fn a_child_refused_for_want_of_allowance_ends_the_attempts_and_keeps_what_completed() {
    // The filler charges far over its bound, which puts the loop in debt: its next
    // call, the check, is refused for want of allowance.
    let mut setup = Setup::answering(vec![passes()]);
    setup.fill_charges = Some(bound_of(3) * 2);
    let (world, finished) = looped("budget-ran-out", &setup);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    assert_eq!(world.kit.requests(), (1, 1, 0));
    let attempts = world.attempts(&finished);
    assert_eq!(attempts.list.len(), 1);
    let why = attempts.list[0].unchecked.as_deref().unwrap();
    assert!(
        why.starts_with("the budget ran out: source-fidelity was not called"),
        "{why}"
    );
    assert_eq!(attempts.kept, Some(1));
    assert_eq!(attempts.ended, "the budget ran out after attempt 1");
    // The refusal is in the loop's record, and the loop started no further call.
    let record = world.record(finished.run().as_str());
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), 1);
    assert_eq!(calls(&record).len(), 3);
    assert!(finished.verdict().is_none());
    // Before the first fill has completed, it is the loop's error, of the type
    // `budget`, with the refusal's own sentence.
    let mut setup = Setup::answering(vec![passes()]);
    setup.segment_charges = Some(bound_of(3) * 2);
    let (world, finished) = looped("budget-ran-out-early", &setup);
    let Outcome::Error { kind, message } = finished.outcome() else {
        panic!("{:?}", finished.outcome());
    };
    assert_eq!(kind, "budget");
    assert!(
        message.contains("remove_prompt was not called: it may cost up to"),
        "{message}"
    );
    assert_eq!(world.kit.requests().1, 0);
    assert!(world.record(finished.run().as_str()).outputs().is_empty());
}

#[test]
fn three_attempts_at_their_largest_stay_inside_a_trees_runs_and_a_runs_calls() {
    use whizbang_squint::check::CALLS_LIMIT;
    // The most this loop makes is two attempts: a wider mask is used once. Counted.
    let (world, finished) = looped(
        "budget-count",
        &Setup::answering(vec![remains(), remains()]),
    );
    let record = world.record(finished.run().as_str());
    let own = calls(&record);
    assert_eq!(own.len(), 7);
    let all = records(&world);
    // The first attempt: the find and its two steps, the shape and its five, the fill
    // and its one, the check and its two. A retry is all of that but the find.
    let (first, retry) = (3 + 6 + 2 + 3, 6 + 2 + 3);
    assert_eq!(all.len(), 1 + first + retry);
    // Three attempts, each check dispatching every tool call a check may: under 200
    // runs, of the 1,024 a tree may hold; and no run makes more calls than the check
    // that dispatches them all, of the 256 one run may make.
    let largest = 1 + first + 2 * retry + 3 * CALLS_LIMIT;
    assert_eq!(largest, 133);
    let limits = [200, DESCENDANTS_LIMIT];
    assert!(limits.iter().all(|limit| largest < *limit), "{largest}");
    // Here the run that makes the most calls is the loop itself, with its seven; with
    // three attempts it makes ten, and a check at most as many as it may dispatch.
    let most_calls = (all.iter())
        .map(|(_, record)| record.steps().len())
        .max()
        .unwrap();
    assert_eq!(most_calls, own.len());
    let calls_of_one_run = [4 + 3 + 3, CALLS_LIMIT];
    assert!(calls_of_one_run.iter().all(|calls| *calls <= 256));
}
