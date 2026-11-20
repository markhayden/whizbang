#!/usr/bin/env python3
"""The tally of scripts/loop-evidence.sh (plan m4, task E1, items 1 and 2): what the loop
`remove_checked` did on real models, as aggregates.

Python 3.9 or later, standard library only. It reads two things and nothing else: the
committed manifest of the edit set, and run records as `whizbang runs export` writes them,
one JSON object a line. It never opens an env file, a config or a store.

  loop-evidence.py entries --manifest M --set FOLDER
      The entries the evidence runs, checked before anything is imported: those of the
      manifest whose recipe is `remove_prompt`, exactly 24, each with its picture in the
      folder under the SHA-256 the manifest pins, and none that names a truth mask or a
      clean picture. Prints one line an entry, in the manifest's order: its id, the
      picture's path and the phrase, separated by tabs. On any difference it prints one
      line naming the entry and exits 1, having printed no entry.

  loop-evidence.py tally --records R --manifest M --judge J --commit C --whizbang V --out F
      The aggregates of the runs whose request id is `m4-loop-<judge>-<item id>`, and of
      the runs under them. Writes F, which holds no item: no id, no picture's name, no
      path, no identity of an artifact, no word of a judge.

  loop-evidence.py case --records R --request ID --name N --commit C --whizbang V --out F
      The fields of one run made by hand: its outcome, its plan, its attempts, its verdict,
      why the attempts ended, and what it reserved and settled.

  loop-evidence.py page --tally F --tally F --case F --case F --case F --out PAGE
      The page docs/results/m4-loop.md, from the two tallies and the three cases.

What it reads of a record is the record's own fields (ADR 0005, `run/6`): a loop's plan,
its attempts and why they ended, each attempt's check by the runs the attempt lists, and
what the record keeps of a check's findings, which is counts and places and never a text.
"""

import argparse
import hashlib
import json
import os
import re
import sys

sys.dont_write_bytecode = True

SCHEMA = "loop-evidence/1"
ACTION = "remove_checked"
CHECK = "source-fidelity"
RECIPE = "remove_prompt"
ENTRIES = 24

# What must be preserved, and the requirement that the named thing is gone: the ids the
# loop's table reads (crates/whizbang-rascal/src/checked.rs, `table`).
PRESERVED = ("SF1", "SF2", "RQ2")
NAMED = "RQ1"

# Why a loop's attempts ended, as a closed set: each sentence the loop writes, with every
# number replaced, is one of these or `other`. A sentence never reaches the file.
REASONS = (
    ("passed", r"^attempt N passed$"),
    ("findings_cut_short", r"^the findings of attempt N were cut short"),
    ("damaged_something_preserved", r"^attempt N changed something that must be preserved"),
    ("nothing_says_it_remains", r"^no finding of attempt N says with confidence"),
    ("last_attempt_of_the_plan", r"^the last attempt the plan allows$"),
    ("wider_mask_already_used", r"^a wider mask was already used once$"),
    ("mask_is_the_one_before", r"^the mask of attempt N is the one before it"),
    ("one_unchecked_pass", r"^one unchecked pass: "),
    ("check_could_not_run", r"^the check of attempt N could not run$"),
    ("budget_ran_out", r"^the budget ran out after attempt N$"),
    ("deadline_passed", r"^the deadline passed after attempt N$"),
    ("retry_could_not_be_made", r"^the retry .*and the attempts before it stand$"),
    ("never_said", r"^the run ended before it said why its attempts did$"),
)
ROWS = (
    "row_0_cut_short",
    "row_1_damaged",
    "row_2_remains",
    "row_3_not_sure",
    "row_4_no_violation",
    "row_5_other_violation",
)


class Refused(Exception):
    """What stops a command, in one line."""


# --- the manifest -----------------------------------------------------------------------


