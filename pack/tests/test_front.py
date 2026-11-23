"""The front and its supervision (ADR 0046; docs/plans/m6-http-and-remote-hosting.md, task
P1, item 6): `pack/1` over HTTP on loopback, with the pack's real worker and its fake models
behind it, each duty of the Rust supervisor (ADR 0020, "The worker") held by a test here.

Every worker is a child process, killed and waited for in the test's cleanup; every front
is closed there. Deadlines are the front's own, but for the cancel's, the call's and the
classifier's, which the tests that wait for them give shorter."""

import io
import os
import signal
import socket
import ssl
import threading
import time
import unittest

import front_kit as fk
import kit
from whizbang_pack import fakes, front, proto


def tearDownModule():
    import shutil

    shutil.rmtree(fk._CACHE, True)


def setup(test, limits=None, module="whizbang_pack.worker", more=(), secrets=(), front_limits=None):
    models = fk.Models(test)
    side = fk.Recording(fk.local_side(test, models, limits, module, more, secrets=secrets))
    made, port = fk.serve(test, side, models.dir, front_limits or limits)
    return models, side, made, port


def kill(side):
    """Kills the side's worker from outside, as a host that replaced it would, and waits
    until it is gone."""
    process = side.side._worker.process
    os.kill(process.pid, signal.SIGKILL)
    fk.wait_until(lambda: process.poll() is not None)


def kinds(side):
    return [(kind, model) for _, kind, model, _ in side.journal]


class Routes(unittest.TestCase):
    def test_a_request_streams_the_hello_its_events_and_its_result(self):
        models, side, _, port = setup(self, module=fk.SAY_WORKER)
        sock, load, request = fk.post_request(port, "fake-say", models.identity["fake-say"])
        answer, frames, document, lines = fk.answer_of(sock, load, request)
        self.assertEqual(answer.status, 200)
        self.assertEqual(answer.headers["whizbang-model-files"], front.files_digest(models.record["fake-say"]["files"]))
        hello = frames[0]
        self.assertEqual((hello["type"], hello["id"], hello["role"]), ("hello", "c1", "worker"))
        self.assertEqual([(m["type"], m["id"]) for m in frames[1:]], [("result", "r1"), ("event", "r2"), ("event", "r2"), ("result", "r2")])
        self.assertEqual(frames[2]["text"], "looking")
        self.assertEqual(len(frames[-1]["instances"]), 1)
        self.assertEqual((document["outcome"], document["stage"], document["ended"], document["kind"]), ("complete", "answered", None, None))

    def test_without_the_token_every_route_but_health_is_refused(self):
        models, side, _, port = setup(self)
        for token in (None, "x" * 40, fk.TOKEN + "x"):
            data, _, _ = fk.body("fake-ok", models.identity["fake-ok"])
            sock = fk.connect(port)
            sock.sendall(fk.head("/pack/1/request", fk.dispatch(), len(data), token=token) + data)
            answer = fk.Answer(sock)
            self.assertEqual(answer.status, 401)
            self.assertEqual(answer.rest(), b"")
            sock.close()
            data = fk.cancel_body()
            sock = fk.connect(port)
            sock.sendall(fk.head("/pack/1/cancel", fk.dispatch(), len(data), token=token) + data)
            self.assertEqual(fk.Answer(sock).status, 401)
            sock.close()
        self.assertEqual(fk.get(port)[0], 200)
        self.assertEqual(side.spawns.count, 0)

    def test_a_body_over_its_bound_is_refused_unread(self):
        models, side, _, port = setup(self)
        sock = fk.connect(port)
        # The head alone: the front answers before a byte of the body is sent.
        sock.sendall(fk.head("/pack/1/request", fk.dispatch(), front.BODY_MOST + 1))
        self.assertEqual(fk.Answer(sock).status, 413)
        sock.close()
        sock = fk.connect(port)
        sock.sendall(fk.head("/pack/1/cancel", fk.dispatch(), front.CONTROL_BODY_MOST + 1))
        self.assertEqual(fk.Answer(sock).status, 413)
        sock.close()
        self.assertEqual(side.spawns.count, 0)

    def test_health_needs_no_token_starts_no_worker_and_names_the_hashes(self):
        models, side, _, port = setup(self)
        status, data = fk.get(port)
        self.assertEqual(status, 200)
        import json

        health = json.loads(data)
        self.assertEqual((health["format"], health["protocol"]), ("pack-host/1", "pack/1"))
        named = {each["model"]: each for each in health["models"]}
        self.assertEqual(sorted(named), sorted(fk.MODELS))
        for model in fk.MODELS:
            self.assertEqual(named[model]["installation"], models.identity[model])
            self.assertEqual(named[model]["files"], [{"name": f["name"], "sha256": f["sha256"]} for f in models.record[model]["files"]])
            self.assertFalse(named[model]["checked"])
        self.assertIsNone(health["device"])
        self.assertEqual(side.spawns.count, 0)

    def test_a_client_gone_mid_answer_cancels_its_request(self):
        models, side, _, port = setup(self)
        dispatch = fk.dispatch()
        sock, load, request = fk.post_request(port, "fake-wait", models.identity["fake-wait"], dispatch)
        answer = fk.Answer(sock)
        reader = answer.reader(load, request)
        self.assertEqual([reader.read()["type"] for _ in range(3)], ["hello", "result", "event"])
        answer.close()
        document = side.outcome(dispatch)
        # The worker stopped the call at its cancel and lives on: nothing was killed.
        self.assertEqual((document["outcome"], document["kind"], document["stage"], document["ended"]), ("failed", "cancelled", "answered", None))
        self.assertEqual(side.spawns.count, 1)


