#!/usr/bin/env python3
"""Audits transcripts against the package they were converted from
(docs/baseline/transcript.md, "The audit").

    audit.py --package <package root> --transcripts <folder> [--origin historical]
             [--manifest docs/baseline/manifest.toml] [--summary <file.toml>]

Each transcript ends as one of three things:

  complete     the record is whole and every rule holds
  incomplete   the record is not whole (a stored file is missing, cut short by the recorder,
               or not what was recorded); it is listed, to be captured again
  invalid      the record is whole but breaks a rule; this is a defect to be understood

The audit covers the package, not only the transcripts it is handed: every fixture must have
exactly one transcript, and the transcripts must be the ones the converter listed.

Exit status: 0 when every fixture has a complete transcript, 3 when some are incomplete and
none is invalid, 1 when any is invalid or the coverage is wrong, 2 when there is nothing to
audit.

Prints counts only. What was found, per transcript, goes to audit-findings.jsonl in the
transcripts folder, which is private.

Python 3.9 or later, standard library only.
"""

import argparse
import collections
import json
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common  # noqa: E402

TERMINAL_EVENTS = ("verdict", "error", "cancelled")


class Findings:
    def __init__(self):
        self.incomplete = []
        self.invalid = []

    def missing(self, text):
        self.incomplete.append(text)

    def broken(self, text):
        self.invalid.append(text)


def check_files(t, package, f):
    """Every file the transcript names exists, is whole and has the recorded hash."""
    by_hash = {}
    for entry in t["files"]:
        role = entry["role"]
        if entry["truncated"]:
            f.missing("cut short by the recorder: %s" % role)
            continue
        if entry["sha256"] is None:
            f.missing("no such file: %s" % role)
            continue
        path = os.path.join(package, *entry["path"].split("/"))
        if not os.path.isfile(path) or os.path.islink(path):
            f.missing("no such file: %s" % role)
            continue
        if os.path.getsize(path) != entry["bytes"] or common.sha256_file(path) != entry["sha256"]:
            f.missing("not the recorded bytes: %s" % role)
            continue
        by_hash[entry["sha256"]] = path
    return by_hash


def kept_response(whole):
    return {
        "content": whole.get("content", []),
        "text": whole.get("text", []),
        "tool_calls": whole.get("tool_calls", []),
        "stop_reason": whole.get("stop_reason"),
    }


def check_embedded(t, by_hash, f):
    """What the transcript holds for a request and a response is what the stored file holds."""
    for turn in t["turns"]:
        meta = turn["meta"]
        path = by_hash.get(meta.get("request_file"))
        if path is None:
            f.missing("turn %d has no whole stored request" % turn["turn"])
        elif turn["request"] != common.load_json(path):
            f.broken("turn %d: the request is not the stored request" % turn["turn"])
        if not meta.get("answered"):
            if turn["response"] is not None:
                f.broken("turn %d: a response where none was recorded" % turn["turn"])
            continue
        path = by_hash.get(meta.get("response_file"))
        if path is None:
            f.missing("turn %d has no whole stored response" % turn["turn"])
        elif turn["response"] != kept_response(common.load_json(path)):
            f.broken("turn %d: the response is not the stored response" % turn["turn"])


