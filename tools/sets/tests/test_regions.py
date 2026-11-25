"""An edit's draft region is the dense change, not the sparse noise of a regeneration.
Ported from squint `scripts/sets/tests/test_regions.py`."""

import random
import unittest

import kit
from PIL import Image

from whizbang_sets import regions


class Regions(unittest.TestCase):
    def test_a_dense_patch_is_boxed_and_sparse_noise_is_left_out(self):
        rng = random.Random(1)
        before = Image.new("RGB", (400, 300), (120, 120, 120))
        after = before.copy()
        after.paste((250, 30, 30), (200, 100, 260, 140))
        px = after.load()
        for _ in range(int(400 * 300 * 0.03)):  # 3% of the pixels moved by 90
            x, y = rng.randrange(400), rng.randrange(300)
            r, g, b = px[x, y]
            px[x, y] = (max(0, min(255, r + rng.choice((-90, 90)))), g, b)
        box = regions.difference_bbox(kit.png(before), kit.png(after))
        self.assertIsNotNone(box, "the noise must not make the region the whole picture")
        x0, y0, x1, y1 = box
        self.assertLess(abs(x0 - 0.5), 0.04)
        self.assertLess(abs(y0 - 100 / 300), 0.05)
        self.assertLess(abs(x1 - 0.65), 0.04)
        self.assertLess(abs(y1 - 140 / 300), 0.05)

    def test_a_second_dense_patch_is_a_secondary_change(self):
        before = Image.new("RGB", (400, 300), (120, 120, 120))
        after = before.copy()
        after.paste((250, 30, 30), (200, 100, 260, 140))
        after.paste((30, 30, 250), (20, 20, 40, 40))
        changes = regions.difference_regions(kit.png(before), kit.png(after))
        self.assertLess(abs(changes.primary[0] - 0.5), 0.04)
        self.assertEqual(len(changes.secondary), 1)
        self.assertLess(abs(changes.secondary[0][0] - 0.05), 0.04)

    def test_no_change_and_a_change_of_everything_are_none(self):
        before = Image.new("RGB", (100, 100), (10, 200, 10))
        self.assertIsNone(regions.difference_bbox(kit.png(before), kit.png(before)))
        after = Image.new("RGB", (100, 100), (200, 10, 10))
        self.assertIsNone(regions.difference_bbox(kit.png(before), kit.png(after)))
        self.assertTrue(regions.difference_regions(kit.png(before), kit.png(after)).whole)


if __name__ == "__main__":
    unittest.main()
