# 0002. Artifacts: six types, one canonical file each, and the mask record

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, inside checkpoint 1A, after the reviews of the code that came with it
and of what was built on it. Added: what the rebind checks and what it trusts, with the
derivation as a type of its own; "Limits and memory", and that `whizbang store import`
refuses a file over 65 MiB; a location as a statement, and mapped coordinates and `libm`;
the counts in the mask's prescriptions and sources; that a run passing the end of its plane
and a binding that breaks ADR 0001's rules are refused. Changed: the core is 0 under
protect, which narrows `mask/1` without a new version, as the record says where it states
it. Added to `image/1`: what a change of the encoder that writes an image does to its
identity.
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
4,294,967,295 samples, the most its four bytes can say. Planes are large and mostly flat; as
runs, the three planes of an empty mask are 66 bytes at any size up to 4,294,967,295
samples, which is far over the limit on pixels (below). A plane over that would take one
more run of five bytes for each 4,294,967,295 samples.

**One way to write each artifact.** A file is read, checked as a new artifact is checked,
and then written again: unless that gives the same bytes, the file is refused. That one
rule refuses a run split in two, a zero with its sign set, and anything after the end.
Also refused: a number that is not finite; a kind or an origin version this build does not
read; a count the file cannot back (nothing is set aside for a count: reading runs out of
bytes instead); a plane over the limit on pixels, before any of it is read; a run that
passes the end of its plane, before anything is set aside for it; a binding inside the file
that breaks one of ADR 0001's rules, which is checked as a new binding is; an image whose
record does not describe the picture it holds.

**The file is not the identity.** The identity is hashed from the content, not from the
file's bytes. A change to how a file is written (another coding of planes, a longer
origin) is a new version of the file that leaves every identity as it was.

**Limits and memory.** A small file can stand for a large artifact, so what reading costs
is bounded by the artifact, not by the file:

- A mask's planes are held to ADR 0001's limit on pixels, 50,000,000 unless the caller
  changes it: the width times the height of its binding's source space, checked before any
  plane is read.
- A mask in memory is its three planes, one byte a pixel: 150 MB at the limit, whatever
  its file holds. An empty mask of 50,000,000 pixels is a file of under a kilobyte and
  150 MB once read.
- Each plane is set aside whole when its size has been read, before its runs. A file that
  is refused later (a run that does not fit, a broken invariant, a file not written the one
  way) can have cost as much as the mask it claimed to be: up to 150 MB before the refusal.
- Reading writes the file again to compare it, which costs the file's size once more.
  Working out the identity copies nothing. A rebind holds both masks while both are kept.
- Everything else a file holds (half-widths, rows, values, points, an origin) is set aside
  only as its bytes are read, so it costs no more than a small multiple of the file's size.
  An image is held to ADR 0001's limits.

Nothing here limits the size of the file itself; whoever reads the bytes does. The first
such reader is `whizbang store import`, which refuses a file over 65 MiB before it reads
it. A tighter bound for masks is on `docs/carry-over.md`.

### The six types

Each list is the type's fields, in the order they are hashed and written.

