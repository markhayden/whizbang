//! Tests of `whizbang spend status` and `reset` (plan 1B, task S9): the commands as
//! the binary calls them, over stores that an executor with the real ledger wrote and
//! that other hands then changed. An executor here has one action, `paint`, a call
//! of which may cost a quarter of a dollar, and the test counts its runs: nothing
//! that `status` or `reset` does runs it again.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use whizbang::wiring::{self, Make};
use whizbang_core::capability::Stage;
use whizbang_core::contract::{
    Action, ActionDescriptor, Backends, Call, CallError, Capability, Context, Engine, Estimate,
    Executor, Failure, Kind as ActionKind, ModelIdentity, PreparedCall, Produced, Recovery,
    RecoveryPart, Refusal, Registry, Requirement, RunId, Unavailable,
};
use whizbang_core::identity::Digest;
use whizbang_core::spend::{Amount, ClosedBy, PlaceBy, Record, StoreLedger, Sum, TurnBy};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Reader};
use whizbang_testkit::recover::FakeRecovery;
use whizbang_testkit::scratch::Scratch;

use crate::spend::{self, Command};
use crate::store::Where;

/// What one call of `paint` may cost, in millionths: a quarter of a dollar.
const QUARTER: u64 = 250_000;

/// Backends with one model for every capability, whose every request may cost a
/// quarter.
struct Quarter;

impl Backends for Quarter {
    fn resolve(
        &self,
        _: Capability,
        _: Option<&str>,
        _: &[String],
    ) -> Result<ModelIdentity, Unavailable> {
        Ok(ModelIdentity::new("fake", "fake-model", "1"))
    }

    fn bound(&self, _: Capability, _: &ModelIdentity) -> Estimate {
        Estimate::of_micros(QUARTER)
    }
}

/// An action that needs a filler, does nothing, and counts its runs.
struct Paint {
    descriptor: ActionDescriptor,
    runs: Arc<AtomicU64>,
}

impl Action for Paint {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        Ok(Produced::nothing())
    }
}

/// A store in a scratch folder, and how many times `paint` ran over it.
struct World {
    _dir: Scratch,
    root: PathBuf,
    runs: Arc<AtomicU64>,
}

impl World {
    /// A store that holds nothing.
    fn new(label: &str) -> World {
        let dir = Scratch::new(label);
        let root = dir.path().join("store");
        let opening = wiring::opening(&root, Make::TheRoot, Recovery::none()).unwrap();
        drop(opening.writer().unwrap());
        World {
            _dir: dir,
            root,
            runs: Arc::new(AtomicU64::new(0)),
        }
    }

    fn ran(&self) -> u64 {
        self.runs.load(Ordering::SeqCst)
    }

    /// One call of `paint` through an executor with the real ledger, which is gone
    /// again when this returns: the run it made, or the refusal.
    fn paint(&self) -> Result<RunId, CallError> {
        let opening = wiring::opening(&self.root, Make::Nothing, Recovery::none()).unwrap();
        let store = opening.writer().unwrap();
        let draft = ActionDescriptor::draft("paint", "1.0.0", ActionKind::Primitive, "Paints.");
        let descriptor = draft.needs(Requirement::new(Capability::Fill)).build();
        let mut registry = Registry::new();
        let paint = Paint {
            descriptor: descriptor.unwrap(),
            runs: self.runs.clone(),
        };
        registry.register(Arc::new(paint)).unwrap();
        let engine = Engine::over(registry, Arc::new(Quarter), store, Digest::of_bytes(b""))
            .with_ledger(Arc::new(StoreLedger));
        let finished = engine.submit(&Call::new("paint"))?;
        Ok(finished.run().clone())
    }

    /// Other hands make these records, each under its own name, in one publication.
    fn plant(&self, records: &[Record]) {
        let named: Vec<(String, Vec<u8>)> = records
            .iter()
            .map(|record| (record.name().to_string(), record.to_bytes()))
            .collect();
        self.plant_bytes(&named);
    }

