//! The records of `fan`, `profile`, `ramp`, `apply` and `measure`,
//! `docs/reference/tools/<tool>.md` (plan m3, task D2): one chain on a cut's canvas, as the
//! task `seam` runs it before it fills. Each example calls the operation's function
//! directly, the function its action runs, so the test needs no store and no executor.
//!
//! The canvas is 192 by 64, its seam at column 96: two smooth halves of different tones
//! (`smooth`), and the same with a fine texture on its lower half (`detail`).

use whizbang_core::artifact::{BandProfile, Mask};
use whizbang_core::identity::CanonicalPixels;
use whizbang_rascal::apply::performed;
use whizbang_rascal::constants::{self, FAN, MEASURES_V1};
use whizbang_rascal::examples::{band_of, banded, canvas, canvases, ramped};
use whizbang_rascal::fan::{Settings, fan_profile, windows};
use whizbang_rascal::measure::{MEASURES, Measured, Measuring};
use whizbang_rascal::profile::band_core;

use super::super::record::{picture, time};
use super::{Page, SIZES, count, hold, image_picture, timed_head, timed_row};

const W: u32 = 192;
const H: u32 = 64;
const C: f64 = 96.0;

/// What the chain made of one canvas.
struct Chain {
    name: &'static str,
    canvas: CanonicalPixels,
    band: BandProfile,
    /// The band mask, as `profile` made it.
    banded: Mask,
    /// The band mask with its tone ramp, as `ramp` made it.
    ramped: Mask,
    /// The working image, as `apply` made it, and how many pixels and levels it moved.
    working: CanonicalPixels,
    changed: u64,
    largest: u8,
}

fn chains() -> Vec<Chain> {
    canvases()
        .into_iter()
        .map(|(name, canvas)| {
            let band = band_of(&canvas, C);
            let banded = banded(&band);
            let ramped = ramped(&canvas, &banded);
            let applied = performed(&canvas, &ramped).unwrap();
            Chain {
                name,
                canvas,
                band,
                banded,
                ramped,
                working: applied.pixels,
                changed: applied.changed,
                largest: applied.largest,
            }
        })
        .collect()
}

/// Every measure of `measures-v1` on `after`, with `before` when given.
fn measures(after: &CanonicalPixels, before: Option<&CanonicalPixels>, mask: &Mask) -> Measured {
    let number = |key| constants::number(MEASURES_V1.1, key).unwrap();
    Measuring {
        after,
        before,
        mask,
        column: None,
        chroma_floor: number("chroma_floor"),
        percentile: number("percentile"),
    }
    .run(&MEASURES)
}

/// A number of what was measured, to three places, or a dash when it is not there.
fn said(measured: &Measured, name: &str) -> String {
    let found = measured.numbers.iter().find(|(n, _, _)| n == name);
    found.map_or("-".to_owned(), |(_, value, _)| format!("{value:.3}"))
}

/// The plane of the columns of `band`'s two step windows.
fn step_windows(band: &BandProfile) -> Vec<u8> {
    let [(_, a, b), (_, c, d), _, _] = windows(band.column(), band.thin(), band.windows(), W);
    (0..W * H)
        .map(|at| {
            let x = i64::from(at % W);
            if (a..b).contains(&x) || (c..d).contains(&x) {
                255
            } else {
                0
            }
        })
        .collect()
}

const ROWS: [usize; 4] = [0, 24, 40, 63];

pub fn page(tool: &'static str) -> Page {
    page_of(tool, &chains())
}

