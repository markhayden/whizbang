//! `measure` (plan m3, task O5, item 1). The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/measure.rs` at the commit `docs/sources.toml` pins): two
//! halves of 120 by 20 about column 60, a band of half-width 6 with rascal's windows,
//! and the numbers rascal's measures give on them; then the action through the
//! executor.

use std::sync::Arc;

use whizbang_core::artifact::{
    self, Artifact, ArtifactId, ArtifactType, BandProfile, Image, Line, Location, Mask, Series,
    Shape, Source, UseCase, Windows,
};
use whizbang_core::capability::{Painted, Picture};
use whizbang_core::contract::{Call, Engine, NoBackends, Registry, Value};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, Digest};
use whizbang_rascal::composite::composite;
use whizbang_rascal::fan::{luminance, sobel_magnitude};
use whizbang_rascal::measure::{
    Measuring, continuity, drift, hue_difference, measures_of, summarise, texture_match, tone_step,
    transition,
};
use whizbang_rascal::profile::band_core;

use super::canvases::{World, image, invalid, put, ran};
use super::grow::made_by;

const W: u32 = 120;
const H: u32 = 20;
const C: u32 = 60;

/// Left and right halves of two flat colours, with a texture of amplitude `amp` whose
/// Sobel gradient is nowhere zero: rascal's `halves`.
fn halves(left: [u8; 3], right: [u8; 3], amp: i16) -> CanonicalPixels {
    let rgba = (0..W * H)
        .flat_map(|at| {
            let (x, y) = (at % W, at / W);
            let base = if x < C { left } else { right };
            let t = (((x * 7 + y * 13) % 5) as i16 - 2) * amp;
            let c = |v: u8| (i16::from(v) + t).clamp(0, 255) as u8;
            [c(base[0]), c(base[1]), c(base[2]), 255]
        })
        .collect();
    CanonicalPixels::from_rgba(W, H, rgba).unwrap()
}

/// Rascal's band: column 60, thin 3, the half-width `half` on every row, and its windows.
fn band(on: &Image, half: f64) -> BandProfile {
    BandProfile::new(
        on.binding(),
        ArtifactId(Digest::of_bytes(b"the line")),
        C,
        3.0,
        vec![half; H as usize],
        Windows {
            gap: 1,
            step_columns: 4,
            texture_columns: 10,
        },
        "fan-v1",
        made_by("fan"),
    )
    .unwrap()
}

/// The band mask of rascal's test: the band's core, its profile, and a blend of 128
/// from the core's edge to four pixels beyond.
fn band_mask(on: &Image, half: f64) -> Mask {
    let b = band(on, half);
    let mut mask = Mask::from_core(
        on.binding(),
        UseCase::Seam,
        band_core(W, C, b.half()),
        Source {
            locations: vec![b.line()],
            instances: Vec::new(),
        },
        made_by("profile"),
    )
    .unwrap();
    let sigma = Series::new(
        on.binding(),
        "sigma_px",
        "px",
        1,
        vec![1.0; H as usize],
        made_by("profile"),
    )
    .unwrap();
    mask.set_profile(b, sigma).unwrap();
    let blend = (0..W * H)
        .map(|at| {
            let d = (f64::from(at % W) + 0.5 - f64::from(C)).abs();
            if d > half && d <= half + 4.0 { 128 } else { 0 }
        })
        .collect();
    mask.set_blend(blend).unwrap();
    mask
}

/// The numbers a run of `set` gives, by name.
fn numbers(
    after: &CanonicalPixels,
    before: Option<&CanonicalPixels>,
    mask: &Mask,
    set: &str,
) -> Vec<(String, f64)> {
    let measured = Measuring {
        after,
        before,
        mask,
        column: None,
        chroma_floor: 1.0,
        percentile: 95.0,
    }
    .run(&measures_of(set).unwrap());
    measured
        .numbers
        .into_iter()
        .map(|(n, v, _)| (n, v))
        .collect()
}

