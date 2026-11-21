//! Request ids in volume (plan 2C, task E1, item 6; the carry-over row on every
//! publication writing the whole generation; ADR 0030): a store that holds the names of
//! many request ids, as each admission under an id leaves them (`requests/<id>/admitted`
//! and `requests/<id>/ended`), and what one more publication then costs. The names are
//! made in one publication, as no command makes them, so that the measurement is of the
//! store at that size and not of the way there.

use std::time::Instant;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Kind, Store};
use whizbang_testkit::scratch::Scratch;

/// What the generation file may hold for each request id: its two names, each with its
/// digest, its size and its kind. Measured at 389 bytes, at 2,000 ids and at 100,000
/// (ADR 0030).
const BYTES_PER_ID: u64 = 400;

/// What one publication of one more name may take for each thousand ids the store holds,
/// in milliseconds, and at least: 2,058 ms at 100,000 ids, about 21 for each thousand, in
/// the tests' build on the owner's Mac (ADR 0030), with room for a slower machine.
const MS_PER_THOUSAND_IDS: u128 = 60;
const MS_AT_LEAST: u128 = 500;

/// A store that holds `ids` request ids, and what one more publication cost: the size of
/// the generation file it wrote, and its time in milliseconds.
fn measured(label: &str, ids: usize) -> (u64, u128) {
    let scratch = Scratch::new(label);
    let root = scratch.path().join("store");
    let store = Store::open(&root, std::sync::Arc::new(RealFs)).unwrap();
    // Each id's records are a few hundred bytes; their content is not what grows a
    // generation, so one content serves every name.
    let record = vec![b'x'; 300];
    let mut publication = store.begin().unwrap();
    for at in 0..ids {
        let id = format!("matrix-0123456789abcdef-{at:06}");
        for name in ["admitted", "ended"] {
            let made =
                publication.create(&format!("requests/{id}/{name}"), Kind::Diagnostic, &record);
            made.unwrap();
        }
    }
    publication.publish().unwrap();
    let started = Instant::now();
    let mut publication = store.begin().unwrap();
    publication
        .create("requests/one-more/admitted", Kind::Diagnostic, &record)
        .unwrap();
    let published = publication.publish().unwrap();
    let took = started.elapsed().as_millis();
    let generation = root
        .join("generations")
        .join(format!("{}.json", published.generation));
    let bytes = std::fs::metadata(generation).unwrap().len();
    eprintln!("{ids} ids: a generation of {bytes} bytes, one publication in {took} ms");
    (bytes, took)
}

fn within(ids: usize, (bytes, took): (u64, u128)) {
    let count = ids as u64 + 1;
    assert!(
        bytes <= count * BYTES_PER_ID,
        "{bytes} bytes for {count} ids"
    );
    let bound = (ids as u128 / 1000 * MS_PER_THOUSAND_IDS).max(MS_AT_LEAST);
    assert!(took <= bound, "{took} ms, over {bound}");
}

#[test]
fn request_ids_at_2000_publish_within_the_measured_bound() {
    within(2_000, measured("ids-2000", 2_000));
}

/// Run by hand, its numbers recorded in ADR 0030: `scripts/heavy.sh ids
/// scripts/thread-guard.sh cargo nextest run -p whizbang-core --run-ignored only -E
/// 'test(request_ids_at_100000)'` (`just test-only` takes no `--run-ignored`).
#[test]
#[ignore = "about five seconds and 40 MB of disk; run by hand, its numbers in ADR 0030"]
fn request_ids_at_100000_publish_within_the_measured_bound() {
    within(100_000, measured("ids-100000", 100_000));
}

/// What `calls` nested calls of an action that does nothing cost, made by one run in a
/// store that holds nothing else (plan m3, task R1, item 2; the carry-over row on
/// `STEPS_LIMIT`): the time of the whole run in milliseconds, and the size of the last
/// generation file.
fn nested(label: &str, calls: usize) -> (u128, u64) {
    use whizbang_core::contract::{
        Action, ActionDescriptor, Call, Context, Engine, Executor, Failure, Kind, NoBackends,
        PreparedCall, Produced, Registry,
    };
    use whizbang_core::identity::Digest;
    struct Leaf(ActionDescriptor);
    impl Action for Leaf {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
            Ok(Produced::nothing())
        }
    }
    struct Many(ActionDescriptor, usize);
    impl Action for Many {
        fn descriptor(&self) -> &ActionDescriptor {
            &self.0
        }
        fn run(&self, _: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
            for _ in 0..self.1 {
                context.executor().submit(&Call::new("leaf")).unwrap();
            }
            Ok(Produced::nothing())
        }
    }
    let draft = |name: &str| ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "Volume.");
    let mut registry = Registry::new();
    let leaf = Leaf(draft("leaf").build().unwrap());
    registry.register(std::sync::Arc::new(leaf)).unwrap();
    let many = Many(draft("many").build().unwrap(), calls);
    registry.register(std::sync::Arc::new(many)).unwrap();
    let scratch = Scratch::new(label);
    let root = scratch.path().join("store");
    let store = std::sync::Arc::new(Store::open(&root, std::sync::Arc::new(RealFs)).unwrap());
    let no = std::sync::Arc::new(NoBackends);
    let executor = Engine::over(registry, no, store.clone(), Digest::of_bytes(b""));
    let started = Instant::now();
    let finished = executor.submit(&Call::new("many")).unwrap();
    let took = started.elapsed().as_millis();
    assert_eq!(finished.outcome().name(), "completed");
    let generation = store.snapshot().unwrap().number();
    let file = root.join("generations").join(format!("{generation}.json"));
    let bytes = std::fs::metadata(file).unwrap().len();
    eprintln!("{calls} nested calls: {took} ms, the last generation {bytes} bytes");
    (took, bytes)
}

