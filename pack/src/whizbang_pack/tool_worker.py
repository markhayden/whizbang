"""The tool worker (plan 2C, task X1, items 2 and 3; docs/adr/0032-tool-files.md): the second
process of the pack's runtime, which runs a user's tool file and nothing else.

    python -m whizbang_pack.tool_worker --max-lifetime-s <seconds>

It speaks `tool/1` (the ADR, "tool/1"): frames on its standard input and output, each four
bytes of a header's length, the header (one JSON object), and each payload the header's
`payloads` lists after eight bytes of its length. Only whizbang's frames carry payloads:
the tool file's code and the images of a `run`, the masks of an `answer`. Its error
output is diagnostics only; what anything in the process prints goes there.

**What it runs.** A `run` carries the tool file's bytes and the SHA-256 whizbang worked
out from them. The bytes are hashed again here, and a run whose bytes do not hash to the
digest is refused before anything of them is compiled; the bytes that were hashed are the
bytes that run, as a module of their own, put nowhere the import system looks. Nothing is
read from a file and nothing is fetched. What runs is the user's code, with this
process's privileges: it is given no key and no home of the user's (whizbang clears the
environment), but it is not kept from the network or the disk.

**What a tool reaches.** Its `run(context)` is given a `Context`: the run's arguments,
the images it declared, and `segment`, which asks whizbang for the capability and waits
for its answer. Whizbang answers it as a nested call of the tool's run.

**How it ends.** Whizbang starts it as the leader of a process group of its own. At the
end of its input, at its lifetime, and at a frame it refuses, it ends every process of
that group, a process a tool started among them, itself last, by `SIGKILL`; started
outside a group of its own, as a test of this module does in-process, it ends itself
alone. Whizbang ends a run that fails, is cancelled or passes its time by closing the
input. Exit codes, where the process gets to give one: 2 for a command line it does not
take, before anything starts.
"""

import hashlib
import json
import os
import queue
import signal
import sys
import threading
import types

VERSION = "tool/1"
REFUSED_CODE = 2
LIFETIME_CODE = 3
LIFETIME_MOST = 31_536_000
HEADER_LIMIT = 65_536
CODE_LIMIT = 1 << 20
PAYLOADS_LIMIT = 16
PIXEL_LIMIT = 50_000_000
SENTENCE_LIMIT = 240
NAME_LIMIT = 64
ASKS_LIMIT = 64
INSTANCES_LIMIT = 64
PHRASE_LIMIT = 512
# The most bytes of a result's header: the output, and room for the rest.
OUTPUT_LIMIT = HEADER_LIMIT - 1024

_PRINTABLE = frozenset(chr(c) for c in range(0x20, 0x7F))


class Usage(Exception):
    """A command line the entry point does not take."""


class Refused(Exception):
    """A frame that breaks a rule of `tool/1`."""


class Stopped(Exception):
    """The worker is ending."""


class CapabilityRefused(Exception):
    """Whizbang refused a capability the tool asked for; `sentence` says why."""

    def __init__(self, sentence):
        super().__init__(sentence)
        self.sentence = sentence


def clean(text, limit):
    """A text of printable ASCII cut to `limit` characters: every other character a space."""
    return "".join(c if c in _PRINTABLE else " " for c in str(text)).strip()[:limit]


def end_group(code, exit=os._exit):
    """Ends every process of this process's group, itself among them, when it leads one;
    and otherwise ends itself alone, with `code`."""
    try:
        if os.getpgrp() == os.getpid():
            os.killpg(os.getpid(), signal.SIGKILL)
    except OSError:
        pass
    exit(code)


def parse(argv):
    """`--max-lifetime-s`, whole seconds from 1, given once and nothing else."""
    args = list(argv)
    if len(args) != 2 or args[0] != "--max-lifetime-s":
        raise Usage("the one option is --max-lifetime-s <seconds>")
    text = args[1]
    if not (text.isascii() and text.isdigit() and 1 <= int(text) <= LIFETIME_MOST):
        raise Usage("--max-lifetime-s must be whole seconds from 1 to %d" % LIFETIME_MOST)
    return int(text)