class Identity(unittest.TestCase):
    def test_each_answer_names_the_digest_of_the_files_it_came_from(self):
        models = fk.Models(self)
        side = fk.Recording(fk.local_side(self, models))
        _, port = fk.serve(self, side, models.dir)
        seen = []
        for model in ("fake-ok", "fake-ok", "fake-mps"):
            sock, load, request = fk.post_request(port, model, models.identity[model])
            answer, _, document, _ = fk.answer_of(sock, load, request)
            self.assertEqual(document["outcome"], "complete")
            seen.append(answer.headers["whizbang-model-files"])
        # Worked out apart from the front: the lines `<name> <sha256>`, in the order of names.
        import hashlib

        def expected(model):
            lines = "".join("%s %s\n" % (f["name"], f["sha256"]) for f in sorted(models.record[model]["files"], key=lambda f: f["name"]))
            return "sha256:" + hashlib.sha256(lines.encode("ascii")).hexdigest()

        self.assertEqual(seen, [expected("fake-ok"), expected("fake-ok"), expected("fake-mps")])
        self.assertNotEqual(seen[0], seen[2])
        self.assertEqual(front.files_digest([{"name": "b", "sha256": "y"}, {"name": "a", "sha256": "x"}]), front.files_digest([{"name": "a", "sha256": "x"}, {"name": "b", "sha256": "y"}]))


class Outcome(unittest.TestCase):
    def test_every_answer_ends_with_its_outcome_record_and_a_worker_end_carries_its_status_stage_and_redacted_error_output(self):
        # A worker that writes a marker, a host and a made-up token to its error output at
        # each call: the lines of the call's turn follow the record, the token taken out.
        models, side, _, port = setup(self, module="whizbang_pack.fakes", more=("--fake-fault", "error-output"), secrets=(fakes.TOKEN,))
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        _, frames, document, lines = fk.answer_of(sock, load, request)
        self.assertEqual((document["format"], document["outcome"], document["ended"]), ("pack-host/1", "complete", None))
        self.assertEqual(document["lines"], len(lines))
        joined = b"\n".join(lines)
        self.assertIn(fakes.MARKER.encode(), joined)
        self.assertNotIn(fakes.TOKEN.encode(), joined)
        self.assertIn(b"[redacted]", joined)
        self.assertTrue(all(len(line) <= front.LINE_BYTES for line in lines))
        # A worker that ends inside the call: its exit status, how far the request got.
        models, side, _, port = setup(self)
        sock, load, request = fk.post_request(port, "fake-exit", models.identity["fake-exit"])
        answer, frames, document, lines = fk.answer_of(sock, load, request)
        self.assertEqual(answer.status, 200)
        self.assertEqual([m["type"] for m in frames], ["hello", "result"])
        self.assertEqual(
            (document["outcome"], document["kind"], document["stage"], document["ended"], document["lines"]),
            ("failed", "unavailable", "unresolved", {"exited": 4}, len(lines)),
        )

    def test_the_record_is_read_strictly(self):
        _, document, lines = front.outcome("failed", "unavailable", admission="busy", lines=[b"one", b"x" * 5000])
        data = front.record_bytes(document, lines)
        self.assertEqual(front.read_record(io.BytesIO(data)), (document, [b"one", b"x" * front.LINE_BYTES]))
        for cut in (data[:3], data[:-1], data + b"x", (front.RECORD_MOST + 1).to_bytes(4, "big")):
            with self.assertRaises(ValueError):
                front.read_record(io.BytesIO(cut))


