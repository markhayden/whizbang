"""The worker with a fault of its own, for the tests of whizbang's supervisor
(docs/plans/m1c-local-pack.md, D3). The kind `pack` never starts this entry point; the
worker's own (`whizbang_pack.worker`) refuses `--fake-fault`.

    python -m whizbang_pack.fakes --models-dir <folder> [--max-lifetime-s <seconds>]
        [--fake-fault <name>]

| Fault | What the worker does |
|---|---|
| `no-hello` | Reads whizbang's hello and never answers it |
| `other-version` | Answers the hello with one that states `pack/2`; writes a line to its error output for each `error` it takes, with its fault and its sentence |
| `deaf-to-end-of-input` | Does not end at the end of its input; only its lifetime ends it, so it is refused without `--max-lifetime-s` |
| `error-output` | Writes 300 lines to its error output before its hello, every tenth over 5,000 bytes; then a line for each variable of its environment and for each descriptor it holds, and a marker, a host and a made-up token, a line each, for the redaction tests of whizbang's supervisor. At the start of each call it writes a line that names the call, and the marker, the host and the token again; before a segment of `fake-exit` it first closes its frames' output, and writes them `LATE_SECONDS` later, the end of its input not ending it meanwhile |
| `flood` | Sends 300 events before each call, more than a request may send |
| `slow-load` | Says a `load` began, then waits, deaf to a cancel, until the worker ends |
| `slow-unload` | The same of an `unload` |
| `grandchild` | Before its hello, starts a process of its own that holds the worker's standard input, its frames and its error output open, writes `grandchild <pid>` to its error output, and says hello. The process ends by itself after `GRANDCHILD_SECONDS`, whatever happens to the worker, so that nothing it starts outlives a test by more than that |
| `deaf-after-hello` | Answers the hello, and then reads nothing more: its reader waits until the worker ends, so a frame whizbang writes past what the pipe holds is never taken |
| `garbage-after-hello` | The same, and right after its hello writes a frame whizbang refuses (a header that is no JSON), so that whizbang has something to say back while the pipe is full |
| `asks` | At the start of each call, sends a request of its own (a `segment` of `ASKED_ID`), its header and its payload's length and none of its samples, then waits, deaf to a cancel, until the worker ends: whizbang refuses a request from a worker before a byte of its payload is read. Writes a line to its error output when an `error` answers that request |

Without `--fake-fault` it is the worker itself, with the same fake models.
"""

import os
import stat
import sys
import time

from whizbang_pack import proto
from whizbang_pack import worker as _worker
from whizbang_pack.worker import Usage, Worker

FLOOD = proto.EVENTS_LIMIT + 44
ERROR_LINES = 300

# What `error-output` writes last, for the tests of whizbang's supervisor that hold what it
# keeps of the error output, and a fault it makes, to what may be shown: a marker, a host
# and a token, each made up here. The token is of no real service's shape.
MARKER = "fake-error-output-marker-7f3a9c"
HOST = "pack-worker.example.invalid"
TOKEN = "made-up-token-5b8e1d0c4a2f6e93"

# The descriptors `error-output` looks at: from 0 up to this, not including it.
DESCRIPTORS_LOOKED_AT = 1024

# How long the process `grandchild` starts lives, and a word its command line holds so
# that a test can look for it among the machine's processes.
GRANDCHILD_SECONDS = 2
GRANDCHILD_WORD = "whizbang-fake-grandchild"

# How long `error-output` waits, before a segment of `fake-exit`, between closing its
# frames' output and writing its last lines.
LATE_SECONDS = 0.5

# The frame `garbage-after-hello` writes: a header of two bytes that is no JSON object.
GARBAGE = (2).to_bytes(4, "big") + b"{]"

