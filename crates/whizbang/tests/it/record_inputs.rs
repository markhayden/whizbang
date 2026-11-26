//! `whizbang store record-inputs` (plan E, task O1, "The inputs, made before O1.1"): every
//! record's example inputs, from the constructors its tests use, written into a store whole
//! and printed by identity; and each record lists them. That the constructors are the tests'
//! is held beside the tests: rascal's records take theirs from `whizbang_rascal::examples`,
//! and squint's `record_inputs` holds its images to the ones its records' tests construct.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang::record_inputs::{Input, RECORDS, inputs, line, listed};
use whizbang_core::artifact::{self, Artifact};
use whizbang_core::identity::DecodeLimits;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::scratch::Scratch;

/// What `whizbang store record-inputs --store <store>` prints, or its complaint: the
/// function its one line in the store's commands calls. A test that starts the binary costs
/// about a second here, the budget of this test, so it is called in process.
fn record_inputs(store: &Path, inputs: &[Input]) -> Result<String, String> {
    whizbang::record_inputs::write_these(store, whizbang::wiring::recovery(), inputs)
}

fn tools() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/tools")
}

fn marks() -> (&'static str, &'static str) {
    ("<!-- inputs: begin -->\n", "<!-- inputs: end -->\n")
}

/// The record's list of its inputs: the text between its `inputs` marks.
fn between(record: &str) -> Option<&str> {
    let (begin, end) = marks();
    let from = record.find(begin)? + begin.len();
    Some(&record[from..record.find(end)?])
}

fn record(name: &str) -> String {
    std::fs::read_to_string(tools().join(format!("{name}.md"))).expect("the record")
}

#[test]
fn every_input_is_written_whole_printed_by_its_identity_and_listed_in_its_record() {
    let all = inputs();
    let dir = Scratch::new("record-inputs");
    let root = dir.path().join("store");

    // Where there is no store, none is made and nothing is printed.
    let complaint = record_inputs(&root, &all).expect_err("no store is made");
    assert!(complaint.contains("there is no store at"), "{complaint}");
    assert!(!root.exists(), "{complaint}");

    drop(Store::open(&root, Arc::new(RealFs)).expect("a store"));
    let printed = record_inputs(&root, &all).expect("the inputs are written");
    let mut want: Vec<String> = all.iter().map(line).collect();
    let artifacts = {
        let ids = all.iter().map(|input| input.artifact.id());
        ids.collect::<std::collections::BTreeSet<_>>().len()
    };
    want.push(format!(
        "{} inputs of 34 records: {artifacts} artifacts, {artifacts} new to the store",
        all.len()
    ));
    assert_eq!(printed.lines().collect::<Vec<_>>(), want);

    // Each is in the store, and each mask reads back as the constructor made it, byte for
    // byte: with its protect plane, its band profile and its prescriptions.
    let reader = Reader::open(&root, Arc::new(RealFs)).expect("a reader");
    let generation = reader.snapshot().expect("a generation");
    let limits = DecodeLimits::default();
    let read = |input: &Input| {
        let (kind, id) = (input.artifact.artifact_type(), input.artifact.id());
        artifact::get(&reader, kind, id, &limits).expect("the input is in the store")
    };
    for input in &all {
        let (kind, id) = (input.artifact.artifact_type(), input.artifact.id());
        assert!(artifact::holds(&generation, kind, id), "{}", line(input));
        if let Artifact::Mask(mask) = &input.artifact {
            assert_eq!(read(input).to_bytes(), mask.to_bytes(), "{}", line(input));
        }
    }
    let band = all
        .iter()
        .find(|i| (i.record, i.example, i.port) == ("grow", "band", "mask"));
    let Artifact::Mask(written) = &band.expect("grow's band").artifact else {
        panic!("grow's band is a mask")
    };
    let Artifact::Mask(back) = read(band.expect("grow's band")) else {
        panic!("a mask comes back a mask")
    };
    assert!(written.protect().iter().any(|sample| *sample != 0));
    assert!(written.profile().is_some() && !written.prescriptions().is_empty());
    assert_eq!(
        (back.core(), back.blend(), back.protect()),
        (written.core(), written.blend(), written.protect())
    );
    assert_eq!(back.profile(), written.profile());
    assert_eq!(back.prescriptions(), written.prescriptions());

    // Every record of the session has inputs, and names the command and lists them.
    for name in RECORDS {
        assert!(all.iter().any(|input| input.record == name), "{name}");
        let text = record(name);
        assert!(
            text.contains("whizbang store record-inputs --store <dir>"),
            "{name}.md does not name the command"
        );
        assert!(
            between(&text) == Some(listed(&all, name).as_str()),
            "the inputs {name}.md lists are stale: run scripts/tool-record.sh inputs\n{}",
            listed(&all, name)
        );
    }
}

/// What `scripts/tool-record.sh inputs` runs: each record's list of its inputs, written again.
#[test]
#[ignore = "rewrites the committed records; run it with scripts/tool-record.sh inputs"]
fn write_the_records_inputs() {
    let all = inputs();
    let (begin, end) = marks();
    for name in RECORDS {
        let text = record(name);
        let from = text.find(begin).expect("the opening mark") + begin.len();
        let to = text.find(end).expect("the closing mark");
        let text = format!("{}{}{}", &text[..from], listed(&all, name), &text[to..]);
        std::fs::write(tools().join(format!("{name}.md")), text).expect("the record is written");
    }
}