def _exactly(stream, count):
    chunks, left = [], count
    while left > 0:
        chunk = stream.read(min(left, 1 << 20))
        if not chunk:
            raise Refused("the input ended inside a frame")
        chunks.append(chunk)
        left -= len(chunk)
    return b"".join(chunks)


def read_frame(stream, payload_most):
    """One frame: its header, a `dict`, and its payloads; `None` at the end of the input
    before a frame's first byte. Every bound is looked at before what it bounds is read."""
    first = stream.read(4)
    if not first:
        return None
    if len(first) < 4:
        first += _exactly(stream, 4 - len(first))
    size = int.from_bytes(first, "big")
    if not 1 <= size <= HEADER_LIMIT:
        raise Refused("a header's length is 0 or over its bound")
    try:
        header = json.loads(_exactly(stream, size).decode("utf-8"))
    except (UnicodeError, ValueError):
        raise Refused("a header is not JSON")
    if not isinstance(header, dict) or not isinstance(header.get("type"), str):
        raise Refused("a header is not an object with a type")
    sizes = header.get("payloads", [])
    if not isinstance(sizes, list) or len(sizes) > PAYLOADS_LIMIT:
        raise Refused("a header lists too many payloads")
    if any(type(n) is not int or n < 0 for n in sizes) or sum(sizes) > payload_most:
        raise Refused("a header's payloads are over their bound")
    payloads = []
    for stated in sizes:
        if int.from_bytes(_exactly(stream, 8), "big") != stated:
            raise Refused("a payload's length is not its entry's")
        payloads.append(_exactly(stream, stated))
    return header, payloads


def frame(header):
    """The bytes of a frame of this header and no payload. `ValueError` when it is not
    JSON that a reader takes, or is over its bound."""
    data = json.dumps(header, allow_nan=False, separators=(",", ":"), sort_keys=True).encode("utf-8")
    if not 1 <= len(data) <= HEADER_LIMIT:
        raise ValueError("the header is over its bound")
    return len(data).to_bytes(4, "big") + data


class Image:
    """An image the tool declared: its size and its samples, red, green, blue and alpha
    for each pixel, row by row."""

    def __init__(self, name, width, height, rgba):
        self.name = name
        self.width = width
        self.height = height
        self.rgba = rgba


class Instance:
    """One thing a segmenter found: its score, or `None` where the answer gives none (a
    mask whizbang's store holds has no score), its box in pixels of the image, and its
    mask, one byte for each pixel of the image, 255 where it is and 0 elsewhere."""

    def __init__(self, score, box, width, height, mask):
        self.score = score
        self.box = box
        self.width = width
        self.height = height
        self.mask = mask


class Context:
    """What a tool's `run` is given: its arguments, its images, and the capabilities it
    may ask whizbang for. Each ask waits for whizbang's answer."""

    def __init__(self, worker, id, arguments, images):
        self._worker = worker
        self._id = id
        self._asked = 0
        self._lock = threading.Lock()
        self.arguments = dict(arguments)
        self.images = dict(images)

    def image(self, name):
        """The image the tool declared under this name."""
        if name not in self.images:
            raise KeyError("the tool declared no image " + clean(name, NAME_LIMIT))
        return self.images[name]

    def segment(self, image, phrase=None, point=None, box=None, most=4):
        """The instances whizbang's segmenter finds in the image of this name for one
        query: a phrase, a point `(x, y)` or a box `(left, top, right, bottom)` in pixels.
        `CapabilityRefused` when whizbang refuses or the call does not complete."""
        given = [(kind, value) for kind, value in (("phrase", phrase), ("point", point), ("box", box)) if value is not None]
        if len(given) != 1:
            raise ValueError("segment takes exactly one of phrase, point and box")
        kind, value = given[0]
        if kind == "phrase":
            value = str(value)
            if not 1 <= len(value) <= PHRASE_LIMIT:
                raise ValueError("a phrase is 1 to %d characters" % PHRASE_LIMIT)
        else:
            value = [float(n) for n in value]
        self.image(image)
        if type(most) is not int or not 1 <= most <= INSTANCES_LIMIT:
            raise ValueError("most is a whole number from 1 to %d" % INSTANCES_LIMIT)
        with self._lock:
            if self._asked >= ASKS_LIMIT:
                raise CapabilityRefused("a run asks at most %d times" % ASKS_LIMIT)
            self._asked += 1
            number = self._asked
        ask = {"type": "ask", "id": self._id, "ask": number, "capability": "segment", "image": image, "query": {kind: value}, "most": most}
        header, payloads = self._worker.ask(self._id, number, ask)
        if header["type"] == "refused":
            raise CapabilityRefused(clean(header.get("sentence", ""), SENTENCE_LIMIT))
        return _instances(header, payloads)