class Supervision(unittest.TestCase):
    def test_a_fifth_request_is_refused_busy(self):
        models, side, made, port = setup(self)
        held = []
        first = fk.dispatch()
        held.append(fk.post_request(port, "fake-wait", models.identity["fake-wait"], first))
        side.began(first)
        for _ in range(3):
            held.append(fk.post_request(port, "fake-ok", models.identity["fake-ok"]))
        fk.wait_until(lambda: len(made._queue) == front.QUEUE_MOST)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        answer, frames, document, _ = fk.answer_of(sock, load, request)
        self.assertEqual(answer.headers.get("whizbang-unverified"), "failure")
        self.assertEqual((frames, document["admission"], document["stage"]), ([], "busy", "not_sent"))
        self.assertEqual(fk.post_cancel(port, first), 204)
        for sock, load, request in held:
            fk.answer_of(sock, load, request)

    def test_a_request_for_another_model_unloads_the_first_and_loads_it(self):
        models, side, _, port = setup(self)
        for model in ("fake-ok", "fake-mps"):
            sock, load, request = fk.post_request(port, model, models.identity[model])
            self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        self.assertEqual(
            kinds(side),
            [("load", "fake-ok"), ("segment", "fake-ok"), ("unload", "fake-ok"), ("load", "fake-mps"), ("segment", "fake-mps")],
        )
        self.assertEqual(len({pid for pid, _, _, _ in side.journal}), 1)

    def test_an_ignored_cancel_restarts_the_worker_at_its_deadline(self):
        models, side, _, port = setup(self)
        dispatch = fk.dispatch()
        sock, load, request = fk.post_request(port, "fake-deaf", models.identity["fake-deaf"], dispatch)
        side.began(dispatch)
        began = time.monotonic()
        self.assertEqual(fk.post_cancel(port, dispatch), 204)
        _, _, document, _ = fk.answer_of(sock, load, request)
        self.assertGreaterEqual(time.monotonic() - began, 0.05)
        self.assertEqual((document["kind"], document["stage"], document["ended"]), ("cancelled", "unresolved", "killed"))
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        self.assertEqual(side.spawns.count, 2)

    def test_a_hung_load_is_ended_at_its_deadline_and_the_worker_restarted(self):
        limits = fk.small_limits(load=0.05)
        models, side, _, port = setup(self, limits, module="whizbang_pack.fakes", more=("--fake-fault", "slow-load"))
        for round in (1, 2):
            sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
            answer, frames, document, _ = fk.answer_of(sock, load, request)
            self.assertEqual([m["type"] for m in frames], ["hello", "event"])
            self.assertEqual((document["kind"], document["stage"], document["ended"]), ("timed_out", "unresolved", "killed"))
            self.assertEqual(side.spawns.count, round)

    def test_a_worker_that_dies_fails_the_request_in_flight_and_the_queue_and_replays_nothing(self):
        models, side, made, port = setup(self)
        first = fk.dispatch()
        running = fk.post_request(port, "fake-wait", models.identity["fake-wait"], first)
        side.began(first)
        queued = [fk.post_request(port, "fake-ok", models.identity["fake-ok"]) for _ in range(2)]
        fk.wait_until(lambda: len(made._queue) == 3)
        os.kill(side.spawns.pids[0], signal.SIGKILL)
        _, _, document, _ = fk.answer_of(*running)
        self.assertEqual((document["kind"], document["stage"], document["ended"]), ("unavailable", "unresolved", "signalled"))
        for sock, load, request in queued:
            answer, frames, document, _ = fk.answer_of(sock, load, request)
            self.assertEqual(answer.headers.get("whizbang-unverified"), "failure")
            self.assertEqual((frames, document["kind"], document["stage"]), ([], "unavailable", "not_sent"))
        self.assertEqual(kinds(side), [("load", "fake-wait"), ("segment", "fake-wait")])
        self.assertEqual(side.spawns.count, 1)

    def test_after_a_restart_the_next_request_is_answered(self):
        models, side, _, port = setup(self)
        sock, load, request = fk.post_request(port, "fake-exit", models.identity["fake-exit"])
        self.assertEqual(fk.answer_of(sock, load, request)[2]["ended"], {"exited": 4})
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        _, frames, document, _ = fk.answer_of(sock, load, request)
        self.assertEqual(document["outcome"], "complete")
        self.assertEqual(len(frames[-1]["instances"]), 1)
        self.assertEqual(side.spawns.count, 2)

    def test_a_changed_file_is_refused_at_the_next_load(self):
        models, side, _, port = setup(self)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        with open(models.file("fake-ok"), "ab") as changed:
            changed.write(b"!")
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], settings={"pad": 2})
        answer, frames, document, _ = fk.answer_of(sock, load, request)
        self.assertEqual(answer.headers.get("whizbang-unverified"), "failure")
        self.assertNotIn("whizbang-model-files", answer.headers)
        self.assertEqual((frames, document["kind"], document["stage"]), ([], "rejected", "not_sent"))
        self.assertEqual(kinds(side), [("load", "fake-ok"), ("segment", "fake-ok")])

    def test_two_clients_with_other_settings_each_get_a_load_with_their_own(self):
        models, side, _, port = setup(self)
        for settings in ({"pad": 1}, {"pad": 2}, {"pad": 2}):
            sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], settings=settings)
            self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        loads = [(kind, settings) for _, kind, _, settings in side.journal if kind in ("load", "unload")]
        self.assertEqual(loads, [("load", {"pad": 1}), ("unload", None), ("load", {"pad": 2}), ("load", {"pad": 2})])

    def test_after_a_restart_a_request_is_loaded_with_its_own_settings(self):
        models, side, _, port = setup(self)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], settings={"pad": 3})
        fk.answer_of(sock, load, request)
        kill(side)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], settings={"pad": 3})
        self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        second = [(kind, settings) for pid, kind, _, settings in side.journal if pid == side.spawns.pids[1]]
        self.assertEqual(second, [("load", {"pad": 3}), ("segment", None)])


    def test_three_workers_in_a_row_that_answered_nothing_pause_the_next_start(self):
        # As the local supervisor's `STARTS_FAILED_MOST` and `RESTART_PAUSE`: a worker that
        # never says hello is ended at the hello's limit; after the third, none is started.
        limits = fk.small_limits(hello=0.05)
        models, side, _, port = setup(self, limits, module="whizbang_pack.fakes", more=("--fake-fault", "no-hello"))
        for round in range(front.STARTS_FAILED_MOST + 1):
            sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
            answer, frames, document, _ = fk.answer_of(sock, load, request)
            self.assertEqual((frames, document["kind"], document["stage"]), ([], "timed_out" if round < 3 else "unavailable", "not_sent"))
        self.assertEqual(side.spawns.count, front.STARTS_FAILED_MOST)


