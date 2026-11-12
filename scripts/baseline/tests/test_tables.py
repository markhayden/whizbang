"""Tests of tables.py: the statistics on hand-made data, and the whole path on a synthetic
package built in a temporary folder. No network, no real item. Run by
scripts/tests/test-baseline.sh."""

import hashlib
import json
import os
import shutil
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
import tables  # noqa: E402

RUN = "study-synthetic-2026-11"
GATE = "gate-synthetic-2026-11"
PROFILE = "provider"
MODEL = "model-x"


def paired(rows):
    """rows: (family, item, expected, control verdict, tool verdict)."""
    expected = {r[1]: r[2] for r in rows}
    family = {r[1]: r[0] for r in rows}
    return {r[1]: r[3] for r in rows}, {r[1]: r[4] for r in rows}, expected, family


class Generator(unittest.TestCase):
    def test_the_same_seed_gives_the_same_draws_and_another_seed_others(self):
        a = [tables.Stream(7).below(5) for _ in range(1)] + [x for x in self.draws(7, 5, 40)]
        b = [tables.Stream(7).below(5) for _ in range(1)] + [x for x in self.draws(7, 5, 40)]
        self.assertEqual(a, b)
        self.assertNotEqual(self.draws(7, 5, 40), self.draws(8, 5, 40))
        self.assertTrue(all(0 <= x < 5 for x in a))

    def draws(self, seed, n, count):
        stream = tables.Stream(seed)
        return [stream.below(n) for _ in range(count)]

    def test_the_words_are_sha256_of_seed_and_counter_first_to_last(self):
        stream = tables.Stream(20261111)
        got = [stream.word() for _ in range(6)]
        want = []
        for block in (0, 1):
            digest = hashlib.sha256(("20261111:%d" % block).encode("ascii")).digest()
            want += [int.from_bytes(digest[i : i + 8], "big") for i in (0, 8, 16, 24)]
        self.assertEqual(got, want[:6])

    def test_a_draw_is_the_word_modulo_the_bound_and_never_biased(self):
        # With a bound of 2**63 + 1 nearly half of all words are dropped: every result must
        # still be below the bound, and equal the first kept word modulo it.
        bound = (1 << 63) + 1
        stream, words = tables.Stream(3), tables.Stream(3)
        for _ in range(50):
            got = stream.below(bound)
            limit = (1 << 64) - ((1 << 64) % bound)
            word = words.word()
            while word >= limit:
                word = words.word()
            self.assertEqual(got, word % bound)

    def test_a_bound_must_be_positive(self):
        with self.assertRaises(tables.TablesError):
            tables.Stream(1).below(0)


class Quantiles(unittest.TestCase):
    def test_type_7_matches_hand_values(self):
        x = [1.0, 2.0, 3.0, 4.0]
        self.assertEqual(tables.quantile_type7(x, 0.0), 1.0)
        self.assertEqual(tables.quantile_type7(x, 1.0), 4.0)
        self.assertEqual(tables.quantile_type7(x, 0.5), 2.5)
        self.assertEqual(tables.quantile_type7(x, 0.25), 1.75)
        self.assertEqual(tables.quantile_type7([7.0], 0.3), 7.0)
        self.assertAlmostEqual(tables.quantile_type7([10.0, 20.0, 30.0, 40.0, 50.0], 0.1), 14.0)

    def test_wilson_matches_a_hand_value_and_has_no_trials_case(self):
        low, high = tables.wilson(8, 10)
        self.assertAlmostEqual(low, 0.4902, places=4)
        self.assertAlmostEqual(high, 0.9433, places=4)
        self.assertEqual(tables.wilson(0, 0), (0.0, 1.0))


