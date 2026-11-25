# 0051. The perspective view: a square view into a panorama, a mapping of `binding/1`

Status: accepted. Changes ADR 0001: a sixth kind of mapping in `binding/1`, as that record says a
new kind is added.
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
keeps the parameters and flips the direction, as for every kind.

### The view's pixels (`view`)

`view` cuts a perspective view of a complete panorama `W` pixels wide as a square of side
`round(W × fov / 360)`, held to 256 at least and 1024 at most, as squint's `view_side`
did. Each pixel `(u, v)` shows the panorama at the forward mapping of its centre,
`(u + 0.5, v + 0.5)`, sampled bilinearly between the centres of the four pixels around
it: columns taken round the seam, rows held to the first and the last. Each of the four
channels, red, green, blue and alpha, is sampled apart, and rounded to the nearest level,
a half away from zero. The view is bound onto the panorama by this mapping, stated from
the view. It is the arithmetic of the mapping, so the pixel a view shows and the point its
binding maps it to are one.

## Consequences

- `view` shows a panorama both ways, and each crop is bound onto its source: a crop by box
  through ADR 0001's crop, a perspective view through this kind.
- A store written before this record holds no binding of kind 5, so nothing stored
  changes, and nothing of ADR 0001 is re-hashed. A reader older than this record refuses
  a binding of kind 5 as "not a kind of mapping".
- Squint's stored view is reproduced to within two levels, the difference being the
  platform's functions against `libm`'s; whizbang's own view of it is the same on every
  machine, held by a pixel digest in the tests.
- A location is not carried through this kind until plan mE's task P1 carries one: the
  footprint of a box in a view is no box on the panorama, and may cross its seam or hold a
  pole. Until then `carry` refuses it, naming the mapping.
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
- **Sampling colours weighted by alpha.** The crop's area mean does not weight by alpha
  either; how a reduced crop treats transparency is decided once, for both, by plan mE's
  task P1 (its item 6).
