"""The front: `pack/1` over HTTP, and the supervision of the worker beside a card (ADR 0046;
docs/plans/m6-http-and-remote-hosting.md, task P1).

    python -m whizbang_pack.front --listen <host:port> --models-dir <folder>
        --token-env <NAME> [--worker local | --worker <module:function>]
        [--host-limit-s <seconds>] [--keep-alive-ms <ms>]
        [--tls-cert <file> --tls-key <file>] [--redact-env <NAME>]...

It does on a GPU host what whizbang's Rust supervisor does on the Mac (ADR 0020, "The
worker"), in two parts:

- **the front** (`Front`) holds the connections, the queue of 4 and the routes:
  `POST /pack/1/request`, `POST /pack/1/cancel` and `GET /pack/1/health`;
- **the worker's side** (`LocalSide`) holds the one worker process: the one lock of ordinary
  calls, the claim of each dispatch id, the check of the installation's files before a load,
  the load whenever what is resident differs, the cancel and its deadline, the kill, and
  the worker's error output. Under `--worker local` it is in the front's process; a callable
  named by `--worker <module:function>` puts it elsewhere (on Modal, in the GPU container,
  task D1). A job crosses between the two as plain data, and what the side makes comes back
  through `emit` as plain data.

The standard library only, as the rest of the pack (`dependencies = []`). Every wait is
finite, and every phase of a connection has its deadline (`Limits`).

**An answer's headers come at once** (ADR 0046, "The headers at once"): once the
token is checked and the request read whole, before anything waits on the worker's side, so
that a host's proxy that gives up on an answer whose headers are late (Modal's, at 150
seconds) never sees one. What is known only later comes in the body: the identity of the
files in an identity record, a failure in the outcome record, and a keep-alive whenever
nothing else was written for `Limits.keep_alive` seconds.

**What the front logs** to its error output, one line each and never a token, an image or a
prompt: each cancel it is given and where it went, each cancel frame written to a worker and
how long after it the call ended, and each worker started and ended (`say`).
"""

import collections
import hashlib
import hmac
import importlib
import io
import json
import os
import queue
import re
import selectors
import signal
import socket
import ssl
import subprocess
import sys
import threading
import time

from whizbang_pack import installed, proto
from whizbang_pack.adapter import Fault, Installation

FORMAT = "pack-host/1"

# The front's queue: requests held at once, the one with the worker's side among them.
QUEUE_MOST = 4
# The handlers a classified connection is handed to.
REQUEST_HANDLERS = 6
CONTROL_HANDLERS = 2
# Connections the classifier holds at once, before their request is read.
WAITING_MOST = 64
# The most bytes of a request line and its headers, and of a cancel's body.
HEAD_MOST = 16 * 1024
CONTROL_BODY_MOST = 4 * 1024
# The most bytes of a request's body: whizbang's hello, a `load` and a segment request,
# each a header at its bound, and the segment's image at the protocol's most pixels.
BODY_MOST = 3 * (4 + proto.HEADER_LIMIT) + 8 + 4 * proto.PIXEL_LIMIT
# The outcome record's bounds (ADR 0046): its document, and the lines that follow it.
RECORD_MOST = 4 * 1024
LINES_KEPT = 256
LINE_BYTES = 4_096
LINE_READ_BYTES = 2 * LINE_BYTES
# How many workers in a row may end before they answered anything before no worker is
# started for `restart_pause` seconds, as the local supervisor's `STARTS_FAILED_MOST`.
STARTS_FAILED_MOST = 3
# The host's limit of one attempt when none is given (ADR 0046): the plan's example.
HOST_LIMIT_DEFAULT = 600
# How long an answer stays silent at most before a keep-alive: a third of the client's
# shortest limit between two reads of any answer (whizbang's `BETWEEN_READS_LIMIT`, 30
# seconds); the limit between two reads of an answer of frames is the request's own, its
# host's limit (600 seconds on Modal), so either way three keep-alives fit in it.
KEEP_ALIVE_S = 10.0
# The bounds of `--keep-alive-ms`.
KEEP_ALIVE_MS_LEAST = 1
KEEP_ALIVE_MS_MOST = 20_000
# A token of the front is at least this long, of printable ASCII.
TOKEN_LEAST = 32
# Installations the health document names at most.
HEALTH_MOST = 64

OUTCOMES = ("complete", "failed")
ADMISSIONS = ("busy", "replay", "not_after", "claim_unreachable")
STAGES = ("not_sent", "unresolved", "answered")

DISPATCH = re.compile(r"[0-9a-f]{32}\Z")
_HEADER_NAME = re.compile(rb"[!#$%&'*+.^_`|~0-9A-Za-z-]+\Z")


class Limits:
    """Every deadline the front and the worker's side keep, in seconds: the defaults are
    ADR 0046's, and a test gives shorter ones."""

    def __init__(self, **given):
        self.classify = 2.0  # handshake, line and headers, a cancel's body
        self.body_grace = 5.0  # a request's body: free for its first seconds
        self.body_rate = float(1 << 20)  # then at least this many bytes a second
        self.write = 30.0  # each write of an answer
        self.control_write = 1.0  # a control handler's answer
        self.hello = 60.0
        self.load = 120.0
        self.unload = 30.0
        self.call = 120.0
        self.cancel = 5.0
        self.end = 5.0
        self.restart_pause = 60.0
        # The longest the front writes nothing on a request's answer before a keep-alive:
        # well under the client's limit between two reads (ADR 0046, "The headers at
        # once").
        self.keep_alive = KEEP_ALIVE_S
        # How long past a request's `not_after` the front waits for its outcome before it
        # answers `timed_out` itself and cancels it: keep-alives never hold an answer open
        # past its host's limit.
        self.overrun = 5.0
        for name, value in given.items():
            if not hasattr(self, name):
                raise TypeError("no limit is named " + name)
            setattr(self, name, float(value))


def say(line):
    """One line of the front's log on its error output, flushed at once, so a host's log
    shows it as it happens. A caller never passes a token, an image or a prompt."""
    try:
        sys.stderr.write("whizbang_pack.front: %s\n" % line)
        sys.stderr.flush()
    except (OSError, ValueError):
        pass


class Usage(Exception):
    """A command line the front does not take."""


class Unreachable(Exception):
    """The claim store could not be reached."""


# --- the outcome record ----------------------------------------------------------------------


def outcome(result, kind=None, admission=None, stage="not_sent", ended=None, lines=()):
    """An outcome as the worker's side hands it on: the document and its lines."""
    assert result in OUTCOMES and stage in STAGES
    assert kind is None or kind in proto.FAULTS
    assert admission is None or admission in ADMISSIONS
    lines = list(lines)[-LINES_KEPT:]
    document = {
        "format": FORMAT,
        "outcome": result,
        "kind": kind,
        "admission": admission,
        "stage": stage,
        "ended": ended,
        "lines": len(lines),
    }
    return ("outcome", document, [bytes(line[:LINE_BYTES]) for line in lines])


# What begins the outcome record: four zero bytes, which no frame of `pack/1` begins with (a
# header's length is never 0), so a reader tells the record from a frame by them.
RECORD_MARK = b"\0\0\0\0"


def record_bytes(document, lines):
    """The outcome record as it ends an answer: its mark, four bytes of length and the
    document, then each line as four bytes of length and its bytes (ADR 0046)."""
    head = json.dumps(document, separators=(",", ":"), sort_keys=True).encode("ascii")
    if len(head) > RECORD_MOST:
        raise ValueError("an outcome record is over its bound")
    parts = [RECORD_MARK, len(head).to_bytes(4, "big"), head]
    for line in lines:
        parts.append(len(line).to_bytes(4, "big"))
        parts.append(line)
    return b"".join(parts)


