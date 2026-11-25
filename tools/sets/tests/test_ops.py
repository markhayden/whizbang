"""The construction ops and the draft label each implies. Ported from squint
`scripts/sets/tests/test_ops.py`."""

import unittest

import kit
from PIL import Image

from whizbang_sets import ops


def flat(w=64, h=48):
    im = Image.new("RGB", (w, h), (200, 40, 40))
    im.paste((40, 40, 200), (0, 0, 16, 12))  # a patch, so a flip or a turn shows
    return kit.png(im)


def textured(w=96, h=72):
    im = Image.new("RGB", (w, h))
    im.putdata([((x * 7) % 256, (y * 11) % 256, ((x + y) * 5) % 256) for y in range(h) for x in range(w)])
    return kit.png(im)


class Painter:
    """A generator whose edit paints one green patch, and whose generate is dark."""

    name, model = "fake", "fake-model"

    def __init__(self, box=(20, 20, 40, 40)):
        self.box = box

    def edit(self, image, prompt, seed):
        im = kit.load(image)
        im.paste((0, 255, 0), self.box)
        return kit.png(im)

    def generate(self, prompt, seed, size):
        return kit.png(Image.new("RGB", (size, size), (10, 10, 10)))


class Ops(unittest.TestCase):
    def test_mirror_flips_left_to_right_and_fails_the_structure(self):
        out, label = ops.apply({"kind": "mirror"}, flat(), None, 1)
        self.assertEqual(kit.load(out).getpixel((63, 0)), (40, 40, 200))
        self.assertEqual((label.expected, label.obligation_ids, label.region_kind, label.failure_type),
                         ("fail", ["SF2"], "whole_image", "mirrored"))

    def test_rotate_and_crop_change_the_size_and_fail_the_structure(self):
        out, label = ops.apply({"kind": "rotate", "degrees": 90}, flat(), None, 1)
        self.assertEqual(kit.load(out).size, (48, 64))
        self.assertEqual(label.failure_type, "rotated")
        out, label = ops.apply({"kind": "crop", "keep": 0.75}, flat(), None, 1)
        self.assertEqual(kit.load(out).size, (48, 36))
        self.assertEqual(label.obligation_ids, ["SF2"])

    def test_grade_is_a_mild_permitted_change(self):
        out, label = ops.apply({"kind": "grade", "warmth": 0.1}, flat(), None, 1)
        r, _g, b = kit.load(out).getpixel((40, 40))
        self.assertTrue(r >= 200 and b < 40, "warmer: red up, blue down")
        self.assertEqual((label.expected, label.obligation_ids, label.region_kind), ("pass", [], None))

    def test_edit_asks_the_generator_and_draws_the_region_from_the_difference(self):
        op = {"kind": "edit", "prompt": "Take the boat away.", "removes": "boat"}
        out, label = ops.apply(op, flat(), Painter(), 7)
        self.assertEqual(kit.load(out).getpixel((30, 30)), (0, 255, 0))
        self.assertEqual((label.expected, label.obligation_ids, label.region_kind), ("fail", ["SF1"], "bbox"))
        x0, y0, x1, y1 = label.bbox
        self.assertLess(abs(x0 - 20 / 64), 0.15)
        self.assertLess(abs(y1 - 40 / 48), 0.2)
        self.assertIn("boat", label.words)

    def test_an_edit_that_adds_names_what_was_added(self):
        op = {"kind": "edit", "prompt": "add a chimney", "adds": "a chimney"}
        _, label = ops.apply(op, textured(), Painter((30, 20, 50, 40)), 1)
        self.assertEqual(label.failure_type, "a chimney-added")
        self.assertTrue(label.words.startswith("the candidate has a chimney"))

    def test_restyle_is_a_permitted_edit_and_says_where_it_landed(self):
        op = {"kind": "restyle", "prompt": "Make it dusk. Keep everything where it is."}
        out, label = ops.apply(op, flat(), Painter(), 3)
        self.assertEqual((label.expected, label.obligation_ids, label.region_kind), ("pass", [], None))
        self.assertIn("restyle", label.words)
        self.assertIn("changed_area", label.notes)

    def test_generate_plants_the_named_defect_or_is_a_pass(self):
        op = {"kind": "generate", "prompt": "six fingers", "defect": "anatomy", "words": "six fingers", "size": 32}
        out, label = ops.apply(op, flat(), Painter(), 9)
        self.assertEqual(kit.load(out).size, (32, 32))
        self.assertEqual((label.obligation_ids, label.region_kind, label.words), (["RE1"], "whole_image", "six fingers"))
        _, label = ops.apply({"kind": "generate", "prompt": "clean", "size": 32}, flat(), Painter(), 9)
        self.assertEqual((label.expected, label.obligation_ids), ("pass", []))
        with self.assertRaises(ValueError):
            ops.apply({"kind": "generate", "prompt": "x", "defect": "glow", "size": 32}, flat(), Painter(), 9)

    def test_camera_ops_keep_the_size_fill_the_frame_and_fail_the_structure(self):
        for op, kind in (
            ({"kind": "tilt", "degrees": 3}, "tilted"),
            ({"kind": "zoom", "factor": 1.1}, "zoomed"),
            ({"kind": "keystone", "strength": 0.06}, "keystoned"),
        ):
            out, label = ops.apply(op, textured(), None, 1)
            im = kit.load(out)
            self.assertEqual(im.size, (96, 72), kind)
            self.assertEqual((label.failure_type, label.obligation_ids), (kind, ["SF2"]))
            for xy in ((0, 0), (95, 0), (0, 71), (95, 71)):
                self.assertGreater(sum(im.getpixel(xy)), 0, (kind, xy))

    def test_out_of_range_strengths_and_unknown_ops_are_refused(self):
        for op in (
            {"kind": "tilt", "degrees": 30},
            {"kind": "zoom", "factor": 1.0},
            {"kind": "keystone", "strength": 0.5},
            {"kind": "rotate", "degrees": 45},
            {"kind": "explode"},
            {"kind": "edit", "prompt": "x"},
        ):
            with self.assertRaises(ValueError, msg=op):
                ops.apply(op, textured(), None, 1)

    def test_delivery_damage_keeps_a_pass_a_pass_and_says_so(self):
        op = {"kind": "grade", "warmth": 0.1, "jpeg": 50, "resample": 0.5}
        out, label = ops.apply(op, textured(), None, 1)
        self.assertEqual(kit.load(out).size, (96, 72))
        self.assertEqual(label.expected, "pass")
        self.assertIn("JPEG at quality 50", label.words)
        self.assertIn("50%", label.notes["delivery"])


if __name__ == "__main__":
    unittest.main()
