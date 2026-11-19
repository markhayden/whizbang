//! `edits::…`: the synthetic edit set of plan m3, task B1, item 1, drawn by its program
//! from fixed seeds, and its scoring: preservation outside the mask, and task success (the
//! mask's IoU with the drawn defect's, and the fill's error beside a plain blur fill's).
//! Three pictures of 64 by 64 made in the test; the filler is the scripted one of
//! `lab.rs`, and no model is called.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use whizbang::edits::{self, Source};
use whizbang::harness::Running;
use whizbang::laboratory;
use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Over};
use whizbang_core::artifact::{Artifact, ArtifactType, Shape};
use whizbang_core::identity::{DecodeLimits, Projection, identify};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::lab::corpus::Corpus;
use whizbang_eval::lab::digest_of;
use whizbang_testkit::scratch::Scratch;

use crate::lab::{Counting, backends, checked, counting_config};

/// A picture of 64 by 64 as a set's file would be: a gradient, its colours shifted by
/// `tint`, with its pin as a job holds it.
fn source(id: &str, tint: u8) -> Source {
    use image::ImageEncoder;
    let mut rgb = Vec::new();
    for y in 0..64_u32 {
        for x in 0..64_u32 {
            rgb.extend_from_slice(&[(x * 3) as u8 + tint, (y * 3) as u8, 200 - tint, 255]);
        }
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgb, 64, 64, image::ExtendedColorType::Rgba8)
        .unwrap();
    Source {
        id: id.to_owned(),
        path: format!("items/{id}/candidate.png"),
        sha256: whizbang_core::identity::Digest::of_bytes(&png).hex(),
        bytes: png,
    }
}

fn three() -> Vec<Source> {
    vec![
        source("alpha", 0),
        source("bravo", 30),
        source("charlie", 50),
    ]
}

/// The canonical pixels of a picture's file: red, green, blue and alpha, row by row.
fn pixels(bytes: &[u8]) -> Vec<u8> {
    let identified = identify(bytes, &DecodeLimits::default()).unwrap();
    identified.pixels.rgba().to_vec()
}

/// A truth mask's file as a plane: true where the defect was drawn.
fn plane(bytes: &[u8]) -> Vec<bool> {
    pixels(bytes).chunks(4).map(|p| p[0] == 255).collect()
}

/// The distance from a point to a path of segments.
fn to_path(x: f64, y: f64, path: &[[f64; 2]]) -> f64 {
    let mut best = f64::MAX;
    for pair in path.windows(2) {
        let ([ax, ay], [bx, by]) = (pair[0], pair[1]);
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (px, py) = (ax + t * dx - x, ay + t * dy - y);
        best = best.min((px * px + py * py).sqrt());
    }
    best
}

