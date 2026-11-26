"""The spend guard (plan mE, task B1, item 7): one ledger, its allowance set once, every
attempt charged at its bound and written before it is sent, a request past the allowance not
sent; and a live build run end to end against a provider that stands in on this machine.
No real provider is called; the key is made up."""

import base64
import json
import os
import tomllib
import unittest
from pathlib import Path

import kit
from PIL import Image
from test_egress import TOKEN, Server, approved, http_error, scripted

from whizbang_sets import build, egress, generators, spend
from whizbang_sets import plan as plan_mod


def prices_file(folder, rows):
    path = os.path.join(folder, "prices.toml")
    with open(path, "w") as f:
        for name, usd in rows.items():
            f.write(f"[generator.{name}]\nusd_per_picture = {usd}\nchecked_on = 2026-11-25\n")
    return path


def entries(path):
    lines = Path(path).read_text().splitlines()
    return json.loads(lines[0]), [json.loads(line) for line in lines[1:]]


class Ledger(unittest.TestCase):
    def test_the_allowance_is_set_once_and_shared(self):
        with kit.Scratch() as s:
            path = Path(s) / "owner" / "ledger.jsonl"
            with self.assertRaisesRegex(ValueError, "--cap-usd"):
                spend.Ledger(path)
            spend.Ledger(path, 0.05).charge("fal", 20_000, "https://fal.run")
            again = spend.Ledger(path)  # a later build, after an interruption
            self.assertEqual((again.cap, again.held()), (50_000, 20_000))
            with self.assertRaisesRegex(ValueError, "set once"):
                spend.Ledger(path, 8)
            again.charge("fal", 30_000, "https://fal.run")
            with self.assertRaises(spend.SpendRefused):
                again.charge("fal", 1, "https://fal.run")
            head, lines = entries(path)
            self.assertEqual(head["cap_micros"], 50_000)
            self.assertEqual([e["micros"] for e in lines], [20_000, 30_000])
            for bad in (0, -1, 0.0000001, 101):
                with self.assertRaises(ValueError, msg=bad):
                    spend.Ledger(Path(s) / f"other-{bad}.jsonl", bad)

    def test_a_ledger_inside_a_repository_or_with_a_torn_last_line_is_refused(self):
        with self.assertRaisesRegex(ValueError, "inside a repository"):
            spend.Ledger(Path(kit.HERE) / "ledger.jsonl", 1)
        with kit.Scratch() as s:
            path = Path(s) / "ledger.jsonl"
            spend.Ledger(path, 1)
            with open(path, "a") as f:
                f.write('{"micros": 3')
            with self.assertRaisesRegex(ValueError, "not whole"):
                spend.Ledger(path)

    def test_every_attempt_a_retry_included_is_charged_before_it_is_sent_and_none_past_the_allowance(self):
        with kit.Scratch() as s:
            ledger = spend.Ledger(Path(s) / "ledger.jsonl", 0.03)
            guard = spend.Guard(ledger, {"fal": 10_000})
            sent = []
            opener, calls = scripted([http_error(503), http_error(503), b"ok", b"ok"])

            def counting(*handlers):
                class Counting:
                    def open(self, req, timeout):
                        sent.append(ledger.held())  # what the ledger held when it went
                        return opener().open(req, timeout)

                return Counting()

            a = approved(("https://fal.run", (), ("prompts",)))
            sender = egress.Sender(a, sleep=lambda w: None, opener=counting)
            charge = lambda r: guard.charge("fal", r)  # noqa: E731
            self.assertEqual(sender.post("https://fal.run/m", b"", {}, {}, ("prompts",), "m", charge)[0], b"ok")
            self.assertEqual(sent, [10_000, 20_000, 30_000], "each attempt was written before it went")
            with self.assertRaises(spend.SpendRefused):
                sender.post("https://fal.run/m", b"", {}, {}, ("prompts",), "m", charge)
            self.assertEqual(len(calls), 3, "the request past the allowance was not sent")

    def test_a_generator_without_a_dated_bound_price_is_never_used_live(self):
        with kit.Scratch() as s:
            guard = spend.Guard(spend.Ledger(Path(s) / "l.jsonl", 1), {})
            with self.assertRaisesRegex(spend.SpendRefused, "prices.toml"):
                guard.charge("gemini", egress.Recipient("g", "https://g", frozenset(), frozenset()))
            self.assertEqual(spend.prices(spend.PRICES_FILE), {}, "no price is written before the orchestrator reads one")
            with open(os.path.join(s, "bad.toml"), "w") as f:
                f.write("[generator.fal]\nusd_per_picture = 0.01\n")
            with self.assertRaisesRegex(ValueError, "checked_on"):
                spend.prices(os.path.join(s, "bad.toml"))
            self.assertEqual(spend.prices(prices_file(s, {"fal": 0.0125})), {"fal": 12_500})


def picture_answer(size=64):
    data = base64.b64encode(kit.png(Image.new("RGB", (size, size), (90, 120, 150)))).decode()
    return json.dumps({"images": [{"url": "data:image/png;base64," + data}]}).encode()


class Live(unittest.TestCase):
    def setUp(self):
        os.environ["FAL_KEY"] = TOKEN

    def tearDown(self):
        os.environ.pop("FAL_KEY", None)

    def test_a_live_build_against_a_provider_on_this_machine_charges_each_request(self):
        text = kit.plan_text("reality-checks").replace('generator = "fake"', 'generator = "fal"')
        plan = plan_mod.parse(text)
        with kit.Scratch() as s, Server(body=picture_answer()) as provider:
            a = approved((provider.origin, (), ("prompts", "source_images")))
            ledger = spend.Ledger(Path(s) / "owner" / "ledger.jsonl", 1)
            guard = spend.Guard(ledger, {"fal": 10_000})
            gens = generators.live(egress.Sender(a, sleep=lambda w: None), guard)
            gens["fal"].base = provider.origin
            report = build.build(plan, Path(s) / "evalsets" / plan.set.id, Path(s) / "owner", gens)
            self.assertEqual(report.items, 3)
            # The source, one edit and one generation: three requests, each charged once.
            self.assertEqual(len(provider.seen), 3)
            self.assertTrue(all(seen[2] == "Key " + TOKEN for seen in provider.seen))
            _, lines = entries(ledger.path)
            self.assertEqual([(e["generator"], e["micros"], e["to"]) for e in lines], [("fal", 10_000, provider.origin)] * 3)
            record = tomllib.loads((Path(s) / "owner" / plan.set.id / "construction.toml").read_text())
            self.assertEqual(record["family"][0]["model"], "fal-ai/flux-2-klein/4b")


if __name__ == "__main__":
    unittest.main()
