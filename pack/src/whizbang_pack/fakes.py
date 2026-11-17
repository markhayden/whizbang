"""The worker with a fault of its own, for the tests of whizbang's supervisor
(docs/plans/m1c-local-pack.md, D3). The kind `pack` never starts this entry point; the
worker's own (`whizbang_pack.worker`) refuses `--fake-fault`.

    python -m whizbang_pack.fakes --models-dir <folder> [--max-lifetime-s <seconds>]
        [--fake-fault <name>]

| Fault | What the worker does |
|---|---|
| `no-hello` | Reads whizbang's hello and never answers it |
| `other-version` | Answers the hello with one that states `pack/2` |
| `deaf-to-end-of-input` | Does not end at the end of its input; only its lifetime ends it, so it is refused without `--max-lifetime-s` |
| `error-output` | Writes 300 lines to its error output before its hello, every tenth over 5,000 bytes |
| `flood` | Sends 300 events before each call, more than a request may send |
| `slow-load` | Says a `load` began, then waits, deaf to a cancel, until the worker ends |
| `slow-unload` | The same of an `unload` |

Without `--fake-fault` it is the worker itself, with the same fake models.
"""

import os
import sys

from whizbang_pack import proto
from whizbang_pack import worker as _worker
from whizbang_pack.worker import Usage, Worker

FLOOD = proto.EVENTS_LIMIT + 44
ERROR_LINES = 300


class NoHello(Worker):
    def _say_hello(self):
        pass


class OtherVersion(Worker):
    def _say_hello(self):
        hello = {
            "type": "hello",
            "id": self.conversation,
            "version": proto.VERSION,
            "role": "worker",
            "device": None,
            "threads": self._threads(),
        }
        chunks = proto.chunks(hello)
        # The same number of bytes, so the frame's length still holds.
        chunks[0] = chunks[0].replace(b'"version":"pack/1"', b'"version":"pack/2"')
        self._write(chunks)


class DeafToEndOfInput(Worker):
    def _end_of_input(self):
        # The reader ends; the process lives on until its lifetime.
        pass


class ErrorOutput(Worker):
    def _say_hello(self):
        for number in range(ERROR_LINES):
            line = "error output line %d" % number
            if number % 10 == 0:
                line += " " + "x" * 5_000
            self.stderr.write(line + "\n")
        self.stderr.flush()
        super()._say_hello()


class Flood(Worker):
    def _before_call(self, call):
        for number in range(FLOOD):
            self._write(proto.chunks({"type": "event", "id": call.id, "text": "flood %d" % number}))


class SlowLoad(Worker):
    def _load(self, call):
        call.event("loading slowly, deaf to a cancel")
        call.wait_for_stop()


class SlowUnload(Worker):
    def _unload(self, call):
        call.event("unloading slowly, deaf to a cancel")
        call.wait_for_stop()


FAULTS = {
    "no-hello": NoHello,
    "other-version": OtherVersion,
    "deaf-to-end-of-input": DeafToEndOfInput,
    "error-output": ErrorOutput,
    "flood": Flood,
    "slow-load": SlowLoad,
    "slow-unload": SlowUnload,
}


def make(argv, stdin, stdout, stderr, exit=os._exit, adapters=None):
    """A worker with the fault its command line names, not started; `Usage` for a command
    line this entry point does not take."""
    options = _worker.parse(argv, faults=tuple(FAULTS))
    fault = options["fault"]
    if fault == "deaf-to-end-of-input" and options["lifetime"] is None:
        raise Usage("--fake-fault deaf-to-end-of-input is taken only with --max-lifetime-s")
    kind = FAULTS.get(fault, Worker)
    return kind(options["models_dir"], options["lifetime"], stdin, stdout, stderr, exit, adapters)


if __name__ == "__main__":
    _worker.main(sys.argv[1:], make=make)
