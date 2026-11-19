//! `rasterize` (plan m3, task O1, item 1). The vectors are rascal's own
//! (`crates/rascal-ops/tests/it/rasterize.rs` at the commit `docs/sources.toml` pins), on
//! rascal's picture of 60 by 40, each fraction of rascal's frame taken to pixels as
//! rascal's `rasterize` took it, times the width or the height, so every double is the one
//! rascal's arithmetic met; and the references by brute force rascal's test held them to.

use whizbang_core::artifact::{Artifact, Line, Location, Shape, UseCase};
use whizbang_core::contract::{Call, Value};
use whizbang_core::identity::{Binding, Digest, PixelDigest, Point, Space};
use whizbang_rascal::rasterize::{box_pixels, dilate_path, fill_polygon, location_core, rasterize};

use super::grow::made_by;
use super::shaping::{Shop, count, picture};

const W: u32 = 60;
const H: u32 = 40;

/// A point of rascal's frame, in pixels.
fn px(x: f64, y: f64) -> Point {
    Point::new(x * f64::from(W), y * f64::from(H))
}

fn core(shape: Shape) -> Vec<u8> {
    location_core(&shape, W, H).unwrap()
}

fn at(plane: &[u8], x: u32, y: u32) -> u8 {
    plane[(y * W + x) as usize]
}

#[test]
fn the_ported_vectors_give_rascals_masks() {
    // A box and the whole image (`bbox_and_whole_image_as_before`).
    let boxed = core(Shape::Box {
        left: 0.1 * 60.0,
        top: 0.25 * 40.0,
        right: 0.5 * 60.0,
        bottom: 0.75 * 40.0,
    });
    assert_eq!(count(&boxed), 24 * 20);
    assert_eq!(count(&core(Shape::WholeImage)), (W * H) as usize);

    // A right triangle over half a box of 30 by 20 (`a_polygon_is_filled_as_drawn`).
    let triangle = core(Shape::Polygon {
        points: vec![px(0.0, 0.0), px(0.5, 0.0), px(0.0, 0.5)],
    });
    let n = count(&triangle);
    assert!((285..=315).contains(&n), "{n}");
    assert_eq!(at(&triangle, 1, 1), 255);
    assert_eq!(at(&triangle, 28, 18), 0, "outside the hypotenuse");
    assert_eq!(at(&triangle, 40, 30), 0);
    // A concave star against point-in-polygon by brute force, off the pixel grid.
    let star: Vec<[f64; 2]> = [
        [0.5, 0.05],
        [0.6, 0.4],
        [0.95, 0.4],
        [0.65, 0.6],
        [0.8, 0.95],
        [0.5, 0.7],
        [0.2, 0.95],
        [0.35, 0.6],
        [0.05, 0.4],
        [0.4, 0.4],
    ]
    .into_iter()
    .map(|[x, y]| [x + 0.00137, y + 0.00271])
    .collect();
    let points: Vec<Point> = star.iter().map(|[x, y]| px(*x, *y)).collect();
    let filled = fill_polygon(&points, W, H);
    for y in 0..H {
        for x in 0..W {
            let (cx, cy) = (
                (f64::from(x) + 0.5) / f64::from(W),
                (f64::from(y) + 0.5) / f64::from(H),
            );
            let mut inside = false;
            let mut j = star.len() - 1;
            for i in 0..star.len() {
                let ([xi, yi], [xj, yj]) = (star[i], star[j]);
                if ((yi > cy) != (yj > cy)) && (cx < (xj - xi) * (cy - yi) / (yj - yi) + xi) {
                    inside = !inside;
                }
                j = i;
            }
            assert_eq!(at(&filled, x, y) != 0, inside, "({x},{y})");
        }
    }

    // A column line (`a_column_line_is_a_band_of_its_width_at_the_column`).
    let column = |c: f64, width: f64| {
        core(Shape::Line {
            line: Line::Column(c * f64::from(W)),
            width,
        })
    };
    let band = column(0.5, 6.0);
    for x in 0..W {
        assert_eq!(at(&band, x, 0) != 0, (27..=32).contains(&x), "x {x}");
    }
    assert_eq!(count(&band), 6 * H as usize);
    assert_eq!(count(&column(0.5, 1.0)), 2 * H as usize, "a tie paints two");
    assert_eq!(count(&column(30.5 / 60.0, 1.0)), H as usize);

    // A path line and a stroke (`a_path_line_and_a_stroke_are_the_path_dilated`).
    let path = vec![px(0.1, 0.5), px(0.9, 0.5)];
    let line = core(Shape::Line {
        line: Line::Path(path.clone()),
        width: 8.0,
    });
    assert_eq!(at(&line, 30, 20), 255);
    assert_eq!((at(&line, 30, 16), at(&line, 30, 23)), (255, 255), "3.5 px");
    assert_eq!((at(&line, 30, 15), at(&line, 30, 24)), (0, 0), "4.5 px");
    assert_eq!(
        at(&line, 2, 20),
        255,
        "the round cap reaches 4 px past the end"
    );
    assert_eq!(at(&line, 1, 20), 0);
    let stroke = core(Shape::Stroke { path, radius: 4.0 });
    assert_eq!(stroke, line);
    let bent = [px(0.2, 0.2), px(0.5, 0.8), px(0.8, 0.3)];
    let widened = dilate_path(&bent, 3.0, W, H);
    for y in 0..H {
        for x in 0..W {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let distance = bent
                .windows(2)
                .map(|s| {
                    let (dx, dy) = (s[1].x - s[0].x, s[1].y - s[0].y);
                    let t = (((cx - s[0].x) * dx + (cy - s[0].y) * dy) / (dx * dx + dy * dy))
                        .clamp(0.0, 1.0);
                    ((cx - s[0].x - t * dx).powi(2) + (cy - s[0].y - t * dy).powi(2)).sqrt()
                })
                .fold(f64::INFINITY, f64::min);
            assert_eq!(at(&widened, x, y) != 0, distance <= 3.0 + 1e-9, "({x},{y})");
        }
    }

    // The box's rule (`a_box_covers_pixel_centres_and_agrees_with_the_same_box_as_a_polygon`).
    assert_eq!(
        box_pixels([0.0, 0.0, 0.55 * 100.0, 1.0], 100, 1),
        Some((0, 0, 55, 1))
    );
    assert_eq!(
        box_pixels(
            [0.2 * 320.0, 0.2 * 200.0, 0.6 * 320.0, 0.6 * 200.0],
            320,
            200
        ),
        Some((64, 40, 192, 120))
    );
    assert_eq!(
        box_pixels([0.0, 0.0, 0.004 * 100.0, 1.0], 100, 1),
        None,
        "no centre inside"
    );
    assert_eq!(
        box_pixels([0.0, 0.0, 0.006 * 100.0, 1.0], 100, 1),
        Some((0, 0, 1, 1))
    );
    assert_eq!(
        box_pixels([30.0, 0.0, 20.0, 1.0], 100, 1),
        None,
        "an empty box"
    );
    for [x0, y0, x1, y1] in [
        [0.11, 0.07, 0.55, 0.93],
        [0.0, 0.0, 1.0, 1.0],
        [0.333, 0.25, 0.666, 0.5],
    ] {
        let (w, h) = (f64::from(W), f64::from(H));
        let edges = [x0 * w, y0 * h, x1 * w, y1 * h];
        let (a, b, c, d) = box_pixels(edges, W, H).unwrap();
        let polygon = fill_polygon(&[px(x0, y0), px(x1, y0), px(x1, y1), px(x0, y1)], W, H);
        for y in 0..H {
            for x in 0..W {
                let inside = x >= a && x < c && y >= b && y < d;
                assert_eq!(at(&polygon, x, y) == 255, inside, "{edges:?} at {x},{y}");
            }
        }
    }

    // View and absent have no inside (`view_and_absent_are_not_applicable_...`), and a
    // location that covers no pixel makes no mask.
    let panorama = Binding::root(
        PixelDigest(Digest::of_bytes(b"pano")),
        Space::panorama(80, 40).unwrap(),
    );
    let view = Location::new(
        panorama,
        Shape::View {
            yaw: 0.0,
            pitch: 0.0,
            fov: 60.0,
        },
        made_by("find"),
    )
    .unwrap();
    let why = rasterize(&view, ("custom", UseCase::Custom)).unwrap_err();
    assert!(why.contains("no inside"), "{why}");
    let flat = Binding::root(
        PixelDigest(Digest::of_bytes(b"flat")),
        Space::flat(W, H).unwrap(),
    );
    let absent = Location::new(flat.clone(), Shape::Absent, made_by("find")).unwrap();
    assert!(rasterize(&absent, ("custom", UseCase::Custom)).is_err());
    let sliver = Shape::Box {
        left: 0.0,
        top: 0.0,
        right: 0.4,
        bottom: 1.0,
    };
    let sliver = Location::new(flat, sliver, made_by("find")).unwrap();
    let why = rasterize(&sliver, ("custom", UseCase::Custom)).unwrap_err();
    assert!(why.contains("covers no pixel"), "{why}");
}

