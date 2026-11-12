# 0001. Identities: two digests for an image, a space without content, a binding between images

Status: accepted
Date: 2026-11-12
Spec: section 3, "Identities"; section 16, checkpoint 1A

## Context

`SPEC.md` section 3 separates three things the old tools both called a frame: what an image
is, what space its coordinates live in, and how one image sits on another. It leaves the
types to checkpoint 1A. Neither old identity can be kept as it is:

- Rascal's frame (`crates/rascal-core/src/frame.rs`, and its ADR 0016) has no content: two
  images of the same size and projection are in the same raw frame, so a mask made on one is
  accepted on the other.
- Squint's decoder (`crates/squint-core/src/image.rs`) hashes the pixels after applying the
  orientation tag, but drops the alpha channel from the hash and records a colour profile
  without applying it, so two different pictures can share a digest. Its frames
  (`crates/squint-core/src/frames/mod.rs`) are named by ids that mean something only inside
  one run.

Both are read at the commits pinned in `docs/sources.toml`.

## Decision

The types are in `crates/whizbang-core/src/identity/`. The tests that hold them to this
record are in `crates/whizbang-core/tests/it/identity/`.

### The order identities depend on each other

1. **Content.** The file digest and the canonical pixel digest. Each depends on bytes alone.
   A space has no digest of its own: it is four plain values.
2. **Bindings.** A binding's identity depends on pixel digests, spaces and a mapping.
3. **Run provenance.** What produced an artifact (the action, its arguments, the identities
   of its inputs, the models) comes last and depends on the two above. Its types arrive with
   artifacts and the execution contract; nothing above depends on it.

There is no cycle. Nothing local enters any identity: no id given by a store or a run, no
timestamp, no path, no file name. Every reference from one identity to another is that
thing's digest.

### How an identity is hashed

Every identity but the file digest is the SHA-256 of one encoding:

- the bytes `whizbang:`, the kind with its version (`pixels/1`, `binding/1`), and a zero
  byte;
- then each field, in the order this record gives, as one tag byte and a value: `u` and an
  unsigned integer as eight bytes, big-endian; `f` and an IEEE 754 double as its eight
  bytes, big-endian, with negative zero written as zero; `d` and the 32 bytes of a digest;
  `b`, a length as eight bytes, big-endian, and that many bytes.

A digest is written `sha256:` and 64 lower-case hex digits. A number that is not finite is
refused before anything is hashed.

### The file digest

The SHA-256 of the file's bytes exactly as stored, with nothing added. It is what
`shasum -a 256` prints.

### The canonical pixel digest

The identity `pixels/1` over three fields: the width (`u`), the height (`u`), and the
canonical pixels (`b`). Canonical pixels are the picture as it is shown:

- **Orientation.** The EXIF orientation tag (1 to 8) is applied, so the pixels are upright
  and the width and height are those of the upright picture. A tag that is absent, cannot
  be read, or is outside 1 to 8 is no tag. In a PNG the tag is read from the first `eXIf`
  chunk whose checksum holds, wherever the chunk sits, before the image data or after it.
- **Channel order.** Four bytes a pixel, in the order red, green, blue, alpha, row by row
  from the top left. A grey image has three equal colour channels. A palette is looked up:
  each pixel is its entry's colour.
- **Depth.** Eight bits a channel. A sample of 1, 2 or 4 bits is multiplied by
  `255 / (2^n - 1)`: by 255, 85 or 17. A 16-bit sample `v` becomes
  `(v * 255 + 32767) / 65535` in whole numbers: the nearest 8-bit value. Floating-point
  samples are refused.
- **Alpha.** Straight, never multiplied into the colours. An image without alpha is opaque
  (255), so an opaque alpha channel and no alpha channel are the same picture. A PNG's
  `tRNS` chunk is alpha: for a palette, each entry's alpha, and opaque for entries it does
  not reach; for grey or colour, the one sample value or colour it names is transparent (0)
  and every other is opaque, and the comparison is made at the stored depth, before a
  16-bit sample is narrowed. The colour under a transparent pixel is kept as stored, and is
  part of the picture.
- **Colour.** Canonical pixels are sRGB. An image with no embedded ICC profile is taken as
  sRGB, and its samples are kept. An image whose profile is sRGB keeps its samples too. An
  image with any other RGB matrix profile is converted to sRGB.