fn page_of(tool: &'static str, chains: &[Chain]) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from("\n");
    let rows =
        |values: &dyn Fn(usize) -> String| ROWS.map(|y| format!("{y}: {}", values(y))).join("; ");
    measured.push_str(match tool {
        "fan" => "| Example | Column | Thin | Half-width on rows 0, 24, 40 and 63 | Rows widened |\n|---|---|---|---|---|\n",
        "profile" => "| Example | Core | Blend | Sigma on rows 0, 24, 40 and 63 |\n|---|---|---|---|\n",
        "ramp" => "| Example | Share | Step on rows 0, 24, 40 and 63, red, green and blue |\n|---|---|---|\n",
        "apply" => "| Example | Pixels changed | Largest change in levels | Row 8 at columns 94 to 97, red, before and after |\n|---|---|---|---|\n",
        _ => "| Example | Status | Continuity | Tone step dL | Transition | Texture match | Drift, red, core, blend and outside |\n|---|---|---|---|---|---|---|\n",
    });
    for chain in chains {
        let name = chain.name;
        let half = chain.band.half();
        match tool {
            "fan" => {
                let core = band_core(W, chain.band.column(), half);
                files.push((
                    format!("{name}.png"),
                    image_picture(&chain.canvas, Some(&core)),
                ));
                let widened = half
                    .iter()
                    .filter(|h| **h > chain.band.thin() + 0.5)
                    .count();
                measured.push_str(&format!(
                    "| {name} | {} | {:.3} | {} | {widened} |\n",
                    chain.band.column(),
                    chain.band.thin(),
                    rows(&|y| format!("{:.3}", half[y]))
                ));
            }
            "profile" => {
                files.push((format!("{name}.png"), picture(&chain.banded)));
                let sigma = chain.banded.profile().unwrap().sigma.values();
                measured.push_str(&format!(
                    "| {name} | {} | {} | {} |\n",
                    count(chain.banded.core()),
                    count(chain.banded.blend()),
                    rows(&|y| format!("{:.3}", sigma[y]))
                ));
            }
            "ramp" => {
                let plane = step_windows(&chain.band);
                files.push((
                    format!("{name}.png"),
                    image_picture(&chain.canvas, Some(&plane)),
                ));
                let [whizbang_core::artifact::Prescription::ToneRamp { step, share, .. }] =
                    chain.ramped.prescriptions()
                else {
                    panic!("one tone ramp")
                };
                let row = |y: usize| {
                    let v = step.row(y as u32).unwrap();
                    format!("{:.2}, {:.2}, {:.2}", v[0], v[1], v[2])
                };
                measured.push_str(&format!("| {name} | {share} | {} |\n", rows(&row)));
            }
            "apply" => {
                files.push((format!("{name}.png"), image_picture(&chain.working, None)));
                let red = |p: &CanonicalPixels| {
                    (94..98)
                        .map(|x| p.rgba()[((8 * W + x) * 4) as usize].to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                measured.push_str(&format!(
                    "| {name} | {} | {} | {} to {} |\n",
                    chain.changed,
                    chain.largest,
                    red(&chain.canvas),
                    red(&chain.working)
                ));
            }
            _ => {
                let core = band_core(W, chain.band.column(), half);
                for (when, after, before) in [
                    ("before", &chain.canvas, None),
                    ("after", &chain.working, Some(&chain.canvas)),
                ] {
                    let label = format!("{name}, {when} `apply`");
                    let file = format!("{name}-{when}.png");
                    files.push((file, image_picture(after, Some(&core))));
                    let m = measures(after, before, &chain.ramped);
                    let ok = m.statuses.iter().filter(|s| s.ends_with(": ok")).count();
                    measured.push_str(&format!(
                        "| {label} | {ok} of 5 ok | {} | {} | {} | {} | {}, {}, {} |\n",
                        said(&m, "continuity.mean"),
                        said(&m, "tone_step.dL.mean"),
                        said(&m, "transition.mean"),
                        said(&m, "texture_match.mean"),
                        said(&m, "drift.core.r"),
                        said(&m, "drift.blend.r"),
                        said(&m, "drift.outside.r"),
                    ));
                }
            }
        }
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

pub fn timed(tool: &str) -> String {
    let column = match tool {
        "fan" => "`fan` under `fan-v3`",
        "profile" => "`profile` of that band",
        "ramp" => "`ramp`'s tone step",
        "apply" => "`apply` of that ramp",
        _ => "`measure`, all five",
    };
    let mut out = timed_head(&[column]);
    for (width, height) in SIZES {
        let (w, h) = (width as u32, height as u32);
        let pixels = canvas(w, h, h / 2);
        let column = f64::from(w / 2);
        let settings = Settings::of(FAN[0].0).unwrap();
        let took = if tool == "fan" {
            time(
                5,
                || (),
                |()| fan_profile(&pixels, column, &settings).unwrap(),
            )
        } else {
            let band = band_of(&pixels, column);
            let mask = banded(&band);
            match tool {
                "profile" => time(5, || (), |()| banded(&band)),
                "ramp" => time(5, || (), |()| ramped(&pixels, &mask)),
                "apply" => {
                    let mask = ramped(&pixels, &mask);
                    time(5, || (), |()| performed(&pixels, &mask).unwrap())
                }
                _ => {
                    let mask = ramped(&pixels, &mask);
                    let after = performed(&pixels, &mask).unwrap().pixels;
                    time(5, || (), |()| measures(&after, Some(&pixels), &mask))
                }
            }
        };
        out.push_str(&timed_row(width, height, &[took]));
    }
    out.push('\n');
    out
}

#[test]
fn the_records_of_fan_profile_ramp_apply_and_measure_are_what_they_give() {
    let chains = chains();
    for tool in ["fan", "profile", "ramp", "apply", "measure"] {
        hold(&page_of(tool, &chains));
    }
    let (smooth, detail) = (&chains[0], &chains[1]);
    // On the smooth canvas every row opens to the widest, 0.15 of the width; under the
    // texture the band falls to thin, the smoothing along the rows easing it.
    let wide = 0.15 * f64::from(W);
    assert!(smooth.band.half().iter().all(|h| (h - wide).abs() < 1e-9));
    assert!((detail.band.half()[0] - wide).abs() < 1e-9);
    assert!((detail.band.half()[63] - detail.band.thin()).abs() < 1e-9);
    // `ramp` adds a prescription and paints nothing; `apply` moves the left side toward
    // the right and the right toward the left, by up to half the step at the line.
    assert_eq!(smooth.ramped.core(), smooth.banded.core());
    assert_eq!(smooth.ramped.prescriptions().len(), 1);
    let red = |p: &CanonicalPixels, x: u32| p.rgba()[((8 * W + x) * 4) as usize];
    assert!(red(&smooth.working, 95) < red(&smooth.canvas, 95));
    assert!(red(&smooth.working, 96) > red(&smooth.canvas, 96));
    assert_eq!(red(&smooth.working, 40), red(&smooth.canvas, 40));
    // `measure`: drift says the apply moved nothing outside the mask.
    let m = measures(&smooth.working, Some(&smooth.canvas), &smooth.ramped);
    assert_eq!(said(&m, "drift.outside.r"), "0.000");
}