def read_record(stream, marked=False):
    """The outcome record from a stream, each length read and checked before its bytes:
    `(document, lines)`; `ValueError` for a record that is cut or over a bound. `marked`
    when its mark was read already."""

    def take(count):
        data = stream.read(count)
        if data is None or len(data) != count:
            raise ValueError("truncated")
        return data

    if not marked and take(4) != RECORD_MARK:
        raise ValueError("truncated")
    size = int.from_bytes(take(4), "big")
    if not 0 < size <= RECORD_MOST:
        raise ValueError("truncated")
    document = json.loads(take(size).decode("ascii"))
    count = document.get("lines")
    if type(count) is not int or not 0 <= count <= LINES_KEPT:
        raise ValueError("truncated")
    lines = []
    for _ in range(count):
        length = int.from_bytes(take(4), "big")
        if length > LINE_BYTES:
            raise ValueError("truncated")
        lines.append(take(length))
    if stream.read(1):
        raise ValueError("bytes follow the outcome record")
    return document, lines


# A keep-alive: the mark and a length of 0, which no record has. A reader passes over it.
KEEP_ALIVE = RECORD_MARK + b"\0\0\0\0"


def identity_bytes(digest):
    """The identity record (ADR 0046, "The headers at once"): the mark, four bytes of
    length and the document `{"files": <digest>, "format": "pack-host/1"}`. It comes before
    the first frame of every answer whose files were checked, and never in one whose were
    not."""
    head = json.dumps({"files": digest, "format": FORMAT}, separators=(",", ":"), sort_keys=True).encode("ascii")
    return RECORD_MARK + len(head).to_bytes(4, "big") + head


# --- the identity of the files ---------------------------------------------------------------


def files_digest(files):
    """The digest an answer names in its identity record (ADR 0046, "The identity"): the
    SHA-256 of `<name> <sha256>` lines, one for each file, in the order of the name's bytes."""
    lines = sorted((file["name"].encode("ascii"), file["sha256"].encode("ascii")) for file in files)
    hashed = hashlib.sha256()
    for name, digest in lines:
        hashed.update(name + b" " + digest + b"\n")
    return "sha256:" + hashed.hexdigest()


def check_files(models_dir, model, identity):
    """Checks every file of an installation against its record before a load, as
    `Supervisor::call_checked` does: the record held to the identity, each file's size and
    SHA-256. The digest of the files checked; `Fault` for anything that differs."""
    installation = Installation(models_dir, model, identity)
    record = installed.record_of(installation)
    if record is None:
        raise Fault("rejected", "the installation holds no record, so its files cannot be checked")
    for file in record["files"]:
        path = installation.file(file["name"])
        hashed = hashlib.sha256()
        size = 0
        try:
            with open(path, "rb") as opened:
                while True:
                    chunk = opened.read(1 << 20)
                    if not chunk:
                        break
                    size += len(chunk)
                    hashed.update(chunk)
        except OSError:
            raise Fault("rejected", "a file of the installation cannot be read") from None
        if size != file["size"] or "sha256:" + hashed.hexdigest() != file["sha256"]:
            raise Fault("rejected", "a file of the installation is not the one its record names")
    return files_digest(record["files"]), record["files"]


# --- the claims of dispatch ids --------------------------------------------------------------


class Claims:
    """The claim store of `--worker local`: in the process. `claim` is an atomic put that
    fails when the key is there. Task D1's bridge gives a store of the host's own."""

    def __init__(self):
        self._held = set()
        self._lock = threading.Lock()

    def claim(self, dispatch):
        with self._lock:
            if dispatch in self._held:
                return False
            self._held.add(dispatch)
            return True


# --- the worker process ----------------------------------------------------------------------

_SRC = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# What starts the worker: as the Rust supervisor's boot (`pack/worker.rs`, `BOOT`), it puts
# the pack's source on the path and runs the entry point as a program. The Rust boot also
# closes every descriptor above the three; here `subprocess` does that before the program
# starts (`close_fds=True`), so the boot does not do it again.
BOOT = (
    "import os, sys, runpy\n"
    "sys.path[:0] = sys.argv.pop(1).split(os.pathsep)\n"
    "name = sys.argv.pop(1)\n"
    "runpy.run_module(name, run_name='__main__', alter_sys=True)\n"
)
# The variables the worker is given: those of the parent's locale, and the ones the local
# supervisor sets (`pack/worker.rs`, `environment`), but for its home, which a host gives.
PASSED = ("LANG", "LC_ALL", "LC_CTYPE", "HOME", "TMPDIR")
SET = {
    "HF_HUB_OFFLINE": "1",
    "HF_HUB_DISABLE_TELEMETRY": "1",
    "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1",
    "DO_NOT_TRACK": "1",
    "PATH": "/usr/bin:/bin",
    "OMP_NUM_THREADS": "8",
    "MKL_NUM_THREADS": "8",
    "OPENBLAS_NUM_THREADS": "8",
    "TOKENIZERS_PARALLELISM": "false",
    "PYTHONDONTWRITEBYTECODE": "1",
    "HF_HUB_DISABLE_PROGRESS_BARS": "1",
}


def spawner(models_dir, module="whizbang_pack.worker", more=(), environment=None, lifetime_s=86_400, sources=(_SRC,), flags=("-I", "-B")):
    """What starts a worker process: the pack's interpreter, isolated, with its source (and
    any other folder a test names) on the path and its three standard streams pipes. It
    writes no bytecode (`-B`); a test may give a cache of its own in place of that."""

    def spawn():
        if environment is None:
            env = {name: os.environ[name] for name in PASSED if name in os.environ}
            env.update(SET)
        else:
            env = dict(environment)
        command = [sys.executable] + list(flags) + ["-c", BOOT, os.pathsep.join(sources), module]
        command += ["--models-dir", models_dir, "--max-lifetime-s", str(lifetime_s)] + list(more)
        return subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            close_fds=True,
        )

    return spawn


class _Ended(Exception):
    """A request's end in the worker's side that is no answer of the worker's."""

    def __init__(self, kind, stage):
        super().__init__(kind)
        self.kind = kind
        self.stage = stage


class _NotSent(Exception):
    """A request cancelled before a byte of it was written: the worker is left as it is."""


class _Write:
    __slots__ = ("chunks", "done", "taken", "failed")

    def __init__(self, chunks):
        self.chunks = chunks
        self.done = threading.Event()
        self.taken = 0
        self.failed = False


