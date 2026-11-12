#!/usr/bin/env python3
"""Audits transcripts against the package they were converted from
(docs/baseline/transcript.md, "The audit").

    audit.py --package <package root> --transcripts <folder> [--summary <file.toml>]

Each transcript ends as one of three things:

  complete     the record is whole and every rule holds
  incomplete   the record is not whole (a stored file is missing, cut short by the recorder,
               or not what was recorded); it is listed, to be captured again
  invalid      the record is whole but breaks a rule; this is a defect to be understood

Exit status: 0 when every transcript is complete, 3 when some are incomplete and none is
invalid, 1 when any is invalid, 2 when there is nothing to audit.

Prints counts only. What was found, per transcript, goes to audit-findings.jsonl in the
transcripts folder, which is private.

Python 3.9 or later, standard library only.
"""

import argparse
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common  # noqa: E402


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
    for entry in t.get("files", []):
        role = entry.get("role")
        if entry.get("truncated"):
            f.missing("cut short by the recorder: %s" % role)
            continue
        if entry.get("sha256") is None:
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


def check_embedded(t, by_hash, f):
    """What the transcript holds for a request and a response is what the stored file holds."""
    for turn in t["turns"]:
        meta = turn.get("meta", {})
        path = by_hash.get(meta.get("request_file"))
        if path is None:
            f.missing("turn %d has no whole stored request" % turn["turn"])
        elif turn["request"] != common.load_json(path):
            f.broken("turn %d: the request is not the stored request" % turn["turn"])
        if not meta.get("answered"):
            continue
        path = by_hash.get(meta.get("response_file"))
        if path is None:
            f.missing("turn %d has no whole stored response" % turn["turn"])
            continue
        whole = common.load_json(path)
        kept = {
            "content": whole.get("content", []),
            "text": whole.get("text", []),
            "tool_calls": whole.get("tool_calls", []),
            "stop_reason": whole.get("stop_reason"),
        }
        if turn["response"] != kept:
            f.broken("turn %d: the response is not the stored response" % turn["turn"])


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
        if turn["request"] is None:
            continue
        if index + 1 < len(turns) and turn["response"] is None:
            f.broken("turn %d has no response and is not the last" % turn["turn"])
        if index == 0:
            continue
        before = turns[index - 1]
        if before["request"] is None:
            continue
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
        for block in message.get("content", []):
            if block.get("type") == "tool_result":
                out[block["tool_use_id"]].append(block)
    return out


def check_calls(t, f):
    """Every call the judge made has one of four dispositions, and what each requires."""
    turns = t["turns"]
    answered = [turn for turn in turns if turn["response"] is not None]
    last = answered[-1]["turn"] if answered else None
    terminal = t.get("terminal") or {}
    seen = set()
    final = 0
    history = {}
    for turn in turns:
        if turn["request"] is not None:
            for call_id, blocks in results_in(turn["request"]["history"]).items():
                history[call_id] = blocks
    known = set()
    for turn in answered:
        made = [(c["id"], c["name"], c.get("arguments")) for c in turn["response"]["tool_calls"]]
        kept = [(c["id"], c["name"], c.get("arguments")) for c in turn["calls"]]
        if made != kept:
            f.broken("turn %d: the calls are not the calls of the response" % turn["turn"])
        for call in turn["calls"]:
            where = "turn %d" % turn["turn"]
            if call["id"] in seen:
                f.broken("%s: a call id is used twice" % where)
            seen.add(call["id"])
            known.add(call["id"])
            disposition = call.get("disposition")
            blocks = history.get(call["id"], [])
            if disposition not in common.DISPOSITIONS:
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
                    "content": block.get("content"),
                    "is_error": bool(block.get("is_error")),
                    "images": list(block.get("images", [])),
                }
                if call.get("result") != shown:
                    f.broken("%s: a call's result is not the result in the history" % where)
                if disposition == "executed":
                    if not call.get("meta", {}).get("envelope_file"):
                        f.broken("%s: an executed call has no stored result" % where)
                    if shown["is_error"]:
                        f.broken("%s: an executed call was answered as an error" % where)
                    if call["name"] not in t.get("evidence_tools", []):
                        f.broken("%s: an executed call names no evidence tool" % where)
                elif not shown["is_error"]:
                    f.broken("%s: a rejected call was not answered as an error" % where)
            else:
                if call.get("result") is not None or blocks:
                    f.broken("%s: a call that is %s has a result" % (where, disposition))
            if disposition == "final":
                final += 1
                if turn["turn"] != last or turn["turn"] != turns[-1]["turn"]:
                    f.broken("%s: the final submission is not in the last turn" % where)
                if call["name"] in t.get("evidence_tools", []):
                    f.broken("%s: the final submission names an evidence tool" % where)
    for call_id in history:
        if call_id not in known:
            f.broken("a result in the history answers no call")
    kind = terminal.get("kind")
    if kind == "verdict" and final != 1:
        f.broken("a verdict needs exactly one final submission, found %d" % final)
    if final > 1:
        f.broken("more than one final submission")


