"""`outline` (plan mE, task T1, item 1), the tool file `pack/tools/outline.py`: squint's shape
comparison on planted masks (its `tools/python/tests/test_outline.py` at the commit
`docs/sources.toml` pins, the cases whizbang keeps), the subjects it reads, and runs on a
context whose segmenter is the pack's fake model `fake-ok`, which finds for a phrase every
pixel whose red is 128 or more. The record's two examples (`docs/reference/tools/outline.md`)
are run here: their pictures are drawn and held to the committed files, and the readings and
regions the record shows are held to the tool's."""

import importlib.util
import inspect
import json
import os
import struct
import unittest
import zlib

import kit
from whizbang_pack import fake_models, subjects, tool_worker

TOOL = os.path.join(kit.PACK, "tools", "outline.py")
PER_SUBJECT = os.path.join(kit.PACK, "tools", "per_subject.py")
RECORD = os.path.join(os.path.dirname(kit.PACK), "docs", "reference", "tools", "outline.md")


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


outline = load(TOOL, "outline_under_test")


def rect(grid, x0, y0, x1, y1):
    return grid.of_runs([(y, x0, x1) for y in range(y0, y1)])


def at(grid, m, x, y):
    return (m >> (grid.n - 1 - (y * grid.w + x))) & 1


class Compare(unittest.TestCase):
    def test_identical_shapes_have_no_pieces_and_no_shift(self):
        grid = outline.Grid(400, 300)
        m = rect(grid, 50, 100, 350, 250)
        found = outline.compare(grid, m, m, 1.0)
        self.assertEqual(found["iou"], 1.0)
        self.assertEqual((found["shift_p95_px"], found["shift_max_px"]), (0.0, 0.0))
        self.assertEqual(found["pieces"], [])

    def test_a_widened_edge_is_one_added_piece_with_its_shift(self):
        grid = outline.Grid(400, 300)
        before = rect(grid, 50, 100, 350, 200)
        after = rect(grid, 50, 100, 350, 230)  # the near edge moved 30 px down
        found = outline.compare(grid, before, after, 2.0)
        added = [p for p in found["pieces"] if p["kind"] == "added"]
        self.assertEqual(len(added), 1)
        self.assertFalse([p for p in found["pieces"] if p["kind"] == "removed"])
        _, y0, _, y1 = added[0]["bbox"]
        self.assertLess(abs(y0 - 200 / 300), 0.02)
        self.assertLess(abs(y1 - 230 / 300), 0.02)
        # 30 working pixels at a scale of 2 are 60 pixels of the frame.
        self.assertTrue(55 <= added[0]["shift_px"] <= 62, added[0]["shift_px"])
        self.assertGreater(found["area_change"], 0.25)

    def test_slivers_along_one_moved_edge_are_one_piece_and_pieces_out_of_line_are_not(self):
        grid = outline.Grid(1024, 768)
        # The near edge moved 8 px, but the segmenter kept the old line in two places:
        # three slivers in a row, each too small to score, that are one change.
        before = rect(grid, 100, 300, 900, 500)
        after = rect(grid, 100, 300, 900, 508) & ~rect(grid, 330, 500, 360, 508) & ~rect(grid, 600, 500, 640, 508)
        added = [p for p in outline.compare(grid, before, after, 1.0)["pieces"] if p["kind"] == "added"]
        self.assertEqual([p.get("parts") for p in added], [3])
        self.assertTrue(added[0]["bbox"][0] < 0.11 and added[0]["bbox"][2] > 0.87)
        # One sliver under the left of the edge, one up the right side: apart, and across
        # each other's axis.
        after = before | rect(grid, 100, 500, 300, 508) | rect(grid, 900, 300, 908, 500)
        added = [p for p in outline.compare(grid, before, after, 1.0)["pieces"] if p["kind"] == "added"]
        self.assertEqual(len(added), 2)

    def test_a_cut_corner_is_a_removed_piece_and_jitter_is_none(self):
        grid = outline.Grid(400, 300)
        before = rect(grid, 50, 100, 350, 250)
        cut = before & ~rect(grid, 300, 100, 350, 160)
        self.assertEqual([p["kind"] for p in outline.compare(grid, before, cut, 1.0)["pieces"]], ["removed"])
        # 2 px: under the jitter bar.
        thin = rect(grid, 50, 100, 350, 252)
        self.assertEqual(outline.compare(grid, before, thin, 1.0)["pieces"], [])

    def test_the_frame_edge_is_not_boundary(self):
        grid = outline.Grid(400, 300)
        m = rect(grid, 100, 100, 400, 200)  # runs off the right edge
        self.assertEqual(grid.boundary(m) & grid.coll, 0)
        self.assertEqual(outline.compare(grid, m, m, 1.0)["shift_max_px"], 0.0)

    def test_a_complete_removal_scores_one_and_only_a_small_objects_piece_on_the_edge_scores_half(self):
        grid = outline.Grid(400, 300)
        bench = rect(grid, 100, 100, 140, 200)  # 1.3% of the frame
        (piece,) = outline.compare(grid, bench, 0, 1.0)["pieces"]
        self.assertEqual((piece["kind"], piece["score"], piece["shift_px"]), ("removed", 1.0, None))
        # A 10 px strip lost where a small object meets the frame's left edge, beside a
        # large one that is unchanged: its score is halved.
        big = rect(grid, 100, 50, 380, 280)
        before, after = rect(grid, 0, 100, 30, 150) | big, rect(grid, 10, 100, 30, 150) | big
        (piece,) = outline.compare(grid, before, after, 1.0)["pieces"]
        self.assertEqual(piece["kind"], "removed")
        self.assertTrue(piece["edge"] and piece["score"] <= 0.5)
        # A pool running off the right edge, its end redrawn: part of a large object, not
        # halved.
        before = rect(grid, 50, 100, 400, 250)
        after = before & ~rect(grid, 385, 100, 400, 140)
        (piece,) = [p for p in outline.compare(grid, before, after, 1.0)["pieces"] if p["kind"] == "removed"]
        self.assertTrue(piece["edge"] and piece["object"] > outline.EDGE_OBJECT)
        plain = 0.5 * min(1.0, piece["area"] / 0.005) + 0.5 * min(1.0, (piece["shift_px"] or 0) / 30)
        self.assertGreaterEqual(piece["score"], round(min(1.0, plain), 3) - 0.001)

    def test_specks_go_on_both_sides_by_one_bar_relative_to_the_largest(self):
        grid = outline.Grid(400, 300)
        # A cushion speck beside a large pool goes; a small clock face, all of its mask,
        # stays.
        pool = rect(grid, 50, 100, 350, 250) | rect(grid, 10, 10, 14, 14)
        kept, _ = outline.despeckled_pair(grid, pool, pool)
        self.assertEqual((at(grid, kept, 11, 11), at(grid, kept, 200, 150)), (0, 1))
        clock = rect(grid, 200, 50, 212, 62)
        kept, _ = outline.despeckled_pair(grid, clock, clock)
        self.assertEqual(at(grid, kept, 205, 55), 1)
        # A fragment just over the bar of a small mask, under the bar the other picture's
        # large shape sets, goes on both sides.
        before = rect(grid, 50, 100, 350, 250) | rect(grid, 10, 10, 20, 20)
        after = rect(grid, 10, 10, 20, 20)
        b, a = outline.despeckled_pair(grid, before, after)
        self.assertEqual((at(grid, b, 15, 15), a), (0, 0))