fn number(numbers: &[(String, f64)], name: &str) -> f64 {
    numbers
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name} in {numbers:?}"))
        .1
}

#[test]
fn the_ported_vectors_give_rascals_measures() {
    // Continuity. Flat halves of 120 and 132 with no texture: the only difference on a
    // row is the step of 12 between columns 59 and 60, the row's mean is 12 / 119, so
    // the ratio is 119 on every row.
    let after = halves([120; 3], [132; 3], 0);
    let rows = continuity(&after, C);
    assert_eq!((rows.skipped, rows.rows.len()), (0, H as usize));
    for v in &rows.values {
        assert!((v - 119.0).abs() < 1e-9, "{v}");
    }
    // With a texture of 2 everywhere the step is diluted, and a flat image skips each row.
    for v in &continuity(&halves([120; 3], [132; 3], 2), C).values {
        assert!(*v > 1.0 && *v < 119.0, "{v}");
    }
    let flat = continuity(&halves([120; 3], [120; 3], 0), C);
    assert_eq!((flat.skipped, flat.values.len()), (H, 0));
    // Summaries: the nearest-rank percentile and the median of an even count.
    let s = summarise(&[1.0, 2.0, 3.0, 4.0], 95.0);
    assert_eq!((s.mean, s.median, s.p95, s.max), (2.5, 2.5, 4.0, 4.0));

    // Tone step. Grey halves: a step in lightness, none in chroma, and the hue left out.
    let grey = image(&halves([100; 3], [140; 3], 0), false);
    let b = band(&grey, 6.0);
    let [dl, dc, dh] = tone_step(&halves([100; 3], [140; 3], 0), &b, 1.0);
    assert_eq!(dl.skipped, 0);
    for v in &dl.values {
        assert!(*v > 10.0 && *v < 25.0, "L 100 to 140 grey: {v}");
    }
    for v in &dc.values {
        assert!(v.abs() < 0.5, "{v}");
    }
    assert_eq!(dh.skipped, H, "the hue is left out under the chroma floor");
    // The hue's difference crosses its seam at 180 degrees.
    assert_eq!(hue_difference(179.0, -179.0), 2.0);
    assert_eq!(hue_difference(-179.0, 179.0), -2.0);
    assert_eq!(hue_difference(10.0, 190.0), 180.0);
    assert_eq!(hue_difference(0.0, 90.0), 90.0);
    let [_, _, dh] = tone_step(&halves([0, 200, 190], [0, 190, 200], 0), &b, 1.0);
    assert_eq!(dh.skipped, 0);
    for v in &dh.values {
        assert!(v.abs() < 30.0 && *v != 0.0, "a small turn of hue: {v}");
    }

    // Transition and texture match, recomputed from the Sobel plane by their definitions.
    let textured = halves([120; 3], [140; 3], 3);
    let on = image(&textured, false);
    let (b, mask) = (band(&on, 6.0), band_mask(&on, 6.0));
    let sobel = sobel_magnitude(&luminance(&textured), W, H);
    let at = |x: u32, y: u32| f64::from(sobel[(y * W + x) as usize]);
    let mean = |v: Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
    let tr = transition(&textured, &b, &sobel);
    let tm = texture_match(&textured, &mask, &b, &sobel);
    assert_eq!((tr.skipped, tm.skipped), (0, 0));
    let d = |x: u32| f64::from(x) - f64::from(C);
    for (i, &y) in tr.rows.iter().enumerate() {
        let inside = mean(
            (0..W)
                .filter(|x| (-6.0..6.0).contains(&d(*x)))
                .map(|x| at(x, y))
                .collect(),
        );
        let outside = mean(
            (0..W)
                .filter(|x| d(*x) < -12.0 || d(*x) >= 12.0)
                .map(|x| at(x, y))
                .collect(),
        );
        assert!((tr.values[i] - inside / outside).abs() < 1e-9, "row {y}");
        assert!(tr.values[i] > 1.0, "the step adds gradient in the band");
        let core = mean(
            (0..W)
                .filter(|x| mask.core()[(y * W + x) as usize] != 0)
                .map(|x| at(x, y))
                .collect(),
        );
        let beyond = mean(
            (0..W)
                .filter(|x| (-12.0..-6.0).contains(&d(*x)) || (6.0..12.0).contains(&d(*x)))
                .map(|x| at(x, y))
                .collect(),
        );
        assert!((tm.values[i] - core / beyond).abs() < 1e-9, "row {y}");
    }
    let flat = halves([120; 3], [120; 3], 0);
    let flat_sobel = sobel_magnitude(&luminance(&flat), W, H);
    assert_eq!(transition(&flat, &b, &flat_sobel).skipped, H);
    assert_eq!(texture_match(&flat, &mask, &b, &flat_sobel).skipped, H);
    // A band wider than the image: no column outside, every row skipped.
    assert_eq!(transition(&textured, &band(&on, 40.0), &sobel).skipped, H);

    // Drift. A correct paste leaves where the mask weighs nothing as it was.
    let before = halves([120; 3], [140; 3], 2);
    let base = image(&before, false);
    let mask = band_mask(&base, 6.0);
    let fill = Painted::from_rgb(W, H, [10, 20, 30].repeat((W * H) as usize)).unwrap();
    let pasted = composite(&Picture::new(before.clone()).unwrap(), &fill, &mask).unwrap();
    let d = drift(&before, pasted.pixels(), &mask);
    assert_eq!(d.outside, [0.0, 0.0, 0.0], "a paste leaves the rest alone");
    assert!(d.core.iter().all(|v| *v > 50.0), "{:?}", d.core);
    assert!(
        d.blend.iter().all(|v| *v > 10.0 && *v < d.core[0]),
        "{:?}",
        d.blend
    );

    // The summaries as rascal put them in its envelope, and its statuses.
    let measured = numbers(&after, None, &mask, "continuity");
    assert!((number(&measured, "continuity.mean") - 119.0).abs() < 1e-9);
    assert!((number(&measured, "continuity.p95") - 119.0).abs() < 1e-9);
    assert_eq!(number(&measured, "continuity.rows_skipped"), 0.0);
    let measured = numbers(pasted.pixels(), Some(&before), &mask, "drift");
    assert_eq!(number(&measured, "drift.outside.r"), 0.0);
    assert!(number(&measured, "drift.core.g") > 50.0);
    let run = |set: &str, before: Option<&CanonicalPixels>, mask: &Mask| {
        Measuring {
            after: pasted.pixels(),
            before,
            mask,
            column: None,
            chroma_floor: 1.0,
            percentile: 95.0,
        }
        .run(&measures_of(set).unwrap())
    };
    let none = run("drift", None, &mask);
    assert!(!none.ok());
    assert_eq!(none.statuses, ["drift: not_applicable (no before image)"]);
    let both = run("continuity,drift", None, &mask);
    assert!(both.ok());
    assert!(
        both.line().contains("drift: not_applicable"),
        "{}",
        both.line()
    );
    let plain = Mask::from_core(
        base.binding(),
        UseCase::Remove,
        band_core(W, C, &[6.0; H as usize]),
        Source::default(),
        made_by("fixture"),
    )
    .unwrap();
    let unprofiled = run("tone_step", None, &plain);
    assert_eq!(
        unprofiled.statuses,
        ["tone_step: not_applicable (the mask has no profile)"]
    );
    let nothing = Measuring {
        after: &halves([120; 3], [120; 3], 0),
        before: None,
        mask: &mask,
        column: None,
        chroma_floor: 1.0,
        percentile: 95.0,
    }
    .run(&["continuity"]);
    assert_eq!(nothing.statuses, ["continuity: measured_nothing"]);
    assert!(nothing.series.is_empty());
    assert!(measures_of("continuity,sharpness").is_err());
    assert_eq!(
        measures_of(" drift , drift,continuity").unwrap(),
        ["drift", "continuity"]
    );
}

