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
  be read, or is outside 1 to 8 is no tag.
- **Channel order.** Four bytes a pixel, in the order red, green, blue, alpha, row by row
  from the top left. A grey image has three equal colour channels.
- **Depth.** Eight bits a channel. A 16-bit sample `v` becomes `(v * 255 + 32767) / 65535`
  in whole numbers: the nearest 8-bit value. Floating-point samples are refused.
- **Alpha.** Straight, never multiplied into the colours. An image without an alpha channel
  is opaque (255), so an opaque alpha channel and no alpha channel are the same picture. The
  colour under a transparent pixel is kept as stored, and is part of the picture.
- **Colour.** Canonical pixels are sRGB. An image with no embedded ICC profile is taken as
  sRGB, and its samples are kept. An image whose profile is sRGB keeps its samples too. An
  image with any other RGB matrix profile is converted to sRGB. A profile of any other kind
  (one that works through lookup tables alone, a grey or CMYK profile, a profile on a grey
  image, a damaged profile) has no canonical reading: the image is refused, never guessed.

A profile is read as an RGB matrix profile when its data is RGB, its connection space is
XYZ, and it has the six tags `rXYZ`, `gXYZ`, `bXYZ`, `rTRC`, `gTRC` and `bTRC`; other tags
are ignored. A curve is a table (no entry is the identity, one entry is a power with eight
fractional bits, more are joined by straight lines) or one of the five parametric forms of
the ICC specification.

A profile **is sRGB** when each of its nine colorant values is within 0.001 of sRGB's, and
each channel's curve is within 0.001 of the sRGB curve at each of the 256 samples. sRGB's
colorants are those of its ICC profile, as fractions of 65536: red 28578, 14581, 912; green
25241, 46981, 6362; blue 9376, 3972, 46799.

The **conversion**, for one pixel, after the samples are 8 bits and before the picture is
turned upright:

1. Each colour sample, as a fraction of 255, goes through its channel's curve to linear
   light, clipped to 0 to 1.
2. The three values are multiplied by the matrix whose columns are the profile's colorants,
   then by the inverse of the matrix whose columns are sRGB's.
3. Each result is clipped to 0 to 1, encoded with the sRGB curve (`12.92 v` up to
   0.0031308, else `1.055 v^(1/2.4) - 0.055`), multiplied by 255 and rounded half up.

Alpha is not touched. The arithmetic is IEEE 754 double precision and every power is
computed by the `libm` crate, not by the machine's own library, so the bytes are the same
on every machine.

Which containers can be decoded is not part of the identity. The core decodes PNG and JPEG,
which is what the old tools read. A lossy file has canonical pixels only as far as one
decoder goes: two decoders may disagree on its samples, and the digest then differs.

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
  Frobenius norm.
- Every mapping runs both ways. A point outside the space it is given in is refused, and so
  is a point that lands outside the space it maps to: a point of an image that a crop does
  not cover has no place in the crop.
- The reverse of a binding swaps its two ends, keeps the parameters and flips the direction.
  Nothing is recomputed, and it maps as the inverse.
- Two bindings compose into one when the second starts where the first ends: two crops,
  each stated from its source, into a crop; any other pair of crops and homographies into a
  homography; rotations into a rotation. A wrap does not
  compose with another mapping; its points are mapped through each binding in turn.
- A homography's parameters are hashed as given. Two scalings of one matrix map alike and
  are two bindings.

## Consequences

- A mask made on one image cannot be mistaken for a mask on another of the same size, which
  rascal's frames allowed. Using it on another image is an explicit rebind, which arrives
  with masks.
- A copy of a picture in another container, at another depth, under another orientation
  tag, or with an opaque alpha channel added, is recognised. A lossy recompression is not,
  and nothing here tries to recognise one.
- The same samples under two different profiles are two pictures, which squint's digest did
  not see.
- An image whose profile is not an RGB matrix profile is refused until a later record gives
  it a canonical reading. Colour information a PNG carries outside an ICC profile (its
  gamma, chromaticity and coding-independent chunks) is not read, and such a file is taken
  as sRGB. Both are on `docs/carry-over.md`.
- Pictures deeper than 8 bits lose depth in their identity: two 16-bit pictures that differ
  only below 8 bits share a pixel digest. The tools work on 8-bit pixels.
- The pixel digest is fixed by this record, not by a library: the vectors in
  `crates/whizbang-core/tests/it/identity/vectors_table.rs` are files with the digests they
  must have, and the conversion's expected values were computed apart from the code.
- The core depends on the `image` crate to decode, `sha2` to hash and `libm` for powers.
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