def _instances(header, payloads):
    width, height = header.get("width"), header.get("height")
    found = header.get("instances")
    if type(width) is not int or type(height) is not int or not isinstance(found, list):
        raise Refused("an answer gives no size or no instances")
    if len(found) != len(payloads) or len(found) > INSTANCES_LIMIT:
        raise Refused("an answer's masks are not its instances")
    made = []
    for entry, mask in zip(found, payloads):
        if len(mask) != width * height:
            raise Refused("a mask is not the size of its image")
        score = entry.get("score")
        made.append(Instance(None if score is None else float(score), [float(n) for n in entry["box"]], width, height, mask))
    return made


def compiled(code, digest):
    """The function `run` of a tool file, from these bytes, held to the digest whizbang
    gave: the bytes are hashed, and only bytes that hash to it are compiled and run, as a
    module of their own. What runs here is the user's code."""
    if hashlib.sha256(code).hexdigest() != digest:
        raise Refused("the tool file's bytes are not the ones its digest names")
    module = types.ModuleType("whizbang_tool_" + digest)
    module.__file__ = "<tool file sha256:%s>" % digest
    exec(compile(code, module.__file__, "exec"), module.__dict__)
    run = getattr(module, "run", None)
    if not callable(run):
        raise ValueError("the tool file has no function run")
    return run


