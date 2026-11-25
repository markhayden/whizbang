//! `bundle::…`: invocation bundles (ADR 0048; plan m7, task B1, items 2 and 3), written by
//! the core's writer from runs the binary's own wiring made, with the loop's kit of fakes
//! (`checked::Looped`): a removal (`remove_prompt`) and its check (`source-fidelity`) of
//! the removal's picture against the picture it came from. Every rule of "Reading one
//! trusts nothing" is shown on a bundle built here and then broken, member by member.
//! Nothing here sends anything anywhere.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang::wiring::{self, Over};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::bundle::{self, Made, Manifest, Withheld, tar};
use whizbang_core::contract::record::{Admitted, Left, RunRecord};
use whizbang_core::contract::{Call, Executor, Finished, Outcome, RunId, Value};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_rascal::checked::{CHECK, REMOVE, job};
use whizbang_testkit::scratch::Scratch;

use crate::checked::{Looped, PHRASE, Setup, output, passes};

/// A removal and its check, run in a store on the real disk.
pub struct Chain {
    pub world: Looped,
    pub removal: String,
    pub check: String,
    /// What the removal made of the picture.
    pub removed: ArtifactId,
}

/// The removal of [`PHRASE`] from the world's picture.
pub fn removal(world: &Looped) -> Finished {
    let call = Call::new(REMOVE)
        .argument("prompt", Value::text(PHRASE))
        .input("image", world.image_id());
    let finished = world.run(&call);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    finished
}

/// The check of `removed` against the world's picture.
pub fn checking(world: &Looped, removed: ArtifactId) -> Finished {
    let call = Call::new(CHECK)
        .argument("job", Value::text(&job(PHRASE)))
        .input("images", removed)
        .input("images", world.image_id());
    let finished = world.run(&call);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    finished
}

/// A removal and its check, in a world of this setup, on the real disk.
pub fn chain_of(label: &str, setup: Setup) -> Chain {
    let world = Looped::new(label, &setup.on_disk());
    let removed = removal(&world);
    let image = output(&removed, "image");
    let checked = checking(&world, image);
    Chain {
        removal: removed.run().as_str().to_owned(),
        check: checked.run().as_str().to_owned(),
        removed: image,
        world,
    }
}

/// A removal and its check whose judge passes the removal.
pub fn chain(label: &str) -> Chain {
    chain_of(label, Setup::answering(vec![passes()]))
}

/// The bundle of a chain's two runs, by the binary's writer.
pub fn made(chain: &Chain) -> Made {
    whizbang::bundling::make(&chain.world.root, &[&chain.removal, &chain.check]).unwrap()
}

/// A bundle's members, to be broken and written again: its manifest as JSON, and every
/// other member by its name.
#[derive(Clone)]
pub struct Members {
    pub manifest: Json,
    pub members: BTreeMap<String, Vec<u8>>,
}

impl Members {
    pub fn of(bytes: &[u8]) -> Members {
        let mut reader = tar::Reader::new(Cursor::new(bytes), bundle::MAX_BYTES);
        let mut manifest = None;
        let mut members = BTreeMap::new();
        while let Some(head) = reader.next_head().unwrap() {
            let bytes = reader.bytes(&head).unwrap();
            match head.name.as_str() {
                bundle::MANIFEST => manifest = Some(serde_json::from_slice(&bytes).unwrap()),
                _ => drop(members.insert(head.name, bytes)),
            }
        }
        Members {
            manifest: manifest.unwrap(),
            members,
        }
    }

    /// The bundle again: the manifest first, then the members in the order of their
    /// names.
    pub fn bytes(&self) -> Vec<u8> {
        let mut writer = tar::Writer::new(0);
        let mut said = serde_json::to_vec(&self.manifest).unwrap();
        said.push(b'\n');
        writer.add(bundle::MANIFEST, &said).unwrap();
        for (name, bytes) in &self.members {
            writer.add(name, bytes).unwrap();
        }
        writer.finish()
    }

    /// The text of a run's record.
    pub fn record(&self, run: &str) -> String {
        String::from_utf8(self.members[&format!("runs/{run}/record")].clone()).unwrap()
    }

    /// Replaces text in a run's record, which must hold it.
    pub fn edit_record(&mut self, run: &str, from: &str, to: &str) {
        let record = self.record(run);
        assert!(record.contains(from), "{from} is not in {record}");
        let name = format!("runs/{run}/record");
        self.members
            .insert(name, record.replacen(from, to, 1).into_bytes());
    }

    /// The runs of a step's family.
    pub fn runs(&self, step: usize) -> Vec<String> {
        let runs = self.manifest["steps"][step]["runs"].as_array().unwrap();
        runs.iter()
            .map(|run| run.as_str().unwrap().to_owned())
            .collect()
    }

    /// The run of a step's family whose action is this.
    pub fn run_of(&self, step: usize, action: &str) -> String {
        let runs = self.runs(step);
        let found = runs.into_iter().find(|run| {
            let record = RunRecord::from_bytes(self.record(run).as_bytes()).unwrap();
            record.asked().action.as_deref() == Some(action)
        });
        found.unwrap_or_else(|| panic!("no run of {action}"))
    }

