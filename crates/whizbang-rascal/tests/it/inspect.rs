//! `inspect` (plan m3, task O4, item 2). The vectors are rascal's own
//! (`crates/rascal-mask/tests/it/inspect_json.rs` and `inspect.rs`, and `rendering.rs`'s
//! `overlay`, at the commit `docs/sources.toml` pins): rascal's `rect` of 0.2, 0.2, 0.4,
//! 0.5 on its picture of 320 by 200, a box of 64 by 60 pixels, reported as rascal reported
//! it; the overlay drawn as rascal drew it; and the action run through the executor.

use whizbang_core::artifact::{Artifact, ArtifactType, ImageRole, Mask, Source, UseCase};
use whizbang_core::contract::Call;
use whizbang_core::identity::DecodeLimits;
use whizbang_rascal::inspect::{TINT, core_widths, overlay, report};

use super::grow::{made_by, on, plane};
use super::paste::{H, W, image_of, mask_on, pixels};
use super::shaping::Shop;

#[test]
fn the_ported_vectors_give_rascals_report() {
    // Rascal's box: columns 64 to 127, rows 40 to 99, of 320 by 200.
    let (w, h) = (320, 200);
    let core = plane(w, h, |x, y| {
        (64..128).contains(&x) && (40..100).contains(&y)
    });
    let mask = Mask::from_core(
        on(w as u32, h as u32),
        UseCase::Custom,
        core,
        Source::default(),
        made_by("rasterize"),
    )
    .unwrap();
    let made = report(&mask);
    let zones = &made.zones;
    assert_eq!((zones.width, zones.height), (320, 200));
    assert_eq!(zones.core, 64 * 60);
    assert_eq!(zones.blend, 0);
    assert_eq!(zones.protect, 0);
    assert_eq!(zones.painted, 64 * 60);
    assert_eq!(zones.support, Some([64, 40, 128, 100]));
    assert_eq!(made.identity, mask.id());
    assert_eq!(made.profile, None);
    assert_eq!(made.prescriptions, 0);
    assert!(
        made.line()
            .contains("core 3840, blend 0, protect 0, painted 3840"),
        "{}",
        made.line()
    );
    assert!(made.line().contains("support [64, 40, 128, 100)"));
    // The chart's data: 64 on each of the box's rows, and none elsewhere.
    let widths = core_widths(&mask);
    assert_eq!(widths.len(), 200);
    for (row, width) in widths.iter().enumerate() {
        let expected = if (40..100).contains(&row) { 64.0 } else { 0.0 };
        assert_eq!(*width, expected, "row {row}");
    }
}

#[test]
fn the_overlay_is_rascals_tint_with_the_core_edge_in_white() {
    let base = pixels();
    let image = image_of(&base, "import");
    let mask = mask_on(&image);
    let drawn = overlay(&mask, &base, TINT).unwrap();
    let rgba = base.rgba();
    let width = W as usize;
    let core = |x: i64, y: i64| {
        (0..W as i64).contains(&x)
            && (0..H as i64).contains(&y)
            && mask.core()[y as usize * width + x as usize] != 0
    };
    let mut edges = 0;
    for at in 0..(W * H) as usize {
        let (x, y) = ((at % width) as i64, (at / width) as i64);
        let (got, p) = (&drawn[at * 4..at * 4 + 4], &rgba[at * 4..at * 4 + 4]);
        assert_eq!(got[3], 255, "opaque");
        let edge =
            core(x, y) && !(core(x - 1, y) && core(x + 1, y) && core(x, y - 1) && core(x, y + 1));
        let w = whizbang_core::artifact::effective_weight(
            mask.core()[at],
            mask.blend()[at],
            mask.protect()[at],
        );
        if edge {
            assert_eq!(
                got,
                [255, 255, 255, 255],
                "({x}, {y}) is on the core's edge"
            );
            edges += 1;
        } else if w > 0.0 {
            for c in 0..3 {
                let tinted = ((1.0 - 0.5 * w) * f32::from(p[c]) + 0.5 * w * f32::from(TINT[c]))
                    .round() as u8;
                assert_eq!(got[c], tinted, "({x}, {y}) channel {c}");
            }
        } else {
            assert_eq!(&got[..3], &p[..3], "({x}, {y}) is the image");
        }
    }
    // The core of 6 by 5 has 18 pixels on its edge, and the two protected pixels inside
    // it open no hole in the core: a protected pixel is cleared from the core, so its
    // four neighbours are on the edge too.
    assert!(edges >= 18, "{edges}");
    // The mask belongs to its image, and to no other.
    let other = super::render::picture(W, H).0;
    let refused = overlay(&mask, &other, TINT).unwrap_err();
    assert!(refused.contains("belongs to the image"), "{refused}");
}

#[test]
fn inspect_as_an_action_hands_back_the_overlay_and_the_widths() {
    let shop = Shop::new("inspect", None);
    let base = pixels();
    let image = image_of(&base, "import");
    let mask = mask_on(&image);
    let image_id = shop.put(Artifact::Image(image.clone()));
    let mask_id = shop.put(Artifact::Mask(mask.clone()));
    let outputs = shop.completed(
        &Call::new("inspect")
            .input("mask", mask_id)
            .input("image", image_id),
    );
    let ports: Vec<&str> = outputs.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(ports, ["overlay", "widths"]);
    let drawn = shop.image(outputs[0].1);
    assert_eq!(drawn.role(), ImageRole::Rendering);
    assert_eq!(
        drawn.pixels(&DecodeLimits::default()).unwrap().rgba(),
        overlay(&mask, &base, TINT).unwrap().as_slice()
    );
    let Artifact::Series(widths) = shop.get(ArtifactType::Series, outputs[1].1) else {
        panic!()
    };
    assert_eq!(widths.values(), core_widths(&mask).as_slice());
    assert_eq!(widths.binding(), mask.binding());
    assert_eq!(widths.unit(), "px");
    // Its report is its measurements, a result kind it declares (plan m3, task Q1).
    let report = whizbang_rascal::inspect::report(&mask);
    assert_eq!(report.measured()["line"], report.line());
    let inspect = whizbang_rascal::inspect::Inspect::new();
    let declared = whizbang_core::contract::Action::descriptor(&inspect);
    assert!(declared.answers_with(whizbang_core::contract::ResultKind::Measurements));
    // A rendering is not a picture to draw on.
    let rendering = shop.put(Artifact::Image(drawn));
    let why = shop.refused(
        &Call::new("inspect")
            .input("mask", mask_id)
            .input("image", rendering),
    );
    assert!(!why.is_empty());
}