class _Process:
    """One worker process and its three threads: the reader of its frames, the reader of
    its error output, and the writer of its input."""

    def __init__(self, process, secrets):
        self.process = process
        self.pid = process.pid
        self.open = {}
        self.frames = queue.SimpleQueue()
        self.killed = False
        self.answered = False
        self._secrets = [secret for secret in secrets if secret]
        self._lines = collections.deque(maxlen=LINES_KEPT)
        self._seq = 0
        self._lines_lock = threading.Lock()
        self._jobs = queue.SimpleQueue()
        self._input = process.stdin.fileno()
        self._errors_done = threading.Event()
        self.threads = [
            threading.Thread(target=self._read_frames, name="front-frames", daemon=True),
            threading.Thread(target=self._read_errors, name="front-errors", daemon=True),
            threading.Thread(target=self._write_input, name="front-input", daemon=True),
        ]
        for thread in self.threads:
            thread.start()

    # the three threads

    def _read_frames(self):
        reader = proto.Reader(self.process.stdout, self.open)
        while True:
            try:
                message = reader.read()
            except proto.Refused as refused:
                self.frames.put(("refused", refused.reason))
                return
            except (OSError, ValueError):
                self.frames.put(("ended",))
                return
            if message is None:
                self.frames.put(("ended",))
                return
            self.frames.put(("message", message))

    def _read_errors(self):
        stream = self.process.stderr
        try:
            while True:
                line = stream.readline(LINE_READ_BYTES)
                if not line:
                    return
                if not line.endswith(b"\n"):
                    # The rest of a long line is read and dropped, never left in the pipe.
                    while True:
                        rest = stream.readline(LINE_READ_BYTES)
                        if not rest or rest.endswith(b"\n"):
                            break
                kept = line.rstrip(b"\r\n")
                for secret in self._secrets:
                    kept = kept.replace(secret, b"[redacted]")
                with self._lines_lock:
                    self._seq += 1
                    self._lines.append((self._seq, kept[:LINE_BYTES]))
        except (OSError, ValueError):
            return
        finally:
            self._errors_done.set()

    def _write_input(self):
        fd = self._input
        broken = False
        while True:
            job = self._jobs.get()
            if job is None:
                break
            if broken:
                job.failed = True
                job.done.set()
                continue
            try:
                for chunk in job.chunks:
                    view = memoryview(chunk)
                    while view:
                        written = os.write(fd, view)
                        job.taken += written
                        view = view[written:]
            except OSError:
                job.failed = True
                broken = True
            job.done.set()
        try:
            self.process.stdin.close()
        except OSError:
            pass

    # what the worker's side asks of it

    def write(self, chunks):
        job = _Write(chunks)
        self._jobs.put(job)
        return job

    def mark(self):
        with self._lines_lock:
            return self._seq

    def lines_since(self, mark):
        with self._lines_lock:
            return [line for seq, line in self._lines if seq > mark]

    def kill(self):
        self.killed = True
        try:
            self.process.kill()
        except OSError:
            pass

    def end(self, limits):
        """Waits for the process to end, killing it past `limits.end`; its readers are let go
        past the same. How it ended, as the outcome record says it."""
        self._jobs.put(None)
        try:
            code = self.process.wait(limits.end)
        except subprocess.TimeoutExpired:
            self.kill()
            code = self.process.wait()
        self._errors_done.wait(limits.end)
        if self.killed:
            return "killed"
        if code < 0:
            return "signalled"
        return {"exited": code}


class LocalSide:
    """The worker's side: one worker process, one ordinary call at a time, beside it.

    `call(job, emit)` runs one whole queued request (`job`: `dispatch`, `not_after` on the
    system's clock, and `frames`, the bytes of the `load` and the request frames) and hands
    on, in order: `("verified", digest)` once the files are checked, `("hello", device,
    threads)`, each frame as `("frame", bytes)` under the ids the job's frames carry, and last
    `("outcome", document, lines)`. A failure before the files were checked hands on its
    outcome alone. `cancel(dispatch)` is the control input."""

    def __init__(self, models_dir, spawn=None, claims=None, limits=None, secrets=(), clock=time.time, log=say):
        self.models_dir = models_dir
        self._log = log
        self._spawn = spawn or spawner(models_dir)
        self._claims = claims or Claims()
        self._limits = limits or Limits()
        self._secrets = [secret.encode("utf-8") if isinstance(secret, str) else secret for secret in secrets]
        self._clock = clock
        self._calls = threading.Lock()
        self._state = threading.Lock()
        self._worker = None
        self._resident = None  # (pid, model, installation, settings, digest)
        self._running = None  # the dispatch of the running call
        self._exchange = None  # (worker, own id) of the frame in the worker
        self._cancel_at = None
        self._cancel_said = None  # when the running call's cancel came, on the clock
        # Cancels of a dispatch that is not running here yet, the last 64: a cancel that
        # comes while its call is on its way (on Modal, a control input that overtook the
        # call's input) is found when the call begins, and the call ends before any work.
        self._early = collections.OrderedDict()
        self._count = 0
        self._failed_starts = 0
        self._paused_until = 0.0
        self.device = None
        self.checked = {}
        # What was written to a worker, the last 64: its process id, the request's kind, its
        # model and, for a load, its settings. A diagnostic, and what the tests read.
        self.journal = collections.deque(maxlen=64)
        # What checks an installation's files before a load; a test may wrap it.
        self.check = check_files

    def close(self):
        """Kills the worker, if one runs, and waits for it."""
        worker = self._worker
        if worker is not None:
            worker.kill()
            worker.end(self._limits)

    # the control input

    def cancel(self, dispatch):
        """Cancels the running call when it is `dispatch`, and touches nothing otherwise:
        writes `cancel` on the worker's pipe at once, and sets the cancel's deadline."""
        with self._state:
            if dispatch is None:
                return False
            if self._running != dispatch:
                self._early[dispatch] = True
                while len(self._early) > 64:
                    self._early.popitem(last=False)
                self._log("cancel %s: no call of it runs here; kept for its call" % dispatch)
                return False
            if self._cancel_at is None:
                self._cancel_said = time.monotonic()
                self._cancel_at = self._cancel_said + self._limits.cancel
            exchange = self._exchange
        if exchange is not None:
            worker, own = exchange
            worker.write(proto.chunks({"type": "cancel", "id": own}))
            worker.frames.put(("wake",))
            self._log("cancel %s: cancel frame written to worker %d" % (dispatch, worker.pid))
        else:
            self._log("cancel %s: its call is before the worker; it ends there" % dispatch)
        return True

    # one call

    def call(self, job, emit):
        if not self._calls.acquire(blocking=False):
            emit(outcome("failed", "unavailable", admission="busy"))
            return
        ended = []

        def told(item):
            if item[0] == "outcome":
                ended.append(item[1])
            emit(item)

        try:
            self._call(job, told)
        finally:
            with self._state:
                said = self._cancel_said
                self._running = None
                self._exchange = None
                self._cancel_at = None
                self._cancel_said = None
            if said is not None:
                if ended:
                    came = ended[-1]["kind"] or "complete"
                    how = _how(ended[-1]["ended"])
                else:
                    came, how = "no outcome", "unknown"
                self._log("cancel %s: the call ended %.3f s after its cancel, %s, the worker %s" % (job["dispatch"], time.monotonic() - said, came, how))
            self._calls.release()

    def _call(self, job, emit):
        dispatch = job["dispatch"]
        if self._clock() > job["not_after"]:
            emit(outcome("failed", "timed_out", admission="not_after"))
            return
        # The call is this dispatch's from here, so a cancel that comes during the claim,
        # which is a round trip on a host's store, is kept and not lost.
        with self._state:
            self._running = dispatch
            if self._early.pop(dispatch, None) and self._cancel_at is None:
                self._cancel_said = time.monotonic()
                self._cancel_at = self._cancel_said + self._limits.cancel
        try:
            claimed = self._claims.claim(dispatch)
        except Unreachable:
            emit(outcome("failed", "unavailable", admission="claim_unreachable"))
            return
        if not claimed:
            emit(outcome("failed", "rejected", admission="replay"))
            return
        if self._cancelled():
            emit(outcome("failed", "cancelled"))
            return
        try:
            load, request = _job_frames(job["frames"])
        except proto.Refused:
            emit(outcome("failed", "malformed"))
            return
        if self._cancelled():
            emit(outcome("failed", "cancelled"))
            return
        key = (load["model"], load["installation"], tuple(sorted(load["settings"].items())))
        if self._worker is not None and self._worker.process.poll() is not None:
            # A worker that ended between two calls is noticed by the next, which starts
            # another.
            self._end_worker()
        resident = self._resident
        running = self._worker
        # Whether to load is decided here, beside the worker, by its process.
        same = running is not None and resident is not None and resident[0] == running.pid and resident[1:4] == key
        if same:
            digest = resident[4]
        else:
            try:
                digest, files = self.check(self.models_dir, load["model"], load["installation"])
            except Fault as fault:
                emit(outcome("failed", fault.fault if fault.fault in proto.FAULTS else "rejected"))
                return
            self.checked[load["installation"]] = files
        worker = None
        mark = 0
        try:
            worker = self._started()
            mark = worker.mark()
            if self._cancelled():
                emit(outcome("failed", "cancelled", lines=worker.lines_since(mark)))
                return
            emit(("verified", digest))
            emit(("hello", worker.device, worker.threads))
            if not same:
                if resident is not None and resident[0] == worker.pid:
                    self._resident = None
                    unload = {"type": "request", "id": "", "kind": "unload", "model": resident[1], "installation": resident[2]}
                    self._send(worker, unload, self._limits.unload, None)
                self._resident = None
            answer = self._send(worker, load, self._limits.load, emit)
            if answer["type"] == "error":
                emit(outcome("failed", answer["fault"], stage="answered", lines=worker.lines_since(mark)))
                return
            self._resident = (worker.pid,) + key + (digest,)
            answer = self._send(worker, request, self._limits.call, emit)
            if answer["type"] == "error":
                emit(outcome("failed", answer["fault"], stage="answered", lines=worker.lines_since(mark)))
                return
            emit(outcome("complete", stage="answered", lines=worker.lines_since(mark)))
        except _NotSent:
            emit(outcome("failed", "cancelled", lines=worker.lines_since(mark)))
        except _Ended as ended:
            how = self._end_worker()
            lines = worker.lines_since(mark) if worker is not None else []
            emit(outcome("failed", ended.kind, stage=ended.stage, ended=how, lines=lines))

    def _cancelled(self):
        with self._state:
            return self._cancel_at is not None

    def _started(self):
        """The worker, started with its hello answered when none runs."""
        if self._worker is not None:
            return self._worker
        if self._failed_starts >= STARTS_FAILED_MOST and time.monotonic() < self._paused_until:
            raise _Ended("unavailable", "not_sent")
        try:
            process = self._spawn()
        except OSError:
            self._start_failed()
            raise _Ended("unavailable", "not_sent") from None
        worker = _Process(process, self._secrets)
        self._worker = worker
        self._log("worker %d started" % worker.pid)
        conversation = "front.%d" % os.getpid()
        worker.write(proto.chunks({"type": "hello", "id": conversation, "version": proto.VERSION, "role": "whizbang"}))
        try:
            heard = worker.frames.get(timeout=self._limits.hello)
        except queue.Empty:
            worker.kill()
            raise _Ended("timed_out", "not_sent") from None
        if heard[0] != "message" or heard[1]["type"] != "hello" or heard[1]["role"] != "worker":
            worker.kill()
            raise _Ended("malformed" if heard[0] == "refused" else "unavailable", "not_sent")
        worker.device = heard[1]["device"]
        worker.threads = heard[1]["threads"]
        return worker

    def _start_failed(self):
        self._failed_starts += 1
        if self._failed_starts >= STARTS_FAILED_MOST:
            self._paused_until = time.monotonic() + self._limits.restart_pause

    def _end_worker(self):
        worker = self._worker
        self._worker = None
        self._resident = None
        if worker is None:
            return None
        if not worker.answered:
            self._start_failed()
        how = worker.end(self._limits)
        self._log("worker %d ended: %s; the next call starts another" % (worker.pid, _how(how)))
        return how

    def _send(self, worker, message, limit, emit):
        """One request in the worker, under an id of this side's own: written, and waited for
        on the clock within `limit`, a cancel and its deadline. Its events and its answer are
        handed on under the request's own id; the answer is returned."""
        with self._state:
            self._count += 1
            own = "%d.%d" % (worker.pid, self._count)
        theirs = message["id"]
        mine = dict(message, id=own)
        try:
            chunks = proto.chunks(mine)
        except (proto.Refused, ValueError):
            raise _Ended("malformed", "not_sent") from None
        worker.open[own] = proto.awaiting(mine)
        settings = message.get("settings")
        # The request is handed to the writer under the same lock a cancel reads the
        # exchange under, so a `cancel` frame is never queued ahead of its request.
        with self._state:
            cancel_at = self._cancel_at
            if cancel_at is None:
                self._exchange = (worker, own)
                self.journal.append((worker.pid, message["kind"], message["model"], None if settings is None else dict(settings)))
                write = worker.write(chunks)
        if cancel_at is not None:
            worker.open.pop(own, None)
            raise _NotSent()
        began = time.monotonic()
        deadline = began + limit
        while True:
            with self._state:
                cancel_at = self._cancel_at
            now = time.monotonic()
            until = deadline if cancel_at is None else min(deadline, cancel_at)
            if now >= until:
                worker.kill()
                stage = "unresolved" if write.taken else "not_sent"
                raise _Ended("cancelled" if cancel_at is not None and until == cancel_at else "timed_out", stage)
            try:
                heard = worker.frames.get(timeout=until - now)
            except queue.Empty:
                continue
            # How far the request got: a byte of it taken by the pipe is a request sent.
            stage = "unresolved" if write.taken else "not_sent"
            if heard[0] == "wake":
                continue
            if heard[0] == "ended":
                raise _Ended("cancelled" if cancel_at is not None else "unavailable", stage)
            if heard[0] == "refused":
                worker.kill()
                reason = heard[1]
                raise _Ended("too_large" if reason in ("header_too_large", "over_bound", "too_large") else "malformed", stage)
            said = heard[1]
            if said.get("id") != own or said["type"] not in ("event", "result", "error"):
                worker.kill()
                raise _Ended("malformed", stage)
            if said["type"] == "result":
                self.device = said["device"]
            if said["type"] in ("result", "error"):
                worker.answered = True
                self._failed_starts = 0
            if emit is not None:
                emit(("frame", proto.encode(dict(said, id=theirs))))
            if said["type"] != "event":
                return said