class Scoring(unittest.TestCase):
    def test_unsure_and_error_go_to_review_and_count_for_neither_side(self):
        expected = {"f1": "fail", "f2": "fail", "f3": "fail", "f4": "fail", "p1": "pass", "p2": "pass", "p3": "pass", "p4": "pass"}
        verdicts = {"f1": "fail", "f2": "pass", "f3": "unsure", "f4": "error", "p1": "pass", "p2": "fail", "p3": "unsure", "p4": "error"}
        c = tables.cell_counts(verdicts, expected)
        self.assertEqual((c["detected"], c["missed"], c["review_on_fail"]), (1, 1, 2))
        self.assertEqual((c["cleared"], c["false_rejects"], c["review_on_pass"]), (1, 1, 2))
        self.assertEqual((c["unsure"], c["errors"], c["items"]), (2, 2, 8))
        catch, reject, balanced = tables.rates(c)
        self.assertEqual((catch, reject, balanced), (0.25, 0.25, 0.5))

    def test_each_rate_is_over_its_own_side(self):
        expected = {"f1": "fail", "f2": "fail", "p1": "pass", "p2": "pass", "p3": "pass", "p4": "pass", "p5": "pass"}
        verdicts = {"f1": "fail", "f2": "pass", "p1": "fail", "p2": "pass", "p3": "pass", "p4": "pass", "p5": "unsure"}
        catch, reject, balanced = tables.rates(tables.cell_counts(verdicts, expected))
        self.assertEqual((catch, reject), (1 / 2, 1 / 5))
        self.assertEqual(balanced, (1 / 2 + 4 / 5) / 2)

    def test_the_middle_value_is_the_median_not_the_mean(self):
        self.assertEqual(tables.median([1, 2, 300]), 2)
        self.assertEqual(tables.median([4, 1, 2, 300]), 3.0)

    def test_a_decision_without_a_verdict_is_an_error_and_an_odd_verdict_is_refused(self):
        self.assertEqual(tables.verdict_of({"verdict": None}), "error")
        self.assertEqual(tables.verdict_of({"verdict": "unsure"}), "unsure")
        with self.assertRaises(tables.TablesError):
            tables.verdict_of({"verdict": "Fail"})

    def test_an_unlabelled_item_and_a_one_sided_cell_are_refused(self):
        with self.assertRaises(tables.TablesError):
            tables.cell_counts({"x": "fail"}, {})
        with self.assertRaises(tables.TablesError):
            tables.rates(tables.cell_counts({"x": "fail"}, {"x": "fail"}))

    def test_error_kinds_come_from_the_message(self):
        self.assertEqual(tables.error_kind("submission: finding 2: location invalid: outside"), "location")
        self.assertEqual(tables.error_kind("submission: checklist item \"a\" must appear exactly once, appeared 2 times"), "checklist")
        self.assertEqual(tables.error_kind("evidence \"t\" is not a tool call that completed in this run"), "evidence")
        self.assertEqual(tables.error_kind(None), "other")
        self.assertEqual(tables.error_kind("something new"), "other")


class Pairing(unittest.TestCase):
    ROWS = [
        ("a", "a-1", "fail", "pass", "fail"),
        ("b", "b-1", "fail", "fail", "fail"),
        ("b", "b-2", "fail", "pass", "fail"),
        ("b", "b-3", "fail", "pass", "pass"),
        ("b", "b-4", "pass", "fail", "pass"),
        ("c", "c-1", "pass", "pass", "pass"),
        ("c", "c-2", "pass", "pass", "fail"),
    ]

    def test_families_are_summed_in_id_order(self):
        base, tool, expected, family = paired(list(reversed(self.ROWS)))
        sums = tables.family_sums(base, tool, expected, family)
        self.assertEqual([(s["fails"], s["passes"]) for s in sums], [(1, 0), (3, 1), (0, 2)])
        self.assertEqual([(s["det_c"], s["det_x"]) for s in sums], [(0, 1), (1, 2), (0, 0)])
        self.assertEqual([(s["fr_c"], s["fr_x"]) for s in sums], [(0, 0), (1, 0), (0, 1)])
        self.assertEqual([s["differs"] for s in sums], [True, True, False])

    def test_the_catch_estimand_is_item_weighted_with_multiplicity(self):
        base, tool, expected, family = paired(self.ROWS)
        sums = [s for s in tables.family_sums(base, tool, expected, family) if s["fails"]]
        # a: +1 of 1; b: +1 of 3. Both once: 2/4. a twice and b once: 3/5.
        self.assertAlmostEqual(tables.catch_difference(sums, [0, 1]), 0.5)
        self.assertAlmostEqual(tables.catch_difference(sums, [0, 0, 1]), 0.6)
        self.assertEqual(tables.catch_difference(sums, []), 0.0)

    def test_the_balanced_estimand_uses_both_sides_and_is_undefined_on_one(self):
        base, tool, expected, family = paired(self.ROWS)
        sums = tables.family_sums(base, tool, expected, family)
        # catch +2 of 4, false rejects: control 1 of 3, tool 1 of 3 -> (0.5 - 0) / 2.
        self.assertAlmostEqual(tables.balanced_difference(sums, [0, 1, 2]), 0.25)
        self.assertIsNone(tables.balanced_difference(sums, [0, 0]))
        self.assertIsNone(tables.balanced_difference(sums, [2, 2]))

    def test_arms_with_different_items_are_refused(self):
        base, tool, expected, family = paired(self.ROWS)
        del tool["c-2"]
        with self.assertRaises(tables.TablesError):
            tables.family_sums(base, tool, expected, family)

    def test_an_item_without_a_family_is_refused(self):
        base, tool, expected, family = paired(self.ROWS)
        del family["a-1"]
        with self.assertRaises(tables.TablesError):
            tables.family_sums(base, tool, expected, family)

    def test_families_without_an_expected_fail_are_left_out_of_the_catch_bootstrap_only(self):
        base, tool, expected, family = paired(self.ROWS)
        e = tables.effect(base, tool, expected, family, 5, 200, 1)
        self.assertEqual((e["families"], e["families_with_expected_fail"], e["differing_families"]), (3, 2, 2))
        self.assertEqual((e["flipped_expected_fail"], e["flipped_expected_pass"]), (2, 2))
        self.assertAlmostEqual(e["catch"][0], 0.5)
        self.assertAlmostEqual(e["balanced"][0], 0.25)
        self.assertEqual(e["catch"][3], 200)
        self.assertLess(e["balanced"][3], 200, "some resamples have one side only and are left out")