**What is refused.** These have no canonical reading, and the image is refused, never
guessed:

- a profile that is not an RGB matrix profile: one that works through lookup tables alone,
  a grey or CMYK profile, any profile on a grey image, a damaged profile;
- a profile that is present and cannot be read (below);
- a JPEG whose frame has other than one or three components (four is CMYK, and how its
  inks become colours is not defined here), or samples that are not 8 bits;
- any other sample layout that is not grey or red, green and blue in whole numbers: CMYK
  or floating point from any decoder;
- a JPEG that ends before its end marker, and a PNG that ends before its end chunk (below).

**Where the profile comes from.** The container is read apart from the decoder, because a
decoder says nothing of a profile it cannot read, and the picture read without it would be
another picture.

- In a PNG, the profile is the first `iCCP` chunk, wherever it sits; later ones are not
  applied. The chunk is refused when its checksum fails, when it has no name of 1 to 79 bytes
  ending in a zero byte, when its compression method is not 0, or when its data does not
  inflate. A PNG's gamma, chromaticity, sRGB and coding-independent chunks (`gAMA`, `cHRM`,
  `sRGB`, `cICP`) are not read.
- In a JPEG, the profile is the data of its `ICC_PROFILE` segments, which are numbered from
  1 and each say how many there are. They may come in any order. They are refused unless
  each number from 1 to the count comes exactly once and every segment gives the same
  count.
- A profile of no bytes is no profile.
- A profile of more than 1,048,576 bytes, once inflated, is over the limit. The inflation
  stops at the limit.

A profile is read as an **RGB matrix profile** when it has the profile signature, its data
is RGB, its connection space is XYZ, and it has the six tags `rXYZ`, `gXYZ`, `bXYZ`,
`rTRC`, `gTRC` and `bTRC`; other tags are ignored. It is refused when it lists more than
1,024 tags, when a tag points past its end, when a curve's table has more than 65,536
entries, when the determinant of its three colorants is under 1e-6 in size, or when a curve
has no finite value at one of the 256 samples. Numbers are signed, with 16 fractional bits.
A curve is:

- a table of entries, each a fraction of 65535. No entry is the identity. One entry is a
  power, with eight fractional bits. With `n` entries, more than one, the value at `x` is
  on the straight line between entries `k` and `k + 1`, where `x (n - 1)` lies between
  them: `k` is its whole part, and at most `n - 2`;
- or one of the five parametric forms of the ICC specification, with parameters
  `g a b c d e f`: `x^g`; `(a x + b)^g` from `-b / a` up and 0 below; the same plus `c`,
  and `c` below; `(a x + b)^g` from `d` up and `c x` below; `(a x + b)^g + e` from `d` up
  and `c x + f` below. A form that divides by an `a` of zero is refused.

A profile **is sRGB** when each of its nine colorant values is within 0.001 of sRGB's, and
each channel's curve is within 0.001 of the sRGB curve at each of the 256 samples. sRGB's
colorants are those of its ICC profile, as fractions of 65536: red 28578, 14581, 912; green
25241, 46981, 6362; blue 9376, 3972, 46799. The sRGB curve from a sample to light is
`v / 12.92` up to 0.04045, else `((v + 0.055) / 1.055)^2.4`.

The **conversion** is done after the samples are 8 bits and before the picture is turned
upright. Alpha is not touched. All of it is IEEE 754 double precision, in the order given;
every power is computed by the `libm` crate, pinned to one version, and not by the
machine's own library, so the bytes are the same on every machine.

1. Once for the profile: for each channel, the light of each of the 256 samples is its
   curve's value at `sample / 255`, clipped to 0 to 1.
2. Once for the profile: `P` is the matrix whose columns are the profile's red, green and
   blue colorants, and `S` the same for sRGB. The inverse of `S` is its adjugate with each
   entry divided by the determinant: entry `(i, j)` is the 2 by 2 minor of `S` without row
   `j` and column `i`, as `a d - b c`, divided by the determinant, and negated when
   `i + j` is odd. The determinant is expanded along the first row. `M` is that inverse
   times `P`, each entry the sum of its three products from left to right.