def _how(ended):
    """How a worker ended, as a line of the log says it."""
    if ended is None:
        return "lives on"
    if isinstance(ended, dict):
        return "exited %d" % ended["exited"]
    return ended


def _job_frames(frames):
    """The `load` and the request of a job, read strictly: nothing else, nothing after."""
    stream = io.BytesIO(frames)
    reader = proto.Reader(stream)
    load = reader.read()
    request = reader.read()
    if load is None or request is None or stream.read(1):
        raise proto.Refused("unexpected", "a job holds a load and a request and nothing else")
    _hold_to_shape(load, request)
    return load, request


def _hold_to_shape(load, request):
    if load["type"] != "request" or load["kind"] != "load":
        raise proto.Refused("unexpected", "the second frame must be a load")
    if request["type"] != "request" or request["kind"] != "segment":
        raise proto.Refused("unexpected", "the third frame must be a segment request")
    if (request["model"], request["installation"]) != (load["model"], load["installation"]):
        raise proto.Refused("unexpected", "the request names another model than its load")
    if request["id"] == load["id"]:
        raise proto.Refused("unexpected", "the load and the request share an id")


# --- a worker's side on another machine: the bridge (task D1) ------------------------------
#
# A host that puts the card on another machine than the front (on Modal, a GPU function
# beside a CPU one) runs a `LocalSide` there and carries each job across as plain data. The
# three pieces below name no host: `deploy/modal/sam3_host.py` hands them the host's calls.


class StoreClaims:
    """Claims kept in a host's own store, which outlives a container and a deploy:
    `put_if_absent(key, value)` is an atomic put that answers True when the key was new. Any
    failure of the store is `Unreachable`, so the claim fails closed."""

    def __init__(self, put_if_absent, clock=time.time):
        self._put = put_if_absent
        self._clock = clock

    def claim(self, dispatch):
        try:
            added = self._put(dispatch, int(self._clock()))
        except Exception:
            raise Unreachable() from None
        return added is True


# How long the card's side waits for the next item of a call it relays before it looks
# again whether the call's thread is still alive.
RELAY_LOOK_S = 1.0