class Bootstrap(unittest.TestCase):
    def families(self, diffs):
        return [{"fails": 2, "passes": 2, "det_c": 0, "det_x": d, "fr_c": 0, "fr_x": 0, "differs": d != 0} for d in diffs]

    def test_the_same_seed_gives_the_same_interval(self):
        f = self.families([0, 1, 2, 0, 1, 2, 0, 1])
        a = tables.bootstrap(f, tables.catch_difference, 20261111, 500, 2)
        b = tables.bootstrap(f, tables.catch_difference, 20261111, 500, 2)
        c = tables.bootstrap(f, tables.catch_difference, 20261112, 500, 2)
        self.assertEqual(a, b)
        self.assertNotEqual(a[1:3], c[1:3])
        self.assertLessEqual(a[1], a[0])
        self.assertLessEqual(a[0], a[2])

    def test_the_same_effect_in_every_family_gives_an_interval_of_one_point(self):
        point, low, high, used = tables.bootstrap(self.families([1, 1, 1, 1]), tables.catch_difference, 1, 300, 3)
        self.assertEqual((point, low, high, used), (0.5, 0.5, 0.5, 300))
        point, low, high, _ = tables.bootstrap(self.families([0, 0, 0]), tables.catch_difference, 1, 300, 1)
        self.assertEqual((point, low, high), (0.0, 0.0, 0.0))

    def test_more_hypotheses_widen_the_interval(self):
        f = self.families([0, 1, 2, 0, 1, 2, 0, 1, 2, 2, 0, 0])
        one = tables.bootstrap(f, tables.catch_difference, 9, 2000, 1)
        three = tables.bootstrap(f, tables.catch_difference, 9, 2000, 3)
        self.assertEqual(one[0], three[0])
        self.assertLess(three[1], one[1])
        self.assertGreater(three[2], one[2])

    def test_the_interval_is_the_type_7_quantile_of_the_resamples_at_the_corrected_percentiles(self):
        f = self.families([0, 1, 2, 0, 1])
        seed, resamples, hypotheses = 42, 400, 2
        stream = tables.Stream(seed)
        estimates = sorted(
            tables.catch_difference(f, [stream.below(len(f)) for _ in f]) for _ in range(resamples)
        )
        _, low, high, _ = tables.bootstrap(f, tables.catch_difference, seed, resamples, hypotheses)
        self.assertEqual(low, tables.quantile_type7(estimates, 0.0125))
        self.assertEqual(high, tables.quantile_type7(estimates, 0.9875))

    def test_bad_arguments_are_refused(self):
        f = self.families([1])
        for args in ((0, 1), (10, 0)):
            with self.assertRaises(tables.TablesError):
                tables.bootstrap(f, tables.catch_difference, 1, *args)
        with self.assertRaises(tables.TablesError):
            tables.bootstrap([], tables.catch_difference, 1, 10, 1)


class Flags(unittest.TestCase):
    def test_an_interval_that_only_touches_zero_does_not_exclude_it(self):
        self.assertTrue(tables.excludes_zero(0.1, 5.0))
        self.assertTrue(tables.excludes_zero(-5.0, -0.1))
        self.assertFalse(tables.excludes_zero(0.0, 5.0))
        self.assertFalse(tables.excludes_zero(-5.0, 0.0))
        self.assertFalse(tables.excludes_zero(-1.0, 1.0))
        self.assertFalse(tables.excludes_zero(0.0, 0.0))

    def test_the_level_of_an_interval_follows_the_hypotheses(self):
        self.assertEqual([tables.level_of(h) for h in (1, 2, 3, 13, 26)], [95.0, 97.5, 98.33, 99.62, 99.81])