class Bridge(unittest.TestCase):
    def test_a_request_the_bridge_has_begun_is_refused_when_it_comes_again(self):
        models, side, _, port = setup(self)
        dispatch = fk.dispatch()
        for expected in ("complete", "failed"):
            sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], dispatch)
            _, _, document, _ = fk.answer_of(sock, load, request)
            self.assertEqual(document["outcome"], expected)
        self.assertEqual((document["admission"], document["stage"]), ("replay", "not_sent"))
        self.assertEqual(kinds(side), [("load", "fake-ok"), ("segment", "fake-ok")])

    def test_two_clients_whose_pack_ids_are_the_same_have_other_dispatch_ids_and_both_run(self):
        models, side, _, port = setup(self)
        first = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        second = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        for sock, load, request in (first, second):
            self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        self.assertEqual([kind for kind, _ in kinds(side)].count("segment"), 2)

    def test_a_client_restarted_with_its_ids_counted_from_one_again_is_not_taken_for_a_replay(self):
        models, side, _, port = setup(self)
        for _ in range(2):
            sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], conversation="c1", load_id="r1", request_id="r2")
            self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")

    def test_of_two_deliveries_of_one_dispatch_at_once_one_runs_and_one_is_refused(self):
        # Two containers of the card's side, one claim store: the host's redelivery.
        models = fk.Models(self)
        claims = front.Claims()
        sides = [fk.local_side(self, models, claims=claims) for _ in range(2)]
        data, _, _ = fk.body("fake-ok", models.identity["fake-ok"])
        job = {"dispatch": fk.dispatch(), "not_after": time.time() + 60, "frames": data[len(proto.encode(fk.messages("fake-ok", models.identity["fake-ok"])[0])) :]}
        ends = []
        barrier = threading.Barrier(2)

        def deliver(side):
            barrier.wait(kit.WAIT_SECONDS)
            side.call(job, lambda item: ends.append(item[1]) if item[0] == "outcome" else None)

        threads = [threading.Thread(target=deliver, args=(side,), daemon=True) for side in sides]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(kit.WAIT_SECONDS)
        self.assertEqual(sorted(end["outcome"] for end in ends), ["complete", "failed"])
        refused = next(end for end in ends if end["outcome"] == "failed")
        self.assertEqual((refused["admission"], refused["stage"]), ("replay", "not_sent"))
        self.assertEqual(sum(len(side.journal) for side in sides), 2)

    def test_a_request_past_its_not_after_is_refused_whatever_the_claim_store_holds(self):
        models = fk.Models(self)

        class Counting(front.Claims):
            asked = 0

            def claim(self, dispatch):
                Counting.asked += 1
                return True

        side = fk.local_side(self, models, claims=Counting())
        data, _, _ = fk.body("fake-ok", models.identity["fake-ok"])
        frames = data[len(proto.encode(fk.messages("fake-ok", models.identity["fake-ok"])[0])) :]
        ends = []
        side.call({"dispatch": fk.dispatch(), "not_after": time.time() - 1, "frames": frames}, ends.append)
        self.assertEqual(len(ends), 1)
        self.assertEqual((ends[0][1]["admission"], ends[0][1]["kind"]), ("not_after", "timed_out"))
        self.assertEqual((Counting.asked, side.spawns.count), (0, 0))
        # And through the front, whose stamp is the receipt plus the host's limit.
        made, port = fk.serve(self, side, models.dir, clock=lambda: time.time() - 120, host_limit_s=60)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        self.assertEqual(fk.answer_of(sock, load, request)[2]["admission"], "not_after")

    def test_a_cancel_that_comes_during_the_claim_ends_the_request_cancelled_not_sent_with_no_model_work(self):
        # The claim is a round trip on a host's store: a cancel made while it is out is kept.
        models = fk.Models(self)
        inside, release = threading.Event(), threading.Event()
        self.addCleanup(release.set)

        class Slow(front.Claims):
            def claim(self, dispatch):
                inside.set()
                release.wait(kit.WAIT_SECONDS)
                return super().claim(dispatch)

        side = fk.Recording(fk.local_side(self, models, claims=Slow()))
        _, port = fk.serve(self, side, models.dir)
        dispatch = fk.dispatch()
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], dispatch)
        self.assertTrue(inside.wait(kit.WAIT_SECONDS))
        self.assertEqual(fk.post_cancel(port, dispatch), 204)
        release.set()
        answer, frames, document, _ = fk.answer_of(sock, load, request)
        self.assertEqual(answer.headers.get("whizbang-unverified"), "failure")
        self.assertEqual((frames, document["kind"], document["stage"]), ([], "cancelled", "not_sent"))
        self.assertEqual((list(side.journal), side.spawns.count), ([], 0))
        # And a cancel that reaches the worker's side before its call does, as a control
        # input that overtook the call's input on a host, is found when the call begins.
        early = fk.dispatch()
        self.assertFalse(side.cancel(early))
        data, _, _ = fk.body("fake-ok", models.identity["fake-ok"])
        frames_only = data[len(proto.encode(fk.messages("fake-ok", models.identity["fake-ok"])[0])) :]
        ends = []
        side.side.call({"dispatch": early, "not_after": time.time() + 60, "frames": frames_only}, ends.append)
        self.assertEqual((ends[-1][1]["kind"], ends[-1][1]["stage"]), ("cancelled", "not_sent"))
        self.assertEqual((list(side.journal), side.spawns.count), ([], 0))

    def test_an_unreachable_claim_store_refuses_the_request_before_any_model_work(self):
        models = fk.Models(self)

        class Away(front.Claims):
            def claim(self, dispatch):
                raise front.Unreachable()

        side = fk.local_side(self, models, claims=Away())
        _, port = fk.serve(self, side, models.dir)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        answer, frames, document, _ = fk.answer_of(sock, load, request)
        self.assertEqual(answer.headers.get("whizbang-unverified"), "failure")
        self.assertEqual((frames, document["admission"], document["stage"]), ([], "claim_unreachable", "not_sent"))
        self.assertEqual(side.spawns.count, 0)

    def test_a_cancel_reaches_a_call_blocked_in_the_bridge_through_its_control_input_and_an_ignored_one_restarts_the_worker_at_its_deadline(self):
        models = fk.Models(self)
        side = fk.Recording(fk.local_side(self, models))
        for model, ended, stage in (("fake-wait", None, "answered"), ("fake-deaf", "killed", "unresolved")):
            data, _, _ = fk.body(model, models.identity[model])
            frames = data[len(proto.encode(fk.messages(model, models.identity[model])[0])) :]
            dispatch = fk.dispatch()
            thread = threading.Thread(target=side.call, args=({"dispatch": dispatch, "not_after": time.time() + 60, "frames": frames}, lambda item: None), daemon=True)
            thread.start()
            side.began(dispatch)
            fk.wait_until(lambda: any(i[0] == "frame" and b'"event"' in i[1] for i in side.items[dispatch]))
            self.assertFalse(side.cancel(fk.dispatch()))
            self.assertTrue(side.cancel(dispatch))
            document = side.outcome(dispatch)
            thread.join(kit.WAIT_SECONDS)
            self.assertEqual((document["kind"], document["stage"], document["ended"]), ("cancelled", stage, ended))
        self.assertEqual(side.spawns.count, 1)

    def test_a_new_front_while_the_old_ones_call_is_blocked_has_its_call_refused_busy_its_cancel_reach_the_old_call_by_dispatch_id_and_the_next_call_answered(self):
        models = fk.Models(self)
        side = fk.Recording(fk.local_side(self, models))
        _, old_port = fk.serve(self, side, models.dir)
        _, new_port = fk.serve(self, side, models.dir)
        blocked = fk.dispatch()
        old = fk.post_request(old_port, "fake-wait", models.identity["fake-wait"], blocked)
        side.began(blocked)
        sock, load, request = fk.post_request(new_port, "fake-ok", models.identity["fake-ok"])
        _, frames, document, _ = fk.answer_of(sock, load, request)
        self.assertEqual((frames, document["admission"]), ([], "busy"))
        self.assertEqual(fk.post_cancel(new_port, blocked), 204)
        self.assertEqual(fk.answer_of(*old)[2]["kind"], "cancelled")
        sock, load, request = fk.post_request(new_port, "fake-ok", models.identity["fake-ok"])
        self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        self.assertEqual(side.spawns.count, 1)

    def test_the_cancel_route_of_a_replaced_front_reaches_the_old_call_and_a_late_one_starts_no_model_work(self):
        models = fk.Models(self)
        side = fk.Recording(fk.local_side(self, models))
        old_front, old_port = fk.serve(self, side, models.dir)
        _, new_port = fk.serve(self, side, models.dir)
        blocked = fk.dispatch()
        old = fk.post_request(old_port, "fake-wait", models.identity["fake-wait"], blocked)
        side.began(blocked)
        old_front.close()
        self.assertEqual(fk.post_cancel(new_port, blocked), 204)
        self.assertEqual(side.outcome(blocked)["kind"], "cancelled")
        old[0].close()
        journal = list(side.journal)
        self.assertEqual(fk.post_cancel(new_port, blocked), 204)
        self.assertEqual((list(side.journal), side.spawns.count), (journal, 1))

    def test_a_gpu_worker_replaced_under_a_front_that_stays_loads_again_for_the_next_request_and_replays_nothing(self):
        models, side, _, port = setup(self)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        fk.answer_of(sock, load, request)
        kill(side)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        first, second = side.spawns.pids
        self.assertEqual(
            [(pid, kind) for pid, kind, _, _ in side.journal],
            [(first, "load"), (first, "segment"), (second, "load"), (second, "segment")],
        )

    def test_a_worker_side_named_by_module_and_function_is_made_with_the_models_directory(self):
        made = front.side_of("whizbang_pack.front:local", "/models", {})
        self.assertIsInstance(made, front.LocalSide)
        self.assertEqual(made.models_dir, "/models")
        for wrong in ("whizbang_pack.front", ":x", "whizbang_pack.front:FORMAT"):
            with self.assertRaises(front.Usage):
                front.side_of(wrong, "/models", {})


