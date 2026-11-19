//! The time of encoding a result (plan m3, task O5, item 3; the carry-over row on large
//! results): the encoder of an image artifact's file, `identity::encode` (the setting it
//! names, fast compression since 2026-11-19), against three other settings of the same
//! encoder, on a synthetic picture at three sizes, each timed
//! three times and the middle time kept. Run by hand, in a release build, as
//! `scripts/heavy.sh encoding cargo run --release -p whizbang-core --example encoding`;
//! it prints the fields `docs/results/m3-encoding.md` holds.

use std::time::Instant;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use whizbang_core::identity::encode::{self, Channels};

/// The settings measured: the core's until 2026-11-19 first, then the core's since.
const SETTINGS: [(&str, CompressionType, FilterType); 4] = [
    (
        "default_adaptive",
        CompressionType::Default,
        FilterType::Adaptive,
    ),
    ("fast_adaptive", CompressionType::Fast, FilterType::Adaptive),
    ("fast_sub", CompressionType::Fast, FilterType::Sub),
    (
        "uncompressed",
        CompressionType::Uncompressed,
        FilterType::NoFilter,
    ),
];

/// The sizes: a crop, a phone's picture, and a panorama near the decode limit.
const SIZES: [(u32, u32); 3] = [(1024, 512), (4096, 2048), (8192, 4096)];

/// Smooth gradients with a little deterministic grain, as a photograph has: neither
/// flat, which compresses to nothing, nor noise, which does not compress.
fn picture(width: u32, height: u32) -> Vec<u8> {
    let mut state = 0x2545_f491_u32;
    let mut grain = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state % 9) as i32 - 4
    };
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let (fx, fy) = (x as f32 / width as f32, y as f32 / height as f32);
            let base = [
                255.0 * fx,
                255.0 * fy,
                128.0 + 100.0 * (6.0 * fx + 3.0 * fy).sin(),
            ];
            for b in base {
                rgba.push((b as i32 + grain()).clamp(0, 255) as u8);
            }
            rgba.push(255);
        }
    }
    rgba
}

fn main() -> Result<(), String> {
    for (width, height) in SIZES {
        let rgba = picture(width, height);
        for (name, compression, filter) in SETTINGS {
            let mut times = Vec::new();
            let mut bytes = 0;
            for _ in 0..3 {
                let started = Instant::now();
                let file = if name == encode::SETTING {
                    encode::png_of(&rgba, width, height, Channels::Rgba)?
                } else {
                    let mut file = Vec::new();
                    PngEncoder::new_with_quality(&mut file, compression, filter)
                        .write_image(&rgba, width, height, ExtendedColorType::Rgba8)
                        .map_err(|error| error.to_string())?;
                    file
                };
                times.push(started.elapsed().as_millis());
                bytes = file.len();
            }
            times.sort_unstable();
            println!("{name}_{width}x{height}_ms = \"{}\"", times[1]);
            println!("{name}_{width}x{height}_bytes = \"{bytes}\"");
        }
    }
    Ok(())
}
