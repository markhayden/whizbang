//! `remote_parity::…`: SAM 3's parity fixtures asked of a remote worker (plan m6, task D1
//! writes it; task L1 runs it; `SPEC.md` section 18, item 3; ADR 0027's tolerance). Ignored
//! in the fast tier, and never in the default selection of the model tier, its `parity`
//! part or a release's local leg: only `scripts/test-models.sh parity-remote` selects it,
//! and that refuses unless `WHIZBANG_REMOTE_CONFIG` names a config whose segment is of the
//! kind `remote` (`scripts/tests/test-models-selection.sh`).
//!
//! Each of SAM 3's fixtures is asked of the host three times, in one request each (its
//! phrase, its point and its box), and every answer is held to the committed one: the same
//! counts, every pair's IoU at least 0.99 and its scores within 0.01, and its record naming
//! `cuda`. It writes what it measured to `model-tier/out/remote-parity.txt`: each fixture's
//! least IoU and widest score gap, the device every answer named, and the time of each call
//! (the first cold), for `docs/results/m6-remote-sam3.md`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use whizbang::pack::OneWorker;
use whizbang::parity::{self, Apart};
use whizbang::wiring::{self, Models, Over};
use whizbang_core::contract::{Call, Executor, Value};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

use crate::model_tier::{shared, write_out};
use crate::parity::{Kept, PICTURES, TIMES, asks, committed};
use crate::wired::whizbang;

/// The file `scripts/test-models.sh parity-remote` named, in cargo's folder for tests'
/// scratch: the config, and the env file when one was given.
fn named(file: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(shared().join(file)).ok()?;
    let path = PathBuf::from(text.trim_end_matches('\n'));
    assert!(path.is_absolute() && path.is_file(), "{file}: not a file");
    Some(path)
}

#[test]
#[ignore = "the model tier's remote part: scripts/test-models.sh parity-remote, under a config whose segment is remote"]
fn sam3_answers_each_fixture_inside_the_tolerance_from_a_remote_worker() {
    let config = named("remote-config")
        .expect("run it by scripts/test-models.sh parity-remote, which names the config");
    let env = named("remote-env-file");
    let dir = Scratch::new("remote-parity");
    let root = dir.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    let reset = whizbang(&["spend", "reset", "--allowance", "5"], &root);
    assert_eq!(reset.code, Some(0), "{}", reset.err);
    let worker = OneWorker::installed();
    let over = Over::nothing()
        .config(Some(&config))
        .env_file(env.as_deref())
        .kinds(wiring::kinds(&worker))
        .installed(Models::shipped().unwrap())
        .expect("the catalogue is read");
    let kept: Kept = Arc::default();
    let mut registry = wiring::registry().unwrap();
    registry.register(asks(&kept)).unwrap();
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let executor = wiring::runner(registry, store, &over).expect("the executor builds");
    let mut runs = Vec::new();
    let mut lines = Vec::new();
    for round in 0..TIMES {
        for fixture in &PICTURES {
            let call = Call::new("asks").argument("picture", Value::text(fixture.name));
            let began = Instant::now();
            let finished = executor.submit(&call).unwrap();
            let took = began.elapsed().as_millis();
            assert_eq!(finished.outcome().name(), "completed", "{}", fixture.name);
            lines.push((
                format!("call_ms_round{}_{}", round + 1, fixture.name),
                took.to_string(),
            ));
            runs.push(finished.run().as_str().to_owned());
        }
    }
    drop(executor);
    // No local worker was started: the host answered every request.
    assert!(worker.status().is_none(), "{worker:?}");
    let mut outside = Vec::new();
    // Each answer's record names the remote backend, and the device it was answered on.
    for run in &runs {
        let shown = whizbang(&["runs", "show", run, "--json"], &root);
        assert_eq!(shown.code, Some(0), "{}", shown.err);
        let record: serde_json::Value = serde_json::from_str(&shown.out).unwrap();
        let model = &record["models"][0];
        assert_eq!(model["backend"], "remote", "{run}: {model}");
        if model["device"] != "cuda" {
            outside.push(format!("{run}: answered on {}", model["device"]));
        }
    }
    let kept = std::mem::take(&mut *kept.lock().unwrap());
    assert_eq!(kept.len(), TIMES * PICTURES.len());
    for fixture in &PICTURES {
        let (_, expected) = committed("sam3", fixture);
        let mut worst = Apart {
            least_iou: 1.0,
            widest_score: 0.0,
        };
        for (at, answer) in kept.iter().enumerate() {
            if PICTURES[at % PICTURES.len()].name != fixture.name {
                continue;
            }
            match parity::apart(&expected.found(), &answer.found()) {
                Some(apart) => worst = worst.worst(apart),
                None => outside.push(format!(
                    "{} round {}: the counts differ",
                    fixture.name,
                    at / PICTURES.len() + 1
                )),
            }
            if !parity::agree(&expected.found(), &answer.found()) {
                outside.push(format!(
                    "{} round {}",
                    fixture.name,
                    at / PICTURES.len() + 1
                ));
            }
        }
        lines.push((
            format!("sam3_remote_{}_least_iou", fixture.name),
            format!("{:.6}", worst.least_iou),
        ));
        lines.push((
            format!("sam3_remote_{}_widest_score", fixture.name),
            format!("{:.6}", worst.widest_score),
        ));
    }
    write_out("remote-parity", &lines);
    assert!(
        outside.is_empty(),
        "outside the tolerance, or not on cuda: {outside:?}; measured: {lines:?}"
    );
}
