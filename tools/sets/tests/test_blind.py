"""What the program says of a blind set (plan mE, task B1, item 8): counts and the set's id,
never an item's id, a path, a prompt, a label, a plan's text or a provider's answer; the rest
is kept in owner storage. And the drawer of a blind plan, and `pin`."""

import contextlib
import io
import os
import tomllib
import unittest
from pathlib import Path

import kit
from test_egress import TOKEN, Server
from test_spend import prices_file

from whizbang_sets import __main__ as cli
from whizbang_sets import generators, seal

MARK = "QUIETMARK"  # planted in what must not be printed


def run(*argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = cli.main(list(argv))
    return code, out.getvalue() + err.getvalue()


class Said(unittest.TestCase):
    def silent(self, said, s, also=()):
        for held in ("orchard-gate", s, MARK, "Take the gate away", *also):
            self.assertNotIn(held, said)

    def blind_plan(self, s, text=None):
        path = os.path.join(s, "owner", "blind.plan.toml")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text or kit.plan_text("blind"))
        return path

    def test_a_blind_build_says_counts_and_the_set_s_id_alone(self):
        with kit.Scratch() as s:
            plan = self.blind_plan(s)
            code, said = run("build", "--plan", plan, "--out", f"{s}/sets/x", "--owner", f"{s}/owner", "--fake")
            self.assertEqual(code, 0, said)
            self.assertIn("source-fidelity/dry-run-blind-v1: 1 families, 2 items", said)
            self.silent(said, s)

    def test_a_bad_blind_plan_is_said_by_its_kind_and_kept_in_owner_storage(self):
        with kit.Scratch() as s:
            text = kit.plan_text("blind").replace('id = "orchard-gate-2"', f'id = "orchard-gate-2"\nlabel = "{MARK}"')
            plan = self.blind_plan(s, text)
            code, said = run("build", "--plan", plan, "--out", f"{s}/sets/x", "--owner", f"{s}/owner", "--fake")
            self.assertEqual(code, 1)
            self.assertIn("the plan, the set or an argument was refused; its text is kept in owner storage", said)
            self.silent(said, s, ("label",))
            log = Path(s, "owner", "source-fidelity", "dry-run-blind-v1", "messages.log").read_text()
            self.assertIn("orchard-gate", log)
            # One that is not even a plan is taken as blind too.
            with open(plan, "w") as f:
                f.write(f"[set\n{MARK}")
            code, said = run("build", "--plan", plan, "--out", f"{s}/sets/x", "--owner", f"{s}/owner", "--fake")
            self.assertEqual(code, 1)
            self.silent(said, s)

    def test_a_provider_s_failure_is_said_by_its_status_alone(self):
        os.environ["FAL_KEY"] = TOKEN
        old = generators.FalKlein.base
        try:
            with kit.Scratch() as s, Server(body=f'{{"error": "{MARK} orchard-gate"}}'.encode()) as provider:
                provider.httpd.status = 400
                generators.FalKlein.base = provider.origin
                config = os.path.join(s, "whizbang.toml")
                with open(config, "w") as f:
                    f.write(f'[egress.recipients.fal]\norigin = "{provider.origin}"\nclasses = ["prompts", "source_images"]\n')
                plan = self.blind_plan(s, kit.plan_text("blind").replace('generator = "fake"', 'generator = "fal"'))
                code, said = run(
                    "build", "--plan", plan, "--out", f"{s}/sets/x", "--owner", f"{s}/owner",
                    "--config", config, "--ledger", f"{s}/owner/ledger.jsonl", "--cap-usd", "1",
                    "--prices", prices_file(s, {"fal": 0.01}),
                )
                self.assertEqual(code, 1)
                self.assertIn("a provider's request failed: HTTP 400; its text is kept in owner storage", said)
                self.assertIn("the ledger holds 0.010000 of its 1.000000 dollars", said)
                self.silent(said, s, ("error",))
                log = Path(s, "owner", "source-fidelity", "dry-run-blind-v1", "messages.log").read_text()
                self.assertIn(MARK, log, "the provider's answer is kept in owner storage")
                # Under a dev set too, a provider's answer is never printed.
                dev = kit.plan_text("reality-checks").replace('generator = "fake"', 'generator = "fal"')
                path = os.path.join(s, "dev.plan.toml")
                with open(path, "w") as f:
                    f.write(dev)
                code, said = run(
                    "build", "--plan", path, "--out", f"{s}/sets/y", "--owner", f"{s}/owner",
                    "--config", config, "--ledger", f"{s}/owner/ledger.jsonl", "--prices", prices_file(s, {"fal": 0.01}),
                )
                self.assertEqual(code, 1)
                self.assertIn("HTTP 400", said)
                self.assertNotIn(MARK, said)
        finally:
            generators.FalKlein.base = old
            os.environ.pop("FAL_KEY", None)

    def test_a_blind_promote_with_a_label_unreviewed_says_nothing_of_it(self):
        with kit.Scratch() as s:
            plan = self.blind_plan(s)
            run("build", "--plan", plan, "--out", f"{s}/sets/source-fidelity/b", "--owner", f"{s}/owner", "--fake")
            draft = f"{s}/owner/source-fidelity/dry-run-blind-v1/labels-draft.toml"
            code, said = run("promote", "--draft", draft, "--set", f"{s}/sets/source-fidelity/b",
                             "--attest", "x", "--labels-out", f"{s}/owner/labels")
            self.assertEqual(code, 1)
            self.assertIn("refused; its text is kept in owner storage", said)
            self.silent(said, s, ("draft",))

    def test_a_live_build_without_a_ledger_is_refused_before_anything_is_sent(self):
        with kit.Scratch() as s:
            code, said = run("build", "--plan", os.path.join(kit.PLANS, "reality-checks.plan.toml"),
                             "--out", f"{s}/x/y", "--owner", f"{s}/owner", "--config", f"{s}/none.toml")
            self.assertEqual(code, 1)
            self.assertIn("--ledger", said)