    /// The same, of names with any content.
    fn plant_bytes(&self, named: &[(String, Vec<u8>)]) {
        let opening = wiring::opening(&self.root, Make::Nothing, Recovery::none()).unwrap();
        let store = opening.writer().unwrap();
        let mut publication = store.begin().unwrap();
        for (name, bytes) in named {
            publication.create(name, Kind::Diagnostic, bytes).unwrap();
        }
        publication.publish().unwrap();
    }

    fn status(&self) -> String {
        spend::run(&status(&self.root, None)).unwrap()
    }

    fn of_run(&self, run: &str) -> Result<String, String> {
        spend::run(&status(&self.root, Some(run)))
    }

    fn reset(&self, allowance: &str) -> Result<String, String> {
        spend::run(&reset(&self.root, allowance))
    }

    /// The store's generation number and every name it holds outside runs.
    fn held(&self) -> (u64, Vec<String>) {
        let reader = Reader::open(&self.root, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let names = generation.entries().keys().cloned().collect();
        (generation.number(), names)
    }
}

fn status(root: &Path, run: Option<&str>) -> Command {
    Command::Status {
        at: Where::at(root),
        run: run.map(str::to_owned),
    }
}

fn reset(root: &Path, allowance: &str) -> Command {
    Command::Reset {
        at: Where::at(root),
        allowance: allowance.to_owned(),
    }
}

fn run_id(text: &str) -> RunId {
    RunId::parse(text).unwrap()
}

fn n(number: u64) -> NonZeroU64 {
    NonZeroU64::new(number).unwrap()
}

fn amount(micros: u64) -> Amount {
    Amount::new(micros).unwrap()
}

/// The line of a status that names this run.
fn line_of<'a>(said: &'a str, run: &RunId) -> &'a str {
    let named = |line: &&str| line.trim_start().starts_with(run.as_str());
    said.lines()
        .find(named)
        .unwrap_or_else(|| panic!("no line for {run} in:\n{said}"))
}

/// Whether the call was refused because the allowance does not cover it, or because
/// the epoch admits nothing until a reset.
fn refused(answer: Result<RunId, CallError>) -> &'static str {
    match answer {
        Err(CallError::Refused(Refusal::OverAllowance { .. })) => "over the allowance",
        Err(CallError::Refused(Refusal::NeedsOwner { .. })) => "needs its owner",
        other => panic!("not refused by the ledger: {other:?}"),
    }
}

// --- the four states a reset is the way out of --------------------------------------------------

/// A store with an allowance of one call, and that call made: its reservation stays
/// open, since nothing closes one yet, and holds all of the allowance.
fn one_call(label: &str) -> (World, RunId) {
    let world = World::new(label);
    let found = world.held().0;
    assert_eq!(
        world.reset("0.25").unwrap(),
        format!(
            "allowance reset: $0.25\nepoch: 1\ngeneration: {}\n",
            found + 1
        )
    );
    let run = world.paint().unwrap();
    assert_eq!(world.ran(), 1);
    (world, run)
}

/// After the state is named: nothing more is admitted, nothing was sent a second
/// time by looking or by resetting, and after the reset the next call that the new
/// allowance covers is admitted.
fn a_reset_is_the_way_out(world: &World, why: &str) {
    assert_eq!(refused(world.paint()), why);
    assert_eq!(world.ran(), 1, "a refused call ran");
    let before = world.held();
    world.status();
    assert_eq!(world.held(), before, "status reads only");
    let said = world.reset("0.25").unwrap();
    assert!(
        said.starts_with("allowance reset: $0.25\nepoch: "),
        "{said}"
    );
    assert_eq!(world.ran(), 1, "a reset sent something again");
    let next = world.paint().unwrap();
    assert_eq!(world.ran(), 2);
    // The new call is the one thing counted against the new allowance.
    let said = world.status();
    assert!(said.contains("allowance: $0.25\n"), "{said}");
    assert!(said.contains("counted against it: $0.25\n"), "{said}");
    assert!(said.contains("left: $0.00\n"), "{said}");
    assert!(line_of(&said, &next).contains("  open  counts for $0.25"));
}