def relay(side, job):
    """On the card's side: runs one job on `side` and yields what it hands on, in order, as
    plain data, ending with its outcome. The call runs on a thread of its own, so a consumer
    that goes away (a front replaced in the middle of a call) leaves the call to end at its
    own limits; what it makes after that is dropped."""
    items = queue.SimpleQueue()

    def run():
        try:
            side.call(job, items.put)
        except Exception:
            items.put(outcome("failed", "unavailable", stage="unresolved"))
        finally:
            items.put(None)

    thread = threading.Thread(target=run, name="front-relay", daemon=True)
    thread.start()
    while True:
        try:
            item = items.get(timeout=RELAY_LOOK_S)
        except queue.Empty:
            if thread.is_alive():
                continue
            item = None
        if item is None:
            yield outcome("failed", "unavailable", stage="unresolved")
            return
        yield item
        if item[0] == "outcome":
            return


class Bridged:
    """On the front's side: the worker's side the front is given when the card is
    elsewhere. `call_remote(job)` is the host's call of the card's function, an iterable of
    what `relay` yields there; `cancel_remote(dispatch)` is its control input. A call whose
    items end before its outcome, or whose host call fails, ends `unavailable` and
    unresolved: the job may have begun there, and whizbang charges it at its bound.

    A cancel is handed to the card's control input only for a dispatch this side has handed
    on and not yet seen end: any other is answered here, as touching nothing, so a cancel can
    never start the card's container, whose time no attempt's bound holds. Such a cancel is
    kept, the last 64, as `LocalSide` keeps one: through a host's proxy a cancel's few bytes
    can reach the front before its request's body is read whole, and that request, when it
    comes, ends `cancelled` here before the host is called. Each cancel is a line of the
    log: what became of it, and for one the host's control input raised, the exception's
    type alone, since its message may name the host's account or an address."""

    def __init__(self, call_remote, cancel_remote, log=say):
        self._call = call_remote
        self._cancel = cancel_remote
        self._log = log
        self._open = set()
        self._early = collections.OrderedDict()
        self._lock = threading.Lock()
        self.checked = {}
        self.device = None

    def call(self, job, emit):
        dispatch = job["dispatch"]
        try:
            installation = _job_frames(job["frames"])[0]["installation"]
        except proto.Refused:
            emit(outcome("failed", "malformed"))
            return
        with self._lock:
            early = self._early.pop(dispatch, None)
            if not early:
                self._open.add(dispatch)
        if early:
            self._log("cancel %s: kept from before its call, which ends here before the host is called" % dispatch)
            emit(outcome("failed", "cancelled"))
            return
        try:
            self._relayed(job, installation, emit)
        finally:
            with self._lock:
                self._open.discard(dispatch)

    def _relayed(self, job, installation, emit):
        try:
            for item in self._call(job):
                if item[0] == "verified":
                    self.checked[installation] = True
                elif item[0] == "hello":
                    self.device = item[1]
                emit(item)
                if item[0] == "outcome":
                    return
        except Exception:
            pass
        emit(outcome("failed", "unavailable", stage="unresolved"))

    def cancel(self, dispatch):
        if dispatch is None:
            return False
        with self._lock:
            if dispatch not in self._open:
                self._early[dispatch] = True
                while len(self._early) > 64:
                    self._early.popitem(last=False)
                self._log("cancel %s: not open here; the host is not asked, and the cancel is kept for its call" % dispatch)
                return False
        try:
            found = bool(self._cancel(dispatch))
        except Exception as error:
            self._log("cancel %s: the host's cancel raised %s" % (dispatch, type(error).__name__))
            return False
        self._log("cancel %s: handed to the host's control input, which %s" % (dispatch, "cancelled its call" if found else "found no call of it running"))
        return found

    def close(self):
        pass


# --- the front ------------------------------------------------------------------------------


class _Entry:
    """A request the front holds: its job, its mailbox and the pipe that wakes its handler."""

    def __init__(self, dispatch, job):
        self.dispatch = dispatch
        self.job = job
        self.state = "waiting"
        self.mail = queue.SimpleQueue()
        self.cancelled = False
        self.wake_r, self.wake_w = os.pipe()
        self._lock = threading.Lock()
        self._closed = False

    def put(self, item):
        self.mail.put(item)
        with self._lock:
            if self._closed:
                return
            try:
                os.write(self.wake_w, b"x")
            except OSError:
                pass

    def close(self):
        with self._lock:
            self._closed = True
            for fd in (self.wake_r, self.wake_w):
                try:
                    os.close(fd)
                except OSError:
                    pass


class _Request:
    """What the classifier read of a connection: its method, path and headers, and the bytes
    of its body it has read already."""

    __slots__ = ("conn", "method", "path", "headers", "leftover", "accepted", "body")

    def __init__(self, conn, method, path, headers, leftover, accepted):
        self.conn = conn
        self.method = method
        self.path = path
        self.headers = headers
        self.leftover = leftover
        self.accepted = accepted
        self.body = None


class _Body(io.RawIOBase):
    """A request's body, read as the frames reader asks for it: held to its length and to its
    rate, at least `rate` bytes a second after its first `grace` seconds."""

    def __init__(self, conn, leftover, length, limits):
        super().__init__()
        self.conn = conn
        self.leftover = leftover
        self.left = length
        self.taken = 0
        self.began = time.monotonic()
        self.limits = limits

    def readable(self):
        return True

    def readinto(self, buffer):
        if self.left <= 0:
            return 0
        want = min(len(buffer), self.left)
        if self.leftover:
            count = min(want, len(self.leftover))
            buffer[:count] = self.leftover[:count]
            self.leftover = self.leftover[count:]
        else:
            deadline = self.began + self.limits.body_grace + (self.taken + 1) / self.limits.body_rate
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("the body came too slowly")
            self.conn.settimeout(remaining)
            count = self.conn.recv_into(memoryview(buffer)[:want])
            if count == 0:
                return 0
        self.left -= count
        self.taken += count
        return count


