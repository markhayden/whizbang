"""`promote` publishes only what the owner confirmed and seals the set; a blind set's labels
leave it. The seal is the one `whizbang eval sets check` verifies: shown here against the
seals of the sets already in `evalsets/`, read and never changed. Ported from squint
`scripts/sets/tests/test_build.py` (promote) and squint's seal (`crates/squint-eval`)."""

import glob
import hashlib
import os
import tomllib
import unittest
from pathlib import Path

import kit

from whizbang_sets import build, generators, seal
from whizbang_sets import plan as plan_mod


def read(path):
    return tomllib.loads(Path(path).read_text())


def built(s, name, plan_text=None):
    plan = plan_mod.parse(plan_text or kit.plan_text(name))
    out = Path(s) / "evalsets" / plan.set.id
    build.build(plan, out, Path(s) / "owner", generators.fake())
    return out, Path(s) / "owner" / plan.set.id / "labels-draft.toml"


class Seal(unittest.TestCase):
    def test_the_seal_of_every_set_in_the_repository_is_made_again_exactly(self):
        manifests = sorted(glob.glob(os.path.join(kit.REPO, "evalsets", "*", "*", "set.toml")))
        self.assertGreater(len(manifests), 10)
        for manifest in manifests:
            folder = Path(manifest).parent
            sealed = read(folder / "seal.toml")["seal"]
            made = seal.components(folder)
            self.assertEqual(made, sealed["components"], folder.name)
            self.assertEqual(seal.overall(made), sealed["sha256"], folder.name)

    def test_promote_publishes_the_confirmed_labels_drops_the_struck_items_and_seals(self):
        with kit.Scratch() as s:
            out, draft = built(s, "reality-checks")
            text = draft.read_text().replace('status = "draft"', 'status = "confirmed"', 2)
            draft.write_text(text.replace('status = "draft"', 'status = "struck"', 1))
            report = seal.promote(draft, out, "a dry run")
            self.assertEqual((report.confirmed, report.struck), (2, 1))
            labels = read(out / "labels.toml")["label"]
            self.assertEqual([l["item"] for l in labels], ["bakery-counter-clean", "bakery-counter-board"])
            self.assertNotIn("status", labels[0])
            self.assertEqual(labels[0]["adjudicated_on"], build.today())
            self.assertEqual(len(read(out / "set.toml")["item"]), 2)
            self.assertFalse((out / "items/bakery-counter-hand").exists())
            sealed = read(out / "seal.toml")["seal"]
            self.assertEqual(sealed["attestation"], "a dry run")
            self.assertEqual(sealed["components"], seal.components(out))

    def test_a_label_still_a_draft_is_refused_and_nothing_changes(self):
        with kit.Scratch() as s:
            out, draft = built(s, "reality-checks")
            before = (out / "set.toml").read_text()
            with self.assertRaisesRegex(ValueError, "still a draft"):
                seal.promote(draft, out, "a dry run")
            self.assertEqual((out / "set.toml").read_text(), before)
            self.assertFalse((out / "labels.toml").exists())

    def test_a_blind_set_s_labels_leave_it_for_owner_storage(self):
        with kit.Scratch() as s:
            out, draft = built(s, "blind")
            draft.write_text(draft.read_text().replace('status = "draft"', 'status = "confirmed"'))
            with self.assertRaisesRegex(ValueError, "--labels-out"):
                seal.promote(draft, out, "a dry run")
            with self.assertRaisesRegex(ValueError, "inside a repository"):
                seal.promote(draft, out, "a dry run", Path(kit.HERE))
            # The draft, copied into a work tree, is not read from there.
            os.makedirs(os.path.join(s, "repo", ".git"))
            inside = Path(s) / "repo" / "labels-draft.toml"
            inside.write_bytes(draft.read_bytes())
            with self.assertRaisesRegex(ValueError, "--draft is inside a repository"):
                seal.promote(inside, out, "a dry run", Path(s) / "owner" / "elsewhere")
            away = Path(s) / "owner" / "blind-labels"
            seal.promote(draft, out, "a dry run", away)
            self.assertFalse((out / "labels.toml").exists())
            meta = read(out / "set.toml")["set"]
            self.assertNotIn("labels", meta)
            digest = hashlib.sha256((away / "labels.toml").read_bytes()).hexdigest()
            self.assertEqual(meta["labels_sha256"], digest)
            self.assertEqual(read(out / "seal.toml")["seal"]["components"]["labels"], digest)
            with self.assertRaisesRegex(ValueError, "already holds"):
                seal.promote(draft, out, "again", away)

    def test_labels_out_is_refused_for_a_set_that_is_not_blind(self):
        with kit.Scratch() as s:
            out, draft = built(s, "reality-checks")
            with self.assertRaisesRegex(ValueError, "blind set only"):
                seal.promote(draft, out, "a dry run", Path(s) / "away")


if __name__ == "__main__":
    unittest.main()
