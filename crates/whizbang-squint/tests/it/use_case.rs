//! The table of use cases (plan 2C, task A1, item 1): its rows, squint's rule files byte
//! for byte, and the aggregation for every kind.

use serde_json::{Value as Json, json};
use whizbang_core::contract::record::Decision;
use whizbang_core::identity::Digest;
use whizbang_squint::job::{Job, Role};
use whizbang_squint::use_case::{
    self, Checklist, Confidence, Disposition, Instance, Kind, Row, TABLE, row,
};

/// The job the command line makes for a use case: a source where it takes one, and one
/// reference where it takes them.
pub fn fitting(name: &str, brief: Option<&str>) -> Job {
    let roles = use_case::rules(name).unwrap().rules.requires_roles;
    let references = usize::from(roles.contains(&Role::Reference));
    Job::shorthand(name, roles.contains(&Role::Source), references, brief)
}

/// A row of the table as the test states it: the name, the roles, the rules with their
/// kinds, and the checklist of a fitting job.
type Stated = (
    &'static str,
    &'static [Role],
    &'static [(&'static str, Kind)],
    &'static [&'static str],
);

#[test]
fn each_of_the_five_use_cases_is_in_the_table_with_its_roles_rules_and_checklist() {
    use Kind::{Allowed as A, MustHold as H, MustNot as N, Observe as O};
    use Role::{Candidate as C, Reference as R, Source as S};
    let rows: [Stated; 5] = [
        (
            "source-fidelity",
            &[C, S],
            &[("SF1", H), ("SF2", H), ("SF3", A), ("SF4", O)],
            &["chk:ob:SF1@candidate", "chk:ob:SF2@candidate"],
        ),
        (
            "reference-consistency",
            &[C, R],
            &[("RC1", H), ("RC2", H), ("RC3", H), ("RC4", A), ("RC5", O)],
            &[
                "chk:ob:RC1@candidate",
                "chk:ob:RC2@candidate",
                "chk:ob:RC3@candidate",
                "chk:ref:reference-1",
            ],
        ),
        (
            "reality-checks",
            &[C],
            &[("RE1", N), ("RE2", N), ("RE3", N), ("RE4", N), ("RE5", O)],
            &[
                "chk:ob:RE1@candidate",
                "chk:ob:RE2@candidate",
                "chk:ob:RE3@candidate",
                "chk:ob:RE4@candidate",
            ],
        ),
        (
            "brief-compliance",
            &[C],
            &[("BC1", H), ("BC2", N), ("BC3", H), ("BC4", A), ("BC5", O)],
            &[
                "chk:ob:BC1@candidate",
                "chk:ob:BC2@candidate",
                "chk:ob:BC3@candidate",
            ],
        ),
        (
            "product-fidelity",
            &[C, R],
            &[("PF1", H), ("PF2", H), ("PF3", H), ("PF4", A), ("PF5", O)],
            &[
                "chk:ob:PF1@candidate",
                "chk:ob:PF2@candidate",
                "chk:ob:PF3@candidate",
                "chk:ref:reference-1",
            ],
        ),
    ];
    assert_eq!(use_case::NAMES, rows.map(|(name, ..)| name));
    for (name, roles, rules, checklist) in rows {
        let read = use_case::rules(name).unwrap();
        assert_eq!(read.rules.use_case, name);
        assert_eq!(read.rules.requires_roles, roles, "{name}");
        let kinds: Vec<(&str, Kind)> = read
            .obligations
            .iter()
            .map(|o| (o.id.as_str(), o.kind))
            .collect();
        assert_eq!(kinds, rules, "{name}");
        let job = fitting(name, Some("A brief."));
        let resolved = use_case::resolve(&job).unwrap();
        let built = Checklist::build(&resolved, &job);
        assert_eq!(built.ids().collect::<Vec<_>>(), checklist, "{name}");
        // The prompt numbers every rule and names the kind of each in words.
        let system = use_case::system(&resolved, &job);
        for (id, kind) in rules {
            let said = match kind {
                H => format!("{id} (must hold;"),
                N => format!("{id} (must not occur;"),
                A => format!("{id} (allowed;"),
                O => format!("{id} (observe;"),
            };
            assert!(system.contains(&said), "{name}: {said}");
        }
    }
    assert!(use_case::rules("general").is_none());
}