class Front:
    """The routes, the classifier, the handlers and the queue of 4 (ADR 0046)."""

    def __init__(self, side, models_dir, token, limits=None, host_limit_s=HOST_LIMIT_DEFAULT, tls=None, clock=time.time, log=say):
        self.side = side
        self._log = log
        self.models_dir = models_dir
        self.token = token.encode("ascii")
        self.limits = limits or Limits()
        self.host_limit_s = host_limit_s
        self.tls = tls
        self.clock = clock
        self.device = None
        self._queue = collections.deque()
        self._by_dispatch = {}
        self._cond = threading.Condition()
        self._closed = threading.Event()
        self._request_free = threading.Semaphore(REQUEST_HANDLERS)
        self._control_free = threading.Semaphore(CONTROL_HANDLERS)
        self._request_jobs = queue.SimpleQueue()
        self._control_jobs = queue.SimpleQueue()
        self._listener = None
        self._threads = []
        self._wake_r, self._wake_w = os.pipe()

    # starting and stopping

    def start(self, listener):
        """Serves on a listening socket: the classifier, the runner of the queue and the 8
        handlers, each a daemon thread."""
        listener.setblocking(False)
        self._listener = listener
        targets = [("front-classifier", self._classify), ("front-runner", self._run_queue)]
        targets += [("front-request-%d" % n, self._request_handler) for n in range(REQUEST_HANDLERS)]
        targets += [("front-control-%d" % n, self._control_handler) for n in range(CONTROL_HANDLERS)]
        for name, target in targets:
            thread = threading.Thread(target=target, name=name, daemon=True)
            self._threads.append(thread)
            thread.start()

    def close(self):
        self._closed.set()
        try:
            os.write(self._wake_w, b"x")
        except OSError:
            pass
        with self._cond:
            self._cond.notify_all()
        for _ in range(REQUEST_HANDLERS):
            self._request_jobs.put(None)
        for _ in range(CONTROL_HANDLERS):
            self._control_jobs.put(None)

    # the queue

    def admit(self, dispatch, job):
        """A place in the queue, or `None` when 4 are held or the dispatch is held already."""
        with self._cond:
            if len(self._queue) >= QUEUE_MOST or dispatch in self._by_dispatch:
                return None
            entry = _Entry(dispatch, job)
            self._queue.append(entry)
            self._by_dispatch[dispatch] = entry
            self._cond.notify_all()
            return entry

    def cancel(self, dispatch):
        """A cancel by dispatch id: a waiting request is taken off the queue and ended; any
        other is the worker's side's to find, whether this front holds it or not."""
        with self._cond:
            entry = self._by_dispatch.get(dispatch)
            if entry is not None and entry.state == "waiting":
                self._queue.remove(entry)
                del self._by_dispatch[dispatch]
                entry.state = "done"
                entry.put(outcome("failed", "cancelled"))
                self._log("cancel %s: taken off the queue before its turn" % dispatch)
                return
            if entry is not None:
                entry.cancelled = True
        self._log("cancel %s: handed to the worker's side" % dispatch)
        self.side.cancel(dispatch)

    def _run_queue(self):
        while not self._closed.is_set():
            with self._cond:
                while not self._closed.is_set() and not any(e.state == "waiting" for e in self._queue):
                    self._cond.wait()
                if self._closed.is_set():
                    return
                entry = next(e for e in self._queue if e.state == "waiting")
                entry.state = "running"
                cancelled = entry.cancelled
            # The requests waiting when the worker's end was told: a restart fails these,
            # and not one that came after it, which the next worker answers.
            ended = []

            def emit(item, entry=entry, ended=ended):
                if item[0] == "outcome" and item[1]["ended"] is not None:
                    with self._cond:
                        ended[:] = [e for e in self._queue if e.state == "waiting"]
                entry.put(item)

            try:
                if cancelled:
                    entry.put(outcome("failed", "cancelled"))
                elif self.clock() > entry.job["not_after"]:
                    # It waited in the queue past its host's limit: answered here, and the
                    # worker's side (on a host, a card) is never woken for it.
                    entry.put(outcome("failed", "timed_out", admission="not_after"))
                else:
                    self.side.call(entry.job, emit)
            except Exception:
                entry.put(outcome("failed", "unavailable", stage="unresolved"))
            with self._cond:
                self._queue.remove(entry)
                self._by_dispatch.pop(entry.dispatch, None)
                entry.state = "done"
                # A restart fails every request that was waiting here when it was told,
                # and replays nothing.
                for waiting in ended:
                    if waiting in self._queue and waiting.state == "waiting":
                        self._queue.remove(waiting)
                        self._by_dispatch.pop(waiting.dispatch, None)
                        waiting.state = "done"
                        waiting.put(outcome("failed", "unavailable"))

    # the classifier

    def _classify(self):
        selector = selectors.DefaultSelector()
        selector.register(self._listener, selectors.EVENT_READ, None)
        selector.register(self._wake_r, selectors.EVENT_READ, "wake")
        waiting = collections.OrderedDict()
        try:
            while not self._closed.is_set():
                now = time.monotonic()
                timeout = None
                if waiting:
                    oldest = next(iter(waiting.values()))
                    timeout = max(0.0, oldest["accepted"] + self.limits.classify - now)
                for key, mask in selector.select(timeout):
                    if key.data == "wake":
                        continue
                    if key.data is None:
                        self._accept(selector, waiting)
                    else:
                        self._advance(selector, waiting, key.data, mask)
                now = time.monotonic()
                for pending in list(waiting.values()):
                    if pending["accepted"] + self.limits.classify <= now:
                        self._drop(selector, waiting, pending)
        finally:
            for pending in list(waiting.values()):
                self._drop(selector, waiting, pending)
            selector.close()
            _close(self._listener)

    def _accept(self, selector, waiting):
        while True:
            try:
                conn, _ = self._listener.accept()
            except (BlockingIOError, InterruptedError):
                return
            except OSError:
                return
            conn.setblocking(False)
            if len(waiting) >= WAITING_MOST:
                self._drop(selector, waiting, next(iter(waiting.values())))
            pending = {"conn": conn, "accepted": time.monotonic(), "buf": b"", "stage": "head", "request": None}
            if self.tls is not None:
                try:
                    pending["conn"] = self.tls.wrap_socket(conn, server_side=True, do_handshake_on_connect=False)
                except (OSError, ValueError):
                    conn.close()
                    continue
                pending["stage"] = "handshake"
            waiting[id(pending)] = pending
            selector.register(pending["conn"], selectors.EVENT_READ, pending)
            self._advance(selector, waiting, pending, selectors.EVENT_READ)

    def _drop(self, selector, waiting, pending, keep=False):
        waiting.pop(id(pending), None)
        try:
            selector.unregister(pending["conn"])
        except (KeyError, ValueError, OSError):
            pass
        if not keep:
            try:
                pending["conn"].close()
            except OSError:
                pass

    def _advance(self, selector, waiting, pending, mask):
        conn = pending["conn"]
        try:
            if pending["stage"] == "handshake":
                try:
                    conn.do_handshake()
                except ssl.SSLWantReadError:
                    selector.modify(conn, selectors.EVENT_READ, pending)
                    return
                except ssl.SSLWantWriteError:
                    selector.modify(conn, selectors.EVENT_WRITE, pending)
                    return
                pending["stage"] = "head"
                selector.modify(conn, selectors.EVENT_READ, pending)
            while True:
                try:
                    data = conn.recv(8192)
                except (BlockingIOError, ssl.SSLWantReadError, ssl.SSLWantWriteError):
                    return
                if not data:
                    self._drop(selector, waiting, pending)
                    return
                pending["buf"] += data
                if pending["stage"] == "head":
                    end = pending["buf"].find(b"\r\n\r\n")
                    if end < 0:
                        if len(pending["buf"]) > HEAD_MOST:
                            self._answer_now(selector, waiting, pending, 431)
                            return
                        continue
                    if end + 4 > HEAD_MOST:
                        self._answer_now(selector, waiting, pending, 431)
                        return
                    head, pending["buf"] = pending["buf"][:end], pending["buf"][end + 4 :]
                    if not self._route(selector, waiting, pending, head):
                        return
                if pending["stage"] == "body":
                    request = pending["request"]
                    want = int(request.headers["content-length"])
                    if len(pending["buf"]) >= want:
                        if len(pending["buf"]) > want:
                            self._answer_now(selector, waiting, pending, 400)
                            return
                        request.body = pending["buf"]
                        self._hand_control(selector, waiting, pending, "cancel")
                        return
        except (OSError, ssl.SSLError, ValueError):
            self._drop(selector, waiting, pending)

    def _route(self, selector, waiting, pending, head):
        """Reads the line and headers and decides where the connection goes. False when it was
        answered or handed on."""
        try:
            method, path, headers = _parse_head(head)
        except ValueError:
            self._answer_now(selector, waiting, pending, 400)
            return False
        routes = {"/pack/1/request": "POST", "/pack/1/cancel": "POST", "/pack/1/health": "GET"}
        if path not in routes:
            self._answer_now(selector, waiting, pending, 404)
            return False
        if method != routes[path]:
            self._answer_now(selector, waiting, pending, 405)
            return False
        pending["request"] = _Request(pending["conn"], method, path, headers, b"", pending["accepted"])
        if path == "/pack/1/health":
            self._hand_control(selector, waiting, pending, "health")
            return False
        if not self._authorized(headers):
            self._answer_now(selector, waiting, pending, 401)
            return False
        dispatch = headers.get("whizbang-dispatch", "")
        if not DISPATCH.match(dispatch):
            self._answer_now(selector, waiting, pending, 400)
            return False
        if "transfer-encoding" in headers or "content-length" not in headers:
            self._answer_now(selector, waiting, pending, 411)
            return False
        length = headers["content-length"]
        if not (length.isascii() and length.isdigit() and len(length) <= 12):
            self._answer_now(selector, waiting, pending, 400)
            return False
        most = CONTROL_BODY_MOST if path == "/pack/1/cancel" else BODY_MOST
        if int(length) > most:
            self._answer_now(selector, waiting, pending, 413)
            return False
        if path == "/pack/1/cancel":
            pending["stage"] = "body"
            return True
        if not self._request_free.acquire(blocking=False):
            self._answer_now(selector, waiting, pending, 503)
            return False
        pending["request"].leftover = pending["buf"]
        self._drop(selector, waiting, pending, keep=True)
        self._request_jobs.put(pending["request"])
        return False

    def _hand_control(self, selector, waiting, pending, what):
        if not self._control_free.acquire(blocking=False):
            self._answer_now(selector, waiting, pending, 503)
            return
        self._drop(selector, waiting, pending, keep=True)
        self._control_jobs.put((what, pending["request"]))

    def _authorized(self, headers):
        given = headers.get("authorization", "")
        if not given.startswith("Bearer "):
            hmac.compare_digest(b"", self.token)
            return False
        return hmac.compare_digest(given[len("Bearer ") :].encode("utf-8", "replace"), self.token)

    def _answer_now(self, selector, waiting, pending, status):
        """A few bytes, written without waiting, then the connection closed."""
        conn = pending["conn"]
        try:
            conn.send(_plain(status))
        except (OSError, ssl.SSLError, ValueError):
            pass
        self._drop(selector, waiting, pending)

    # the handlers

    def _control_handler(self):
        while True:
            job = self._control_jobs.get()
            if job is None:
                return
            what, request = job
            conn = request.conn
            try:
                conn.setblocking(True)
                conn.settimeout(self.limits.control_write)
                if what == "health":
                    body = json.dumps(self.health(), sort_keys=True).encode("ascii")
                    conn.sendall(_plain(200, body, b"application/json"))
                else:
                    if _cancel_body(request.body):
                        self.cancel(request.headers["whizbang-dispatch"])
                        conn.sendall(_plain(204))
                    else:
                        conn.sendall(_plain(400))
            except (OSError, ssl.SSLError, ValueError):
                pass
            finally:
                _close(conn)
                self._control_free.release()

    def _request_handler(self):
        while True:
            request = self._request_jobs.get()
            if request is None:
                return
            try:
                self._serve_request(request)
            except (OSError, ssl.SSLError, ValueError):
                pass
            finally:
                _close(request.conn)
                self._request_free.release()

    def _serve_request(self, request):
        conn = request.conn
        conn.setblocking(True)
        length = int(request.headers["content-length"])
        body = _Body(conn, request.leftover, length, self.limits)
        reader = proto.Reader(body)
        try:
            hello = reader.read()
            load = reader.read()
            call = reader.read()
            if hello is None or hello["type"] != "hello" or hello["role"] != "whizbang":
                raise proto.Refused("unexpected", "the first frame must be whizbang's hello")
            if load is None or call is None:
                raise proto.Refused("truncated", "the body ended before its frames")
            _hold_to_shape(load, call)
        except (proto.Refused, TimeoutError):
            conn.settimeout(self.limits.control_write)
            conn.sendall(_plain(400))
            return
        if body.left:
            conn.settimeout(self.limits.control_write)
            conn.sendall(_plain(400))
            return
        dispatch = request.headers["whizbang-dispatch"]
        job = {
            "dispatch": dispatch,
            "not_after": self.clock() + self.host_limit_s,
            "frames": proto.encode(load) + proto.encode(call),
        }
        conn.settimeout(self.limits.write)
        entry = self.admit(dispatch, job)
        if entry is None:
            # Decided at once: the headers, and the outcome record alone.
            _, document, lines = outcome("failed", "unavailable", admission="busy")
            conn.sendall(STREAMED + _chunk(record_bytes(document, lines)) + LAST_CHUNK)
            return
        try:
            self._follow(conn, entry, hello["id"], load, call)
        finally:
            entry.close()

    def _follow(self, conn, entry, conversation, load, call):
        """Sends the answer's headers and a first keep-alive at once, then hands what the
        worker's side makes on to the client as it comes, with a keep-alive whenever nothing
        was written for `Limits.keep_alive`; and cancels the request when the client is
        gone."""
        opened = {load["id"]: proto.awaiting(load), call["id"]: proto.awaiting(call)}
        ended = False
        selector = selectors.DefaultSelector()
        try:
            selector.register(entry.wake_r, selectors.EVENT_READ, "mail")
            selector.register(conn, selectors.EVENT_READ, "conn")
            try:
                # The headers wait for nothing of the worker's side; the first keep-alive
                # is a byte of the body, so a proxy that holds the headers until the body
                # begins passes them on.
                conn.sendall(STREAMED + _chunk(KEEP_ALIVE))
            except (OSError, ssl.SSLError):
                return
            quiet_until = time.monotonic() + self.limits.keep_alive
            # The latest the outcome may come: the request's `not_after` and the overrun.
            give_up = time.monotonic() + max(0.0, entry.job["not_after"] - self.clock()) + self.limits.overrun
            while True:
                now = time.monotonic()
                if now >= give_up:
                    # The worker's side has not ended it by its limit: it is ended here, as
                    # timed out, and cancelled on the side.
                    _, document, lines = outcome("failed", "timed_out", stage="unresolved")
                    try:
                        conn.sendall(_chunk(record_bytes(document, lines)) + LAST_CHUNK)
                    except (OSError, ssl.SSLError):
                        pass
                    self._log("request %s: no outcome by its limit; answered timed_out and cancelled" % entry.dispatch)
                    return
                for key, _ in selector.select(max(0.0, min(quiet_until, give_up) - now)):
                    if key.data == "conn" and self._gone(conn):
                        return
                    if key.data == "mail":
                        os.read(entry.wake_r, 4096)
                wrote = False
                while True:
                    try:
                        item = entry.mail.get_nowait()
                    except queue.Empty:
                        break
                    wrote = True
                    try:
                        if item[0] == "verified":
                            conn.sendall(_chunk(identity_bytes(item[1])))
                        elif item[0] == "hello":
                            frame = proto.encode(
                                {"type": "hello", "id": conversation, "version": proto.VERSION, "role": "worker", "device": item[1], "threads": item[2]}
                            )
                            conn.sendall(_chunk(frame))
                        elif item[0] == "frame":
                            message = proto.Reader(io.BytesIO(item[1]), opened).read()
                            if message["type"] == "result":
                                self.device = message["device"]
                            conn.sendall(_chunk(item[1]))
                        else:
                            ended = True
                            conn.sendall(_chunk(record_bytes(item[1], item[2])) + LAST_CHUNK)
                            return
                    except (OSError, ssl.SSLError, proto.Refused):
                        # A client that takes nothing within the write's limit, or is gone, or
                        # a frame from the worker's side that breaks `pack/1`.
                        return
                now = time.monotonic()
                if wrote:
                    quiet_until = now + self.limits.keep_alive
                elif now >= quiet_until:
                    try:
                        conn.sendall(_chunk(KEEP_ALIVE))
                    except (OSError, ssl.SSLError):
                        return
                    quiet_until = now + self.limits.keep_alive
        finally:
            selector.close()
            if not ended:
                # Whatever ended the answer before its outcome, the request is cancelled:
                # no entry is left queued with nobody to answer.
                self.cancel(entry.dispatch)

    def _gone(self, conn):
        """True when the client closed its end, or sent what no request of this route has."""
        try:
            conn.setblocking(False)
            try:
                data = conn.recv(1)
            except (BlockingIOError, ssl.SSLWantReadError, ssl.SSLWantWriteError):
                return False
            except (OSError, ssl.SSLError):
                return True
            return True
        finally:
            try:
                conn.setblocking(True)
                conn.settimeout(self.limits.write)
            except OSError:
                pass

    # the health document

    def health(self):
        """`pack-host/1`'s health: each installation of the models directory, with each
        file's SHA-256 as its record states it and whether this front's side checked it, and
        the device the worker last reported. It starts no worker."""
        found = []
        models = os.path.join(self.models_dir, "models")
        try:
            names = sorted(os.listdir(models))
        except OSError:
            names = []
        checked = getattr(self.side, "checked", {})
        for model in names:
            folder = os.path.join(models, model, "installations")
            try:
                identities = sorted(os.listdir(folder))
            except OSError:
                continue
            for hexdigits in identities:
                if len(found) >= HEALTH_MOST:
                    break
                identity = "sha256:" + hexdigits
                try:
                    record = installed.record_of(Installation(self.models_dir, model, identity))
                except Fault:
                    continue
                if record is None:
                    continue
                found.append(
                    {
                        "model": model,
                        "installation": identity,
                        "checked": identity in checked,
                        "files": [{"name": file["name"], "sha256": file["sha256"]} for file in record["files"]],
                    }
                )
        device = getattr(self.side, "device", None) or self.device
        return {"format": FORMAT, "protocol": proto.VERSION, "models": found, "device": device}