def read_tables(text, name):
    """The `[[name]]` tables of a manifest written by `whizbang eval edits draw`: each a
    dict of its `key = value` lines, a quoted string without its quotes, anything else as
    it is written. The manifest is a program's output, one key a line; a line this reader
    cannot place is refused, never skipped."""
    tables, current = [], None
    for number, line in enumerate(text.split("\n"), 1):
        line = line.rstrip("\r")
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        heading = re.match(r"^\[\[([A-Za-z0-9_-]+)\]\]\s*$", line) or re.match(
            r"^\[([A-Za-z0-9_.-]+)\]\s*$", line
        )
        if heading:
            current = {} if heading.group(1) == name and line.startswith("[[") else None
            if current is not None:
                tables.append(current)
            elif heading.group(1) == name:
                raise Refused("line %d: %s is not written as [[%s]]" % (number, name, name))
            continue
        pair = re.match(r"^([A-Za-z0-9_-]+) = (.*)$", line)
        if not pair:
            raise Refused("line %d of the manifest is not a table or `key = value`" % number)
        if current is None:
            continue
        key, value = pair.group(1), pair.group(2).strip()
        if key in current:
            raise Refused("line %d of the manifest gives %s twice" % (number, key))
        quoted = re.match(r'^"([^"\\]*)"$', value)
        if value.startswith('"') and not quoted:
            raise Refused("line %d of the manifest holds a string this reader cannot read" % number)
        current[key] = quoted.group(1) if quoted else value
    return tables


def agrees_with_tomllib(text, name, got):
    """Where the standard library reads TOML in full (Python 3.11 on), the small reader
    must agree with it on every string it took."""
    try:
        import tomllib
    except ImportError:
        return
    full = tomllib.loads(text).get(name, [])
    want = [{k: v for k, v in row.items() if isinstance(v, str)} for row in full]
    have = [{k: v for k, v in row.items() if k in wanted} for row, wanted in zip(got, want)]
    if len(full) != len(got) or want != have:
        raise Refused("the small TOML reader disagrees with tomllib on the %s tables" % name)


def manifest(path):
    with open(path, "rb") as f:
        raw = f.read()
    text = raw.decode("utf-8")
    items = read_tables(text, "item")
    sources = read_tables(text, "source")
    agrees_with_tomllib(text, "item", items)
    agrees_with_tomllib(text, "source", sources)
    return "sha256:" + hashlib.sha256(raw).hexdigest(), items, sources


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return "sha256:" + digest.hexdigest()


def entries(manifest_path, folder=None):
    """The entries the evidence runs, each (id, picture, phrase); checked whole, against
    the folder too when one is given, before one is handed back."""
    digest, items, sources = manifest(manifest_path)
    chosen = [item for item in items if item.get("recipe") == RECIPE]
    for at, item in enumerate(chosen, 1):
        for key in ("id", "picture", "picture_sha256", "phrase"):
            if not item.get(key):
                raise Refused("entry %d of %s has no %s" % (at, RECIPE, key))
    if len(chosen) != ENTRIES:
        last = chosen[-1]["id"] if chosen else "none"
        raise Refused(
            "the manifest holds %d entries of %s, and the evidence is of exactly %d (the last is %s)"
            % (len(chosen), RECIPE, ENTRIES, last)
        )
    # What an entry may not name: a truth mask or a clean picture, by path or by content.
    not_a_picture = {}
    for item in items:
        for key in ("truth", "truth_sha256"):
            if item.get(key):
                not_a_picture[item[key]] = "a truth mask"
    for source in sources:
        for key in ("clean", "clean_sha256"):
            if source.get(key):
                not_a_picture[source[key]] = "a clean picture"
    seen = set()
    for item in chosen:
        name, picture = item["id"], item["picture"]
        if not re.match(r"^[a-z0-9][a-z0-9_-]{0,63}$", name):
            raise Refused("an entry's id is not letters, digits, _ and -: %r" % name[:64])
        if name in seen:
            raise Refused("the entry %s is in the manifest twice" % name)
        seen.add(name)
        if "\t" in item["phrase"] or "\n" in item["phrase"] or not item["phrase"].strip():
            raise Refused("the entry %s has a phrase that is empty or holds a tab" % name)
        for held in (picture, item["picture_sha256"]):
            if held in not_a_picture:
                raise Refused("the entry %s names %s" % (name, not_a_picture[held]))
        if picture.endswith("-truth.png"):
            raise Refused("the entry %s names a truth mask" % name)
        parts = picture.split("/")
        if picture.startswith("/") or "\\" in picture or any(p in ("", ".", "..") for p in parts):
            raise Refused("the entry %s names a picture outside the set's folder" % name)
        if not re.match(r"^sha256:[0-9a-f]{64}$", item["picture_sha256"]):
            raise Refused("the entry %s pins no SHA-256" % name)
    if folder is not None:
        for item in chosen:
            path = os.path.join(folder, *item["picture"].split("/"))
            if os.path.islink(path) or not os.path.isfile(path):
                raise Refused("the entry %s: its picture is not in the folder" % item["id"])
            if sha256_of(path) != item["picture_sha256"]:
                raise Refused(
                    "the entry %s: its picture is not the one the manifest pins" % item["id"]
                )
    return digest, [(item["id"], item["picture"], item["phrase"]) for item in chosen]


