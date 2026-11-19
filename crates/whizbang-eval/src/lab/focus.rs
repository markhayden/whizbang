//! The focus of a pair (plan m3, task L3; rascal's `lab/focus.rs` at the commit
//! `docs/sources.toml` pins): where the two results of a pair changed their source, so the
//! pairs page shows that region at full size. [`Changes`] marks the columns and rows where a
//! result differs from its source; the two sides' changes are joined so both crops frame
//! one region; [`Focus`] is that region with context around it, through the edge of a
//! frame that wraps and clamped at the edge of a bounded one.
//!
//! Rascal compared three channels of decoded images; here the canonical pixels, four bytes
//! a pixel, are compared, so a change of alpha alone is a change. Rascal's centre (for its
//! look-around view, not ported) is left out.

use whizbang_core::identity::Topology;

/// Context around the changed region, on each axis: half the region's extent on it, at
/// least `CONTEXT_MIN` and at most `CONTEXT_MAX` pixels, on each side.
pub const CONTEXT_MIN: u32 = 64;
pub const CONTEXT_MAX: u32 = 512;

/// The columns and rows where a result differs from its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changes {
    width: u32,
    height: u32,
    columns: Vec<bool>,
    rows: Vec<bool>,
}

/// A region of a frame with context around it: `x` is its first column, and on a frame
/// that wraps `x + width` may pass the frame's width, the columns going on from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Focus {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Changes {
    /// Where `after` differs from `before`, both `width` by `height` pixels of four bytes
    /// each, row by row. Refused: sizes that do not agree.
    pub fn between(
        width: u32,
        height: u32,
        before: &[u8],
        after: &[u8],
    ) -> Result<Changes, String> {
        let stride = 4 * width as usize;
        let size = stride * height as usize;
        if before.len() != size || after.len() != size || size == 0 {
            return Err("a result and its source are not pictures of one size".to_owned());
        }
        let mut columns = vec![false; width as usize];
        let mut rows = vec![false; height as usize];
        for (y, (a, b)) in (before.chunks_exact(stride))
            .zip(after.chunks_exact(stride))
            .enumerate()
        {
            if a == b {
                continue;
            }
            rows[y] = true;
            for (x, (pa, pb)) in a
                .as_chunks::<4>()
                .0
                .iter()
                .zip(b.as_chunks::<4>().0)
                .enumerate()
            {
                if pa != pb {
                    columns[x] = true;
                }
            }
        }
        Ok(Changes {
            width,
            height,
            columns,
            rows,
        })
    }

    /// Adds another side's changes over the same frame.
    pub fn join(&mut self, other: &Changes) -> Result<(), String> {
        if (other.width, other.height) != (self.width, self.height) {
            return Err("the two sides of a pair are not pictures of one size".to_owned());
        }
        for (mine, theirs) in self.columns.iter_mut().zip(&other.columns) {
            *mine |= theirs;
        }
        for (mine, theirs) in self.rows.iter_mut().zip(&other.rows) {
            *mine |= theirs;
        }
        Ok(())
    }

    /// The changed region with context, or none when nothing changed. On a frame that
    /// wraps the columns are the tightest interval around the changes through the edge;
    /// when the context meets itself the region is the whole width.
    pub fn focus(&self, topology: Topology) -> Option<Focus> {
        let (y, height) = bounded_span(&self.rows)?;
        let (y, height) = with_context(y, height, self.height);
        let (x, width) = match topology {
            Topology::Bounded => {
                let (x, width) = bounded_span(&self.columns)?;
                with_context(x, width, self.width)
            }
            Topology::Wrap => {
                let (x, width) = wrapped_span(&self.columns)?;
                let context = context_of(width);
                if width + 2 * context >= self.width {
                    (0, self.width)
                } else {
                    ((x + self.width - context) % self.width, width + 2 * context)
                }
            }
        };
        Some(Focus {
            x,
            y,
            width,
            height,
        })
    }
}

fn context_of(extent: u32) -> u32 {
    (extent / 2).clamp(CONTEXT_MIN, CONTEXT_MAX)
}

/// `start` and `extent` grown by the context and clamped to `[0, size)`.
fn with_context(start: u32, extent: u32, size: u32) -> (u32, u32) {
    let context = context_of(extent);
    let first = start.saturating_sub(context);
    let end = (start + extent + context).min(size);
    (first, end - first)
}

/// The first marked index and the count through the last.
fn bounded_span(marks: &[bool]) -> Option<(u32, u32)> {
    let first = marks.iter().position(|&m| m)?;
    let last = marks.iter().rposition(|&m| m)?;
    Some((first as u32, (last - first + 1) as u32))
}

/// The tightest circular interval holding every mark: the complement of the longest
/// circular run of unmarked indices.
fn wrapped_span(marks: &[bool]) -> Option<(u32, u32)> {
    let n = marks.len();
    if !marks.iter().any(|&m| m) {
        return None;
    }
    if marks.iter().all(|&m| m) {
        return Some((0, n as u32));
    }
    let mut best: Option<(usize, usize)> = None;
    let mut run: Option<(usize, usize)> = None;
    for i in 0..2 * n {
        if marks[i % n] {
            run = None;
            continue;
        }
        let (start, len) = match run {
            Some((start, len)) => (start, (len + 1).min(n)),
            None => (i, 1),
        };
        run = Some((start, len));
        if best.is_none_or(|(_, best_len)| len > best_len) {
            best = Some((start, len));
        }
    }
    let (gap_start, gap_len) = best?;
    Some((((gap_start + gap_len) % n) as u32, (n - gap_len) as u32))
}

impl Focus {
    /// The region's pixels, four bytes each, of a picture `width` pixels wide, its columns
    /// taken modulo the width.
    pub fn crop(&self, width: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 * self.width as usize * self.height as usize);
        for y in self.y..self.y + self.height {
            let row = y as usize * width as usize;
            for x in 0..self.width {
                let at = 4 * (row + ((self.x + x) % width) as usize);
                out.extend_from_slice(&rgba[at..at + 4]);
            }
        }
        out
    }
}
