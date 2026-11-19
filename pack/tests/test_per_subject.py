"""Per-subject comparison (plan 2C, task X1, item 7), the tool file `pack/tools/per_subject.py`,
run on a context whose segmenter is the pack's fake model `fake-ok`: a phrase finds every
pixel whose red is 128 or more; a point or a box finds what `fake-ok` finds there that the
phrase finds too, the red thing at that place, as a model asked about a place answers with
the thing that is there."""

import importlib.util
import json
import os
import unittest

import kit
from whizbang_pack import fake_models, tool_worker

TOOL = os.path.join(kit.PACK, "tools", "per_subject.py")


def load():
    spec = importlib.util.spec_from_file_location("per_subject_under_test", TOOL)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


per_subject = load()


class Picture:
    """What `fake_models.determined` reads of a request's image."""

    def __init__(self, width, height, samples):
        self.width, self.height, self.samples = width, height, samples


def picture(width, height, red):
    """An image whose pixels in `red` (a set of (x, y)) are red and the rest grey."""
    rgba = bytearray()
    for y in range(height):
        for x in range(width):
            rgba += bytes([220, 30, 30, 255]) if (x, y) in red else bytes([60, 60, 60, 255])
    return tool_worker.Image("", width, height, bytes(rgba))


def block(left, top, right, bottom):
    return {(x, y) for x in range(left, right) for y in range(top, bottom)}


class Faked:
    """A context whose segmenter is `fake-ok`, over two named images; each ask noted."""

    def __init__(self, source, candidate, arguments):
        self.images = {"source": source, "candidate": candidate}
        self.arguments = arguments
        self.asked = []

    def image(self, name):
        return self.images[name]

    def segment(self, image, phrase=None, point=None, box=None, most=4):
        ((kind, value),) = [(k, v) for k, v in (("phrase", phrase), ("point", point), ("box", box)) if v is not None]
        self.asked.append((image, kind, value))
        shown = self.images[image]
        request = {"image": Picture(shown.width, shown.height, shown.rgba), "queries": [{kind: value}], "max_instances": most}
        found = fake_models.determined(request)
        if kind != "phrase" and found:
            red = fake_models.determined(dict(request, queries=[{"phrase": "red"}]))
            there = bytes(a & b for a, b in zip(found[0].samples, red[0].samples)) if red else b""
            found = [type(found[0])(0, found[0].score, None, found[0].box, there)] if any(there) else []
        return [tool_worker.Instance(f.score, f.box, shown.width, shown.height, f.samples) for f in found]


def job(*requirements, labels=()):
    images = [{"id": "candidate", "role": "candidate", "label": labels[0] if labels else None}, {"id": "source", "role": "source"}]
    listed = [{"id": "RQ%d" % n, "kind": kind, "text": text} for n, (kind, text) in enumerate(requirements, 1)]
    return json.dumps({"contract": "job/1", "use_case": "source-fidelity", "requirements": listed, "images": images})


class Subjects(unittest.TestCase):
    def test_subjects_come_from_the_jobs_requirements_and_never_its_label(self):
        given = json.loads(
            job(
                ("must_hold", "The rectangular swimming pool keeps its identity, shape and position."),
                ("must_hold", "A red front door keeps its colour."),
                ("must_not", "The lawn keeps its stripes."),
                labels=("a striped deck chair",),
            )
        )
        named, not_applicable = per_subject.subjects_of(given)
        self.assertEqual(named, ["rectangular swimming pool", "red front door"])
        self.assertEqual(not_applicable, [])
        # The label of an image names no subject, and is never asked for.
        context = Faked(picture(4, 4, set()), picture(4, 4, set()), {"job": json.dumps(given)})
        per_subject.run(context)
        phrases = {value for _, kind, value in context.asked if kind == "phrase"}
        self.assertEqual(phrases, {"rectangular swimming pool", "red front door"})

    def test_a_requirement_not_of_the_form_is_not_applicable(self):
        given = job(("must_hold", "The pool keeps its shape."), ("must_hold", "Nothing about the sky changes."))
        context = Faked(picture(4, 4, set()), picture(4, 4, set()), {"job": given})
        output = per_subject.run(context)
        self.assertEqual([r["subject"] for r in output["readings"]], ["pool"])
        self.assertEqual(output["not_applicable"], ["RQ2"])


class Score(unittest.TestCase):
    def test_the_items_value_is_the_largest_subjects_one_minus_iou(self):
        width = height = 8
        whole = bytes([255]) * 16 + bytes(48)
        half = bytes([255]) * 8 + bytes(56)
        # The same mask: 0; half of it: one minus a half; one empty: 1; both empty: 0.
        self.assertEqual(per_subject.compared(whole, whole, width, height)["one_minus_iou"], 0.0)
        self.assertEqual(per_subject.compared(whole, half, width, height)["one_minus_iou"], 0.5)
        self.assertEqual(per_subject.compared(whole, bytes(64), width, height)["one_minus_iou"], 1.0)
        self.assertEqual(per_subject.compared(bytes(64), bytes(64), width, height)["one_minus_iou"], 0.0)
        # The item's value is the largest of its subjects', as the declaration's rule
        # reduces them: the tool states each, and `worst` is declared.
        source = picture(8, 8, block(0, 0, 4, 4))
        candidate = picture(8, 8, block(0, 0, 4, 2))
        context = Faked(source, candidate, {"phrases": "red; grey", "boxes": "0 0 4 4"})
        readings = per_subject.run(context)["readings"]
        values = [r["values"]["one_minus_iou"] for r in readings]
        self.assertEqual(max(values), 0.5)
        with open(TOOL) as file:
            declared = file.read()
        self.assertIn('# primary = "one_minus_iou"\n# changed = "larger"\n# reduce = "worst"\n', declared)


class Compare(unittest.TestCase):
    def test_a_subject_removed_is_reported_absent_by_name_and_by_location(self):
        source = picture(12, 8, block(2, 2, 6, 6))
        removed = picture(12, 8, set())
        kept = picture(12, 8, block(2, 2, 6, 6))
        arguments = {"phrases": "red block", "boxes": "2 2 6 6"}
        gone = per_subject.run(Faked(source, removed, arguments))["readings"]
        self.assertEqual([r["subject"] for r in gone], ["red block", "box 2 2 6 6"])
        by_name, by_place = gone[0]["values"], gone[1]["values"]
        for values in (by_name, by_place):
            self.assertEqual((values["present_source"], values["present_candidate"], values["one_minus_iou"]), (1.0, 0.0, 1.0))
            self.assertEqual(values["area_candidate"], 0.0)
        still = per_subject.run(Faked(source, kept, arguments))["readings"]
        self.assertEqual([r["values"]["one_minus_iou"] for r in still], [0.0, 0.0])
        self.assertEqual(still[0]["values"]["area_candidate"], 16 / 96)
        # A candidate at another size: its mask is brought to the source's before they
        # are compared, and the box is scaled to it.
        larger = picture(24, 16, block(4, 4, 12, 12))
        scaled = per_subject.run(Faked(source, larger, arguments))["readings"]
        self.assertEqual([r["values"]["one_minus_iou"] for r in scaled], [0.0, 0.0])


if __name__ == "__main__":
    unittest.main()
