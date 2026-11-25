"""A plan read strictly; a set built into its two halves for each family kind; one item made
again. Ported from squint `scripts/sets/tests/test_build.py`, `test_kinds.py`,
`test_subject.py` and `test_rebuild.py`, on plans made up for these tests."""

import hashlib
import os
import tomllib
import unittest
from pathlib import Path

import kit

from whizbang_sets import build, generators
from whizbang_sets import plan as plan_mod


def built(scratch, name, gens=None):
    plan = plan_mod.parse(kit.plan_text(name))
    out = Path(scratch) / "evalsets" / plan.set.id
    owner = Path(scratch) / "owner"
    report = build.build(plan, out, owner, gens or generators.fake())
    return plan, out, owner / plan.set.id, report


def read(path):
    return tomllib.loads(Path(path).read_text())


def drafts(owner_dir):
    return {label["item"]: label for label in read(owner_dir / "labels-draft.toml")["label"]}


class Counting(generators.FakeGenerator):
    def __init__(self):
        self.edits = 0

    def edit(self, image, prompt, seed):
        self.edits += 1
        return super().edit(image, prompt, seed)


class Plans(unittest.TestCase):
    def test_a_plan_is_read_and_an_unknown_key_anywhere_is_refused(self):
        text = kit.plan_text("source-fidelity")
        plan = plan_mod.parse(text)
        self.assertEqual(plan.families[0].items[0].id, "boathouse-grade")
        for old, new, said in (
            ('split = "dev"', 'split = "dev"\nexpected = "pass"', "expected"),
            ('id = "boathouse-mirror"', 'id = "boathouse-mirror"\nlabel = "fail"', "label"),
            ('split = "dev"', 'split = "practice"', "split"),
            ('generator = "openai"', 'generator = "dalle"', "generator must be one of"),
            ('id = "boathouse-crop"', 'id = "boathouse-mirror"', "duplicate"),
        ):
            with self.assertRaisesRegex(ValueError, said):
                plan_mod.parse(text.replace(old, new, 1))

    def test_a_subject_item_from_an_unknown_item_is_refused(self):
        text = kit.plan_text("reference-consistency").replace('from = "moss-library"', 'from = "nowhere"')
        with self.assertRaisesRegex(ValueError, "nowhere"):
            plan_mod.parse(text)


class Build(unittest.TestCase):
    def test_a_scene_family_builds_the_set_folder_and_the_worksheet(self):
        with kit.Scratch() as s:
            gens = generators.fake()
            gens["fake"], gens["openai"] = Counting(), Counting()
            plan, out, owner_dir, report = built(s, "source-fidelity", gens)
            self.assertEqual((report.items, report.images), (10, 11))
            # The family's generator made the source; the item's own made its edit.
            self.assertEqual((gens["fake"].edits, gens["openai"].edits), (2, 1))
            manifest = read(out / "set.toml")
            self.assertEqual(manifest["set"]["labels"], "labels.toml")
            self.assertEqual(len(manifest["item"]), 10)
            self.assertFalse((out / "labels.toml").exists(), "no answer is public before promote")
            job_text = (out / "items/boathouse-grade/job.toml").read_text()
            self.assertNotIn("expected", job_text)
            job = tomllib.loads(job_text)["job"]
            roles = {i["id"]: i for i in job["images"]}
            self.assertEqual(roles["source"]["source"]["path"], "families/boathouse/source.png")
            for img in job["images"]:
                data = (out / img["source"]["path"]).read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), img["source"]["sha256"])
            self.assertEqual(len(job["requirements"]), 2)
            d = drafts(owner_dir)
            self.assertEqual((d["boathouse-grade"]["expected"], d["boathouse-grade"]["status"]), ("pass", "draft"))
            self.assertEqual(d["boathouse-no-boat"]["obligation_ids"], ["SF1", "RQ1"])
            self.assertEqual(d["boathouse-no-boat"]["regions"][0]["kind"], "bbox")
            self.assertEqual(d["boathouse-tilt"]["obligation_ids"], ["SF2"])
            record = read(owner_dir / "construction.toml")
            self.assertEqual(record["family"][0]["generator"], "fake")
            self.assertEqual(record["item"][-1]["generator"], "openai")
            self.assertTrue((owner_dir / "contact-boathouse.png").exists())

    def test_a_reality_check_job_is_the_candidate_alone(self):
        with kit.Scratch() as s:
            _, out, owner_dir, report = built(s, "reality-checks")
            job = read(out / "items/bakery-counter-board/job.toml")["job"]
            self.assertEqual([i["role"] for i in job["images"]], ["candidate"])
            self.assertNotIn("requirements", job)
            d = drafts(owner_dir)
            self.assertEqual(d["bakery-counter-clean"]["expected"], "pass")
            self.assertEqual(d["bakery-counter-board"]["obligation_ids"], ["RE3"])
            self.assertEqual(d["bakery-counter-hand"]["obligation_ids"], ["RE1"])

    def test_a_brief_family_generates_from_the_brief_or_from_one_altered(self):
        with kit.Scratch() as s:
            _, out, owner_dir, report = built(s, "brief-compliance")
            self.assertEqual((report.items, report.images), (3, 3))
            job = read(out / "items/kite-beach-asked/job.toml")["job"]
            kinds = [(rq["id"], rq["kind"]) for rq in job["requirements"]]
            self.assertEqual(kinds, [("RQ1", "must_hold"), ("RQ2", "must_hold"), ("RQ3", "must_hold"), ("RQ4", "must_not")])
            d = drafts(owner_dir)
            self.assertEqual(d["kite-beach-no-windbreak"]["obligation_ids"], ["BC1", "RQ2"])
            self.assertEqual(d["kite-beach-misspelt"]["failure_types"], ["text-misspelt"])

    def test_subject_families_under_reference_consistency_and_product_fidelity(self):
        with kit.Scratch() as s:
            _, out, owner_dir, report = built(s, "reference-consistency")
            self.assertEqual((report.items, report.images), (3, 5))
            job = read(out / "items/moss-library/job.toml")["job"]
            self.assertEqual(job["requirements"][0]["evidence"], ["ref:moss-1", "ref:moss-2"])
            d = drafts(owner_dir)
            self.assertEqual(d["moss-library"]["expected"], "pass")
            self.assertEqual(d["moss-library-collar"]["obligation_ids"], ["RC2", "RQ2"])
            self.assertEqual(d["moss-library-lookalike"]["obligation_ids"], ["RC3", "RC1"])
            _, out, owner_dir, _ = built(s, "product-fidelity")
            job = read(out / "items/teapot-shelf/job.toml")["job"]
            self.assertTrue(job["requirements"][0]["text"].endswith("matches the reference exactly."))
            d = drafts(owner_dir)
            self.assertEqual(d["teapot-shelf-band"]["obligation_ids"], ["PF3", "RQ1"])
            self.assertEqual(d["teapot-table-lookalike"]["obligation_ids"], ["PF1", "PF2"])

    def test_owner_storage_inside_a_repository_is_refused_before_anything_is_written(self):
        with kit.Scratch() as s:
            os.mkdir(os.path.join(s, ".git"))
            plan = plan_mod.parse(kit.plan_text("reality-checks"))
            out = Path(s) / "public" / plan.set.id
            with self.assertRaisesRegex(ValueError, "inside a repository"):
                build.build(plan, out, Path(s) / "owner", generators.fake())
            self.assertFalse(out.exists())
        with self.assertRaisesRegex(ValueError, "inside a repository"):
            build.outside_every_repository(Path(kit.HERE), "--owner")


