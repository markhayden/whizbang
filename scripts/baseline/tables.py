"""The numbers of the 2026-11 tool study, computed from the baseline package's study inputs
alone, and the tables of the document that reports them.

    tables.py compute <package> --toml <numbers.toml> [--resamples N]
    tables.py render <numbers.toml> <document.md>            rewrite the generated blocks
    tables.py check <numbers.toml> <document.md> [--manifest <manifest.toml>]

`compute` reads, under the package: the analysis settings, every decision's result, each
run's spend ledger, the set's item families, the sealed labels and the owner's corrections.
It prints nothing about an item and writes only aggregates.

`check` needs no package: the document's generated blocks must equal what the numbers file
renders to, every figure in the prose must be one the numbers file holds, and the counts and
spend must equal the baseline manifest's.

Python 3.9 or later, standard library only.
"""

import argparse
import hashlib
import json
import os
import re
import sys

sys.dont_write_bytecode = True

TABLES_VERSION = "1"
ARMS = ("C", "D_outline", "D_regions")
TOOL_ARMS = ("D_outline", "D_regions")
VERDICTS = ("pass", "fail", "unsure", "error")
Z_95 = 1.959963984540054
SET_PATH = ("inputs", "recorded", "set", "source-fidelity", "blind-v3", "set.toml")
LABELS_PATH = ("inputs", "recorded", "labels", "labels.toml")
CORRECTIONS_PATH = ("assembly", "owner", "corrections.toml")
SETTINGS_PATH = ("assembly", "generated", "analysis-settings.json")

# What an invalid submission was refused for, by the words of the old harness's message.
ERROR_KINDS = (
    ("location", "location invalid"),
    ("rule_kind", "cite observe rules only"),
    ("checklist", "must appear exactly once"),
    ("checklist", "is a finding disposition, not an item one"),
    ("evidence", "is not a tool call that completed in this run"),
    ("image", "does not judge image"),
)


class TablesError(Exception):
    pass


# --- a generator that is the same on every Python ---------------------------------------


class Stream:
    """Whole numbers drawn from SHA-256 in counter mode. Block `i` is the SHA-256 of the
    ASCII text `<seed>:<i>`; it gives four 64-bit big-endian words, used first to last. A
    draw below `n` takes the next word, drops it if it is not below the largest multiple
    of `n` that fits in 64 bits, and otherwise returns it modulo `n`."""

    def __init__(self, seed):
        self.seed = int(seed)
        self.block = 0
        self.words = []

    def word(self):
        if not self.words:
            digest = hashlib.sha256(("%d:%d" % (self.seed, self.block)).encode("ascii")).digest()
            self.block += 1
            self.words = [int.from_bytes(digest[i : i + 8], "big") for i in (24, 16, 8, 0)]
        return self.words.pop()

    def below(self, n):
        if n <= 0:
            raise TablesError("a draw needs a positive bound")
        limit = (1 << 64) - ((1 << 64) % n)
        while True:
            word = self.word()
            if word < limit:
                return word % n


def quantile_type7(ordered, p):
    """Hyndman and Fan type 7: linear interpolation between order statistics."""
    n = len(ordered)
    if n == 0:
        raise TablesError("no values to take a quantile of")
    if n == 1:
        return ordered[0]
    h = (n - 1) * min(max(p, 0.0), 1.0)
    lo = int(h)
    hi = min(lo + 1, n - 1)
    return ordered[lo] + (h - lo) * (ordered[hi] - ordered[lo])


def wilson(count, total):
    """The 95% Wilson interval of a proportion; the whole unit interval with no trials."""
    if total == 0:
        return 0.0, 1.0
    p = count / total
    z2 = Z_95 * Z_95
    centre = (p + z2 / (2 * total)) / (1 + z2 / total)
    half = Z_95 * ((p * (1 - p) / total + z2 / (4 * total * total)) ** 0.5) / (1 + z2 / total)
    return max(0.0, centre - half), min(1.0, centre + half)


# --- scoring ----------------------------------------------------------------------------


def verdict_of(result):
    """A decision's outcome as one of four words. A decision with no verdict is an error."""
    verdict = result.get("verdict")
    if verdict is None:
        return "error"
    if verdict not in ("pass", "fail", "unsure"):
        raise TablesError("an unknown verdict: %r" % (verdict,))
    return verdict


def cell_counts(verdicts, expected):
    """Counts for one model and arm in one repetition, under the policy that an unsure
    decision and an error both go to review: neither detects a failure, and neither
    rejects a sound image.

    `verdicts`: item -> one of VERDICTS. `expected`: item -> "pass" or "fail"."""
    c = {
        "items": 0,
        "expected_fail": 0,
        "expected_pass": 0,
        "detected": 0,
        "missed": 0,
        "review_on_fail": 0,
        "false_rejects": 0,
        "cleared": 0,
        "review_on_pass": 0,
        "unsure": 0,
        "errors": 0,
    }
    for item in sorted(verdicts):
        if item not in expected:
            raise TablesError("a decision's item has no label")
        v = verdicts[item]
        c["items"] += 1
        c["unsure"] += v == "unsure"
        c["errors"] += v == "error"
        if expected[item] == "fail":
            c["expected_fail"] += 1
            c["detected"] += v == "fail"
            c["missed"] += v == "pass"
            c["review_on_fail"] += v in ("unsure", "error")
        else:
            c["expected_pass"] += 1
            c["false_rejects"] += v == "fail"
            c["cleared"] += v == "pass"
            c["review_on_pass"] += v in ("unsure", "error")
    return c