def check_journal(t, package, fixture, by_hash, f):
    """Everything the transcript took from the journal and the old result is taken again,
    here, and must be what the transcript holds."""
    _name, decision_dir, run_dir = fixture
    events = common.load_lines(os.path.join(run_dir, "journal.jsonl"))
    if [e.get("seq") for e in events] != list(range(len(events))):
        f.broken("the journal's events are not numbered without a gap")
    ends = [i for i, e in enumerate(events) if e["event"] in TERMINAL_EVENTS]
    if ends != [len(events) - 1]:
        f.broken("the journal does not end with its one terminal event")
        return
    by_event = collections.defaultdict(list)
    for e in events:
        by_event[e["event"]].append(e["data"])

    last = events[-1]
    if last["event"] == "verdict":
        terminal = {"kind": "verdict", "verdict": last["data"]["verdict"]}
    else:
        terminal = {"kind": last["event"], "class": last["data"].get("class"), "message": last["data"].get("message")}
    if t["terminal"] != terminal:
        f.broken("the terminal outcome is not the journal's")

    repairs = [{"after_turn": d["after_turn"], "form": d["form"], "reason": d["reason"]} for d in by_event["repair"]]
    if t["repairs"] != repairs:
        f.broken("the repairs are not the journal's")

    resolved = by_event["resolved"]
    if len(resolved) != 1 or len(by_event["prepared"]) != 1:
        f.broken("the journal does not resolve and prepare exactly once")
        return
    if t["evidence_tools"] != resolved[0].get("tools"):
        f.broken("the evidence tools are not the journal's")

    result = common.load_json(os.path.join(os.path.dirname(decision_dir), "result.json"))
    decision = common.load_json(os.path.join(decision_dir, "decision.json"))
    model = decision.get("config", {}).get("provider", {}).get("model")
    subject = {
        "set": result.get("set"),
        "item": result.get("item"),
        "model": model,
        "arm": str(result.get("cell") or "").rsplit("/", 1)[-1],
        "repetition": result.get("repetition"),
        "use_case": decision.get("use_case"),
    }
    if t["subject"] != subject:
        f.broken("the subject is not the old result's")
    if terminal["kind"] == "verdict":
        if result.get("verdict") != terminal["verdict"] or result.get("error_class") or result.get("error"):
            f.broken("the verdict is not the old result's")
    elif result.get("error_class") != terminal["class"] or result.get("error") != terminal["message"]:
        f.broken("the error is not the old result's")

    prep_path = os.path.join(package, "assembly", "generated", "preparation.json")
    settings = None
    if os.path.isfile(prep_path) and model:
        settings = common.load_json(prep_path).get("settings", {}).get(model.split(":")[-1])
    if t["preparation"] != {"version": by_event["prepared"][0].get("prep_version"), "settings": settings}:
        f.broken("the preparation is not the journal's version and the package's settings")

    # Turns: the stored request and response of each, and which were excluded.
    sent = {d["turn"]: d for d in by_event["turn_sent"]}
    received = {d["turn"]: d for d in by_event["turn_received"]}
    excluded = {d["turn"]: d["reason"] for d in by_event["turn_excluded"]}
    if sorted(sent) != [turn["turn"] for turn in t["turns"]]:
        f.broken("the turns are not the journal's turns")
        return
    for turn in t["turns"]:
        number = turn["turn"]
        if turn["meta"].get("request_file") != sent[number]["request"]["sha256"]:
            f.broken("turn %d: not the request the journal names" % number)
        got = received.get(number)
        if bool(turn["meta"].get("answered")) != (got is not None):
            f.broken("turn %d: answered in one record and not the other" % number)
        elif got is not None and turn["meta"].get("response_file") != got["response"]["sha256"]:
            f.broken("turn %d: not the response the journal names" % number)
        reason = (turn["excluded"] or {}).get("reason")
        if reason != excluded.get(number):
            f.broken("turn %d: excluded in one record and not the other" % number)

    # The old run's own report, written when the run ended, names the same turns.
    report = common.load_json(os.path.join(decision_dir, "report.json"))
    runs = report.get("runs", [])
    if len(runs) != 1 or runs[0].get("terminal") != terminal["kind"]:
        f.broken("the old report does not hold one run that ended the same way")
    else:
        told = [
            (r.get("turn"), (r.get("request") or {}).get("sha256"), (r.get("response") or {}).get("sha256"))
            for r in runs[0].get("turns", [])
        ]
        held = [
            (turn["turn"], turn["meta"].get("request_file"), turn["meta"].get("response_file"))
            for turn in t["turns"]
        ]
        if told != held:
            f.broken("the turns are not the turns of the old report")

    # Images: the job's, then each returned image, with the bytes that were sent.
    images = []
    images_sent = resolved[0].get("images_sent", {})
    for image in resolved[0].get("images", []):
        ref = images_sent.get("raw:%s" % image.get("id"))
        images.append(
            {
                "id": image.get("id"),
                "frame": "raw:%s" % image.get("id"),
                "origin": "job",
                "role": image.get("role"),
                "width": image.get("width"),
                "height": image.get("height"),
                "source": {"sha256": image.get("sha256"), "pixel_sha256": image.get("pixel_sha256")},
                "prepared": {"sha256": ref["sha256"], "bytes": ref["bytes"]} if ref else None,
            }
        )
    dispatched = {}
    for d in by_event["tool_called"]:
        if d.get("call_id") and d["call_id"] not in dispatched:
            dispatched[d["call_id"]] = d
    answers = {}
    for d in by_event["tool_returned"]:
        call = dispatched.get(d["call_id"])
        if call is None:
            f.broken("the journal returns a call it never dispatched")
            continue
        answers[call["provider_call_id"]] = d
    by_turn_order = [c for turn in t["turns"] for c in turn["calls"]]
    for call in by_turn_order:
        answer = answers.get(call["id"])
        if answer is None:
            continue
        for image in answer.get("images", []):
            ref = image.get("artifact")
            images.append(
                {
                    "id": image["frame"],
                    "frame": image["frame"],
                    "origin": call["id"],
                    "width": image.get("width"),
                    "height": image.get("height"),
                    "parent": image.get("parent"),
                    "root": image.get("root"),
                    "mapping": image.get("mapping"),
                    "prepared": {"sha256": ref["sha256"], "bytes": ref["bytes"]} if ref else None,
                }
            )
    if t["images"] != images:
        f.broken("the images are not the journal's")

    # Calls: what ran, what it returned, and the submission that ended the run.
    executed = {c["id"] for c in by_turn_order if c["disposition"] == "executed"}
    if executed != set(answers):
        f.broken("the executed calls are not the calls the journal says returned")
    for call in by_turn_order:
        answer = answers.get(call["id"])
        if answer is None or call["disposition"] != "executed":
            continue
        meta = call["meta"]
        if (meta.get("call_id"), meta.get("envelope_file"), meta.get("status")) != (
            answer["call_id"], answer["envelope"]["sha256"], answer.get("status"),
        ):
            f.broken("a call's stored result is not the one the journal names")
            continue
        path = by_hash.get(answer["envelope"]["sha256"])
        if path is None or call["result"] is None:
            f.broken("an executed call has no stored result among the files")
            continue
        envelope = common.load_json(path)
        try:
            shown = common.loads(call["result"]["content"])
        except ValueError:
            shown = None
        if not isinstance(shown, dict) or (shown.get("call_id"), shown.get("status"), shown.get("summary")) != (
            answer["call_id"], envelope.get("status"), envelope.get("summary"),
        ):
            f.broken("a call's result is not the result of its stored envelope")
        if call["result"]["images"] != [image["frame"] for image in answer.get("images", [])]:
            f.broken("a call's result does not carry the images the journal says returned")
    finals = [c for c in by_turn_order if c["disposition"] == "final"]
    submissions = by_event["submission"]
    if terminal["kind"] == "verdict":
        if len(submissions) != 1 or len(finals) != 1 or finals[0]["id"] != submissions[0]["provider_call_id"]:
            f.broken("the final submission is not the one the journal accepted")
        else:
            arguments = finals[0]["arguments"] if isinstance(finals[0]["arguments"], dict) else {}
            if len(arguments.get("items", [])) != len(submissions[0].get("items", [])) or len(
                arguments.get("findings", [])
            ) != submissions[0].get("findings"):
                f.broken("the final submission's arguments are not what the journal accepted")
    elif submissions:
        f.broken("the journal accepted a submission and ended without a verdict")


