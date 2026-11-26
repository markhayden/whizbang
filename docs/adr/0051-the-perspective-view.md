# 0051. The perspective view: a square view into a panorama, a mapping of `binding/1`

Status: accepted. Changes ADR 0001: a sixth kind of mapping in `binding/1`, as that record says a
new kind is added; and the slack ADR 0001 gives a computed point past an edge, scaled by a
crop's magnification. Amended 2026-11-25, plan mE, task P1's second pull request: a view's
colours weighted by alpha; locations carried through this kind and the slack scaled
Date: 2026-11-25
Spec: section 3, "Identities"; section 8, the table of kinds (`view` is the control);
plan mE, task P1, items 1 and 2

## Context

Squint's `view` showed a panorama two ways: a crop by box, and a square perspective view
by yaw, pitch and field of view (squint `crates/squint-tools/src/builtin/view.rs` and
`crates/squint-core/src/frames/sphere.rs`, at the commit `docs/sources.toml` pins).
Whizbang's `view` refused every panorama until this record, because a crop is an image
bound onto its source (ADR 0001), and none of ADR 0001's five mappings places a
perspective view on a panorama: a crop is affine, a rotation runs between two panoramas,
and a wrap holds whole columns. ADR 0001 says such a mapping is "a new kind number in
`binding/1`, and its own record".

Two things bind the choice. A binding's identity hashes its parameters, and a point mapped
through it may enter a record whose bytes are hashed, so the arithmetic is the same on every
machine (ADR 0001, "Points are not hashed"). And the view's pixels are a picture's
identity, so the sample at each pixel must be the same on every machine too: squint used
the platform's sine, cosine and arc functions, whose last bits may differ from one
platform to another.

## Decision

### The mapping

Kind 5, **perspective**: three parameters, `yaw`, `pitch` and `fov`, each `f`, in degrees,
in that order. `from` is the view and `to` the panorama.

**Needs.** `from` is flat and square. `to` is a complete panorama: equirectangular and
wrapped. Each parameter is finite; `-180 <= yaw < 180`, `-90 <= pitch <= 90`, and
`0 < fov < 180`. Anything else is refused when the binding is built and when it is read.

**Meaning.** In radians, with `y` for yaw and `p` for pitch, the view looks along
`forward = (cos p cos y, cos p sin y, sin p)`; `right = (-sin y, cos y, 0)` and
`up = (-sin p cos y, -sin p sin y, cos p)`; and `t = tan(fov / 2)`. Yaw 0 looks at the
panorama's middle column and grows to the right; pitch 0 is the horizon and grows upward;
`fov` is the view's field of view across, and down, since the view is square.

- **Forward,** a point `(x, y)` of a view `s` pixels square: `cx = (2 x / s - 1) t` and
  `cy = (1 - 2 y / s) t`; the direction is `forward + cx right + cy up`, divided by its
  length; its longitude is `atan2` of its second and first components and its latitude the
  `asin` of its third, held to `[-1, 1]`; the point is
  `((lon + π) / 2π × W, (π/2 - lat) / π × H)` in a panorama `W` by `H`, its column taken
  round as a wrapped space takes it (ADR 0001, "Coordinate space").
- **Backward,** a point of the panorama: its direction, as ADR 0001 defines it; its depth,
  the direction's dot product with `forward`. At a depth of at most `1e-9` the point is
  behind the view and has no place in it. Otherwise `cx` is the dot product with `right`
  divided by the depth and by `t`, `cy` the same with `up`, and the point is
  `((cx + 1) / 2 × s, (1 - cy) / 2 × s)`; one that lands outside the view is refused, as
  every point that lands outside its space is.

The sine, cosine, tangent, `atan2` and `asin` are `libm`'s, and the square root is the
one IEEE 754 defines, so the same point maps to the same bits on every machine.

**Composition.** A perspective view composes with the identity and with nothing else: the
view is not affine, so a crop of a view, or a view of a rotated panorama, has its points
mapped through each binding in turn, as a wrap does (ADR 0001). The reverse of a binding
keeps the parameters and flips the direction, as for every kind. `box_to_root` takes
crops alone, and refuses a chain that holds this kind; `box_onto_panorama` takes a chain
of crops that ends in it.

**Locations.** A location is carried through this kind as its footprint (`carry`):

- from the view onto the panorama, a box, a polygon or the whole view is sampled along its
  boundary, a box at 60 points an edge and a polygon's edges divided evenly to at most 256
  points, of which every fourth, a box's corners and a polygon's vertices among them, is
  kept for the outline: 15 points an edge of a box, and each point mapped forward. When the shape holds a pole, the pole the view
  sees mapped back into the shape, the footprint is a band the panorama's full width from
  that pole to the farthest point the boundary reaches, and the whole panorama when it
  holds both. Otherwise each point is taken round the seam to within half the width of
  the one before, and a step still over a quarter of the width is refused, since which
  way round it went is not known. Near a pole a step that short in angle can be long in
  longitude, so a boundary that passes within about half a degree of a pole it does not
  hold can be refused. The ring is then clipped to the panorama once for each
  whole turn of the width it reaches into, so a part on either side of the seam, or on
  both, is kept. Where the ring crosses a column, the pieces between the columns are joined
  along it only to the next piece in the way the ring turns, so each part is a simple
  polygon. A part over 64 vertices is thinned evenly to 64;