    /// Takes a run's diagnostics and journal out, and every use of a file by it, with
    /// the files nothing else uses, and what is withheld of it: for a record put in
    /// its place that has none.
    pub fn strip(&mut self, run: &str) {
        let prefix = format!("runs/{run}/");
        self.members
            .retain(|name, _| !name.starts_with(&prefix) || name.ends_with("/record"));
        let artifacts = self.manifest["artifacts"].as_array_mut().unwrap();
        for file in artifacts.iter_mut() {
            let of = file["of"].as_array_mut().unwrap();
            of.retain(|used| used["run"] != run);
        }
        let mut gone = Vec::new();
        artifacts.retain(|file| {
            let kept = !file["of"].as_array().unwrap().is_empty();
            if !kept {
                let hex = file["file"].as_str().unwrap().trim_start_matches("sha256:");
                gone.push(format!("files/sha256-{hex}"));
            }
            kept
        });
        for name in gone {
            self.members.remove(&name);
        }
        let withheld = self.manifest["withheld"].as_array_mut().unwrap();
        withheld.retain(|item| item["run"] != run);
    }
}

/// What a bundle is refused for.
pub fn refused(bytes: &[u8]) -> String {
    bundle::check(Cursor::new(bytes)).unwrap_err().to_string()
}

/// Holds a refusal to name its fault and to say that nothing was imported or ran.
fn refused_for(bytes: &[u8], says: &str) {
    let said = refused(bytes);
    assert!(said.contains(says), "{said:?} does not say {says:?}");
    assert!(
        said.ends_with("nothing was imported and nothing ran"),
        "{said}"
    );
}

/// The steps of the chain's bundle, read back.
fn manifest(made: &Made) -> Manifest {
    bundle::check(Cursor::new(&made.bytes)).unwrap().manifest
}

#[test]
fn the_same_runs_make_the_same_bytes_twice() {
    let chain = chain("bundle-twice");
    let first = made(&chain);
    let second = made(&chain);
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.digest.hex().len(), 64);
    // The command writes the same bytes.
    let to = chain.world.root.parent().unwrap().join("chain.tar");
    let out = crate::wired::binary()
        .args(["runs", "bundle", &chain.removal, &chain.check, "--to"])
        .arg(&to)
        .arg("--store")
        .arg(&chain.world.root)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(said.contains(&first.digest.to_string()), "{said}");
    assert_eq!(std::fs::read(&to).unwrap(), first.bytes);
    // Not over the file without --overwrite.
    let again = crate::wired::binary()
        .args(["runs", "bundle", &chain.removal, "--to"])
        .arg(&to)
        .arg("--store")
        .arg(&chain.world.root)
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&again.stderr).contains("--overwrite"));
    // Every header is fixed: mode 0644, uid and gid 0, mtime 0, no names of users.
    let header = &first.bytes[..512];
    assert_eq!(&header[100..108], b"0000644\0");
    assert_eq!(&header[108..116], b"0000000\0");
    assert_eq!(&header[116..124], b"0000000\0");
    assert_eq!(&header[136..148], b"00000000000\0");
    assert!(header[265..329].iter().all(|b| *b == 0));
}

#[test]
fn a_bundle_reads_back_whole_with_every_member_checked() {
    // The names the core keeps of rascal's and squint's are theirs.
    assert_eq!(bundle::FILL_ACTION, whizbang_rascal::fill::NAME);
    assert_eq!(bundle::FILL_ANSWER, whizbang_rascal::fill::ANSWER);
    assert_eq!(bundle::CACHE_SETTING, whizbang_squint::cache::SETTING);
    assert_eq!(bundle::ANSWERED_BY, whizbang_squint::cache::ANSWERED_BY);

    let chain = chain("bundle-reads");
    let made = made(&chain);
    let checked = bundle::check(Cursor::new(&made.bytes)).unwrap();
    assert_eq!(checked.digest, made.digest);
    assert_eq!(checked.bytes, made.bytes.len() as u64);
    assert_eq!(checked.manifest, made.manifest);
    let manifest = &checked.manifest;
    assert_eq!(manifest.bundle, 1);
    assert_eq!(manifest.whizbang, env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest.steps.len(), 2);
    assert_eq!(manifest.steps[0].run, chain.removal);
    assert_eq!(manifest.steps[0].action.as_deref(), Some(REMOVE));
    assert_eq!(manifest.steps[1].action.as_deref(), Some(CHECK));
    assert!(
        manifest
            .steps
            .iter()
            .all(|step| step.replayable && step.why.is_none())
    );
    // The manifest first, then every other member in the order of its name's bytes.
    let names: Vec<&str> = checked
        .members
        .iter()
        .map(|(name, _, _)| name.as_str())
        .collect();
    assert_eq!(names[0], "bundle.json");
    let mut sorted = names[1..].to_vec();
    sorted.sort_unstable();
    assert_eq!(names[1..], sorted);
    // Each run of each family: its record, its journal and its diagnostics.
    let members = Members::of(&made.bytes);
    for step in &manifest.steps {
        assert!(step.runs.len() > 1, "{:?}", step.runs);
        for run in &step.runs {
            let record =
                RunRecord::from_bytes(&members.members[&format!("runs/{run}/record")]).unwrap();
            for diagnostic in record.diagnostics() {
                let name = format!("runs/{run}/{}", diagnostic.name());
                assert!(members.members.contains_key(&name), "{name}");
            }
        }
    }
    // The fill kept its painted answer, and it travels.
    let fill = members.run_of(0, "fill");
    assert!(
        members
            .members
            .contains_key(&format!("runs/{fill}/diagnostics/fill-answer.png"))
    );
    // Each file is the file of what its runs took or made.
    assert!(!manifest.artifacts.is_empty());
    for file in &manifest.artifacts {
        let name = format!("files/sha256-{}", file.file.hex());
        assert_eq!(
            whizbang_core::identity::Digest::of_bytes(&members.members[&name]),
            file.file
        );
        assert!(!file.of.is_empty());
    }
    // The bundle holds exactly what its manifest and its records name.
    assert_eq!(members.members.len() + 1, checked.members.len());
    // `bundle show` reads it the same way, into no store.
    let file = chain.world.root.parent().unwrap().join("shown.tar");
    std::fs::write(&file, &made.bytes).unwrap();
    let out = crate::wired::binary()
        .args(["bundle", "show"])
        .arg(&file)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.starts_with(&format!("bundle: {}\n", made.digest)),
        "{said}"
    );
    assert!(said.contains("steps: 2\n"), "{said}");
    assert!(
        said.contains("\n  input images 0: the output image 0 of step 0\n"),
        "{said}"
    );
}

