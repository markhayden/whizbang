"""`classes` (plan mE, task T2, item 4), the tool file `pack/tools/classes.py`, the method
`eomt` of `regions` and of `outline` (item 5), and the shared reading of the fixed-class
segmenter's classes (`whizbang_pack.class_groups`), on a context whose fixed-class segmenter
is a fake: each red pixel (red 128 or more) is `swimming pool`, sure at 0.9, and every other
`floor`, sure at 0.8, as `eomt-ade20k`'s labels come through `context.labels`. The record's
two examples (`docs/reference/tools/classes.md`) are run here: their pictures are drawn and
held to the committed files, and the readings and region the record shows are held to the
tool's."""

import os
import unittest

import kit
from test_outline import assert_region_held, decoded, drawn, job, load, record_rows
from whizbang_pack import class_groups, tool_worker

TOOL = os.path.join(kit.PACK, "tools", "classes.py")
RECORD = os.path.join(os.path.dirname(kit.PACK), "docs", "reference", "tools", "classes.md")

classes = load(TOOL, "classes_under_test")
regions = load(os.path.join(kit.PACK, "tools", "regions.py"), "regions_by_class")
outline = load(os.path.join(kit.PACK, "tools", "outline.py"), "outline_by_class")


class Classed:
    """A context over two named images whose fixed-class segmenter is the fake above; it
    answers `labels` and `segment` of the model `eomt-ade20k` only, and counts each ask."""

    SURE = {"swimming pool": 0.9, "floor": 0.8}

    def __init__(self, source, candidate, arguments):
        self.images = {"source": source, "candidate": candidate}
        self.arguments = arguments
        self.asked = []

    def image(self, name):
        return self.images[name]

    def _class_at(self, image, at):
        return "swimming pool" if image.rgba[4 * at] >= 128 else "floor"

    def labels(self, image, asked, model=None):
        self.asked.append(("labels", image, model))
        assert model == class_groups.MODEL and list(asked) == list(class_groups.CLASSES)
        shown = self.images[image]
        names = [self._class_at(shown, at) for at in range(shown.width * shown.height)]
        present = [name for name in ("floor", "swimming pool") if name in names]
        plane = bytes(present.index(name) + 1 for name in names)
        return tool_worker.Labels(shown.width, shown.height, [(name, self.SURE[name]) for name in present], plane)

    def segment(self, image, phrase=None, point=None, box=None, most=4, model=None):
        self.asked.append(("segment", image, phrase, model))
        assert model == class_groups.MODEL
        shown = self.images[image]
        on = bytes(255 if self._class_at(shown, at) == phrase else 0 for at in range(shown.width * shown.height))
        return [tool_worker.Instance(None, [0, 0, shown.width, shown.height], shown.width, shown.height, on)] if any(on) else []


class Groups(unittest.TestCase):
    def test_a_phrase_names_a_class_or_an_alias_and_groups_are_relabellings(self):
        self.assertEqual(class_groups.class_of("The pool"), "swimming pool")
        self.assertEqual(class_groups.class_of("building"), "building")
        self.assertIsNone(class_groups.class_of("a duck"))
        self.assertTrue(class_groups.related("road", "sidewalk"))
        self.assertFalse(class_groups.related("floor", "swimming pool"))
        for group in class_groups.GROUPS:
            self.assertLessEqual(group, set(class_groups.CLASSES))
        self.assertLessEqual(set(class_groups.ALIASES.values()), set(class_groups.CLASSES))


class Classes(unittest.TestCase):
    def test_the_box_names_its_classes_and_where_its_main_class_changed(self):
        source = drawn(40, 30)
        candidate = drawn(40, 30, (10, 10, 30, 20))
        context = Classed(source, candidate, {"bbox": "0.25 0.34 0.75 0.66"})
        output = classes.run(context)
        box, *named = output["readings"]
        self.assertEqual(box["subject"], "the box")
        self.assertEqual(box["values"], {"class_changed": 1.0, "confidence_source": 0.8, "confidence_candidate": 0.9})
        self.assertEqual([(r["subject"], r["values"]) for r in named], [("floor", {"share_source": 1.0, "share_candidate": 0.0}), ("swimming pool", {"share_source": 0.0, "share_candidate": 1.0})])
        (region,) = output["regions"]
        self.assertEqual((region["label"], region["score"], region["frame"]), ("floor -> swimming pool", 0.8, "candidate"))
        self.assertEqual(region["box"], [10.0, 10.2, 30.0, 19.8])
        self.assertEqual([a[0] for a in context.asked], ["labels", "labels"])
        # Given no box, the whole frame, as `eval tool` gives it: a sixth of it changed, its
        # main class still the floor, so no region; the confidence a share-weighted mean.
        whole = classes.run(Classed(source, candidate, {}))
        values = whole["readings"][0]["values"]
        self.assertEqual(values["class_changed"], round(200 / 1200, 4))
        self.assertEqual(values["confidence_candidate"], round((1000 * 0.8 + 200 * 0.9) / 1200, 4))
        self.assertEqual(whole["regions"], [])
        for text in ("0 0 1", "0.5 0 0.4 1", "a b c d", "0 0 1.5 1"):
            with self.assertRaises(ValueError):
                classes.box_of(text)

    def test_a_class_under_the_least_share_is_folded_away(self):
        self.assertEqual(classes.composition(["a"] * 96 + ["b"] * 4), [("a", 0.96)])
        self.assertEqual(classes.composition(["a", "b", "c", "d", "e"] * 2)[:1], [("a", 0.2)])
        self.assertEqual(len(classes.composition(list("abcdefgh") * 2)), classes.MAX_CLASSES)