def check_images(t, f):
    """Every image a history names is listed with the bytes that were sent."""
    listed = {}
    for image in t.get("images", []):
        listed[image["id"]] = image
        if image.get("prepared") is None:
            f.missing("image has no prepared bytes recorded")
    stored = {entry.get("sha256") for entry in t.get("files", [])}
    for image in listed.values():
        if image.get("prepared") and image["prepared"]["sha256"] not in stored:
            f.broken("an image's prepared bytes are not among the files")
    names = set()
    for turn in t["turns"]:
        if turn["request"] is None:
            continue
        for message in turn["request"]["history"]:
            for block in message.get("content", []):
                if block.get("type") == "image":
                    names.add(block["image_id"])
                if block.get("type") == "tool_result":
                    names.update(block.get("images", []))
    if len(listed) != len(t.get("images", [])):
        f.broken("an image id is listed twice")
    if not names <= set(listed):
        f.broken("a history names an image that is not listed")
    prep = t.get("preparation") or {}
    if not prep.get("version"):
        f.broken("no preparation version")
    if not prep.get("settings"):
        f.broken("no preparation settings")


def check_terminal(t, package, f):
    """The terminal outcome is the one the old result recorded."""
    terminal = t.get("terminal")
    if not terminal or terminal.get("kind") not in common.TERMINAL_KINDS:
        f.missing("no terminal outcome")
        return
    entry = next((e for e in t.get("files", []) if e.get("role") == "result"), None)
    if entry is None or entry.get("sha256") is None:
        f.missing("no old result to compare with")
        return
    path = os.path.join(package, *entry["path"].split("/"))
    if not os.path.isfile(path):
        return  # already reported by check_files
    old = common.load_json(path)
    if terminal["kind"] == "verdict":
        if old.get("verdict") != terminal.get("verdict") or old.get("error_class"):
            f.broken("the verdict is not the old result's")
    else:
        if old.get("error_class") != terminal.get("class"):
            f.broken("the error is not the old result's")


def audit_one(t, package):
    f = Findings()
    if t.get("transcript_version") != common.TRANSCRIPT_VERSION:
        f.broken("unknown transcript version")
        return f
    if t.get("volatile") != common.VOLATILE:
        f.broken("the volatile declaration is not the format's")
    # The transcript must agree with itself about being whole.
    marked = any(e.get("truncated") or e.get("sha256") is None for e in t.get("files", []))
    if bool(t.get("complete")) == (marked or bool(t.get("incomplete"))):
        f.broken("marked complete, and the record is not whole" if t.get("complete") else "marked incomplete, and nothing is missing")
    by_hash = check_files(t, package, f)
    check_embedded(t, by_hash, f)
    if f.incomplete:
        # The rules below cannot be judged on a record that is not whole.
        return f
    check_order(t, f)
    check_calls(t, f)
    check_images(t, f)
    check_terminal(t, package, f)
    return f