3. For each pixel and each output channel, with `r`, `g`, `b` the light of the pixel's
   three samples: `M[0] r + M[1] g + M[2] b` along that channel's row, from left to right;
   clipped to 0 to 1; encoded with the sRGB curve (`12.92 v` up to 0.0031308, else
   `1.055 v^(1/2.4) - 0.055`); multiplied by 255; and one half added and the whole part
   taken.

**Lossy files and the decoder.** A PNG's samples are what the file says. A JPEG's are what
a decoder makes of it. Two decoders, or two versions of one, can differ: by a level or two
where they round differently, and by up to about 20 levels in a file whose colour is
stored at lower resolution, which each fills in its own way. So for a lossy file the decoder and its version are part of what `pixels/1`
means: the `image` crate and the JPEG decoder it uses (`zune-jpeg`, with `zune-core`) are
pinned to exact versions in `crates/whizbang-core/Cargo.toml`, and the JPEG vectors fail if
a decoder decodes them differently. A change in a decoder's output is carried by the
version in the kind: it is a new `pixels/2`, by a new record, or the proof that every
vector is unchanged. How a store is brought from one to the next is not designed here, and
is on `docs/carry-over.md`.

**A file that ends early.** A decoder is content with less than the whole file, and reports
nothing, so both containers are walked to their end before anything is decoded.

- A PNG's chunks are walked from the signature, each by its length, and the file is
  refused unless they reach an end chunk (`IEND`) that is well formed: no data, and a
  checksum that holds. A decoder stops caring once the image data is whole; a file cut
  inside a profile or an orientation tag that follows the image data would otherwise be
  read without it, as another picture.
- A JPEG's markers are walked, through each scan's data (in which a stuffed zero, a
  restart marker and a fill byte are not the end of the scan), and the file is refused
  unless they reach the end marker. A segment whose length is under 2, which is less than
  the length's own two bytes, is refused.
- What follows the end chunk or the end marker is not part of the picture and is not read.
  Two files that differ only there have two file digests and one pixel digest.

For a JPEG this sees only the markers. A file that has its end marker and too little data
before it is decoded as far as it goes and is given an identity; that is on
`docs/carry-over.md`.

**Limits.** Checked before anything is decoded, each with a default the caller may change:
64 MiB of file, 50,000,000 pixels as stored, and 1 MiB of profile. One more is fixed: the
decoded samples may take at most 512 MiB, which is more than 50,000,000 pixels need at
their widest (8 bytes a pixel). Identifying an image holds, at its peak, the file, the
decoded samples, and the canonical pixels at 4 bytes a pixel; and a second copy of those
while a quarter turn is applied. An 8-bit colour image of 49 megapixels peaked at 346 MB.

Which containers can be decoded is not part of the identity. The core decodes PNG and JPEG,
which is what the old tools read.

### Coordinate space

Four values and no content: width, height, projection (flat or equirectangular) and
topology (bounded, or wrapped: the left and right edges are one column). Two spaces are
equal only when all four match. Both dimensions are at least 1, and a flat space does not
wrap. An equirectangular space that does not wrap is part of a panorama.

Coordinates are pixels and continuous. `x` runs from 0 at the left edge to the width at the
right, `y` from 0 at the top to the height at the bottom; pixel `(i, j)` covers
`[i, i + 1) × [j, j + 1)` and its centre is `(i + 0.5, j + 0.5)`. A bounded space holds its
edges and nothing past them. A wrapped space takes any column round to `[0, width)`; rows
never wrap. In an equirectangular space the left edge is longitude -180 degrees, the right
edge 180, the top latitude 90 and the bottom -90, and the direction of a longitude and
latitude is `(cos lat cos lon, cos lat sin lon, sin lat)`.

### Binding

A binding places a source image onto a destination. It holds, and its identity
`binding/1` hashes, in this order:

1. the source image's canonical pixel digest (`d`);
2. the source space: width, height, projection (0 flat, 1 equirectangular) and topology
   (0 bounded, 1 wrapped), each `u`;
3. the destination: `u` 0 for the root, or `u` 1 and the destination image's canonical
   pixel digest (`d`);