#[test]
fn a_member_named_absolutely_with_dot_dot_twice_or_as_a_link_is_refused_and_nothing_imported() {
    let chain = chain("bundle-names");
    let made = made(&chain);
    let members = Members::of(&made.bytes);
    let any = members.members.keys().next().unwrap().clone();
    let mut absolute = members.clone();
    absolute
        .members
        .insert(format!("/{any}"), members.members[&any].clone());
    let mut dotted = members.clone();
    dotted
        .members
        .insert("runs/../files/x".to_owned(), b"x".to_vec());
    let mut emptied = members.clone();
    emptied
        .members
        .insert("runs//record".to_owned(), b"x".to_vec());
    // The same name twice: written by hand, since a map holds a name once.
    let mut twice = members.bytes();
    twice.truncate(twice.len() - 1024);
    let mut writer = tar::Writer::new(0);
    writer.add(&any, &members.members[&any]).unwrap();
    twice.extend(writer.finish());
    // A link: a header of type 2 among the members.
    let mut linked = members.bytes();
    linked.truncate(linked.len() - 1024);
    let mut header = tar::header("files/link", 0, 0).unwrap();
    header[156] = b'2';
    header[157..157 + 11].copy_from_slice(b"/etc/passwd");
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    linked.extend_from_slice(&header);
    linked.extend(std::iter::repeat_n(0u8, 1024));

    let cases = [
        (absolute.bytes(), "is not named plainly"),
        (dotted.bytes(), "is not named plainly"),
        (emptied.bytes(), "is not named plainly"),
        (twice, "is there twice"),
        (linked, "is a symbolic link, not a plain file"),
    ];
    let dir = Scratch::new("bundle-names-store");
    let store = Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    for (bytes, says) in cases {
        refused_for(&bytes, says);
        let imported = bundle::import(&store, || Ok(Cursor::new(bytes.clone())));
        assert!(imported.unwrap_err().to_string().contains(says));
        let generation = store.snapshot().unwrap();
        assert_eq!(generation.number(), 0);
        assert!(bundle::held(&generation).is_empty());
    }
    // Nothing was staged for good either: the store's objects are empty.
    let objects = std::fs::read_dir(dir.path().join("store/objects")).unwrap();
    assert_eq!(objects.count(), 0);
}

#[test]
fn a_member_not_named_by_the_manifest_or_a_named_one_missing_is_refused() {
    let chain = chain("bundle-unnamed");
    let members = Members::of(&made(&chain).bytes);
    let removal = chain.removal.clone();
    // A file, a run and a diagnostic no one names.
    let mut extra = members.clone();
    let stray = whizbang_core::identity::Digest::of_bytes(b"stray");
    extra
        .members
        .insert(format!("files/sha256-{}", stray.hex()), b"stray".to_vec());
    refused_for(&extra.bytes(), "is not one the manifest names");
    let mut extra = members.clone();
    extra
        .members
        .insert("runs/9-9/record".to_owned(), b"{}".to_vec());
    refused_for(&extra.bytes(), "is not one the manifest names");
    let mut extra = members.clone();
    extra.members.insert(
        format!("runs/{removal}/diagnostics/stray.txt"),
        b"stray".to_vec(),
    );
    refused_for(&extra.bytes(), "is not one its run's record lists");
    let mut extra = members.clone();
    extra
        .members
        .insert("notes/readme".to_owned(), b"stray".to_vec());
    refused_for(&extra.bytes(), "is not one the manifest names");
    // A file, a record and a journal it names, gone.
    let mut missing = members.clone();
    let file = members
        .members
        .keys()
        .find(|n| n.starts_with("files/"))
        .unwrap();
    missing.members.remove(&file.clone());
    refused_for(&missing.bytes(), "which the bundle does not hold");
    let mut missing = members.clone();
    missing.members.remove(&format!("runs/{removal}/record"));
    refused_for(&missing.bytes(), "whose record the bundle does not hold");
    let mut missing = members.clone();
    missing.members.remove(&format!("runs/{removal}/journal"));
    refused_for(&missing.bytes(), "neither holds nor withholds");
    // The manifest elsewhere than first.
    let mut later = members.bytes();
    let mut writer = tar::Writer::new(0);
    for (name, bytes) in &members.members {
        writer.add(name, bytes).unwrap();
    }
    writer.add("bundle.json", &later[512..1024]).unwrap();
    later = writer.finish();
    refused_for(&later, "not the manifest");
}

