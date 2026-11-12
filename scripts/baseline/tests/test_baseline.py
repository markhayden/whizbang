"""Tests of convert.py and audit.py on a synthetic package built in a temporary folder.
No network, no real fixture. Run by scripts/tests/test-baseline.sh."""

import contextlib
import copy
import hashlib
import io
import json
import os
import shutil
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
import audit  # noqa: E402
import common  # noqa: E402
import convert  # noqa: E402

ITEM = "synthetic-item-7"
SYSTEM = "You compare a candidate image with its source."
TOOLS = [
    {"name": "view", "description": "Look closer.", "input_schema": {"type": "object"}},
    {"name": "submit_verdict", "description": "Answer.", "input_schema": {"type": "object"}},
]


SUBMIT = {"schema_version": 1, "items": [{"id": "i1", "state": "met"}], "findings": [], "opinion": "pass"}


def text(role, value):
    return {"role": role, "content": [{"type": "text", "text": value}]}


def opaque(call_id, name, arguments):
    return {"type": "opaque", "adapter": "synthetic", "block": {"call": call_id, "sig": "s-" + call_id}}


class Package:
    """Builds one decision's fixture the way the old recorder laid it out."""

    def __init__(self, root):
        self.root = root
        self.decision = os.path.join(
            root, "replay", "run-a", "set-1", ITEM, "provider_model-x", "C", "rep-1", "decision-1-1"
        )
        self.run = os.path.join(self.decision, "run-1")
        os.makedirs(os.path.join(self.run, "artifacts"))
        self.events = []
        self.history = []
        self.turn = 0
        self.calls = 0
        first = [{"type": "text", "text": "Compare these."}]
        sent = {}
        images = []
        for role in ("source", "candidate"):
            ref = self.put(("prepared-" + role).encode())
            sent["raw:" + role] = ref
            images.append({"id": role, "role": role, "width": 8, "height": 8, "sha256": "f" * 64, "pixel_sha256": "e" * 64})
            first.append({"type": "image", "image_id": role})
        self.history.append({"role": "user", "content": first})
        self.event("resolved", {"images": images, "images_sent": sent, "tools": ["view"]})
        self.event("prepared", {"prep_version": "1", "images": 2})

    def put(self, data, truncated=False):
        sha = hashlib.sha256(data).hexdigest()
        with open(os.path.join(self.run, "artifacts", sha), "wb") as f:
            f.write(data)
        return {"sha256": sha, "bytes": len(data), "truncated": truncated, "full_sha256": "a" * 64 if truncated else None}

    def event(self, name, data):
        self.events.append({"seq": len(self.events), "unix_ms": 1, "event": name, "data": data})

    def exchange(self, calls, stop="tool_use", texts=(), truncate_request=False, truncate_response=False):
        """One request and its response; returns the turn number."""
        self.turn += 1
        request = {"system": SYSTEM, "history": copy.deepcopy(self.history), "tools": TOOLS, "max_output_tokens": 512}
        body = json.dumps(request).encode()
        ref = self.put(body[:20] + b"\n[truncated: 9 bytes in full, sha256 x]\n" if truncate_request else body, truncate_request)
        self.event("turn_sent", {"turn": self.turn, "tools": ["view", "submit_verdict"], "history_messages": len(self.history), "max_output_tokens": 512, "bound_usd": 0.1, "request": ref})
        response = {
            "text": list(texts),
            "tool_calls": [{"id": c[0], "name": c[1], "arguments": c[2]} for c in calls],
            "content": [opaque(*c) for c in calls] + [{"type": "text", "text": t} for t in texts],
            "stop_reason": stop,
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "provider_request_id": "req-%d" % self.turn,
        }
        body = json.dumps(response).encode()
        ref = self.put(body[:20] if truncate_response else body, truncate_response)
        self.event("turn_received", {"turn": self.turn, "stop_reason": stop, "tool_calls": [c[1] for c in calls], "usage": response["usage"], "estimated": False, "provider_request_id": response["provider_request_id"], "attempts": 1, "cost_usd": 0.01, "response": ref})
        self.last_response = response
        return self.turn

    def keep(self):
        """The last response enters the history."""
        self.history.append({"role": "assistant", "content": self.last_response["content"]})

    def exclude(self, reason, calls):
        self.event("turn_excluded", {"turn": self.turn, "stop_reason": self.last_response["stop_reason"], "reason": reason, "text_blocks": 0, "tool_calls": calls})
        self.event("repair", {"form": "reissue", "reason": reason, "after_turn": self.turn, "repairs_used": 1})

    def execute(self, call_id, arguments, summary=None):
        internal = "call:%d" % self.calls
        self.calls += 1
        self.event("tool_called", {"call_id": internal, "provider_call_id": call_id, "tool": "view", "version": "1", "arguments": arguments, "executed": True, "reason": None, "invalid_args": None, "status": None, "duration_ms": None})
        frame = "crop:%s:0" % internal
        image = self.put(("crop-" + internal).encode())
        summary = summary or "A closer look at %s." % internal
        envelope = self.put(json.dumps({"status": "ok", "summary": summary, "images": [{"frame": frame}], "provenance": {"duration_ms": 3}}).encode())
        self.event("tool_returned", {"call_id": internal, "status": "ok", "regions": 0, "images": [{"frame": frame, "artifact": image, "parent": "raw:candidate", "root": "raw:candidate", "mapping": {"kind": "crop"}, "width": 4, "height": 4}], "added": [], "duration_ms": 3, "envelope": envelope})
        self.history.append({"role": "user", "content": [{"type": "tool_result", "tool_use_id": call_id, "content": json.dumps({"call_id": internal, "status": "ok", "summary": summary, "score": 0.50}), "is_error": False, "images": [frame]}]})

    def reject(self, call_id, reason="invalid_submission"):
        self.event("repair", {"form": "rejected", "reason": reason, "after_turn": self.turn, "repairs_used": 1})
        self.history.append({"role": "user", "content": [{"type": "tool_result", "tool_use_id": call_id, "content": "That submission is not valid.", "is_error": True, "images": []}]})

    def finish(self, verdict="pass", call_id=None, error=None, kind="error"):
        base = {"id": "x", "set": "set-1", "item": ITEM, "cell": "provider:model-x/C", "repetition": 1, "dir": "out/rep-1/decision-1-1"}
        if error is None:
            self.event("submission", {"provider_call_id": call_id, "items": [{}], "findings": 0, "finding_rows": []})
            self.event("verdict", {"verdict": verdict})
            result = dict(base, verdict=verdict, error_class=None, error=None)
        else:
            self.event(kind, {"class": error, "message": "the submission was not valid", "state": "turn_received"})
            result = dict(base, verdict=None, error_class=error, error="the submission was not valid")
        self.write(result)

    def write(self, result):
        sent = {e["data"]["turn"]: e["data"]["request"] for e in self.events if e["event"] == "turn_sent"}
        got = {e["data"]["turn"]: e["data"]["response"] for e in self.events if e["event"] == "turn_received"}
        report = {"runs": [{"run": 1, "terminal": self.events[-1]["event"], "turns": [{"turn": n, "request": sent[n], "response": got.get(n)} for n in sorted(sent)]}]}
        with open(os.path.join(self.decision, "report.json"), "w") as f:
            json.dump(report, f)
        self.write_journal()
        with open(os.path.join(os.path.dirname(self.decision), "result.json"), "w") as f:
            json.dump(result, f)
        with open(os.path.join(self.decision, "decision.json"), "w") as f:
            json.dump({"harness_version": "0.0.0 (synthetic)", "use_case": "source-fidelity", "prompt_version": "p1", "config": {"provider": {"model": "provider:model-x"}}}, f)
        prep = os.path.join(self.root, "assembly", "generated")
        os.makedirs(prep, exist_ok=True)
        with open(os.path.join(prep, "preparation.json"), "w") as f:
            json.dump({"settings": {"model-x": {"long_edge_px": 1024, "jpeg_quality": 90}}}, f)
        with open(os.path.join(self.root, "PACKAGE.digest"), "w") as f:
            f.write("0" * 64 + "\n")

    def write_journal(self):
        with open(os.path.join(self.run, "journal.jsonl"), "w") as f:
            for e in self.events:
                f.write(json.dumps(e) + "\n")


