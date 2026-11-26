//! The record of `subtract`, `docs/reference/tools/subtract.md`.

use whizbang_core::artifact::{Location, Mask, Shape};
use whizbang_rascal::examples::{
    subtract_bar as bar, subtract_faint as faint, subtract_oval as oval,
};
use whizbang_rascal::subtract::{Other, subtract};

use super::super::grow::{made_by, mask_of, plane};
use super::super::record::{picture, tiled, time};
use super::{Page, SIZES, count, hold, timed_head, timed_row};

const W: usize = 96;

fn never() -> bool {
    false
}

struct Example {
    name: &'static str,
    grow_px: f64,
    mask: Mask,
    other: Mask,
    after: Mask,
}

fn examples() -> [Example; 2] {
    let example = |name, other: Mask, grow_px| {
        let after = subtract(oval(), Some(Other::Mask(&other)), grow_px, &never)
            .unwrap()
            .unwrap()
            .mask;
        Example {
            name,
            grow_px,
            mask: oval(),
            other,
            after,
        }
    };
    [example("bar", bar(), 2.0), example("faint", faint(), 0.0)]
}

pub fn page(tool: &'static str) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from(
        "\n| Example | `grow_px` | Second mask's core | Its blend | Core before | Core after \
         | Blend before | Blend after | Protected before | Protected after | Profile and \
         prescriptions |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for example in examples() {
        let (mask, after) = (&example.mask, &example.after);
        files.push((format!("{}.png", example.name), picture(mask)));
        files.push((
            format!("{}-second.png", example.name),
            picture(&example.other),
        ));
        files.push((format!("{}-subtracted.png", example.name), picture(after)));
        measured.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            example.name,
            example.grow_px,
            count(example.other.core()),
            count(example.other.blend()),
            count(mask.core()),
            count(after.core()),
            count(mask.blend()),
            count(after.blend()),
            count(mask.protect()),
            count(after.protect()),
            if after.profile() == mask.profile() && after.prescriptions() == mask.prescriptions() {
                "kept"
            } else {
                "CHANGED"
            },
        ));
    }
    measured.push('\n');
    Page {
        tool,
        files,
        measured,
    }
}

pub fn timed(_: &str) -> String {
    let mut out = timed_head(&[
        "`subtract` of a mask, `grow_px` 4",
        "`subtract` of a box, `grow_px` 4",
    ]);
    for (width, height) in SIZES {
        let mask = tiled(width, height);
        let other = mask_of(
            width,
            height,
            plane(width, height, |x, _| x % 256 < 16),
            &vec![0; width * height],
        );
        let shape = Shape::Box {
            left: 10.0,
            top: 10.0,
            right: width as f64 / 2.0,
            bottom: height as f64 / 2.0,
        };
        let boxed = Location::new(mask.binding().clone(), shape, made_by("rect")).unwrap();
        let by_mask = time(
            5,
            || mask.clone(),
            |m| subtract(m, Some(Other::Mask(&other)), 4.0, &never),
        );
        let by_box = time(
            5,
            || mask.clone(),
            |m| subtract(m, Some(Other::Location(&boxed)), 4.0, &never),
        );
        out.push_str(&timed_row(width, height, &[by_mask, by_box]));
    }
    out.push('\n');
    out
}

#[test]
fn the_record_of_subtract_is_what_subtract_gives() {
    let page = page("subtract");
    assert!(!page.measured.contains("CHANGED"), "{}", page.measured);
    hold(&page);
    let [bar, faint] = examples();
    let column = |mask: &Mask, y: usize, plane: fn(&Mask) -> &[u8]| -> Vec<usize> {
        (0..W).filter(|x| plane(mask)[y * W + x] != 0).collect()
    };
    // The bar, columns 46 to 49, grown by 2: protect is columns 44 to 51 in every row,
    // and neither the core nor the blend is set there.
    for y in [0, 32, 63] {
        assert_eq!(
            column(&bar.after, y, Mask::protect),
            (44..52).collect::<Vec<_>>()
        );
    }
    for x in 44..52 {
        assert_eq!(
            (bar.after.core()[32 * W + x], bar.after.blend()[32 * W + x]),
            (0, 0)
        );
    }
    // The faint disc: its core is 4 pixels from its centre, and protect reaches 9 more,
    // where its blend was faintest.
    let row = column(&faint.after, 32, Mask::protect);
    assert_eq!((row[0], row[row.len() - 1]), (17, 43));
    assert_eq!(faint.other.blend()[32 * W + 17], 5);
    assert_eq!(
        column(&faint.other, 32, Mask::core),
        (26..35).collect::<Vec<_>>()
    );
}