fn instance(kind: Kind, matters: bool, hidden_ok: bool) -> Instance {
    Instance {
        obligation_id: "XX1".into(),
        target: "candidate".into(),
        kind,
        matters,
        hidden_ok,
        text: String::new(),
        evidence: Vec::new(),
        evidence_kinds: Vec::new(),
        provenance: "rules",
    }
}

/// An answer for each item of the checklist: `held`, confidently, but for the one
/// named, which is given this disposition and confidence.
pub fn answers(checklist: &Checklist, named: &str, disposition: &str, confidence: &str) -> Json {
    let items: Vec<Json> = checklist
        .ids()
        .map(|id| {
            let (d, c) = if id == named {
                (disposition, confidence)
            } else {
                ("held", "confident")
            };
            let location = (d == "violation").then(|| {
                json!({"kind": "bbox", "frame": "raw:candidate", "bbox": [0.1, 0.1, 0.4, 0.4],
                       "polygon": null, "yaw": null, "pitch": null, "fov": null})
            });
            json!({"id": id, "disposition": d, "confidence": c, "reason": "Looked at.",
                   "allowed_by": null, "evidence": [], "location": location})
        })
        .collect();
    json!({"schema_version": 1, "items": items, "findings": [],
           "opinion": {"verdict": "pass", "reason": "Recorded only."}})
}

