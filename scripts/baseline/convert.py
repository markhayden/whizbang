#!/usr/bin/env python3
"""Turns each replay fixture of a baseline package into a transcript
(docs/baseline/transcript.md).

    convert.py --package <package root> --out <folder> [--origin historical|recaptured]

Reads the package and writes one transcript per decision under <out>/<origin>/, then a
listing and its digest over everything it wrote. Nothing is added to a record: what a
fixture does not hold is marked missing and the transcript is marked incomplete. Prints
counts only.

Python 3.9 or later, standard library only.
"""

import argparse
import collections
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common  # noqa: E402


def is_ref(value):
    return (
        isinstance(value, dict)
        and isinstance(value.get("sha256"), str)
        and "bytes" in value
        and "truncated" in value
    )


class Fixture:
    """One decision's attempt folder inside the package."""

    def __init__(self, package, decision_dir, run_dir):
        self.package = package
        self.run_dir = run_dir
        self.decision_dir = decision_dir
        self.repetition_dir = os.path.dirname(decision_dir)
        self.journal_path = os.path.join(run_dir, "journal.jsonl")
        self.files = []
        self.problems = []

    def rel(self, path):
        return os.path.relpath(path, self.package).replace(os.sep, "/")

    def plain_file(self, role, path):
        """A file the journal does not name by hash: hashed as it is now."""
        entry = {"role": role, "path": self.rel(path), "truncated": False}
        if os.path.isfile(path):
            entry["sha256"] = common.sha256_file(path)
            entry["bytes"] = os.path.getsize(path)
        else:
            entry["sha256"] = None
            entry["bytes"] = None
            self.problems.append("missing file: %s" % role)
        self.files.append(entry)
        return entry

    def stored(self, role, ref):
        """A file the journal names by hash. Returns its bytes, or None when it is missing,
        cut short by the recorder, or not what the journal recorded."""
        path = os.path.join(self.run_dir, "artifacts", ref["sha256"])
        entry = {
            "role": role,
            "path": self.rel(path),
            "sha256": ref["sha256"],
            "bytes": ref["bytes"],
            "truncated": bool(ref["truncated"]),
        }
        self.files.append(entry)
        if ref["truncated"]:
            self.problems.append("stored file cut short by the recorder: %s" % role)
            return None
        if not os.path.isfile(path):
            self.problems.append("missing stored file: %s" % role)
            return None
        with open(path, "rb") as f:
            data = f.read()
        if len(data) != ref["bytes"] or common.sha256_bytes(data) != ref["sha256"]:
            self.problems.append("stored file does not match its recorded hash: %s" % role)
            return None
        return data

    def stored_json(self, role, ref):
        data = self.stored(role, ref)
        if data is None:
            return None
        try:
            return common.loads(data.decode("utf-8"))
        except ValueError:
            self.problems.append("stored file is not whole JSON: %s" % role)
            return None


def tool_results(history):
    """Every tool result in a history, by the id of the call it answers."""
    out = {}
    for message in history:
        for block in message.get("content", []):
            if block.get("type") == "tool_result":
                out[block["tool_use_id"]] = {
                    "content": block.get("content"),
                    "is_error": bool(block.get("is_error")),
                    "images": list(block.get("images", [])),
                }
    return out


