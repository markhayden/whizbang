"""Masks on a working grid, for the tool files that compare shapes and changes (plan mE, task
T1): `pack/tools/outline.py` and `pack/tools/regions.py`. Ported from squint's
`tools/python/src/squint_tools/regions.py` (`components`) and `outline.py` (the masks'
filters) at the commit `docs/sources.toml` pins, and written for the standard library: a mask
is a Python integer of one bit a pixel, the first pixel (top left) its highest bit, row by
row, so that an erosion or a dilation by a square of three is four shifts, the frame's edge
repeated past it, as the image library squint used does.

A tool file reaches it as `from whizbang_pack.masks import ...`: the tool worker runs from the
pack this module is part of."""

import re

_BITS = bytes([0x30]) + bytes([0x31]) * 255
_RUNS = re.compile("1+")


class Grid:
    """The working grid: `w` by `h` pixels, a mask on it a Python integer of `w * h` bits,
    the first pixel (top left) its highest bit, row by row."""

    def __init__(self, width, height):
        self.w, self.h = width, height
        self.n = n = width * height
        self.full = (1 << n) - 1
        self.col0 = int(("1" + "0" * (width - 1)) * height, 2)
        self.coll = int(("0" * (width - 1) + "1") * height, 2)
        self.row0 = int("1" * width + "0" * (n - width), 2)
        self.rowl = int("0" * (n - width) + "1" * width, 2)
        if width > 4 and height > 4:
            row = "00" + "1" * (width - 4) + "00"
            self.inner = int("0" * (2 * width) + row * (height - 4) + "0" * (2 * width), 2)
        else:
            self.inner = 0

    def text(self, m):
        return format(m, "0%db" % self.n)

    def mask(self, value, width, height):
        """A mask of `width` by `height` bits brought to the grid by its nearest pixel: each
        working pixel takes the pixel under its centre."""
        if (width, height) == (self.w, self.h):
            return value
        text = format(value, "0%db" % (width * height))
        xs = [nearest_index(x, width, self.w) for x in range(self.w)]
        made, rows = {}, []
        for y in range(self.h):
            sy = nearest_index(y, height, self.h)
            row = made.get(sy)
            if row is None:
                line = text[sy * width : (sy + 1) * width]
                row = "".join([line[x] for x in xs]) if "1" in line else "0" * self.w
                made[sy] = row
            rows.append(row)
        return int("".join(rows), 2)

    # What is at the neighbour on each side, brought to each pixel; past the frame's edge
    # the edge itself.
    def _left(self, m):
        return ((m >> 1) & ~self.col0) | (m & self.col0)

    def _right(self, m):
        return ((m << 1) & self.full & ~self.coll) | (m & self.coll)

    def _up(self, m):
        return (m >> self.w) | (m & self.row0)

    def _down(self, m):
        return ((m << self.w) & self.full) | (m & self.rowl)

    def eroded(self, m):
        """The mask's minimum over a square of three."""
        across = m & self._left(m) & self._right(m)
        return across & self._up(across) & self._down(across)

    def dilated(self, m):
        """The mask's maximum over a square of three."""
        across = m | self._left(m) | self._right(m)
        return across | self._up(across) | self._down(across)

    def opened(self, m, side):
        """Eroded, then dilated, by a square of `side` (odd)."""
        for _ in range(side // 2):
            m = self.eroded(m)
        for _ in range(side // 2):
            m = self.dilated(m)
        return m

    def boundary(self, m):
        """The mask's own edge pixels, without those within two of the frame's edge: a shape
        running off the picture has no outline there."""
        return m & ~self.eroded(m) & self.inner

    def of_runs(self, runs):
        """The mask of these runs, each (y, x0, x1)."""
        if not runs:
            return 0
        made = bytearray(b"0") * self.n
        for y, x0, x1 in runs:
            made[y * self.w + x0 : y * self.w + x1] = b"1" * (x1 - x0)
        return int(made, 2)


def components(grid, m):
    """The 4-connected pieces of a mask, by the runs of each row and a union of them: each
    piece `[pixels, x0, y0, x1, y1, runs]`, its box's right and bottom out; and for each row
    its runs, each `(x0, x1, piece)`."""
    text, w = grid.text(m), grid.w
    parent = []

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    rows, previous = [], []
    for y in range(grid.h):
        row = text[y * w : (y + 1) * w]
        current = []
        if "1" in row:
            j = 0
            for found in _RUNS.finditer(row):
                x0, x1 = found.span()
                label = len(parent)
                parent.append(label)
                while j < len(previous) and previous[j][1] <= x0:
                    j += 1
                k = j
                while k < len(previous) and previous[k][0] < x1:
                    a, b = find(label), find(previous[k][2])
                    if a != b:
                        parent[max(a, b)] = min(a, b)
                    k += 1
                current.append((x0, x1, label))
        rows.append(current)
        previous = current
    pieces, index = [], {}
    for y, current in enumerate(rows):
        named = []
        for x0, x1, label in current:
            root = find(label)
            if root not in index:
                index[root] = len(pieces)
                pieces.append([0, x0, y, x1, y + 1, []])
            piece = pieces[index[root]]
            piece[0] += x1 - x0
            piece[1], piece[3], piece[4] = min(piece[1], x0), max(piece[3], x1), y + 1
            piece[5].append((y, x0, x1))
            named.append((x0, x1, index[root]))
        rows[y] = named
    return pieces, rows


def nearest_index(i, size, to):
    """The index of `size` under the centre of index `i` of `to`: nearest-pixel resampling."""
    return min(size - 1, int((i + 0.5) * size / to))


def one_grid(source, candidate, aspect):
    """Refuses two pictures whose shapes differ by more than `aspect`: they are not one grid."""
    if abs((candidate.width * source.height) / (candidate.height * source.width) - 1.0) > aspect:
        raise ValueError("the source and the candidate are not one grid: register the candidate onto the source first")


def mask_of(context, image, phrase, most, model=None):
    """The union of what the segmenter finds for a phrase in the named picture: a mask of
    its own size, as `(value, width, height)`. Each instance's mask is of the size the
    instance states, which must be the picture's: a mask of another size is refused rather
    than read out of line. `model`, when given, is the model of the catalogue asked."""
    shown = context.image(image)
    value = 0
    asked = {"model": model} if model is not None else {}
    for instance in context.segment(image, phrase=phrase, most=most, **asked):
        if (instance.width, instance.height) != (shown.width, shown.height) or len(instance.mask) != shown.width * shown.height:
            raise ValueError("the segmenter answered a mask of another size than the picture's")
        value |= int(instance.mask.translate(_BITS), 2)
    return value, shown.width, shown.height


def printable(text, most):
    """The text as printable ASCII, every other character a space, cut to `most`."""
    return "".join(c if " " <= c <= "~" else " " for c in text)[:most]