POOL = """
[set]
id = "source-fidelity/drawn-v1"
use_case = "source-fidelity"
licence = "CC-BY-4.0"
rules = { base = "source-fidelity", version = 1 }

[draw]
items_per_family = 6
generators = ["fal", "gemini"]

[[family]]
id = "quayside"
description = "a quayside with a red crane and a moored barge"
brief = "Restyle the scene; keep the structure and every named subject."
source = { prompt = "A quayside with a red crane and a moored barge.", seed = 77 }
protected = [{ subject = "the red crane", colour = "red" }, "the moored barge"]
edits = [{ prompt = "Take the crane away. Change nothing else.", removes = "red crane" }]
restyles = [{ prompt = "Make it a foggy morning. Keep everything where it is." }]

[[family]]
id = "hill-chapel"
generator = "openai"
description = "a white chapel on a hill with a bell"
brief = "Restyle the scene; keep the structure and every named subject."
source = { prompt = "A white chapel on a green hill with one bell.", seed = 78 }
protected = ["the white chapel"]
edits = [{ prompt = "Paint the chapel blue. Change nothing else.", alters = "white chapel" }]
"""


class Draw(unittest.TestCase):
    def pool(self, s):
        path = os.path.join(s, "owner", "pool.toml")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(POOL)
        return path

    def test_a_seed_draws_one_blind_plan_with_opaque_ids_and_counts_alone_are_said(self):
        with kit.Scratch() as s:
            pool = self.pool(s)
            code, said = run("draw", "--pool", pool, "--seed", "7", "--out", f"{s}/owner/a.plan.toml")
            self.assertEqual((code, said.strip()), (0, "drew a blind plan: 2 families, 12 items, in owner storage"))
            run("draw", "--pool", pool, "--seed", "7", "--out", f"{s}/owner/b.plan.toml")
            run("draw", "--pool", pool, "--seed", "8", "--out", f"{s}/owner/c.plan.toml")
            a, b, c = (Path(s, "owner", f"{n}.plan.toml").read_text() for n in "abc")
            self.assertEqual(a, b)
            self.assertNotEqual(a, c)
            plan = tomllib.loads(a)
            self.assertEqual(plan["set"]["split"], "blind")
            for family in plan["family"]:
                ops = [i["op"]["kind"] for i in family["item"]]
                self.assertEqual([i["id"] for i in family["item"]], [f"{family['id']}-{n}" for n in range(1, 7)])
                self.assertTrue(any(k in ("grade", "restyle") for k in ops), ops)
                self.assertIn("edit", ops)
                self.assertTrue(any(k in ("mirror", "rotate", "crop", "tilt", "zoom", "keystone") for k in ops), ops)
            self.assertEqual(plan["family"][1]["generator"], "openai", "a family's own generator is kept")
            code, said = run("draw", "--pool", pool, "--seed", "7", "--out", f"{s}/owner/a.plan.toml")
            self.assertEqual(code, 1, "a drawn plan is never written over")
            self.assertNotIn("quayside", said)

    def test_a_pool_edit_cannot_name_another_kind(self):
        with kit.Scratch() as s:
            path = self.pool(s)
            with open(path, "w") as f:
                f.write(POOL.replace('removes = "red crane" }', 'removes = "red crane", kind = "grade" }'))
            code, _ = run("draw", "--pool", path, "--seed", "1", "--out", f"{s}/owner/k.plan.toml")
            self.assertEqual(code, 1)
            self.assertFalse(os.path.exists(f"{s}/owner/k.plan.toml"))

    def test_a_pool_or_a_plan_inside_a_repository_is_refused(self):
        with kit.Scratch() as s:
            pool = self.pool(s)
            code, _ = run("draw", "--pool", pool, "--seed", "1", "--out", os.path.join(kit.HERE, "drawn.plan.toml"))
            self.assertEqual(code, 1)
            self.assertFalse(os.path.exists(os.path.join(kit.HERE, "drawn.plan.toml")))