#[test]
fn an_artifact_whose_file_digest_or_decoded_identity_differs_is_refused() {
    let chain = chain("bundle-artifacts");
    let members = Members::of(&made(&chain).bytes);
    let files: Vec<String> = (members.members.keys())
        .filter(|name| name.starts_with("files/"))
        .cloned()
        .collect();
    // Other bytes under a file's name.
    let mut other = members.clone();
    let (first, second) = (&files[0], &files[1]);
    let bytes = members.members[second].clone();
    other.members.insert(first.clone(), bytes);
    refused_for(&other.bytes(), "is not the bytes its name says");
    // A file the manifest says is another artifact.
    let mut renamed = members.clone();
    let artifacts = renamed.manifest["artifacts"].as_array_mut().unwrap();
    let elsewhere = artifacts[1]["artifact"].clone();
    artifacts[0]["artifact"] = elsewhere;
    refused_for(&renamed.bytes(), "decodes to the artifact");
    // A file the manifest says is of another type.
    let mut retyped = members.clone();
    let artifacts = retyped.manifest["artifacts"].as_array_mut().unwrap();
    let kind = artifacts[0]["type"].as_str().unwrap().to_owned();
    artifacts[0]["type"] = json!(if kind == "image" { "mask" } else { "image" });
    refused_for(&retyped.bytes(), "that this build reads");
}

#[test]
fn a_diagnostic_whose_bytes_differ_from_its_record_is_refused() {
    let chain = chain("bundle-diagnostics");
    let members = Members::of(&made(&chain).bytes);
    let fill = members.run_of(0, "fill");
    let name = format!("runs/{fill}/diagnostics/fill-answer.png");
    let mut changed = members.clone();
    let bytes = changed.members.get_mut(&name).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    refused_for(&changed.bytes(), "is not the bytes its record says");
    // A diagnostic cut short.
    let mut cut = members.clone();
    cut.members.get_mut(&name).unwrap().pop();
    refused_for(&cut.bytes(), "is not the bytes its record says");
    // A journal that is not one is refused by its own reader.
    let mut journal = members.clone();
    journal
        .members
        .insert(format!("runs/{fill}/journal"), b"not a journal".to_vec());
    refused_for(&journal.bytes(), "the journal of");
}

#[test]
fn a_manifest_that_contradicts_a_record_or_a_link_that_contradicts_both_is_refused() {
    let chain = chain("bundle-contradicts");
    let members = Members::of(&made(&chain).bytes);
    let mut action = members.clone();
    action.manifest["steps"][0]["action"] = json!("remove");
    refused_for(&action.bytes(), "contradicts the record");
    let mut version = members.clone();
    version.manifest["steps"][1]["version"] = json!("0.0.1");
    refused_for(&version.bytes(), "contradicts the record");
    let mut root = members.clone();
    root.manifest["steps"][0]["run"] = json!(members.runs(0)[1]);
    refused_for(&root.bytes(), "does not start with its run");
    // An input that names another identity than its record took.
    let mut input = members.clone();
    let other = members.manifest["artifacts"][0]["artifact"].clone();
    input.manifest["steps"][1]["inputs"][1]["artifact"] = other;
    refused_for(&input.bytes(), "names another artifact");
    // A use of a file at a port its record does not have.
    let mut used = members.clone();
    used.manifest["artifacts"][0]["of"][0]["port"] = json!("elsewhere");
    refused_for(&used.bytes(), "the manifest contradicts the record");
    // A link to another port, to the step itself, and to a later step.
    let link = &members.manifest["steps"][1]["inputs"][0]["from"];
    assert_eq!(link, &json!({"step": 0, "port": "image", "place": 0}));
    for wrong in [
        json!({"step": 0, "port": "mask", "place": 0}),
        json!({"step": 0, "port": "image", "place": 1}),
        json!({"step": 1, "port": "image", "place": 0}),
    ] {
        let mut linked = members.clone();
        linked.manifest["steps"][1]["inputs"][0]["from"] = wrong;
        refused_for(&linked.bytes(), "which no earlier step's record committed");
    }
    // Both an identity and a link.
    let mut both = members.clone();
    both.manifest["steps"][1]["inputs"][0]["artifact"] = json!(chain.removed.0.to_string());
    refused_for(&both.bytes(), "both or neither");
}

#[test]
fn a_family_with_a_missing_parent_a_cycle_or_two_roots_is_refused() {
    let chain = chain("bundle-family");
    let members = Members::of(&made(&chain).bytes);
    let root = chain.removal.clone();
    let (first, second) = (members.run_of(0, "segment"), members.run_of(0, "select"));
    let parent = format!("\"parent\":\"{root}\"");
    // A parent the bundle does not hold.
    let mut missing = members.clone();
    missing.edit_record(&first, &parent, "\"parent\":\"1-404\"");
    refused_for(&missing.bytes(), "which the bundle does not hold");
    // Two runs each the other's parent.
    let mut cycle = members.clone();
    cycle.edit_record(&first, &parent, &format!("\"parent\":\"{second}\""));
    cycle.edit_record(&second, &parent, &format!("\"parent\":\"{first}\""));
    refused_for(&cycle.bytes(), "make a cycle");
    // A run of the family that names no parent: a second root.
    let mut two = members.clone();
    two.edit_record(&first, &parent, "\"parent\":null");
    refused_for(&two.bytes(), "a family has two roots");
    // A parent in another step's family.
    let mut across = members.clone();
    across.edit_record(&first, &parent, &format!("\"parent\":\"{}\"", chain.check));
    refused_for(&across.bytes(), "a parent of another step's family");
}

