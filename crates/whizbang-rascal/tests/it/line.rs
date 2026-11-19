//! `line` as an action (plan m3, task O1, item 1). The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/rasterize.rs`, `the_line_and_stroke_ops_make_validated_locations`,
//! at the commit `docs/sources.toml` pins), on rascal's picture of 60 by 40, each
//! fraction of rascal's frame taken to pixels as rascal's `rasterize` took it: times the
//! width or the height.

use whizbang_core::artifact::{Artifact, Line, Shape};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::Point;

use super::shaping::{Shop, picture};

const W: f64 = 60.0;
const H: f64 = 40.0;

#[test]
fn the_ported_vectors_give_rascals_locations() {
    let shop = Shop::new("line", None);
    let image = picture(60, 40);
    let id = shop.put(Artifact::Image(image.clone()));
    let line = |arguments: &[(&str, Value)]| {
        let mut call = Call::new("line").input("image", id);
        for (name, value) in arguments {
            call = call.argument(name, value.clone());
        }
        call
    };
    // The column form: rascal's column 0.5 and width 3.
    let made = shop.one(&line(&[
        ("column", Value::Number(0.5 * W)),
        ("width_px", Value::Number(3.0)),
    ]));
    let location = shop.location(made);
    assert_eq!(
        location.shape(),
        &Shape::Line {
            line: Line::Column(30.0),
            width: 3.0
        }
    );
    assert_eq!(location.binding(), &image.binding());
    assert_eq!(location.origin().steps()[0].action, "line");
    // The path form: rascal's [[0.1, 0.1], [0.5, 0.9]], width 2.
    let path = format!("{} {}; {} {}", 0.1 * W, 0.1 * H, 0.5 * W, 0.9 * H);
    let made = shop.one(&line(&[
        ("path", Value::text(&path)),
        ("width_px", Value::Number(2.0)),
    ]));
    assert_eq!(
        shop.location(made).shape(),
        &Shape::Line {
            line: Line::Path(vec![
                Point::new(0.1 * W, 0.1 * H),
                Point::new(0.5 * W, 0.9 * H)
            ]),
            width: 2.0
        }
    );
    // Neither form, and both, are refused, in rascal's words.
    let neither = shop.refused(&line(&[("width_px", Value::Number(2.0))]));
    assert!(
        neither.contains("exactly one of column and path"),
        "{neither}"
    );
    let both = shop.refused(&line(&[
        ("column", Value::Number(0.5 * W)),
        ("path", Value::text(&path)),
        ("width_px", Value::Number(2.0)),
    ]));
    assert!(both.contains("exactly one of column and path"), "{both}");
    // Rascal's column 1.5 is outside the image; a path of one point is too short.
    let outside = shop.refused(&line(&[
        ("column", Value::Number(1.5 * W)),
        ("width_px", Value::Number(2.0)),
    ]));
    assert!(outside.contains("is not inside"), "{outside}");
    let short = shop.refused(&line(&[
        ("path", Value::text("6 4")),
        ("width_px", Value::Number(2.0)),
    ]));
    assert!(short.contains("at least 2 points"), "{short}");
    // A width that is not over 0 is refused, and one is required.
    let thin = shop.refused(&line(&[
        ("column", Value::Number(30.0)),
        ("width_px", Value::Number(0.0)),
    ]));
    assert!(thin.contains("not positive"), "{thin}");
    let without = shop.refused(&line(&[("column", Value::Number(30.0))]));
    assert!(without.contains("width_px"), "{without}");
}