/// What one nested call may take, in milliseconds, at the most a run makes: 256 took
/// 18,879 ms on the owner's Mac in the tests' build, about 74 each, and 32 took 1,932 ms
/// (ADR 0033, "How many calls a run makes"); with a busy disk a call took over 300, so
/// the bound is what catches a cost that grows, not a slow machine.
const NESTED_MS_EACH: u128 = 1_000;

/// What the generation file may hold for each run: 585 bytes measured at 256.
const NESTED_BYTES_EACH: u64 = 800;

#[test]
fn a_run_of_many_nested_calls_publishes_within_the_measured_bound() {
    // Eight, under a second on the owner's Mac: what CI can spend on it.
    let (took, bytes) = nested("nested-8", 8);
    assert!(took <= 8 * NESTED_MS_EACH, "{took} ms");
    assert!(bytes <= 9 * NESTED_BYTES_EACH, "{bytes} bytes");
}

/// Run by hand, its numbers recorded in ADR 0033: `scripts/heavy.sh nested
/// scripts/thread-guard.sh cargo nextest run -p whizbang-core --run-ignored only -E
/// 'test(nested_calls_at_the_limit)'`.
#[test]
#[ignore = "the most nested calls a run makes; run by hand, its numbers in ADR 0033"]
fn nested_calls_at_the_limit_publish_within_the_measured_bound() {
    let calls = whizbang_core::contract::STEPS_LIMIT;
    let (took, bytes) = nested("nested-limit", calls);
    assert!(took <= calls as u128 * NESTED_MS_EACH, "{took} ms");
    assert!(
        bytes <= (calls as u64 + 1) * NESTED_BYTES_EACH,
        "{bytes} bytes"
    );
}

/// What one publication of one more run costs in a store that holds `runs` runs, each as
/// a run of the playground leaves it (plan m5, task H1, item 7): its record, its journal
/// and two more diagnostics, one output, its request id's two names and two names of the
/// ledger. Made in one publication, so that the measurement is of the store at that size.
/// The size of the generation file it wrote, and its time in milliseconds.
fn runs_measured(label: &str, runs: usize) -> (u64, u128) {
    let scratch = Scratch::new(label);
    let root = scratch.path().join("store");
    let store = Store::open(&root, std::sync::Arc::new(RealFs)).unwrap();
    let record = vec![b'x'; 300];
    let mut publication = store.begin().unwrap();
    let made = |publication: &mut whizbang_core::store::Publication<'_>, at: usize| {
        let run = format!("1795275290505-{at}");
        publication.create_run(&run).unwrap();
        for (name, kind) in [
            ("record.json", Kind::Diagnostic),
            ("journal.jsonl", Kind::Diagnostic),
            ("diagnostics/1", Kind::Diagnostic),
            ("diagnostics/2", Kind::Diagnostic),
            ("outputs/mask", Kind::Output),
        ] {
            // One content for each name, whichever run: what grows a generation is its
            // names, not their content, and a content written once is synced once.
            let mut bytes = record.clone();
            bytes.extend_from_slice(name.as_bytes());
            publication.put_in(&run, name, kind, &bytes).unwrap();
        }
        let id = format!("play-0123456789abcdef-{at:06}");
        for name in [
            format!("requests/{id}/admitted"),
            format!("requests/{id}/ended"),
            format!("spend/{run}/reserved"),
            format!("spend/{run}/released"),
        ] {
            publication
                .create(&name, Kind::Diagnostic, &record)
                .unwrap();
        }
    };
    for at in 0..runs {
        made(&mut publication, at);
    }
    publication.publish().unwrap();
    let started = Instant::now();
    let mut publication = store.begin().unwrap();
    made(&mut publication, runs);
    let published = publication.publish().unwrap();
    let took = started.elapsed().as_millis();
    let generation = root
        .join("generations")
        .join(format!("{}.json", published.generation));
    let bytes = std::fs::metadata(generation).unwrap().len();
    eprintln!("{runs} runs: a generation of {bytes} bytes, one publication in {took} ms");
    (bytes, took)
}

/// The bound measured at 10,000 runs, on the owner's Mac in the tests' build (plan m5,
/// task H1): 1,706 bytes of the generation file a run (a file of 17.1 MB), and one
/// publication of one more run in 712 and 678 ms; held here with room for a slower
/// machine. The 64 MiB limit of a generation is reached at about 39,000 such runs.
const BYTES_PER_RUN: u64 = 1_800;
const MS_AT_TEN_THOUSAND_RUNS: u128 = 2_500;

#[test]
#[ignore = "about two seconds and 20 MB of disk; run by hand, its numbers in docs/carry-over.md"]
fn ten_thousand_runs_publish_within_the_measured_bound() {
    let (bytes, took) = runs_measured("volume-runs", 10_000);
    assert!(bytes <= 10_001 * BYTES_PER_RUN, "{bytes} bytes");
    assert!(took <= MS_AT_TEN_THOUSAND_RUNS, "{took} ms");
}