# The id of the request `asks` sends, and the binding its image is under.
ASKED_ID = "w1"
ASKED_BINDING = "sha256:" + "0" * 64


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

    def _take(self, message):
        # What whizbang answers the hello with, which this reader took as a frame of
        # pack/1: a line of the error output, for the test of the text whizbang sends.
        if message["type"] == "error":
            self.stderr.write("took an error %s: %s\n" % (message["fault"], message["sentence"]))
            self.stderr.flush()
        super()._take(message)


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
        for name in sorted(os.environ):
            self.stderr.write("environment %s=%s\n" % (name, os.environ[name]))
        for line in descriptors():
            self.stderr.write(line + "\n")
        self.stderr.write("marker %s\n" % MARKER)
        self.stderr.write("connecting to host %s\n" % HOST)
        self.stderr.write("token %s\n" % TOKEN)
        self.stderr.flush()
        super()._say_hello()

    late = False

    def _end_of_input(self):
        # Inside the late lines before `fake-exit` ends the process, the end of the input
        # does not end it: `fake-exit` does, at once after them.
        if not self.late:
            super()._end_of_input()

    def _before_call(self, call):
        # The same three lines again, inside the call, just before it does anything: a
        # supervisor that hands on this error output with a failure of the call holds them
        # only if it waited for the error output's end. Before a segment of `fake-exit`,
        # which ends the process, the frames' output is closed first and the lines come
        # `LATE_SECONDS` after it, the end of its input not ending it meanwhile: a
        # supervisor that took the error output as soon as the frames ended would miss them.
        request = call.request
        if request["kind"] == "segment" and request["model"] == "fake-exit":
            self.late = True
            self.stdout.close()
            time.sleep(LATE_SECONDS)
        self.stderr.write("in the call %s\n" % call.id)
        self.stderr.write("marker %s\n" % MARKER)
        self.stderr.write("connecting to host %s\n" % HOST)
        self.stderr.write("token %s\n" % TOKEN)
        self.stderr.flush()


def descriptors():
    """A line for each descriptor the process holds below `DESCRIPTORS_LOOKED_AT`: its
    number; `pipe`, `file` or `other`; and the device and the number of what it is open on.
    Each is looked at with `fstat`, which opens nothing."""
    lines = []
    for number in range(DESCRIPTORS_LOOKED_AT):
        try:
            status = os.fstat(number)
        except OSError:
            continue
        if stat.S_ISFIFO(status.st_mode):
            kind = "pipe"
        elif stat.S_ISREG(status.st_mode):
            kind = "file"
        else:
            kind = "other"
        lines.append("descriptor %d %s %d %d" % (number, kind, status.st_dev, status.st_ino))
    return lines


class DeafAfterHello(Worker):
    def _say_hello(self):
        super()._say_hello()
        self._after_hello()
        # The reader, which said hello, reads nothing more: it waits here until the worker
        # ends, by its lifetime, its end of input never being read, or a kill.
        self._stopping.wait()

    def _after_hello(self):
        """Nothing; `garbage-after-hello` writes its frame here."""


class GarbageAfterHello(DeafAfterHello):
    def _after_hello(self):
        # A header of two bytes that is no JSON object.
        self._write([GARBAGE])


class Asks(Worker):
    def _before_call(self, call):
        # A request of the worker's own, as a request at pack/1 is asked of a worker: its
        # header and the eight bytes of its payload's length are written, and none of the
        # payload's samples. Then the call waits, deaf to a cancel, until the worker ends.
        asked = {
            "type": "request",
            "id": ASKED_ID,
            "kind": "segment",
            "model": call.request["model"],
            "installation": call.request["installation"],
            "image": proto.Payload(1, 1, ASKED_BINDING, bytes(4)),
            "queries": [{"phrase": "anything"}],
            "max_instances": 1,
        }
        self._write(proto.chunks(asked)[:-1])
        call.wait_for_stop()

    def _take(self, message):
        # What whizbang answers that request with: a line of the error output.
        if message["type"] == "error" and message["id"] == ASKED_ID:
            self.stderr.write("asked %s and was answered %s\n" % (ASKED_ID, message["fault"]))
            self.stderr.flush()
        super()._take(message)


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


class Grandchild(Worker):
    def _say_hello(self):
        import subprocess

        # It inherits the standard input and the error output (and the standard output,
        # which is the error output here: `main`), and is handed the copy the frames are
        # written to. It reads nothing and writes nothing; it only holds them.
        process = subprocess.Popen(
            [
                sys.executable,
                "-I",
                "-c",
                "import time; time.sleep(%d)" % GRANDCHILD_SECONDS,
                GRANDCHILD_WORD,
            ],
            pass_fds=(self.stdout.fileno(),),
        )
        self.stderr.write("grandchild %d\n" % process.pid)
        self.stderr.flush()
        super()._say_hello()


FAULTS = {
    "no-hello": NoHello,
    "other-version": OtherVersion,
    "deaf-to-end-of-input": DeafToEndOfInput,
    "error-output": ErrorOutput,
    "flood": Flood,
    "slow-load": SlowLoad,
    "slow-unload": SlowUnload,
    "grandchild": Grandchild,
    "asks": Asks,
    "deaf-after-hello": DeafAfterHello,
    "garbage-after-hello": GarbageAfterHello,
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