class Finer(unittest.TestCase):
    """Thirty families of three expected-fail and two expected-pass items, with effects of
    every size, so that neighbouring quantiles and seeds give different intervals."""

    def rows(self, quiet=0):
        rows = []
        for f in range(30):
            for n in range(3):
                gain = (f * 7 + n * 3) % 5 < 2
                loss = (f + n) % 11 == 0
                rows.append(("f%02d" % f, "f%02d-%d" % (f, n), "fail", "fail" if loss else "pass", "fail" if gain else "pass"))
            for n in range(3, 5):
                rows.append(("f%02d" % f, "f%02d-%d" % (f, n), "pass", "pass", "fail" if (f + n) % 9 == 0 else "pass"))
        for f in range(quiet):
            for n in range(4):
                rows.append(("q%02d" % f, "q%02d-%d" % (f, n), "pass", "pass", "pass"))
        return rows

    def test_the_tail_is_0_025_over_the_hypotheses_and_not_0_05(self):
        base, tool, expected, family = paired(self.rows())
        sums = tables.family_sums(base, tool, expected, family)
        stream = tables.Stream(77)
        estimates = sorted(tables.catch_difference(sums, [stream.below(len(sums)) for _ in sums]) for _ in range(4000))
        _, low, high, _ = tables.bootstrap(sums, tables.catch_difference, 77, 4000, 2)
        self.assertEqual((low, high), (tables.quantile_type7(estimates, 0.0125), tables.quantile_type7(estimates, 0.9875)))
        self.assertNotEqual(low, tables.quantile_type7(estimates, 0.025))
        self.assertNotEqual(high, tables.quantile_type7(estimates, 0.975))

    def test_the_seed_given_is_the_seed_used(self):
        base, tool, expected, family = paired(self.rows())
        rows = [
            tables.effect_row("r", "m", "D_outline", base, tool, expected, family, {"seed": seed, "hypotheses": 2}, 150)
            for seed in (11, 12, 11)
        ]
        bounds = ("catch_low", "catch_high", "balanced_low", "balanced_high")
        self.assertEqual(rows[0], rows[2])
        self.assertNotEqual([rows[0][b] for b in bounds], [rows[1][b] for b in bounds])
        direct = tables.effect(base, tool, expected, family, 11, 150, 2)
        self.assertEqual(rows[0]["catch_low"], tables.points(direct["catch"][1]))
        self.assertEqual(rows[0]["balanced_high"], tables.points(direct["balanced"][2]))

    def test_the_catch_interval_resamples_only_families_with_an_expected_fail(self):
        base, tool, expected, family = paired(self.rows(quiet=30))
        every = tables.family_sums(base, tool, expected, family)
        with_fail = [f for f in every if f["fails"]]
        self.assertEqual((len(every), len(with_fail)), (60, 30))
        e = tables.effect(base, tool, expected, family, 5, 2000, 1)
        self.assertEqual(e["catch"], tables.bootstrap(with_fail, tables.catch_difference, 5, 2000, 1))
        self.assertNotEqual(e["catch"][1:3], tables.bootstrap(every, tables.catch_difference, 5, 2000, 1)[1:3])
        self.assertEqual(e["balanced"], tables.bootstrap(every, tables.balanced_difference, 5, 2000, 1))

    def test_gains_and_losses_among_expected_fail_items_are_counted_with_where_they_came_from(self):
        rows = [
            ("a", "a-1", "fail", "pass", "fail"),
            ("a", "a-2", "fail", "unsure", "fail"),
            ("a", "a-3", "fail", "error", "fail"),
            ("a", "a-4", "fail", "fail", "unsure"),
            ("a", "a-5", "fail", "fail", "fail"),
            ("a", "a-6", "pass", "pass", "fail"),
        ]
        base, tool, expected, family = paired(rows)
        row = tables.effect_row("r", "m", "D_regions", base, tool, expected, family, {"seed": 1, "hypotheses": 1}, 50)
        self.assertEqual(
            (row["expected_fail_gained"], row["expected_fail_gained_from_review"], row["expected_fail_gained_from_pass"], row["expected_fail_lost"]),
            (3, 2, 1, 1),
        )
        self.assertEqual((row["flipped_expected_fail"], row["flipped_expected_pass"]), (4, 1))

    def test_a_wider_correction_names_its_hypotheses_and_says_when_the_tail_is_coarse(self):
        base, tool, expected, family = paired(self.rows())
        e = tables.effect(base, tool, expected, family, 9, 800, 1)
        one, thirteen, twenty_six = (tables.wider(e, 9, 800, h) for h in (1, 13, 26))
        self.assertEqual((thirteen["hypotheses"], thirteen["interval_level"]), (13, 99.62))
        self.assertEqual((twenty_six["hypotheses"], twenty_six["interval_level"]), (26, 99.81))
        # 800 resamples put 20 below the lower quantile for one hypothesis, and fewer for more.
        self.assertEqual([w["tail_resolved"] for w in (one, thirteen, twenty_six)], [True, False, False])
        self.assertFalse(tables.wider(e, 9, 799, 1)["tail_resolved"])
        self.assertLessEqual(twenty_six["catch_low"], thirteen["catch_low"])
        self.assertLess(thirteen["catch_low"], one["catch_low"])
        self.assertEqual(one["catch_low"], tables.points(e["catch"][1]))
        again = tables.bootstrap(e["families_with_fail"], tables.catch_difference, 9, 800, 13)
        self.assertEqual((thirteen["catch_low"], thirteen["catch_high"]), (tables.points(again[1]), tables.points(again[2])))
        again = tables.bootstrap(e["every_family"], tables.balanced_difference, 9, 800, 26)
        self.assertEqual(twenty_six["balanced_low"], tables.points(again[1]))