#[test]
fn a_child_listed_by_one_parent_and_naming_another_or_listed_twice_is_refused() {
    let chain = chain("bundle-listed");
    let members = Members::of(&made(&chain).bytes);
    let root = chain.removal.clone();
    let (first, second) = (members.run_of(0, "segment"), members.run_of(0, "select"));
    let parent = format!("\"parent\":\"{root}\"");
    // Listed by the root, and naming its sibling as its parent.
    let mut other = members.clone();
    other.edit_record(&second, &parent, &format!("\"parent\":\"{first}\""));
    refused_for(&other.bytes(), "is listed as a call by");
    // Listed by its sibling too.
    let mut twice = members.clone();
    let listed = format!(
        "\"steps\":[{{\"step\":\"call\",\"run\":\"{second}\",\"action\":\"select\",\"status\":\"completed\"}}]"
    );
    twice.edit_record(&first, "\"steps\":[]", &listed);
    refused_for(&twice.bytes(), "listed as a call by two runs, or twice");
    // Listed with another ending than its record's.
    let mut ending = members.clone();
    let noted = format!("\"run\":\"{first}\",\"action\":\"segment\",\"status\":\"completed\"");
    let wrong = format!("\"run\":\"{first}\",\"action\":\"segment\",\"status\":\"cancelled\"");
    ending.edit_record(&root, &noted, &wrong);
    refused_for(&ending.bytes(), "another ending");
    // Named as a parent by a run it does not list.
    let mut unlisted = members.clone();
    let segment = format!("\"run\":\"{first}\",\"action\":\"segment\"");
    unlisted.edit_record(&root, &segment, "\"run\":\"1-404\",\"action\":\"segment\"");
    refused_for(&unlisted.bytes(), "whose record the bundle does not hold");
}

#[test]
fn an_interrupted_parent_with_lost_steps_and_a_null_ending_of_a_recovered_child_are_kept() {
    let chain = chain("bundle-interrupted");
    let made = whizbang::bundling::make(&chain.world.root, &[&chain.removal]).unwrap();
    let members = Members::of(&made.bytes);
    let root = chain.removal.clone();
    let root_id = RunId::parse(&root).unwrap();
    let fill = members.run_of(0, "fill");
    let fill_record = RunRecord::from_bytes(members.record(&fill).as_bytes()).unwrap();

    // A child recovered as interrupted, which its parent noted with no ending.
    let mut recovered = members.clone();
    let left = Left {
        run: &RunId::parse(&fill).unwrap(),
        whizbang: env!("CARGO_PKG_VERSION"),
        admitted: Some(Admitted {
            request: None,
            parent: Some(&root_id),
            action: "fill",
            version: fill_record.asked().version.as_deref().unwrap(),
            reserved_micros: None,
            start_ms: fill_record.time().start_ms.unwrap(),
        }),
        end_ms: fill_record.time().end_ms,
    };
    let interrupted = RunRecord::interrupted(left).unwrap();
    recovered.strip(&fill);
    recovered
        .members
        .insert(format!("runs/{fill}/record"), interrupted.to_bytes());
    let noted = format!("\"run\":\"{fill}\",\"action\":\"fill\",\"status\":\"completed\"");
    let unknown = format!("\"run\":\"{fill}\",\"action\":\"fill\",\"status\":null");
    // Its ending as its parent noted it, and the root's outputs, which were the fill's.
    let mut root_record = recovered.record(&root);
    root_record = root_record.replacen(&noted, &unknown, 1);
    recovered
        .members
        .insert(format!("runs/{root}/record"), root_record.into_bytes());
    let checked = bundle::check(Cursor::new(recovered.bytes()));
    assert!(checked.is_ok(), "{}", checked.unwrap_err());

    // A root recovered as interrupted, its steps lost: its calls still name it.
    let mut lost = members.clone();
    let root_record = RunRecord::from_bytes(members.record(&root).as_bytes()).unwrap();
    let left = Left {
        run: &root_id,
        whizbang: env!("CARGO_PKG_VERSION"),
        admitted: Some(Admitted {
            request: None,
            parent: None,
            action: REMOVE,
            version: root_record.asked().version.as_deref().unwrap(),
            reserved_micros: None,
            start_ms: root_record.time().start_ms.unwrap(),
        }),
        end_ms: root_record.time().end_ms,
    };
    let interrupted = RunRecord::interrupted(left).unwrap();
    lost.strip(&root);
    lost.members
        .insert(format!("runs/{root}/record"), interrupted.to_bytes());
    lost.manifest["steps"][0]["inputs"] = json!([]);
    lost.manifest["steps"][0]["config"] = json!(null);
    let checked = bundle::check(Cursor::new(lost.bytes()));
    assert!(checked.is_ok(), "{}", checked.unwrap_err());
    // Its run list is whole: every call it made is carried.
    assert_eq!(checked.unwrap().manifest.steps[0].runs, members.runs(0));
}

