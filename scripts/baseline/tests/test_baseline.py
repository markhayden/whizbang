"""Tests of convert.py and audit.py on a synthetic package built in a temporary folder.
No network, no real fixture. Run by scripts/tests/test-baseline.sh."""

import contextlib
import copy
import hashlib
import io
import json
import os
import sys
import tempfile
import unittest

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
        self.events.append({"seq": len(self.events) + 1, "unix_ms": 1, "event": name, "data": data})

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

    def execute(self, call_id, arguments):
        internal = "call:%d" % self.calls
        self.calls += 1
        self.event("tool_called", {"call_id": internal, "provider_call_id": call_id, "tool": "view", "version": "1", "arguments": arguments, "executed": True, "reason": None, "invalid_args": None, "status": None, "duration_ms": None})
        frame = "crop:%s:0" % internal
        image = self.put(("crop-" + internal).encode())
        envelope = self.put(json.dumps({"status": "ok", "provenance": {"duration_ms": 3}}).encode())
        self.event("tool_returned", {"call_id": internal, "status": "ok", "regions": 0, "images": [{"frame": frame, "artifact": image, "parent": "raw:candidate", "root": "raw:candidate", "mapping": {"kind": "crop"}, "width": 4, "height": 4}], "added": [], "duration_ms": 3, "envelope": envelope})
        self.history.append({"role": "user", "content": [{"type": "tool_result", "tool_use_id": call_id, "content": "A closer look.", "is_error": False, "images": [frame]}]})

    def reject(self, call_id, reason="invalid_submission"):
        self.event("repair", {"form": "rejected", "reason": reason, "after_turn": self.turn, "repairs_used": 1})
        self.history.append({"role": "user", "content": [{"type": "tool_result", "tool_use_id": call_id, "content": "That submission is not valid.", "is_error": True, "images": []}]})

    def finish(self, verdict="pass", call_id=None, error=None):
        if error is None:
            self.event("submission", {"provider_call_id": call_id, "items": [], "findings": 0, "finding_rows": []})
            self.event("verdict", {"verdict": verdict})
            result = {"id": "x", "set": "set-1", "item": ITEM, "cell": "c", "repetition": 1, "verdict": verdict, "error_class": None}
        else:
            self.event("error", {"class": error, "message": "the submission was not valid", "state": "turn_received"})
            result = {"id": "x", "set": "set-1", "item": ITEM, "cell": "c", "repetition": 1, "verdict": "error", "error_class": error}
        with open(os.path.join(self.run, "journal.jsonl"), "w") as f:
            for e in self.events:
                f.write(json.dumps(e) + "\n")
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


def ordinary(root):
    """A view call executed, an invalid submission rejected, then the final submission."""
    p = Package(root)
    p.exchange([("c1", "view", {"image": "candidate"})])
    p.keep()
    p.execute("c1", {"image": "candidate"})
    p.exchange([("c2", "submit_verdict", {"verdict": "maybe"})])
    p.keep()
    p.reject("c2")
    p.exchange([("c3", "submit_verdict", {"verdict": "pass"})])
    p.finish("pass", "c3")
    return p


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

    def audit(self):
        return self.run_tool(audit.main, ["--package", self.package, "--transcripts", self.out])

    def findings(self):
        with open(os.path.join(self.out, "audit-findings.jsonl")) as f:
            return [json.loads(line) for line in f]

    def rewrite(self, change):
        """Changes the written transcript, as a faulty converter would have."""
        t = self.transcript()
        change(t)
        with open(self.path(), "wb") as f:
            f.write(common.canonical(t))
        os.remove(os.path.join(self.out, "TRANSCRIPTS.digest"))

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
        self.assertEqual(t["turns"][0]["calls"][0]["result"], {"content": "A closer look.", "is_error": False, "images": ["crop:call:0:0"]})
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
        p.exchange([("c2", "submit_verdict", {"verdict": "pass"})])
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
        p.exchange([("c2", "submit_verdict", {"verdict": "fail"})])
        p.finish("fail", "c2")
        t = self.convert()
        self.assertTrue(t["complete"])
        self.assertEqual(t["turns"][0]["response"]["stop_reason"], "max_tokens")
        self.assertEqual(t["turns"][0]["response"]["text"], ["I was about to"])
        self.assertEqual(t["turns"][1]["request"]["history"], t["turns"][0]["request"]["history"])
        self.assertEqual(self.audit(), 0, self.printed)

    def test_a_stored_response_cut_short_by_the_recorder_is_incomplete(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", {"verdict": "pass"})], truncate_response=True)
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
        p.exchange([("c1", "submit_verdict", {"verdict": "pass"})], truncate_request=True)
        p.finish("pass", "c1")
        t = self.convert()
        self.assertFalse(t["complete"])
        self.assertIsNone(t["turns"][0]["request"])
        self.assertEqual(self.audit(), 3, self.printed)

    def test_nothing_is_added_to_an_incomplete_record(self):
        p = Package(self.package)
        p.exchange([("c1", "submit_verdict", {"verdict": "pass"})], truncate_response=True)
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
        p.exchange([("c1", "submit_verdict", {"verdict": "maybe"})])
        p.finish(error="submission_invalid")
        t = self.convert()
        self.assertTrue(t["complete"])
        self.assertEqual(t["terminal"], {"kind": "error", "class": "submission_invalid", "message": "the submission was not valid"})
        self.assertEqual(t["turns"][0]["calls"][0]["disposition"], "final")
        self.assertEqual(self.audit(), 0, self.printed)

    def test_a_call_with_neither_a_result_nor_an_end_is_reported_not_guessed(self):
        p = Package(self.package)
        p.exchange([("c1", "view", {"image": "candidate"})])
        p.keep()
        p.exchange([("c2", "submit_verdict", {"verdict": "pass"})])
        p.finish("pass", "c2")
        t = self.convert()
        self.assertFalse(t["complete"])
        self.assertIsNone(t["turns"][0]["calls"][0]["disposition"])
        self.assertEqual(self.audit(), 1, self.printed)


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
        self.assert_invalid(change, "an executed call has no stored result")

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
        self.assert_invalid(change, "the verdict is not the old result's")

    def test_a_verdict_replaced_by_an_error(self):
        def change(t):
            t["terminal"] = {"kind": "error", "class": "submission_invalid", "message": "m"}
        self.assert_invalid(change, "the error is not the old result's")

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
        self.assert_invalid(change, "a history names an image that is not listed")

    def test_a_weakened_volatile_declaration(self):
        def change(t):
            t["volatile"].append("/turns")
        self.assert_invalid(change, "the volatile declaration is not the format's")

    def test_marked_complete_when_a_file_is_cut_short(self):
        def change(t):
            t["files"][3]["truncated"] = True
        self.assert_invalid(change, "marked complete, and the record is not whole")

    def test_the_summary_holds_counts_only(self):
        summary = os.path.join(self.tmp.name, "audit.toml")
        code = self.run_tool(audit.main, ["--package", self.package, "--transcripts", self.out, "--summary", summary])
        self.assertEqual(code, 0, self.printed)
        with open(summary) as f:
            text = f.read()
        self.assertIn("decisions = 1\ncomplete = 1\nincomplete = 0\ninvalid = 0", text)
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
        p.exchange([("c2", "submit_verdict", {"verdict": "pass"})])
        p.finish("pass", "c2")
        self.convert()
        self.assertEqual(self.audit(), 1, self.printed)
        self.assertIn("an excluded response entered the history", " | ".join(self.findings()[0]["invalid"]))


if __name__ == "__main__":
    unittest.main()