def ordinary(root):
    """A view call executed, an invalid submission rejected, then the final submission."""
    p = Package(root)
    p.exchange([("c1", "view", {"image": "candidate"})])
    p.keep()
    p.execute("c1", {"image": "candidate"})
    p.exchange([("c2", "submit_verdict", {"items": "none", "findings": []})])
    p.keep()
    p.reject("c2")
    p.exchange([("c3", "submit_verdict", SUBMIT)])
    p.finish("pass", "c3")
    return p


def example(root):
    """The decision docs/baseline/example-transcript.json is converted from: a response cut at
    the token limit that held a call, an evidence call that ran, a submission that did not
    validate, and the final submission."""
    p = Package(root)
    p.exchange([("c0", "view", {"image": "candidate"})], stop="max_tokens")
    p.exclude("truncated", 1)
    p.exchange([("c1", "view", {"image": "candidate"})])
    p.keep()
    p.execute("c1", {"image": "candidate"})
    p.exchange([("c2", "submit_verdict", {"items": "none", "findings": []})])
    p.keep()
    p.reject("c2")
    p.exchange([("c3", "submit_verdict", SUBMIT)])
    p.finish("pass", "c3")
    return p


EXAMPLE = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(HERE))), "docs", "baseline", "example-transcript.json")


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.package = os.path.join(self.tmp.name, "package")
        self.out = os.path.join(self.tmp.name, "transcripts")
        os.makedirs(self.package)

    def run_tool(self, main, argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
            code = main(argv)
        self.printed = buf.getvalue()
        return code

    def convert(self):
        shutil.rmtree(self.out, ignore_errors=True)
        code = self.run_tool(convert.main, ["--package", self.package, "--out", self.out])
        self.assertEqual(code, 0, self.printed)
        return self.transcript()

    def path(self):
        found = []
        for folder, _d, files in os.walk(os.path.join(self.out, "historical")):
            found += [os.path.join(folder, n) for n in files]
        self.assertEqual(len(found), 1)
        return found[0]

    def transcript(self):
        return common.load_json(self.path())

    def audit(self, *more):
        return self.run_tool(audit.main, ["--package", self.package, "--transcripts", self.out] + list(more))

    def relist(self):
        """Writes the listing again, as someone hiding a change would."""
        data, _n = common.listing(self.out, leave_out=(common.LISTING, common.DIGEST, common.FINDINGS))
        common.write_private(os.path.join(self.out, common.LISTING), data)
        common.write_private(os.path.join(self.out, common.DIGEST), (common.digest_of_listing(data)[7:] + "\n").encode())

    def findings(self):
        with open(os.path.join(self.out, "audit-findings.jsonl")) as f:
            return [json.loads(line) for line in f]

    def rewrite(self, change):
        """Changes the written transcript, as a faulty converter would have."""
        t = self.transcript()
        change(t)
        with open(self.path(), "wb") as f:
            f.write(common.canonical(t))
        self.relist()

    def assert_invalid(self, change, says):
        self.rewrite(change)
        self.assertEqual(self.audit(), 1, self.printed)
        said = " | ".join(self.findings()[0]["invalid"])
        self.assertIn(says, said)


class Convert(Base):
    def test_an_ordinary_decision_is_complete_with_three_dispositions(self):
        ordinary(self.package)
        t = self.convert()
        self.assertTrue(t["complete"])
        self.assertEqual(t["transcript_version"], 1)
        got = [(c["id"], c["disposition"]) for turn in t["turns"] for c in turn["calls"]]
        self.assertEqual(got, [("c1", "executed"), ("c2", "rejected"), ("c3", "final")])
        self.assertEqual(t["turns"][0]["calls"][0]["result"]["images"], ["crop:call:0:0"])
        self.assertFalse(t["turns"][0]["calls"][0]["result"]["is_error"])
        self.assertEqual(t["subject"], {"set": "set-1", "item": ITEM, "model": "provider:model-x", "arm": "C", "repetition": 1, "use_case": "source-fidelity"})
        self.assertEqual(t["submission_tool"], "submit_verdict")
        self.assertEqual(common.schema_errors(t), [])
        self.assertIsNone(t["turns"][2]["calls"][0]["result"])
        self.assertEqual(t["terminal"], {"kind": "verdict", "verdict": "pass"})
        self.assertEqual(t["turns"][0]["request"]["system"], SYSTEM)
        self.assertEqual(t["turns"][0]["request"]["tools"], TOOLS)
        self.assertEqual(t["turns"][0]["request"]["max_output_tokens"], 512)
        self.assertEqual(self.audit(), 0, self.printed)

    def test_opaque_parts_of_a_response_are_kept_whole(self):
        ordinary(self.package)
        t = self.convert()
        self.assertEqual(t["turns"][0]["response"]["content"], [{"type": "opaque", "adapter": "synthetic", "block": {"call": "c1", "sig": "s-c1"}}])

    def test_usage_and_request_ids_are_under_meta_only(self):
        ordinary(self.package)
        t = self.convert()
        self.assertNotIn("usage", t["turns"][0]["response"])
        self.assertEqual(t["turns"][0]["meta"]["provider_request_id"], "req-1")
        self.assertEqual(t["turns"][1]["excluded"], None)
        self.assertEqual(t["volatile"], ["/origin", "/meta", "/files", "/turns/*/meta", "/turns/*/calls/*/meta"])

    def test_images_carry_ids_and_how_they_were_prepared(self):
        ordinary(self.package)
        t = self.convert()
        self.assertEqual([i["id"] for i in t["images"]], ["source", "candidate", "crop:call:0:0"])
        self.assertEqual(t["images"][0]["prepared"]["sha256"], hashlib.sha256(b"prepared-source").hexdigest())
        self.assertEqual(t["images"][2]["origin"], "c1")
        self.assertEqual(t["preparation"], {"version": "1", "settings": {"long_edge_px": 1024, "jpeg_quality": 90}})

    def test_the_same_package_gives_the_same_bytes(self):
        ordinary(self.package)
        self.convert()
        with open(self.path(), "rb") as f:
            first = f.read()
        with open(os.path.join(self.out, "TRANSCRIPTS.digest")) as f:
            digest = f.read()
        self.convert()
        with open(self.path(), "rb") as f:
            self.assertEqual(f.read(), first)
        with open(os.path.join(self.out, "TRANSCRIPTS.digest")) as f:
            self.assertEqual(f.read(), digest)

    def test_nothing_printed_names_an_item(self):
        ordinary(self.package)
        self.convert()
        self.assertNotIn(ITEM, self.printed)
        self.audit()
        self.assertNotIn(ITEM, self.printed)

    def test_no_package_digest_is_refused(self):
        ordinary(self.package)
        os.remove(os.path.join(self.package, "PACKAGE.digest"))
        self.assertEqual(self.run_tool(convert.main, ["--package", self.package, "--out", self.out]), 2)


class Completeness(Base):
    def test_an_excluded_turn_holding_a_call_is_complete_and_the_call_is_excluded(self):
        p = Package(self.package)
        p.exchange([("c1", "view", {"image": "candidate"})], stop="max_tokens")
        p.exclude("truncated", 1)
        p.exchange([("c2", "submit_verdict", SUBMIT)])
        p.finish("pass", "c2")
        t = self.convert()
        self.assertTrue(t["complete"])
        self.assertEqual(t["turns"][0]["excluded"], {"reason": "truncated"})
        self.assertEqual(t["turns"][0]["calls"][0]["disposition"], "excluded")
        self.assertIsNone(t["turns"][0]["calls"][0]["result"])
        self.assertEqual(t["repairs"], [{"after_turn": 1, "form": "reissue", "reason": "truncated"}])
        self.assertEqual(self.audit(), 0, self.printed)

    def test_a_response_cut_at_the_token_limit_is_complete_and_kept_as_recorded(self):
        p = Package(self.package)
        p.exchange([], stop="max_tokens", texts=["I was about to"])
        p.exclude("truncated", 0)
        p.exchange([("c2", "submit_verdict", SUBMIT)])
        p.finish("fail", "c2")
        t = self.convert()
        self.assertTrue(t["complete"])
        self.assertEqual(t["turns"][0]["response"]["stop_reason"], "max_tokens")
        self.assertEqual(t["turns"][0]["response"]["text"], ["I was about to"])
        self.assertEqual(t["turns"][1]["request"]["history"], t["turns"][0]["request"]["history"])
        self.assertEqual(self.audit(), 0, self.printed)

    def test_a_stored_response_cut_short_by_the_recorder_is_incomplete(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", SUBMIT)], truncate_response=True)
        p.finish("pass", "c1")
        t = self.convert()
        self.assertFalse(t["complete"])
        self.assertIsNone(t["turns"][0]["response"])
        self.assertIn("stored file cut short by the recorder: response 1", t["incomplete"])
        self.assertEqual(self.audit(), 3, self.printed)
        self.assertEqual(self.findings()[0]["outcome"], "incomplete")
        self.assertIn("1 incomplete", self.printed)

    def test_a_stored_request_cut_short_by_the_recorder_is_incomplete(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", SUBMIT)], truncate_request=True)
        p.finish("pass", "c1")
        t = self.convert()
        self.assertFalse(t["complete"])
        self.assertIsNone(t["turns"][0]["request"])
        self.assertEqual(self.audit(), 3, self.printed)

    def test_nothing_is_added_to_an_incomplete_record(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", SUBMIT)], truncate_response=True)
        p.finish("pass", "c1")
        t = self.convert()
        self.assertEqual(t["turns"][0]["calls"], [])

    def test_a_missing_stored_file_is_incomplete(self):
        p = ordinary(self.package)
        self.convert()
        t = self.transcript()
        os.remove(os.path.join(self.package, *t["files"][-1]["path"].split("/")))
        self.assertEqual(self.audit(), 3, self.printed)
        t = self.convert()
        self.assertFalse(t["complete"])
        del p

    def test_a_stored_file_with_other_bytes_is_incomplete(self):
        ordinary(self.package)
        t = self.convert()
        entry = next(e for e in t["files"] if e["role"] == "request 1")
        with open(os.path.join(self.package, *entry["path"].split("/")), "ab") as f:
            f.write(b" ")
        self.assertEqual(self.audit(), 3, self.printed)

    def test_a_recorded_error_with_a_whole_record_is_complete(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", {"items": "none"})])
        p.finish(error="submission_invalid")
        t = self.convert()
        self.assertTrue(t["complete"])
        self.assertEqual(t["terminal"], {"kind": "error", "class": "submission_invalid", "message": "the submission was not valid"})
        self.assertEqual(t["turns"][0]["calls"][0]["disposition"], "final")
        self.assertEqual(self.audit(), 0, self.printed)

    def test_a_cancelled_runs_undelivered_call_is_not_called_final(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", SUBMIT)])
        p.finish(error="interrupted", kind="cancelled")
        t = self.convert()
        self.assertFalse(t["complete"])
        self.assertIsNone(t["turns"][0]["calls"][0]["disposition"])
        self.assertIn("a call has neither a result nor an end of the run", t["incomplete"])
        self.assertEqual(self.audit(), 3, self.printed)

    def test_another_error_does_not_make_the_last_call_final(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", SUBMIT)])
        p.finish(error="budget_exhausted")
        t = self.convert()
        self.assertIsNone(t["turns"][0]["calls"][0]["disposition"])
        self.assertEqual(self.audit(), 3, self.printed)

    def test_an_evidence_call_left_unanswered_in_the_last_turn_is_not_final(self):
        p = Package(self.package)
        p.exchange([("c1", "view", {"image": "candidate"})])
        p.finish(error="submission_invalid")
        t = self.convert()
        self.assertIsNone(t["turns"][0]["calls"][0]["disposition"])

    def test_a_call_with_neither_a_result_nor_an_end_is_reported_not_guessed(self):
        p = Package(self.package)
        p.exchange([("c1", "view", {"image": "candidate"})])
        p.keep()
        p.exchange([("c2", "submit_verdict", SUBMIT)])
        p.finish("pass", "c2")
        t = self.convert()
        self.assertFalse(t["complete"])
        self.assertIsNone(t["turns"][0]["calls"][0]["disposition"])
        self.assertEqual(self.audit(), 3, self.printed)
        self.assertEqual(self.findings()[0]["outcome"], "incomplete")

    def test_numbers_keep_the_text_they_were_stored_with(self):
        ordinary(self.package)
        t = self.convert()
        self.assertTrue(isinstance(t["turns"][0]["meta"]["bound_usd"], common.Num))
        self.assertEqual(str(t["turns"][0]["meta"]["bound_usd"]), "0.1")
        value = common.loads('{"a": 1.50, "b": 1e-07, "c": 12345678901234567890123, "d": -0.0}')
        self.assertEqual(common.canonical(value).decode(), '{\n  "a": 1.50,\n  "b": 1e-07,\n  "c": 12345678901234567890123,\n  "d": -0.0\n}\n')


class Example(Base):
    """The example on the format's page is what the converter writes, so the page cannot
    drift from the code."""

    def test_the_committed_example_is_the_converters_output_and_passes_the_audit(self):
        example(self.package)
        t = self.convert()
        self.assertEqual(common.schema_errors(t), [])
        self.assertEqual(self.audit(), 0, self.printed)
        with open(self.path(), "rb") as f:
            fresh = f.read()
        if os.environ.get("WHIZBANG_WRITE_EXAMPLE") == "1":
            with open(EXAMPLE, "wb") as f:
                f.write(fresh)
        with open(EXAMPLE, "rb") as f:
            committed = f.read()
        self.assertTrue(committed == fresh, "docs/baseline/example-transcript.json is stale: run the tests with WHIZBANG_WRITE_EXAMPLE=1")

    def test_the_example_shows_every_disposition_and_an_excluded_turn(self):
        t = common.load_json(EXAMPLE)
        self.assertEqual(common.schema_errors(t), [])
        got = [c["disposition"] for turn in t["turns"] for c in turn["calls"]]
        self.assertEqual(got, ["excluded", "executed", "rejected", "final"])
        self.assertEqual(t["turns"][0]["excluded"], {"reason": "truncated"})
        self.assertEqual(t["turns"][0]["response"]["stop_reason"], "max_tokens")

    def test_the_schema_refuses_what_the_page_does_not_describe(self):
        for change, says in [
            (lambda t: t.update(extra=1), "unknown field extra"),
            (lambda t: t["subject"].pop("arm"), "/subject: no arm"),
            (lambda t: t["turns"][0].update(excluded="truncated"), "/turns/0/excluded: not an object"),
            (lambda t: t["turns"][0]["excluded"].update(reason="other"), "/turns/0/excluded/reason: not one of"),
            (lambda t: t["turns"][1]["calls"][0].update(disposition="done"), "/turns/1/calls/0/disposition: not one of"),
            (lambda t: t["turns"][1]["response"].update(text="a"), "/turns/1/response/text: not a list"),
            (lambda t: t["turns"][2]["request"]["history"][2]["content"][0].pop("tool_use_id"), "no tool_use_id"),
            (lambda t: t["turns"][1]["response"]["content"][0].pop("adapter"), "no adapter"),
            (lambda t: t["repairs"][0].update(after_turn="1"), "/repairs/0/after_turn: not a whole number"),
            (lambda t: t["images"][0]["prepared"].pop("bytes"), "/images/0/prepared: no bytes"),
            (lambda t: t["images"][0]["source"].pop("pixel_sha256"), "/images/0/source: no pixel_sha256"),
            (lambda t: t["images"][0].update(width=8.0), "/images/0/width: not a whole number"),
            (lambda t: t["preparation"].update(settings=[1]), "/preparation/settings: not an object"),
            (lambda t: t.update(transcript_version=2), "/transcript_version: not 1"),
            (lambda t: t["volatile"].pop(), "/volatile: not the format's list"),
            (lambda t: t["terminal"].update(kind="done"), "/terminal: not a terminal outcome"),
            (lambda t: t["turns"][0]["request"].update(max_output_tokens="512"), "max_output_tokens: not a whole number"),
        ]:
            t = common.load_json(EXAMPLE)
            change(t)
            self.assertTrue(any(says in e for e in common.schema_errors(t)), says)


class Attempts(Base):
    def test_two_attempts_where_the_result_names_one_is_an_error_not_a_choice(self):
        p = ordinary(self.package)
        shutil.copytree(p.decision, os.path.join(os.path.dirname(p.decision), "decision-1-2"))
        self.assertEqual(self.run_tool(convert.main, ["--package", self.package, "--out", self.out]), 2)
        self.assertIn("a result names one attempt and its folder holds 2", self.printed)
        self.assertFalse(os.path.exists(os.path.join(self.out, "historical")))

    def test_the_attempt_converted_is_the_one_the_result_names(self):
        p = ordinary(self.package)
        os.rename(p.decision, os.path.join(os.path.dirname(p.decision), "decision-1-2"))
        self.assertEqual(self.run_tool(convert.main, ["--package", self.package, "--out", self.out]), 2)
        self.assertIn("holds 1", self.printed)

    def test_two_journals_in_one_attempt_is_an_error(self):
        p = ordinary(self.package)
        shutil.copytree(p.run, os.path.join(p.decision, "run-2"))
        self.assertEqual(self.run_tool(convert.main, ["--package", self.package, "--out", self.out]), 2)
        self.assertIn("an attempt holds 2 journals", self.printed)

    def test_converting_into_a_folder_that_holds_transcripts_is_refused(self):
        ordinary(self.package)
        self.convert()
        self.assertEqual(self.run_tool(convert.main, ["--package", self.package, "--out", self.out]), 2)


class Coverage(Base):
    """The audit covers the package: it cannot pass on fewer transcripts than fixtures."""

    def setUp(self):
        super().setUp()
        ordinary(self.package)
        self.convert()

    def test_no_listing_is_a_failure(self):
        os.remove(os.path.join(self.out, common.LISTING))
        self.assertEqual(self.audit(), 1)
        self.assertIn("has no TRANSCRIPTS.sha256", self.printed)

    def test_no_digest_is_a_failure(self):
        os.remove(os.path.join(self.out, common.DIGEST))
        self.assertEqual(self.audit(), 1)
        self.assertIn("has no TRANSCRIPTS.digest", self.printed)

    def test_a_transcript_deleted_with_the_listing_deleted(self):
        os.remove(self.path())
        os.remove(os.path.join(self.out, common.LISTING))
        os.remove(os.path.join(self.out, common.DIGEST))
        self.assertEqual(self.audit(), 1)

    def test_a_transcript_deleted_with_the_listing_written_again(self):
        os.remove(self.path())
        self.relist()
        self.assertEqual(self.audit(), 1)
        self.assertIn("1 without a transcript, 0 transcripts without a fixture", self.printed)

    def test_a_transcript_edited_with_the_listing_deleted(self):
        t = self.transcript()
        t["terminal"]["verdict"] = "fail"
        with open(self.path(), "wb") as f:
            f.write(common.canonical(t))
        os.remove(os.path.join(self.out, common.LISTING))
        os.remove(os.path.join(self.out, common.DIGEST))
        self.assertEqual(self.audit(), 1)

    def test_a_transcript_with_no_fixture(self):
        shutil.copy(self.path(), os.path.join(os.path.dirname(self.path()), "decision-9-9.json"))
        self.relist()
        self.assertEqual(self.audit(), 1)
        self.assertIn("0 without a transcript, 1 transcripts without a fixture", self.printed)

    def manifest(self, digest="sha256:" + "0" * 64, decisions=1):
        path = os.path.join(self.tmp.name, "manifest.toml")
        with open(path, "w") as f:
            f.write('package_version = 1\ndigest = "%s"\n\n[[cell]]\nmodel = "m"\narm = "D"\ndecisions = 7\n\n[[cell]]\nmodel = "m"\narm = "C"\ndecisions = %d\nspend_usd = 1.0\n' % (digest, decisions))
        return path

    def test_the_count_is_checked_against_the_manifest(self):
        self.assertEqual(self.audit("--manifest", self.manifest()), 0, self.printed)
        self.assertEqual(self.audit("--manifest", self.manifest(decisions=2)), 1)
        self.assertIn("the manifest counts other decisions", self.printed)

    def test_a_manifest_of_another_package_is_refused(self):
        self.assertEqual(self.audit("--manifest", self.manifest(digest="sha256:" + "1" * 64)), 1)
        self.assertIn("the manifest names another package", self.printed)

    def test_a_summary_needs_a_manifest(self):
        self.assertEqual(self.audit("--summary", os.path.join(self.tmp.name, "a.toml")), 1)
        self.assertFalse(os.path.exists(os.path.join(self.tmp.name, "a.toml")))


class Audit(Base):
    def setUp(self):
        super().setUp()
        ordinary(self.package)
        self.convert()

    def test_a_changed_transcript_no_longer_matches_the_listing(self):
        t = self.transcript()
        t["subject"]["item"] = "other"
        with open(self.path(), "wb") as f:
            f.write(common.canonical(t))
        self.assertEqual(self.audit(), 1)
        self.assertIn("not the ones the converter listed", self.printed)

    # What the transcript took from the journal and the old result is taken again.

    def test_repairs_emptied(self):
        def change(t):
            t["repairs"] = []
        self.assert_invalid(change, "the repairs are not the journal's")

    def test_a_changed_subject(self):
        def change(t):
            t["subject"]["model"] = "provider:model-y"
        self.assert_invalid(change, "the subject is not the old result's")

    def test_a_prepared_hash_swapped_for_another_listed_hash(self):
        def change(t):
            t["images"][0]["prepared"], t["images"][1]["prepared"] = t["images"][1]["prepared"], t["images"][0]["prepared"]
        self.assert_invalid(change, "the images are not the journal's")

    def test_preparation_settings_replaced(self):
        def change(t):
            t["preparation"]["settings"] = {"long_edge_px": 64}
        self.assert_invalid(change, "the preparation is not the journal's version and the package's settings")

    def test_a_changed_error_message(self):
        p = os.path.join(self.tmp.name, "other")
        os.makedirs(p)
        self.package = p
        q = Package(p)
        q.exchange([("c1", "submit_verdict", {"items": "none"})])
        q.finish(error="submission_invalid")
        self.convert()
        def change(t):
            t["terminal"]["message"] = "another message"
        self.assert_invalid(change, "the terminal outcome is not the journal's")

    def test_an_error_message_that_is_not_the_old_results(self):
        p = os.path.join(self.tmp.name, "other")
        os.makedirs(p)
        self.package = p
        q = Package(p)
        q.exchange([("c1", "submit_verdict", {"items": "none"})])
        q.event("error", {"class": "submission_invalid", "message": "the journal says this", "state": "turn_received"})
        q.write({"id": "x", "set": "set-1", "item": ITEM, "cell": "provider:model-x/C", "repetition": 1, "dir": "o/decision-1-1", "verdict": None, "error_class": "submission_invalid", "error": "the result says that"})
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("the error is not the old result's", " | ".join(self.findings()[0]["invalid"]))

    def test_a_journal_that_lost_its_last_turn(self):
        p = os.path.join(self.tmp.name, "other")
        os.makedirs(p)
        self.package = p
        q = Package(p)
        q.exchange([("c0", "submit_verdict", {"items": "none"})])
        q.keep()
        q.reject("c0")
        q.exchange([("c1", "submit_verdict", {"items": "none"})])
        q.finish(error="submission_invalid")
        # The last request and response are lost from the journal, and the repair before
        # them; the error stays, so the first submission looks like the one that ended it.
        q.events = [e for e in q.events if e["event"] != "repair"]
        q.events = [e for e in q.events if not (e["event"] in ("turn_sent", "turn_received") and e["data"]["turn"] == 2)]
        q.write_journal()
        t = self.convert()
        self.assertEqual(len(t["turns"]), 1)
        self.assertTrue(t["complete"])
        self.assertEqual(self.audit(), 1, self.printed)
        said = " | ".join(self.findings()[0]["invalid"])
        self.assertIn("the journal's events are not numbered without a gap", said)
        # The same loss with the events numbered again is caught by the old report.
        for number, e in enumerate(q.events):
            e["seq"] = number
        q.write_journal()
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("the turns are not the turns of the old report", " | ".join(self.findings()[0]["invalid"]))

    def test_a_journal_with_two_terminal_events(self):
        p = os.path.join(self.tmp.name, "other")
        os.makedirs(p)
        self.package = p
        q = Package(p)
        q.exchange([("c1", "submit_verdict", {"items": "none"})])
        q.event("error", {"class": "submission_invalid", "message": "m", "state": None})
        q.finish(error="submission_invalid")
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("the journal does not end with its one terminal event", " | ".join(self.findings()[0]["invalid"]))

    def test_results_swapped_between_two_calls_with_consistent_hashes(self):
        p = os.path.join(self.tmp.name, "other")
        os.makedirs(p)
        self.package = p
        q = Package(p)
        q.exchange([("c1", "view", {"image": "candidate"}), ("c2", "view", {"image": "source"})])
        q.keep()
        q.execute("c1", {"image": "candidate"})
        q.execute("c2", {"image": "source"})
        # The defect: each result is filed under the other call's id, everywhere.
        for message in q.history:
            for block in message["content"]:
                if block.get("type") == "tool_result":
                    block["tool_use_id"] = {"c1": "c2", "c2": "c1"}[block["tool_use_id"]]
        q.exchange([("c3", "submit_verdict", SUBMIT)])
        q.finish("pass", "c3")
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("a call's result is not the result of its stored envelope", " | ".join(self.findings()[0]["invalid"]))

    def test_results_swapped_between_two_calls_that_say_the_same_thing(self):
        p = os.path.join(self.tmp.name, "other")
        os.makedirs(p)
        self.package = p
        q = Package(p)
        q.exchange([("c1", "view", {"image": "candidate"}), ("c2", "view", {"image": "source"})])
        q.keep()
        q.execute("c1", {"image": "candidate"}, summary="Nothing to see.")
        q.execute("c2", {"image": "source"}, summary="Nothing to see.")
        for message in q.history:
            for block in message["content"]:
                if block.get("type") == "tool_result":
                    block["tool_use_id"] = {"c1": "c2", "c2": "c1"}[block["tool_use_id"]]
        q.exchange([("c3", "submit_verdict", SUBMIT)])
        q.finish("pass", "c3")
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("a call's result is not the result of its stored envelope", " | ".join(self.findings()[0]["invalid"]))

    def test_a_final_call_that_is_not_the_accepted_submission(self):
        def change(t):
            t["turns"][2]["calls"][0]["arguments"]["items"] = []
            t["turns"][2]["response"]["tool_calls"][0]["arguments"]["items"] = []
        self.assert_invalid(change, "the response is not the stored response")

    def test_final_arguments_that_are_not_what_the_journal_accepted(self):
        p = os.path.join(self.tmp.name, "other")
        os.makedirs(p)
        self.package = p
        q = Package(p)
        q.exchange([("c1", "submit_verdict", dict(SUBMIT, items=[]))])
        q.finish("pass", "c1")
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("the final submission's arguments are not what the journal accepted", " | ".join(self.findings()[0]["invalid"]))

    def test_an_unknown_field_is_not_the_format(self):
        def change(t):
            t["turns"][0]["extra"] = 1
        self.assert_invalid(change, "/turns/0: unknown field extra")

    def test_excluded_is_an_object_or_null(self):
        def change(t):
            t["turns"][0]["excluded"] = "truncated"
        self.assert_invalid(change, "/turns/0/excluded: not an object")

    def test_a_request_that_is_not_the_stored_request(self):
        def change(t):
            t["turns"][0]["request"]["system"] = "Another prompt."
        self.assert_invalid(change, "turn 1: the request is not the stored request")

    def test_a_response_that_is_not_the_stored_response(self):
        def change(t):
            t["turns"][0]["response"]["content"][0]["block"]["sig"] = "forged"
        self.assert_invalid(change, "turn 1: the response is not the stored response")

    def test_an_executed_call_without_its_result(self):
        def change(t):
            t["turns"][0]["calls"][0]["result"] = None
        self.assert_invalid(change, "a call's result is not the result in the history")

    def test_a_rejected_call_marked_executed(self):
        def change(t):
            t["turns"][1]["calls"][0]["disposition"] = "executed"
        self.assert_invalid(change, "the executed calls are not the calls the journal says returned")

    def test_an_executed_call_marked_excluded(self):
        def change(t):
            t["turns"][0]["calls"][0]["disposition"] = "excluded"
        self.assert_invalid(change, "excluded turn and disposition disagree")

    def test_a_final_submission_with_a_result(self):
        def change(t):
            t["turns"][2]["calls"][0]["result"] = {"content": "x", "is_error": False, "images": []}
        self.assert_invalid(change, "a call that is final has a result")

    def test_a_final_submission_that_is_not_last(self):
        def change(t):
            t["turns"][1]["calls"][0]["disposition"] = "final"
        self.assert_invalid(change, "the final submission is not in the last turn")

    def test_a_verdict_with_no_final_submission(self):
        def change(t):
            t["turns"][2]["calls"] = []
        self.assert_invalid(change, "the calls are not the calls of the response")

    def test_a_verdict_that_is_not_the_old_results(self):
        def change(t):
            t["terminal"]["verdict"] = "fail"
        self.assert_invalid(change, "the terminal outcome is not the journal's")

    def test_a_verdict_replaced_by_an_error(self):
        def change(t):
            t["terminal"] = {"kind": "error", "class": "submission_invalid", "message": "m"}
        self.assert_invalid(change, "the terminal outcome is not the journal's")

    def test_a_verdict_that_is_not_the_old_results(self):
        with open(os.path.join(self.package, *[e for e in self.transcript()["files"] if e["role"] == "result"][0]["path"].split("/"))) as f:
            result = json.load(f)
        result["verdict"] = "fail"
        ordinary_path = os.path.join(self.package, *[e for e in self.transcript()["files"] if e["role"] == "result"][0]["path"].split("/"))
        with open(ordinary_path, "w") as f:
            json.dump(result, f)
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("the verdict is not the old result's", " | ".join(self.findings()[0]["invalid"]))

    def test_turns_out_of_order(self):
        def change(t):
            t["turns"][0], t["turns"][1] = t["turns"][1], t["turns"][0]
        self.assert_invalid(change, "turns are not numbered 1 to 3 in order")

    def test_a_history_that_does_not_extend_the_one_before(self):
        def change(t):
            t["turns"][1]["request"]["history"][0]["content"][0]["text"] = "Changed."
        self.assert_invalid(change, "turn 2: the history does not begin with the history before it")

    def test_an_image_that_is_not_listed(self):
        def change(t):
            t["images"] = t["images"][:2]
        self.assert_invalid(change, "the images are not the journal's")

    def test_a_weakened_volatile_declaration(self):
        def change(t):
            t["volatile"].append("/turns")
        self.assert_invalid(change, "/volatile: not the format's list")

    def test_marked_complete_when_a_file_is_cut_short(self):
        def change(t):
            t["files"][3]["truncated"] = True
        self.assert_invalid(change, "marked complete, and the record is not whole")

    def test_the_summary_holds_counts_only(self):
        summary = os.path.join(self.tmp.name, "audit.toml")
        manifest = os.path.join(self.tmp.name, "manifest.toml")
        with open(manifest, "w") as f:
            f.write('digest = "sha256:%s"\n[[cell]]\narm = "C"\ndecisions = 1\n' % ("0" * 64))
        code = self.run_tool(audit.main, ["--package", self.package, "--transcripts", self.out, "--manifest", manifest, "--summary", summary])
        self.assertEqual(code, 0, self.printed)
        with open(summary) as f:
            text = f.read()
        self.assertIn("fixtures = 1\ndecisions = 1\ncomplete = 1\nincomplete = 0\ninvalid = 0", text)
        self.assertIn("[calls_by_disposition]\nexecuted = 1\nfinal = 1\nrejected = 1", text)
        self.assertIn("[terminal_outcomes]\nverdict = 1", text)
        self.assertNotIn(ITEM, text)
        self.assertNotIn(self.tmp.name, text)
        self.assertNotIn("pass", text)


class ExcludedHistory(Base):
    def test_an_excluded_response_that_entered_the_history(self):
        p = Package(self.package)
        p.exchange([], stop="max_tokens", texts=["I was about to"])
        p.exclude("truncated", 0)
        p.keep()  # the defect: an excluded response echoed
        p.exchange([("c2", "submit_verdict", SUBMIT)])
        p.finish("pass", "c2")
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("an excluded response entered the history", " | ".join(self.findings()[0]["invalid"]))


if __name__ == "__main__":
    unittest.main()