#[test]
fn an_unknown_version_is_refused_naming_it() {
    let chain = chain("bundle-version");
    let members = Members::of(&made(&chain).bytes);
    for (version, named) in [(json!(2), "2"), (json!("one"), "\"one\""), (json!(0), "0")] {
        let mut other = members.clone();
        other.manifest["bundle"] = version;
        refused_for(
            &other.bytes(),
            &format!("its manifest's version is {named}"),
        );
    }
    let mut none = members.clone();
    none.manifest.as_object_mut().unwrap().remove("bundle");
    refused_for(&none.bytes(), "its manifest's version is none");
}

/// A reader that counts what is read from it.
struct Counting<'a> {
    from: Cursor<Vec<u8>>,
    read: &'a std::cell::Cell<u64>,
}

impl std::io::Read for Counting<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.from.read(buffer)?;
        self.read.set(self.read.get() + read as u64);
        Ok(read)
    }
}

#[test]
fn a_bundle_over_its_bounds_is_refused_before_it_is_read_whole() {
    let checked = |bytes: Vec<u8>| {
        let read = std::cell::Cell::new(0);
        let said = bundle::check(Counting {
            from: Cursor::new(bytes),
            read: &read,
        });
        (said.unwrap_err().to_string(), read.get())
    };
    // A manifest that says it is over its bound: refused at its header.
    let mut big = tar::header("bundle.json", bundle::MANIFEST_MAX_BYTES + 1, 0)
        .unwrap()
        .to_vec();
    big.extend(std::iter::repeat_n(b' ', 4096));
    let (said, read) = checked(big);
    assert!(said.contains("over its bound of 1048576"), "{said}");
    assert_eq!(read, 512);
    // A member that says it is over the whole's bound: refused at its header.
    let chain = chain("bundle-bounds");
    let members = Members::of(&made(&chain).bytes);
    let mut over = members.bytes();
    over.truncate(over.len() - 1024);
    let before = over.len() as u64;
    let file = members
        .members
        .keys()
        .find(|n| n.starts_with("files/"))
        .unwrap();
    over.extend_from_slice(&tar::header(file, bundle::MAX_BYTES, 0).unwrap());
    over.extend(std::iter::repeat_n(0u8, 4096));
    let (said, read) = checked(over);
    assert!(said.contains("over its bound of 268435456 bytes"), "{said}");
    assert_eq!(read, before + 512);
    // More members than a bundle holds: refused at the first past the bound.
    let removal = &chain.removal;
    let mut many = members.clone();
    for at in 0..bundle::MEMBERS_LIMIT {
        many.members.insert(
            format!("runs/{removal}/diagnostics/extra-{at:05}"),
            Vec::new(),
        );
    }
    let said = refused(&many.bytes());
    assert!(said.contains("more than 4096 members"), "{said}");
    // A file that ends inside a member.
    let mut short = members.bytes();
    short.truncate(2048);
    let said = refused(&short);
    assert!(said.contains("ends inside a header or a member"), "{said}");
    // Something after the end.
    let mut after = members.bytes();
    after.extend_from_slice(b"more");
    let said = refused(&after);
    assert!(said.contains("after the archive's end"), "{said}");
}

#[test]
fn a_long_member_name_round_trips_through_prefix_and_name_and_one_too_long_is_refused() {
    let run = format!("1{}", "2".repeat(63));
    let diagnostic = format!("d{}", "e".repeat(63));
    let long = format!("runs/{run}/diagnostics/{diagnostic}");
    assert!(long.len() > tar::NAME_BYTES, "{}", long.len());
    let (prefix, name) = tar::split(&long).unwrap();
    assert!(!prefix.is_empty() && prefix.len() <= tar::PREFIX_BYTES);
    assert!(name.len() <= tar::NAME_BYTES);
    assert_eq!(format!("{prefix}/{name}"), long);
    let mut writer = tar::Writer::new(0);
    writer.add(&long, b"kept").unwrap();
    writer.add("short", b"too").unwrap();
    let bytes = writer.finish();
    // The header holds the two parts apart.
    assert_eq!(&bytes[345..345 + prefix.len()], prefix.as_bytes());
    assert_eq!(&bytes[..name.len()], name.as_bytes());
    let mut reader = tar::Reader::new(Cursor::new(bytes), 1 << 20);
    let head = reader.next_head().unwrap().unwrap();
    assert_eq!(head.name, long);
    assert_eq!(reader.bytes(&head).unwrap(), b"kept");
    let head = reader.next_head().unwrap().unwrap();
    assert_eq!(head.name, "short");
    assert_eq!(reader.bytes(&head).unwrap(), b"too");
    assert_eq!(reader.next_head().unwrap(), None);
    // A name of no `/` past 100 bytes, a last part past 100, a first part past 155, and
    // a name with a NUL, cannot be written.
    for name in [
        "a".repeat(101),
        format!("runs/{}", "b".repeat(101)),
        format!("{}/c", "a".repeat(200)),
        "runs/a\0b".to_owned(),
        String::new(),
    ] {
        let said = tar::Writer::new(0).add(&name, b"x").unwrap_err();
        assert!(said.to_string().contains("cannot be a member's name"));
    }
    // A run id at its longest and a diagnostic's name at its longest fit.
    assert!(
        tar::split(&format!(
            "runs/{}/diagnostics/{}",
            "1".repeat(64),
            "d".repeat(64)
        ))
        .is_ok()
    );
}