def assistant_message(response):
    """The message the old harness echoed for a response: its content as received, or, when
    the adapter gave none, the text and calls in order."""
    content = response.get("content") or []
    if not content:
        content = [{"type": "text", "text": text} for text in response.get("text", [])]
        content += [
            {"type": "tool_use", "id": c["id"], "name": c["name"], "arguments": c.get("arguments")}
            for c in response.get("tool_calls", [])
        ]
    return {"role": "assistant", "content": content}


def check_order(t, f):
    """Turns are numbered from 1 without a gap; each request's history begins with the one
    before it; a response that was kept is the next message; one that was excluded never is."""
    turns = t["turns"]
    if [turn["turn"] for turn in turns] != list(range(1, len(turns) + 1)):
        f.broken("turns are not numbered 1 to %d in order" % len(turns))
        return
    for index, turn in enumerate(turns):
        if index + 1 < len(turns) and turn["response"] is None:
            f.broken("turn %d has no response and is not the last" % turn["turn"])
        if index == 0:
            continue
        before = turns[index - 1]
        old = before["request"]["history"]
        new = turn["request"]["history"]
        if new[: len(old)] != old:
            f.broken("turn %d: the history does not begin with the history before it" % turn["turn"])
            continue
        added = new[len(old):]
        if before["response"] is None:
            continue
        echo = assistant_message(before["response"])
        if before["excluded"] is None:
            if not added or added[0] != echo:
                f.broken("turn %d: the response before it is not the next message" % turn["turn"])
        elif echo in added:
            f.broken("turn %d: an excluded response entered the history" % turn["turn"])