class Toml(unittest.TestCase):
    def test_the_small_reader_takes_plain_values_and_skips_the_rest(self):
        text = "\n".join(
            [
                "# a comment",
                "version = 2",
                'name = "a \\"quoted\\" name"  # trailing',
                "rate = -0.25",
                "on = true",
                "",
                "[[label]]",
                'item = "one"',
                'expected = "fail"',
                'kinds = ["a",',
                '  "b"]',
                'words = """',
                'item = "not this"',
                '"""',
                "[[label.regions]]",
                'kind = "bbox"',
                "",
                "[[label]]",
                'item = "two"',
                'expected = "pass"',
                "[other.table]",
                "n = 3",
            ]
        )
        top, found = tables.read_toml(text)
        self.assertEqual(top, {"version": 2, "name": 'a "quoted" name', "rate": -0.25, "on": True})
        self.assertEqual(found["label"], [{"item": "one", "expected": "fail"}, {"item": "two", "expected": "pass"}])
        self.assertEqual(found["label.regions"], [{"kind": "bbox"}])
        self.assertEqual(found["other.table"], [{"n": 3}])

    def test_what_is_written_reads_back_the_same(self):
        numbers = {
            "meta": {"study": "s", "items": 4, "spend_usd": 1.5, "ok": True},
            "run": [{"run": "r", "seed": 7}],
            "cell": [{"model": "m", "catch_rate": 0.8571, "errors": 0}],
            "effect": [{"catch_points": -2.74, "catch_excludes_zero": False}],
            "sealed_effect": [],
            "sensitivity": [],
            "repeat": [],
            "error": [],
            "error_kind": [],
        }
        self.assertEqual(tables.from_toml(tables.to_toml(numbers)), numbers)

    def test_a_number_that_cannot_be_written_plainly_is_refused(self):
        with self.assertRaises(tables.TablesError):
            tables.toml_value(1e-9)
        with self.assertRaises(tables.TablesError):
            tables.toml_value(float("nan"))


class Synthetic:
    """A package with one study run of one model: 4 families of 4 items, three arms, and a
    second repetition of the control. `gate=True` adds a second run that is not a study run,
    with the control and the outline arm."""

    def __init__(self, root, gate=False):
        self.root = root
        self.gate = gate
        self.items = []
        for fam in "abcd":
            for n in range(1, 5):
                expected = "fail" if n <= 2 else "pass"
                self.items.append(("fam-" + fam, "item-%s-%d" % (fam, n), expected))
        self.sealed = {i: e for _, i, e in self.items}
        self.corrections = [("item-d-3", "pass", "fail")]
        self.verdicts = {}
        for arm in tables.ARMS:
            for _, item, expected in self.items:
                self.verdicts[(arm, 1, item)] = expected
        self.verdicts[("C", 1, "item-a-1")] = "pass"  # a miss the tools repair
        self.verdicts[("C", 1, "item-b-1")] = "unsure"
        self.verdicts[("D_outline", 1, "item-b-1")] = "unsure"
        self.verdicts[("D_regions", 1, "item-c-3")] = "fail"  # a new false reject
        self.verdicts[("D_regions", 1, "item-c-1")] = None  # an error
        for _, item, _ in self.items:
            self.verdicts[("C", 2, item)] = self.verdicts[("C", 1, item)]
        self.verdicts[("C", 2, "item-a-1")] = "fail"
        self.verdicts[("C", 2, "item-b-1")] = "pass"
        self.verdicts[("C", 2, "item-c-2")] = None  # an error in the second repetition
        self.settings = {
            "bootstrap": {RUN: {"hypotheses": 2, "resamples": 300, "seed": 11}},
            "corrections_version": 1,
            "headline_repetition": 1,
            "scoring_policy": {RUN: {"on_error": "review", "on_unsure": "review"}},
        }
        if gate:
            self.settings["bootstrap"][GATE] = {"hypotheses": 1, "resamples": 300, "seed": 5}
            self.settings["scoring_policy"][GATE] = {"on_error": "review", "on_unsure": "review"}

    def put(self, parts, text):
        path = os.path.join(self.root, *parts)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)

    def write_run(self, run, verdicts, spent):
        ledger = []
        for (arm, rep, item), verdict in sorted(verdicts.items(), key=lambda kv: (kv[0][0], kv[0][1], kv[0][2])):
            cell = "%s:%s/%s" % (PROFILE, MODEL, arm)
            slow = arm == "C" and item == "item-a-1"
            result = {
                "item": item,
                "cell": cell,
                "repetition": rep,
                "verdict": verdict,
                "error_class": None if verdict else "submission_invalid",
                "error": None if verdict else "submission: finding 1: location invalid: outside",
                "cost_usd": 0.01,
                "wall_ms": 500000 if slow else (2000 if arm == "C" else 4000),
            }
            self.put(
                ("study", run, "decisions", "results", "set", item, PROFILE + "_" + MODEL, arm, "rep-%d" % rep, "result.json"),
                json.dumps(result),
            )
            name = "set/%s/%s/rep-%d" % (item, cell, rep)
            ledger.append(json.dumps({"event": "reserve", "id": name, "attempt": 1, "usd": 1.0}))
            ledger.append(json.dumps({"event": "settle", "id": name, "attempt": 1, "reserved": 1.0, "spent": spent}))
        self.put(("study", run, "ledger.jsonl"), "\n".join(ledger) + "\n")
        self.put(("study", run, "descriptor.json"), json.dumps({"harness_version": "9.9.9 (abc1234)", "prompt_version": "v7"}))

    def write(self):
        shutil.rmtree(self.root, ignore_errors=True)
        self.put(tables.SETTINGS_PATH, json.dumps(self.settings))
        self.put(("PACKAGE.digest",), "ab" * 32 + "\n")
        labels = "".join(
            '[[label]]\nitem = "%s"\nexpected = "%s"\nfailure_types = ["x",\n  "y"]\n\n' % (i, e)
            for i, e in sorted(self.sealed.items())
        )
        self.put(tables.LABELS_PATH, labels)
        corrections = "corrections_version = 1\n\n" + "".join(
            '[[correction]]\nitem = "%s"\nwas = "%s"\nexpected = "%s"\n\n' % c for c in self.corrections
        )
        self.put(tables.CORRECTIONS_PATH, corrections)
        self.put(
            tables.SET_PATH,
            '[set]\nid = "synthetic"\n\n'
            + "".join('[[item]]\nid = "%s"\nfamily = "%s"\njob = "j"\n\n' % (i, f) for f, i, _ in self.items),
        )
        self.write_run(RUN, self.verdicts, 0.0125)
        if self.gate:
            gate = {(arm, 1, item): expected for arm in ("C", "D_outline") for _, item, expected in self.items}
            self.write_run(GATE, gate, 0.0125)
        return self.root