#[test]
fn an_open_reservation_a_dead_process_left_with_a_dispatch_is_named_and_a_reset_clears_it() {
    let (world, run) = one_call("spend-left-open");
    // Its request was dispatched and never settled, and its process is gone.
    world.plant(&[
        Record::Place {
            run: run.clone(),
            place: n(1),
            by: PlaceBy::Dispatch,
        },
        Record::Dispatch {
            run: run.clone(),
            place: n(1),
            by: run.clone(),
            request: 1,
            bound: amount(QUARTER),
            at_ms: 0,
        },
    ]);
    let said = world.status();
    assert!(said.starts_with("epoch: 1\nallowance: $0.25\n"), "{said}");
    assert!(said.contains("counted against it: $0.25\n"), "{said}");
    assert!(
        said.contains("reservations that count in epoch 1 and were not released: 1\n"),
        "{said}"
    );
    assert_eq!(
        line_of(&said, &run),
        format!(
            "  {run}  open  counts for $0.25  (reserved $0.25, charged $0.00, outstanding $0.25)"
        )
    );
    a_reset_is_the_way_out(&world, "over the allowance");
    // The earlier run's cost is still seen, by its run.
    let said = world.of_run(run.as_str()).unwrap();
    assert!(said.contains("  outstanding: $0.25\n"), "{said}");
}

#[test]
fn a_frozen_reservation_is_named_with_what_it_counts_for_and_a_reset_clears_it() {
    let (world, run) = one_call("spend-frozen");
    // A settlement that no dispatch and no guard explains: not a set the ledger
    // could have written.
    world.plant(&[Record::Settlement {
        run: run.clone(),
        place: n(1),
        stage: Stage::Answered,
        charged: amount(100_000),
        over_bound: false,
        at_ms: 0,
    }]);
    let said = world.status();
    // The amount, the charge, and the amount again for the place that holds no
    // dispatch: the most its names can mean.
    assert_eq!(
        line_of(&said, &run),
        format!(
            "  {run}  frozen  counts for $0.60  (reserved $0.25, charged $0.10, outstanding $0.00)"
        )
    );
    assert!(said.contains("counted against it: $0.60\n"), "{said}");
    assert!(said.contains("debt: $0.35\n"), "{said}");
    a_reset_is_the_way_out(&world, "over the allowance");
    // It is frozen still, and counts in the epoch the reset closed alone.
    let said = world.status();
    assert!(
        said.contains("frozen reservations that count in earlier epochs alone: 1\n"),
        "{said}"
    );
    assert!(line_of(&said, &run).ends_with("; in epoch 1"), "{said}");
}

#[test]
fn a_reservation_with_no_hold_is_named_as_unknown_and_counts_nowhere() {
    let (world, _) = one_call("spend-no-hold");
    // A name for a run that has no reservation: no hold says what was reserved,
    // or in which epoch.
    let stray = run_id("7000-99");
    world.plant(&[Record::Settlement {
        run: stray.clone(),
        place: n(1),
        stage: Stage::Unresolved,
        charged: amount(60_000),
        over_bound: false,
        at_ms: 0,
    }]);
    let said = world.status();
    assert!(
        said.contains(
            "reservations that count against no allowance, because no hold names an \
             epoch for them: 1\n"
        ),
        "{said}"
    );
    assert!(
        line_of(&said, &stray).starts_with("  7000-99  frozen  counts for unknown, at least $0.06"),
        "{said}"
    );
    // It is counted nowhere: what is counted is the first call, and the epoch
    // admits what its allowance covers, which is nothing more.
    assert!(said.contains("counted against it: $0.25\n"), "{said}");
    assert!(!said.contains("admits nothing"), "{said}");
    a_reset_is_the_way_out(&world, "over the allowance");
    let said = world.status();
    assert!(line_of(&said, &stray).contains("unknown, at least $0.06"));
}