class Cancel(unittest.TestCase):
    def test_with_two_clients_using_r1_a_cancel_of_the_queued_one_leaves_the_running_one(self):
        models, side, made, port = setup(self)
        running, queued = fk.dispatch(), fk.dispatch()
        first = fk.post_request(port, "fake-wait", models.identity["fake-wait"], running, load_id="r1", request_id="r2")
        side.began(running)
        second = fk.post_request(port, "fake-ok", models.identity["fake-ok"], queued, load_id="r1", request_id="r2")
        fk.wait_until(lambda: queued in made._by_dispatch)
        self.assertEqual(fk.post_cancel(port, queued), 204)
        answer, frames, document, _ = fk.answer_of(*second)
        self.assertEqual((frames, document["kind"], document["stage"]), ([], "cancelled", "not_sent"))
        self.assertEqual(side.side._running, running)
        self.assertEqual(fk.post_cancel(port, running), 204)
        self.assertEqual(fk.answer_of(*first)[2]["kind"], "cancelled")
        self.assertEqual([kind for kind, _ in kinds(side)].count("segment"), 1)

    def test_a_late_cancel_after_the_call_ended_or_after_its_worker_was_replaced_touches_nothing(self):
        models, side, _, port = setup(self)
        done = fk.dispatch()
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], done)
        fk.answer_of(sock, load, request)
        journal = list(side.journal)
        self.assertEqual(fk.post_cancel(port, done), 204)
        self.assertEqual((list(side.journal), side.spawns.count), (journal, 1))
        kill(side)
        self.assertEqual(fk.post_cancel(port, done), 204)
        self.assertEqual((list(side.journal), side.spawns.count), (journal, 1))
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")