# --- the records ------------------------------------------------------------------------


def load_records(path):
    """Every record of an export, by its run's id."""
    records = {}
    with open(path, "rb") as f:
        for number, line in enumerate(f.read().decode("utf-8").split("\n"), 1):
            if not line.strip():
                continue
            try:
                record = json.loads(line)
                records[record["identity"]["run"]] = record
            except (ValueError, KeyError, TypeError):
                raise Refused("line %d of the export is not a run's record" % number)
    return records


def tree_of(records, top):
    """The run `top` and every run under it, by the parent each record names."""
    children = {}
    for run, record in records.items():
        children.setdefault(record["identity"].get("parent"), []).append(run)
    found, queue = [], [top]
    while queue:
        run = queue.pop()
        found.append(run)
        queue.extend(children.get(run, []))
    return [records[run] for run in found]


def argument(record, name):
    for given in record["asked"].get("arguments", []):
        if given.get("name") == name and isinstance(given.get("value"), dict):
            return given["value"].get("text")
    return None


def setting(record, name):
    for given in record["asked"].get("settings", []):
        if given.get("name") == name and isinstance(given.get("value"), dict):
            return given["value"].get("text")
    return None


def row_of(found):
    """The row of the loop's table a check's findings are of, from what a record keeps of
    them: whether they are complete, and how many cite each obligation with each
    disposition and confidence. A finding that cites two obligations is counted under
    each, which decides no row wrongly: row 1 is any violation that cites something to be
    preserved, and without one every confident violation of the named requirement is a
    finding of row 2."""
    if not found.get("complete", False):
        return "row_0_cut_short"
    violations = [c for c in found.get("counts", []) if c.get("disposition") == "violation"]
    if any(c.get("obligation") in PRESERVED for c in violations):
        return "row_1_damaged"
    named = [c for c in violations if c.get("obligation") == NAMED]
    if any(c.get("confidence") == "confident" for c in named):
        return "row_2_remains"
    if named:
        return "row_3_not_sure"
    if violations:
        return "row_5_other_violation"
    return "row_4_no_violation"


def reason_of(sentence):
    plain = re.sub(r"[0-9]+", "N", sentence or "")
    for name, pattern in REASONS:
        if re.search(pattern, plain):
            return name
    return "other"


def word(text, pattern=r"^[a-z][a-z0-9_-]{0,31}$"):
    """A short word of a record, or `other`: nothing longer or stranger reaches the file."""
    return text if isinstance(text, str) and re.match(pattern, text) else "other"


def model_line(model):
    """A backend's model as one line, of plain characters alone."""
    parts = [model.get("backend"), model.get("model"), model.get("version")]
    line = " ".join(str(part) for part in parts if part)
    return line if re.match(r"^[A-Za-z0-9 ._:+-]{1,200}$", line) else "other"


def models_of(tree, capability):
    lines = set()
    for record in tree:
        for model in record.get("models", []):
            if model.get("capability") == capability:
                lines.add(model_line(model))
    return ", ".join(sorted(lines)) if lines else "none"