#[test]
fn rasterize_runs_as_an_action_and_refuses_a_location_on_another_image() {
    let shop = Shop::new("rasterize", None);
    let image = picture(W, H);
    let id = shop.put(Artifact::Image(image.clone()));
    let stroke = Location::new(
        image.binding(),
        Shape::Stroke {
            path: vec![px(0.1, 0.5), px(0.9, 0.5)],
            radius: 4.0,
        },
        made_by("stroke"),
    )
    .unwrap();
    let location = shop.put(Artifact::Location(stroke.clone()));
    let made = shop.one(
        &Call::new("rasterize")
            .argument("use_case", Value::text("repair"))
            .input("location", location)
            .input("image", id),
    );
    let mask = shop.mask(made);
    assert_eq!(mask.use_case(), UseCase::Repair);
    assert_eq!(mask.source().locations, vec![stroke.id()]);
    assert_eq!(mask.binding(), &image.binding());
    assert_eq!(count(mask.core()), count(&core(stroke.shape().clone())));
    assert!(mask.blend().iter().chain(mask.protect()).all(|s| *s == 0));
    // On another picture of the same size: refused before anything runs.
    let other = shop.put(Artifact::Image(super::paste::image_of(
        &whizbang_core::identity::CanonicalPixels::from_rgba(W, H, vec![7; (W * H * 4) as usize])
            .unwrap(),
        "import",
    )));
    let why = shop.refused(
        &Call::new("rasterize")
            .input("location", location)
            .input("image", other),
    );
    assert!(why.contains("belongs to the image"), "{why}");
}