class RegionsByClass(unittest.TestCase):
    def test_the_method_eomt_labels_a_region_by_its_classes_and_preserve_raises_it(self):
        source = drawn(64, 48)
        candidate = drawn(64, 48, (20, 10, 44, 30))
        context = Classed(source, candidate, {"method": "eomt"})
        output = regions.run(context)
        (reading,) = output["readings"]
        self.assertNotIn("subject_disagreement", reading["values"])
        self.assertGreater(reading["values"]["class_disagreement"], 0.0)
        top = output["regions"][0]
        self.assertEqual(top["label"], "floor -> swimming pool")
        self.assertEqual({a[0] for a in context.asked}, {"labels"})
        # A preserved class raises the region; both sides ignored lowers it.
        preserved = regions.run(Classed(source, candidate, {"method": "eomt", "preserve": "pool"}))["regions"][0]
        ignored = regions.run(Classed(source, candidate, {"method": "eomt", "ignore": "floor; pool"}))["regions"][0]
        self.assertGreaterEqual(preserved["score"], top["score"])
        self.assertLess(ignored["score"], top["score"])
        # An unchanged pair finds nothing.
        self.assertEqual(regions.run(Classed(source, source, {"method": "eomt"}))["regions"], [])
        with self.assertRaises(ValueError):
            regions.run(Classed(source, source, {"method": "onnx"}))

    def test_a_disagreement_between_related_classes_is_a_relabelling(self):
        grid = regions.Grid(20, 10)
        before = (["floor"] * 200, [0.9] * 200)
        after = (["floor"] * 100 + ["road"] * 100, [0.9] * 200)
        residual = [3.0 if at >= 100 else 0.0 for at in range(200)]
        photometric = 0
        found = regions.find_classes(grid, residual, photometric, before, after, set(), set())[0]
        unrelated = regions.find_classes(grid, residual, photometric, before, (["floor"] * 100 + ["car"] * 100, [0.9] * 200), set(), set())[0]
        self.assertEqual(found[0]["label"], "floor -> road")
        self.assertAlmostEqual(found[0]["score"], round(unrelated[0]["score"] * class_groups.RELABEL, 3), places=2)


class OutlineByClass(unittest.TestCase):
    def test_the_method_eomt_asks_the_class_a_subject_names_and_a_subject_of_none_is_not_applicable(self):
        source = drawn(40, 30, (10, 10, 30, 20))
        candidate = drawn(40, 30, (10, 10, 30, 24))
        context = Classed(source, candidate, {"method": "eomt", "job": job("The pool keeps its shape.", "The duck keeps its place.")})
        output = outline.run(context)
        (reading,) = output["readings"]
        self.assertEqual(reading["subject"], "pool")
        self.assertGreater(reading["values"]["one_minus_iou"], 0.0)
        self.assertEqual(output["not_applicable"], ["duck"])
        self.assertEqual({(a[2], a[3]) for a in context.asked}, {("swimming pool", class_groups.MODEL)})
        # Every subject's source first, then the candidates: one picture's labels at a time.
        self.assertEqual([a[1] for a in context.asked], ["source", "candidate"])


# --- the record's examples ----------------------------------------------------------------

EXAMPLES = {
    "source": drawn(160, 120, (100, 30, 140, 90)),
    "poured": drawn(160, 120, (100, 30, 140, 90), (20, 40, 80, 100)),
}
BOX = "0.125 0.3333 0.5 0.8333"


class Record(unittest.TestCase):
    def test_the_records_examples_are_the_tools_readings_on_its_pictures(self):
        folder = os.path.join(os.path.dirname(RECORD), "classes")
        for name, picture in EXAMPLES.items():
            self.assertEqual(decoded(os.path.join(folder, name + ".png")).rgba, picture.rgba, name)
        rows = record_rows("read", RECORD)
        made = []
        for name, arguments in (("box", {"bbox": BOX}), ("whole", {})):
            output = classes.run(Classed(EXAMPLES["source"], EXAMPLES["poured"], arguments))
            for reading in output["readings"]:
                shown = " ".join("%s %s" % item for item in sorted(reading["values"].items()))
                made.append([name, reading["subject"], shown])
            for region in output["regions"]:
                assert_region_held(self, region, EXAMPLES["poured"], 120)
                made.append([name, "region", "%s %s" % (region["label"], region["score"])])
        self.assertEqual(rows, made)


if __name__ == "__main__":
    unittest.main()


def method_rows(tool, folder, examples, arguments):
    """The rows of a record's table of its method `eomt`: for each example against the
    record's own source, the values of each reading and its regions, as the record shows."""
    source = decoded(os.path.join(folder, "source.png"))
    rows = []
    for example in examples:
        picture = decoded(os.path.join(folder, example + ".png"))
        output = tool.run(Classed(source, picture, dict(arguments, method="eomt")))
        for reading in output["readings"]:
            values = " ".join("%s %g" % item for item in sorted(reading["values"].items()))
            found = "; ".join("%s %g" % (r["label"], r["score"]) for r in output["regions"]) or "none"
            rows.append([example, reading["subject"], values, found])
    return rows


class MethodRecords(unittest.TestCase):
    def test_the_records_of_regions_and_outline_show_their_method_eomt(self):
        # Plan mE, task T2, item 5: each method's examples run in the record. The records'
        # own pictures, read by a fake fixed-class segmenter (red is `swimming pool`).
        tools = os.path.dirname(RECORD)
        for tool, name in ((regions, "regions"), (outline, "outline")):
            record = os.path.join(tools, name + ".md")
            made = method_rows(tool, os.path.join(tools, name), ("tub", "redrawn") if name == "regions" else ("widened", "towel"), {"job": job("The pool keeps its shape.")})
            self.assertEqual(record_rows("eomt", record), made, name)