class Tls(unittest.TestCase):
    def test_the_front_serves_https_with_the_cert_it_is_given(self):
        server, client = fk.tls(self)
        models = fk.Models(self)
        side = fk.local_side(self, models)
        _, port = fk.serve(self, side, models.dir, tls=server)
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"], tls=client)
        self.assertEqual(fk.answer_of(sock, load, request)[2]["outcome"], "complete")
        self.assertEqual(fk.get(port, tls=client)[0], 200)
        # A client that does not trust the cert is refused at the handshake.
        with self.assertRaises(ssl.SSLError):
            fk.connect(port, ssl.create_default_context())


def closed(sock):
    """Reads until the front closes the connection: how long that took."""
    sock.settimeout(kit.WAIT_SECONDS)
    began = time.monotonic()
    try:
        while sock.recv(65536):
            pass
    except (ConnectionError, OSError):
        pass
    return time.monotonic() - began


class Deadlines(unittest.TestCase):
    def test_incomplete_headers_a_trickling_body_a_stalled_handshake_and_a_client_that_reads_nothing_are_each_closed(self):
        limits = fk.small_limits(classify=0.05, body_grace=0.05, body_rate=1000, write=0.1)
        models, side, made, port = setup(self, front_limits=limits)
        sock = fk.connect(port)
        sock.sendall(b"POST /pack/1/request HTTP/1.1\r\nHost: front")
        self.assertLess(closed(sock), 2.0)
        sock.close()
        # A body that keeps coming, a byte every 10 ms, under the 1,000 bytes a second its
        # front asks for after the first 50 ms: answered 400 and closed long before its end.
        data, _, _ = fk.body("fake-ok", models.identity["fake-ok"])
        sock = fk.connect(port)
        sock.sendall(fk.head("/pack/1/request", fk.dispatch(), len(data)))
        sent = [0]
        stop = threading.Event()

        def trickle():
            for byte in data[:300]:
                if stop.wait(0.01):
                    return
                try:
                    sock.sendall(bytes((byte,)))
                except OSError:
                    return
                sent[0] += 1

        trickler = threading.Thread(target=trickle, daemon=True)
        began = time.monotonic()
        trickler.start()
        answer = fk.Answer(sock)
        self.assertEqual(answer.status, 400)
        self.assertLess(time.monotonic() - began, 2.0)
        stop.set()
        trickler.join(kit.WAIT_SECONDS)
        self.assertGreater(sent[0], 0)
        self.assertLess(sent[0], 300)
        answer.close()
        # A handshake that never comes, on a front that speaks TLS.
        server, _ = fk.tls(self)
        _, tls_port = fk.serve(self, side, models.dir, limits, tls=server)
        sock = fk.connect(tls_port)
        self.assertLess(closed(sock), 2.0)
        sock.close()
        # A client that reads nothing of a big answer: closed at the write's limit.
        width = height = 256
        dispatch = fk.dispatch()
        points = [{"point": [float(n), 1.0]} for n in range(64)]
        data, load, request = fk.body("fake-ok", models.identity["fake-ok"], width=width, height=height, image=bytes(4 * width * height), queries=points)
        sock = fk.connect(port, buffer=4096)
        sock.sendall(fk.head("/pack/1/request", dispatch, len(data)) + data)
        self.assertEqual(side.outcome(dispatch)["outcome"], "complete")
        fk.wait_until(lambda: made._request_free._value == front.REQUEST_HANDLERS)
        self.assertLess(closed(sock), kit.WAIT_SECONDS)
        sock.close()

    def test_with_every_request_handler_taken_and_sixty_four_connections_stalled_in_their_handshake_or_headers_a_cancel_made_after_them_is_answered_within_2_seconds_of_its_acceptance(self):
        # On a front that speaks TLS: the six request handlers taken (one request with the
        # worker, three queued, two whose bodies stall), then 64 connections, half stalled
        # in their handshake and half in their headers after it.
        server, client = fk.tls(self)
        models = fk.Models(self)
        side = fk.Recording(fk.local_side(self, models))
        made, port = fk.serve(self, side, models.dir, tls=server)
        held = []
        running = fk.dispatch()
        held.append(fk.post_request(port, "fake-wait", models.identity["fake-wait"], running, tls=client))
        side.began(running)
        for _ in range(3):
            held.append(fk.post_request(port, "fake-ok", models.identity["fake-ok"], tls=client))
        stalled_bodies = []
        for _ in range(2):
            sock = fk.connect(port, client)
            sock.sendall(fk.head("/pack/1/request", fk.dispatch(), 1000))
            stalled_bodies.append(sock)
        fk.wait_until(lambda: made._request_free._value == 0)
        stalled = []
        for n in range(front.WAITING_MOST):
            if n % 2:
                sock = fk.connect(port, client)
                sock.sendall(b"POST /pack/1/cancel HTTP/1.1\r\n")
            else:
                sock = fk.connect(port)
            stalled.append(sock)
        began = time.monotonic()
        self.assertEqual(fk.post_cancel(port, running, tls=client), 204)
        self.assertLess(time.monotonic() - began, 2.0)
        self.assertEqual(side.outcome(running)["kind"], "cancelled")
        for sock in stalled + stalled_bodies:
            sock.close()
        for sock, load, request in held:
            sock.close()

    def test_a_ready_connection_is_read_while_others_are_stalled(self):
        models, side, made, port = setup(self)
        stalled = [fk.connect(port) for _ in range(10)]
        for sock in stalled[:5]:
            sock.sendall(b"GET /pack/1/hea")
        began = time.monotonic()
        self.assertEqual(fk.get(port)[0], 200)
        self.assertLess(time.monotonic() - began, 1.0)
        for sock in stalled:
            sock.close()

    def test_two_cancels_stalled_in_their_bodies_leave_a_whole_third_cancel_to_reach_the_worker_within_2_seconds(self):
        models, side, made, port = setup(self)
        running = fk.dispatch()
        held = fk.post_request(port, "fake-wait", models.identity["fake-wait"], running)
        side.began(running)
        data = fk.cancel_body()
        stalled = []
        for _ in range(2):
            sock = fk.connect(port)
            sock.sendall(fk.head("/pack/1/cancel", running, len(data)) + data[:5])
            stalled.append(sock)
        began = time.monotonic()
        self.assertEqual(fk.post_cancel(port, running), 204)
        self.assertEqual(side.outcome(running)["kind"], "cancelled")
        self.assertLess(time.monotonic() - began, 2.0)
        for sock in stalled:
            sock.close()
        held[0].close()