class Subjects(unittest.TestCase):
    def test_subjects_are_read_by_per_subjects_rule_shared(self):
        per_subject = load(PER_SUBJECT, "per_subject_beside_outline")
        # One rule: the same function, word for word, over the same constants.
        self.assertEqual(inspect.getsource(subjects.subjects_of), inspect.getsource(per_subject.subjects_of))
        self.assertEqual((subjects.KEEPS, subjects.ARTICLES), (per_subject.KEEPS, per_subject.ARTICLES))
        cases = [
            {"requirements": [{"id": "RQ1", "kind": "must_hold", "text": "The rectangular swimming pool keeps its shape."}]},
            {"requirements": [{"id": "RQ1", "kind": "must_hold", "text": "Nothing about the sky changes."}, {"id": "RQ2", "kind": "must_not", "text": "The lawn keeps its stripes."}]},
            {"requirements": [{"id": "RQ1", "kind": "must_hold", "text": "An old shed keeps its roof."}, {"kind": "must_hold", "text": "keeps"}]},
            {"requirements": [{"id": "RQ1", "kind": "must_hold", "text": "THE Pool keeps its shape."}, {"id": "RQ2", "kind": "must_hold", "text": "The keeps x"}]},
            {},
        ]
        for job in cases:
            self.assertEqual(subjects.subjects_of(job), per_subject.subjects_of(job), job)
        # An article in capitals is dropped; "The" before " keeps " with no space after it is
        # a subject of its own.
        self.assertEqual(subjects.subjects_of(cases[3]), (["Pool", "The"], []))
        # outline reads them from the shared module, and from nowhere of its own.
        self.assertIs(outline.subjects_of, subjects.subjects_of)