def results_in(history):
    out = collections.defaultdict(list)
    for message in history:
        for block in message["content"]:
            if block["type"] == "tool_result":
                out[block["tool_use_id"]].append(block)
    return out


def check_calls(t, f):
    """Every call the judge made has one of four dispositions, and what each requires."""
    turns = t["turns"]
    answered = [turn for turn in turns if turn["response"] is not None]
    last = answered[-1]["turn"] if answered else None
    terminal = t["terminal"] or {}
    offered = [tool["name"] for tool in turns[0]["request"]["tools"]] if turns else []
    others = [name for name in offered if name not in t["evidence_tools"]]
    if others != [t["submission_tool"]]:
        f.broken("the submission tool is not the one tool offered that gathers no evidence")
    seen = set()
    final = 0
    history = {}
    for turn in turns:
        for call_id, blocks in results_in(turn["request"]["history"]).items():
            history[call_id] = blocks
    for turn in turns:
        made = [(c["id"], c["name"], c.get("arguments")) for c in (turn["response"] or {}).get("tool_calls", [])]
        kept = [(c["id"], c["name"], c["arguments"]) for c in turn["calls"]]
        if made != kept:
            f.broken("turn %d: the calls are not the calls of the response" % turn["turn"])
        for call in turn["calls"]:
            where = "turn %d" % turn["turn"]
            if call["id"] in seen:
                f.broken("%s: a call id is used twice" % where)
            seen.add(call["id"])
            disposition = call["disposition"]
            blocks = history.get(call["id"], [])
            if disposition is None:
                f.broken("%s: a call has no disposition" % where)
                continue
            if (turn["excluded"] is not None) != (disposition == "excluded"):
                f.broken("%s: excluded turn and disposition disagree" % where)
            if disposition in ("executed", "rejected"):
                if len(blocks) != 1:
                    f.broken("%s: a %s call has %d results in the history" % (where, disposition, len(blocks)))
                    continue
                block = blocks[0]
                shown = {
                    "content": block["content"],
                    "is_error": block["is_error"],
                    "images": list(block.get("images", [])),
                }
                if call["result"] != shown:
                    f.broken("%s: a call's result is not the result in the history" % where)
                if disposition == "executed":
                    if not call["meta"].get("envelope_file"):
                        f.broken("%s: an executed call has no stored result" % where)
                    if shown["is_error"]:
                        f.broken("%s: an executed call was answered as an error" % where)
                    if call["name"] not in t["evidence_tools"]:
                        f.broken("%s: an executed call names no evidence tool" % where)
                elif not shown["is_error"]:
                    f.broken("%s: a rejected call was not answered as an error" % where)
            elif call["result"] is not None or blocks:
                f.broken("%s: a call that is %s has a result" % (where, disposition))
            if disposition == "final":
                final += 1
                if turn["turn"] != last or turn["turn"] != turns[-1]["turn"]:
                    f.broken("%s: the final submission is not in the last turn" % where)
                if call["name"] != t["submission_tool"]:
                    f.broken("%s: the final submission is not a call of the submission tool" % where)
                ends = terminal.get("kind") == "verdict" or (
                    terminal.get("kind") == "error" and terminal.get("class") == "submission_invalid"
                )
                if not ends:
                    f.broken("%s: a final submission in a run that no submission ended" % where)
    for call_id in history:
        if call_id not in seen:
            f.broken("a result in the history answers no call")
    if terminal.get("kind") == "verdict" and final != 1:
        f.broken("a verdict needs exactly one final submission, found %d" % final)
    if final > 1:
        f.broken("more than one final submission")


