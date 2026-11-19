//! `rect` as an action (plan m3, task O1, item 1). Rascal's rule (`crates/rascal-ops/src/rect.rs`
//! at the commit `docs/sources.toml` pins): the edges in order, and a box wholly outside
//! the frame does not apply; rascal's boxes of `rasterize.rs`'s test, in pixels of its
//! picture of 60 by 40.

use whizbang_core::artifact::{Artifact, Shape};
use whizbang_core::contract::{Call, Value};

use super::shaping::{Shop, count, picture};

#[test]
fn the_ported_vectors_give_rascals_locations() {
    let shop = Shop::new("rect", None);
    let image = picture(60, 40);
    let id = shop.put(Artifact::Image(image.clone()));
    let rect = |text: &str| {
        Call::new("rect")
            .argument("box", Value::text(text))
            .input("image", id)
    };
    // Rascal's [0.1, 0.25, 0.5, 0.75] of 60 by 40: 24 by 20 pixels once rasterized.
    let made = shop.one(&rect(&format!(
        "{} {} {} {}",
        0.1 * 60.0,
        0.25 * 40.0,
        0.5 * 60.0,
        0.75 * 40.0
    )));
    let location = shop.location(made);
    assert_eq!(
        location.shape(),
        &Shape::Box {
            left: 6.0,
            top: 10.0,
            right: 30.0,
            bottom: 30.0
        }
    );
    assert_eq!(location.binding(), &image.binding());
    let mask = shop.one(
        &Call::new("rasterize")
            .input("location", made)
            .input("image", id),
    );
    assert_eq!(count(shop.mask(mask).core()), 24 * 20);
    // Out of order, or with no area: rascal's "x0 < x1 and y0 < y1".
    for text in ["30 10 6 30", "6 30 30 10", "6 10 6 30"] {
        let why = shop.refused(&rect(text));
        assert!(
            why.contains("left < right and top < bottom"),
            "{text}: {why}"
        );
    }
    // Wholly outside the image: rascal said it did not apply.
    for text in ["60 0 70 10", "-10 0 0 10", "0 40 10 50"] {
        let why = shop.refused(&rect(text));
        assert!(why.contains("wholly outside"), "{text}: {why}");
    }
    // Partly outside: cut at the image's edge, covering the pixels rascal's box did.
    let cut = shop.location(shop.one(&rect("54 -5 66 4")));
    assert_eq!(
        cut.shape(),
        &Shape::Box {
            left: 54.0,
            top: 0.0,
            right: 60.0,
            bottom: 4.0
        }
    );
    // Not four numbers.
    let why = shop.refused(&rect("1 2 3"));
    assert!(why.contains("four finite numbers"), "{why}");
}