class Pin(unittest.TestCase):
    def test_pin_names_the_revision_and_leaves_the_seal_whole(self):
        with kit.Scratch() as s:
            code, _ = run("build", "--plan", os.path.join(kit.PLANS, "reality-checks.plan.toml"),
                          "--out", f"{s}/sets/reality-checks/dry-run-v1", "--owner", f"{s}/owner", "--fake")
            draft = Path(s, "owner", "reality-checks", "dry-run-v1", "labels-draft.toml")
            draft.write_text(draft.read_text().replace('status = "draft"', 'status = "confirmed"'))
            folder = Path(s, "sets", "reality-checks", "dry-run-v1")
            seal.promote(draft, folder, "x")
            sealed = (folder / "seal.toml").read_text()
            code, said = run("pin", "--set", str(folder), "--rev", "ab" * 20)
            self.assertEqual(code, 0, said)
            meta = tomllib.loads((folder / "set.toml").read_text())["set"]
            self.assertEqual(meta["images_rev"], "ab" * 20)
            self.assertEqual(meta["images_repo"], "github.com/markhayden/whizbang-evalsets")
            self.assertEqual(seal.components(folder), tomllib.loads(sealed)["seal"]["components"])
            code, _ = run("pin", "--set", str(folder), "--rev", "main")
            self.assertEqual(code, 1)
            self.assertEqual(run("pin", "--set", str(folder), "--rev", "ab" * 20)[0], 0, "the same pin again")
            code, said = run("pin", "--set", str(folder), "--rev", "cd" * 20)
            self.assertEqual(code, 1)
            self.assertIn("pinned at another revision", said)


if __name__ == "__main__":
    unittest.main()