def check_of(records, attempt):
    """The record of the check an attempt made: the run among its runs that is squint's
    checking recipe."""
    for run in attempt.get("runs", []):
        record = records.get(run)
        if record and record["asked"].get("action") == CHECK:
            return record
    return None


def status_of(record):
    if record["outcome"].get("lost"):
        return "interrupted"
    return word(record["outcome"].get("status"))


def spend_of(tree):
    settled = unresolved = 0
    for record in tree:
        for request in record.get("spend", {}).get("requests", []):
            settled += int(request.get("settled_micros", 0))
            unresolved += int(request.get("unresolved_micros", 0))
    return settled, unresolved


def duration_of(record):
    time = record.get("time", {})
    if time.get("start_ms") is None:
        return None
    return max(0, int(time["end_ms"]) - int(time["start_ms"]))


def median(values):
    ordered = sorted(values)
    if not ordered:
        return 0
    middle = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[middle]
    return (ordered[middle - 1] + ordered[middle]) // 2


def how_it_ended(record):
    """One of: passed at first, passed after a retry, kept unsure, kept fail, unchecked,
    error."""
    attempts = record.get("attempts") or {}
    kept = attempts.get("kept")
    if status_of(record) != "completed" or kept is None:
        return "error"
    listed = [a for a in attempts.get("list", []) if a.get("attempt") == kept]
    verdict = listed[0].get("verdict") if listed else None
    if verdict == "pass":
        return "passed_at_first" if kept == 1 else "passed_after_retry"
    if verdict == "unsure":
        return "kept_unsure"
    if verdict == "fail":
        return "kept_fail"
    return "unchecked"


# --- writing ----------------------------------------------------------------------------


