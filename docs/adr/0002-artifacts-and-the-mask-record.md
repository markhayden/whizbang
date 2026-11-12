# 0002. Artifacts: six types, one canonical file each, and the mask record

Status: accepted
Date: 2026-11-12
Spec: section 3, "Identities" and "Storage"; section 16, checkpoint 1A

## Context

`SPEC.md` section 3 keeps five invariants whatever the storage format becomes. Three are
about artifacts: a rendering cannot be used where a mask is required, every artifact records
what produced it, and a mask can be exported from one store and imported into another
without loss. It also says a mask belongs to a binding, never to a bare space, and that
using a mask on another image needs an explicit, checked rebind.

Rascal has the six types (`crates/rascal-core/src/artifact.rs`) and the mask record
(`mask.rs`, its ADR 0006). Three things in it cannot be kept:

- Its artifacts live in a frame, which has no content (ADR 0001), and are named in a
  workspace by local ids.
- Its identities are hashed from canonical JSON (its ADR 0016), which ADR 0001 turned down:
  the bytes depend on how a JSON library orders keys and prints numbers.
- A mask is a directory (`mask.toml`, three PNG planes, the profile's files) and travels
  between workspaces as a tar bundle with a manifest (its ADR 0022). The directory is not
  one thing a store can name, and the bundle is a second format beside it.

All are read at the commit pinned in `docs/sources.toml`.

## Decision

The types are in `crates/whizbang-core/src/artifact/`. The tests that hold them to this
record are in `crates/whizbang-core/tests/it/artifact/`.

### What every artifact has

- **A type,** one of six: `mask`, `location`, `image`, `band_profile`, `series`, `plan`.
- **A binding** (ADR 0001) that it belongs to. Its coordinates, rows and planes are those of
  the binding's source image. An image is the exception: it is what bindings name.
- **An identity,** hashed as ADR 0001 hashes every identity: the bytes `whizbang:`, the kind
  with its version (`mask/1`), a zero byte, then the fields this record lists for the type,
  in order, each as a tag and a value (`u`, `f`, `d`, `b`). A text is its UTF-8 bytes as
  `b`. A list is its count as `u` and then its items. A reference to a binding, to another
  artifact or to an image is that thing's identity as `d`. Nothing local enters: no id, no
  path, no name, no time.
- **An origin:** the steps that made it, oldest first. A step is the action's name, its
  version, its arguments as pairs of name and text, and the identities of its inputs by
  port. There is always at least one step, and a step always names its action; a file
  without one is refused. The origin is **not** part of the identity: the same artifact made
  twice, or held in two stores, is one artifact. The run that made the artifact, and the
  models it used, join the origin with the execution contract (`docs/carry-over.md`).
- **One canonical file.**

### The canonical file

One file holds one artifact whole. It is the identity's encoding with four differences,
each of which lets the file hold something the identity only names:

| Field | In the identity | In the file |
|---|---|---|
| A plane of a mask | `b` and the samples | `p`; the count of samples and the count of runs, each as eight bytes, big-endian; then each run as a value (one byte) and a length (four bytes, big-endian) |
| A file the artifact holds (an image's picture) | `d` and the digest of its bytes | `x`, its length as eight bytes, big-endian, and its bytes |
| A binding, or an artifact inside another | `d` and its identity | `a`, then its own file: its kind, its fields, and for an artifact its origin |
| The origin | nothing | `o`; `u` 1, the origin's version; the count of steps; for each step the action and the version as texts, the count of arguments and each as two texts, the count of inputs and each as a text and `d` |

So a file starts with `whizbang:`, the kind and a zero byte, holds the type's fields in
order, ends with the origin, and has nothing after it. A binding is written with the fields
of its identity (ADR 0001), under the kind `binding/1`.

A plane's runs are as long as they can be: a run ends where the value changes, or at
4,294,967,295 samples. Planes are large and mostly flat; as runs, the three planes of an
empty mask are 66 bytes, whatever its size.

**One way to write each artifact.** A file is read, checked as a new artifact is checked,
and then written again: unless that gives the same bytes, the file is refused. That one
rule refuses a run split in two, a zero with its sign set, and anything after the end.
Also refused: a number that is not finite; a kind or an origin version this build does not
read; a count the file cannot back (nothing is set aside for a count: reading runs out of
bytes instead); a plane over the limit on pixels, before any of it is read; an image whose
record does not describe the picture it holds.

**The file is not the identity.** The identity is hashed from the content, not from the
file's bytes. A change to how a file is written (another coding of planes, a longer
origin) is a new version of the file that leaves every identity as it was.

### The six types

Each list is the type's fields, in the order they are hashed and written.

**`image/1`.** The picture file, held whole and exactly as given (in the identity, the
digest of its bytes: ADR 0001's file digest); the canonical pixel digest (`d`); the space of
the upright picture: width, height, projection, topology (`u` each, as ADR 0001 numbers
them); what it is for (`u`): 0 a picture, 1 a rendering. The projection and topology are
stated by whoever makes the artifact; the size and both digests are taken from the file,
which is decoded within ADR 0001's limits when the artifact is made and again when it is
read. The same picture in two files is two artifacts with one pixel digest: bindings name
images by pixel digest, and an artifact names the bytes.

**`location/1`.** The binding; the shape's kind (`u`) and its parameters, in pixels of the
binding's source space:

| Kind | Parameters | Rule |
|---|---|---|
| 0 box | left, top, right, bottom (`f`) | Has area; inside the image |
| 1 polygon | the count, then x and y of each vertex (`f`) | 3 to 64 vertices, each inside the image |
| 2 view | yaw, pitch, field of view, in degrees (`f`) | The space is a complete panorama; yaw from -180 to 180, pitch from -90 to 90, field of view from 10 to 120 |
| 3 whole image | none | |
| 4 absent | none | Something missing from the image |
| 5 line | `u` 0 and a column (`f`), or `u` 1 and a path as a polygon's vertices are written; then the full width (`f`) | The column is inside the image; a path has at least 2 points, each inside; the width is positive and at most the image's diagonal |
| 6 stroke | a path; then the radius (`f`) | As a line's path and width |

Whether a polygon's edges cross is not checked, and nothing ties together the parts of a
shape that was split at a panorama's seam; both are on `docs/carry-over.md`.

**`mask/1`.** The mask record:

1. the binding;
2. what it was made for (`u`): 0 seam, 1 remove, 2 repair, 3 protect, 4 custom;
3. the three planes, each one byte a pixel, row by row from the top left, the size of the
   binding's source image: **core**, **blend**, **protect**;
4. the profile: `u` 0 for none, or `u` 1, the band profile and the feather's sigma (a
   series);
5. the prescriptions: for each, its kind (`u`; 0 is a tone ramp), and for a tone ramp its
   version (`u`, 1), its step (a series) and its share (`f`);
6. what it was made from: the identities of the locations (`d` each), then of the
   instances a segmenter found (`d` each).

The planes and the one rule for a pixel's weight are rascal's (its ADR 0006):

- Core is binary, 0 or 255: where a filler may paint, with a weight of 1.
- Blend holds the weights outside the core as fractions of 255. It is 0 inside the core
  and under protect.
- Protect is binary: where nothing may change.
- **The effective weight** of a pixel is 0 where protected, else 1 in the core, else its
  blend over 255. Everything that renders, refines or pastes through a mask uses that rule.
- Replacing the core clears the blend, the profile and every prescription, which were
  drawn from the old core; protect is kept and still wins. Adding to protect clears the
  core and the blend under it.
- The profile's band and sigma, and a tone ramp's step, belong to the mask's own binding.
  The sigma has one channel and an entry for every row; a step has three channels and an
  entry for every row; a share is from 0 to 1.

A prescription is data on the record, not pixels. Applying one, drawing a band, feathering
and rendering arrive with the operations of milestone 3.

**`band_profile/1`.** The binding; the identity of the line location the band follows
(`d`); the centre column (`u`); the least half-width (`f`, positive); the half-width of
every row (`f` each, one for each row of the image, none under the least); the sampling
windows: gap, step columns, texture columns (`u` each); the name of the set of constants
that produced it (text).

**`series/1`.** The binding; what was measured and its unit (texts); the channels (`u`, at
least 1); the rows (`u` each, ascending, inside the image); the values (`f` each, one for
each channel of each row, row by row).

**`plan/1`.** The binding of the panorama that is cut; the strip width (`u`). The space is
a complete panorama, and the strip is even, at least 2 and at most half the width: the rule
of ADR 0001's wrap, which is the mapping a plan describes. The canvas's left half holds the
panorama's last strip of columns, and its right half the first.

`band_profile`, `series` and `plan` have their type, their rules and their identity here.
What makes and reads them is milestone 3's.

### A type is never taken for another

A rendering of a mask (its planes as a grey picture, a tint over its image) is an `image`
whose role is rendering. It is refused where a mask is required at each place one could
arrive:

- **As a file.** Reading a file as a mask refuses a file of another kind, naming the kind,
  and refuses a PNG or a JPEG, saying that a picture is no artifact file.
- **From the store.** An artifact is asked for by type and identity. An identity the store
  holds under another type is refused as the wrong type; what is found is read as the type
  asked for, and must have the identity it is stored under.
- **In memory.** The six are six Rust types; the one value that can hold any of them gives
  up a mask only when it is one.

### What a mask carries, so that another store can read it

Everything, in its one file: the binding in full (both spaces, the destination, the
direction, the mapping and its parameters, not the binding's identity alone); the three
planes; the band profile and the sigma series, each whole, with its own binding and its own
origin; each prescription's series, whole; the identities of what it was made from; and
its origin. The locations and instances it was made from, and the images its binding
names, are named by identity and do not travel: a mask's planes are coordinates, and the
other store needs the image only to apply it.

Reading the file in another store needs nothing from the first. The mask read has the same
identity, and the store holds the same bytes.

### Where an artifact is stored

Under the name `artifacts/<type>/<the identity's 64 hex digits>`, as its canonical file.
`whizbang store import <file>` reads a canonical file of any type and publishes it under
that name; an artifact the store already holds is left as it is.

### A mask on another image: the checked rebind

A mask may be used on one image: its binding's source, by canonical pixel digest, in its
space. Any other image is refused, the same size or not.

To use it on an image derived from its own (the composite a fill made of it), it is
**rebound**. The rebind is given a binding, the *derivation*, that places the new image
onto the mask's image, and checks that:

- the derivation's destination is the mask's image;
- both of its spaces are the mask's space;
- its mapping is the identity, so that every pixel of the planes keeps its place;
- its source is not the mask's own image.

An image in its own space, an image placed onto some other image, a crop, a resize and a
homography are each refused. The rebound mask belongs to the derivation, so it names both
images. It has a new identity and the same planes; its profile and its prescriptions' series
move to the derivation with it; and its origin gains a step, `rebind`, that names the mask
it came from and the derivation.

What the rebind checks is the statement, not the pixels: a derivation is made by the action
that made the new image from the old, and says so. The rebind does not compare the two
pictures. A mask is often used on a composite that an earlier mask changed elsewhere, so
"unchanged outside this mask" is not a rule a rebind could hold. That protected pixels
never change is checked where a composite is made (the fill composite, task A8).

## Consequences

- A mask made on one image is refused on another of the same size, and on a picture of
  itself. Both were possible in rascal.
- The same mask has the same identity and the same bytes in every store, so export and
  import are a file copy and a read, and the bundle format is not needed.
- A file's bytes do not depend on a JSON library, an image encoder or a compressor. The
  price is that a file cannot be read by eye; `whizbang store import` says what a file is.
- An image artifact is decoded when it is read, to check its record against its picture.
  Reading a large image from the store costs a decode.
- An identity is worked out again after every change to a mask, which hashes its three
  planes.
- The identities are fixed by this record, not by the code: the vectors in
  `crates/whizbang-core/tests/it/artifact/vectors.rs` were computed apart from it.
- Reversing this costs every stored artifact. A store written under these kinds is
  discarded or migrated. A change to a type's fields is a new version of its kind and a
  record that supersedes this one; a new shape or prescription is a new number in its
  kind, and its own record.

## Alternatives

- **Keep rascal's mask directory and bundle.** Two formats for one thing, three encoders
  (TOML, JSON, PNG) whose output would have to be pinned for the bytes to be the same, and
  nothing the store can name as one entry.
- **A JSON record beside binary planes.** The record's bytes would depend on a JSON
  library's printing of numbers, and one of its features, turned on anywhere in the
  workspace, changes that.
- **Hash the file.** The identity would change with the coding of planes and with the
  origin, so the same mask made twice would be two masks.
- **Planes as PNG, or deflated.** The bytes would depend on a compressor's version, and
  the rule that a file is written one way would refuse every older file after an update.
- **Planes as plain bytes in the file.** A mask of a 50-megapixel image would be 150 MB
  whatever it held.
- **Put the origin in the identity.** Two stores could not agree on a mask that two runs
  made.
- **Name the profile's band and series by identity, and store them apart.** A mask could
  be read only where they also were, and an export would be several files.
- **A rebind that compares the two pictures outside the mask.** It refuses the ordinary
  case of a second mask used after a first, and it makes the rebind need both pictures
  decoded.
- **A rebind that takes only the new image.** Nothing would say the image is derived from
  the mask's, and an unrelated image of the same size would pass.
