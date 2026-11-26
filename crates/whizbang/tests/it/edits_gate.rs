//! `edits_gate::…`: `whizbang eval edits gate` (plan mE, task M2, item 2), scored from a
//! store that holds a run of the loop and a run of `remove_prompt`, each under a request
//! id of its own, made with the loop's test kit ([`crate::checked`]). Its marks and picks
//! are made up here, in a folder of the test's own.

use std::path::{Path, PathBuf};

use whizbang::edits_gate::{self, Gating};
use whizbang::pairs_page::{self, FromRuns};
use whizbang_core::contract::{Answer, Call, Executor, Request, Value};
use whizbang_eval::lab::digest_of;
use whizbang_rascal::checked::{NAME, REMOVE};
use whizbang_testkit::scratch::Scratch;

use crate::checked::{Looped, PHRASE, Setup, passes, remains};

/// The judge the loop is asked, by its name in the kit's config.
pub const JUDGE: &str = "plain";

/// The request ids of the two runs, and the case they are of.
pub const LOOP_REQUEST: &str = "m2-loop-c01";
pub const REMOVE_REQUEST: &str = "m2-remove-c01";
pub const CASE: &str = "case-one";
pub const FAMILY: &str = "family-one";

/// A store with a run of the loop (its first attempt judged to leave the thing, its
/// second passed) and a run of `remove_prompt` on the same picture; the cases file of each
/// and a protocol of the one case, in a folder outside any work tree.
pub struct Recorded {
    pub world: Looped,
    pub dir: Scratch,
    pub loop_run: String,
    pub remove_run: String,
    pub loop_cases: PathBuf,
    pub remove_cases: PathBuf,
    pub protocol: PathBuf,
}

fn cases_text(action: &str, request: &str) -> String {
    format!(
        "kind = \"edit-cases/1\"\naction = \"{action}\"\n\n[[case]]\nid = \"{CASE}\"\n\
         family = \"{FAMILY}\"\nrun = \"{request}\"\nwhat = \"{PHRASE}\"\n"
    )
}

/// [`recorded_of`] with both runs.
pub fn recorded(label: &str) -> Recorded {
    recorded_of(label, true)
}

/// The store and files, with the loop's run only when `with_loop`: a test of
/// `remove_prompt`'s run alone does not pay for the loop's.
pub fn recorded_of(label: &str, with_loop: bool) -> Recorded {
    let mut setup = Setup::answering(vec![remains(), passes()]);
    setup.dear = true;
    let world = Looped::new(label, &setup);
    let engine = world.engine();
    let ran = |call: Call, request: &str| {
        let id = engine.request_id(request).unwrap();
        match engine.request(&Request::new(call).with_id(id)).unwrap() {
            Answer::Ran(finished) => finished.run().as_str().to_owned(),
            other => panic!("{other:?}"),
        }
    };
    let loop_run = if with_loop {
        ran(
            world.call().argument("judge", Value::text(JUDGE)),
            LOOP_REQUEST,
        )
    } else {
        String::new()
    };
    let remove = Call::new(REMOVE)
        .argument("prompt", Value::text(PHRASE))
        .input("image", world.image_id());
    let remove_run = ran(remove, REMOVE_REQUEST);
    drop(engine);
    world.spill();
    let dir = Scratch::new(&format!("{label}-files"));
    let write = |name: &str, text: String| {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    };
    let loop_cases = write("loop-cases.toml", cases_text(NAME, LOOP_REQUEST));
    let remove_cases = write("remove-cases.toml", cases_text(REMOVE, REMOVE_REQUEST));
    let protocol = write(
        "protocol.toml",
        format!(
            "version = 1\nname = \"m2-pairs\"\nseed = 7\narms = [\"recipe\", \"source\"]\n\
             min_decided = 1\n\n[[group]]\nname = \"all\"\nimages = [\"{CASE}\"]\n"
        ),
    );
    Recorded {
        world,
        dir,
        loop_run,
        remove_run,
        loop_cases,
        remove_cases,
        protocol,
    }
}