def _parse_head(head):
    """The request line and headers: `HTTP/1.1` only, each header once, names lower-cased."""
    lines = head.split(b"\r\n")
    parts = lines[0].split(b" ")
    if len(parts) != 3 or parts[2] != b"HTTP/1.1":
        raise ValueError("not a request line")
    method = parts[0].decode("ascii")
    path = parts[1].decode("ascii")
    headers = {}
    for line in lines[1:]:
        name, colon, value = line.partition(b":")
        if not colon or not _HEADER_NAME.match(name):
            raise ValueError("not a header")
        key = name.decode("ascii").lower()
        if key in headers:
            raise ValueError("a header given twice")
        headers[key] = value.strip(b" \t").decode("latin-1")
    return method, path, headers


def _cancel_body(body):
    """A cancel's body: whizbang's hello, then one `cancel` frame, and nothing after."""
    stream = io.BytesIO(body or b"")
    try:
        reader = proto.Reader(stream)
        hello = reader.read()
        cancel = reader.read()
    except proto.Refused:
        return False
    if hello is None or hello["type"] != "hello" or hello["role"] != "whizbang":
        return False
    return cancel is not None and cancel["type"] == "cancel" and not stream.read(1)


_REASONS = {
    200: b"OK",
    204: b"No Content",
    400: b"Bad Request",
    401: b"Unauthorized",
    404: b"Not Found",
    405: b"Method Not Allowed",
    411: b"Length Required",
    413: b"Content Too Large",
    431: b"Request Header Fields Too Large",
    503: b"Service Unavailable",
}


