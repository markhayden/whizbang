# Encoding a result: the time it takes, at three sizes

Plan m3, task O5, item 3 (the carry-over row on large results, from rascal, whose runs spent
most of their time encoding images and one large report page). Every image an action hands
back is written as a PNG by the core's one encoder (`identity::encode`), and that file is
part of the image's identity (ADR 0002). This record times that encoder against three other
settings of the same `image` encoder, on a synthetic picture (smooth gradients with a little
deterministic grain, as a photograph has) at three sizes, each timed three times with the
middle time kept. It was run by hand with
`scripts/heavy.sh encoding cargo run --release -p whizbang-core --example encoding`, alone on
the machine, from the commit below; `crates/whizbang-core/examples/encoding.rs` is the
program.

| Setting | 1024 by 512 | 4096 by 2048 | 8192 by 4096 |
|---|---|---|---|
| default compression, adaptive filter (the core's) | 94 ms, 965,715 bytes | 1,535 ms, 15,371,141 bytes | 6,184 ms, 61,421,508 bytes |
| fast compression, adaptive filter | 2 ms, 976,681 bytes | 38 ms, 15,574,525 bytes | 154 ms, 62,265,184 bytes |
| fast compression, sub filter | 1 ms, 1,014,046 bytes | 26 ms, 16,179,612 bytes | 103 ms, 64,692,634 bytes |
| no compression | 0 ms, 2,097,892 bytes | 4 ms, 33,559,108 bytes | 18 ms, 134,232,132 bytes |

**What it shows.** The core's setting spends about forty times the time of fast compression
with the same filter, for files about 1.4 per cent smaller: six seconds a result at the
panorama's size, which a recipe of eight steps pays at each step that hands back an image.
The sub filter is a third faster again and its files 4 per cent larger; no compression
doubles the store.

**The choice.** Fast compression with the adaptive filter. It is not switched in this pull
request: the encoder's bytes are part of every image's identity, so the switch changes the
identities of every image an action makes, the crops `view`'s record pins and the images
of the records of `view` and `grow`, which must be made again in the same change, with an
ADR, as a change that is costly to reverse. `docs/carry-over.md` holds it. Rascal's large
report page has no counterpart here: whizbang writes no report page until the report viewer
of milestone 5, so thumbnails in reports wait for it.

```toml
commit = "a933d72dacae1ff37153459d8b34da4134d24072"
whizbang = "0.0.1"
machine = "Apple M4 Max, release build"
current = "default_adaptive"
chosen = "fast_adaptive"
default_adaptive_8192x4096_ms = "6184"
fast_adaptive_8192x4096_ms = "154"
default_adaptive_8192x4096_bytes = "61421508"
fast_adaptive_8192x4096_bytes = "62265184"
```