class Picture:
    """What `fake_models.determined` reads of a request's image."""

    def __init__(self, width, height, samples):
        self.width, self.height, self.samples = width, height, samples


class Faked:
    """A context whose segmenter is `fake-ok` (phrases only), over two named images."""

    def __init__(self, source, candidate, arguments):
        self.images = {"source": source, "candidate": candidate}
        self.arguments = arguments
        self.asked = []

    def image(self, name):
        return self.images[name]

    def segment(self, image, phrase=None, point=None, box=None, most=4):
        self.asked.append((image, phrase))
        shown = self.images[image]
        request = {"image": Picture(shown.width, shown.height, shown.rgba), "queries": [{"phrase": phrase}], "max_instances": most}
        return [tool_worker.Instance(f.score, f.box, shown.width, shown.height, f.samples) for f in fake_models.determined(request)]


GREY, RED = bytes([60, 60, 60, 255]), bytes([220, 30, 30, 255])


def drawn(width, height, *blocks, scale=1):
    """A grey picture with red blocks, each (left, top, right, bottom) before `scale`."""
    red = set()
    for left, top, right, bottom in blocks:
        red |= {(x, y) for x in range(left * scale, right * scale) for y in range(top * scale, bottom * scale)}
    rgba = b"".join(RED if (x, y) in red else GREY for y in range(height * scale) for x in range(width * scale))
    return tool_worker.Image("", width * scale, height * scale, rgba)


def job(*texts):
    return json.dumps({"contract": "job/1", "use_case": "source-fidelity", "requirements": [{"id": "RQ%d" % n, "kind": "must_hold", "text": t} for n, t in enumerate(texts, 1)]})


class Run(unittest.TestCase):
    def test_a_subject_on_neither_picture_is_not_applicable_and_the_label_is_never_asked(self):
        source = drawn(40, 30, (10, 10, 30, 20))
        context = Faked(source, source, {"job": job("The pool keeps its shape.", "Nothing changes."), "phrases": "red"})
        output = outline.run(context)
        self.assertEqual([r["subject"] for r in output["readings"]], ["pool", "red"])
        self.assertEqual(output["not_applicable"], ["RQ2"])
        self.assertEqual({p for _, p in context.asked}, {"pool", "red"})
        self.assertEqual([r["values"]["one_minus_iou"] for r in output["readings"]], [0.0, 0.0])
        self.assertEqual(output["regions"], [])
        # A subject the segmenter finds on neither picture has nothing to compare.
        blank = drawn(40, 30)
        self.assertEqual(outline.run(Faked(blank, blank, {"phrases": "pool"}))["not_applicable"], ["pool"])

    def test_a_candidate_at_another_size_is_brought_to_the_grid_and_one_of_another_shape_is_refused(self):
        source = drawn(40, 30, (10, 10, 30, 20))
        larger = drawn(40, 30, (10, 10, 30, 20), scale=2)
        (reading,) = outline.run(Faked(source, larger, {"phrases": "pool"}))["readings"]
        self.assertEqual(reading["values"]["one_minus_iou"], 0.0)
        with self.assertRaises(ValueError):
            outline.run(Faked(source, drawn(30, 30, (10, 10, 20, 20)), {"phrases": "pool"}))


# --- the record's examples ----------------------------------------------------------------