**`image/1`.** The picture file, held whole and exactly as given (in the identity, the
digest of its bytes: ADR 0001's file digest); the canonical pixel digest (`d`); the space of
the upright picture: width, height, projection, topology (`u` each, as ADR 0001 numbers
them); what it is for (`u`): 0 a picture, 1 a rendering. The projection and topology are
stated by whoever makes the artifact; the size and both digests are taken from the file,
which is decoded within ADR 0001's limits when the artifact is made and again when it is
read. The same picture in two files is two artifacts with one pixel digest: bindings name
images by pixel digest, and an artifact names the bytes. So a change of the encoder that
writes an image a tool makes, giving another file for the same pixels, changes that
artifact's identity through its file digest and leaves its pixel digest and every binding
that names it as they were, as ADR 0001 says of the JPEG decoder and the pixel digest;
what writes a crop is pinned for that reason (`docs/reference/tools/view.md`).

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

"Inside the image" includes its edges: `x` from 0 to the width and `y` from 0 to the
height, as ADR 0001's coordinates run.

**A location is a statement, as a binding is.** Its identity holds the coordinates as they
were given. In a wrapped space the right edge is column 0 (ADR 0001), so a column, a point
or a box's edge at `x` equal to the width and the same one at `x` equal to 0 are one place
with two identities, and both are accepted. Nothing is brought round before hashing: a box
whose right edge is the width would lose its area, and a polygon would turn inside out.
Two equal location identities are the same location; two different ones may still be the
same place. As ADR 0001 says of bindings, later code must never take two different
identities to mean two different places.

**Mapped coordinates and `libm`.** A location's coordinates are hashed. ADR 0001 requires
that a mapped point that enters hashed bytes be computed with `libm`, because the machine's
own sine, cosine and arc functions may differ in their last bits. Nothing maps a location
yet. Whatever first carries a location through a rotation, or through any mapping that
uses those functions, computes its points with `libm`, so that the mapped location has the
same identity on every machine; that is on `docs/carry-over.md`.

**`mask/1`.** The mask record:

1. the binding;
2. what it was made for (`u`): 0 seam, 1 remove, 2 repair, 3 protect, 4 custom;
3. the three planes, each one byte a pixel, row by row from the top left, the size of the
   binding's source image: **core**, **blend**, **protect**;
4. the profile: `u` 0 for none, or `u` 1, the band profile and the feather's sigma (a
   series);
5. the prescriptions: their count (`u`), then for each its kind (`u`; 0 is a tone ramp),
   and for a tone ramp its version (`u`, 1), its step (a series) and its share (`f`);
6. what it was made from: the count of locations (`u`) and the identity of each (`d`), then
   the count of instances a segmenter found (`u`) and the identity of each (`d`).

The planes and the one rule for a pixel's weight are rascal's (its ADR 0006):

- Core is binary, 0 or 255: where a filler may paint, with a weight of 1. It is 0 under
  protect.
- Blend holds the weights outside the core as fractions of 255. It is 0 inside the core
  and under protect.
- Protect is binary: where nothing may change.
- **The effective weight** of a pixel is 0 where protected, else 1 in the core, else its
  blend over 255. Everything that renders, refines or pastes through a mask uses that rule.
- Replacing the core clears the blend, the profile and every prescription, which were
  drawn from the old core; protect is kept and still wins: the new core is cleared under
  it. Adding to protect clears the core and the blend under it.
- So the core is never set under protect: replacing the core and adding to protect give
  the same mask, with the same identity, in either order, and a file whose core is not 0
  under protect is refused. Rascal's mask type kept a replaced core under protect, where
  its weight was 0 all the same; its operations cleared it there themselves before every
  replacement (`replaced_core` in `crates/rascal-ops/src/refine.rs`), and the type now
  does.
- The planes are still a statement, and not the weights alone. A weight of 1 can be stated
  as core, or as a blend of 255 with no core: the two masks weigh every pixel alike, both
  are accepted, and they have two identities. Equal identities mean the same mask, and not
  the reverse, as with a binding (ADR 0001) and a location.
- This narrows `mask/1` without a new version. A mask file that the code merged with task
  A4 wrote after replacing a core over protect is refused by this code, and the same calls
  now give a mask with another identity. The version stays at 1 because the fields and
  their encoding are as they were, every mask this code accepts has the identity it had,
  and nothing has shipped: a store written by earlier code of checkpoint 1A is discarded.
  Once a store is kept, a narrowing like this one is a new version of the kind.
- The profile's band and sigma, and a tone ramp's step, belong to the mask's own binding.
  The sigma has one channel and an entry for every row; a step has three channels and an
  entry for every row; a share is from 0 to 1.

A prescription is data on the record, not pixels. Applying one, drawing a band, feathering
and rendering arrive with the operations of milestone 3.

**`band_profile/1`.** The binding; the identity of the line location the band follows
(`d`); the centre column (`u`), from 0 to the width: it is a place between columns, as an
`x` is, so the right edge is allowed; the least half-width (`f`, positive); the count of
half-widths (`u`), which is the count of the image's rows, and then the half-width of each
row (`f` each), none more than 1e-9 under the least, the tolerance rascal's profile
allowed; the sampling windows: gap, step columns, texture columns (`u` each); the name of
the set of constants that produced it (text).