#[test]
fn an_epoch_whose_turns_are_wrong_is_named_and_a_reset_opens_one_that_admits() {
    let (world, run) = one_call("spend-turns");
    // A turn that no publication of the ledger took.
    world.plant(&[Record::Turn {
        epoch: 1,
        turn: n(50),
        by: TurnBy::Admission,
        run: None,
    }]);
    let said = world.status();
    assert!(
        said.contains("epoch 1 admits nothing until a reset: its turns are not those"),
        "{said}"
    );
    assert!(said.contains("left: $0.00\n"), "{said}");
    assert!(line_of(&said, &run).contains("  open  counts for $0.25"));
    a_reset_is_the_way_out(&world, "needs its owner");
    // The older epoch is still named, and the newest admits.
    let said = world.status();
    assert!(
        said.contains("earlier epochs whose turns are not those their publications took: 1\n"),
        "{said}"
    );
    assert!(!said.contains("admits nothing"), "{said}");
}

// --- a hold that other hands made for an epoch not yet opened ----------------------------------

#[test]
fn a_hold_planted_for_the_epoch_a_reset_then_opens_counts_there_and_the_next_reset_passes_it() {
    let world = World::new("spend-planted");
    world.reset("0.25").unwrap();
    let planted = run_id("7000-77");
    world.plant(&[Record::Hold {
        epoch: 2,
        run: planted.clone(),
        reserved: amount(QUARTER),
    }]);
    // It counts nowhere yet: the first epoch admits its call.
    world.paint().unwrap();
    assert_eq!(world.ran(), 1);
    // The reset opens the epoch the hold names: it counts there, and its turn is
    // one that epoch's publications did not take.
    assert!(world.reset("0.25").unwrap().contains("epoch: 2\n"));
    let said = world.status();
    assert!(said.starts_with("epoch: 2\nallowance: $0.25\n"), "{said}");
    assert!(
        line_of(&said, &planted).starts_with("  7000-77  frozen  counts for $0.25"),
        "{said}"
    );
    assert!(said.contains("counted against it: $0.25\n"), "{said}");
    assert!(
        said.contains("epoch 2 admits nothing until a reset"),
        "{said}"
    );
    assert_eq!(refused(world.paint()), "needs its owner");
    // One more reset passes it.
    assert!(world.reset("0.25").unwrap().contains("epoch: 3\n"));
    world.paint().unwrap();
    assert_eq!(world.ran(), 2);
    let said = world.status();
    assert!(said.contains("counted against it: $0.25\n"), "{said}");
    assert!(line_of(&said, &planted).ends_with("; in epoch 2"), "{said}");
}

// --- reading while another writes, and what a reset keeps --------------------------------------

#[test]
fn status_answers_while_another_opening_holds_the_store_and_reset_is_refused_with_the_holder_named()
{
    let (world, run) = one_call("spend-held");
    let before = world.held();
    let holder = wiring::opening(&world.root, Make::Nothing, Recovery::none()).unwrap();
    let said = world.status();
    assert!(line_of(&said, &run).contains("  open  counts for $0.25"));
    let why = world.reset("5").unwrap_err();
    assert!(why.starts_with("the store at "), "{why}");
    assert!(why.contains(world.root.to_str().unwrap()), "{why}");
    // The holder as the store names it: the writer's own line, or "another
    // process" until it has written one; and what is true whoever holds it.
    assert!(why.contains(" is being written by "), "{why}");
    assert!(
        why.ends_with(
            "one process at a time may write a store (the writer that holds it is \
             another opening of the store, in another process or in this one)"
        ),
        "{why}"
    );
    assert_eq!(world.held(), before);
    drop(holder);
    world.reset("5").unwrap();
}