def check_images(t, f):
    """Every image a history names is listed once, with the bytes that were sent."""
    listed = [image["id"] for image in t["images"]]
    if len(set(listed)) != len(listed):
        f.broken("an image id is listed twice")
    stored = {entry["sha256"] for entry in t["files"]}
    for image in t["images"]:
        if image["prepared"] is None:
            f.broken("an image has no prepared bytes recorded")
        elif image["prepared"]["sha256"] not in stored:
            f.broken("an image's prepared bytes are not among the files")
    names = set()
    for turn in t["turns"]:
        for message in turn["request"]["history"]:
            for block in message["content"]:
                if block["type"] == "image":
                    names.add(block["image_id"])
                if block["type"] == "tool_result":
                    names.update(block.get("images", []))
    if not names <= set(listed):
        f.broken("a history names an image that is not listed")


def audit_one(t, package, fixture):
    f = Findings()
    errors = common.schema_errors(t)
    if errors:
        f.broken("not a version %d transcript: %s" % (common.TRANSCRIPT_VERSION, "; ".join(errors[:5])))
        return f
    # The transcript must agree with itself about being whole.
    marked = any(e["truncated"] or e["sha256"] is None for e in t["files"])
    missing = marked or bool(t["incomplete"]) or t["terminal"] is None
    missing = missing or None in t["subject"].values() or None in t["preparation"].values()
    missing = missing or t["submission_tool"] is None
    missing = missing or any(turn["request"] is None for turn in t["turns"])
    missing = missing or any(turn["response"] is None and turn["meta"].get("answered") for turn in t["turns"])
    if t["complete"] == missing:
        f.broken("marked complete, and the record is not whole" if t["complete"] else "marked incomplete, and nothing is missing")
    by_hash = check_files(t, package, f)
    check_embedded(t, by_hash, f)
    if f.incomplete or not t["complete"]:
        # The rules below cannot be judged on a record that is not whole.
        if not f.incomplete and not f.invalid:
            f.missing("marked incomplete by the converter")
        return f
    check_journal(t, package, fixture, by_hash, f)
    check_order(t, f)
    check_calls(t, f)
    check_images(t, f)
    return f


def toml_table(name, counter):
    lines = ["", "[%s]" % name]
    for key in sorted(counter):
        lines.append("%s = %d" % (key if key.replace("_", "").isalnum() else json.dumps(key), counter[key]))
    return lines