def toml_table(name, counter):
    lines = ["", "[%s]" % name]
    for key in sorted(counter):
        lines.append('%s = %d' % (key if key.replace("_", "").isalnum() else json.dumps(key), counter[key]))
    return lines


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--package", required=True)
    p.add_argument("--transcripts", required=True)
    p.add_argument("--summary", help="write the public, counts-only record here")
    args = p.parse_args(argv)
    package = os.path.abspath(args.package)
    top = os.path.abspath(args.transcripts)

    paths = []
    for folder, _dirs, files in os.walk(top):
        for name in files:
            if name.endswith(".json"):
                paths.append(os.path.join(folder, name))
    paths.sort()
    if not paths:
        print("audit: no transcripts found", file=sys.stderr)
        return 2

    state = collections.Counter()
    origins = collections.Counter()
    dispositions = collections.Counter()
    terminals = collections.Counter()
    stops = collections.Counter()
    excluded = collections.Counter()
    repairs = collections.Counter()
    totals = collections.Counter()
    digests = set()
    findings = []
    for path in paths:
        t = common.load_json(path)
        f = audit_one(t, package)
        rel = os.path.relpath(path, top).replace(os.sep, "/")
        if f.invalid:
            outcome = "invalid"
        elif f.incomplete:
            outcome = "incomplete"
        else:
            outcome = "complete"
        state[outcome] += 1
        origins[t.get("origin", "unknown")] += 1
        digests.add(t.get("meta", {}).get("package_digest"))
        if f.invalid or f.incomplete:
            findings.append({"transcript": rel, "outcome": outcome, "incomplete": f.incomplete, "invalid": f.invalid})
        totals["turns"] += len(t.get("turns", []))
        for turn in t.get("turns", []):
            if turn.get("response"):
                stop = turn["response"].get("stop_reason")
                stops[stop if isinstance(stop, str) else "other"] += 1
            if turn.get("excluded"):
                excluded[turn["excluded"]["reason"]] += 1
            for call in turn.get("calls", []):
                dispositions[call.get("disposition") or "none"] += 1
                totals["calls"] += 1
        for repair in t.get("repairs", []):
            repairs["%s_%s" % (repair["form"], repair["reason"])] += 1
        terminal = t.get("terminal") or {}
        kind = terminal.get("kind", "none")
        terminals[kind if kind == "verdict" else "%s_%s" % (kind, terminal.get("class"))] += 1

    fd = os.open(os.path.join(top, "audit-findings.jsonl"), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as out:
        for item in findings:
            out.write(json.dumps(item, sort_keys=True) + "\n")

    listing_path = os.path.join(top, "TRANSCRIPTS.digest")
    transcripts_digest = None
    if os.path.isfile(listing_path):
        data, _count = common.listing(top, leave_out=("TRANSCRIPTS.sha256", "TRANSCRIPTS.digest", "audit-findings.jsonl"))
        transcripts_digest = common.digest_of_listing(data)
        with open(listing_path) as fh:
            if "sha256:" + fh.read().strip() != transcripts_digest:
                print("audit: the transcripts are not the ones the converter listed", file=sys.stderr)
                return 1

    print("audit: %d transcripts: %d complete, %d incomplete, %d invalid" % (len(paths), state["complete"], state["incomplete"], state["invalid"]))
    print("audit: calls by disposition: %s" % ", ".join("%s %d" % kv for kv in sorted(dispositions.items())))
    print("audit: terminal outcomes: %s" % ", ".join("%s %d" % kv for kv in sorted(terminals.items())))

    if args.summary:
        if len(digests) != 1 or None in digests:
            print("audit: the transcripts do not name one package", file=sys.stderr)
            return 1
        lines = [
            "# The audit of the baseline transcripts: counts only. Written by scripts/baseline/audit.py;",
            "# see transcript.md beside this file. It holds no item names, file names or verdicts.",
            "",
            "transcript_version = %d" % common.TRANSCRIPT_VERSION,
            'tools_version = "%s"' % common.TOOLS_VERSION,
            'package_digest = "%s"' % digests.pop(),
            'transcripts_digest = "%s"' % (transcripts_digest or ""),
            "",
            "decisions = %d" % len(paths),
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