def rates(c):
    """Catch rate, false-reject rate and their mean-of-two balanced accuracy."""
    if not c["expected_fail"] or not c["expected_pass"]:
        raise TablesError("a cell needs both expected-fail and expected-pass items")
    catch = c["detected"] / c["expected_fail"]
    false_reject = c["false_rejects"] / c["expected_pass"]
    return catch, false_reject, (catch + 1 - false_reject) / 2


# --- the paired bootstrap ---------------------------------------------------------------


def family_sums(base, tool, expected, family):
    """Per family, sorted by id: what both estimands need. The two arms must hold the same
    items; an item's detection indicator is `verdict == fail`."""
    if set(base) != set(tool):
        raise TablesError("the two arms of a comparison do not hold the same items")
    sums = {}
    for item in sorted(base):
        if item not in expected or item not in family:
            raise TablesError("a paired item has no label or no family")
        s = sums.setdefault(
            family[item], {"fails": 0, "passes": 0, "det_c": 0, "det_x": 0, "fr_c": 0, "fr_x": 0, "differs": False}
        )
        dc, dx = base[item] == "fail", tool[item] == "fail"
        if expected[item] == "fail":
            s["fails"] += 1
            s["det_c"] += dc
            s["det_x"] += dx
            s["differs"] = s["differs"] or dc != dx
        else:
            s["passes"] += 1
            s["fr_c"] += dc
            s["fr_x"] += dx
    return [sums[k] for k in sorted(sums)]


def catch_difference(families, picks):
    """The item-weighted paired difference in the share of expected-fail items detected,
    tool arm minus control, over a selection of families (with multiplicity)."""
    diff = sum(families[i]["det_x"] - families[i]["det_c"] for i in picks)
    fails = sum(families[i]["fails"] for i in picks)
    return diff / fails if fails else 0.0


def balanced_difference(families, picks):
    """The paired difference in balanced accuracy over a selection of families; None when
    the selection has no expected-fail item or no expected-pass item."""
    fails = sum(families[i]["fails"] for i in picks)
    passes = sum(families[i]["passes"] for i in picks)
    if not fails or not passes:
        return None
    catch = sum(families[i]["det_x"] - families[i]["det_c"] for i in picks) / fails
    reject = sum(families[i]["fr_x"] - families[i]["fr_c"] for i in picks) / passes
    return (catch - reject) / 2


def bootstrap(families, estimand, seed, resamples, hypotheses):
    """Resamples whole families with replacement: one stream from the seed, one draw per
    family per resample, families in id order. The interval is the type 7 quantiles at
    0.025 / hypotheses and its complement. Returns (point, lower, upper, used)."""
    if resamples <= 0 or hypotheses <= 0:
        raise TablesError("resamples and hypotheses must be positive")
    if not families:
        raise TablesError("no family to resample")
    point = estimand(families, list(range(len(families))))
    stream = Stream(seed)
    estimates = []
    for _ in range(resamples):
        value = estimand(families, [stream.below(len(families)) for _ in families])
        if value is not None:
            estimates.append(value)
    estimates.sort()
    lower = 0.025 / hypotheses
    return point, quantile_type7(estimates, lower), quantile_type7(estimates, 1 - lower), len(estimates)


def effect(base, tool, expected, family, seed, resamples, hypotheses):
    """Both estimands for one model and one tool arm against the control."""
    every = family_sums(base, tool, expected, family)
    with_fail = [f for f in every if f["fails"]]
    if not with_fail:
        raise TablesError("no family has an expected-fail item")
    catch = bootstrap(with_fail, catch_difference, seed, resamples, hypotheses)
    balanced = bootstrap(every, balanced_difference, seed, resamples, hypotheses)
    flips_fail = sum(1 for i in base if expected[i] == "fail" and (base[i] == "fail") != (tool[i] == "fail"))
    flips_pass = sum(1 for i in base if expected[i] == "pass" and (base[i] == "fail") != (tool[i] == "fail"))
    return {
        "items": len(base),
        "families": len(every),
        "families_with_expected_fail": len(with_fail),
        "differing_families": sum(1 for f in with_fail if f["differs"]),
        "catch": catch,
        "balanced": balanced,
        "flipped_expected_fail": flips_fail,
        "flipped_expected_pass": flips_pass,
    }


def error_kind(message):
    for kind, words in ERROR_KINDS:
        if words in (message or ""):
            return kind
    return "other"


# --- a small reader for the TOML these scripts meet -------------------------------------

_HEADER = re.compile(r"^\[(\[)?\s*([A-Za-z0-9_.-]+)\s*\]\]?\s*(#.*)?$")
_PAIR = re.compile(r"^([A-Za-z0-9_-]+)\s*=\s*(.+?)\s*$")
_STRING = re.compile(r'^("(?:[^"\\]|\\.)*")\s*(#.*)?$')
_NUMBER = re.compile(r"^([+-]?\d+(?:\.\d+)?)\s*(#.*)?$")
_BOOL = re.compile(r"^(true|false)\s*(#.*)?$")