def convert(package, subdir, decision_dir, run_dir, origin, package_digest, preparation):
    fx = Fixture(package, decision_dir, run_dir)
    events = common.load_lines(fx.journal_path)
    fx.plain_file("journal", fx.journal_path)
    result_path = os.path.join(fx.repetition_dir, "result.json")
    fx.plain_file("result", result_path)
    decision_path = os.path.join(fx.decision_dir, "decision.json")
    fx.plain_file("decision", decision_path)
    fx.plain_file("report", os.path.join(fx.decision_dir, "report.json"))
    result = common.load_json(result_path) if os.path.isfile(result_path) else {}
    decision = common.load_json(decision_path) if os.path.isfile(decision_path) else {}

    by_event = collections.defaultdict(list)
    for e in events:
        by_event[e["event"]].append(e["data"])

    run_name = os.path.relpath(fx.decision_dir, os.path.join(package, subdir)).split(os.sep)[0]
    model = (decision.get("config", {}).get("provider", {}) or {}).get("model")
    evidence_tools = []
    images = []
    for data in by_event["resolved"]:
        evidence_tools = list(data.get("tools", []))
        sent = data.get("images_sent", {})
        for image in data.get("images", []):
            frame = "raw:%s" % image.get("id")
            ref = sent.get(frame)
            entry = {
                "id": image.get("id"),
                "frame": frame,
                "origin": "job",
                "role": image.get("role"),
                "width": image.get("width"),
                "height": image.get("height"),
                "source": {
                    "sha256": image.get("sha256"),
                    "pixel_sha256": image.get("pixel_sha256"),
                },
                "prepared": None,
            }
            if ref is not None:
                fx.stored("image %s" % frame, ref)
                entry["prepared"] = {"sha256": ref["sha256"], "bytes": ref["bytes"]}
            else:
                fx.problems.append("job image has no prepared bytes recorded")
            images.append(entry)

    prep_version = None
    for data in by_event["prepared"]:
        prep_version = data.get("prep_version")
    settings = None
    if preparation is not None and model is not None:
        settings = preparation.get("settings", {}).get(model.split(":")[-1])

    # Calls the old harness dispatched or refused, by the provider's call id.
    called = collections.defaultdict(list)
    for data in by_event["tool_called"]:
        called[data["provider_call_id"]].append(data)
    returned = {data["call_id"]: data for data in by_event["tool_returned"]}
    excluded = {data["turn"]: data for data in by_event["turn_excluded"]}
    received = {data["turn"]: data for data in by_event["turn_received"]}
    submission_ids = [data["provider_call_id"] for data in by_event["submission"]]
    refusals = {
        data["after_turn"]: data["reason"] for data in by_event["repair"] if data["form"] == "rejected"
    }

    turns = []
    requests = {}
    for data in by_event["turn_sent"]:
        number = data["turn"]
        request = fx.stored_json("request %d" % number, data["request"])
        requests[number] = request
        turn = {
            "turn": number,
            "request": request,
            "response": None,
            "excluded": None,
            "calls": [],
            "meta": {
                "bound_usd": data.get("bound_usd"),
                "request_file": data["request"]["sha256"],
            },
        }
        got = received.get(number)
        if got is None:
            turn["meta"]["answered"] = False
        else:
            whole = fx.stored_json("response %d" % number, got["response"])
            turn["meta"].update(
                {
                    "answered": True,
                    "usage": got.get("usage"),
                    "estimated": got.get("estimated", False),
                    "provider_request_id": got.get("provider_request_id"),
                    "attempts": got.get("attempts"),
                    "cost_usd": got.get("cost_usd"),
                    "response_file": got["response"]["sha256"],
                }
            )
            if whole is not None:
                turn["response"] = {
                    "content": whole.get("content", []),
                    "text": whole.get("text", []),
                    "tool_calls": whole.get("tool_calls", []),
                    "stop_reason": whole.get("stop_reason"),
                }
        if number in excluded:
            turn["excluded"] = {"reason": excluded[number]["reason"]}
        turns.append(turn)
    turns.sort(key=lambda t: t["turn"])

    # A result is what a later request's history holds for the call.
    results = {}
    for number in sorted(requests):
        if requests[number] is not None:
            results.update(tool_results(requests[number].get("history", [])))

    offered = []
    for number in sorted(requests):
        if requests[number] is not None:
            offered = [tool["name"] for tool in requests[number].get("tools", [])]
            break
    others = [name for name in offered if name not in evidence_tools]
    submission_tool = others[0] if len(others) == 1 else None
    if submission_tool is None:
        fx.problems.append("the tools offered do not name one submission tool")

    terminal = None
    for data in by_event["verdict"]:
        terminal = {"kind": "verdict", "verdict": data["verdict"]}
    for kind in ("error", "cancelled"):
        for data in by_event[kind]:
            terminal = {"kind": kind, "class": data.get("class"), "message": data.get("message")}
    if terminal is None:
        fx.problems.append("the journal has no terminal event")

    last_answered = max([t["turn"] for t in turns if t["response"] is not None], default=None)
    for turn in turns:
        if turn["response"] is None:
            continue
        for call in turn["response"]["tool_calls"]:
            entry = {
                "id": call["id"],
                "name": call["name"],
                "arguments": call.get("arguments"),
                "disposition": None,
                "result": None,
                "meta": {},
            }
            lines = called.get(call["id"], [])
            call_ids = [line["call_id"] for line in lines if line.get("call_id")]
            answer = returned.get(call_ids[0]) if call_ids else None
            if turn["excluded"] is not None:
                entry["disposition"] = "excluded"
            elif call["id"] in results:
                entry["result"] = results[call["id"]]
                if answer is not None:
                    entry["disposition"] = "executed"
                    envelope = answer["envelope"]
                    fx.stored("envelope %s" % call_ids[0], envelope)
                    entry["meta"] = {
                        "call_id": call_ids[0],
                        "status": answer.get("status"),
                        "duration_ms": answer.get("duration_ms"),
                        "envelope_file": envelope["sha256"],
                    }
                    for image in answer.get("images", []):
                        ref = image.get("artifact")
                        record = {
                            "id": image["frame"],
                            "frame": image["frame"],
                            "origin": call["id"],
                            "width": image.get("width"),
                            "height": image.get("height"),
                            "parent": image.get("parent"),
                            "root": image.get("root"),
                            "mapping": image.get("mapping"),
                            "prepared": None,
                        }
                        if ref is not None:
                            fx.stored("image %s" % image["frame"], ref)
                            record["prepared"] = {"sha256": ref["sha256"], "bytes": ref["bytes"]}
                        images.append(record)
                else:
                    entry["disposition"] = "rejected"
                    reasons = [
                        line.get("reason") or ("invalid_args" if line.get("invalid_args") else None)
                        for line in lines
                    ]
                    reason = next((r for r in reasons if r), None)
                    entry["meta"] = {"reason": reason or refusals.get(turn["turn"])}
            elif (
                turn["turn"] == last_answered
                and call["name"] == submission_tool
                and terminal is not None
                and (
                    (terminal["kind"] == "verdict" and call["id"] in submission_ids)
                    or (terminal["kind"] == "error" and terminal["class"] == "submission_invalid")
                )
            ):
                # The submission that ended the run: with a verdict, or, when it did not
                # validate and no repair was left, with that error.
                entry["disposition"] = "final"
            else:
                fx.problems.append("a call has neither a result nor an end of the run")
            turn["calls"].append(entry)

    arm = str(result.get("cell") or "").rsplit("/", 1)[-1] or None
    if prep_version is None or settings is None:
        fx.problems.append("the preparation's version or settings are not known")
    if None in (result.get("set"), result.get("item"), model, arm, result.get("repetition"), decision.get("use_case")):
        fx.problems.append("the subject is not fully known")

    transcript = {
        "transcript_version": common.TRANSCRIPT_VERSION,
        "origin": origin,
        "complete": not fx.problems,
        "incomplete": sorted(set(fx.problems)),
        "subject": {
            "set": result.get("set"),
            "item": result.get("item"),
            "model": model,
            "arm": arm,
            "repetition": result.get("repetition"),
            "use_case": decision.get("use_case"),
        },
        "preparation": {"version": prep_version, "settings": settings},
        "evidence_tools": evidence_tools,
        "submission_tool": submission_tool,
        "images": images,
        "turns": turns,
        "repairs": [
            {"after_turn": d["after_turn"], "form": d["form"], "reason": d["reason"]}
            for d in by_event["repair"]
        ],
        "terminal": terminal,
        "volatile": list(common.VOLATILE),
        "files": fx.files,
        "meta": {
            "fixtures": subdir,
            "tools_version": common.TOOLS_VERSION,
            "package_digest": package_digest,
            "harness": "squint",
            "harness_version": decision.get("harness_version"),
            "run": run_name,
            "decision": fx.rel(fx.decision_dir),
            "name": os.path.relpath(fx.decision_dir, os.path.join(package, subdir)).replace(os.sep, "/"),
            "attempt": os.path.basename(fx.run_dir),
            "prompt_version": decision.get("prompt_version"),
        },
    }
    return transcript


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--package", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--origin", choices=("historical", "recaptured"), default="historical")
    p.add_argument("--fixtures", default="replay", help="folder of fixtures inside the package")
    args = p.parse_args(argv)

    package = os.path.abspath(args.package)
    package_digest = common.package_digest(package)
    if package_digest is None:
        print("convert: no PACKAGE.digest under the package root", file=sys.stderr)
        return 2
    prep_path = os.path.join(package, "assembly", "generated", "preparation.json")
    preparation = common.load_json(prep_path) if os.path.isfile(prep_path) else None

    try:
        found = common.fixtures(package, args.fixtures)
    except common.FixtureError as e:
        print("convert: %s" % e, file=sys.stderr)
        return 2
    if not found:
        print("convert: no fixtures found", file=sys.stderr)
        return 2
    top = os.path.abspath(args.out)
    out_root = os.path.join(top, args.origin)
    if os.path.exists(out_root) and os.listdir(out_root):
        print("convert: the output folder already holds transcripts of this origin", file=sys.stderr)
        return 2
    complete = 0
    for name, decision_dir, run_dir in found:
        t = convert(package, args.fixtures, decision_dir, run_dir, args.origin, package_digest, preparation)
        target = os.path.join(out_root, *name.split("/")) + ".json"
        os.makedirs(os.path.dirname(target), mode=0o700, exist_ok=True)
        common.write_private(target, common.canonical(t))
        complete += 1 if t["complete"] else 0

    data, count = common.listing(top, leave_out=(common.LISTING, common.DIGEST, common.FINDINGS))
    common.write_private(os.path.join(top, common.LISTING), data)
    common.write_private(
        os.path.join(top, common.DIGEST), (common.digest_of_listing(data)[len("sha256:"):] + "\n").encode()
    )
    print("convert: %d transcripts written (%d complete, %d incomplete)" % (len(found), complete, len(found) - complete))
    print("convert: %d files listed, digest %s" % (count, common.digest_of_listing(data)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