#[test]
fn an_opaque_part_a_missing_picture_and_a_cached_check_are_listed_as_withheld() {
    // The judge answers with an opaque part beside its submission, and the second
    // check, of the same job, is answered from the decision cache.
    let submit = json!({"tool_call": {"id": "s1", "name": "submit_verdict",
                        "arguments": passes()}});
    let turn = json!({"parts": [{"opaque": {"adapter": "loopkit",
                                             "body": "MARMALADE kept opaque"}}, submit]});
    let mut setup = Setup::answering(vec![turn]);
    setup.cached = true;
    let chain = chain_of("bundle-withheld", setup);
    let again = checking(&chain.world, chain.removed);
    let made = whizbang::bundling::make(
        &chain.world.root,
        &[&chain.removal, &chain.check, again.run().as_str()],
    )
    .unwrap();
    let manifest = manifest(&made);
    let opaque = manifest.withheld.iter().find_map(|item| match item {
        Withheld::OpaquePart {
            adapter,
            bytes,
            why,
            ..
        } => Some((adapter.clone(), *bytes, why.clone())),
        _ => None,
    });
    let (adapter, bytes, why) = opaque.expect("the opaque part is withheld");
    assert_eq!((adapter.as_str(), bytes), ("loopkit", 21));
    assert!(why.contains("never its body"), "{why}");
    // Its body is nowhere in the bundle.
    let whole = made
        .bytes
        .windows(21)
        .any(|window| window == b"MARMALADE kept opaque");
    assert!(!whole, "the body travelled");
    let cached = manifest.withheld.iter().find_map(|item| match item {
        Withheld::CachedCheck {
            run, answered_by, ..
        } => Some((run.clone(), answered_by.clone())),
        _ => None,
    });
    let (cached, by) = cached.expect("the cached check is withheld");
    assert!(manifest.steps[2].runs.contains(&cached));
    let first_check = Members::of(&made.bytes).run_of(1, "check");
    assert_eq!(by.as_deref(), Some(first_check.as_str()));
    // Every picture the judge was shown is an image the bundle carries.
    assert!(!(manifest.withheld.iter()).any(|item| matches!(item, Withheld::Picture { .. })));
    // A picture a stored exchange names that no image of the runs has: an exchange of
    // the check is made to name other pixels, as one would whose picture the store
    // let go of, with its record's digest of it made to agree.
    let unknown = naming_other_pixels(&chain.world, &first_check);
    let made =
        whizbang::bundling::make(&chain.world.root, &[&chain.removal, &chain.check]).unwrap();
    let manifest = self::manifest(&made);
    let picture = manifest.withheld.iter().find_map(|item| match item {
        Withheld::Picture {
            run, pixels, why, ..
        } => Some((run.clone(), *pixels, why.clone())),
        _ => None,
    });
    let (run, pixels, why) = picture.expect("the picture is withheld");
    assert_eq!((run.as_str(), pixels.0), (first_check.as_str(), unknown));
    assert!(why.contains("no image the bundle carries"), "{why}");
}

/// Makes the first exchange of the run `run` name pixels no image has, and its record's
/// digest of that exchange agree: what a store holds of a judge shown a picture it no
/// longer has. Gives the pixels' digest.
fn naming_other_pixels(world: &Looped, run: &str) -> whizbang_core::identity::Digest {
    use whizbang_core::identity::Digest;
    let store = Store::open(&world.root, Arc::new(RealFs)).unwrap();
    let generation = store.snapshot().unwrap();
    let held = generation.run(run).unwrap();
    let (name, entry) = (held.entries().iter())
        .find(|(name, _)| name.starts_with("diagnostics/exchange-"))
        .unwrap();
    let text = String::from_utf8(store.read(entry).unwrap()).unwrap();
    let unknown = Digest::of_bytes(b"pixels nobody has");
    let at = text.find("\"pixels\":\"sha256:").unwrap() + "\"pixels\":\"".len();
    let changed = format!("{}{unknown}{}", &text[..at], &text[at + 71..]);
    let record = String::from_utf8(store.read(held.get("record").unwrap()).unwrap()).unwrap();
    let (old, new) = (
        Digest::of_bytes(text.as_bytes()),
        Digest::of_bytes(changed.as_bytes()),
    );
    assert!(record.contains(&old.to_string()));
    let record = record.replacen(&old.to_string(), &new.to_string(), 1);
    let mut publication = store.begin().unwrap();
    let kind = whizbang_core::store::Kind::Diagnostic;
    publication
        .put_in(run, name, kind, changed.as_bytes())
        .unwrap();
    publication
        .put_in(run, "record", kind, record.as_bytes())
        .unwrap();
    publication.publish().unwrap();
    unknown
}