#[test]
fn after_a_reset_the_earlier_records_are_all_still_read_and_the_balances_start_again() {
    let (world, run) = one_call("spend-kept");
    let (found, before) = world.held();
    let said = world.reset("12.5").unwrap();
    assert_eq!(
        said,
        format!(
            "allowance reset: $12.50\nepoch: 2\ngeneration: {}\n",
            found + 1
        )
    );
    // One publication of three names: the reset, its guard, and the next turn of
    // the epoch it closes. Every earlier name is there still.
    let (number, after) = world.held();
    assert_eq!(number, found + 1);
    let made: Vec<&String> = after.iter().filter(|name| !before.contains(name)).collect();
    assert_eq!(
        made,
        ["spend/epoch/2", "spend/reset/2/12500000", "spend/turn/1/2"]
    );
    assert!(before.iter().all(|name| after.contains(name)));
    // The balances start again from the new allowance.
    let said = world.status();
    let head = "epoch: 2\nallowance: $12.50\ncounted against it: $0.00\n  charged: $0.00\n  \
                held beyond that: $0.00\n  unresolved: $0.00\nleft: $12.50\ndebt: $0.00\n\
                last reset: epoch 2, to $12.50 at 20";
    assert!(said.starts_with(head), "{said}");
    assert!(
        said.contains("reservations that count in epoch 2 and were not released: 0\n"),
        "{said}"
    );
    // The earlier reservation is read as it was: open, of the epoch it was made in.
    let said = world.of_run(run.as_str()).unwrap();
    assert_eq!(
        said,
        format!(
            "run: {run}\nreservation: open\n  reserved: $0.25, in epoch 1\n  counts for: \
             $0.25\n  charged: $0.00\n  outstanding: $0.00\n  unresolved: $0.00\n  still \
             held beyond what was charged: $0.25\nits requests under its own reservation: \
             charged $0.00, still held $0.00, unresolved $0.00\n"
        )
    );
}

#[test]
fn a_reservation_its_runs_end_closed_as_released_is_counted_for_its_charges_and_not_listed() {
    let (world, run) = one_call("spend-released");
    // Its run's end closed it with nothing charged, as the ledger will once it
    // closes reservations (task S11): the closing, its place and its turn.
    world.plant(&[
        Record::Closing {
            run: run.clone(),
            released: true,
            by: ClosedBy::End,
            charged: Sum::ZERO,
            unresolved: Sum::ZERO,
            at_ms: 0,
        },
        Record::Place {
            run: run.clone(),
            place: n(1),
            by: PlaceBy::Closing,
        },
        Record::Turn {
            epoch: 1,
            turn: n(2),
            by: TurnBy::Closing,
            run: Some(run.clone()),
        },
    ]);
    let said = world.status();
    assert!(said.contains("counted against it: $0.00\n"), "{said}");
    assert!(said.contains("left: $0.25\n"), "{said}");
    assert!(
        said.contains("reservations that count in epoch 1 and were not released: 0\n"),
        "{said}"
    );
    assert!(!said.contains(run.as_str()), "{said}");
    assert!(!said.contains("admits nothing"), "{said}");
    // By its run it is still shown.
    let said = world.of_run(run.as_str()).unwrap();
    assert!(said.contains("reservation: released\n"), "{said}");
    world.paint().unwrap();
}

#[test]
fn a_store_no_reset_was_made_in_allows_nothing_and_status_says_how_to_set_an_allowance() {
    let world = World::new("spend-none");
    let said = world.status();
    assert!(
        said.starts_with(
            "epoch: 0\nallowance: none. No reset has set one, so every paid call is \
             refused: whizbang spend reset --allowance <usd>\n"
        ),
        "{said}"
    );
    assert!(said.contains("last reset: none\n"), "{said}");
    assert_eq!(refused(world.paint()), "over the allowance");
    // A store that is not there is said as the store says it, and nothing is made.
    let nowhere = world.root.with_file_name("nowhere");
    for command in [status(&nowhere, None), reset(&nowhere, "1")] {
        let why = spend::run(&command).unwrap_err();
        assert!(why.starts_with("there is no store at "), "{why}");
    }
    assert!(!nowhere.exists());
}

// --- one run -----------------------------------------------------------------------------------