#[test]
fn the_synthetic_set_is_drawn_by_its_program_from_fixed_seeds() {
    let sources = three();
    let drawn = edits::draw("test/tiny-v1", &sources, 2, edits::SEED).unwrap();
    // Drawn again from the same seed: the same files, byte for byte, and the same manifest.
    let again = edits::draw("test/tiny-v1", &sources, 2, edits::SEED).unwrap();
    assert_eq!(drawn.files, again.files);
    assert_eq!(drawn.set.text(), again.set.text());
    // Another seed draws other defects.
    let other = edits::draw("test/tiny-v1", &sources, 2, edits::SEED + 1).unwrap();
    assert_ne!(other.set.text(), drawn.set.text());
    let set = &drawn.set;
    assert_eq!(edits::EditSet::parse(&set.text()).unwrap(), *set);
    assert_eq!((set.seed, set.phrase.as_str()), (edits::SEED, "a red ball"));
    // Two of the three pictures, each under the four recipes.
    assert_eq!(set.source.len(), 2);
    assert_eq!(set.item.len(), 8);
    let recipes: BTreeSet<&str> = set.item.iter().map(|i| i.recipe.as_str()).collect();
    assert_eq!(
        recipes,
        BTreeSet::from(["remove", "remove_prompt", "repair", "repair_area"])
    );
    for item in &set.item {
        let file = |name: &str| drawn.files[name].as_slice();
        assert_eq!(digest_of(file(&item.picture)), item.picture_sha256);
        assert_eq!(digest_of(file(&item.truth)), item.truth_sha256);
        let source = set.source.iter().find(|s| s.id == item.source).unwrap();
        let pinned = sources.iter().find(|s| s.id == item.source).unwrap();
        assert_eq!(source.sha256, format!("sha256:{}", pinned.sha256));
        assert_eq!(pixels(file(&source.clean)), pixels(&pinned.bytes));
        let clean = pixels(file(&source.clean));
        let picture = pixels(file(&item.picture));
        let truth = plane(file(&item.truth));
        // The defect is its mask, exactly: outside it every pixel is the clean picture's.
        let drawn_pixels = truth.iter().filter(|t| **t).count() as u64;
        assert_eq!(drawn_pixels, item.truth_pixels);
        assert!(drawn_pixels >= 8, "{}: {drawn_pixels}", item.id);
        let mut changed = 0;
        for (at, inside) in truth.iter().enumerate() {
            let (a, b) = (&clean[at * 4..at * 4 + 4], &picture[at * 4..at * 4 + 4]);
            if *inside {
                changed += u64::from(a != b);
            } else {
                assert_eq!(a, b, "{}: pixel {at} outside the defect", item.id);
            }
        }
        assert!(changed * 2 > drawn_pixels, "{}", item.id);
        let inside_truth = |x: usize, y: usize| truth[y * 64 + x];
        let location = item.location.as_ref().map(|name| {
            assert_eq!(
                digest_of(file(name)),
                *item.location_sha256.as_ref().unwrap()
            );
            let read = Artifact::from_bytes_as(
                ArtifactType::Location,
                file(name),
                &DecodeLimits::default(),
            );
            let Ok(Artifact::Location(location)) = read else {
                panic!("a location's canonical file")
            };
            // On the picture as `store import` brings it in.
            let image =
                whizbang::import::image_of_bytes(file(&item.picture), Projection::Flat).unwrap();
            assert_eq!(*location.binding(), image.binding());
            location.shape().clone()
        });
        match item.recipe.as_str() {
            "remove" | "repair_area" => {
                let [left, top, right, bottom] = item.bounds.unwrap();
                for y in 0..64 {
                    for x in 0..64 {
                        let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                        let within = cx > left && cx < right && cy > top && cy < bottom;
                        assert!(within || !inside_truth(x, y), "{}", item.id);
                    }
                }
                assert_eq!(
                    location,
                    Some(Shape::Box {
                        left,
                        top,
                        right,
                        bottom
                    })
                );
                assert_eq!(item.phrase, None);
            }
            "remove_prompt" => {
                assert_eq!(item.phrase.as_deref(), Some("a red ball"));
                assert_eq!((location, item.bounds), (None, None));
                // The same picture as `remove`'s on the same source: the same shapes.
                let remove = (set.item.iter())
                    .find(|i| i.recipe == "remove" && i.source == item.source)
                    .unwrap();
                assert_eq!(remove.picture_sha256, item.picture_sha256);
            }
            "repair" => {
                let stroke = item.stroke.as_ref().unwrap();
                for y in 0..64 {
                    for x in 0..64 {
                        let away = to_path(x as f64 + 0.5, y as f64 + 0.5, &stroke.path);
                        assert!(away <= stroke.radius || !inside_truth(x, y), "{}", item.id);
                    }
                }
                let Some(Shape::Stroke { path, radius }) = location else {
                    panic!("a stroke")
                };
                assert_eq!(radius, stroke.radius);
                let path: Vec<[f64; 2]> = path.iter().map(|p| [p.x, p.y]).collect();
                assert_eq!(path, stroke.path);
            }
            other => panic!("{other}"),
        }
    }
    // The corpus of the lab pins every file the set drew, and the clean pictures.
    let lab = edits::lab_files(set);
    let corpus = Corpus::parse(&lab["corpus.toml"]).unwrap();
    assert_eq!(corpus.image.len(), 2 + 2 * 3 + 2 * 3);
    for picture in &corpus.image {
        assert_eq!(digest_of(&drawn.files[&picture.file]), picture.sha256);
    }
    // A picture that is not the one its job pins is refused, and nothing is drawn.
    let mut changed = three();
    changed[1].bytes.push(0);
    let refused = edits::draw("test/tiny-v1", &changed, 2, edits::SEED).unwrap_err();
    assert!(refused.contains("not the one its job pins"), "{refused}");
}

/// A picture of 64 by 64 in canonical pixels: a gradient.
fn gradient() -> Vec<u8> {
    let mut rgba = Vec::new();
    for y in 0..64_u32 {
        for x in 0..64_u32 {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 4) as u8, 90, 255]);
        }
    }
    rgba
}

/// The weights of a mask whose core is the block from `(x0, y0)` to `(x1, y1)`, with a
/// ring of weight `ring` around it.
fn block(x0: usize, y0: usize, x1: usize, y1: usize, ring: f32) -> Vec<f32> {
    let mut weights = vec![0.0_f32; 64 * 64];
    for y in y0 - 1..=y1 {
        for x in x0 - 1..=x1 {
            let core = (x0..x1).contains(&x) && (y0..y1).contains(&y);
            weights[y * 64 + x] = if core { 1.0 } else { ring };
        }
    }
    weights
}

