//! The size a cut is shown at, and scaling it down. Whole numbers throughout, so the same
//! cut gives the same bytes on every machine.

use whizbang_core::identity::CanonicalPixels;

/// A box of whole pixels: from `left`, `top` up to and not including `right`, `bottom`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Region {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Region {
    pub(super) fn width(&self) -> u32 {
        self.right - self.left
    }

    pub(super) fn height(&self) -> u32 {
        self.bottom - self.top
    }
}

/// The size a cut of `width` by `height` is shown at: itself when its longer side is at
/// most `bound`; otherwise the longer side becomes `bound` and the shorter one is scaled
/// by the same ratio, to the nearest whole pixel, a half going up, and to at least 1.
pub(super) fn fit(width: u32, height: u32, bound: u32) -> (u32, u32) {
    let longer = width.max(height);
    if longer <= bound {
        return (width, height);
    }
    let scaled = |side: u32| {
        let (side, bound, longer) = (u64::from(side), u64::from(bound), u64::from(longer));
        // At most `bound`, since `side` is at most `longer`.
        ((2 * side * bound + longer) / (2 * longer)).max(1) as u32
    };
    if width >= height {
        (bound, scaled(height))
    } else {
        (scaled(width), bound)
    }
}

/// The region's pixels, as they are.
pub(super) fn cut(pixels: &CanonicalPixels, region: &Region) -> Vec<u8> {
    let stride = pixels.width() as usize * 4;
    let (from, to) = (region.left as usize * 4, region.right as usize * 4);
    let mut out = Vec::with_capacity((to - from) * region.height() as usize);
    for row in region.top as usize..region.bottom as usize {
        out.extend_from_slice(&pixels.rgba()[row * stride..][from..to]);
    }
    out
}

/// For each of `target` equal parts of a side of `source` pixels: the first source pixel
/// the part touches, and how much of each pixel from there on lies in the part, in units
/// of one `target`th of a pixel. A part's weights add up to `source`.
fn spans(source: u64, target: u64) -> Vec<(usize, Vec<u64>)> {
    (0..target)
        .map(|part| {
            let (low, high) = (part * source, (part + 1) * source);
            let first = low / target;
            let weights = (first..)
                .take_while(|pixel| pixel * target < high)
                .map(|pixel| ((pixel + 1) * target).min(high) - (pixel * target).max(low))
                .collect();
            (first as usize, weights)
        })
        .collect()
}

/// The region scaled down to `width` by `height`: neither is larger than the region's.
///
/// The region is divided into `width` by `height` equal rectangles, one for each output
/// pixel. An output sample is the mean of the source samples over its rectangle, each
/// source pixel counted by the area of it that lies inside the rectangle; the mean is
/// rounded to the nearest whole number, a half going up. The four channels are averaged
/// apart, and alpha is not multiplied into the colours. Every sum is a whole number: the
/// weights are in units of one output pixel's share of a source pixel, so nothing is
/// rounded before the one division.
///
/// `None` when the sums could pass 64 bits, which takes a region of more than 2^54
/// pixels.
pub(super) fn downscale(
    pixels: &CanonicalPixels,
    region: &Region,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let (cut_width, cut_height) = (u64::from(region.width()), u64::from(region.height()));
    let area = cut_width.checked_mul(cut_height)?;
    // The largest value divided below is 2 * 255 * area + area.
    area.checked_mul(511)?;
    let columns = spans(cut_width, u64::from(width));
    let rows = spans(cut_height, u64::from(height));
    let stride = pixels.width() as usize * 4;
    let rgba = pixels.rgba();
    let across = width as usize * 4;

    // One source row summed into the output's columns, kept while the next output row
    // wants it too: a source row on the border between two output rows is in both.
    let mut sums = vec![0u64; across];
    let mut held = None;
    let mut totals = vec![0u64; across];
    let mut out = Vec::with_capacity(across * height as usize);
    for (first_row, weights) in &rows {
        totals.fill(0);
        for (offset, weight) in weights.iter().enumerate() {
            let row = region.top as usize + first_row + offset;
            if held != Some(row) {
                let line = &rgba[row * stride + region.left as usize * 4..];
                let (line, _) = line.as_chunks::<4>();
                let (sums, _) = sums.as_chunks_mut::<4>();
                for ((first, weights), sum) in columns.iter().zip(sums) {
                    *sum = [0; 4];
                    for (weight, pixel) in weights.iter().zip(&line[*first..]) {
                        for (channel, sample) in sum.iter_mut().zip(pixel) {
                            *channel += weight * u64::from(*sample);
                        }
                    }
                }
                held = Some(row);
            }
            for (total, sum) in totals.iter_mut().zip(&sums) {
                *total += weight * sum;
            }
        }
        out.extend(
            totals
                .iter()
                .map(|total| ((2 * total + area) / (2 * area)) as u8),
        );
    }
    Some(out)
}