def _plain(status, body=b"", kind=b"text/plain"):
    head = b"HTTP/1.1 %d %s\r\nConnection: close\r\nContent-Length: %d\r\n" % (status, _REASONS[status], len(body))
    if body:
        head += b"Content-Type: " + kind + b"\r\n"
    return head + b"\r\n" + body


# The headers of every answer of the request route that was read whole: the same for each,
# so they wait for nothing. Whatever they once named that is known only later is in the body.
STREAMED = (
    b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/octet-stream\r\n"
    b"Transfer-Encoding: chunked\r\n\r\n"
)
LAST_CHUNK = b"0\r\n\r\n"


def _chunk(data):
    return b"%x\r\n" % len(data) + data + b"\r\n"


def _close(conn):
    try:
        conn.close()
    except OSError:
        pass


# --- the command line ------------------------------------------------------------------------


def parse(argv):
    """The front's options; `Usage` for a command line it does not take."""
    once = {
        "--listen": "listen",
        "--models-dir": "models_dir",
        "--token-env": "token_env",
        "--worker": "worker",
        "--host-limit-s": "host_limit_s",
        "--keep-alive-ms": "keep_alive_ms",
        "--tls-cert": "tls_cert",
        "--tls-key": "tls_key",
    }
    given = {"redact_env": []}
    args = list(argv)
    while args:
        name = args.pop(0)
        if not args:
            raise Usage("%s needs a value" % name)
        value = args.pop(0)
        if name == "--redact-env":
            given["redact_env"].append(value)
            continue
        if name not in once:
            raise Usage("an option the front does not take")
        if once[name] in given:
            raise Usage("%s is given twice" % name)
        given[once[name]] = value
    for needed in ("listen", "models_dir", "token_env"):
        if needed not in given:
            raise Usage("--%s is needed" % needed.replace("_", "-"))
    if not os.path.isabs(given["models_dir"]):
        raise Usage("--models-dir must name an absolute folder")
    host, colon, port = given["listen"].rpartition(":")
    if not colon or not (port.isascii() and port.isdigit() and 0 <= int(port) <= 65535):
        raise Usage("--listen is a host and a port, as 127.0.0.1:8080")
    given["listen"] = (host.strip("[]"), int(port))
    limit = given.get("host_limit_s", str(HOST_LIMIT_DEFAULT))
    if not (limit.isascii() and limit.isdigit() and 1 <= int(limit) <= 86_400):
        raise Usage("--host-limit-s is whole seconds from 1 to 86400")
    given["host_limit_s"] = int(limit)
    every = given.get("keep_alive_ms", str(int(KEEP_ALIVE_S * 1000)))
    if not (every.isascii() and every.isdigit() and KEEP_ALIVE_MS_LEAST <= int(every) <= KEEP_ALIVE_MS_MOST):
        raise Usage("--keep-alive-ms is whole milliseconds from %d to %d" % (KEEP_ALIVE_MS_LEAST, KEEP_ALIVE_MS_MOST))
    given["keep_alive_ms"] = int(every)
    if ("tls_cert" in given) != ("tls_key" in given):
        raise Usage("--tls-cert and --tls-key are given together")
    given.setdefault("worker", "local")
    return given


def token_of(environ, name):
    """The token, read once from the variable named: at least 32 printable ASCII characters."""
    token = environ.get(name)
    if token is None or len(token) < TOKEN_LEAST or not all(" " < c <= "~" for c in token):
        raise Usage("the token's variable holds no token of at least %d printable characters" % TOKEN_LEAST)
    return token


def local(models_dir, options):
    """The worker's side of `--worker local`: in the front's process."""
    return LocalSide(models_dir, secrets=options.get("secrets", ()))


def side_of(worker, models_dir, options):
    """The worker's side: `local`'s, or what a `module:function` makes of the models
    directory and the front's options."""
    if worker == "local":
        return local(models_dir, options)
    module, colon, function = worker.partition(":")
    if not colon or not module or not function:
        raise Usage("--worker is local or module:function")
    made = getattr(importlib.import_module(module), function, None)
    if not callable(made):
        raise Usage("--worker names nothing that can be called")
    return made(models_dir=models_dir, options=options)


def main(argv=None, environ=None):
    argv = sys.argv[1:] if argv is None else argv
    environ = os.environ if environ is None else environ
    try:
        options = parse(argv)
        token = token_of(environ, options["token_env"])
        secrets = [token] + [environ[name] for name in options["redact_env"] if environ.get(name)]
        options["secrets"] = secrets
        side = side_of(options["worker"], options["models_dir"], options)
    except Usage as usage:
        sys.stderr.write("whizbang_pack.front: %s\n" % usage)
        return 2
    tls = None
    if "tls_cert" in options:
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(options["tls_cert"], options["tls_key"])
    host, port = options["listen"]
    listener = socket.create_server((host, port), family=socket.AF_INET6 if ":" in host else socket.AF_INET)
    limits = Limits(keep_alive=options["keep_alive_ms"] / 1000.0)
    front = Front(side, options["models_dir"], token, limits=limits, host_limit_s=options["host_limit_s"], tls=tls)
    front.start(listener)
    sys.stderr.write("whizbang_pack.front: listening on %s:%d\n" % listener.getsockname()[:2])
    sys.stderr.flush()
    stop = threading.Event()
    for each in (signal.SIGTERM, signal.SIGINT):
        signal.signal(each, lambda *_: stop.set())
    while not stop.wait(3600):
        pass
    front.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
