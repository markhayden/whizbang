//! `stroke` as an action (plan m3, task O1, item 1). The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/rasterize.rs`, `the_line_and_stroke_ops_make_validated_locations`,
//! at the commit `docs/sources.toml` pins), on rascal's picture of 60 by 40, each
//! fraction of rascal's frame taken to pixels as rascal's `rasterize` took it.

use whizbang_core::artifact::{Artifact, Shape};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::Point;

use super::shaping::{Shop, count, picture};

const W: f64 = 60.0;
const H: f64 = 40.0;

#[test]
fn the_ported_vectors_give_rascals_locations() {
    let shop = Shop::new("stroke", None);
    let id = shop.put(Artifact::Image(picture(60, 40)));
    // Rascal's path [[0.1, 0.1], [0.3, 0.3]] with a radius of 4.
    let path = format!("{} {}; {} {}", 0.1 * W, 0.1 * H, 0.3 * W, 0.3 * H);
    let stroke = Call::new("stroke")
        .argument("path", Value::text(&path))
        .argument("radius_px", Value::Number(4.0))
        .input("image", id);
    let made = shop.one(&stroke);
    assert_eq!(
        shop.location(made).shape(),
        &Shape::Stroke {
            path: vec![Point::new(0.1 * W, 0.1 * H), Point::new(0.3 * W, 0.3 * H)],
            radius: 4.0
        }
    );
    // Through rasterize, the stroke draws: rascal's "more than 50 pixels".
    let mask = shop.one(
        &Call::new("rasterize")
            .input("location", made)
            .input("image", id),
    );
    assert!(count(shop.mask(mask).core()) > 50);
    // The radius is required, and a path needs two points.
    let without = shop.refused(
        &Call::new("stroke")
            .argument("path", Value::text(&path))
            .input("image", id),
    );
    assert!(without.contains("radius_px"), "{without}");
    let short = shop.refused(
        &Call::new("stroke")
            .argument("path", Value::text("6 4"))
            .argument("radius_px", Value::Number(4.0))
            .input("image", id),
    );
    assert!(short.contains("at least 2 points"), "{short}");
    let unread = shop.refused(
        &Call::new("stroke")
            .argument("path", Value::text("6 4 1; 18 12"))
            .argument("radius_px", Value::Number(4.0))
            .input("image", id),
    );
    assert!(unread.contains("two finite numbers"), "{unread}");
}