#[test]
fn status_of_one_run_says_what_was_reserved_what_its_requests_were_charged_and_what_they_hold() {
    let (world, run) = one_call("spend-run");
    let child = run_id("7000-50");
    // Its own request of a dime, settled at six cents; and a request of a nickel
    // by a run it called, which is still out: no answer settled it, so its bound
    // is outstanding and unresolved.
    world.plant(&[
        Record::Place {
            run: run.clone(),
            place: n(1),
            by: PlaceBy::Dispatch,
        },
        Record::Dispatch {
            run: run.clone(),
            place: n(1),
            by: run.clone(),
            request: 1,
            bound: amount(100_000),
            at_ms: 0,
        },
        Record::Settling {
            run: run.clone(),
            place: n(1),
        },
        Record::Settlement {
            run: run.clone(),
            place: n(1),
            stage: Stage::Answered,
            charged: amount(60_000),
            over_bound: false,
            at_ms: 0,
        },
        Record::Turn {
            epoch: 1,
            turn: n(2),
            by: TurnBy::Settlement,
            run: Some(run.clone()),
        },
        Record::Place {
            run: run.clone(),
            place: n(2),
            by: PlaceBy::Dispatch,
        },
        Record::Dispatch {
            run: run.clone(),
            place: n(2),
            by: child.clone(),
            request: 1,
            bound: amount(50_000),
            at_ms: 0,
        },
    ]);
    assert_eq!(
        world.of_run(run.as_str()).unwrap(),
        format!(
            "run: {run}\nreservation: open\n  reserved: $0.25, in epoch 1\n  counts for: \
             $0.25\n  charged: $0.06\n  outstanding: $0.05\n  unresolved: $0.05\n  still \
             held beyond what was charged: $0.19\nits requests under its own reservation: \
             charged $0.06, still held $0.00, unresolved $0.00\n"
        )
    );
    assert_eq!(
        world.of_run("7000-50").unwrap(),
        format!(
            "run: 7000-50\nits requests under the reservation of {run}: charged $0.00, \
             still held $0.05, unresolved $0.05\n"
        )
    );
    assert_eq!(
        world.of_run("7000-51").unwrap(),
        "run: 7000-51\nthe books hold no reservation of it and no request it sent\n"
    );
    // A text that is no run's id is refused, shown escaped.
    let why = world.of_run("no\u{1b}run").unwrap_err();
    assert!(!why.contains('\u{1b}') && why.contains("\\u{1b}"), "{why}");
    // Under a frozen reservation, which run sent what cannot be read.
    world.plant_bytes(&[(format!("spend/place/{run}/x"), b"?\n".to_vec())]);
    let said = world.of_run("7000-50").unwrap();
    assert!(
        said.contains(": unknown, at most $") && said.contains("that reservation is frozen"),
        "{said}"
    );
}

// --- damage, and what is printed of it ---------------------------------------------------------

#[test]
fn status_names_a_damaged_record_and_a_text_of_no_form_escaped_and_cut_with_no_path() {
    let (world, run) = one_call("spend-damaged");
    let long = format!("spend/elsewhere/{}", "x".repeat(150));
    let marker = "CONTENT-THAT-IS-NEVER-PRINTED";
    world.plant_bytes(&[
        // A record that does not say what its name says: content of another kind,
        // with a marker in it, which is never read out.
        (
            format!("spend/place/{run}/1"),
            format!("{{\"record\":\"nonsense\",\"note\":\"{marker}\"}}\n").into_bytes(),
        ),
        // Texts that are no name of the ledger: one with a tab in it, one long.
        ("spend/tab\there".to_owned(), b"x".to_vec()),
        (long.clone(), b"x".to_vec()),
        // A turn of no form: its epoch admits nothing.
        ("spend/turn/1/x".to_owned(), b"x".to_vec()),
    ]);
    let said = world.status();
    let line = line_of(&said, &run);
    assert!(line.contains("  frozen  counts for "), "{line}");
    assert!(
        line.contains(&format!(
            "; 1 of its records cannot be read or do not say what their names say, \
             the first under spend/place/{run}/1"
        )),
        "{line}"
    );
    assert!(
        said.contains(
            "texts under spend/ that are no name of the ledger and belong to no \
             reservation: 3, of which 2 are placed nowhere and change no balance\n"
        ),
        "{said}"
    );
    // Escaped: the tab is shown as an escape. Cut: the long text is not whole.
    assert!(said.contains("\"spend/tab\\there\" is no name"), "{said}");
    assert!(!said.contains('\t') && !said.contains(&long), "{said}");
    assert!(said.contains("..."), "{said}");
    assert!(
        said.contains("epoch 1 admits nothing until a reset"),
        "{said}"
    );
    // No path, and nothing of a record's content.
    assert!(!said.contains(world.root.to_str().unwrap()), "{said}");
    assert!(
        !said.contains(marker) && !said.contains("nonsense"),
        "{said}"
    );
    // The newest reset's own record damaged: the epoch allows nothing, and status
    // says why.
    let world = World::new("spend-damaged-reset");
    world.plant_bytes(&[
        (
            "spend/reset/1/250000".to_owned(),
            b"not a record\n".to_vec(),
        ),
        ("spend/turn/0/1".to_owned(), b"x".to_vec()),
    ]);
    let said = world.status();
    assert!(said.contains("allowance: $0.00\n"), "{said}");
    assert!(
        said.contains("last reset: epoch 1, and no record of it can be read\n"),
        "{said}"
    );
    assert!(
        said.contains("the reset of epoch 1 is not whole") && said.contains("allows nothing"),
        "{said}"
    );
    // The owner's reset is the way out of that too.
    world.reset("0.25").unwrap();
    world.paint().unwrap();
}