class Start(unittest.TestCase):
    def test_the_headers_follow_the_check_of_the_files_and_a_failed_check_is_an_error_answer_with_no_frame(self):
        models, side, _, port = setup(self)
        gate = threading.Event()
        checking = threading.Event()
        real = side.side.check

        def held(*given):
            checking.set()
            gate.wait(kit.WAIT_SECONDS)
            return real(*given)

        side.side.check = held
        sock, load, request = fk.post_request(port, "fake-ok", models.identity["fake-ok"])
        self.assertTrue(checking.wait(kit.WAIT_SECONDS))
        sock.settimeout(0.05)
        with self.assertRaises(socket.timeout):
            sock.recv(1)
        sock.settimeout(kit.WAIT_SECONDS)
        gate.set()
        answer, frames, document, _ = fk.answer_of(sock, load, request)
        self.assertIn("whizbang-model-files", answer.headers)
        self.assertEqual(document["outcome"], "complete")
        side.side.check = real
        os.remove(models.file("fake-mps"))
        sock, load, request = fk.post_request(port, "fake-mps", models.identity["fake-mps"])
        answer, frames, document, lines = fk.answer_of(sock, load, request)
        self.assertEqual((answer.status, answer.headers.get("whizbang-unverified")), (200, "failure"))
        self.assertNotIn("whizbang-model-files", answer.headers)
        self.assertEqual((frames, document["kind"], document["stage"], lines), ([], "rejected", "not_sent", []))


