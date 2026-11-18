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