4. the destination space, as the source space;
5. the direction: `u` 0 when the mapping's parameters take the source's points to the
   destination's, 1 when they take the destination's to the source's;
6. the mapping: its kind (`u`) and its parameters as they were given.

Images are named by their pixel digests, not their file digests, so a binding, and a mask
that belongs to it, holds for every file that stores the same picture. The **root** is no
image: a binding to the root is an image in its own space, and its mapping is the identity.
Aligning one source onto two references of the same size gives two bindings, because the
destination's digest differs.

The mappings. `from` and `to` are the two spaces in the order the parameters run, which the
direction decides:

| Kind | Parameters | Needs | Meaning |
|---|---|---|---|
| 0 identity | none | The two spaces are equal | The same point |
| 1 crop | left, top, right, bottom (`f`), in `to`'s pixels | The box has area and lies inside `to`. `from` is flat; when the box is the whole of `to`, `from` has `to`'s projection and topology instead | The whole of `from` covers the box, at any scale |
| 2 homography | nine numbers (`f`), row-major | Both spaces flat; the matrix can be inverted | Takes `from`'s pixels to `to`'s, in homogeneous coordinates |
| 3 rotation | nine numbers (`f`), row-major | Both spaces complete panoramas; the rows are unit vectors at right angles to within 1e-9 and the determinant is 1 | Takes `from`'s directions to `to`'s |
| 4 wrap | the strip width (`u`) | `to` is a complete panorama; the strip is even, at least 2, and at most half `to`'s width; `from` is flat, twice the strip wide and as tall as `to` | `from` holds `to`'s last strip of columns and then its first, so the seam is `from`'s middle column |

- A mapping that does not fit its two spaces is refused when the binding is built, and again
  when a binding is read from a record.
- A homography is singular, and refused, when an entry is not finite, or when, taken
  between the two spaces' unit squares, its determinant is at most 1e-12 of the cube of its
  Frobenius norm. The test holds for entries, so taken, between about 1e-100 and 1e100 in
  size; outside that the cube overflows or the determinant vanishes, and the matrix is
  refused.
- Every mapping runs both ways. A point outside the space it is given in is refused, and so
  is a point that lands outside the space it maps to: a point of an image that a crop does
  not cover has no place in the crop. A computed point up to 1e-9 of a pixel past an edge
  of a bounded space is on the edge.
- A crop maps a point from the nearer edge of its box, as two crops compose (below), and
  back the same way, so each edge of the crop maps onto its edge of the box exactly, and
  back exactly.
- In a wrapped space the right edge is column 0. A crop whose box reaches the right edge
  of a wrapped space holds that column as its own right edge: the crop's right edge maps to
  column 0, and column 0 maps back to the crop's right edge. A crop cannot cross the seam;
  a wrap does that.
- The reverse of a binding swaps its two ends, keeps the parameters and flips the direction.
  Nothing is recomputed, and it maps as the inverse.
- Two bindings compose into one when the second starts where the first ends: two crops,
  each stated from its source, into a crop; any other pair of crops and homographies into a
  homography; rotations into a rotation. A wrap does not compose with another mapping; its
  points are mapped through each binding in turn.
- Two crops compose edge by edge. With the outer box from `low` to `high` and the inner
  edge at `at` of the outer crop's `size`, the edge is `low + at (high - low) / size` when
  `at` is in the first half, and `high - (size - at) (high - low) / size` otherwise. An
  edge the inner crop shares with the outer one is then the outer one's edge exactly, and
  no edge passes it.

**What an equal identity means.** Two bindings with the same identity are the same
placement. The reverse does not hold: one placement can be stated in more than one way,
and each statement has its own identity. A binding and the reverse of its reverse are
equal, but the same mapping stated in the other direction with inverted parameters is
not; `(a then b) then c` and `a then (b then c)` may differ in their last bits; a crop
written as a homography is another binding; and a homography's parameters are hashed as
given, so two scalings of one matrix are two bindings. Later code must never take two
different binding identities to mean two different placements, and must never look a
placement up by an identity it has recomputed from another statement of it. Whether two
bindings place alike is a question about their mappings, within a tolerance, and nothing
here answers it.