#[test]
fn a_step_of_a_recipe_or_tool_file_is_carried_as_not_replayable() {
    let world = Looped::new("bundle-recipe", &Setup::answering(vec![passes()]).on_disk());
    let recipe = r#"version = 1
name = "outline"
purpose = "Grows what a phrase names by a margin."
use = "In a test of a bundle of a user's recipe."

[inputs]
image = { type = "Image" }
prompt = { type = "Text" }

[[step]]
id = "found"
op = "segment"
inputs = { image = "inputs.image" }
query = "inputs.prompt"

[[step]]
id = "kept"
op = "select"
inputs = { mask = "found.mask" }

[[step]]
id = "grown"
op = "grow"
inputs = { mask = "kept.mask" }
px = 2

[outputs]
mask = "grown.mask"
"#;
    let over: Over = world.over.clone().recipe(recipe.to_owned());
    let mut registry = wiring::registry_over(&over, false).unwrap();
    // An action no build ships, as a tool file's is: the test kit's echo.
    registry
        .register(Arc::new(whizbang_testkit::echo::Echo::new()))
        .unwrap();
    let engine = world.engine_of(registry);
    let outlined = engine
        .submit(
            &Call::new("outline")
                .argument("prompt", Value::text(PHRASE))
                .input("image", world.image_id()),
        )
        .unwrap();
    assert_eq!(
        outlined.outcome().name(),
        "completed",
        "{:?}",
        outlined.outcome()
    );
    let echoed = engine
        .submit(&Call::new("echo").argument("text", Value::text("hello")))
        .unwrap();
    assert!(matches!(echoed.outcome(), Outcome::Completed { .. }));
    let removed = engine
        .submit(
            &Call::new(REMOVE)
                .argument("prompt", Value::text(PHRASE))
                .input("image", world.image_id()),
        )
        .unwrap();
    drop(engine);
    let made = whizbang::bundling::make(
        &world.root,
        &[
            outlined.run().as_str(),
            echoed.run().as_str(),
            removed.run().as_str(),
        ],
    )
    .unwrap();
    let manifest = manifest(&made);
    for step in &manifest.steps[..2] {
        assert!(!step.replayable, "{step:?}");
        let why = step.why.as_deref().unwrap();
        assert!(why.contains("is not an action this build ships"), "{why}");
        assert!(why.contains("never runs"), "{why}");
    }
    assert!(manifest.steps[2].replayable);
    // The recipe itself is not in the bundle: its runs' records are, as evidence.
    let recipe_text: &[u8] = b"op = \"grow\"";
    assert!(
        !made
            .bytes
            .windows(recipe_text.len())
            .any(|w| w == recipe_text)
    );
}

#[test]
fn the_manifest_holds_no_path_and_no_secret() {
    let chain = chain("bundle-no-path");
    let made = made(&chain);
    let said = String::from_utf8(made.manifest.to_bytes()).unwrap();
    let root = chain.world.root.display().to_string();
    let scratch = chain.world.root.parent().unwrap().display().to_string();
    for absent in [
        root.as_str(),
        scratch.as_str(),
        "whizbang.toml",
        "/Users/",
        "/home/",
        "/tmp",
        "example.org",
        "loopkit",
    ] {
        assert!(
            !said.contains(absent),
            "the manifest holds {absent}: {said}"
        );
    }
    // No path anywhere in the bundle either: the records and diagnostics hold none.
    for absent in [root.as_str(), scratch.as_str()] {
        let found = made
            .bytes
            .windows(absent.len())
            .any(|window| window == absent.as_bytes());
        assert!(!found, "the bundle holds {absent}");
    }
    // Every text of the manifest is an id, a name, a version, a digest or a sentence
    // the writer wrote.
    let document: Json = serde_json::from_str(&said).unwrap();
    let keys: Vec<&str> = document
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["artifacts", "bundle", "steps", "whizbang", "withheld"]
    );
}

#[test]
fn a_chain_links_an_input_to_the_earlier_output_it_took() {
    let chain = chain("bundle-links");
    let manifest = manifest(&made(&chain));
    let step = &manifest.steps[1];
    assert_eq!(step.inputs.len(), 2);
    assert_eq!(step.inputs[0].port, "images");
    assert_eq!(step.inputs[0].place, 0);
    assert_eq!(step.inputs[0].artifact, None);
    let link = step.inputs[0].from.as_ref().unwrap();
    assert_eq!((link.step, link.port.as_str(), link.place), (0, "image", 0));
    // The picture it came from is not an output of the chain: it is named.
    assert_eq!(step.inputs[1].place, 1);
    assert_eq!(step.inputs[1].artifact, Some(chain.world.image_id().0));
    assert_eq!(step.inputs[1].from, None);
    // The first step's input is the picture too.
    assert_eq!(
        manifest.steps[0].inputs[0].artifact,
        Some(chain.world.image_id().0)
    );
    assert_eq!(chain.removed.0, {
        let removal = &manifest.steps[0];
        let file = manifest.artifacts.iter().find(|file| {
            file.of.iter().any(|used| {
                used.run == removal.run && used.port == "image" && used.role == bundle::Role::Output
            })
        });
        file.unwrap().artifact
    });
}

#[test]
fn an_input_two_earlier_outputs_could_be_refuses_the_chain() {
    let chain = chain("bundle-ambiguous");
    // A second removal of the same picture makes the same picture.
    let again = removal(&chain.world);
    assert_eq!(output(&again, "image"), chain.removed);
    let said = whizbang::bundling::make(
        &chain.world.root,
        &[&chain.removal, again.run().as_str(), &chain.check],
    )
    .unwrap_err();
    assert!(said.contains("the input images of step 2"), "{said}");
    assert!(said.contains("step 0") && said.contains("step 1"), "{said}");
    // A child named as a step is refused, naming its root.
    let members = Members::of(&made(&chain).bytes);
    let child = members.runs(0)[1].clone();
    let said = whizbang::bundling::make(&chain.world.root, &[&child]).unwrap_err();
    assert!(
        said.contains(&format!("give {} instead", chain.removal)),
        "{said}"
    );
    // A run given twice, and none.
    let said =
        whizbang::bundling::make(&chain.world.root, &[&chain.removal, &chain.removal]).unwrap_err();
    assert!(said.contains("given twice"), "{said}");
    assert!(whizbang::bundling::make(&chain.world.root, &[]).is_err());
}