impl Recorded {
    /// The pairs of a cases file's runs, written in `out` under the test's folder; the
    /// path of their assignment.
    pub fn pairs(&self, cases: &Path, reference: Option<&Path>, out: &str) -> PathBuf {
        let out = self.dir.path().join(out);
        let asked = FromRuns {
            cases,
            protocol: &self.protocol,
            reference,
            root: &self.world.root,
            out: &out,
            limit: None,
            force: false,
        };
        pairs_page::pairs_from_runs(&asked).unwrap();
        out.join("assignment.json")
    }
}

/// Picks of every pair of an assignment, each for the recipe's side, or against it.
pub fn picks_for(assignment: &Path, for_the_recipe: bool) -> PathBuf {
    let bytes = std::fs::read(assignment).unwrap();
    let drawn: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let picks: Vec<serde_json::Value> = (drawn["pair"].as_array().unwrap().iter())
        .map(|pair| {
            let a = pair["a_side"].as_str().unwrap();
            let other = if a == "left" { "right" } else { "left" };
            let pick = if for_the_recipe { a } else { other };
            serde_json::json!({ "pair": pair["id"], "pick": pick })
        })
        .collect();
    let file = assignment.with_file_name("picks.json");
    let text = serde_json::json!({
        "picks_version": 1,
        "assignment_digest": digest_of(&bytes),
        "pick": picks,
    });
    std::fs::write(&file, text.to_string()).unwrap();
    file
}

/// A gate file of the recipe beside the cases and the protocol, for one case of one family.
fn gate(recorded: &Recorded, recipe: &str, cases: &Path, rung: u8, from: &str) -> PathBuf {
    let pin = |path: &Path| digest_of(&std::fs::read(path).unwrap());
    let name = cases.file_name().unwrap().to_str().unwrap();
    let judge = if from == "check" {
        format!("judge = \"{JUDGE}\"\n")
    } else {
        String::new()
    };
    let admits = if rung == 4 {
        format!("\n[admits]\nrung = 4\nkind = \"recipe\"\nname = \"{recipe}\"\n")
    } else {
        String::new()
    };
    let text = format!(
        "gate = \"edit-gate/1\"\nname = \"m2-{rung}\"\nrecipe = \"{recipe}\"\nrung = {rung}\n\
         corpus = \"owners/fresh\"\ncases = \"{name}\"\ncases_sha256 = \"{}\"\nhypotheses = 1\nlevel = 0.95\nseed = 3\n\
         min_families = 1\n\n[success]\nfrom = \"{from}\"\n{judge}bar = 0.75\n\
         lower_above = 0.1\n\n[preservation]\nexact = true\n\n[quality]\n\
         protocol = \"protocol.toml\"\nprotocol_sha256 = \"{}\"\n\
         outcomes = [\"preferred\", \"not_worse\"]\nmin_picked = 1\n\n[errors]\nbelow = 0.5\n\n\
         [cost]\nguide_micros = 1000000\ntimes = 2.0\n\n[latency]\ntimes = 2.0\n\n\
         [[latency.record]]\npixels = 4096\nms = 600000\n{admits}",
        pin(cases),
        pin(&recorded.protocol)
    );
    let file = recorded
        .dir
        .path()
        .join(format!("{recipe}-gate-{rung}.toml"));
    std::fs::write(&file, text).unwrap();
    file
}

