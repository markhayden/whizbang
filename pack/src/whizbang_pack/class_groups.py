"""The classes of the fixed-class segmenter as the tool files read them (plan mE, task T2,
items 4 and 5): which ADE20K classes it confuses with one another, and the words for
structures it has no class of its own for. Squint's `outline.py` (`GROUPS`, `RELABEL`,
`ALIASES`, `related`), at the commit `docs/sources.toml` pins, shared by `outline`,
`regions` and `classes` as `whizbang_pack.subjects` is.
"""

from whizbang_pack.eomt_ade20k import CLASSES

# The model a tool asks for a class, by its name in the catalogue.
MODEL = "eomt-ade20k"

# Classes the segmenter reads as one another when the light or the season changes: a change
# between two of one group is a relabelling, not a change of the picture, and scores
# `RELABEL` of what it would.
GROUPS = [
    {"building", "house", "skyscraper", "tower", "hovel", "wall", "column", "tank", "awning"},
    {"floor", "road", "sidewalk", "earth", "path", "land", "sand", "field", "grass", "dirt track", "step", "stairs", "stairway", "rug"},
    {"tree", "plant", "grass", "flower", "palm", "field"},
    {"water", "sea", "river", "lake", "swimming pool", "waterfall"},
    {"sky", "mountain", "hill", "ceiling"},
    {"mountain", "hill", "rock", "earth", "land"},
    {"windowpane", "door", "screen door", "glass", "mirror", "painting", "poster", "wall"},
    {"awning", "canopy", "tent", "booth"},
    {"car", "truck", "van", "bus", "minibike"},
    {"boat", "ship"},
    {"pole", "streetlight", "signboard", "traffic light", "flag"},
    {"signboard", "trade name", "poster", "bulletin board"},
    {"railing", "bannister", "fence", "bridge", "pier", "sidewalk", "stairs", "stairway"},
    {"cabinet", "shelf", "counter", "countertop", "case", "bookcase", "buffet", "box"},
]
RELABEL = 0.35

# Words for structures the segmenter has no class of its own for, and the class that covers
# them.
ALIASES = {
    "barn": "building",
    "silo": "building",
    "chimney": "building",
    "cottage": "house",
    "shed": "hovel",
    "hut": "hovel",
    "cabana": "hovel",
    "deck": "floor",
    "patio": "floor",
    "terrace": "floor",
    "pool": "swimming pool",
    "window": "windowpane",
    "lamp post": "streetlight",
    "street lamp": "streetlight",
    "narrowboat": "boat",
}


def related(a, b):
    """Whether two classes are one, or of one group."""
    return a == b or any(a in group and b in group for group in GROUPS)


def class_of(phrase):
    """The class a subject's phrase names: a class itself, or an alias of one, compared in
    lower case with a leading article dropped; `None` when it names none."""
    text = " ".join(str(phrase).lower().split())
    for article in ("the ", "a ", "an "):
        if text.startswith(article):
            text = text[len(article) :]
            break
    if text in CLASSES:
        return text
    return ALIASES.get(text)


def labels_of(context, image):
    """The fixed-class segmenter's labels of every class of the named picture: the context's
    `Labels`, each pixel's class one of `CLASSES` and the confidence its class has there."""
    return context.labels(image, list(CLASSES), model=MODEL)


def on_grid(labels, gw, gh):
    """A picture's labels on a working grid of `gw` by `gh`: for each working pixel, row by
    row, its class's name (`None` where none is) and that class's confidence, read at the
    pixel under the working pixel's centre, as `masks.Grid.mask` reads a mask. The class's
    confidence is its mean over its pixels (the labels' one figure a class)."""
    w, h, plane, classes = labels.width, labels.height, labels.plane, labels.classes
    xs = [min(w - 1, int((x + 0.5) * w / gw)) for x in range(gw)]
    names, sure = [], []
    for y in range(gh):
        row = min(h - 1, int((y + 0.5) * h / gh)) * w
        for x in xs:
            k = plane[row + x]
            names.append(classes[k - 1][0] if k else None)
            sure.append(classes[k - 1][1] if k else 0.0)
    return names, sure