**`series/1`.** The binding; what was measured and its unit (texts); the channels (`u`, at
least 1); the count of rows (`u`) and then each row (`u` each, ascending, inside the
image); the count of values (`u`), which is the rows times the channels, and then each
value (`f` each, one for each channel of each row, row by row).

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
`whizbang store import <file> --store <dir>` reads a canonical file of any type and
publishes it under that name; an artifact the store already holds is left as it is.

### A mask on another image: the checked rebind

A mask may be used on one image: its binding's source, by canonical pixel digest, in its
space. Any other image is refused, the same size or not.

To use it on an image derived from its own (the composite a fill made of it), it is
**rebound**. The rebind is given a *derivation*: the statement that a new image was made
from another, pixel for pixel in place. A derivation holds a binding that places the new
image onto the one it was made from, and it is a type of its own (`Derivation`): the
rebind takes nothing else, so a binding that an alignment or a crop returned is not taken
for one by accident. Stating a derivation checks the binding, and the rebind checks it
against the mask:

- the derivation's destination is an image, and for the rebind it is the mask's image;
- its mapping is the identity, so that every pixel of the planes keeps its place;
- both of its spaces are the mask's space: an identity has two equal spaces (ADR 0001), so
  a space of as many samples that is not the mask's (6 by 8 for 8 by 6, a panorama of the
  same size) is refused;
- its source is not the image it is placed onto.

An image in its own space, an image placed onto some other image, a crop, a resize and a
homography are each refused. The rebound mask belongs to the derivation's binding, so it
names both images. It has a new identity and the same planes; its profile and its
prescriptions' series move to that binding with it; and its origin gains a step, `rebind`,
whose inputs are the identity of the mask it came from (`mask`) and the identity of the
derivation's binding (`derivation`).

**What the rebind checks, and what it trusts.** It checks the four things above, which are
all about the statement. It trusts that the statement is true: that the new image really
was made from the mask's image with every pixel in its place. A derivation's binding has
the same identity as an identity binding between two images of one size that anyone can
build (ADR 0001 keeps `binding/1` as it is, and to ADR 0001 such a binding is an ordinary
alignment), and stating a derivation is open to any caller. So a caller who states a false
derivation can move a mask onto an unrelated image of the same size. The type stops that
happening by mistake; it does not stop it being done on purpose. This boundary is accepted
here, and closed where derivations are made: only the executor or the fill composite will
state one, and a derivation stated by hand is refused there (tasks A5 and A8, on
`docs/carry-over.md`). A rebound mask read from a file is a mask on a binding like any
other: the file does not say whether its binding was a derivation.

Nothing at this layer can check more. A mask and a binding hold digests of pictures, not
pictures, and two digests say nothing of how alike two pictures are. Comparing the pixels
is not the check either, even where both pictures are at hand: a derived image differs
from its parent by design, inside whatever mask the fill used, and a mask is often used on
a composite that an earlier mask changed elsewhere, so "unchanged outside this mask" is
not a rule a rebind could hold; and a comparison would make every rebind decode two
pictures.

**Protected pixels never change, on the new image as on the old.** That rule does not rest
on the rebind. It will be held where a composite is made (the fill composite, task A8, which
does not exist yet), against the mask it is given and the image it is given, whichever
they are: every protected pixel and every pixel outside the support comes out exactly as
it went in. A mask rebound onto a
composite protects the same places of the composite, and the next composite keeps them. A
false derivation moves the planes onto the wrong picture, where they protect the wrong
things; it does not let a protected pixel of that picture change.

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
  kind, and its own record. So is a rule that refuses what an earlier build wrote, once a
  store is kept; the one such narrowing made before that, the core under protect, is
  stated with the mask record above.

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
- **Make a derivation a kind of binding, with its own number in `binding/1`.** It would
  say in the identity that the image was derived, and it would change what ADR 0001 hashes
  for a statement that places exactly as the identity does. The statement is kept apart
  from the placement instead: a type in the code, and no new bytes.
- **Bring a coordinate at a wrapped space's right edge round to 0 before hashing.** One
  place would have one identity, for a point alone; a box and a polygon cannot be brought
  round without changing their shape, so the rule would hold for some shapes and not for
  others.
- **Keep a replaced core under protect,** as rascal's mask type did. The same core and the
  same protect, applied in the two orders, would be two masks with two identities, for a
  difference nothing reads; and every operation would have to clear the core itself, as
  rascal's did.