#[test]
fn a_loops_runs_are_gated_from_their_checks_and_the_receipt_holds_no_case() {
    let recorded = recorded("m2-gate-checks");
    let assignment = recorded.pairs(&recorded.loop_cases, None, "pairs");
    let picks = picks_for(&assignment, true);
    let file = gate(&recorded, NAME, &recorded.loop_cases, 3, "check");
    let receipt = recorded.dir.path().join("receipts/m2.json");
    std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
    let gating = Gating {
        gate: &file,
        root: &recorded.world.root,
        marks: None,
        picks: Some((assignment.as_path(), picks.as_path())),
        receipt: &receipt,
        commit: "0123456",
    };
    let read = edits_gate::score(&gating).unwrap();
    let d = &read.decided;
    // The kept attempt's own check passed; its fill touched nothing outside its mask.
    assert_eq!(d.outcome, "admitted", "{:?}", d.why);
    assert_eq!((d.success.successes, d.success.of), (1, 1));
    assert_eq!((d.preservation.finished, d.preservation.preserved), (1, 1));
    assert!(d.cost.mean_spent_micros.unwrap() > 0.0);
    assert_eq!(d.quality.groups[0].outcome, "preferred");
    let written = std::fs::read_to_string(&receipt).unwrap();
    for word in [
        CASE,
        FAMILY,
        PHRASE,
        LOOP_REQUEST,
        recorded.loop_run.as_str(),
    ] {
        assert!(!written.contains(word), "{word}");
    }
    // A rung 3 receipt admits nothing.
    let json: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(json["outcome"], "admitted");
    assert!(whizbang::admitted::admits("d", &json).is_none());
    // Picks against the recipe: its quality is worse, and it is rejected.
    let against = picks_for(&assignment, false);
    let gating = Gating {
        picks: Some((assignment.as_path(), against.as_path())),
        ..gating
    };
    let read = edits_gate::score(&gating).unwrap();
    assert_eq!(read.decided.outcome, "rejected");
    // Without a decode it is inconclusive, never a pass.
    let gating = Gating {
        picks: None,
        ..gating
    };
    assert_eq!(
        edits_gate::score(&gating).unwrap().decided.outcome,
        "inconclusive"
    );
    // A check is of pixels: remove_prompt's fill is the loop's first, the same mask filled
    // by the same filler, so the loop's check of that attempt, which failed, is its check.
    let file = gate(&recorded, REMOVE, &recorded.remove_cases, 3, "check");
    let gating = Gating {
        gate: &file,
        ..gating
    };
    let read = edits_gate::score(&gating).unwrap();
    assert_eq!(
        (
            read.decided.success.successes,
            read.decided.success.unscored
        ),
        (0, 0)
    );
    assert!(
        read.decided
            .why
            .contains(&"task_success_below_its_bar".to_owned())
    );
    // A finished edit no check was made of is unscored, and the gate inconclusive: here,
    // under a judge no check was asked of.
    let other = std::fs::read_to_string(&file)
        .unwrap()
        .replace(&format!("judge = \"{JUDGE}\""), "judge = \"nobody\"");
    std::fs::write(&file, other).unwrap();
    let read = edits_gate::score(&gating).unwrap();
    assert_eq!(read.decided.success.unscored, 1);
    assert_eq!(read.decided.outcome, "inconclusive");
}

