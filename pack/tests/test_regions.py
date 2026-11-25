"""`regions` (plan mE, task T1, item 2), the tool file `pack/tools/regions.py`, and the shared
masks it is built on (`whizbang_pack.masks`): squint's cases that whizbang keeps (its
`tools/python/tests/test_regions.py` at the commit `docs/sources.toml` pins), `change_map`'s
arithmetic, and runs on a context whose segmenter is the pack's fake model `fake-ok`, which
finds for a phrase every pixel whose red is 128 or more. The record's two examples
(`docs/reference/tools/regions.md`) are run here: their pictures are drawn and held to the
committed files, and the readings and regions the record shows are held to the tool's."""

import os
import time
import unittest

import kit
from test_outline import Faked, assert_region_held, decoded, job, load, record_rows
from whizbang_pack import masks, tool_worker

TOOL = os.path.join(kit.PACK, "tools", "regions.py")
RECORD = os.path.join(os.path.dirname(kit.PACK), "docs", "reference", "tools", "regions.md")

regions = load(TOOL, "regions_under_test")


def rect(grid, x0, y0, x1, y1):
    return grid.of_runs([(y, x0, x1) for y in range(y0, y1)])


class Masks(unittest.TestCase):
    def test_components_label_each_connected_blob_once_and_a_serpentine_stays_one(self):
        grid = masks.Grid(40, 30)
        m = rect(grid, 1, 1, 5, 5) | rect(grid, 10, 1, 12, 20) | rect(grid, 30, 25, 40, 30)
        pieces, _ = masks.components(grid, m)
        self.assertEqual(sorted(p[0] for p in pieces), [16, 38, 50])
        # A serpentine: rows joined at alternate ends, one piece, found fast.
        grid = masks.Grid(200, 200)
        m = 0
        for y in range(0, 200, 2):
            m |= rect(grid, 0, y, 200, y + 1)
            x = 199 if (y // 2) % 2 == 0 else 0
            if y + 2 < 200:
                m |= rect(grid, x, y + 1, x + 1, y + 2)
        started = time.monotonic()
        self.assertEqual(len(masks.components(grid, m)[0]), 1)
        self.assertLess(time.monotonic() - started, 1.0)

    def test_a_mask_is_brought_to_the_grid_by_the_pixel_under_each_centre(self):
        # Two by one, the right pixel on, to five by one: the centres at 0.2, 0.6, 1.0, 1.4
        # and 1.8 of the mask's pixels fall on 0, 0, 1, 1 and 1.
        grid = masks.Grid(5, 1)
        self.assertEqual(grid.text(grid.mask(0b01, 2, 1)), "00111")
        self.assertEqual([masks.nearest_index(i, 2, 5) for i in range(5)], [0, 0, 1, 1, 1])

    def test_a_mask_of_another_size_than_its_picture_is_refused(self):
        picture = tool_worker.Image("", 4, 4, bytes(64))

        class Wrong:
            def image(self, name):
                return picture

            def segment(self, image, phrase=None, most=4):
                return [tool_worker.Instance(0.7, [0, 0, 2, 2], 2, 2, bytes([255]) * 4)]

        with self.assertRaises(ValueError):
            masks.mask_of(Wrong(), "source", "pool", 4)


class ChangeMap(unittest.TestCase):
    def test_a_flat_picture_normalises_to_nothing_and_blur_keeps_a_flat_one(self):
        flat = [0.4] * 12
        self.assertEqual(regions.blur3(flat, 4, 3), [0.4] * 12)
        self.assertTrue(all(abs(v) < 1e-9 for v in regions.lcn(flat, 4, 3)))
        # An edge stands out, the same either side of it.
        edge = [0.0, 0.0, 1.0, 1.0] * 3
        normalised = regions.lcn(edge, 4, 3, radius=1)
        self.assertLess(normalised[1], 0)
        self.assertGreater(normalised[2], 0)


def maps(grid, changed=(), residual=2.0):
    """A residual of `residual` inside each box of `changed`, and nothing elsewhere."""
    values = [0.0] * grid.n
    m = 0
    for x0, y0, x1, y1 in changed:
        m |= rect(grid, x0, y0, x1, y1)
        for y in range(y0, y1):
            for x in range(x0, x1):
                values[y * grid.w + x] = residual
    return values, m


class Find(unittest.TestCase):
    def test_an_unchanged_pair_finds_nothing(self):
        grid = masks.Grid(64, 48)
        residual, photometric = maps(grid)
        pool = rect(grid, 10, 10, 40, 30)
        found, changed, _ = regions.find(grid, residual, photometric, [("pool", pool, pool)], set(), set())
        self.assertEqual((found, changed), ([], 0))

    def test_specks_go_and_a_subjects_box_inside_a_photometric_box_is_not_counted_twice(self):
        grid = masks.Grid(64, 48)
        residual, photometric = maps(grid, [(10, 10, 30, 30), (50, 40, 51, 41)])
        before, after = 0, rect(grid, 12, 12, 28, 28)
        found, _, _ = regions.find(grid, residual, photometric, [("tub", before, after)], set(), set())
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["label"], "tub changed")
        # The cleaning grows a box by one pixel round: 20 by 20 changed of 22 by 22, each by
        # 2 over a saturation of 3; and the tub's 18 by 18, grown the same, its disagreement.
        strength, share = 2.0 * 400 / 484 / 3.0, 324 / 484
        self.assertEqual(found[0]["score"], round(0.5 * strength + 0.5 * max(strength, share), 3))

    def test_a_subjects_flip_with_nothing_changed_in_the_pixels_scores_low_and_names_its_side(self):
        grid = masks.Grid(64, 48)
        residual, photometric = maps(grid)
        before = rect(grid, 10, 10, 30, 30)
        found, _, _ = regions.find(grid, residual, photometric, [("pool", before, 0)], set(), set())
        self.assertEqual([(r["label"], r["score"]) for r in found], [("pool removed", 0.0)])

    def test_hints_move_scores_by_exactly_their_factors(self):
        grid = masks.Grid(64, 48)
        residual, photometric = maps(grid, [(10, 10, 30, 30)], residual=1.2)
        on = rect(grid, 10, 10, 30, 30)
        subjects = [("wall", on, on)]
        plain = regions.find(grid, residual, photometric, subjects, set(), set())[0][0]["score"]
        kept = regions.find(grid, residual, photometric, subjects, {"wall"}, set())[0][0]["score"]
        dropped = regions.find(grid, residual, photometric, subjects, set(), {"wall"})[0][0]["score"]
        strength = 1.2 * 400 / 484 / 3.0
        self.assertEqual(plain, round(strength, 3))
        self.assertEqual((kept, dropped), (round(strength * regions.PRESERVE, 3), round(strength * regions.IGNORE, 3)))

    def test_the_cap_keeps_the_strongest_twelve(self):
        grid = masks.Grid(160, 120)
        boxes = [(10 + 30 * (k % 5), 10 + 30 * (k // 5), 20 + 30 * (k % 5), 20 + 30 * (k // 5)) for k in range(15)]
        values = [0.0] * grid.n
        m = 0
        for k, (x0, y0, x1, y1) in enumerate(boxes):
            m |= rect(grid, x0, y0, x1, y1)
            for y in range(y0, y1):
                for x in range(x0, x1):
                    values[y * grid.w + x] = 0.2 * (k + 1)
        found = regions.find(grid, values, m, [], set(), set())[0]
        self.assertEqual(len(found), 15)
        scores = [r["score"] for r in found]
        self.assertEqual(scores, sorted(scores, reverse=True))
        # The run keeps the twelve strongest of them, its declared bound.
        self.assertEqual(regions.MAX_REGIONS, 12)
        with open(TOOL, encoding="utf-8") as file:
            self.assertIn("# max_regions = 12\n", file.read())


# --- the record's examples ------------------------------------------------------------------


def scene(blocks, shift=0, width=160, height=120):
    """A lawn of a checked texture with red blocks (a pool and what is added), each block
    (left, top, right, bottom); `shift` moves the lawn's texture across, as a picture drawn
    again would."""
    rgba = bytearray()
    for y in range(height):
        for x in range(width):
            if any(left <= x < right and top <= y < bottom for left, top, right, bottom in blocks):
                v = 30 if (x // 3 + y // 3) % 2 else 0
                rgba += bytes([200 + v // 2, 40 + v, 40 + v, 255])
            else:
                v = 30 if ((x + shift) // 4 + y // 4) % 2 else 0
                rgba += bytes([60 + v, 90 + v, 60 + v, 255])
    return tool_worker.Image("", width, height, bytes(rgba))


POOL = (40, 40, 120, 80)
EXAMPLES = {
    "source": scene([POOL]),
    "tub": scene([POOL, (125, 20, 150, 40)]),
    "redrawn": scene([POOL], shift=4),
}


class Record(unittest.TestCase):
    def test_the_records_examples_are_the_tools_readings_on_its_pictures(self):
        folder = os.path.join(os.path.dirname(RECORD), "regions")
        pictures = {}
        for name, image in EXAMPLES.items():
            pictures[name] = decoded(os.path.join(folder, name + ".png"))
            self.assertEqual((pictures[name].width, pictures[name].height, pictures[name].rgba), (image.width, image.height, image.rgba), name)
        read, found = [], []
        for example in ("tub", "redrawn"):
            output = regions.run(Faked(pictures["source"], pictures[example], {"job": job("The pool keeps its shape.")}))
            v = output["readings"][0]["values"]
            read.append([example] + ["%.3f" % v[k] for k in ("strongest", "changed_fraction", "photometric_fraction", "subject_disagreement")] + ["%d" % v["candidates"]])
            for region in output["regions"]:
                assert_region_held(self, region, pictures[example], regions.LABEL_MOST)
                found.append([example, " ".join("%g" % e for e in region["box"]), region["label"], "%.3f" % region["score"]])
        self.assertEqual(record_rows("read", RECORD), read)
        self.assertEqual(record_rows("regions", RECORD), found)


if __name__ == "__main__":
    unittest.main()