**Points are not hashed.** Mapping a point through a rotation uses the machine's own sine,
cosine and arc functions, which may differ in their last bits from one machine to another.
Nothing hashed depends on them today: an identity holds the parameters of a mapping and
never a mapped point. A mapped point that enters a record whose bytes are hashed must be
computed with `libm`, as the colour conversion is.

## Consequences

- A mask made on one image cannot be mistaken for a mask on another of the same size, which
  rascal's frames allowed. Using it on another image is an explicit rebind, which arrives
  with masks.
- A copy of a picture in another container, at another depth, through a palette, under
  another orientation tag, or with an opaque alpha channel added, is recognised. A lossy
  recompression is not, and nothing here tries to recognise one.
- The same samples under two different profiles are two pictures, which squint's digest did
  not see.
- Two files that render alike can still be two pictures. Anything that passes pixels
  through alpha multiplied into the colours and back, and any optimiser that sets the
  colour under transparent pixels to zero, changes the canonical pixels and so the
  identity.
- An image whose profile is not an RGB matrix profile, and a CMYK JPEG, are refused until a
  later record gives them a canonical reading. Colour information a PNG carries outside an
  ICC profile is not read, and such a file is taken as sRGB. Both are on
  `docs/carry-over.md`.
- Pictures deeper than 8 bits lose depth in their identity: two 16-bit pictures that differ
  only below 8 bits share a pixel digest. The tools work on 8-bit pixels.
- The pixel digest is fixed by this record and by pinned versions, not by whatever a
  library does next: the vectors in
  `crates/whizbang-core/tests/it/identity/vectors_table.rs` are files with the digests they
  must have, the conversion's expected values and the digests of a sweep of colours were
  computed apart from the code, and a decoder or a power that changes fails a test.
- The core depends on the `image` crate to decode, with `png`, `zune-jpeg` and `zune-core`
  named only to pin them; `miniz_oxide` to inflate a PNG's profile inside a limit; `sha2`
  to hash; and `libm` for powers. All but `sha2` are pinned to exact versions, so raising
  one is a deliberate change that the vectors judge.
- Reversing this costs every stored identity. A store written under `pixels/1` and
  `binding/1` is discarded or migrated, and the vectors are regenerated. A change to either
  encoding is a new kind version and a new record that supersedes this one.
- A new kind of mapping (squint's perspective view into a panorama is the first that will
  be wanted) is a new kind number in `binding/1`, and its own record.

## Alternatives

- **Keep rascal's frame, and add the image to it.** A frame would then be a space and a
  binding at once, and every operation that only needs dimensions would carry an image.
- **Name images in a binding by their file digests.** A mask would stop applying when its
  image was saved again without a change to one pixel.
- **Hash records as canonical JSON,** as rascal's ADR 0016 does. The bytes would depend on
  how a JSON library orders keys and prints numbers, and one of its features, turned on
  anywhere in the workspace, changes the order.
- **Record the profile and hash the samples as stored,** as squint did. Two different
  pictures share a digest.
- **Hash the profile's bytes beside the samples, without converting.** The many files that
  write sRGB differently would each be another picture.
- **Convert through a colour management library.** Its rounding, which may differ by
  processor and by version, would be part of every identity. The matrix arithmetic above is
  small enough to state in full.
- **Canonical pixels at 16 bits.** Every picture would be hashed and held at twice the
  size, for tools that work at 8.
- **Set the colour under transparent pixels to zero.** It would make two files that differ
  only where nothing shows the same picture, but an edit can make those pixels show, so the
  colours are kept.
- **Normalise a homography before hashing it.** Dividing by a norm rounds, so two scalings
  would still differ in their last bits; the parameters are hashed as given and the rule is
  stated.
- **Trust the decoder for the profile, the orientation tag and the end of the data.** It
  passes over a profile it cannot read, reads a PNG's tag only before the image data, and
  fills in a JPEG that ends early, each without a word; the picture it then gives is not
  the file's.
- **Give a CMYK JPEG the decoder's own conversion.** It is one rule among several in use,
  with no profile behind it, and it would become part of every such identity.
- **Compose two crops as matrices.** A multiplication and a division do not return an edge
  to where it was: the composed box can pass its parent's edge by one rounding step and be
  refused.
- **Leave the decoders unpinned.** A lossy file's identity would change with an update that
  nobody chose.