/// The executor over `measure`.
fn engine(world: &World) -> Engine {
    let mut registry = Registry::new();
    whizbang_rascal::measure::register(&mut registry).unwrap();
    Engine::over(
        registry,
        Arc::new(NoBackends),
        world.store.clone(),
        Digest::of_bytes(b""),
    )
}

fn series(world: &World, id: ArtifactId) -> Series {
    let read = artifact::get(
        &*world.store,
        ArtifactType::Series,
        id,
        &DecodeLimits::default(),
    );
    match read.unwrap() {
        Artifact::Series(series) => series,
        other => panic!("{other:?}"),
    }
}

#[test]
fn measure_as_an_action_hands_back_a_series_for_each_measure_that_measured() {
    let world = World::new("measure");
    let engine = engine(&world);
    let before = halves([120; 3], [140; 3], 2);
    let base = image(&before, false);
    let mask = band_mask(&base, 6.0);
    let fill = Painted::from_rgb(W, H, [10, 20, 30].repeat((W * H) as usize)).unwrap();
    let pasted = composite(&Picture::new(before).unwrap(), &fill, &mask).unwrap();
    let after = image(pasted.pixels(), false);
    let (base_id, after_id) = (put(&world, base.clone()), put(&world, after.clone()));
    let mask_id = put(&world, mask.clone());
    let call = |set: &str| {
        Call::new("measure")
            .argument("set", Value::text(set))
            .input("after", after_id)
            .input("before", base_id)
            .input("mask", mask_id)
    };
    // All five: six series, on the mask's rows, and drift's numbers in the journal alone.
    let made = ran(
        &engine,
        &call("continuity,tone_step,transition,texture_match,drift"),
    );
    let ports: Vec<&str> = made.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        ports,
        [
            "continuity",
            "tone_step_dl",
            "tone_step_dchroma",
            "tone_step_dhue",
            "transition",
            "texture_match"
        ]
    );
    let first = series(&world, made[0].1);
    assert_eq!((first.name(), first.unit()), ("continuity", "ratio"));
    assert_eq!(first.binding(), &after.binding());
    assert_eq!(first.rows(), (0..H).collect::<Vec<_>>());
    // Drift alone measures, and hands back no series.
    assert!(ran(&engine, &call("drift")).is_empty());
    // A flat image: continuity measured nothing, and nothing is handed back.
    let flat_id = put(&world, image(&halves([120; 3], [120; 3], 0), false));
    let flat = Call::new("measure")
        .argument("set", Value::text("continuity"))
        .input("after", flat_id)
        .input("mask", mask_id);
    assert!(ran(&engine, &flat).is_empty());
    // A line on the image sets continuity's column; one on another image is refused.
    let line_on = |on: &Image, column: f64| {
        let location = Location::new(
            on.binding(),
            Shape::Line {
                line: Line::Column(column),
                width: 1.0,
            },
            made_by("line"),
        )
        .unwrap();
        put(&world, location)
    };
    let with_line = |line| {
        Call::new("measure")
            .argument("set", Value::text("continuity"))
            .input("after", after_id)
            .input("mask", mask_id)
            .input("line", line)
    };
    let at_line = series(&world, ran(&engine, &with_line(line_on(&base, 60.0)))[0].1);
    assert_eq!(at_line.values(), first.values());
    let elsewhere = image(&halves([1; 3], [2; 3], 0), false);
    put(&world, elsewhere.clone());
    let why = invalid(&engine, &with_line(line_on(&elsewhere, 60.0)));
    assert!(why.contains("the line is on none"), "{why}");
    // Unknown measures, and images of another size, are refused.
    assert!(invalid(&engine, &call("continuity,sharpness")).contains("unknown"));
    let small = put(&world, image(&super::canvases::pano(64, 32), false));
    let wrong = Call::new("measure")
        .input("after", small)
        .input("mask", mask_id);
    assert!(invalid(&engine, &wrong).contains("of one size"));
}