#[test]
fn a_confident_must_not_violation_fails_and_a_tentative_one_is_unsure() {
    // Squint's table, cell by cell, for a must_hold and a must_not alike: the kind
    // never changes the row.
    use Confidence::{Confident, Tentative};
    use Disposition::{AllowedChange, CantTell, Held, Hidden, Observed, Violation};
    let expected = |d, c, matters: bool, hidden_ok: bool| match (d, c) {
        (Violation, Confident) => Row::Fail,
        (Violation, Tentative) => Row::Unsure,
        (CantTell, _) | (Held | AllowedChange, Tentative) if matters => Row::Unsure,
        (Hidden, _) if matters && !hidden_ok => Row::Unsure,
        _ => Row::Recorded,
    };
    let mut cells = 0;
    for kind in [Kind::MustHold, Kind::MustNot] {
        for d in [Violation, Held, AllowedChange, Hidden, CantTell, Observed] {
            for c in [Confident, Tentative] {
                for (matters, hidden_ok) in
                    [(true, true), (true, false), (false, true), (false, false)]
                {
                    cells += 1;
                    let got = row(&instance(kind, matters, hidden_ok), d, c);
                    assert_eq!(
                        got,
                        expected(d, c, matters, hidden_ok),
                        "{kind:?} {d:?} {c:?}"
                    );
                }
            }
        }
    }
    assert_eq!(cells, 96);
    // Through a submission of reality-checks, whose rules are all must_not.
    let job = fitting("reality-checks", None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    let decided = |confidence: &str| {
        let submitted = answers(&checklist, "chk:ob:RE3@candidate", "violation", confidence);
        use_case::decided_of(&submitted, &job, &resolved, &checklist).unwrap()
    };
    let confident = decided("confident");
    assert_eq!(confident.verdict.decision(), Decision::Fail);
    assert!(
        confident
            .verdict
            .why()
            .starts_with("chk:ob:RE3@candidate: violation (confident)")
    );
    assert_eq!(decided("tentative").verdict.decision(), Decision::Unsure);
    let held = answers(&checklist, "", "held", "confident");
    let held = use_case::decided_of(&held, &job, &resolved, &checklist).unwrap();
    assert_eq!(held.verdict.decision(), Decision::Pass);
}

/// The SHA-256 of each file at the commit `docs/sources.toml` pins for squint:
/// `crates/squint-core/rules/<name>.toml`, and `crates/squint-core/prompts/v1.md`.
const SQUINTS: [(&str, &str); 5] = [
    (
        "source-fidelity",
        "21e48f14cae39a05e5ae160c2e3a1973a96adf8036916575b0d817ae419e1da7",
    ),
    (
        "reference-consistency",
        "15c89f179ef9d51467367a457ada8677e9b386335ab914f461861ef942134dc5",
    ),
    (
        "reality-checks",
        "b9ff72502160e27cdb00e3496c774c40fa013b906f4d8d693056c0d0295cfc54",
    ),
    (
        "brief-compliance",
        "09cad5a029f0afec4f9cd4fba97f8c9401158495bf6901371eb2381aac2e9ea0",
    ),
    (
        "product-fidelity",
        "8af991822da4f66476a905ba935cb7c786a85009d99ab871d7d24745986f2084",
    ),
];
const SQUINTS_TEMPLATE: &str = "4c5297e8d5829cafb4be5b9d5763a83bbc9a6c181ed103959c0c412279b05de7";

#[test]
fn the_ported_rules_are_squints_rules_byte_for_byte_less_the_fields_dropped() {
    for ((name, text), (squints, digest)) in TABLE.iter().zip(SQUINTS) {
        assert_eq!(*name, squints);
        assert_eq!(Digest::of_bytes(text.as_bytes()).hex(), digest, "{name}");
    }
    let template = include_str!("../../prompts/v1.md");
    assert_eq!(
        Digest::of_bytes(template.as_bytes()).hex(),
        SQUINTS_TEMPLATE
    );
    // Nothing of the files is lost in reading: no base file holds a field that was
    // dropped (a profile, `extends`, `[screen]`; plan mE, task T1, item 4, with a line in
    // the carry-over list), and a file that holds one is not read. `[hints]` is read
    // (`checklist::hints_are_read_from_a_rule_file_and_given_to_their_tools_as_arguments`),
    // and no base file holds one.
    for (name, text) in TABLE {
        assert!(use_case::rules(name).is_some(), "{name}");
        for dropped in ["profile", "extends", "[hints", "[screen"] {
            assert!(!text.contains(dropped), "{name}: {dropped}");
        }
    }
    let screened = format!("{}\n[screen]\ntools = []\n", TABLE[0].1);
    assert!(toml::from_str::<use_case::Rules>(&screened).is_err());
    for key in [
        "profile = \"architecture\"",
        "extends = \"source-fidelity\"",
    ] {
        let text = TABLE[0]
            .1
            .replace("[rules]\n", &format!("[rules]\n{key}\n"));
        assert!(toml::from_str::<use_case::Rules>(&text).is_err(), "{key}");
    }
}

#[test]
fn a_rejection_names_the_frames_and_rules_the_judge_may_use() {
    // E1's live run (plan 2C, task F1): of ten decisions ended as invalid submissions,
    // the first rejection of five was a frame that was not raw:, and the second of
    // most a finding that cited nothing or the wrong kind of rule. Each rejection names
    // what the judge may give instead, of the ids it was given, and nothing it wrote.
    let job = fitting("reference-consistency", None);
    let resolved = use_case::resolve(&job).unwrap();
    let checklist = Checklist::build(&resolved, &job);
    let rejected = |finding: Json| {
        let mut submitted = answers(&checklist, "", "held", "confident");
        submitted["findings"] = json!([finding]);
        use_case::decided_of(&submitted, &job, &resolved, &checklist)
            .unwrap_err()
            .message
    };
    let finding = |ids: Json, disposition: &str, frame: &str| {
        json!({"obligation_ids": ids, "disposition": disposition, "confidence": "confident",
               "reason": "Seen.", "allowed_by": null, "evidence": [],
               "location": {"kind": "whole_image", "frame": frame, "bbox": null,
                            "polygon": null, "yaw": null, "pitch": null, "fov": null}})
    };
    let said = rejected(finding(json!(["RC1"]), "violation", "aligned:candidate:t1"));
    assert!(
        said.contains("not one of raw:candidate, raw:reference-1:"),
        "{said}"
    );
    let said = rejected(finding(json!([]), "violation", "raw:candidate"));
    assert!(
        said.ends_with("cites no obligation: obligation_ids lists the rules it bears on, of the obligations RC1, RC2, RC3, or for an observed finding the observe rules RC5"),
        "{said}"
    );
    let said = rejected(finding(json!(["RC4"]), "allowed_change", "raw:candidate"));
    assert!(said.contains("RC4 is an exemption"), "{said}");
    assert!(
        said.ends_with("RC4 waives nothing, so it is never an allowed_by"),
        "{said}"
    );
    let said = rejected(finding(json!(["RC1"]), "observed", "raw:candidate"));
    assert!(
        said.contains("observed findings cite observe rules only (RC5);"),
        "{said}"
    );
    let mut text = answers(&checklist, "", "held", "confident");
    text["items"] = Json::String(text["items"].to_string());
    let said = use_case::decided_of(&text, &job, &resolved, &checklist)
        .unwrap_err()
        .message;
    assert!(said.contains(": items is text;"), "{said}");
}

#[test]
fn each_use_case_is_a_recipe_with_its_rules_details_and_tools() {
    use whizbang_core::recipe::{Recipe, Written};
    // Each use case of the table, and no other, has a recipe, in the table's order.
    let named: Vec<&str> = (whizbang_squint::recipes::SHIPPED.iter())
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(named, use_case::NAMES);
    let text = |value: &str| Written::Text(value.to_owned());
    for (name, file) in whizbang_squint::recipes::SHIPPED {
        let recipe = Recipe::read(file).unwrap();
        assert_eq!(recipe.name, name);
        assert_eq!(recipe.use_case.as_deref(), Some(name));
        // Two steps: the details measured first, then the judge's.
        let steps: Vec<(&str, &str)> = (recipe.steps.iter())
            .map(|step| (step.id.as_str(), step.op.as_str()))
            .collect();
        assert_eq!(
            steps,
            [("details", "details"), ("judge", "check")],
            "{name}"
        );
        let (details, judge) = (&recipe.steps[0], &recipe.steps[1]);
        // Its rules: the use case's, whose rule file the judge's step reads.
        assert!(use_case::rules(name).is_some(), "{name}");
        for step in [details, judge] {
            assert_eq!(step.params["use_case"], text(name), "{name}");
            assert_eq!(step.params["job"], text("inputs.job"), "{name}");
        }
        // Nothing runs before the judge as shipped (plan m4, task A1: the control arm of
        // the gate of details first); the tool a run that sets `first` measures with,
        // and what the step hands the judge's.
        assert_eq!(details.params["details"], text("none"), "{name}");
        assert_eq!(details.params["detail"], text("align"), "{name}");
        assert_eq!(details.inputs["images"], "inputs.images");
        assert_eq!(judge.params["measured"], text("details.measurements"));
        assert_eq!(judge.inputs["measured_images"], "details.images");
        assert_eq!(judge.inputs["images"], "inputs.images");
        // The tools the judge may call, its output limit (the door's), and the second
        // look, off in every recipe: on reality-checks it raised false accepts (plan m3,
        // task U1), and elsewhere a run sets it.
        assert_eq!(judge.params["tools"], text("view,align"), "{name}");
        assert_eq!(judge.params["projection"], text("visual_only"), "{name}");
        assert_eq!(judge.params["second_look"], text("off"), "{name}");
        assert_eq!(judge.params["max_output_tokens"], Written::Integer(2048));
        assert_eq!(judge.params["judge"], text("inputs.judge"));
    }
    // Each is checked against the actions of the build, as every recipe is, and answers
    // with the judge's verdict.
    use std::sync::Arc;
    use whizbang_core::contract::{Kind as Level, Registry, ResultKind};
    let mut registry = Registry::new();
    let toolbox = whizbang_squint::check::Toolbox::standard();
    registry
        .register(Arc::new(
            whizbang_squint::check::Check::new(toolbox).unwrap(),
        ))
        .unwrap();
    registry
        .register(Arc::new(whizbang_squint::view::View::new().unwrap()))
        .unwrap();
    registry
        .register(Arc::new(whizbang_squint::align::Align::new().unwrap()))
        .unwrap();
    registry
        .register(Arc::new(whizbang_squint::resize::Resize::new().unwrap()))
        .unwrap();
    for compare in [
        whizbang_squint::compare::Compare::change_map(),
        whizbang_squint::compare::Compare::edge_similarity(),
    ] {
        registry.register(Arc::new(compare.unwrap())).unwrap();
    }
    let details = whizbang_squint::measured::DetailsStep::new().unwrap();
    registry.register(Arc::new(details)).unwrap();
    for (name, file) in whizbang_squint::recipes::SHIPPED {
        let recipe = super::world::recipe_of(file, &registry);
        let descriptor = whizbang_core::contract::Action::descriptor(&recipe);
        assert_eq!(descriptor.name(), name);
        assert_eq!(descriptor.kind(), Level::Recipe);
        assert!(descriptor.answers_with(ResultKind::Verdict), "{name}");
        assert!(descriptor.input("images").is_some(), "{name}");
        assert!(recipe.recipe().outputs.is_empty(), "{name}");
    }
}

#[test]
fn each_shipped_recipe_called_with_nothing_set_is_the_control_arm() {
    // Plan m4, task A1, items 1 and 2: a recipe called by a caller that sets nothing is
    // the control arm of the gate of details first, whole (arm C,
    // `crates/whizbang-eval/src/matrix.rs`): nothing measured first, `view` and `align`
    // offered and no `resize`, and a call of `align` answered with its pictures alone.
    use super::check::{checklist_of, paired, results, texts, turns};
    use super::runner::{argument, calls, steps_of};
    use whizbang_core::artifact::ArtifactId;
    use whizbang_core::contract::{Executor, Secrets};
    for (name, _) in whizbang_squint::recipes::SHIPPED {
        let (world, scene, candidate, _) = paired();
        let job = fitting(
            name,
            (name == "brief-compliance").then_some("A scene, kept."),
        );
        let images: Vec<ArtifactId> = [candidate.id(), scene.id()]
            .into_iter()
            .take(job.images.len())
            .collect();
        // The judge of a use case that relates a pair calls `align` on it, then answers.
        let pair = job.images.get(1).map(|other| other.id.clone());
        let checklist = checklist_of(&job);
        let submit = json!({"tool_call": {"id": "s1", "name": "submit_verdict",
            "arguments": answers(&checklist, "", "held", "confident")}});
        let mut script = Vec::new();
        if let Some(other) = &pair {
            let align = json!({"tool_call": {"id": "a1", "name": "align",
                "arguments": {"image": "candidate", "against": other}}});
            script.push((json!([align]), "tool_use"));
        }
        script.push((json!([submit]), "tool_use"));
        let judge = super::world::judge(&turns(&script), &[]);
        let executor = world.engine(judge.clone(), Secrets::none());
        // Nothing is set: the job, the images, and no override.
        let finished = executor
            .submit(&super::world::World::recipe(&job, &images))
            .unwrap();
        drop(executor);
        assert_eq!(finished.outcome().name(), "completed", "{name}");
        let steps = steps_of(&world, &finished);
        let (details, judged) = (&steps[0].1, &steps[1].1);
        // No details: the step ran as `none`, started no tool and handed on nothing.
        assert_eq!(argument(details, "details").as_deref(), Some("none"));
        assert_eq!(calls(details), [], "{name}");
        assert!(details.record().outputs().is_empty(), "{name}");
        let received = judge.received();
        let opening = texts(received[0].messages()[0].parts());
        assert!(
            !opening
                .iter()
                .any(|said| said == whizbang_squint::details::DETAILS_INTRO
                    || said.starts_with("align (control)")),
            "{name}: {opening:?}"
        );
        // `view` and `align` are offered, and no `resize`.
        assert_eq!(argument(judged, "tools").as_deref(), Some("view,align"));
        let offered: Vec<&str> = received[0].tools().iter().map(|tool| tool.name()).collect();
        assert_eq!(offered, ["view", "align", "submit_verdict"], "{name}");
        // `visual_only`: what the judge's step ran under, and, where the use case relates
        // a pair, what a call of `align` was answered with: the fixed sentence, nothing
        // measured, and the aligned picture.
        assert_eq!(
            argument(judged, "projection").as_deref(),
            Some("visual_only"),
            "{name}"
        );
        if pair.is_some() {
            let result = results(&judge, 1).remove(0);
            let said: Json = serde_json::from_str(result.text()).unwrap();
            assert_eq!(said["status"], "ok", "{name}");
            assert_eq!(
                said["summary"], "Result computed; its measurements are withheld.",
                "{name}"
            );
            assert_eq!(said["measurements"], json!([]), "{name}");
            assert_eq!(result.images().len(), 1, "{name}");
        }
    }
}
