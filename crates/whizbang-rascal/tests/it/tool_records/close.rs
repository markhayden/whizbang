//! The record of `close`, `docs/reference/tools/close.md`.

use whizbang_core::artifact::Mask;
use whizbang_rascal::close::close;
use whizbang_rascal::examples::{close_blocks as blocks, close_discs as discs};
use whizbang_rascal::grow::radius_squared;

use super::super::record::{picture, tiled, time};
use super::{Page, SIZES, count, hold, timed_head, timed_row};

const W: usize = 96;

fn never() -> bool {
    false
}

fn examples() -> [(&'static str, f64, Mask, Mask); 2] {
    let closed = |mask: Mask, radius| close(mask, radius, &never).unwrap().unwrap().mask;
    [
        ("blocks", 3.0, blocks(), closed(blocks(), 3.0)),
        ("discs", 3.0, discs(), closed(discs(), 3.0)),
    ]
}

pub fn page(tool: &'static str) -> Page {
    let mut files = Vec::new();
    let mut measured = String::from(
        "\n| Example | `radius_px` | K | Core before | Core after | Blend before | Blend after \
         | Protected before and after |\n|---|---|---|---|---|---|---|---|\n",
    );
    for (name, radius, before, after) in examples() {
        files.push((format!("{name}.png"), picture(&before)));
        files.push((format!("{name}-closed.png"), picture(&after)));
        assert_eq!(before.protect(), after.protect());
        measured.push_str(&format!(
            "| {name} | {radius} | {} | {} | {} | {} | {} | {} |\n",
            radius_squared(radius),
            count(before.core()),
            count(after.core()),
            count(before.blend()),
            count(after.blend()),
            count(after.protect()),
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
    let mut out = timed_head(&["`close`, `radius_px` 3", "`close`, `radius_px` 100"]);
    for (width, height) in SIZES {
        let mask = tiled(width, height);
        let near = time(5, || mask.clone(), |m| close(m, 3.0, &never));
        let far = time(5, || mask.clone(), |m| close(m, 100.0, &never));
        out.push_str(&timed_row(width, height, &[near, far]));
    }
    out.push('\n');
    out
}

#[test]
fn the_record_of_close_is_what_close_gives() {
    hold(&page("close"));
    let [(_, _, blocks, closed), (_, _, discs, merged)] = examples();
    let at = |mask: &Mask, x: usize, y: usize| mask.core()[y * W + x] != 0;
    // The gap and the notch are filled but for the one row at each mouth, which a disc
    // of radius 3 does not reach across; the outer edges are where they were; the patch
    // in the gap stays out of the core.
    assert!(!at(&blocks, 45, 20) && at(&closed, 45, 20));
    assert!(!at(&closed, 45, 16) && at(&closed, 45, 17));
    assert!(at(&closed, 45, 46) && !at(&closed, 45, 47));
    assert!(!at(&closed, 29, 16) && at(&closed, 29, 17) && !at(&blocks, 29, 17));
    assert_eq!(count(closed.core()) - count(blocks.core()), 110);
    assert!(at(&closed, 20, 16) && !at(&closed, 19, 16) && !at(&closed, 20, 15));
    assert!(at(&closed, 71, 47) && !at(&closed, 72, 47) && !at(&closed, 71, 48));
    assert!(!at(&closed, 45, 31));
    // The two discs are one shape after, joined across the gap, and the feather is gone.
    assert!(!at(&discs, 46, 32) && at(&merged, 46, 32));
    assert!(count(discs.blend()) > 0 && count(merged.blend()) == 0);
}
