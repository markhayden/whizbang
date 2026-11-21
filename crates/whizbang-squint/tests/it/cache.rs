//! `cache::…`: the decision cache (ADR 0031) under a use case's recipe (plan m3, task Q1,
//! item 1): its key takes the recipe's digest.

use std::sync::Arc;

use whizbang_core::capability::written::read_script;
use whizbang_core::contract::{Executor, Secrets};
use whizbang_squint::job::Job;
use whizbang_testkit::capabilities::FakeJudge;

use super::check::{paired, submitting};
use super::runner::{argument, steps_of};
use super::world::World;

/// The setting `decision_cache` of the judge's step of a recipe run on the pair, under a
/// judge that reports its model.
fn decided(
    world: &World,
    source: &whizbang_core::artifact::Image,
    candidate: &whizbang_core::artifact::Image,
) -> (String, Option<String>) {
    let turns = read_script(submitting(&[]).as_bytes()).unwrap();
    let turns = (turns.into_iter())
        .map(|turn| turn.map(|turn| turn.reported_by(Some("fake-judge"))))
        .collect();
    let judge = Arc::new(FakeJudge::scripted(turns));
    let executor = world.engine(judge, Secrets::none());
    let job = Job::shorthand("source-fidelity", true, 0, None);
    let finished = executor
        .submit(&World::recipe(&job, &[candidate.id(), source.id()]))
        .unwrap();
    drop(executor);
    assert_eq!(
        finished.outcome().name(),
        "completed",
        "{:?}",
        finished.outcome()
    );
    let steps = steps_of(world, &finished);
    let judged = &steps[1].1;
    let settings = &judged.record().asked().settings;
    let said = (settings.iter())
        .find(|s| s.name() == whizbang_squint::cache::SETTING)
        .map(|s| format!("{:?}", s.value()))
        .unwrap_or_default();
    (said, argument(judged, "recipe_digest"))
}

#[test]
fn a_changed_use_case_recipe_misses_and_an_unchanged_one_hits() {
    let (mut world, source, candidate, _) = paired();
    world.cached = true;
    let (first, digest) = decided(&world, &source, &candidate);
    assert!(first.contains("\"kept\""), "{first}");
    // Unchanged: the same recipe's run is answered from the first.
    let (again, same) = decided(&world, &source, &candidate);
    assert!(again.contains("\"hit\""), "{again}");
    assert_eq!(same, digest);
    // Changed: a recipe whose text differs, though its steps do not, misses.
    let at = world
        .recipes
        .iter()
        .position(|text| text.contains("name = \"source-fidelity\""))
        .unwrap();
    world.recipes[at].push_str("\n# changed\n");
    let (changed, other) = decided(&world, &source, &candidate);
    assert!(changed.contains("\"kept\""), "{changed}");
    assert_ne!(other, digest);
}

#[test]
fn the_same_pixels_in_the_fast_encoders_file_miss_what_the_old_encoders_file_was_answered() {
    // Plan m3, task O5: the canonical encoder became the fast one on 2026-11-19, which
    // gives another file, and so another image identity, for the same pixels. The key
    // takes each image by its identity, so a cached answer about the old file is not
    // served for the new one, though the pixels and the binding are the same.
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use whizbang_core::identity::{DecodeLimits, Projection, Topology, encode, identify};

    let (mut world, source, candidate, _) = paired();
    world.cached = true;
    let pixels = identify(candidate.file(), &DecodeLimits::default())
        .unwrap()
        .pixels;
    let mut old = Vec::new();
    PngEncoder::new_with_quality(&mut old, CompressionType::Default, FilterType::Adaptive)
        .write_image(
            pixels.rgba(),
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let new = encode::png(&pixels).unwrap();
    assert!(old != new, "the core's encoder writes another file");
    let (old, new) = (
        super::support::image_in(old, Projection::Flat, Topology::Bounded),
        super::support::image_in(new, Projection::Flat, Topology::Bounded),
    );
    assert_ne!(old.id(), new.id());
    world.hold(&[&old, &new]);
    let (first, _) = decided(&world, &source, &old);
    assert!(first.contains("\"kept\""), "{first}");
    let (again, _) = decided(&world, &source, &old);
    assert!(again.contains("\"hit\""), "{again}");
    let (fast, _) = decided(&world, &source, &new);
    assert!(fast.contains("\"kept\""), "{fast}");
}

#[test]
fn the_submission_setting_is_in_the_key() {
    use super::check::{item, of_pair, submission_v2, turns};
    use serde_json::json;
    // Plan e1, task F1, item 1: a run of v2 is never answered by a run of v1, nor the
    // reverse, though everything else is the same.
    let (mut world, source, candidate, _) = paired();
    world.cached = true;
    let items = json!([
        item("chk:ob:SF1@candidate", "held", "confident"),
        item("chk:ob:SF2@candidate", "held", "confident"),
    ]);
    let ran = |submission: &str| -> String {
        let script = if submission == "v2" {
            turns(&[(json!([submission_v2(&items)]), "tool_use")])
        } else {
            submitting(&[])
        };
        let turns = read_script(script.as_bytes()).unwrap();
        let turns = (turns.into_iter())
            .map(|turn| turn.map(|turn| turn.reported_by(Some("fake-judge"))))
            .collect();
        let executor = world.engine(Arc::new(FakeJudge::scripted(turns)), Secrets::none());
        let call = of_pair(&source, &candidate, &[("submission", submission)]);
        let finished = executor.submit(&call).unwrap();
        drop(executor);
        assert_eq!(
            finished.outcome().name(),
            "completed",
            "{:?}",
            finished.outcome()
        );
        let left = super::world::left(&world.root(), world.fs.clone(), finished.run());
        let settings = &left.record().asked().settings;
        (settings.iter())
            .find(|s| s.name() == whizbang_squint::cache::SETTING)
            .map(|s| format!("{:?}", s.value()))
            .unwrap_or_default()
    };
    assert!(ran("v1").contains("\"kept\""));
    assert!(ran("v1").contains("\"hit\""));
    assert!(ran("v2").contains("\"kept\""));
    assert!(ran("v2").contains("\"hit\""));
}