class Rebuild(unittest.TestCase):
    def test_one_item_is_made_again_and_the_rest_is_left_alone(self):
        with kit.Scratch() as s:
            plan, out, owner_dir, _ = built(s, "reference-consistency")
            ref = (out / "families/moss/ref-1.png").read_bytes()
            other = (out / "items/moss-library/candidate.png").read_bytes()
            draft_path = owner_dir / "labels-draft.toml"
            draft_path.write_text(draft_path.read_text().replace('status = "draft"', 'status = "confirmed"'))
            changed = kit.plan_text("reference-consistency").replace("Make the collar red.", "Make the collar blue.")
            report = build.rebuild(plan_mod.parse(changed), out, Path(s) / "owner", generators.fake(), "moss-library-collar", 5)
            self.assertEqual(report.items, 1)
            self.assertEqual((out / "families/moss/ref-1.png").read_bytes(), ref)
            self.assertEqual((out / "items/moss-library/candidate.png").read_bytes(), other)
            record = read(owner_dir / "construction.toml")
            entry = next(i for i in record["item"] if i["id"] == "moss-library-collar")
            self.assertIn("collar blue", entry["op"]["prompt"])
            self.assertEqual(entry["seed_offset"], 5)
            self.assertEqual([i["id"] for i in record["item"]], ["moss-library", "moss-library-collar", "moss-library-lookalike"])
            d = drafts(owner_dir)
            self.assertEqual(d["moss-library-collar"]["status"], "draft")
            self.assertEqual(d["moss-library"]["status"], "confirmed")
            with self.assertRaisesRegex(ValueError, "nope"):
                build.rebuild(plan_mod.parse(changed), out, Path(s) / "owner", generators.fake(), "nope")

    def test_an_item_renamed_in_the_plan_replaces_the_old_one_everywhere(self):
        with kit.Scratch() as s:
            _, out, owner_dir, _ = built(s, "reference-consistency")
            renamed = kit.plan_text("reference-consistency").replace('id = "moss-library-lookalike"', 'id = "moss-garden-lookalike"')
            build.rebuild(plan_mod.parse(renamed), out, Path(s) / "owner", generators.fake(), "moss-garden-lookalike")
            want = ["moss-library", "moss-library-collar", "moss-garden-lookalike"]
            self.assertEqual([i["id"] for i in read(out / "set.toml")["item"]], want)
            self.assertFalse((out / "items/moss-library-lookalike").exists())
            self.assertEqual([i["id"] for i in read(owner_dir / "construction.toml")["item"]], want)
            self.assertEqual(list(drafts(owner_dir)), want)

    def test_a_scene_item_is_made_again_from_the_source_on_disk(self):
        with kit.Scratch() as s:
            plan, out, owner_dir, _ = built(s, "source-fidelity")
            before = (out / "items/boathouse-grade/candidate.png").read_bytes()
            build.rebuild(plan, out, Path(s) / "owner", generators.fake(), "boathouse-no-boat", 3)
            self.assertEqual((out / "items/boathouse-grade/candidate.png").read_bytes(), before)
            self.assertEqual(drafts(owner_dir)["boathouse-no-boat"]["obligation_ids"], ["SF1", "RQ1"])


if __name__ == "__main__":
    unittest.main()