def read_toml(text):
    """Top-level keys, `[table]` and `[[array of tables]]` with one-line string, number and
    boolean values. A value of any other shape, and every line of a multi-line string, is
    skipped: these scripts need only plain values. Returns (top, tables) where `tables`
    maps a header name to the list of its tables in order."""
    top, tables = {}, {}
    current = top
    in_text = None
    for line in text.split("\n"):
        stripped = line.strip()
        if in_text:
            if in_text in stripped:
                in_text = None
            continue
        for mark in ('"""', "'''"):
            if stripped.count(mark) == 1:
                in_text = mark
        if in_text or not stripped or stripped.startswith("#"):
            continue
        header = _HEADER.match(stripped)
        if header:
            current = {}
            tables.setdefault(header.group(2), []).append(current)
            continue
        pair = _PAIR.match(stripped)
        if not pair:
            continue
        key, raw = pair.group(1), pair.group(2)
        string = _STRING.match(raw)
        number = _NUMBER.match(raw)
        boolean = _BOOL.match(raw)
        if string:
            try:
                current[key] = json.loads(string.group(1))
            except ValueError:
                continue
        elif number:
            current[key] = float(number.group(1)) if "." in number.group(1) else int(number.group(1))
        elif boolean:
            current[key] = boolean.group(1) == "true"
    return top, tables


def read_toml_file(path):
    with open(path, "rb") as f:
        text = f.read().decode("utf-8")
    top, tables = read_toml(text)
    return top, tables, text


def cross_check(text, name, keys, got):
    """Where the standard library can read TOML in full (Python 3.11 on), the small reader
    must agree with it on the values these scripts use."""
    try:
        import tomllib
    except ImportError:
        return
    full = tomllib.loads(text).get(name, [])
    want = [{k: row[k] for k in keys if k in row} for row in full]
    have = [{k: row[k] for k in keys if k in row} for row in got]
    if want != have:
        raise TablesError("the small TOML reader disagrees with tomllib on the %s tables" % name)


# --- reading the package ----------------------------------------------------------------


def load_json(path):
    with open(path, "rb") as f:
        return json.loads(f.read().decode("utf-8"))


def read_labels(package):
    """Sealed labels, the corrections applied on top, and each item's family."""
    _, tables, text = read_toml_file(os.path.join(package, *LABELS_PATH))
    rows = tables.get("label", [])
    cross_check(text, "label", ("item", "expected"), rows)
    sealed = {}
    for row in rows:
        if row.get("expected") not in ("pass", "fail") or "item" not in row or row["item"] in sealed:
            raise TablesError("the labels file has a label these scripts cannot read")
        sealed[row["item"]] = row["expected"]
    top, tables, text = read_toml_file(os.path.join(package, *CORRECTIONS_PATH))
    rows = tables.get("correction", [])
    cross_check(text, "correction", ("item", "was", "expected"), rows)
    corrected = dict(sealed)
    for row in rows:
        item = row.get("item")
        if item not in sealed or row.get("was") != sealed[item] or row.get("expected") not in ("pass", "fail"):
            raise TablesError("a correction does not start from the sealed label")
        if row["expected"] == sealed[item]:
            raise TablesError("a correction changes nothing")
        corrected[item] = row["expected"]
    _, tables, text = read_toml_file(os.path.join(package, *SET_PATH))
    rows = tables.get("item", [])
    cross_check(text, "item", ("id", "family"), rows)
    family = {}
    for row in rows:
        if "id" not in row or "family" not in row or row["id"] in family:
            raise TablesError("the set file has an item these scripts cannot read")
        family[row["id"]] = row["family"]
    if set(family) != set(sealed):
        raise TablesError("the set's items and the labelled items differ")
    return sealed, corrected, family, top.get("corrections_version"), len(rows)


def split_cell(cell):
    """`<profile>:<model>/<arm>` -> (profile, model, arm)."""
    head, _, arm = cell.rpartition("/")
    profile, _, model = head.partition(":")
    if not profile or not model or arm not in ARMS:
        raise TablesError("a cell these scripts cannot read")
    return profile, model, arm


def read_run(package, run):
    """Every decision of one run: {(profile, model, arm, repetition): {item: result}}, and
    the settled spend per (profile, model, arm) from the run's ledger."""
    root = os.path.join(package, "study", run)
    cells = {}
    for folder, _, files in os.walk(os.path.join(root, "decisions")):
        if "result.json" not in files:
            continue
        result = load_json(os.path.join(folder, "result.json"))
        key = split_cell(result["cell"]) + (int(result["repetition"]),)
        items = cells.setdefault(key, {})
        if result["item"] in items:
            raise TablesError("%s holds two results for one item, cell and repetition" % run)
        items[result["item"]] = result
    spend = {}
    with open(os.path.join(root, "ledger.jsonl"), "rb") as f:
        for line in f.read().decode("utf-8").split("\n"):
            if not line.strip():
                continue
            event = json.loads(line)
            if event.get("event") != "settle":
                continue
            parts = event["id"].split("/")
            key = split_cell("/".join(parts[-3:-1]))
            spend[key] = spend.get(key, 0.0) + float(event["spent"])
    return cells, spend


