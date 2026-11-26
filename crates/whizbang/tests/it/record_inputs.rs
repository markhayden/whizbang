//! `whizbang store record-inputs` (plan E, task O1, "The inputs, made before O1.1"): every
//! record's example inputs, from the constructors its tests use, written into a store whole
//! and printed by identity; and each record lists them. That the constructors are the tests'
//! is held beside the tests: rascal's records take theirs from `whizbang_rascal::examples`,
//! and squint's `record_inputs` holds its images to the ones its records' tests construct.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value as Json;
use whizbang::play_kept::PICTURES;
use whizbang::play_result::Tasks;
use whizbang::play_saved::{example_in, open};
use whizbang::record_inputs::{Input, RECORDS, example_id, inputs, is_port, line, listed};
use whizbang_core::artifact::{self, Artifact, ArtifactId};
use whizbang_core::identity::{Binding, DecodeLimits, PixelDigest};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::scratch::Scratch;

/// What `whizbang store record-inputs --store <store>` prints, or its complaint: the
/// function its one line in the store's commands calls. A test that starts the binary costs
/// about a second here, the budget of this test, so it is called in process.
fn record_inputs(store: &Path, inputs: &[Input]) -> Result<String, String> {
    let made = || Ok(inputs.to_vec());
    whizbang::record_inputs::write_these(store, whizbang::wiring::recovery(), made)
}

/// What an artifact is on and names: the image its binding is on, and the locations and
/// line it was drawn from.
fn needs(artifact: &Artifact) -> (Option<PixelDigest>, Vec<ArtifactId>) {
    let on = |binding: &Binding| Some(binding.source());
    match artifact {
        Artifact::Mask(mask) => {
            let mut named = mask.source().locations.clone();
            named.extend(mask.profile().map(|profile| profile.band.line()));
            (on(mask.binding()), named)
        }
        Artifact::Location(location) => (on(location.binding()), Vec::new()),
        Artifact::BandProfile(band) => (on(band.binding()), vec![band.line()]),
        Artifact::Plan(plan) => (on(plan.binding()), Vec::new()),
        _ => (None, Vec::new()),
    }
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
    let all = inputs().expect("the inputs are made");
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
    let images = all
        .iter()
        .filter(|i| matches!(i.artifact, Artifact::Image(_)));
    let pictures = images
        .map(|input| input.artifact.id())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let mut examples: Vec<(&str, &str)> = all.iter().map(|i| (i.record, i.example)).collect();
    examples.dedup();
    let summary = |new: usize, named: usize, saved: usize| {
        format!(
            "{} inputs of 34 records: {artifacts} artifacts, {new} new to the store; {named} \
             pictures named and {saved} of {} saved examples written for the playground",
            all.len(),
            examples.len()
        )
    };
    want.push(summary(artifacts, pictures, examples.len()));
    assert_eq!(printed.lines().collect::<Vec<_>>(), want);
    // Written again, nothing is new and nothing is published.
    let number = || {
        let reader = Reader::open(&root, Arc::new(RealFs)).expect("a reader");
        reader.snapshot().expect("a generation").number()
    };
    let before = number();
    let again = record_inputs(&root, &all).expect("the inputs are written again");
    assert_eq!(again.lines().last(), Some(summary(0, 0, 0).as_str()));
    assert_eq!(number(), before, "writing again published a generation");

    // What an input is on, or names, is among its example's inputs.
    for input in &all {
        let of = || {
            all.iter()
                .filter(|i| (i.record, i.example) == (input.record, input.example))
        };
        let (on, named) = needs(&input.artifact);
        if let Some(on) = on {
            let images = of().filter_map(|i| match &i.artifact {
                Artifact::Image(image) => Some(image.identity().pixels),
                _ => None,
            });
            assert!(images.collect::<Vec<_>>().contains(&on), "{}", line(input));
        }
        for id in named {
            assert!(of().any(|i| i.artifact.id() == id), "{}", line(input));
        }
    }

    // Each reads back as the constructor made it, byte for byte: a mask with its protect
    // plane, its band profile and its prescriptions.
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
        let bytes = input.artifact.to_bytes();
        assert_eq!(read(input).to_bytes(), bytes, "{}", line(input));
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

    // The playground has a name for each image, and a saved example for each record's example
    // holding its call's inputs, by port, as they were written.
    let store = Store::open(&root, Arc::new(RealFs)).expect("the store");
    let generation = store.snapshot().expect("a generation");
    let registry = whizbang::wiring::registry().expect("the build's actions");
    let descriptors: Vec<_> = registry.descriptors().cloned().collect();
    let kits = whizbang::wiring::kits();
    let tasks = Tasks {
        descriptors: &descriptors,
        kits: &kits,
    };
    for input in all
        .iter()
        .filter(|i| matches!(i.artifact, Artifact::Image(_)))
    {
        let name = format!("{PICTURES}{}", input.artifact.id().0.hex());
        assert!(generation.get(&name).is_some(), "{}", line(input));
    }
    for (record, example) in &examples {
        let saved = example_in(&store, &generation, &example_id(record, example));
        let saved = saved.unwrap_or_else(|| panic!("{record} {example} is not saved"));
        let ports: Vec<(String, String)> = (all.iter())
            .filter(|i| (i.record, i.example) == (*record, *example) && is_port(i))
            .map(|i| (i.port.to_owned(), i.artifact.id().to_string()))
            .collect();
        let held: Vec<(String, String)> = (saved.inputs.iter())
            .map(|input| (input.port.clone(), input.id.clone()))
            .collect();
        assert_eq!(held, ports, "{record} {example}");

        // Opened, as the playground opens it, its draft gives each port those identities:
        // `select`'s form keeps one mask of several, the first (the carry-over row on the
        // pickers), and nothing else is short.
        let id = example_id(record, example);
        let draft = open(&store, &id, &tasks)
            .unwrap_or_else(|refused| panic!("{record} {example} does not open: {refused:?}"));
        let given = |port: &str| -> Vec<String> {
            let value = (draft["values"].get(port)).or_else(|| draft["kept"].get(port));
            match value {
                Some(Json::String(id)) => vec![id.clone()],
                Some(Json::Array(ids)) => ids
                    .iter()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect(),
                _ => Vec::new(),
            }
        };
        let mut by_port: Vec<(String, Vec<String>)> = Vec::new();
        for (port, id) in &ports {
            match by_port.iter_mut().find(|(at, _)| at == port) {
                Some((_, ids)) => ids.push(id.clone()),
                None => by_port.push((port.clone(), vec![id.clone()])),
            }
        }
        for (port, ids) in by_port {
            let got = given(&port);
            if *record == "select" {
                assert!(ids.len() > 1, "{example}");
                assert_eq!(
                    got,
                    ids[..1],
                    "select {example}: the form's one mask is the first"
                );
            } else {
                assert_eq!(got, ids, "{record} {example} {port}");
            }
        }
    }
    drop(store);

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
    let all = inputs().expect("the inputs are made");
    let (begin, end) = marks();
    for name in RECORDS {
        let text = record(name);
        let from = text.find(begin).expect("the opening mark") + begin.len();
        let to = text.find(end).expect("the closing mark");
        let text = format!("{}{}{}", &text[..from], listed(&all, name), &text[to..]);
        std::fs::write(tools().join(format!("{name}.md")), text).expect("the record is written");
    }
}