#[test]
fn preservation_outside_the_mask_is_counted_exactly() {
    let input = gradient();
    // A mask whose core is a block of 20 by 20, with a ring of the least weight around it.
    let weights = block(20, 20, 40, 40, 1.0 / 255.0);
    // Inside the support, the core and the ring, anything may change.
    let mut output = input.clone();
    for (at, weight) in weights.iter().enumerate() {
        if *weight > 0.0 {
            output[at * 4..at * 4 + 3].copy_from_slice(&[1, 2, 3]);
        }
    }
    assert_eq!(edits::changed_outside(&input, &output, &weights), Ok(0));
    // Outside it, seven pixels changed: one by a single unit of one channel, one in alpha
    // alone, one in every channel (counted once), the others anyhow; the pixels next to
    // the ring among them.
    let mut changed = output.clone();
    for (x, y, channel, by) in [
        (0_usize, 0_usize, 0_usize, 1_u8),
        (63, 63, 3, 1),
        (18, 30, 1, 40),
        (41, 41, 2, 7),
        (5, 60, 0, 200),
        (60, 5, 1, 9),
    ] {
        let at = (y * 64 + x) * 4 + channel;
        changed[at] = changed[at].wrapping_sub(by);
    }
    let at = (10 * 64 + 10) * 4;
    for channel in 0..4 {
        changed[at + channel] ^= 0x55;
    }
    assert_eq!(edits::changed_outside(&input, &changed, &weights), Ok(7));
    // Pictures of two sizes are refused, not counted.
    assert!(edits::changed_outside(&input, &changed[4..], &weights).is_err());
}

#[test]
fn task_success_is_the_masks_iou_and_the_fills_error_beside_a_blur_fill() {
    // The drawn defect: a block of 10 by 10. The recipe's mask: a core of 10 by 12 over
    // it, and a ring under half a weight, which is not the mask's.
    let mut truth = vec![false; 64 * 64];
    for y in 10..20 {
        for x in 10..20 {
            truth[y * 64 + x] = true;
        }
    }
    let weights = block(10, 10, 20, 22, 0.4);
    assert_eq!(edits::iou(&truth, &weights), Ok(100.0 / 120.0));
    // A flat clean picture; the defect painted over it; the fill leaves 30 in red over the
    // clean picture inside the defect: a mean error of 10 a channel.
    let flat: Vec<u8> = [120_u8, 140, 160, 255].repeat(64 * 64);
    let mut defect = flat.clone();
    let mut output = flat.clone();
    for (at, inside) in truth.iter().enumerate() {
        if *inside {
            defect[at * 4..at * 4 + 3].copy_from_slice(&[250, 0, 0]);
            output[at * 4] = 150;
        }
    }
    let scored = edits::score_item(&defect, &flat, &truth, &output, &weights, 64, 64).unwrap();
    assert_eq!(scored.changed_outside, 0);
    assert_eq!(scored.iou, 100.0 / 120.0);
    assert_eq!(scored.fill_error, 10.0);
    // A plain blur fill of the same mask: around a flat picture it is the clean picture
    // inside the defect, and outside the support it changes nothing.
    assert_eq!(scored.blur_error, 0.0);
    let blurred = edits::blur_fill(&defect, &weights, 64, 64).unwrap();
    for (at, weight) in weights.iter().enumerate() {
        if *weight == 0.0 {
            assert_eq!(blurred[at * 4..at * 4 + 4], defect[at * 4..at * 4 + 4]);
        }
    }
    // A mask that misses the defect: no overlap, and the defect left is the fill's error.
    let away = block(40, 40, 50, 50, 0.0);
    let missed = edits::score_item(&defect, &flat, &truth, &defect, &away, 64, 64).unwrap();
    assert_eq!(missed.iou, 0.0);
    assert_eq!(missed.fill_error, (130.0 + 140.0 + 160.0) / 3.0);
    assert_eq!(missed.blur_error, missed.fill_error);
    scored_from_a_round();
}