# --- computing --------------------------------------------------------------------------


def r4(x):
    return round(x, 4)


def points(x):
    return round(100 * x, 2)


def median(values):
    ordered = sorted(values)
    mid = len(ordered) // 2
    return ordered[mid] if len(ordered) % 2 else (ordered[mid - 1] + ordered[mid]) / 2


def cell_row(run, key, results, expected, spend):
    profile, model, arm, repetition = key
    verdicts = {item: verdict_of(r) for item, r in results.items()}
    c = cell_counts(verdicts, expected)
    catch, false_reject, balanced = rates(c)
    catch_low, catch_high = wilson(c["detected"], c["expected_fail"])
    reject_low, reject_high = wilson(c["false_rejects"], c["expected_pass"])
    row = {"run": run, "profile": profile, "model": model, "arm": arm, "repetition": repetition}
    row.update(c)
    row.update(
        {
            "catch_rate": r4(catch),
            "catch_rate_low": r4(catch_low),
            "catch_rate_high": r4(catch_high),
            "false_reject_rate": r4(false_reject),
            "false_reject_rate_low": r4(reject_low),
            "false_reject_rate_high": r4(reject_high),
            "balanced_accuracy": r4(balanced),
            "decided_correctly": c["detected"] + c["cleared"],
            "correct_rate": r4((c["detected"] + c["cleared"]) / c["items"]),
            "median_wall_s": round(median([r["wall_ms"] for r in results.values()]) / 1000.0, 1),
        }
    )
    if spend is not None:
        row["spend_usd"] = r4(spend)
    return row


def effect_row(run, model, arm, base, tool, expected, family, gate, resamples):
    e = effect(base, tool, expected, family, gate["seed"], resamples, gate["hypotheses"])
    level = round(100 * (1 - 0.05 / gate["hypotheses"]), 2)
    catch, balanced = e["catch"], e["balanced"]
    return {
        "run": run,
        "model": model,
        "tool": arm,
        "items": e["items"],
        "families": e["families"],
        "families_with_expected_fail": e["families_with_expected_fail"],
        "differing_families": e["differing_families"],
        "hypotheses": gate["hypotheses"],
        "interval_level": level,
        "catch_points": points(catch[0]),
        "catch_low": points(catch[1]),
        "catch_high": points(catch[2]),
        "catch_excludes_zero": catch[1] > 0 or catch[2] < 0,
        "balanced_points": points(balanced[0]),
        "balanced_low": points(balanced[1]),
        "balanced_high": points(balanced[2]),
        "balanced_excludes_zero": balanced[1] > 0 or balanced[2] < 0,
        "balanced_resamples_used": balanced[3],
        "flipped_expected_fail": e["flipped_expected_fail"],
        "flipped_expected_pass": e["flipped_expected_pass"],
    }