EXAMPLES = {
    "source": drawn(160, 120, (40, 40, 120, 80)),
    "widened": drawn(160, 120, (40, 40, 120, 92)),
    "towel": drawn(160, 120, (40, 40, 120, 80), (125, 60, 135, 70)),
}


def decoded(path):
    """A PNG this test's examples are kept as: 8-bit RGB, not interlaced, each row of filter
    0; its samples as RGBA."""
    with open(path, "rb") as file:
        data = file.read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    at, idat, header = 8, b"", None
    while at < len(data):
        (length,) = struct.unpack(">I", data[at : at + 4])
        kind, body = data[at + 4 : at + 8], data[at + 8 : at + 8 + length]
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        at += 12 + length
    width, height, depth, colour, _, _, interlace = header
    assert (depth, colour, interlace) == (8, 2, 0), header
    raw = zlib.decompress(idat)
    rgba = bytearray()
    for y in range(height):
        row = raw[y * (3 * width + 1) : (y + 1) * (3 * width + 1)]
        assert row[0] == 0, "a row of another filter"
        for x in range(width):
            rgba += row[1 + 3 * x : 4 + 3 * x] + b"\xff"
    return tool_worker.Image("", width, height, bytes(rgba))


def encoded(image):
    """The PNG an example is kept as (`decoded` reads it)."""

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    raw = b"".join(b"\x00" + b"".join(image.rgba[4 * (y * image.width + x) : 4 * (y * image.width + x) + 3] for x in range(image.width)) for y in range(image.height))
    header = struct.pack(">IIBBBBB", image.width, image.height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def record_rows(marker, record=RECORD):
    """The rows of a record's table between `<!-- <marker>: begin -->` and its end, each a
    list of cells, its heading and rule left out."""
    with open(record, encoding="utf-8") as file:
        text = file.read()
    part = text.split("<!-- %s: begin -->" % marker)[1].split("<!-- %s: end -->" % marker)[0]
    lines = [line for line in part.strip().splitlines() if line.startswith("|")]
    return [[cell.strip() for cell in line.strip("|").split("|")] for line in lines[2:]]


def assert_region_held(test, region, candidate, label_most):
    """A region as `tool-file/1` holds it (ADR 0032, "Amended: regions"): a box on the
    candidate inside it with area, a printable label of 1 to `label_most` bytes, a score from
    0 to 1, and nothing else."""
    test.assertEqual(set(region), {"frame", "box", "label", "score"})
    test.assertEqual(region["frame"], "candidate")
    left, top, right, bottom = region["box"]
    test.assertTrue(0 <= left < right <= candidate.width and 0 <= top < bottom <= candidate.height, region)
    test.assertTrue(1 <= len(region["label"].encode()) <= label_most, region)
    test.assertTrue(all(" " <= c <= "~" for c in region["label"]), region)
    test.assertTrue(0.0 <= region["score"] <= 1.0, region)


class Record(unittest.TestCase):
    def test_the_records_examples_are_the_tools_readings_on_its_pictures(self):
        folder = os.path.join(os.path.dirname(RECORD), "outline")
        pictures = {}
        for name, image in EXAMPLES.items():
            pictures[name] = decoded(os.path.join(folder, name + ".png"))
            self.assertEqual((pictures[name].width, pictures[name].height, pictures[name].rgba), (image.width, image.height, image.rgba), name)
        read, regions = [], []
        for example in ("widened", "towel"):
            output = outline.run(Faked(pictures["source"], pictures[example], {"job": job("The pool keeps its shape.")}))
            for reading in output["readings"]:
                v = reading["values"]
                read.append([example, reading["subject"]] + ["%.3f" % v[k] for k in ("one_minus_iou", "area_source", "area_candidate", "area_change", "added", "removed")] + ["%.1f" % v[k] for k in ("shift_p95_px", "shift_max_px")])
            for region in output["regions"]:
                assert_region_held(self, region, pictures[example], outline.LABEL_MOST)
                regions.append([example, " ".join("%g" % e for e in region["box"]), region["label"], "%.3f" % region["score"]])
        self.assertEqual(record_rows("read"), read)
        self.assertEqual(record_rows("regions"), regions)


if __name__ == "__main__":
    unittest.main()