// --- what a reset refuses ----------------------------------------------------------------------

#[test]
fn an_allowance_is_dollars_as_digits_and_anything_else_is_refused_before_a_store_is_opened() {
    let world = World::new("spend-allowance");
    let before = world.held();
    // The largest number there is, which is no amount; one more; and what is no
    // number of dollars.
    let largest = "18446744073709.551615";
    let why = world.reset(largest).unwrap_err();
    assert_eq!(
        why,
        format!(
            "{largest} is no allowance: it is the largest number there is, which is no \
             amount. Give US dollars as digits, with at most six after a point, as in 20 \
             or 12.50"
        )
    );
    for (asked, why) in [
        ("18446744073709.551616", "it is more than can be counted"),
        ("99999999999999999999999", "it is more than can be counted"),
        (
            "0.0000001",
            "a millionth of a dollar is the least that is counted",
        ),
        ("", "it is not a number of dollars"),
        ("-1", "it is not a number of dollars"),
        ("+1", "it is not a number of dollars"),
        ("$5", "it is not a number of dollars"),
        ("1e3", "it is not a number of dollars"),
        ("1.", "it is not a number of dollars"),
        (".5", "it is not a number of dollars"),
        ("1.2.3", "it is not a number of dollars"),
        ("1,000", "it is not a number of dollars"),
        ("five\u{1b}[31m", "it is not a number of dollars"),
    ] {
        let said = world.reset(asked).unwrap_err();
        assert!(
            said.contains(&format!(" is no allowance: {why}. ")),
            "{asked}: {said}"
        );
        assert!(!said.contains('\u{1b}'), "{asked}");
    }
    assert_eq!(world.held(), before, "nothing was made");
    // What is taken: whole dollars, cents, and down to a millionth; the largest
    // amount there is; and nothing.
    for (asked, said) in [
        ("20", "$20.00"),
        ("12.50", "$12.50"),
        ("0.000001", "$0.000001"),
        ("007.1", "$7.10"),
        ("18446744073709.551614", "$18446744073709.551614"),
        ("0", "$0.00"),
    ] {
        let made = world.reset(asked).unwrap();
        assert!(
            made.starts_with(&format!("allowance reset: {said}\n")),
            "{asked}: {made}"
        );
        assert!(
            world.status().contains(&format!("allowance: {said}\n")),
            "{asked}"
        );
    }
}