#[test]
fn a_rung_4_gate_reads_a_persons_marks_and_admits_its_recipe() {
    let recorded = recorded_of("m2-gate-marks", false);
    let assignment = recorded.pairs(&recorded.remove_cases, None, "pairs");
    let picks = picks_for(&assignment, true);
    let sheet_out = recorded.dir.path().join("sheet");
    whizbang::loop_sheet::sheet_of_cases(&whizbang::loop_sheet::AskedCases {
        cases: &recorded.remove_cases,
        root: &recorded.world.root,
        out: &sheet_out,
    })
    .unwrap();
    let sheet = sheet_out.join("assignment.json");
    let drawn: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&sheet).unwrap()).unwrap();
    let marks = |visible: bool, damaged: bool| {
        let file = recorded.dir.path().join("marks.json");
        let text = serde_json::json!({
            "marks_version": 1,
            "sheet": whizbang::loop_score::sheet_id(drawn["seed"].as_u64().unwrap()),
            "marks": [{ "picture": drawn["pictures"][0]["key"], "visible": visible,
                        "damaged": damaged }],
        });
        std::fs::write(&file, text.to_string()).unwrap();
        file
    };
    let file = gate(&recorded, REMOVE, &recorded.remove_cases, 4, "marks");
    let receipts = recorded.dir.path().join("receipts");
    std::fs::create_dir_all(&receipts).unwrap();
    let receipt = receipts.join("m2-4.json");
    let gone = marks(false, false);
    let gating = Gating {
        gate: &file,
        root: &recorded.world.root,
        marks: Some((gone.as_path(), sheet.as_path())),
        picks: Some((assignment.as_path(), picks.as_path())),
        receipt: &receipt,
        commit: "0123456",
    };
    let read = edits_gate::score(&gating).unwrap();
    assert_eq!(read.decided.outcome, "admitted", "{:?}", read.decided.why);
    assert!(read.marked);
    let (admitted, receipts_read) = whizbang::admitted::read(&receipts).unwrap();
    assert_eq!(receipts_read, 1);
    assert_eq!(admitted.len(), 1);
    assert_eq!(
        (admitted[0].kind.as_str(), admitted[0].name.as_str()),
        ("recipe", REMOVE)
    );
    // A receipt on an exposed corpus admits nothing, whatever it says.
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt).unwrap()).unwrap();
    json["corpus"] = "labs/m3-edits".into();
    assert!(whizbang::admitted::admits("d", &json).is_none());
    // Something else damaged: no success, and the recipe is not admitted.
    let damaged = marks(false, true);
    let gating = Gating {
        marks: Some((damaged.as_path(), sheet.as_path())),
        ..gating
    };
    assert_eq!(
        edits_gate::score(&gating).unwrap().decided.outcome,
        "rejected"
    );
    // A gate of marks given no marks is refused; marks inside a work tree are never read.
    let none = Gating {
        marks: None,
        ..gating
    };
    assert!(edits_gate::score(&none).unwrap_err().contains("--labels"));
    let tree = recorded.dir.path().join("repo");
    std::fs::create_dir_all(tree.join(".git")).unwrap();
    let inside = tree.join("marks.json");
    std::fs::copy(&gone, &inside).unwrap();
    let gating = Gating {
        marks: Some((inside.as_path(), sheet.as_path())),
        ..gating
    };
    let refused = edits_gate::score(&gating).unwrap_err();
    assert!(refused.contains("inside a git work tree"), "{refused}");
    // A rung 4 gate's cases are read only from outside a work tree.
    for name in ["remove-cases.toml", "protocol.toml"] {
        std::fs::copy(recorded.dir.path().join(name), tree.join(name)).unwrap();
    }
    let inside_gate = tree.join("gate.toml");
    std::fs::copy(&file, &inside_gate).unwrap();
    let gating = Gating {
        gate: &inside_gate,
        marks: Some((gone.as_path(), sheet.as_path())),
        ..gating
    };
    let refused = edits_gate::score(&gating).unwrap_err();
    assert!(refused.contains("fresh corpus"), "{refused}");
}