BLOCKS = (
    "scope", "runs", "cells", "outcomes", "effects", "sensitivity", "gains", "sealed-effects", "repeat",
    "errors", "errors-by-model", "error-kinds", "spend", "earlier",
)
WORDS = "Some words about <!-- =meta/items -->16<!-- /= --> items."
DOCUMENT = "# A study\n\n" + WORDS + "\n\n" + "".join(
    "<!-- generated:%s -->\n<!-- /generated -->\n\n" % name for name in BLOCKS
)


class Whole(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.package = Synthetic(os.path.join(self.tmp, "package"))
        self.package.write()

    def numbers(self):
        return tables.compute(self.package.root)

    def cell(self, numbers, arm):
        return [c for c in numbers["cell"] if c["arm"] == arm][0]

    def test_counts_rates_and_spend_per_cell(self):
        n = self.numbers()
        m = n["meta"]
        self.assertEqual((m["items"], m["families"], m["expected_fail"], m["expected_pass"]), (16, 4, 9, 7))
        self.assertEqual((m["sealed_expected_fail"], m["corrections"], m["corrections_version"]), (8, 1, 1))
        self.assertEqual((m["runs"], m["decisions"], m["models"], m["comparisons"]), (1, 64, 1, 2))
        self.assertEqual(m["package_digest"], "sha256:" + "ab" * 32)
        self.assertEqual(len(n["cell"]), 3, "one row per arm, from repetition 1")
        c = self.cell(n, "C")
        # 9 expected fail: item-a-1 let through, item-b-1 unsure, item-d-3 (corrected) passed.
        self.assertEqual((c["detected"], c["missed"], c["review_on_fail"], c["false_rejects"]), (6, 2, 1, 0))
        self.assertEqual(c["catch_rate"], round(6 / 9, 4))
        self.assertEqual(c["balanced_accuracy"], round((6 / 9 + 1) / 2, 4))
        # Decided correctly: caught or cleared. Review is neither.
        self.assertEqual((c["decided_correctly"], c["correct_rate"]), (13, round(13 / 16, 4)))
        self.assertEqual((c["decisions_all_repetitions"], c["spend_usd"]), (32, 0.4))
        self.assertEqual(c["median_wall_s"], 2.0, "one decision of 500 s does not move the median")
        self.assertEqual(c["profile"], PROFILE)
        r = self.cell(n, "D_regions")
        self.assertEqual((r["errors"], r["false_rejects"], r["decisions_all_repetitions"], r["spend_usd"]), (1, 1, 16, 0.2))
        self.assertEqual(r["false_reject_rate"], round(1 / 7, 4))
        run = n["run"][0]
        self.assertEqual((run["spend_usd"], run["decision_cost_usd"], run["abandoned_attempts_usd"]), (0.8, 0.64, 0.16))
        self.assertEqual((run["harness_version"], run["prompt_version"]), ("9.9.9 (abc1234)", "v7"))

    def test_errors_are_counted_over_every_repetition(self):
        n = self.numbers()
        self.assertEqual(n["meta"]["errors"], 2)
        self.assertEqual(
            [(e["arm"], e["repetition"], e["errors"], e["on_expected_fail"]) for e in n["error"]],
            [("C", 2, 1, 1), ("D_regions", 1, 1, 1)],
        )
        self.assertEqual(n["error_kind"], [{"model": MODEL, "kind": "location", "errors": 2}])
        self.assertEqual(self.cell(n, "C")["errors"], 0, "the cell's row is the first repetition's")

    def test_a_run_that_is_not_a_study_run_is_counted_apart(self):
        package = Synthetic(os.path.join(self.tmp, "with-gate"), gate=True)
        package.write()
        n = tables.compute(package.root)
        m = n["meta"]
        self.assertEqual((m["runs"], m["decisions"], m["spend_usd"]), (2, 96, 1.2))
        self.assertEqual((m["study_runs"], m["study_decisions"], m["study_spend_usd"]), (1, 64, 0.8))
        self.assertEqual([r["run"] for r in n["run"]], [GATE, RUN])
        self.assertEqual((m["comparisons"], [e["hypotheses"] for e in n["effect"]]), (3, [1, 2, 2]))
        self.assertEqual(len(n["cell"]), 5)

    def test_effects_pair_each_tool_arm_with_the_control_under_corrected_and_sealed_labels(self):
        n = self.numbers()
        outline, regions = n["effect"]
        self.assertEqual((outline["tool"], regions["tool"]), ("D_outline", "D_regions"))
        self.assertEqual(outline["catch_points"], round(100 / 9, 2))
        self.assertEqual((outline["hypotheses"], outline["interval_level"]), (2, 97.5))
        self.assertEqual((outline["flipped_expected_fail"], outline["flipped_expected_pass"]), (1, 0))
        self.assertEqual((outline["expected_fail_gained"], outline["expected_fail_gained_from_pass"], outline["expected_fail_lost"]), (1, 1, 0))
        self.assertEqual((regions["expected_fail_gained"], regions["expected_fail_gained_from_review"], regions["expected_fail_lost"]), (2, 1, 1))
        self.assertEqual(regions["flipped_expected_pass"], 1)
        self.assertEqual(regions["balanced_points"], round(100 * (1 / 9 - 1 / 7) / 2, 2))
        self.assertLessEqual(outline["catch_low"], outline["catch_points"])
        self.assertGreaterEqual(outline["catch_high"], outline["catch_points"])
        sealed = n["sealed_effect"][0]
        self.assertEqual(sealed["catch_points"], round(100 / 8, 2))
        shifts = [
            abs(a[name] - b[name])
            for a, b in zip(n["effect"], n["sealed_effect"])
            for name in ("catch_points", "balanced_points")
        ]
        self.assertEqual(n["meta"]["corrections_largest_point_shift"], round(max(shifts), 2))
        self.assertGreater(n["meta"]["corrections_largest_point_shift"], 1.0)

    def test_only_a_comparison_that_excludes_zero_is_widened_for_the_whole_study(self):
        self.assertEqual(self.numbers()["sensitivity"], [], "neither synthetic effect excludes zero")
        # The control misses every expected fail and the outline arm catches each one.
        for _, item, expected in self.package.items:
            if expected == "fail":
                self.package.verdicts[("C", 1, item)] = "pass"
                self.package.verdicts[("D_outline", 1, item)] = "fail"
                self.package.verdicts[("D_regions", 1, item)] = "pass"
        self.package.write()
        n = self.numbers()
        self.assertTrue(n["effect"][0]["catch_excludes_zero"])
        self.assertEqual(n["meta"]["comparisons_excluding_zero"], 1)
        self.assertEqual(
            [(w["tool"], w["scope"], w["hypotheses"]) for w in n["sensitivity"]],
            [("D_outline", "comparisons", 2), ("D_outline", "comparisons_and_measures", 4)],
        )
        document = tables.render(n, DOCUMENT)
        self.assertIn("Across the 2 comparisons and both measures", document)

    def test_the_resample_count_comes_from_the_settings_unless_given(self):
        self.assertEqual(self.numbers()["run"][0]["resamples"], 300)
        self.assertEqual(tables.compute(self.package.root, 50)["run"][0]["resamples"], 50)

    def test_the_second_repetition_only_measures_disagreement(self):
        n = self.numbers()
        (r,) = n["repeat"]
        self.assertEqual(r["arm"], "C")
        self.assertEqual((r["fail_indicator_disagreements"], r["verdict_disagreements"]), (2, 3))
        self.assertEqual((r["catch_rate_rep_2"], r["errors_rep_2"]), (round(6 / 9, 4), 1))

    def test_two_computations_write_the_same_bytes(self):
        self.assertEqual(tables.to_toml(self.numbers()), tables.to_toml(self.numbers()))

    def refused(self, change, words):
        change(self.package)
        self.package.write()
        with self.assertRaises(tables.TablesError) as caught:
            tables.compute(self.package.root)
        self.assertIn(words, str(caught.exception))

    def test_a_correction_that_does_not_start_from_the_sealed_label_is_refused(self):
        self.refused(lambda p: p.corrections.append(("item-a-1", "pass", "fail")), "sealed label")

    def test_a_correction_that_changes_nothing_is_refused(self):
        self.refused(lambda p: p.corrections.append(("item-a-3", "pass", "pass")), "changes nothing")

    def test_another_corrections_version_is_refused(self):
        self.refused(lambda p: p.settings.update(corrections_version=2), "different versions")

    def test_another_scoring_policy_is_refused(self):
        self.refused(lambda p: p.settings["scoring_policy"][RUN].update(on_unsure="pass"), "policy")

    def test_a_cell_short_of_an_item_is_refused(self):
        self.refused(lambda p: p.verdicts.pop(("D_outline", 1, "item-a-2")), "every item")

    def test_an_unlabelled_set_item_is_refused(self):
        self.refused(lambda p: p.sealed.pop("item-a-4"), "differ")

    def test_the_document_is_rendered_from_the_numbers_and_checked_against_them(self):
        n = tables.from_toml(tables.to_toml(self.numbers()))
        document = tables.render(n, DOCUMENT)
        self.assertIn("| model-x | provider | control |", document)
        self.assertIn("`9.9.9 (abc1234)`", document)
        self.assertEqual(tables.render(n, document), document, "rendering again changes nothing")
        self.assertEqual(tables.check_document(n, document), [])
        self.assertTrue(tables.check_document(n, DOCUMENT), "an unrendered document is stale")
        edited = document.replace("66.7%", "76.7%", 1)
        self.assertNotEqual(edited, document)
        self.assertTrue(tables.check_document(n, edited))
        self.assertIn("<!-- =meta/items -->16<!-- /= -->", document)
        stale = DOCUMENT.replace("-->16<!--", "-->15<!--")
        self.assertEqual(tables.render(n, stale), document, "a value is filled from the numbers file")

    def test_a_figure_in_the_prose_must_be_one_the_numbers_hold(self):
        n = self.numbers()
        document = tables.render(n, DOCUMENT)

        def with_prose(words):
            return document.replace(WORDS, words)

        for fine in (
            "The control caught 66.7% of failures and the run cost $0.80.",
            "The tool added +11.1 points, seed 11 at a 97.5% level, in about 55 minutes on day 12.",
            "Run `study-synthetic-2026-11` used model-9.9-x with 300 resamples.",
            "It caught <!-- =cell/model-x/C/detected -->6<!-- /= --> of the "
            "<!-- =meta/expected_fail -->9<!-- /= --> failures, <!-- =cell/model-x/C/catch_rate|pct -->66.7%<!-- /= -->.",
            "The tool added <!-- =effect/model-x/D_outline/catch_points|points -->+11.1<!-- /= --> points "
            "for <!-- =meta/spend_usd|usd -->$0.80<!-- /= -->; errors: <!-- =errors/model-x -->2<!-- /= -->.",
        ):
            self.assertEqual(tables.check_document(n, with_prose(fine)), [], fine)
        for wrong in (
            "The control caught 67.7% of failures.",
            "The run cost $0.90.",
            "The tool added +12.0 points.",
            "It covers 1,700 decisions.",
            "It covers 17 items.",
            "It covers 16 items.",
            "There were 2 errors.",
            "It caught 6 of the 9 failures.",
            "Across 9 expected-fail items.",
            "It caught <!-- =cell/model-x/C/detected -->7<!-- /= --> failures.",
            "It caught <!-- =cell/model-x/C/nothing -->6<!-- /= --> failures.",
            "It caught <!-- =cell/model-y/C/detected -->6<!-- /= --> failures.",
            "It caught <!-- =cell/model-x/C/detected|words -->6<!-- /= --> failures.",
        ):
            self.assertTrue(tables.check_document(n, with_prose(wrong)), wrong)

    def test_a_missing_or_unknown_block_is_reported(self):
        n = self.numbers()
        document = tables.render(n, DOCUMENT)
        without = document.replace("<!-- generated:spend -->", "<!-- generated:spending -->")
        self.assertTrue(tables.check_document(n, without))
        self.assertTrue(tables.check_document(n, document.split("<!-- generated:spend -->")[0]))

    def test_counts_and_spend_must_equal_the_manifest(self):
        n = self.numbers()
        cells = "".join(
            '[[cell]]\nmodel = "%s:%s"\narm = "%s"\ndecisions = %d\nspend_usd = %s\n\n' % (PROFILE, MODEL, arm, d, s)
            for arm, d, s in (("C", 32, "0.4"), ("D_outline", 16, "0.2"), ("D_regions", 16, "0.2"))
        )
        manifest = 'digest = "sha256:%s"\n\n%s' % ("ab" * 32, cells)
        self.assertEqual(tables.check_manifest(n, manifest), [])
        self.assertTrue(tables.check_manifest(n, manifest.replace("decisions = 32", "decisions = 31")))
        self.assertTrue(tables.check_manifest(n, manifest.replace("spend_usd = 0.4", "spend_usd = 0.41")))
        self.assertTrue(tables.check_manifest(n, manifest.replace("ab" * 32, "cd" * 32)))
        self.assertTrue(tables.check_manifest(n, manifest.replace('arm = "D_regions"', 'arm = "D_other"')))

    def test_the_command_line_computes_renders_and_checks(self):
        numbers = os.path.join(self.tmp, "numbers.toml")
        document = os.path.join(self.tmp, "document.md")
        with open(document, "w", encoding="utf-8") as f:
            f.write(DOCUMENT)
        self.assertEqual(tables.main(["compute", self.package.root, "--toml", numbers]), 0)
        self.assertEqual(tables.main(["check", numbers, document]), 1)
        self.assertEqual(tables.main(["render", numbers, document]), 0)
        self.assertEqual(tables.main(["check", numbers, document]), 0)
        self.assertEqual(tables.main(["compute", os.path.join(self.tmp, "nowhere"), "--toml", numbers]), 1)


if __name__ == "__main__":
    unittest.main()