class Worker:
    def __init__(self, lifetime, stdin, stdout, end=end_group):
        self.lifetime = lifetime
        self.stdin = stdin
        self.stdout = stdout
        self._end = end
        self._runs = queue.SimpleQueue()
        self._waiting = {}
        self._lock = threading.Lock()
        self._writing = threading.Lock()
        self._stopping = threading.Event()
        self._busy = False
        self._said_hello = False
        self.threads = []

    def start(self):
        for name, target in (("tool-reader", self._read), ("tool-run", self._run_tools)):
            thread = threading.Thread(target=target, name=name, daemon=True)
            self.threads.append(thread)
            thread.start()

    def wait(self):
        if not self._stopping.wait(self.lifetime):
            self.stop(LIFETIME_CODE)

    def stop(self, code):
        """Ends the worker and its group, waiting for nothing."""
        self._stopping.set()
        with self._lock:
            waiting = list(self._waiting.values())
        for slot in waiting:
            slot["event"].set()
        self._runs.put(None)
        self._end(code)

    # --- the reader ------------------------------------------------------------------

    def _read(self):
        while not self._stopping.is_set():
            try:
                read = read_frame(self.stdin, PAYLOADS_LIMIT * 4 * PIXEL_LIMIT)
            except (Refused, OSError, ValueError):
                self.stop(REFUSED_CODE)
                return
            if read is None:
                self.stop(0)
                return
            try:
                self._take(*read)
            except (Refused, KeyError, TypeError, ValueError):
                self.stop(REFUSED_CODE)
                return

    def _take(self, header, payloads):
        kind = header["type"]
        if not self._said_hello:
            if kind != "hello" or header.get("version") != VERSION or header.get("role") != "whizbang":
                raise Refused("the first frame must be whizbang's hello of tool/1")
            self._said_hello = True
            self._write(frame({"type": "hello", "version": VERSION, "role": "worker"}))
        elif kind == "run":
            with self._lock:
                busy, self._busy = self._busy, True
            if busy:
                raise Refused("a run came while another runs")
            self._runs.put((header, payloads))
        elif kind in ("answer", "refused"):
            with self._lock:
                slot = self._waiting.get((header["id"], header["ask"]))
            if slot is None or slot["answer"] is not None:
                raise Refused("an answer to nothing that was asked")
            slot["answer"] = (header, payloads)
            slot["event"].set()
        else:
            raise Refused("a frame this side does not take")

    # --- a run -----------------------------------------------------------------------

    def ask(self, id, number, header):
        """Writes an ask and waits for its answer, or for the worker's end."""
        slot = {"event": threading.Event(), "answer": None}
        with self._lock:
            self._waiting[(id, number)] = slot
        try:
            self._write(frame(header))
            slot["event"].wait()
            if self._stopping.is_set() or slot["answer"] is None:
                raise Stopped()
            return slot["answer"]
        finally:
            with self._lock:
                self._waiting.pop((id, number), None)

    def _run_tools(self):
        while True:
            taken = self._runs.get()
            if taken is None or self._stopping.is_set():
                return
            header, payloads = taken
            try:
                answer = self._perform(header, payloads)
            except Stopped:
                return
            except Exception as failure:
                answer = {"type": "error", "id": header.get("id"), "sentence": "the tool failed with " + type(failure).__name__ + ": " + clean(failure, SENTENCE_LIMIT)}
            try:
                data = frame(answer)
            except (ValueError, TypeError):
                data = frame({"type": "error", "id": header.get("id"), "sentence": "the tool's output cannot be written as JSON within its bound"})
            with self._lock:
                self._busy = False
            self._write(data)

    def _perform(self, header, payloads):
        id = header["id"]
        tool = header["tool"]
        images = header["images"]
        if len(payloads) != 1 + len(images) or len(payloads[0]) > CODE_LIMIT:
            raise Refused("a run's payloads are not its code and its images")
        made = {}
        for entry, samples in zip(images, payloads[1:]):
            width, height = entry["width"], entry["height"]
            if len(samples) != 4 * width * height:
                raise Refused("an image is not the size its entry gives")
            made[entry["name"]] = Image(entry["name"], width, height, samples)
        run = compiled(payloads[0], tool["digest"])
        output = run(Context(self, id, header["arguments"], made))
        if not isinstance(output, dict):
            raise TypeError("run returned no dict")
        data = json.dumps(output, allow_nan=False)
        if len(data) > OUTPUT_LIMIT:
            raise ValueError("the output is over %d bytes" % OUTPUT_LIMIT)
        return {"type": "result", "id": id, "output": output}

    def _write(self, data):
        failed = False
        with self._writing:
            try:
                self.stdout.write(data)
                self.stdout.flush()
            except (OSError, ValueError):
                failed = True
        if failed:
            self.stop(REFUSED_CODE)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    # Frames go to a copy of the standard output; the standard output itself becomes the
    # error output, so that what a tool prints is never read as a frame.
    frames = os.fdopen(os.dup(1), "wb", buffering=0)
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    try:
        lifetime = parse(argv)
    except Usage as usage:
        sys.stderr.write("whizbang_pack: %s\n" % usage)
        sys.stderr.flush()
        os._exit(REFUSED_CODE)
    worker = Worker(lifetime, sys.stdin.buffer, frames)
    worker.start()
    worker.wait()


if __name__ == "__main__":
    # Run from the module as it is imported, so that a tool that imports this module
    # (for `CapabilityRefused`) meets the classes its context raises, and not a second
    # copy of them under `__main__`.
    from whizbang_pack import tool_worker as _imported

    _imported.main()
