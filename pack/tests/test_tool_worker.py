"""The tool worker (plan 2C, task X1; docs/adr/0032-tool-files.md), in this process over two
pipes: whizbang's side is played by the test, and the worker's end is recorded, not done."""

import hashlib
import json
import os
import unittest

import kit
from whizbang_pack import tool_worker

TOOL = b'''
def run(context):
    found = context.segment("source", phrase=context.arguments["phrase"])
    image = context.image("source")
    on = sum(1 for sample in found[0].mask if sample)
    return {"readings": [{"subject": context.arguments["phrase"], "values": {"area": on / (image.width * image.height)}}]}
'''


def framed(header, payloads=()):
    header = dict(header, payloads=[len(p) for p in payloads]) if payloads else header
    data = json.dumps(header).encode("utf-8")
    out = len(data).to_bytes(4, "big") + data
    for payload in payloads:
        out += len(payload).to_bytes(8, "big") + payload
    return out


class Pipes:
    """A worker in this process over two pipes; its end recorded in `ends`."""

    def __init__(self, test):
        to_read, self.to_worker = os.pipe()
        self.from_worker, from_write = os.pipe()
        self.ends = []
        self.worker = tool_worker.Worker(60, open(to_read, "rb", buffering=0), open(from_write, "wb", buffering=0), end=self.ends.append)
        self.stream = kit.FdStream(self.from_worker)
        test.addCleanup(self.close)
        self.worker.start()

    def send(self, data):
        view = memoryview(data)
        while view:
            view = view[os.write(self.to_worker, view) :]

    def read(self):
        return tool_worker.read_frame(self.stream, 0)[0]

    def close(self):
        if self.to_worker is not None:
            os.close(self.to_worker)
            self.to_worker = None
        for thread in self.worker.threads:
            thread.join(kit.WAIT_SECONDS)
        if self.from_worker is not None:
            os.close(self.from_worker)
            self.from_worker = None


class Capabilities(unittest.TestCase):
    def test_a_tool_file_asks_for_a_segment_and_is_answered(self):
        pipes = Pipes(self)
        pipes.send(framed({"type": "hello", "version": "tool/1", "role": "whizbang"}))
        self.assertEqual(pipes.read(), {"type": "hello", "version": "tool/1", "role": "worker"})
        image = bytes([200, 0, 0, 255]) * 4
        run = {
            "type": "run",
            "id": "r1",
            "tool": {"name": "areas", "digest": hashlib.sha256(TOOL).hexdigest()},
            "arguments": {"phrase": "red"},
            "images": [{"name": "source", "width": 2, "height": 2}],
        }
        pipes.send(framed(run, [TOOL, image]))
        # The tool asks for the capability through its context, and waits.
        ask = pipes.read()
        self.assertEqual(
            ask,
            {"type": "ask", "id": "r1", "ask": 1, "capability": "segment", "image": "source", "query": {"phrase": "red"}, "most": 4},
        )
        answer = {"type": "answer", "id": "r1", "ask": 1, "width": 2, "height": 2, "instances": [{"score": 0.9, "box": [0, 0, 2, 1]}]}
        pipes.send(framed(answer, [bytes([255, 255, 0, 0])]))
        # Its answer reaches the tool, whose output is the run's result.
        result = pipes.read()
        self.assertEqual(result, {"type": "result", "id": "r1", "output": {"readings": [{"subject": "red", "values": {"area": 0.5}}]}})
        # Bytes that do not hash to the digest are never compiled: the run fails.
        spoiled = dict(run, id="r2")
        pipes.send(framed(spoiled, [TOOL + b"\n", image]))
        failed = pipes.read()
        self.assertEqual((failed["type"], failed["id"]), ("error", "r2"))
        self.assertIn("digest", failed["sentence"])
        # A refusal of the capability reaches the tool as one.
        pipes.send(framed(dict(run, id="r3"), [TOOL, image]))
        self.assertEqual(pipes.read()["ask"], 1)
        pipes.send(framed({"type": "refused", "id": "r3", "ask": 1, "sentence": "no segmenter"}))
        refused = pipes.read()
        self.assertEqual(refused["type"], "error")
        self.assertIn("CapabilityRefused: no segmenter", refused["sentence"])
        # The end of the input ends the worker, at once.
        pipes.close()
        self.assertEqual(pipes.ends, [0])

    def test_an_answer_to_nothing_asked_ends_the_worker(self):
        pipes = Pipes(self)
        pipes.send(framed({"type": "hello", "version": "tool/1", "role": "whizbang"}))
        pipes.read()
        pipes.send(framed({"type": "refused", "id": "r9", "ask": 1, "sentence": "x"}))
        for thread in pipes.worker.threads[:1]:
            thread.join(kit.WAIT_SECONDS)
        self.assertEqual(pipes.ends, [tool_worker.REFUSED_CODE])


class Ending(unittest.TestCase):
    def test_outside_a_group_of_its_own_it_ends_itself_alone(self):
        # This process leads no group of its own (the runner does, or the shell): the end
        # signals nothing and only exits.
        if os.getpgrp() == os.getpid():
            self.skipTest("the test runner leads its own group")
        ended = []
        tool_worker.end_group(0, exit=ended.append)
        self.assertEqual(ended, [0])

    def test_a_command_line_is_the_lifetime_alone(self):
        self.assertEqual(tool_worker.parse(["--max-lifetime-s", "5"]), 5)
        for argv in ([], ["--max-lifetime-s", "0"], ["--models-dir", "/x"]):
            with self.assertRaises(tool_worker.Usage):
                tool_worker.parse(argv)


if __name__ == "__main__":
    unittest.main()