def compute(package, resamples=None):
    settings = load_json(os.path.join(package, *SETTINGS_PATH))
    sealed, corrected, family, corrections_version, set_items = read_labels(package)
    if corrections_version != settings.get("corrections_version"):
        raise TablesError("the corrections file and the analysis settings name different versions")
    headline = settings.get("headline_repetition")
    if headline != 1:
        raise TablesError("these tables take repetition 1 as the headline")
    with open(os.path.join(package, "PACKAGE.digest"), "rb") as f:
        package_digest = "sha256:" + f.read().decode("ascii").strip()

    out = {
        "meta": {
            "study": "2026-11-tool-study",
            "tables_version": TABLES_VERSION,
            "package_digest": package_digest,
            "corrections_version": corrections_version,
            "headline_repetition": headline,
            "policy_on_unsure": "review",
            "policy_on_error": "review",
            "items": set_items,
            "families": len(set(family.values())),
            "expected_fail": sum(1 for v in corrected.values() if v == "fail"),
            "expected_pass": sum(1 for v in corrected.values() if v == "pass"),
            "sealed_expected_fail": sum(1 for v in sealed.values() if v == "fail"),
            "sealed_expected_pass": sum(1 for v in sealed.values() if v == "pass"),
            "corrections": sum(1 for i in sealed if sealed[i] != corrected[i]),
        },
        "run": [],
        "cell": [],
        "effect": [],
        "sealed_effect": [],
        "repeat": [],
        "error": [],
        "error_kind": [],
    }
    kinds = {}
    totals = {"decisions": 0, "spend": 0.0, "errors": 0, "unsure": 0, "study_spend": 0.0, "study_decisions": 0}
    for run in sorted(settings["bootstrap"]):
        policy = settings["scoring_policy"][run]
        if policy != {"on_error": "review", "on_unsure": "review"}:
            raise TablesError("%s was scored under a policy these tables do not implement" % run)
        gate = dict(settings["bootstrap"][run])
        used = resamples or gate["resamples"]
        cells, spend = read_run(package, run)
        decisions = sum(len(v) for v in cells.values())
        run_spend = sum(spend.values())
        out["run"].append(
            {
                "run": run,
                "seed": gate["seed"],
                "resamples": used,
                "hypotheses": gate["hypotheses"],
                "decisions": decisions,
                "repetitions": max(k[3] for k in cells),
                "spend_usd": r4(run_spend),
                "decision_cost_usd": r4(sum(r["cost_usd"] for v in cells.values() for r in v.values())),
            }
        )
        totals["decisions"] += decisions
        totals["spend"] += run_spend
        if run.startswith("study-"):
            totals["study_spend"] += run_spend
            totals["study_decisions"] += decisions
        for key in sorted(cells):
            profile, model, arm, repetition = key
            results = cells[key]
            if len(results) != set_items:
                raise TablesError("%s: a cell does not hold every item of the set" % run)
            cell_spend = spend.get((profile, model, arm)) if repetition == 1 else None
            row = cell_row(run, key, results, corrected, cell_spend)
            if repetition == 1:
                row["decisions_all_repetitions"] = sum(len(v) for k, v in cells.items() if k[:3] == key[:3])
                out["cell"].append(row)
            errors = [r for r in results.values() if verdict_of(r) == "error"]
            totals["errors"] += len(errors)
            totals["unsure"] += row["unsure"]
            if errors:
                classes = sorted({r.get("error_class") or "unclassed" for r in errors})
                out["error"].append(
                    {
                        "run": run,
                        "model": model,
                        "arm": arm,
                        "repetition": repetition,
                        "errors": len(errors),
                        "on_expected_fail": sum(1 for r in errors if corrected[r["item"]] == "fail"),
                        "error_classes": ",".join(classes),
                    }
                )
            for r in errors:
                k = (model, error_kind(r.get("error")))
                kinds[k] = kinds.get(k, 0) + 1
        verdicts = {k: {i: verdict_of(r) for i, r in v.items()} for k, v in cells.items()}
        for key in sorted(verdicts):
            profile, model, arm, repetition = key
            if repetition != 1:
                continue
            if arm in TOOL_ARMS:
                base = verdicts.get((profile, model, "C", 1))
                if base is None:
                    raise TablesError("%s: a tool arm has no control" % run)
                gate_used = {"seed": gate["seed"], "hypotheses": gate["hypotheses"]}
                out["effect"].append(effect_row(run, model, arm, base, verdicts[key], corrected, family, gate_used, used))
                out["sealed_effect"].append(effect_row(run, model, arm, base, verdicts[key], sealed, family, gate_used, used))
            second = verdicts.get((profile, model, arm, 2))
            if second is not None:
                first = verdicts[key]
                if set(first) != set(second):
                    raise TablesError("%s: two repetitions do not hold the same items" % run)
                c2 = cell_counts(second, corrected)
                out["repeat"].append(
                    {
                        "run": run,
                        "model": model,
                        "arm": arm,
                        "items": len(first),
                        "fail_indicator_disagreements": sum(
                            1 for i in first if (first[i] == "fail") != (second[i] == "fail")
                        ),
                        "verdict_disagreements": sum(1 for i in first if first[i] != second[i]),
                        "catch_rate_rep_1": r4(rates(cell_counts(first, corrected))[0]),
                        "catch_rate_rep_2": r4(rates(c2)[0]),
                        "balanced_accuracy_rep_1": r4(rates(cell_counts(first, corrected))[2]),
                        "balanced_accuracy_rep_2": r4(rates(c2)[2]),
                        "errors_rep_2": c2["errors"],
                        "unsure_rep_2": c2["unsure"],
                    }
                )
    for (model, kind) in sorted(kinds):
        out["error_kind"].append({"model": model, "kind": kind, "errors": kinds[(model, kind)]})
    out["meta"].update(
        {
            "runs": len(out["run"]),
            "decisions": totals["decisions"],
            "spend_usd": round(totals["spend"], 2),
            "study_runs": sum(1 for r in out["run"] if r["run"].startswith("study-")),
            "study_decisions": totals["study_decisions"],
            "study_spend_usd": round(totals["study_spend"], 2),
            "errors": totals["errors"],
            "unsure": totals["unsure"],
        }
    )
    return out


# --- writing and reading the numbers file -----------------------------------------------

HEADER = """# The numbers of the 2026-11 tool study. Written by scripts/baseline/tables.py from the
# baseline package's study inputs; see 2026-11-tool-study.md beside this file. It holds
# aggregates only: no item names, no item-level verdicts and no labels.
"""