def refuse(text):
    print("audit: %s" % text, file=sys.stderr)
    return 1


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--package", required=True)
    p.add_argument("--transcripts", required=True)
    p.add_argument("--origin", choices=common.ORIGINS, default="historical")
    p.add_argument("--fixtures", default="replay", help="folder of fixtures inside the package")
    p.add_argument("--manifest", help="the package's public manifest, to check the count against")
    p.add_argument("--summary", help="write the public, counts-only record here")
    args = p.parse_args(argv)
    package = os.path.abspath(args.package)
    top = os.path.abspath(args.transcripts)
    if args.summary and not args.manifest:
        return refuse("--summary needs --manifest: the record is only written for a counted package")

    # Coverage first: the transcripts are the ones the converter listed, and there is exactly
    # one for every fixture of the package.
    for name in (common.LISTING, common.DIGEST):
        if not os.path.isfile(os.path.join(top, name)):
            return refuse("the transcripts folder has no %s" % name)
    data, _count = common.listing(top, leave_out=(common.LISTING, common.DIGEST, common.FINDINGS))
    transcripts_digest = common.digest_of_listing(data)
    with open(os.path.join(top, common.LISTING), "rb") as fh:
        listed = fh.read()
    with open(os.path.join(top, common.DIGEST)) as fh:
        stated = "sha256:" + fh.read().strip()
    if listed != data or stated != transcripts_digest:
        return refuse("the transcripts are not the ones the converter listed")

    package_digest = common.package_digest(package)
    if package_digest is None:
        return refuse("no PACKAGE.digest under the package root")
    try:
        found = common.fixtures(package, args.fixtures)
    except common.FixtureError as e:
        return refuse(str(e))
    root = os.path.join(top, args.origin)
    have = set()
    for folder, _dirs, files in os.walk(root):
        for name in files:
            have.add(os.path.relpath(os.path.join(folder, name), root).replace(os.sep, "/"))
    want = {name + ".json" for name, _d, _r in found}
    if not want:
        print("audit: no fixtures found", file=sys.stderr)
        return 2
    if have != want:
        return refuse(
            "fixtures and transcripts do not match one to one: %d without a transcript, %d transcripts without a fixture"
            % (len(want - have), len(have - want))
        )
    if args.manifest:
        digest, arms = common.manifest_counts(args.manifest)
        if digest != package_digest:
            return refuse("the manifest names another package")

    state = collections.Counter()
    origins = collections.Counter()
    dispositions = collections.Counter()
    terminals = collections.Counter()
    stops = collections.Counter()
    excluded = collections.Counter()
    repairs = collections.Counter()
    totals = collections.Counter()
    by_arm = collections.Counter()
    findings = []
    for fixture in found:
        rel = fixture[0] + ".json"
        t = common.load_json(os.path.join(root, *rel.split("/")))
        f = audit_one(t, package, fixture)
        if isinstance(t, dict) and isinstance(t.get("meta"), dict):
            if t["meta"].get("package_digest") != package_digest or t["meta"].get("name") != fixture[0]:
                f.broken("converted from another package or another fixture")
            if t.get("origin") != args.origin:
                f.broken("not of the origin its folder names")
        if f.invalid:
            outcome = "invalid"
        elif f.incomplete:
            outcome = "incomplete"
        else:
            outcome = "complete"
        state[outcome] += 1
        if f.invalid or f.incomplete:
            findings.append({"transcript": rel, "outcome": outcome, "incomplete": f.incomplete, "invalid": f.invalid})
        if outcome == "invalid":
            continue
        origins[t["origin"]] += 1
        by_arm[t["subject"]["arm"]] += 1
        totals["turns"] += len(t["turns"])
        for turn in t["turns"]:
            if turn["response"]:
                stop = turn["response"]["stop_reason"]
                stops[stop if isinstance(stop, str) else "other"] += 1
            if turn["excluded"]:
                excluded[turn["excluded"]["reason"]] += 1
            for call in turn["calls"]:
                dispositions[call["disposition"] or "none"] += 1
                totals["calls"] += 1
        for repair in t["repairs"]:
            repairs["%s_%s" % (repair["form"], repair["reason"])] += 1
        terminal = t["terminal"] or {}
        kind = terminal.get("kind", "none")
        terminals[kind if kind == "verdict" else "%s_%s" % (kind, terminal.get("class"))] += 1

    common.write_private(
        os.path.join(top, common.FINDINGS),
        "".join(json.dumps(item, sort_keys=True) + "\n" for item in findings).encode("utf-8"),
    )

    print("audit: %d fixtures, %d transcripts: %d complete, %d incomplete, %d invalid" % (len(found), len(have), state["complete"], state["incomplete"], state["invalid"]))
    print("audit: calls by disposition: %s" % ", ".join("%s %d" % kv for kv in sorted(dispositions.items())))
    print("audit: terminal outcomes: %s" % ", ".join("%s %d" % kv for kv in sorted(terminals.items())))

    if args.manifest and not state["invalid"]:
        counted = {arm: arms.get(arm) for arm in by_arm}
        if args.origin == "historical" and dict(by_arm) != counted:
            return refuse("the manifest counts other decisions for the arm than the package holds fixtures")

    if args.summary and not state["invalid"]:
        lines = [
            "# The audit of the baseline transcripts: counts only. Written by scripts/baseline/audit.py;",
            "# see transcript.md beside this file. It holds no item names, file names or verdicts.",
            "",
            "transcript_version = %d" % common.TRANSCRIPT_VERSION,
            'tools_version = "%s"' % common.TOOLS_VERSION,
            'package_digest = "%s"' % package_digest,
            'transcripts_digest = "%s"' % transcripts_digest,
            "",
            "fixtures = %d" % len(found),
            "decisions = %d" % len(have),
            "complete = %d" % state["complete"],
            "incomplete = %d" % state["incomplete"],
            "invalid = %d" % state["invalid"],
            "turns = %d" % totals["turns"],
            "calls = %d" % totals["calls"],
        ]
        lines += toml_table("origin", origins)
        lines += toml_table("calls_by_disposition", dispositions)
        lines += toml_table("terminal_outcomes", terminals)
        lines += toml_table("responses_by_stop_reason", stops)
        lines += toml_table("excluded_turns", excluded)
        lines += toml_table("repairs", repairs)
        with open(args.summary, "w") as out:
            out.write("\n".join(lines) + "\n")
    if state["invalid"]:
        return 1
    return 3 if state["incomplete"] else 0


if __name__ == "__main__":
    sys.exit(main())