#[test]
fn a_reset_after_the_largest_epoch_and_one_at_the_generation_limit_are_refused_and_say_why() {
    // Other hands opened the largest epoch a name can say: no reset can follow.
    let world = World::new("spend-last-epoch");
    world.reset("1").unwrap();
    world.plant(&[Record::Epoch { epoch: n(u64::MAX) }]);
    let before = world.held();
    assert_eq!(
        world.reset("1").unwrap_err(),
        "the allowance was not reset: the store's newest epoch is 18446744073709551615, \
         the largest number a name can say, so no reset can follow it. Whizbang opens no \
         such epoch; nothing gets a store out of it but a new store"
    );
    assert_eq!(world.held(), before);
    assert!(world.status().starts_with("epoch: 18446744073709551615\n"));

    // A store whose generation file has no room for three names more.
    let world = World::new("spend-limit");
    world.reset("1").unwrap();
    let before = world.held();
    let generation = world
        .root
        .join("generations")
        .join(format!("{}.json", before.0));
    let limit = std::fs::metadata(generation).unwrap().len() + 100;
    let limited = Recovery::none().limiting_generations_to(limit);
    let why = spend::run_with(&reset(&world.root, "2"), limited).unwrap_err();
    assert!(
        why.starts_with(
            "the allowance was not reset: a reset adds three names, and the store holds \
             too many for one generation already (its file would be "
        ),
        "{why}"
    );
    assert!(why.contains("nothing was published"), "{why}");
    assert!(!why.contains(world.root.to_str().unwrap()), "{why}");
    assert_eq!(world.held(), before);
    assert!(world.status().contains("allowance: $1.00\n"));
}

#[test]
fn reset_is_a_command_and_no_registered_action() {
    let registry = wiring::registry().unwrap();
    let description = wiring::description(&registry);
    let names: Vec<&str> = description.actions().iter().map(|one| one.name()).collect();
    assert!(!names.is_empty());
    for name in names {
        assert!(!name.contains("reset") && !name.contains("spend"), "{name}");
    }
}

// --- the one way to a writer -------------------------------------------------------------------

#[test]
fn spend_reset_on_a_store_a_killed_process_left_recovers_before_it_resets() {
    let world = World::new("spend-recovers");
    // A run that a killed process made and never ended.
    {
        let opening = wiring::opening(&world.root, Make::Nothing, Recovery::none()).unwrap();
        let store = opening.writer().unwrap();
        let mut publication = store.begin().unwrap();
        publication.create_run("1000-1").unwrap();
        publication
            .put_in("1000-1", "admitted", Kind::Diagnostic, b"left\n")
            .unwrap();
        publication.publish().unwrap();
    }
    let found = world.held().0;
    let part = FakeRecovery::new("fake");
    part.make("recovered/fake", b"once\n");
    let part = Arc::new(part);
    let with = || {
        let part: Arc<dyn RecoveryPart> = part.clone();
        Recovery::none().with_part(part)
    };
    let said = spend::run_with(&reset(&world.root, "3"), with()).unwrap();
    // The part was asked once and shown the store as it was left; its publication
    // is the generation before the reset's.
    let [shown] = part.shown().try_into().unwrap();
    assert_eq!(shown.generation, found);
    assert_eq!(
        said,
        format!(
            "allowance reset: $3.00\nepoch: 1\ngeneration: {}\n",
            found + 2
        )
    );
    let (number, names) = world.held();
    assert_eq!(number, found + 2);
    assert!(names.contains(&"recovered/fake".to_owned()));
    // Status opens nothing to write and recovers nothing.
    spend::run_with(&status(&world.root, None), with()).unwrap();
    assert_eq!((part.asked(), world.held().0), (1, found + 2));
    // After a recovery that failed, the reset is refused with the reason and
    // nothing is made.
    let failing = FakeRecovery::new("fake");
    failing.fail("it could not read what was left");
    let failing: Arc<dyn RecoveryPart> = Arc::new(failing);
    let why = spend::run_with(
        &reset(&world.root, "4"),
        Recovery::none().with_part(failing),
    );
    let why = why.unwrap_err();
    assert!(
        why.starts_with("the store could not be recovered, so it is open to read only"),
        "{why}"
    );
    assert_eq!(world.held().0, found + 2);
    assert!(world.status().contains("allowance: $3.00\n"));
}