class CommandLine(unittest.TestCase):
    def test_the_front_takes_its_options_once_and_a_token_of_32_printable_characters(self):
        given = front.parse(["--listen", "127.0.0.1:8080", "--models-dir", "/models", "--token-env", "T", "--redact-env", "A", "--redact-env", "B"])
        self.assertEqual((given["listen"], given["worker"], given["host_limit_s"], given["redact_env"]), (("127.0.0.1", 8080), "local", 600, ["A", "B"]))
        for wrong in (
            ["--listen", "127.0.0.1:8080", "--models-dir", "/models"],
            ["--listen", "8080", "--models-dir", "/models", "--token-env", "T"],
            ["--listen", "127.0.0.1:8080", "--models-dir", "models", "--token-env", "T"],
            ["--listen", "127.0.0.1:8080", "--models-dir", "/models", "--token-env", "T", "--token-env", "U"],
            ["--listen", "127.0.0.1:8080", "--models-dir", "/models", "--token-env", "T", "--tls-cert", "c"],
            ["--listen", "127.0.0.1:8080", "--models-dir", "/models", "--token-env", "T", "--host-limit-s", "0"],
            ["--listen", "127.0.0.1:8080", "--models-dir", "/models", "--token-env", "T", "--other", "x"],
        ):
            with self.assertRaises(front.Usage):
                front.parse(wrong)
        self.assertEqual(front.token_of({"T": fk.TOKEN}, "T"), fk.TOKEN)
        for value in (None, "x" * 31, "x" * 31 + " ", "x" * 31 + "\u00e9"):
            with self.assertRaises(front.Usage):
                front.token_of({} if value is None else {"T": value}, "T")


class Framing(unittest.TestCase):
    def test_a_frame_header_over_its_bound_inside_a_permitted_body_is_refused_with_the_rest_unread(self):
        models, side, _, port = setup(self)
        hello = proto.encode({"type": "hello", "id": "c1", "version": proto.VERSION, "role": "whizbang"})
        sock = fk.connect(port)
        # A body of 10 MB is permitted; its second frame's header says 1 MiB, over 64 KiB.
        sock.sendall(fk.head("/pack/1/request", fk.dispatch(), 10 << 20) + hello + (1 << 20).to_bytes(4, "big"))
        began = time.monotonic()
        self.assertEqual(fk.Answer(sock).status, 400)
        self.assertLess(time.monotonic() - began, 1.0)
        sock.close()
        self.assertEqual(side.spawns.count, 0)

    def test_a_payload_length_that_disagrees_with_its_header_is_refused_before_its_samples_are_read(self):
        models, side, _, port = setup(self)
        hello, load, request = fk.messages("fake-ok", models.identity["fake-ok"])
        chunks = proto.chunks(request)
        stated = int.from_bytes(chunks[1], "big")
        head = proto.encode(hello) + proto.encode(load) + chunks[0] + (stated + 1).to_bytes(8, "big")
        sock = fk.connect(port)
        sock.sendall(fk.head("/pack/1/request", fk.dispatch(), len(head) + stated + 1) + head)
        began = time.monotonic()
        self.assertEqual(fk.Answer(sock).status, 400)
        self.assertLess(time.monotonic() - began, 1.0)
        sock.close()
        self.assertEqual(side.spawns.count, 0)


if __name__ == "__main__":
    unittest.main()