/// A round of `repair_area` on three drawn pictures under the scripted filler, which
/// inverts what the mask covers, scored from the store's runs as the orchestrator's run is.
fn scored_from_a_round() {
    let dir = Scratch::new("edits-round");
    let drawn = edits::draw("test/tiny-v1", &three(), 3, edits::SEED).unwrap();
    let set = dir.path().join("set");
    edits::write(&drawn, &set).unwrap();
    let labs = dir.path().join("labs");
    std::fs::create_dir_all(&labs).unwrap();
    for (name, text) in edits::lab_files(&drawn.set) {
        std::fs::write(labs.join(name), text).unwrap();
    }
    let root = dir.path().join("store");
    let made = wiring::opening(&root, wiring::Make::TheRoot, wiring::recovery()).unwrap();
    let store = made.writer().unwrap();
    let dollar = whizbang_core::spend::Amount::new(1_000_000).unwrap();
    whizbang_core::spend::reset(&store, dollar, 1).unwrap();
    drop((store, made));
    let config = dir.path().join("whizbang.toml");
    std::fs::write(&config, counting_config("https://fill.example.org/v1")).unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    let calls = Arc::new(AtomicU64::new(0));
    kinds
        .register(Arc::new(Counting { calls, fail_at: 0 }))
        .unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    let matrix = labs.join("repair_area-matrix.toml");
    let asked = laboratory::Asked {
        matrix: &matrix,
        corpus: &set,
        root: &root,
        over: &over,
        recovery: wiring::recovery(),
    };
    let prepared = laboratory::prepare(&asked).unwrap();
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let registry = wiring::registry_over(&over, false).unwrap();
    let executor = wiring::runner(registry, store, &over).unwrap();
    let running = Running {
        executor: &executor,
        root: &root,
        left_micros: 1_000_000,
        stop: &|| false,
    };
    let report = laboratory::round(&prepared, &running).unwrap();
    assert_eq!((report.completed, report.failed), (3, 0));
    let scoring = edits::Scoring {
        set: &set,
        labs: &labs,
        root: &root,
        commit: "0123456789abcdef0123456789abcdef01234567",
    };
    let record = edits::score(&scoring).unwrap();
    let block = (record.recipe.iter())
        .find(|r| r.recipe == "repair_area")
        .unwrap();
    assert_eq!((block.items, block.completed, block.failed), (3, 3, 0));
    assert_eq!((block.changed_outside_mask, block.items_preserved), (0, 3));
    // The box, feathered, covers the spot and more: some overlap, never all.
    let iou = block.mask_iou_mean.unwrap();
    assert!(iou > 0.0 && iou < 1.0, "{iou}");
    assert!(block.mask_iou_min.unwrap() <= iou);
    // An inverting fill is worse than a blur of what is around the defect.
    let (fill, blur) = (
        block.fill_error_mean.unwrap(),
        block.blur_error_mean.unwrap(),
    );
    assert!(fill > blur, "{fill} {blur}");
    assert_eq!(block.fill_beats_blur, 0);
    // The other recipes did not run: no number of theirs is made up.
    for other in record.recipe.iter().filter(|r| r.recipe != "repair_area") {
        assert_eq!((other.completed, other.not_run), (0, 3));
        assert_eq!(other.mask_iou_mean, None);
    }
    // The record: the commit, the shipped recipe's digest, and aggregates only.
    let page = record.page();
    assert!(page.contains("commit = \"0123456789abcdef0123456789abcdef01234567\""));
    let shipped = digest_of(whizbang_rascal::recipes::REPAIR_AREA.as_bytes());
    assert!(
        page.contains(&format!("recipe_digest = \"{shipped}\"")),
        "{page}"
    );
    for source in &drawn.set.source {
        assert!(!page.contains(&source.id), "{page}");
    }
    let machine: toml::Value = toml::from_str(&record.toml()).unwrap();
    assert_eq!(machine["recipe"].as_array().unwrap().len(), 4);
}

#[test]
fn the_committed_lab_files_are_the_programs_and_valid_against_their_protocols() {
    let lab = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../labs/m3-edits");
    let read = |name: &str| std::fs::read_to_string(lab.join(name)).unwrap();
    let set = edits::EditSet::parse(&read("edits.toml")).unwrap();
    assert_eq!(set.set, "source-fidelity/scenes-v1");
    assert_eq!(
        (set.seed, set.source.len(), set.item.len()),
        (edits::SEED, 24, 96)
    );
    // Each lab file is what the program writes from the set it drew.
    let written = edits::lab_files(&set);
    for (name, text) in &written {
        assert_eq!(&read(name), text, "{name}");
    }
    for recipe in edits::RECIPES {
        let (validated, cells) = checked(
            &read(&format!("{recipe}-matrix.toml")),
            &read(&format!("{recipe}-protocol.toml")),
            &read("corpus.toml"),
            &backends("1", 40_000),
        )
        .unwrap();
        let protocol = &validated.protocol;
        assert_eq!(protocol.name, format!("m3-edits-{recipe}"));
        assert_eq!(protocol.arms, [recipe, "clean"]);
        assert_eq!(
            (protocol.rules.preferred, protocol.rules.not_worse),
            ([2, 3], [1, 3])
        );
        assert_eq!(protocol.min_decided, 10);
        assert_eq!(cells.len(), 48);
        let fills: u64 = cells.iter().map(|c| c.forecast_micros).sum();
        assert_eq!(fills, 24 * 40_000);
    }
}