def toml_value(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        text = repr(v)
        if "e" in text or "E" in text or "n" in text:
            raise TablesError("a number these tables cannot write: %s" % text)
        return text
    if isinstance(v, str):
        return json.dumps(v, ensure_ascii=True)
    raise TablesError("a value these tables cannot write")


def to_toml(numbers):
    lines = [HEADER]
    for key, value in numbers["meta"].items():
        lines.append("%s = %s" % (key, toml_value(value)))
    for name in ("run", "cell", "effect", "sealed_effect", "repeat", "error", "error_kind"):
        for row in numbers[name]:
            lines.append("")
            lines.append("[[%s]]" % name)
            for key, value in row.items():
                lines.append("%s = %s" % (key, toml_value(value)))
    return "\n".join(lines) + "\n"


def from_toml(text):
    top, tables = read_toml(text)
    numbers = {"meta": top}
    for name in ("run", "cell", "effect", "sealed_effect", "repeat", "error", "error_kind"):
        numbers[name] = tables.get(name, [])
    return numbers


# --- the document's generated blocks ----------------------------------------------------

ARM_WORDS = {"C": "control", "D_outline": "with outline", "D_regions": "with regions"}
TOOL_WORDS = {"D_outline": "outline", "D_regions": "regions"}
BLOCK = re.compile(r"(<!-- generated:([a-z-]+) -->\n)(.*?)(<!-- /generated -->)", re.S)


def pct(x):
    return "%.1f%%" % (100 * x)


def signed(x):
    return "%+.1f" % x


def usd(x):
    return "$%.2f" % x


def count(n):
    return "{:,}".format(n)


def table(head, rows):
    lines = ["| " + " | ".join(head) + " |", "|" + "|".join("---" for _ in head) + "|"]
    lines += ["| " + " | ".join(row) + " |" for row in rows]
    return "\n".join(lines) + "\n"


def interval(row, name):
    return "%s (%s to %s)" % (signed(row[name + "_points"]), signed(row[name + "_low"]), signed(row[name + "_high"]))


def effects_table(rows):
    head = ["Model", "Tool", "Interval", "Catch rate, points", "Balanced accuracy, points", "Items that changed"]
    body = []
    for e in rows:
        body.append(
            [
                e["model"],
                TOOL_WORDS[e["tool"]],
                "%g%%" % e["interval_level"],
                interval(e, "catch") + (" *" if e["catch_excludes_zero"] else ""),
                interval(e, "balanced") + (" *" if e["balanced_excludes_zero"] else ""),
                "%d of %d" % (e["flipped_expected_fail"] + e["flipped_expected_pass"], e["items"]),
            ]
        )
    return table(head, body)


def render_blocks(n):
    m = n["meta"]
    blocks = {}
    blocks["scope"] = table(
        ["Measure", "Value"],
        [
            ["Items in the set", count(m["items"])],
            ["Families of items", count(m["families"])],
            ["Expected to fail, after corrections", count(m["expected_fail"])],
            ["Expected to pass, after corrections", count(m["expected_pass"])],
            ["Labels corrected after the runs", "%d (corrections version %d)" % (m["corrections"], m["corrections_version"])],
            ["Expected to fail, as sealed", count(m["sealed_expected_fail"])],
            ["Runs", count(m["runs"])],
            ["Decisions, every repetition", count(m["decisions"])],
            ["Spend, every run", usd(m["spend_usd"])],
            ["Decisions in the six study runs", count(m["study_decisions"])],
            ["Spend in the six study runs", usd(m["study_spend_usd"])],
            ["Decisions that ended in an error", count(m["errors"])],
        ],
    )
    blocks["runs"] = table(
        ["Run", "Decisions", "Repetitions", "Hypotheses", "Seed", "Resamples", "Spend"],
        [
            [
                "`%s`" % r["run"],
                count(r["decisions"]),
                str(r["repetitions"]),
                str(r["hypotheses"]),
                str(r["seed"]),
                count(r["resamples"]),
                usd(r["spend_usd"]),
            ]
            for r in n["run"]
        ],
    )
    blocks["cells"] = table(
        ["Model", "Arm", "Catch rate", "False rejects", "Balanced accuracy", "Decided correctly", "Unsure", "Errors", "Median time"],
        [
            [
                c["model"],
                ARM_WORDS[c["arm"]],
                "%s (%d of %d)" % (pct(c["catch_rate"]), c["detected"], c["expected_fail"]),
                "%s (%d of %d)" % (pct(c["false_reject_rate"]), c["false_rejects"], c["expected_pass"]),
                pct(c["balanced_accuracy"]),
                "%s (%d of %d)" % (pct(c["correct_rate"]), c["decided_correctly"], c["items"]),
                str(c["unsure"]),
                str(c["errors"]),
                "%.1f s" % c["median_wall_s"],
            ]
            for c in n["cell"]
        ],
    )
    blocks["outcomes"] = table(
        ["Model", "Arm", "Fail: caught", "Fail: let through", "Fail: to review", "Pass: cleared", "Pass: rejected", "Pass: to review"],
        [
            [
                c["model"],
                ARM_WORDS[c["arm"]],
                str(c["detected"]),
                str(c["missed"]),
                str(c["review_on_fail"]),
                str(c["cleared"]),
                str(c["false_rejects"]),
                str(c["review_on_pass"]),
            ]
            for c in n["cell"]
        ],
    )
    blocks["effects"] = effects_table(n["effect"])
    blocks["sealed-effects"] = effects_table(n["sealed_effect"])
    blocks["repeat"] = table(
        ["Model", "Arm", "Items", "Fail or not fail differs", "Verdict differs", "Catch rate, first and second", "Balanced accuracy, first and second"],
        [
            [
                r["model"],
                ARM_WORDS[r["arm"]],
                str(r["items"]),
                str(r["fail_indicator_disagreements"]),
                str(r["verdict_disagreements"]),
                "%s and %s" % (pct(r["catch_rate_rep_1"]), pct(r["catch_rate_rep_2"])),
                "%s and %s" % (pct(r["balanced_accuracy_rep_1"]), pct(r["balanced_accuracy_rep_2"])),
            ]
            for r in n["repeat"]
        ],
    )
    blocks["errors"] = table(
        ["Model", "Arm", "Repetition", "Errors", "On items expected to fail", "Class"],
        [
            [
                e["model"],
                ARM_WORDS[e["arm"]],
                str(e["repetition"]),
                str(e["errors"]),
                str(e["on_expected_fail"]),
                "`%s`" % e["error_classes"],
            ]
            for e in n["error"]
        ],
    )
    by_model = {}
    for e in n["error"]:
        by_model[e["model"]] = by_model.get(e["model"], 0) + e["errors"]
    blocks["errors-by-model"] = table(
        ["Model", "Errors, every repetition"],
        [[model, str(by_model[model])] for model in sorted(by_model)] + [["All models", str(m["errors"])]],
    )
    kind_names = sorted({k["kind"] for k in n["error_kind"]})
    kind_total = {k: sum(row["errors"] for row in n["error_kind"] if row["kind"] == k) for k in kind_names}
    blocks["error-kinds"] = table(
        ["What the submission was refused for", "Errors"],
        [[KIND_WORDS.get(k, k), str(kind_total[k])] for k in sorted(kind_names, key=lambda k: (-kind_total[k], k))],
    )
    blocks["spend"] = table(
        ["Model", "Arm", "Decisions", "Spend", "Per decision"],
        [
            [
                c["model"],
                ARM_WORDS[c["arm"]],
                count(c["decisions_all_repetitions"]),
                usd(c["spend_usd"]),
                "$%.4f" % (c["spend_usd"] / c["decisions_all_repetitions"]),
            ]
            for c in n["cell"]
        ],
    )
    blocks["earlier"] = earlier_table(n, by_model)
    return blocks


def span(row, name):
    return "%s to %s" % (signed(row[name + "_low"]), signed(row[name + "_high"]))


def earlier_table(n, errors_by_model):
    """The figures first reported for this study, beside what these tables compute. The
    first column of figures is fixed: it is what was said before this script existed."""
    m = n["meta"]

    def pick(rows):
        found = [r for r in rows if r["model"] == "gemini-3.8-flash" and r["tool"] == "D_regions"]
        return found[0] if found else None

    sealed, corrected = pick(n["sealed_effect"]), pick(n["effect"])
    absent = "not in these numbers"
    if sealed and corrected:
        gain = "%s as sealed, %s after corrections" % (signed(sealed["balanced_points"]), signed(corrected["balanced_points"]))
        gain_status = "Reproduced as sealed" if signed(sealed["balanced_points"]) == "+5.6" else "Changed"
        width = "%s as sealed, %s after corrections" % (span(sealed, "balanced"), span(corrected, "balanced"))
        width_status = "Reproduced" if span(sealed, "balanced") == "+1.2 to +10.6" else "Changed"
    else:
        gain = width = gain_status = width_status = absent
    haiku = errors_by_model.get("claude-haiku-5-5", 0)
    others = m["errors"] - haiku
    classes = sorted({c for e in n["error"] for c in e["error_classes"].split(",")})
    return table(
        ["Figure", "First reported", "Computed here", "Result"],
        [
            ["Gain from regions for gemini-3.8-flash, balanced accuracy, points", "+5.6", gain, gain_status],
            ["Its 97.5% interval", "+1.2 to +10.6", width, width_status],
            [
                "Errors, and whose",
                "31, all claude-haiku-5-5",
                "%d in all: %d claude-haiku-5-5, %d other models" % (m["errors"], haiku, others),
                "Reproduced" if haiku == 31 and others == 0 else "Changed",
            ],
            [
                "What the errors were",
                "Submissions in the wrong form",
                "Every error is `%s`" % "`, `".join(classes) if classes else "No errors",
                "Reproduced" if classes == ["submission_invalid"] else "Changed",
            ],
            [
                "Spend, six study runs",
                "About $148",
                usd(m["study_spend_usd"]),
                "Reproduced" if "%.0f" % m["study_spend_usd"] == "148" else "Changed",
            ],
            [
                "Spend, with the gate run",
                "$158.58",
                usd(m["spend_usd"]),
                "Reproduced" if usd(m["spend_usd"]) == "$158.58" else "Changed",
            ],
        ],
    )


KIND_WORDS = {
    "location": "A finding's location was invalid",
    "rule_kind": "A finding used a rule in a way the rule does not allow",
    "checklist": "The checklist was not filled in as required",
    "evidence": "Evidence named a tool call that did not complete",
    "image": "A rule was applied to an image it does not judge",
    "other": "Something else",
}


def render(numbers, document):
    blocks = render_blocks(numbers)
    seen = set()

    def swap(match):
        name = match.group(2)
        if name not in blocks:
            raise TablesError("the document has a generated block these tables do not know: %s" % name)
        seen.add(name)
        return match.group(1) + blocks[name] + match.group(4)

    out = BLOCK.sub(swap, document)
    missing = sorted(set(blocks) - seen)
    if missing:
        raise TablesError("the document has no block for: %s" % ", ".join(missing))
    return out


# --- checking the document against the numbers ------------------------------------------

FIGURE = re.compile(r"(?<![\w.#-])[+-]?\$?\d[\d,]*(?:\.\d+)?%?")
# Figures the prose may state that are not results: the method's constants.
METHOD = {"0.025", "0.05", "95", "97.5", "98.33", "64", "256", "2026", "3.9"}


def bare(token):
    return token.replace(",", "").replace("$", "").replace("%", "").lstrip("+")


def figures_in(text):
    return {bare(t) for t in FIGURE.findall(text)}


def known_figures(numbers, blocks):
    known = set(METHOD)
    for block in blocks.values():
        known |= figures_in(block)
    rows = [numbers["meta"]] + [row for name in numbers if name != "meta" for row in numbers[name]]
    for row in rows:
        for value in row.values():
            if isinstance(value, bool) or not isinstance(value, (int, float)):
                continue
            known.add(bare(toml_value(value)))
            if isinstance(value, float):
                known |= {"%.1f" % value, "%.2f" % value, "%.1f" % (100 * value), "%.1f" % abs(value), "%.2f" % abs(value)}
    return known


def check_document(numbers, document):
    """The problems of a document against its numbers; an empty list when there are none."""
    problems = []
    try:
        if render(numbers, document) != document:
            problems.append("a generated block differs from what the numbers render to: run `tables.py render`")
    except TablesError as e:
        problems.append(str(e))
        return problems
    blocks = render_blocks(numbers)
    prose = BLOCK.sub("", document)
    prose = re.sub(r"`[^`\n]*`", "", prose)  # names of files, runs and commands
    prose = re.sub(r"^#+ .*$", "", prose, flags=re.M)
    known = known_figures(numbers, blocks)
    for token in FIGURE.findall(prose):
        value = bare(token)
        plain = value.lstrip("-")
        small = re.fullmatch(r"\d+", plain) and int(plain) < 100 and not token.endswith("%") and "$" not in token
        if small or value in known or plain in known:
            continue
        problems.append("the prose states %s, which the numbers file does not hold" % token)
    return problems


def check_manifest(numbers, manifest_text):
    """Counts and spend per model and arm must equal the baseline manifest's."""
    top, tables = read_toml(manifest_text)
    problems = []
    if top.get("digest") != numbers["meta"].get("package_digest"):
        problems.append("the numbers were computed from another package than the manifest names")
    want = {(c["model"], c["arm"]): (c["decisions"], c["spend_usd"]) for c in tables.get("cell", [])}
    have = {
        ("%s:%s" % (c["profile"], c["model"]), c["arm"]): (c["decisions_all_repetitions"], c["spend_usd"])
        for c in numbers["cell"]
    }
    if set(want) != set(have):
        problems.append("the numbers and the manifest name different models and arms")
    for key in sorted(set(want) & set(have)):
        if want[key][0] != have[key][0] or abs(want[key][1] - have[key][1]) > 0.00005:
            problems.append("%s %s: the manifest has %s decisions and %s spent" % (key + want[key]))
    return problems


# --- the command line -------------------------------------------------------------------


def read_text(path):
    with open(path, "rb") as f:
        return f.read().decode("utf-8")


def write_text(path, text):
    with open(path, "wb") as f:
        f.write(text.encode("utf-8"))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command")
    p = sub.add_parser("compute")
    p.add_argument("package")
    p.add_argument("--toml", required=True)
    p.add_argument("--resamples", type=int)
    p = sub.add_parser("render")
    p.add_argument("toml")
    p.add_argument("document")
    p = sub.add_parser("check")
    p.add_argument("toml")
    p.add_argument("document")
    p.add_argument("--manifest")
    args = parser.parse_args(argv)
    try:
        if args.command == "compute":
            numbers = compute(args.package, args.resamples)
            write_text(args.toml, to_toml(numbers))
            m = numbers["meta"]
            print("tables: %d runs, %d decisions, %d cells, %d effects" % (m["runs"], m["decisions"], len(numbers["cell"]), len(numbers["effect"])))
            return 0
        if args.command == "render":
            numbers = from_toml(read_text(args.toml))
            write_text(args.document, render(numbers, read_text(args.document)))
            return 0
        if args.command == "check":
            numbers = from_toml(read_text(args.toml))
            problems = check_document(numbers, read_text(args.document))
            if args.manifest:
                problems += check_manifest(numbers, read_text(args.manifest))
            for problem in problems:
                print("tables: " + problem, file=sys.stderr)
            if problems:
                return 1
            print("tables: the document agrees with its numbers")
            return 0
    except TablesError as e:
        print("tables: " + str(e), file=sys.stderr)
        return 1
    except OSError as e:
        # The reason only: a path under the package can hold an item's name.
        print("tables: a file could not be read or written: " + (e.strerror or "unknown"), file=sys.stderr)
        return 1
    except (KeyError, ValueError, TypeError) as e:
        print("tables: an input is not in the shape these tables read (%s)" % type(e).__name__, file=sys.stderr)
        return 1
    parser.print_usage(sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