- from the panorama into the view, a box is sampled as above and a polygon by its
  vertices, each made a direction; the ring of directions is clipped to what lies in
  front of the view (a depth of at least `1e-6`), so an edge that passes behind it ends
  where it leaves the view's half of the sphere, and is not drawn as a chord; what is
  left is seen in the view and clipped to the view's square. The result is the box around
  it for a box and the polygon for a polygon, partial when anything was clipped. The whole
  panorama is the whole view;
- a line, a stroke and a view are refused: their width, and a view's angles, have no one
  value through it.

A rotation carries a box sampled as above, and a polygon by its vertices, each mapped as
it always was, onto the same footprint: cut at the seam, or a band at a pole the shape
holds. Which way round the panorama each vertex lies is read from the polygon's edges,
sampled as a box's are, so a polygon wider than half the panorama is taken round the way
its edges go, and one whose edge leaps over a quarter of the width between two samples is
refused.

### The slack past an edge

ADR 0001 brings a computed point back onto an edge of a bounded space when it is at most
`1e-9` of a pixel past it. A point that comes through a crop that magnifies, into the
crop, carries the rounding of the space it came from times that magnification, which can be
far more. So the slack a mapping gives a point where it lands is `1e-9` times how many
pixels there one pixel where it came from becomes, never less than `1e-9` and never more
than `1e-3` (a thousandth of a pixel where it lands): for a crop
toward its picture, the larger of the picture's width over the box's and its height over
the box's; toward the box, the inverse; 1 for every other kind. A point on an edge of a
crop of a crop magnified 2.8 million times, mapped up to the root and back, then lands
(`space::a_point_on_the_edge_of_a_magnified_crop_of_a_crop_lands`).

### The view's pixels (`view`)

`view` takes yaw 180 as -180, the seam, which is how the mapping states it. It cuts a
perspective view of a complete panorama `W` pixels wide as a square of side
`round(W × fov / 360)`, held to 256 at least and 1024 at most, as squint's `view_side`
did. Each pixel `(u, v)` shows the panorama at the forward mapping of its centre,
`(u + 0.5, v + 0.5)`, sampled bilinearly between the centres of the four pixels around
it: columns taken round the seam, rows held to the first and the last. Alpha is the blend
of the four alphas; each colour the blend of the four weighted by alpha as well, divided
by the blended alpha, or the plain blend where the four alphas are equal or all 0. The
plain blend is taken across, then down, as squint's `sample` did, and alpha is always the
plain blend, so an opaque view has the pixels it had before alpha was weighed, to the
bit. Each is rounded to the nearest level, a half away from zero. The view is bound onto the panorama by this mapping, stated from
the view. It is the arithmetic of the mapping, so the pixel a view shows and the point its
binding maps it to are one.

## Consequences

- `view` shows a panorama both ways, and each crop is bound onto its source: a crop by box
  through ADR 0001's crop, a perspective view through this kind.
- A store written before this record holds no binding of kind 5, so nothing stored
  changes, and nothing of ADR 0001 is re-hashed. A reader older than this record refuses
  a binding of kind 5 as "not a kind of mapping".
- Squint's stored view is reproduced sample for sample, though squint's functions were the
  platform's (its own test allowed two levels); whizbang's view of it is the same on every
  machine, held by a pixel digest in the tests.
- A location in a view lands on its panorama as a polygon, or two at the seam, or a band
  at a pole, not as the kind of shape it was: a caller that wants a box takes the box
  around a part.
- Reversing this costs every stored perspective view's binding.

## Alternatives

- **A view of any shape,** with a field of view across and another down. Squint's views
  were square, and nothing asks for another shape.
- **The view's rotation as a matrix, and the field of view.** Ten numbers where three do,
  and a roll that no caller sets.
- **The platform's functions,** as squint used. A view's pixels, and a mapped point, could
  then differ in their last bits between macOS and Linux.
- **Normalised coordinates,** as squint stated the mapping (`[0, 1]` across the view).
  Every other mapping of ADR 0001 is in pixels.
- **Sampling the four channels apart,** as squint did and the first pull request of plan
  mE's task P1 did. A colour hidden under alpha 0 then shows at the edge of a transparent
  area; the area mean of a reduced crop is weighted the same way (`view`'s record,
  "Alternatives").
- **Squint's footprint at the seam,** cut to the side that holds the region's centre and
  flagged, with the other side kept apart. Here both sides are parts of one carry.
- **A fixed slack.** It refuses a point on the edge of a crop magnified a million times
  that the arithmetic put a millionth of a pixel past it.
