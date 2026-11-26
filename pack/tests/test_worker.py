"""The worker and its fake models (docs/plans/m1c-local-pack.md, P2).

Most tests drive the worker in this process, over two pipes, with an exit that is recorded
in place of `os._exit`. Four tests start a child process, one at a time and four times in
all (three in `Ends`, one in `Fakes`), each its entry point's module run as a program, each
killed and waited for in the test's cleanup, with its environment cleared to the
variables `scripts/test-pack.sh` hands on.
"""

import ast
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import types
import unittest

import kit
from whizbang_pack import adapter, conformance, fake_models, fakes, installed, proto, worker

MODELS = "/nowhere/models"
INSTALLATION = "sha256:" + "77f358414083a02e8d99d0d097f53bb7c63890ce465acbb742020e63961699fd"
OTHER_INSTALLATION = "sha256:" + "11" * 32
BINDING = "sha256:" + "92b2fa58028958317e408bd84ecfa70f5ee35b121991dbc232c49d166353708b"
WIDTH, HEIGHT = 8, 6


def picture():
    """8 by 6: red 255 in the left half and 0 in the right, so fake-ok's phrase finds the
    left half."""
    samples = bytearray()
    for _y in range(HEIGHT):
        for x in range(WIDTH):
            samples += bytes((255 if x < WIDTH // 2 else 0, 10, 20, 255))
    return bytes(samples)


def load(id, model="fake-ok", installation=INSTALLATION, settings=None):
    return {
        "type": "request",
        "id": id,
        "kind": "load",
        "model": model,
        "installation": installation,
        "settings": {} if settings is None else settings,
    }


def unload(id, model="fake-ok", installation=INSTALLATION):
    return {"type": "request", "id": id, "kind": "unload", "model": model, "installation": installation}


def segment(id, model="fake-ok", queries=None, most=2, installation=INSTALLATION):
    return {
        "type": "request",
        "id": id,
        "kind": "segment",
        "model": model,
        "installation": installation,
        "image": proto.Payload(WIDTH, HEIGHT, BINDING, picture()),
        "queries": queries or [{"phrase": "a red thing"}, {"point": [5.5, 2.25]}, {"box": [1.0, 1.0, 3.0, 4.0]}],
        "max_instances": most,
    }


def hello(id="h1", version=proto.VERSION):
    header = '{"type":"hello","id":"%s","version":"%s","role":"whizbang"}' % (id, version)
    return len(header).to_bytes(4, "big") + header.encode("ascii")


class Running:
    """A worker in this process, over two pipes, whose exit is recorded."""

    def __init__(self, test, module=worker, options=(), lifetime=None, adapters=None, error_output=None, first=b"", models=MODELS):
        to_worker_read, self.to_worker = os.pipe()
        self.from_worker, from_worker_write = os.pipe()
        self.exits = []
        self.error_output = _Lines() if error_output is None else error_output
        self.made = module.make(
            ["--models-dir", models] + list(options),
            stdin=open(to_worker_read, "rb", buffering=0),
            stdout=open(from_worker_write, "wb", buffering=0),
            stderr=self.error_output,
            exit=self.exits.append,
            adapters=adapters,
        )
        if lifetime is not None:
            self.made.lifetime = lifetime
        self.reader = proto.Reader(kit.FdStream(self.from_worker), {})
        test.addCleanup(self.close)
        # What is sent before the worker starts waits in the pipe for its reader.
        self.send(first)
        self.made.start()

    def send(self, frame):
        view = memoryview(frame)
        while view:
            view = view[os.write(self.to_worker, view) :]

    def ask(self, request):
        self.reader.open[request["id"]] = proto.awaiting(request)
        self.send(proto.encode(request))

    def read(self):
        return self.reader.read()

    def hello(self):
        self.send(hello())
        return self.read()

    def end_input(self):
        if self.to_worker is not None:
            os.close(self.to_worker)
            self.to_worker = None

    def ended(self):
        """The code the worker ended with, once its two threads are gone."""
        self.join()
        return self.exits

    def rest(self):
        """What the worker wrote after what was read, once it has ended: `None` for nothing."""
        self.join()
        self.made.stdout.close()
        return self.read()

    def join(self):
        for thread in self.made.threads:
            thread.join(kit.WAIT_SECONDS)
            assert not thread.is_alive(), "a thread of the worker did not end: %s" % thread.name

    def close(self):
        self.end_input()
        if self.made.code is None:
            # Only a worker deaf to the end of its input is still running here; in a
            # process its lifetime would end it.
            self.made.stop(0)
        self.join()
        self.made.stdin.close()
        self.made.stdout.close()
        os.close(self.from_worker)


class _Lines:
    """An error output that keeps what is written to it."""

    def __init__(self):
        self.parts = []
        self.lock = threading.Lock()

    def write(self, text):
        with self.lock:
            self.parts.append(text)
        return len(text)

    def flush(self):
        pass

    def lines(self):
        with self.lock:
            return "".join(self.parts).splitlines()


class Hello(unittest.TestCase):
    def test_another_version_is_refused_and_the_worker_exits(self):
        running = Running(self)
        running.send(hello(version="pack/2"))
        error = running.read()
        self.assertEqual(error["type"], "error")
        self.assertEqual(error["id"], "h1", "the error names the conversation")
        self.assertEqual(error["fault"], "malformed")
        self.assertIn("pack 2", error["sentence"])
        self.assertIn("pack 1", error["sentence"])
        self.assertEqual(running.ended(), [2])

    def test_a_hello_is_answered_with_the_workers_own_of_the_same_id(self):
        running = Running(self)
        answer = running.hello()
        self.assertEqual(answer["type"], "hello")
        self.assertEqual(answer["id"], "h1")
        self.assertEqual(answer["version"], "pack/1")
        self.assertEqual(answer["role"], "worker")
        # The device its work runs on before a model names its own: the pack's core
        # imports no runtime, so the CPU (plan 1C, task P7a).
        self.assertEqual(answer["device"], "cpu")
        self.assertGreaterEqual(answer["threads"], 3)

    def test_a_first_frame_that_is_no_hello_and_a_frame_that_is_refused_each_end_the_conversation(self):
        running = Running(self)
        running.ask(load("r1"))
        error = running.read()
        self.assertEqual((error["type"], error["id"], error["fault"]), ("error", "r1", "malformed"))
        self.assertEqual(running.ended(), [2])

        running = Running(self)
        running.hello()
        running.send(len(b"[]").to_bytes(4, "big") + b"[]")
        error = running.read()
        self.assertEqual((error["type"], error["id"], error["fault"]), ("error", "h1", "malformed"))
        self.assertEqual(running.ended(), [2])

        running = Running(self)
        running.hello()
        running.send((proto.HEADER_LIMIT + 1).to_bytes(4, "big"))
        error = running.read()
        self.assertEqual((error["id"], error["fault"]), ("h1", "too_large"))
        self.assertEqual(running.ended(), [2])

    def test_an_error_that_names_the_conversation_ends_it(self):
        running = Running(self)
        running.hello()
        running.send(proto.encode({"type": "error", "id": "h1", "fault": "cancelled", "sentence": "goodbye"}))
        self.assertEqual(running.ended(), [0])


class Calls(unittest.TestCase):
    def test_one_call_runs_at_a_time_and_a_second_model_unloads_the_first(self):
        running = Running(self)
        running.hello()
        running.ask(load("r1"))
        loaded = running.read()
        self.assertEqual((loaded["type"], loaded["id"], loaded["kind"], loaded["device"]), ("result", "r1", "load", "cpu"))
        first = running.made.resident.adapter
        self.assertTrue(first.loaded)

        running.ask(load("r2", model="fake-wait"))
        self.assertEqual(running.read()["type"], "result")
        self.assertFalse(first.loaded, "the first model was unloaded")
        self.assertEqual(running.made.resident.model, "fake-wait")
        running.ask(segment("r3", model="fake-ok"))
        error = running.read()
        self.assertEqual((error["id"], error["fault"], error["sentence"]), ("r3", "unavailable", "the model is not loaded"))

        running.ask(segment("r4", model="fake-wait", queries=[{"phrase": "anything"}]))
        self.assertEqual(running.read(), {"type": "event", "id": "r4", "text": "waiting for a cancel"})
        running.ask(load("r5"))
        busy = running.read()
        self.assertEqual((busy["type"], busy["id"], busy["fault"]), ("error", "r5", "unavailable"))
        self.assertEqual(busy["sentence"], "another call is running")
        running.send(proto.encode({"type": "cancel", "id": "r4"}))
        cancelled = running.read()
        self.assertEqual((cancelled["id"], cancelled["fault"]), ("r4", "cancelled"))

        running.ask(load("r6"))
        self.assertEqual(running.read()["type"], "result", "one call ended, the next is taken")
        self.assertEqual(running.made.resident.model, "fake-ok")

    def test_a_load_opens_the_installation_by_its_model_and_identity(self):
        running = Running(self)
        running.hello()
        running.ask(load("r1", settings={"outline": "whole", "pad": -5, "strict": True}))
        result = running.read()
        self.assertEqual(result["diagnostics"], "loaded with 3 settings")
        installation = running.made.resident.installation
        self.assertEqual(
            installation.folder,
            os.path.join(MODELS, "models", "fake-ok", "installations", INSTALLATION[len("sha256:") :]),
        )
        self.assertEqual(running.made.resident.adapter.settings, {"outline": "whole", "pad": -5, "strict": True})
        # The same model of another installation is another load, and unloads the first.
        running.ask(load("r2", installation=OTHER_INSTALLATION))
        self.assertIn("unloaded fake-ok", running.read()["diagnostics"])
        running.ask(load("r3", installation=OTHER_INSTALLATION))
        self.assertEqual(running.read()["diagnostics"], "already loaded")
        running.ask(unload("r4", installation=OTHER_INSTALLATION))
        self.assertEqual(running.read()["kind"], "unload")
        self.assertIsNone(running.made.resident)
        running.ask(load("r5", model="no-such-model"))
        self.assertEqual(running.read()["fault"], "rejected")

    def test_fake_ok_answers_the_masks_its_request_determines(self):
        running = Running(self)
        running.hello()
        running.ask(load("r1"))
        running.read()
        running.ask(segment("r2"))
        result = running.read()
        self.assertEqual(result["kind"], "segment")
        found = {item["query"]: item for item in result["instances"]}
        self.assertEqual(sorted(found), [0, 1, 2])
        # The phrase: every pixel whose red is 128 or more, the left half.
        self.assertEqual(found[0]["plane"].samples, bytes(([255] * 4 + [0] * 4) * HEIGHT))
        self.assertEqual(found[0]["box"], [0.0, 0.0, 4.0, 6.0])
        self.assertIsNone(found[0]["iou"])
        # The point: the one pixel it is in.
        point = bytearray(WIDTH * HEIGHT)
        point[2 * WIDTH + 5] = 255
        self.assertEqual(found[1]["plane"].samples, bytes(point))
        self.assertEqual(found[1]["box"], [5.0, 2.0, 6.0, 3.0])
        # The box: every pixel whose centre is inside it.
        box = bytearray(WIDTH * HEIGHT)
        for y in (1, 2, 3):
            for x in (1, 2):
                box[y * WIDTH + x] = 255
        self.assertEqual(found[2]["plane"].samples, bytes(box))
        self.assertEqual(found[2]["box"], [1.0, 1.0, 3.0, 4.0])
        self.assertEqual([item["query"] for item in result["instances"]], [2, 1, 0], "best score first")

    def test_events_and_diagnostics_are_held_to_their_bounds_and_cleaned(self):
        class Talkative(adapter.Adapter):
            features = frozenset(("phrase",))

            def load(self, installation, settings, call):
                for number in range(300):
                    call.event("event %d at /tmp/x" % number + "e" * 400)
                return "d" * 70_000

        running = Running(self, adapters={"talkative": Talkative})
        running.hello()
        running.ask(load("r1", model="talkative"))
        events = 0
        while True:
            message = running.read()
            if message["type"] != "event":
                break
            self.assertNotIn("/", message["text"])
            events += 1
        # The reader of the test holds the worker to the bounds a reader takes: one more
        # event, or one more byte of text, would have been refused before this.
        self.assertEqual(message["type"], "result")
        self.assertLessEqual(events, proto.EVENTS_LIMIT)
        self.assertGreater(events, 100)
        self.assertGreater(len(message["diagnostics"]), 0)

    def test_a_query_the_model_does_not_take_is_unsupported_and_fake_big_and_fake_bad_break_their_frames(self):
        running = Running(self)
        running.hello()
        running.ask(load("r1", model="fake-wait"))
        running.read()
        running.ask(segment("r2", model="fake-wait"))
        self.assertEqual(running.read()["fault"], "unsupported", "fake-wait takes no point or box")

        running.ask(load("r3", model="fake-big"))
        running.read()
        running.ask(segment("r4", model="fake-big", most=2))
        with self.assertRaises(proto.Refused) as caught:
            running.read()
        self.assertEqual(caught.exception.reason, "over_bound")

        running = Running(self)
        running.hello()
        running.ask(load("r1", model="fake-bad"))
        running.read()
        running.ask(segment("r2", model="fake-bad"))
        with self.assertRaises(proto.Refused) as caught:
            running.read()
        self.assertEqual(caught.exception.reason, "wrong_length")

    def test_fake_exit_ends_the_process_inside_a_call(self):
        running = Running(self)
        running.hello()
        running.ask(load("r1", model="fake-exit"))
        running.read()
        running.ask(segment("r2", model="fake-exit"))
        running.made.threads[1].join(kit.WAIT_SECONDS)
        self.assertEqual(running.exits, [fakes_exit_code()], "the call ended the process")
        # In a process of its own nothing outlives the exit; here the reader is let go by
        # the end of its input, which ends nothing more.
        running.end_input()
        self.assertEqual(running.ended(), [fakes_exit_code()])
        self.assertIsNone(running.rest(), "the call wrote no answer")


def fakes_exit_code():
    from whizbang_pack import fake_models

    return fake_models.EXIT_CODE


class Cancel(unittest.TestCase):
    def test_a_cancel_ends_a_waiting_call_as_cancelled(self):
        running = Running(self)
        running.hello()
        running.ask(load("r1", model="fake-wait"))
        running.read()
        running.ask(segment("r2", model="fake-wait", queries=[{"phrase": "anything"}]))
        self.assertEqual(running.read()["type"], "event", "the call is waiting")
        running.send(proto.encode({"type": "cancel", "id": "r9"}))
        running.send(proto.encode({"type": "cancel", "id": "r2"}))
        error = running.read()
        self.assertEqual((error["type"], error["id"], error["fault"]), ("error", "r2", "cancelled"))
        # A cancel of a call that has ended is answered by nothing, and the worker goes on.
        running.ask(load("r3"))
        running.read()
        running.send(proto.encode({"type": "cancel", "id": "r3"}))
        running.ask(unload("r4"))
        self.assertEqual(running.read()["id"], "r4")


class Threads(unittest.TestCase):
    def test_the_worker_holds_three_threads_with_a_fake_model(self):
        before = set(threading.enumerate())
        running = Running(self)
        own = [thread for thread in threading.enumerate() if thread not in before]
        self.assertEqual(sorted(thread.name for thread in own), ["whizbang-pack-call", "whizbang-pack-reader"])
        self.assertTrue(all(thread.daemon for thread in own))
        self.assertEqual(sorted(own, key=id), sorted(running.made.threads, key=id))

        running.hello()
        running.ask(load("r1"))
        running.read()
        running.ask(segment("r2"))
        running.read()
        running.ask(load("r3", model="fake-wait"))
        running.read()
        running.ask(segment("r4", model="fake-wait", queries=[{"phrase": "anything"}]))
        running.read()
        running.send(proto.encode({"type": "cancel", "id": "r4"}))
        running.read()
        # No call started a thread: the worker's are its reader, its call and the one
        # that started it, three.
        now = [thread for thread in threading.enumerate() if thread not in before]
        self.assertEqual(sorted(now, key=id), sorted(own, key=id))
        self.assertEqual(len(now) + 1, 3)
        # In a process of its own it counts three (Ends checks it in a child).
        running.end_input()
        self.assertEqual(running.ended(), [0])

    def test_the_call_thread_runs_before_the_reader_can_answer_a_hello_sent_at_once(self):
        # A hello waits in the pipe before the worker starts, as whizbang sends it right
        # after starting the process. Between one start and the next, the reader, once
        # started, is given all the time it needs to answer it; the answer counts the
        # worker's threads alive then, and the main thread. Started reader first, it would
        # count two.
        counted = threading.Event()
        alive = []
        made = []

        class Paused(worker.Worker):
            def _started(self, thread):
                if thread.name == "whizbang-pack-reader":
                    counted.wait(kit.WAIT_SECONDS)

        def count():
            alive.append(sorted(thread.name for thread in made[0].threads if thread.is_alive()))
            counted.set()
            return 1 + len(alive[-1])

        def make(argv, **given):
            made.append(Paused(MODELS, None, count_threads=count, **given))
            return made[0]

        running = Running(self, module=types.SimpleNamespace(make=make), first=hello())
        self.assertTrue(counted.is_set(), "the reader answered the hello while the worker started")
        answer = running.read()
        self.assertEqual(alive[0], ["whizbang-pack-call", "whizbang-pack-reader"])
        self.assertEqual(answer["threads"], 3)
        # The list keeps its order: the reader first, then the call.
        self.assertEqual([thread.name for thread in running.made.threads], ["whizbang-pack-reader", "whizbang-pack-call"])
        running.end_input()
        self.assertEqual(running.ended(), [0])


class Fakes(unittest.TestCase):
    def test_the_worker_entry_point_refuses_a_fault_and_the_fakes_entry_point_takes_each(self):
        before = threading.active_count()
        for module, options in (
            (worker, ["--fake-fault", "flood"]),
            (fakes, ["--fake-fault", "no-such-fault"]),
            (fakes, ["--fake-fault", "deaf-to-end-of-input"]),
            (worker, ["--models-dir"]),
            (worker, ["--max-lifetime-s", "0"]),
            (worker, ["--max-lifetime-s", "1.5"]),
        ):
            with self.subTest(module=module.__name__, options=options):
                with self.assertRaises(worker.Usage):
                    module.make(["--models-dir", MODELS] + options, stdin=None, stdout=None, stderr=None, exit=None)
        with self.assertRaises(worker.Usage):
            worker.make(["--models-dir", "relative/models"], stdin=None, stdout=None, stderr=None, exit=None)
        self.assertEqual(threading.active_count(), before, "a refused command line starts nothing")
        self.assertEqual(
            sorted(fakes.FAULTS),
            sorted(
                (
                    "no-hello",
                    "other-version",
                    "deaf-to-end-of-input",
                    "error-output",
                    "flood",
                    "slow-load",
                    "slow-unload",
                    "grandchild",
                    "asks",
                    "deaf-after-hello",
                    "garbage-after-hello",
                )
            ),
        )

        def faulty(name, lifetime=None):
            options = ["--fake-fault", name]
            if name == "deaf-to-end-of-input":
                options += ["--max-lifetime-s", "120"]
            return Running(self, module=fakes, options=options, lifetime=lifetime)

        # no-hello: the hello is read and not answered.
        running = faulty("no-hello")
        running.send(hello())
        running.end_input()
        self.assertEqual(running.ended(), [0])
        self.assertIsNone(running.rest(), "nothing was written")

        # other-version: the hello states pack/2; an error it takes is a line of its error
        # output, and it ends the conversation.
        running = faulty("other-version")
        running.send(hello())
        with self.assertRaises(proto.Refused) as caught:
            running.read()
        self.assertEqual(caught.exception.reason, "version")
        running.send(proto.encode({"type": "error", "id": "h1", "fault": "unavailable", "sentence": "another version"}))
        self.assertEqual(running.ended(), [0])
        self.assertIn("took an error unavailable: another version", running.error_output.lines())

        # deaf-to-end-of-input: the reader ends at the end of the input and the worker
        # does not; its lifetime ends it.
        running = faulty("deaf-to-end-of-input", lifetime=0.2)
        self.assertEqual(running.made.lifetime, 0.2)
        running.hello()
        running.end_input()
        reader_thread = running.made.threads[0]
        reader_thread.join(kit.WAIT_SECONDS)
        self.assertFalse(reader_thread.is_alive())
        self.assertEqual(running.exits, [])
        self.assertEqual(running.made.wait(), worker.LIFETIME_CODE)
        self.assertEqual(running.ended(), [worker.LIFETIME_CODE])

        # error-output: more lines than a supervisor keeps, some longer than it keeps.
        running = faulty("error-output")
        running.hello()
        lines = running.error_output.lines()
        self.assertGreater(len(lines), 256)
        self.assertTrue(any(len(line) > 4096 for line in lines))
        # Then its environment and its descriptors, and last the marker, the host and the
        # made-up token that whizbang's tests look for, all within the last 256 lines.
        self.assertTrue(any(line.startswith("environment ") for line in lines[-256:]))
        self.assertTrue(any(line.startswith("descriptor 0 ") for line in lines[-256:]))
        self.assertEqual(
            lines[-3:],
            ["marker " + fakes.MARKER, "connecting to host " + fakes.HOST, "token " + fakes.TOKEN],
        )
        # At the start of a call, the same three lines again, after one that names it.
        running.ask(load("r1"))
        self.assertEqual(running.read()["type"], "result")
        self.assertEqual(
            running.error_output.lines()[-4:],
            ["in the call r1", "marker " + fakes.MARKER, "connecting to host " + fakes.HOST, "token " + fakes.TOKEN],
        )

        # asks: at the start of a call, a request of its own, of which the header and the
        # payload's length are written and none of the samples.
        running = faulty("asks")
        running.hello()
        running.ask(load("r1"))
        stream = kit.FdStream(running.from_worker)
        written = b""
        while len(written) < 4 or len(written) < 4 + int.from_bytes(written[:4], "big") + 8:
            chunk = stream.read(65536)
            self.assertTrue(chunk, "the output ended inside the request")
            written += chunk
        length = int.from_bytes(written[:4], "big")
        asked = json.loads(written[4 : 4 + length])
        self.assertEqual((asked["type"], asked["id"], asked["kind"]), ("request", fakes.ASKED_ID, "segment"))
        self.assertEqual(written[4 + length :], (4).to_bytes(8, "big"), "the length, and no sample")
        # The error that answers it is a line of the error output.
        running.send(proto.encode({"type": "error", "id": fakes.ASKED_ID, "fault": "unsupported", "sentence": "no"}))
        running.end_input()
        self.assertEqual(running.ended(), [0])
        self.assertIn("asked w1 and was answered unsupported", running.error_output.lines())

        # deaf-after-hello and garbage-after-hello: the hello is answered, and then
        # nothing is read: a request is never answered, and the end of the input does not
        # end the worker. garbage-after-hello writes its frame right after its hello.
        for name, after in (("deaf-after-hello", b""), ("garbage-after-hello", fakes.GARBAGE)):
            running = faulty(name)
            self.assertEqual(running.hello()["type"], "hello", name)
            stream = kit.FdStream(running.from_worker)
            written = b""
            while len(written) < len(after):
                chunk = stream.read(65536)
                self.assertTrue(chunk, name)
                written += chunk
            self.assertEqual(written, after, name)
            running.ask(load("r1"))
            running.end_input()
            self.assertEqual(running.exits, [], "%s ended at its input's end" % name)
            # Its lifetime would end it; the test ends it as that would.
            running.made.stop(worker.LIFETIME_CODE)
            self.assertEqual(running.ended(), [worker.LIFETIME_CODE], name)

        # flood: more events than a request may send.
        running = faulty("flood")
        running.hello()
        running.ask(load("r1"))
        with self.assertRaises(proto.Refused) as caught:
            for _ in range(proto.EVENTS_LIMIT + 1):
                running.read()
        self.assertEqual(caught.exception.reason, "over_bound")

        # slow-load and slow-unload: the call says it began, a cancel does not end it, and
        # the end of the input ends the worker inside it.
        for name, first, slow in (
            ("slow-load", None, load("r1")),
            ("slow-unload", load("r1"), unload("r2")),
        ):
            running = faulty(name)
            running.hello()
            if first is not None:
                running.ask(first)
                self.assertEqual(running.read()["type"], "result")
            running.ask(slow)
            self.assertEqual(running.read()["type"], "event")
            running.send(proto.encode({"type": "cancel", "id": slow["id"]}))
            running.end_input()
            self.assertEqual(running.ended(), [0], name)
            self.assertIsNone(running.rest(), "the slow call never answered")

    def test_the_fakes_entry_point_runs_as_a_program_with_its_fault(self):
        # The fourth and last child of these tests: `python -m whizbang_pack.fakes` takes
        # its fault. (The three of `Ends` run `python -m whizbang_pack.worker`.)
        child = Child(self, module="whizbang_pack.fakes", options=["--fake-fault", "other-version"])
        child.send(hello())
        with self.assertRaises(proto.Refused) as caught:
            child.read()
        self.assertEqual(caught.exception.reason, "version")
        child.end_input()
        self.assertEqual(child.wait(), 0)


# --- the adapter of an installation (plan 1D, task U1) --------------------------------------

# A user's adapter of a made-up fake model: it reads its weights, a file of the installation,
# at its load, and answers what `fake-ok` answers, in the fixed order.
USER_ADAPTER = b"""
from whizbang_pack import fake_models
from whizbang_pack.adapter import Adapter


class Weighed(Adapter):
    features = frozenset(("phrase", "point", "box"))
    revision = "user"

    def load(self, installation, settings, call):
        with open(installation.file("weights.bin"), "rb") as file:
            self.weights = file.read()
        return "a user's adapter read %d bytes of weights" % len(self.weights)

    def segment(self, request, call):
        found = fake_models.determined(request)
        found.sort(key=lambda item: (-item.score, item.query, item.box[1], item.box[0], item.box[2], item.box[3]))
        return found, ""


ADAPTER = Weighed
"""

WEIGHTS = b"made-up weights of a fake model"


def record_in(models, model, adapter_name, revision):
    """Installs a record of this model under this adapter in this models directory, with one
    file of made-up weights, as the binary writes one; answers its identity."""
    record = {
        "format": "installation/1",
        "model": model,
        "catalogue_revision": "2026-11.1",
        "adapter": adapter_name,
        "adapter_revision": revision,
        "files": [{"name": "weights.bin", "size": len(WEIGHTS), "sha256": "sha256:" + hashlib.sha256(WEIGHTS).hexdigest()}],
    }
    identity = installed.identity_of(record)
    folder = os.path.join(models, "models", model, "installations", identity[len("sha256:") :])
    os.makedirs(os.path.join(folder, "files"))
    with open(os.path.join(folder, "RECORD"), "wb") as file:
        file.write(json.dumps(record, separators=(",", ":")).encode("ascii"))
    with open(os.path.join(folder, "files", "weights.bin"), "wb") as file:
        file.write(WEIGHTS)
    return identity


def copy_in(models, data, revision=None):
    """A user's adapter copied into the models directory under its hash, or under this
    revision; answers the revision it is named by."""
    revision = revision or hashlib.sha256(data).hexdigest()
    os.makedirs(os.path.join(models, "adapters"), exist_ok=True)
    with open(os.path.join(models, "adapters", revision + ".py"), "wb") as file:
        file.write(data)
    return revision


class Adapters(unittest.TestCase):
    def models(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        return scratch.name

    def loaded(self, running, model, identity):
        running.ask(load("r-" + model + identity[-4:], model=model, installation=identity))
        return running.read()

    def test_the_adapter_is_the_one_the_installations_record_names(self):
        models = self.models()
        # A model of a name no table holds, installed under the adapter `fake-ok`: the
        # record names the adapter, and the model's name picks nothing.
        mine = record_in(models, "my-weights", "fake-ok", "7")
        # And `fake-ok` installed under another adapter: the record wins over the name.
        waits = record_in(models, "fake-ok", "fake-wait", "7")
        shipped = record_in(models, "sam3", "user-mine", "ab" * 32)
        unknown = record_in(models, "other-weights", "no-such-adapter", "7")
        # The record of `my-weights` under a folder of another identity.
        elsewhere = os.path.join(models, "models", "my-weights", "installations", OTHER_INSTALLATION[len("sha256:") :])
        os.makedirs(elsewhere)
        with open(os.path.join(models, "models", "my-weights", "installations", mine[len("sha256:") :], "RECORD"), "rb") as file:
            moved = file.read()
        with open(os.path.join(elsewhere, "RECORD"), "wb") as file:
            file.write(moved)

        running = Running(self, models=models)
        running.hello()
        self.assertEqual(self.loaded(running, "my-weights", mine)["type"], "result")
        self.assertIs(type(running.made.resident.adapter), fake_models.FakeOk)
        running.ask(segment("s1", model="my-weights", installation=mine))
        self.assertEqual(len(running.read()["instances"]), 3)
        self.assertEqual(self.loaded(running, "fake-ok", waits)["type"], "result")
        self.assertIs(type(running.made.resident.adapter), fake_models.FakeWait)

        refused = self.loaded(running, "my-weights", OTHER_INSTALLATION)
        self.assertEqual((refused["fault"], refused["sentence"]), ("rejected", "the installation's record is not the one its identity names"))
        refused = self.loaded(running, "sam3", shipped)
        self.assertEqual(refused["sentence"], "a model of the shipped catalogue is never run by a user's adapter")
        refused = self.loaded(running, "other-weights", unknown)
        self.assertEqual(refused["sentence"], "no adapter answers for this model")
        # Nothing refused unloaded what was resident.
        self.assertIs(type(running.made.resident.adapter), fake_models.FakeWait)

    def test_an_adapter_copy_whose_hash_is_not_the_installations_is_refused_before_import(self):
        models = self.models()
        marker = os.path.join(models, "imported")
        tampered = USER_ADAPTER + b"\nopen(%r, 'w').close()\n" % marker.encode("ascii")
        revision = copy_in(models, USER_ADAPTER)
        good = record_in(models, "mine", "user-mine", revision)
        # The copy another installation names holds other bytes than its hash.
        other = hashlib.sha256(b"an earlier adapter").hexdigest()
        copy_in(models, tampered, revision=other)
        changed = record_in(models, "mine", "user-mine", other)
        missing = record_in(models, "mine", "user-mine", "cd" * 32)
        no_hash = record_in(models, "mine", "user-mine", "7")

        running = Running(self, models=models)
        running.hello()
        for identity, sentence in (
            (changed, "the copy of the user's adapter is not the one the installation was installed with"),
            (missing, "the user's adapter the installation was installed with is not in the models directory"),
            (no_hash, "the installation names a user's adapter by no hash"),
        ):
            refused = self.loaded(running, "mine", identity)
            self.assertEqual((refused["fault"], refused["sentence"]), ("rejected", sentence))
        self.assertFalse(os.path.exists(marker), "nothing of a refused copy ran")
        self.assertIsNone(running.made.resident)

        # The copy whose hash is the record's is run, over the installation's files.
        loaded = self.loaded(running, "mine", good)
        self.assertEqual(loaded["diagnostics"], "a user's adapter read %d bytes of weights" % len(WEIGHTS))
        request = segment("s1", model="mine", installation=good)
        running.ask(request)
        kit.conforming(self, request, running.read())

        # A folder of users' adapters that is a link is not gone through.
        linked = self.models()
        os.symlink(os.path.join(models, "adapters"), os.path.join(linked, "adapters"))
        through = record_in(linked, "mine", "user-mine", revision)
        running = Running(self, models=linked)
        running.hello()
        refused = self.loaded(running, "mine", through)
        self.assertEqual(refused["sentence"], "the folder of users' adapters is a link, and nothing is run through it")

    def test_a_users_adapter_passes_the_conformance_suite_against_a_fake_model(self):
        folder = self.models()
        files = os.path.join(folder, "files")
        os.makedirs(files)
        with open(os.path.join(files, "weights.bin"), "wb") as file:
            file.write(WEIGHTS)
        good = os.path.join(folder, "good.py")
        with open(good, "wb") as file:
            file.write(USER_ADAPTER)
        said = io.StringIO()
        self.assertEqual(conformance.main([good, files], out=said), 0, said.getvalue())
        self.assertEqual(
            said.getvalue().splitlines(),
            [
                "adapter sha256:" + hashlib.sha256(USER_ADAPTER).hexdigest(),
                "phrase: conforms",
                "point: conforms",
                "box: conforms",
                "phrase and point and box: conforms",
                "conforms",
            ],
        )
        # One that breaks the contract is told how, and the suite fails.
        bad = os.path.join(folder, "bad.py")
        with open(bad, "wb") as file:
            file.write(USER_ADAPTER.replace(b'return found, ""', b'return [type(item)(item.query, item.score, item.iou, [0.0, 0.0, 1.0, 1.0], item.samples) for item in found], ""'))
        said = io.StringIO()
        self.assertEqual(conformance.main([bad, files], out=said), 1)
        self.assertIn("phrase: does not conform: a box that is not the bounds of its plane", said.getvalue())
        self.assertEqual(conformance.main([bad], out=io.StringIO()), 2)


# The runtime of the adapter `sam3` (task R2a), which only that adapter imports, and only
# inside a function, when its model is loaded: never when the package is imported.
RUNTIME = {
    "sam3.py": ("PIL", "torch", "transformers"),
    "owlv2_sam2.py": ("PIL", "torch", "transformers", "safetensors"),
    "eomt_ade20k.py": ("PIL", "numpy", "torch", "transformers"),
}


class Load(unittest.TestCase):
    def test_a_load_with_no_record_beside_files_is_refused(self):
        # The review of plan 1D, task U1: an installation's folder that holds `files/` and
        # no record is not run by the adapter its model's name would pick.
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        models = scratch.name
        identity = record_in(models, "fake-ok", "fake-ok", "7")
        folder = os.path.join(models, "models", "fake-ok", "installations", identity[len("sha256:") :])
        os.remove(os.path.join(folder, "RECORD"))
        running = Running(self, models=models)
        running.hello()
        running.ask(load("r1", model="fake-ok", installation=identity))
        refused = running.read()
        self.assertEqual((refused["fault"], refused["sentence"]), ("rejected", "the installation's folder holds files and no record"))
        self.assertIsNone(running.made.resident)


class Shipped(unittest.TestCase):
    def test_the_workers_shipped_map_is_the_shipped_catalogue(self):
        # The worker's map of shipped models (`SHIPPED`) is each model of the catalogue the
        # binary ships, by name, with its adapter: the two cannot drift apart.
        path = os.path.join(os.path.dirname(__file__), "..", "..", "crates", "whizbang", "src", "catalogue.json")
        with open(path, "rb") as file:
            catalogue = json.loads(file.read().decode("utf-8"))
        shipped = {model["name"]: model["adapter"] for model in catalogue["models"]}
        self.assertEqual(worker.SHIPPED, shipped)
        for adapter in shipped.values():
            self.assertIn(adapter, worker.REAL)


class Imports(unittest.TestCase):
    def test_the_core_imports_nothing_outside_the_standard_library(self):
        package = os.path.join(kit.SRC, "whizbang_pack")
        names = sorted(name for name in os.listdir(package) if name.endswith(".py"))
        self.assertEqual(
            names,
            [
                "__init__.py",
                "adapter.py",
                "conformance.py",
                "eomt_ade20k.py",
                "fake_models.py",
                "fakes.py",
                "front.py",
                "installed.py",
                "masks.py",
                "owlv2_sam2.py",
                "proto.py",
                "sam3.py",
                "subjects.py",
                "tool_worker.py",
                "worker.py",
            ],
        )
        for name in names:
            with open(os.path.join(package, name), encoding="utf-8") as file:
                tree = ast.parse(file.read(), name)
            # What runs when the module is imported: everything but a function's body.
            on_import = set()
            todo = list(tree.body)
            while todo:
                node = todo.pop()
                on_import.add(id(node))
                if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
                    todo.extend(ast.iter_child_nodes(node))
            for node in ast.walk(tree):
                if isinstance(node, ast.Import):
                    found = [alias.name for alias in node.names]
                elif isinstance(node, ast.ImportFrom):
                    self.assertEqual(node.level, 0, "%s imports relatively" % name)
                    found = [node.module]
                else:
                    continue
                for module in found:
                    top = module.split(".")[0]
                    with self.subTest(file=name, module=module):
                        runtime = top in RUNTIME.get(name, ()) and id(node) not in on_import
                        self.assertTrue(
                            top == "whizbang_pack" or top in sys.stdlib_module_names or runtime,
                            "%s imports %s" % (name, module),
                        )


# --- child processes ------------------------------------------------------------------------

# Runs an entry point's module as a program, as `python -m` does, with the pack's `src`
# on the path (the pack is not installed): its `if __name__ == "__main__"` stanza runs.
BOOT = (
    "import runpy, sys; sys.path.insert(0, sys.argv.pop(1)); name = sys.argv.pop(1); "
    "runpy.run_module(name, run_name='__main__', alter_sys=True)"
)


class Child:
    """One worker in a child process, started by its entry point's module as a program: its
    environment cleared to the allow-list, every wait on it bounded, and killed and waited
    for in its test's cleanup."""

    def __init__(self, test, lifetime="120", module="whizbang_pack.worker", options=()):
        self.error_output = tempfile.TemporaryFile()
        test.addCleanup(self.error_output.close)
        self.began = time.monotonic()
        self.process = subprocess.Popen(
            [sys.executable, "-I", "-B", "-c", BOOT, kit.SRC, module, "--models-dir", MODELS, "--max-lifetime-s", lifetime]
            + list(options),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.error_output,
            env=kit.child_environment(),
            close_fds=True,
        )
        test.addCleanup(self.kill_and_wait)
        self.reader = proto.Reader(kit.FdStream(self.process.stdout.fileno()), {})

    def send(self, frame):
        self.process.stdin.write(frame)
        self.process.stdin.flush()

    def ask(self, request):
        self.reader.open[request["id"]] = proto.awaiting(request)
        self.send(proto.encode(request))

    def read(self):
        return self.reader.read()

    def end_input(self):
        self.process.stdin.close()

    def wait(self):
        return self.process.wait(kit.WAIT_SECONDS)

    def kill_and_wait(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(kit.WAIT_SECONDS)
        for pipe in (self.process.stdin, self.process.stdout):
            if not pipe.closed:
                pipe.close()
        assert self.process.returncode is not None


class _Blocked:
    """An error output whose every write and flush waits, as a full pipe nobody reads does,
    until its test lets it go (or the test's wait passes)."""

    def __init__(self, test):
        self.released = threading.Event()
        test.addCleanup(self.released.set)

    def write(self, text):
        self.released.wait(kit.WAIT_SECONDS)
        return len(text)

    def flush(self):
        self.released.wait(kit.WAIT_SECONDS)


class Ends(unittest.TestCase):
    def test_the_lifetime_ends_the_worker_though_its_error_output_is_blocked(self):
        running = Running(self, lifetime=0.2, error_output=_Blocked(self))
        began = time.monotonic()
        self.assertEqual(running.made.wait(), worker.LIFETIME_CODE)
        self.assertEqual(running.exits, [worker.LIFETIME_CODE])
        self.assertLess(time.monotonic() - began, 2.0, "nothing waited on the error output")

    # Its lifetime ends it too: in this process, the test above, at 0.2 seconds; and as a
    # process of its own with its input open, exit code 3, in the Rust tests of the
    # supervisor (crates/whizbang-backends/tests/it/pack/worker.rs), whose worker of the
    # fakes runs this module's `main`. This test showed that too, at the least lifetime the
    # command line takes, a whole second (plan m7, task A0, item 2).
    def test_the_worker_exits_at_the_end_of_its_input(self):
        child = Child(self)
        child.send(hello())
        answer = child.read()
        self.assertEqual((answer["type"], answer["role"]), ("hello", "worker"))
        self.assertEqual(answer["threads"], 3, "the reader, the call and the main thread")
        child.ask(load("r1"))
        self.assertEqual(child.read()["threads"], 3)
        child.ask(segment("r2"))
        self.assertEqual(len(child.read()["instances"]), 3)
        child.end_input()
        self.assertEqual(child.wait(), 0)
        self.assertIsNone(child.read(), "nothing more was written")

    def test_the_worker_exits_at_the_end_of_its_input_inside_a_call_that_ignores_cancel(self):
        child = Child(self)
        child.send(hello())
        child.read()
        child.ask(load("r1", model="fake-deaf"))
        self.assertEqual(child.read()["type"], "result")
        child.ask(segment("r2", model="fake-deaf", queries=[{"phrase": "anything"}]))
        self.assertEqual(child.read(), {"type": "event", "id": "r2", "text": "waiting, deaf to a cancel"})
        child.send(proto.encode({"type": "cancel", "id": "r2"}))
        child.end_input()
        self.assertEqual(child.wait(), 0)
        self.assertIsNone(child.read(), "the call that ignored its cancel never answered")


if __name__ == "__main__":
    unittest.main()