def toml_value(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    text = str(value)
    if '"' in text or "\\" in text or "\n" in text:
        raise Refused("a value cannot be written: it holds a quote, a backslash or a line break")
    return '"%s"' % text


def toml_lines(fields):
    return ["%s = %s" % (name, toml_value(value)) for name, value in fields]


def write_out(path, text):
    if path == "-":
        sys.stdout.write(text)
        return
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def held_commit(commit):
    if not re.match(r"^[0-9a-f]{40}$", commit):
        raise Refused("the commit is not 40 hexadecimal digits")
    return commit


def held_version(version):
    if not re.match(r"^[0-9]+\.[0-9]+\.[0-9]+$", version):
        raise Refused("the version is not three numbers")
    return version


# --- tally ------------------------------------------------------------------------------


def tally(args):
    digest, chosen = entries(args.manifest)
    if not re.match(r"^[a-z][a-z0-9_-]{0,63}$", args.judge):
        raise Refused("the judge is not a backend's name")
    records = load_records(args.records)
    by_request = {}
    for record in records.values():
        request = record["identity"].get("request")
        if request and record["asked"].get("action") == ACTION:
            by_request[request] = record
    loops = []
    for name, _, _ in chosen:
        record = by_request.get("m4-loop-%s-%s" % (args.judge, name))
        if record is None:
            continue
        if argument(record, "judge") != args.judge:
            raise Refused("a run under this judge's request id was asked of another judge")
        loops.append(record)

    outcomes = dict.fromkeys(("completed", "error", "invalid", "cancelled", "interrupted", "other"), 0)
    ended = dict.fromkeys(
        ("passed_at_first", "passed_after_retry", "kept_unsure", "kept_fail", "unchecked", "error"), 0
    )
    made = {1: 0, 2: 0, 3: 0}
    allowed = {1: 0, 2: 0, 3: 0}
    reasons = dict.fromkeys([name for name, _ in REASONS] + ["other", "no_attempt"], 0)
    rows = dict.fromkeys(ROWS + ("passed", "not_checked", "no_attempt"), 0)
    verdicts = dict.fromkeys(("pass", "fail", "unsure", "unchecked"), 0)
    errors = {}
    counts = {}
    plan = {"checked": 0, "unchecked": 0, "none": 0}
    retries = {"wider": 0, "located": 0}
    places = dict.fromkeys(
        (
            "cut",
            "found_nothing",
            "masks_carried",
            "cited_by_checks",
            "cited_as_polygons",
            "retries_wider_though_a_check_cited_a_place",
        ),
        0,
    )
    attempts_made = unsure_rascal = not_complete = 0
    reserved = settled = unresolved = by_attempts = 0
    times, templates, configs, versions, whole = [], set(), set(), set(), []
    for record in loops:
        tree = tree_of(records, record["identity"]["run"])
        whole.extend(tree)
        status = status_of(record)
        outcomes[status if status in outcomes else "other"] += 1
        ended[how_it_ended(record)] += 1
        if status == "error":
            kind = word((record["outcome"].get("error") or {}).get("kind"))
            errors[kind] = errors.get(kind, 0) + 1
        versions.add("%s %s" % (word(record["asked"].get("action")), record["asked"].get("version")))
        templates.add(setting(record, "job.template") or "none")
        configs.add(record["asked"].get("config") or "none")
        stated = (record.get("plan") or {}).get("attempts")
        if stated is None:
            plan["none"] += 1
        else:
            plan["checked" if stated.get("checks") else "unchecked"] += 1
            if stated.get("allowed") in allowed:
                allowed[stated["allowed"]] += 1
        attempted = record.get("attempts") or {}
        listed = attempted.get("list", [])
        attempts_made += len(listed)
        if len(listed) in made:
            made[len(listed)] += 1
        reasons[reason_of(attempted.get("ended")) if listed else "no_attempt"] += 1
        before = None
        for attempt in listed:
            by_attempts += int(attempt.get("settled_micros", 0))
            verdicts[attempt.get("verdict") or "unchecked"] += 1
            changed = attempt.get("changed") or {}
            if changed.get("change") == "wider":
                retries["wider"] += 1
                if before is not None and before.get("places"):
                    places["retries_wider_though_a_check_cited_a_place"] += 1
            elif changed.get("change") == "located":
                retries["located"] += 1
                crops = changed.get("crops", [])
                places["cut"] += len({crop.get("crop") for crop in crops})
                places["found_nothing"] += sum(1 for crop in crops if crop.get("mask") is None)
                places["masks_carried"] += sum(1 for crop in crops if crop.get("mask") is not None)
            check = check_of(records, attempt)
            found = (check or {}).get("findings")
            before = found
            if found is None:
                continue
            for count in found.get("counts", []):
                key = (
                    word(count.get("obligation"), r"^[A-Z]{1,8}[0-9]{1,4}$"),
                    word(count.get("disposition")),
                    word(count.get("confidence") or "none"),
                )
                counts[key] = counts.get(key, 0) + int(count.get("count", 0))
            places["cited_by_checks"] += len(found.get("places", []))
            places["cited_as_polygons"] += sum(1 for p in found.get("places", []) if "polygon" in p)
        # The row of the attempt the loop stopped on: the last that was judged.
        judged = [a for a in listed if a.get("verdict")]
        if not listed:
            rows["no_attempt"] += 1
        elif not judged:
            rows["not_checked"] += 1
        else:
            final = judged[-1]
            found = (check_of(records, final) or {}).get("findings")
            if final.get("verdict") == "pass":
                rows["passed"] += 1
            elif found is None:
                rows["row_0_cut_short"] += 1
                not_complete += 1
            else:
                row = row_of(found)
                rows[row] += 1
                if row == "row_0_cut_short":
                    not_complete += 1
                if row == "row_3_not_sure" and final.get("verdict") == "unsure" and found.get("places"):
                    unsure_rascal += 1
        reserved += int(record.get("spend", {}).get("reserved_micros") or 0)
        took = duration_of(record)
        if took is not None:
            times.append(took)
    part_settled, part_unresolved = spend_of(whole)
    settled += part_settled
    unresolved += part_unresolved

    def joined(values):
        return ", ".join(sorted(values)) if values else "none"

    for values in (templates, configs):
        for value in values:
            if value != "none" and not re.match(r"^sha256:[0-9a-f]{64}$", value):
                raise Refused("a record names a digest that is not one")
    out = [
        "# What the loop `remove_checked` did on the %d entries of `remove_prompt` of the edit" % ENTRIES,
        "# set, under one judge (plan m4, task E1, item 1). Written by scripts/loop-evidence.sh;",
        "# never edited by hand. Aggregates only: no item, no picture, no path, no word of a judge.",
        "#",
        "# `unsure_a_retry_under_rascals_rule_would_have_followed` is an upper bound: the runs that",
        "# stopped on an `unsure` whose check held a tentative violation of RQ1 and cited any place.",
        "# A record keeps a check's places apart from its findings, so the place may be another",
        "# finding's. `stopped_by_row` is the row of the loop's table the last judged attempt's",
        "# findings are of; `stopped_by_reason` is the sentence the loop wrote, as one of a closed set.",
    ]
    out += toml_lines(
        [
            ("schema", SCHEMA),
            ("commit", held_commit(args.commit)),
            ("whizbang", held_version(args.whizbang)),
            ("judge", args.judge),
            ("judge_model", models_of(whole, "judge")),
            ("filler", models_of(whole, "fill")),
            ("finder", models_of(whole, "segment")),
            ("action", joined(versions)),
            ("job_template", joined(templates)),
            ("config", joined(configs)),
            ("manifest_sha256", digest),
            ("entries", len(chosen)),
            ("runs", len(loops)),
            ("not_run", len(chosen) - len(loops)),
            ("attempts_made", attempts_made),
            ("unsure_a_retry_under_rascals_rule_would_have_followed", unsure_rascal),
            ("stopped_on_findings_not_complete", not_complete),
            ("median_ms", median(times)),
            ("least_ms", min(times) if times else 0),
            ("most_ms", max(times) if times else 0),
            ("reserved_micros", reserved),
            ("settled_micros", settled),
            ("settled_by_attempts_micros", by_attempts),
            ("unresolved_micros", unresolved),
        ]
    )

    def table(name, fields):
        return ["", "[%s]" % name] + toml_lines(fields)

    out += table("outcomes", sorted(outcomes.items()))
    out += table("errors_by_type", sorted(errors.items()))
    out += table("ended", list(ended.items()))
    out += table("plans", list(plan.items()) + [("allowed_%d" % n, allowed[n]) for n in (1, 2, 3)])
    out += table("runs_by_attempts_made", [("made_%d" % n, made[n]) for n in (1, 2, 3)])
    out += table("attempt_verdicts", list(verdicts.items()))
    out += table("retries", list(retries.items()))
    out += table("stopped_by_row", list(rows.items()))
    out += table("stopped_by_reason", list(reasons.items()))
    out += table("places", list(places.items()))
    for (obligation, disposition, confidence), count in sorted(counts.items()):
        out += ["", "[[finding]]"] + toml_lines(
            [
                ("obligation", obligation),
                ("disposition", disposition),
                ("confidence", confidence),
                ("count", count),
            ]
        )
    write_out(args.out, "\n".join(out) + "\n")
    print(
        "loop-evidence: %d of %d entries have a run; %d attempts; settled %d millionths of a dollar"
        % (len(loops), len(chosen), attempts_made, settled)
    )


# --- one case ---------------------------------------------------------------------------


def sentence(text):
    """A sentence of a record, kept only when it is plain and names no path."""
    if not isinstance(text, str) or not text:
        return "none"
    cut = text[:300]
    if re.search(r"[\"\\\x00-\x1f]", cut) or re.search(r"(^|[\s:=(])[~/]|[A-Za-z]:\\|sha256:", cut):
        return "(not kept: it holds a path, an identity or a character a line cannot)"
    return cut


def case(args):
    if not re.match(r"^[a-z][a-z0-9-]{0,31}$", args.name):
        raise Refused("the case's name is lower-case letters, digits and -")
    records = load_records(args.records)
    found = [
        r
        for r in records.values()
        if r["identity"].get("request") == args.request and r["asked"].get("action") == ACTION
    ]
    if len(found) != 1:
        raise Refused("the export holds %d runs of the loop under that request id" % len(found))
    record = found[0]
    tree = tree_of(records, record["identity"]["run"])
    attempted = record.get("attempts") or {}
    listed = attempted.get("list", [])
    kept = attempted.get("kept")
    kept_attempt = ([a for a in listed if a.get("attempt") == kept] or [None])[0]
    stated = (record.get("plan") or {}).get("attempts") or {}
    settled, unresolved = spend_of(tree)
    error = record["outcome"].get("error") or {}
    out = [
        "# One run of the loop `remove_checked` made by hand (plan m4, task E1, item 2).",
        "# Written by scripts/loop-evidence.sh --case; never edited by hand.",
    ]
    out += toml_lines(
        [
            ("schema", SCHEMA),
            ("case", args.name),
            ("commit", held_commit(args.commit)),
            ("whizbang", held_version(args.whizbang)),
            ("judge_model", models_of(tree, "judge")),
            ("filler", models_of(tree, "fill")),
            ("finder", models_of(tree, "segment")),
            ("outcome", status_of(record)),
            ("error_type", word(error.get("kind")) if error else "none"),
            ("error_said", sentence(error.get("message")) if error else "none"),
            ("attempts_asked", int(stated.get("asked", 0))),
            ("attempts_allowed", int(stated.get("allowed", 0))),
            ("plan_checks", bool(stated.get("checks", False))),
            ("plan_why", sentence((record.get("plan") or {}).get("why"))),
            ("attempts", len(listed)),
            ("kept", int(kept) if kept is not None else 0),
            ("checked", bool(kept_attempt and kept_attempt.get("verdict"))),
            ("verdict", (kept_attempt or {}).get("verdict") or "none"),
            ("unchecked_why", sentence((kept_attempt or {}).get("unchecked"))),
            ("ended", sentence(attempted.get("ended"))),
            ("ended_as", how_it_ended(record)),
            ("ms", duration_of(record) or 0),
            ("reserved_micros", int(record.get("spend", {}).get("reserved_micros") or 0)),
            ("settled_micros", settled),
            ("unresolved_micros", unresolved),
        ]
    )
    for attempt in listed:
        changed = attempt.get("changed") or {}
        check = check_of(records, attempt)
        found_of = (check or {}).get("findings")
        out += ["", "[[attempt]]"] + toml_lines(
            [
                ("attempt", int(attempt.get("attempt", 0))),
                ("verdict", attempt.get("verdict") or "none"),
                ("unchecked_why", sentence(attempt.get("unchecked"))),
                ("changed", word(changed.get("change") or "nothing")),
                ("places_cut", len({c.get("crop") for c in changed.get("crops", [])})),
                ("row", row_of(found_of) if found_of is not None else "none"),
                ("runs", len(attempt.get("runs", []))),
                ("settled_micros", int(attempt.get("settled_micros", 0))),
            ]
        )
    write_out(args.out, "\n".join(out) + "\n")
    print("loop-evidence: the case %s: %s, %d attempts" % (args.name, status_of(record), len(listed)))


# --- the page ---------------------------------------------------------------------------


def top_fields(text):
    """The fields of a tally or a case before its first table, as written."""
    fields = {}
    for line in text.split("\n"):
        if line.startswith("["):
            break
        pair = re.match(r"^([a-z0-9_]+) = (.*)$", line)
        if pair:
            fields[pair.group(1)] = pair.group(2).strip('"')
    return fields


def body(text):
    return "\n".join(line for line in text.split("\n") if not line.startswith("#")).strip("\n")


def page(args):
    if len(args.tally) != 2 or len(args.case) != 3:
        raise Refused("the page is of two tallies and three cases")
    tallies, cases = [], []
    for path in args.tally + args.case:
        with open(path, "rb") as f:
            text = f.read().decode("utf-8")
        fields = top_fields(text)
        if fields.get("schema") != SCHEMA:
            raise Refused("a file given to the page was not written by this script")
        (cases if "case" in fields else tallies).append((fields, body(text)))
    if len(tallies) != 2 or len(cases) != 3:
        raise Refused("the page is of two tallies and three cases")
    commits = {fields["commit"] for fields, _ in tallies + cases}
    spent = sum(int(fields["settled_micros"]) for fields, _ in tallies + cases)
    out = [
        "# The loop on real models: what happened",
        "",
        "Plan m4, task E1, items 1 to 3 (`SPEC.md` section 8, \"The ladder\"). The loop",
        "`remove_checked` (`docs/reference/tools/remove_checked.md`, `docs/guides/loop.md`) was run",
        "on the %d entries of `remove_prompt` of the synthetic edit set" % ENTRIES,
        "(`labs/m3-edits/edits.toml`), once under each of two judges, by",
        "`scripts/loop-evidence.sh`, with SAM 3 on the Mac finding the phrase and Bria Eraser",
        "through fal filling; and three more times by hand. Everything below was read from the",
        "runs' records by `whizbang runs export`. Only aggregates are kept, never an item.",
        "",
        "**What this is not.** It scores nothing. No attempt's fill is measured against a clean",
        "picture, and no first attempt is compared with a kept one: the scorer of milestone 3",
        "cannot read a loop's runs. The masks of milestone 3 covered the drawn ball whole, so",
        "most runs were expected to pass at first; the tallies say how many retried, whatever",
        "that number is. That each kind of retry works is shown by the loop's tests with fakes.",
        "Whether a retry makes a result better is rung 2 proper and rungs 3 and 4, and is",
        "milestone E's. The loop's visual quality is not yet confirmed by a person.",
        "",
        "**One count is an upper bound.** The loop never retries on an `unsure`; rascal retried on",
        "one that named a place. `unsure_a_retry_under_rascals_rule_would_have_followed` counts the",
        "runs that stopped on an `unsure` whose check held a tentative violation of `RQ1` and cited",
        "any place. A record keeps a check's places apart from its findings, so the place may be",
        "another finding's, and the true number is this or fewer.",
        "",
        "## The runs",
        "",
        "```toml",
        "commits = \"%s\"" % ", ".join(sorted(commits)),
        "tallies = \"2\"",
        "cases = \"3\"",
        "settled_micros = \"%d\"" % spent,
        "```",
    ]
    for fields, text in tallies:
        out += ["", "## The %d entries under `%s`" % (ENTRIES, fields["judge"]), "", "```toml", text, "```"]
    out += [
        "",
        "## Three cases by hand",
        "",
        "The cases the carry-over list keeps for the playground of milestone 5, run from the",
        "command line through the same script and under the same guards: `pass`, a clean pass,",
        "or a retry then a pass; `vague`, a phrase that names nothing in the picture; `no-key`,",
        "an env file without the judge's key.",
    ]
    for fields, text in cases:
        out += ["", "### `%s`" % fields["case"], "", "```toml", text, "```"]
    write_out(args.out, "\n".join(out) + "\n")
    print("loop-evidence: wrote the page from 2 tallies and 3 cases")


def main(argv):
    parser = argparse.ArgumentParser(prog="loop-evidence.py", description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    listing = commands.add_parser("entries")
    listing.add_argument("--manifest", required=True)
    listing.add_argument("--set", required=True)
    counting = commands.add_parser("tally")
    for name in ("--records", "--manifest", "--judge", "--commit", "--whizbang", "--out"):
        counting.add_argument(name, required=True)
    one = commands.add_parser("case")
    for name in ("--records", "--request", "--name", "--commit", "--whizbang", "--out"):
        one.add_argument(name, required=True)
    paging = commands.add_parser("page")
    paging.add_argument("--tally", action="append", default=[])
    paging.add_argument("--case", action="append", default=[])
    paging.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "entries":
            _, chosen = entries(args.manifest, args.set)
            for name, picture, phrase in chosen:
                print("%s\t%s\t%s" % (name, picture, phrase))
        elif args.command == "tally":
            tally(args)
        elif args.command == "case":
            case(args)
        else:
            page(args)
    except Refused as refused:
        print("loop-evidence: refused: %s" % refused, file=sys.stderr)
        return 1
    except OSError as error:
        print("loop-evidence: refused: a file could not be read or written (%s)" % error.strerror, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