#[test]
fn a_seams_mask_is_mapped_onto_its_panorama_through_its_cut() {
    // `seam` hands back its ramp's mask on the cut's canvas, 2s columns wide: through the
    // plan of its run's cut, it falls on the panorama's two edge strips, and nothing the
    // seam left alone is counted as changed.
    let world = Looped::new("m2-gate-seam", &Setup::answering(Vec::new()).on_disk());
    // A panorama of 128 by 64 whose wrap edge is a step, as `rascal seam`'s test has.
    let (w, h) = (128_u32, 64_u32);
    let rgba: Vec<u8> = (0..w * h)
        .flat_map(|at| [(100 + (at % w) / 4) as u8, 120, 90, 255])
        .collect();
    let pixels = whizbang_core::identity::CanonicalPixels::from_rgba(w, h, rgba).unwrap();
    let panorama = whizbang_core::artifact::Image::new(
        whizbang_core::identity::encode::png(&pixels).unwrap(),
        whizbang_core::identity::Projection::Equirectangular,
        whizbang_core::identity::Topology::Wrap,
        whizbang_core::artifact::ImageRole::Picture,
        whizbang_core::artifact::Origin::new(whizbang_core::artifact::Step::new("fixture", "1"))
            .unwrap(),
        &whizbang_core::identity::DecodeLimits::default(),
    )
    .unwrap();
    let image = {
        let store = whizbang_core::store::Store::open(
            &world.root,
            std::sync::Arc::new(whizbang_core::store::fs::RealFs),
        )
        .unwrap();
        let mut publication = store.begin().unwrap();
        let artifact = whizbang_core::artifact::Artifact::Image(panorama);
        let id = whizbang_core::artifact::put(&mut publication, &artifact).unwrap();
        publication.publish().unwrap();
        id
    };
    let engine = world.engine();
    let id = engine.request_id("m2-seam-c01").unwrap();
    let call = Call::new("seam").input("image", image);
    match engine.request(&Request::new(call).with_id(id)).unwrap() {
        Answer::Ran(finished) => {
            let record = world.record(finished.run().as_str());
            assert_eq!(
                finished.outcome().name(),
                "completed",
                "{:?}",
                record.outcome()
            );
        }
        other => panic!("{other:?}"),
    }
    drop(engine);
    world.spill();
    let dir = Scratch::new("m2-gate-seam-files");
    let cases = dir.path().join("seam-cases.toml");
    std::fs::write(&cases, cases_text("seam", "m2-seam-c01")).unwrap();
    let text = std::fs::read_to_string(&cases).unwrap();
    let cases_read = whizbang_eval::edit_gate::Cases::parse(&text).unwrap();
    let held = edits_gate::case_runs(&world.root, &cases_read).unwrap();
    let gate = whizbang_eval::edit_gate::EditGate::parse(&format!(
        "gate = \"edit-gate/1\"\nname = \"m2-seam\"\nrecipe = \"seam\"\nrung = 3\n\
         corpus = \"owners/fresh\"\ncases = \"seam-cases.toml\"\ncases_sha256 = \"{}\"\n\
         hypotheses = 1\nlevel = 0.95\nseed = 3\nmin_families = 1\n\n[success]\n\
         from = \"check\"\nbar = 0.75\nlower_above = 0.1\n\n[preservation]\nexact = true\n\n\
         [quality]\nprotocol = \"protocol.toml\"\nprotocol_sha256 = \"{}\"\n\
         outcomes = [\"preferred\"]\nmin_picked = 1\n\n[errors]\nbelow = 0.5\n\n[cost]\n\
         guide_micros = 1000000\ntimes = 2.0\n\n[latency]\ntimes = 2.0\n\n\
         [[latency.record]]\npixels = 4096\nms = 600000\n",
        digest_of(text.as_bytes()),
        digest_of(b"none")
    ))
    .unwrap();
    let observed = edits_gate::observe(&world.root, &held, &gate, None).unwrap();
    let whizbang_eval::edit_gate::Observed::Finished(finished) = &observed[0].observed else {
        panic!("the seam's run did not finish");
    };
    assert_eq!(finished.changed_outside, 0);
    // Its mask is not of the panorama's size: without the plan it could not be read.
    let mask = held.runs[0].mask.unwrap();
    let reader = whizbang_core::store::Reader::open(
        &world.root,
        std::sync::Arc::new(whizbang_core::store::fs::RealFs),
    )
    .unwrap();
    let read = whizbang_core::artifact::get_anywhere(
        &reader,
        whizbang_core::artifact::ArtifactType::Mask,
        mask,
        &whizbang_core::identity::DecodeLimits::default(),
    )
    .unwrap()
    .into_mask()
    .unwrap();
    assert!(read.space().width() < 128, "{}", read.space().width());
}
