"""Full requirements (plan mE, task B1, item 5): a protected subject's stated colour, count
and lettering written into its requirement in the form `per_subject` parses, and a plain
twin from the same plan; and the seasonal cover pair (item 9)."""

import importlib.util
import os
import tomllib
import unittest
from pathlib import Path

import kit

from whizbang_sets import build, generators
from whizbang_sets import plan as plan_mod


def per_subject():
    spec = importlib.util.spec_from_file_location("per_subject_read", os.path.join(kit.REPO, "pack", "tools", "per_subject.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def job(out, item):
    return tomllib.loads((out / "items" / item / "job.toml").read_text())["job"]


def built(s, text, name):
    plan = plan_mod.parse(text)
    out = Path(s) / name / plan.set.id
    build.build(plan, out, Path(s) / name / "owner", generators.fake())
    return out, Path(s) / name / "owner" / plan.set.id


class Requirements(unittest.TestCase):
    def test_a_stated_trait_is_a_clause_of_its_own_and_the_plain_twin_has_none(self):
        full = kit.plan_text("source-fidelity")
        plain = full.replace('split = "dev"', 'split = "dev"\nrequirements = "plain"')
        with kit.Scratch() as s:
            a, _ = built(s, full, "full")
            b, _ = built(s, plain, "plain")
            text = job(a, "boathouse-grade")["requirements"][0]["text"]
            self.assertEqual(
                text,
                "The blue rowing boat keeps its identity, shape, position and extent relative to the "
                "source, and its colour (blue), its count (one) and its lettering (WREN).",
            )
            self.assertEqual(
                job(b, "boathouse-grade")["requirements"][0]["text"],
                "The blue rowing boat keeps its identity, shape, position and extent relative to the source.",
            )
            self.assertEqual(job(a, "boathouse-grade")["requirements"][1], job(b, "boathouse-grade")["requirements"][1])
            # The same pictures in both twins.
            self.assertEqual(
                (a / "items/boathouse-tilt/candidate.png").read_bytes(), (b / "items/boathouse-tilt/candidate.png").read_bytes()
            )

    def test_per_subject_reads_the_same_subjects_from_both_twins_and_none_is_not_applicable(self):
        tool = per_subject()
        full = kit.plan_text("source-fidelity")
        plain = full.replace('split = "dev"', 'split = "dev"\nrequirements = "plain"')
        with kit.Scratch() as s:
            a, _ = built(s, full, "full")
            b, _ = built(s, plain, "plain")
            for item in ("boathouse-grade", "boathouse-no-boat"):
                named_a, na_a = tool.subjects_of(job(a, item))
                named_b, na_b = tool.subjects_of(job(b, item))
                self.assertEqual(named_a, ["blue rowing boat", "wooden jetty"])
                self.assertEqual((named_a, na_a), (named_b, na_b))
                self.assertEqual(na_a, [])

    def test_traits_are_for_a_scene_family_and_known_ones_only(self):
        text = kit.plan_text("source-fidelity")
        for old, new in (
            ('lettering = "WREN"', 'lettering = "WREN", size = "small"'),
            ('lettering = "WREN"', 'lettering = ""'),
            ('split = "dev"', 'split = "dev"\nrequirements = "fuller"'),
        ):
            with self.assertRaises(ValueError, msg=new):
                plan_mod.parse(text.replace(old, new, 1))
        subject = kit.plan_text("reference-consistency").replace('"Moss\'s yellow collar"', '{ subject = "the collar", colour = "yellow" }')
        with self.assertRaisesRegex(ValueError, "scene family"):
            plan_mod.parse(subject)


class Cover(unittest.TestCase):
    def test_the_seasonal_cover_pair_is_two_jobs_on_the_same_pictures_a_pass_and_a_fail(self):
        with open(os.path.join(kit.TOOL, "plans", "seasonal-cover-v1.plan.toml"), encoding="utf-8") as f:
            text = f.read()
        with kit.Scratch() as s:
            plan = plan_mod.parse(text)
            out = Path(s) / plan.set.id
            report = build.build(plan, out, Path(s) / "owner", generators.fake())
            self.assertEqual((report.items, report.images), (2, 3))
            yes, no = job(out, "garden-bench-snow-permitted"), job(out, "garden-bench-snow-not-permitted")
            pins = lambda j: {i["id"]: i["source"]["sha256"] for i in j["images"]}  # noqa: E731
            self.assertEqual(pins(yes), pins(no), "the same pictures")
            self.assertIn("seasonal cover", yes["brief"])
            self.assertNotIn("seasonal cover", no["brief"])
            self.assertIn("its colour (green) and its count (one)", yes["requirements"][0]["text"])
            draft = tomllib.loads((Path(s) / "owner" / plan.set.id / "labels-draft.toml").read_text())
            by = {label["item"]: label for label in draft["label"]}
            self.assertEqual(by["garden-bench-snow-permitted"]["expected"], "pass")
            self.assertEqual(by["garden-bench-snow-not-permitted"]["expected"], "fail")
            self.assertEqual(by["garden-bench-snow-not-permitted"]["obligation_ids"], ["SF1", "RQ1"])
            with self.assertRaisesRegex(ValueError, "build the set again"):
                build.rebuild(plan, out, Path(s) / "owner", generators.fake(), "garden-bench-snow-permitted")

    def test_same_as_names_an_earlier_cover_and_is_one(self):
        with open(os.path.join(kit.TOOL, "plans", "seasonal-cover-v1.plan.toml"), encoding="utf-8") as f:
            text = f.read()
        for old, new in (
            ('same_as = "garden-bench-snow-permitted"', 'same_as = "garden-bench-later"'),
            ('permitted = false', 'permitted = "no"'),
        ):
            with self.assertRaises(ValueError, msg=new):
                plan_mod.parse(text.replace(old, new, 1))


if __name__ == "__main__":
    unittest.main()
