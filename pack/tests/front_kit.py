"""What the front's tests share: installations of the fake models with their records, a
front on loopback with the real worker behind it, and a client that reads an answer as
whizbang's kind `remote` will (ADR 0046). Not a test file: `run.py` finds only `test_*.py`.

Every worker a test starts is a child process of the pack's own entry point, its
environment cleared to the variables `scripts/test-pack.sh` hands on, killed and waited for
in the test's cleanup. Every wait is bounded by `kit.WAIT_SECONDS`."""

import hashlib
import io
import json
import os
import shutil
import socket
import tempfile
import threading
import time

import kit
from whizbang_pack import front, installed, proto

TOKEN = "front-test-token-0123456789abcdefghij"
WIDTH, HEIGHT = 8, 6
BINDING = "sha256:" + "92b2fa58028958317e408bd84ecfa70f5ee35b121991dbc232c49d166353708b"
MODELS = ("fake-ok", "fake-mps", "fake-say", "fake-wait", "fake-deaf", "fake-exit")
# The worker of the tests that need `fake-say`, a model of the tests' own (`say_worker`).
SAY_WORKER = "say_worker"


def small_limits(**given):
    """Deadlines short enough for the pack's ten seconds, the others the front's own."""
    values = {"cancel": 0.05, "end": 2.0, "hello": kit.WAIT_SECONDS, "load": kit.WAIT_SECONDS, "call": kit.WAIT_SECONDS}
    values.update(given)
    return front.Limits(**values)


def picture(width=WIDTH, height=HEIGHT):
    """Red 255 in the left half and 0 in the right, so fake-ok's phrase finds the left half."""
    row = bytearray()
    for x in range(width):
        row += bytes((255 if x < width // 2 else 0, 10, 20, 255))
    return bytes(row) * height


def install(models_dir, model, files=None):
    """An installation of a fake model with its record, as `whizbang models get` writes one;
    its identity."""
    files = {"weights.bin": ("weights of " + model).encode("ascii")} if files is None else files
    record = {
        "format": installed.INSTALLATION_FORMAT,
        "model": model,
        "catalogue_revision": "1",
        "adapter": model,
        "adapter_revision": "fake-1",
        "files": [
            {"name": name, "size": len(data), "sha256": "sha256:" + hashlib.sha256(data).hexdigest()}
            for name, data in sorted(files.items())
        ],
    }
    identity = installed.identity_of(record)
    folder = os.path.join(models_dir, "models", model, "installations", identity[len("sha256:") :])
    os.makedirs(os.path.join(folder, "files"))
    with open(os.path.join(folder, "RECORD"), "w") as written:
        json.dump(record, written)
    for name, data in files.items():
        with open(os.path.join(folder, "files", name), "wb") as written:
            written.write(data)
    return identity, record


class Models:
    """A models directory with each fake model installed once."""

    def __init__(self, test):
        self.dir = tempfile.mkdtemp(prefix="front-models-")
        test.addCleanup(shutil.rmtree, self.dir, True)
        self.identity = {}
        self.record = {}
        for model in MODELS:
            self.identity[model], self.record[model] = install(self.dir, model)

    def file(self, model, name="weights.bin"):
        identity = self.identity[model][len("sha256:") :]
        return os.path.join(self.dir, "models", model, "installations", identity, "files", name)


# One cache of bytecode for every worker the tests start, in place of the `-B` of a worker
# the front starts, so each start does not compile the pack again. Nothing is written under
# the pack; `test_front` removes the cache when its tests are done (`tearDownModule`).
_CACHE = tempfile.mkdtemp(prefix="front-pycache-")


class Spawns:
    """The spawner of a side, counting each worker it starts."""

    def __init__(self, models_dir, module="whizbang_pack.worker", more=()):
        self.count = 0
        self.pids = []
        self._spawn = front.spawner(models_dir, module, more, environment=kit.child_environment(), sources=(kit.SRC, kit.HERE), flags=("-I", "-X", "pycache_prefix=" + _CACHE))

    def __call__(self):
        self.count += 1
        process = self._spawn()
        self.pids.append(process.pid)
        return process


def local_side(test, models, limits=None, module="whizbang_pack.worker", more=(), claims=None, secrets=()):
    spawns = Spawns(models.dir, module, more)
    side = front.LocalSide(models.dir, spawn=spawns, claims=claims, limits=limits or small_limits(), secrets=secrets)
    test.addCleanup(side.close)
    side.spawns = spawns
    return side


class Recording:
    """A side that keeps what each call handed on, by dispatch, and lets a test wait for a
    call's outcome."""

    def __init__(self, side):
        self.side = side
        self.items = {}
        self._cond = threading.Condition()

    def __getattr__(self, name):
        return getattr(self.side, name)

    def call(self, job, emit):
        def kept(item):
            with self._cond:
                self.items.setdefault(job["dispatch"], []).append(item)
                self._cond.notify_all()
            emit(item)

        self.side.call(job, kept)

    def cancel(self, dispatch):
        return self.side.cancel(dispatch)

    def outcome(self, dispatch, seconds=kit.WAIT_SECONDS):
        """The outcome document of a dispatch's call, waited for."""
        deadline = time.monotonic() + seconds
        with self._cond:
            while True:
                for item in self.items.get(dispatch, []):
                    if item[0] == "outcome":
                        return item[1]
                left = deadline - time.monotonic()
                if left <= 0:
                    raise AssertionError("no outcome came for the dispatch")
                self._cond.wait(left)

    def began(self, dispatch, kind="frame", seconds=kit.WAIT_SECONDS):
        """Waits until the dispatch's call handed on an item of this kind."""
        deadline = time.monotonic() + seconds
        with self._cond:
            while not any(item[0] == kind for item in self.items.get(dispatch, [])):
                left = deadline - time.monotonic()
                if left <= 0:
                    raise AssertionError("the call handed on no " + kind)
                self._cond.wait(left)


def serve(test, side, models_dir, limits=None, tls=None, host_limit_s=600, clock=time.time):
    """A front on loopback, closed in the test's cleanup; the front and its port."""
    listener = socket.create_server(("127.0.0.1", 0))
    made = front.Front(side, models_dir, TOKEN, limits or small_limits(), host_limit_s, tls, clock)
    made.start(listener)
    test.addCleanup(made.close)
    return made, listener.getsockname()[1]


def dispatch():
    return os.urandom(16).hex()


def messages(model, identity, conversation="c1", load_id="r1", request_id="r2", settings=None, queries=None, image=None, width=WIDTH, height=HEIGHT):
    """Whizbang's hello, a `load` and a segment request, as messages."""
    hello = {"type": "hello", "id": conversation, "version": proto.VERSION, "role": "whizbang"}
    load = {
        "type": "request",
        "id": load_id,
        "kind": "load",
        "model": model,
        "installation": identity,
        "settings": {} if settings is None else settings,
    }
    request = {
        "type": "request",
        "id": request_id,
        "kind": "segment",
        "model": model,
        "installation": identity,
        "image": proto.Payload(width, height, BINDING, picture(width, height) if image is None else image),
        "queries": [{"phrase": "thing"}] if queries is None else queries,
        "max_instances": 2,
    }
    return hello, load, request


def body(model, identity, **given):
    hello, load, request = messages(model, identity, **given)
    return proto.encode(hello) + proto.encode(load) + proto.encode(request), load, request


def connect(port, tls=None, buffer=None):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    if buffer is not None:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, buffer)
    sock.settimeout(kit.WAIT_SECONDS)
    sock.connect(("127.0.0.1", port))
    if tls is not None:
        sock = tls.wrap_socket(sock, server_hostname="localhost")
    return sock


def head(path, dispatch_id=None, length=None, token=TOKEN, method="POST"):
    lines = ["%s %s HTTP/1.1" % (method, path), "Host: front.test"]
    if token is not None:
        lines.append("Authorization: Bearer " + token)
    if dispatch_id is not None:
        lines.append("Whizbang-Dispatch: " + dispatch_id)
    if length is not None:
        lines.append("Content-Length: %d" % length)
    return ("\r\n".join(lines) + "\r\n\r\n").encode("ascii")


def post_request(port, model, identity, dispatch_id=None, tls=None, **given):
    """Sends a request; the connection, and the load and request it holds."""
    data, load, request = body(model, identity, **given)
    sock = connect(port, tls)
    sock.sendall(head("/pack/1/request", dispatch_id or dispatch(), len(data)) + data)
    return sock, load, request


def cancel_body(conversation="c9", request_id="r2"):
    hello = {"type": "hello", "id": conversation, "version": proto.VERSION, "role": "whizbang"}
    return proto.encode(hello) + proto.encode({"type": "cancel", "id": request_id})


def post_cancel(port, dispatch_id, tls=None):
    """Sends a cancel and reads its answer: the status."""
    data = cancel_body()
    sock = connect(port, tls)
    try:
        sock.sendall(head("/pack/1/cancel", dispatch_id, len(data)) + data)
        return Answer(sock).status
    finally:
        sock.close()


def get(port, path="/pack/1/health", token=None, tls=None):
    sock = connect(port, tls)
    try:
        sock.sendall(head(path, token=token, method="GET"))
        answer = Answer(sock)
        return answer.status, answer.rest()
    finally:
        sock.close()


class _Chunked(io.RawIOBase):
    """A chunked body, read as its chunks come."""

    def __init__(self, file):
        super().__init__()
        self.file = file
        self.left = 0
        self.ended = False

    def readable(self):
        return True

    def readinto(self, buffer):
        if self.ended:
            return 0
        if self.left == 0:
            line = self.file.readline(64)
            size = int(line.strip(), 16)
            if size == 0:
                self.file.readline(64)
                self.ended = True
                return 0
            self.left = size
        data = self.file.read(min(len(buffer), self.left))
        if not data:
            raise ValueError("the body ended inside a chunk")
        buffer[: len(data)] = data
        self.left -= len(data)
        if self.left == 0:
            self.file.readline(64)
        return len(data)


class Answer:
    """An answer read as the kind `remote` reads one: its status and headers; then, when its
    files were checked, the frames, each held to `pack/1` by a reader with the request's
    ids open; and the outcome record."""

    def __init__(self, sock):
        self.sock = sock
        self.file = sock.makefile("rb")
        line = self.file.readline(4096)
        if not line:
            raise ConnectionError("closed with no answer")
        self.status = int(line.split(b" ")[1])
        self.headers = {}
        while True:
            line = self.file.readline(4096)
            if line in (b"\r\n", b""):
                break
            name, _, value = line.decode("latin-1").partition(":")
            self.headers[name.strip().lower()] = value.strip()
        if self.headers.get("transfer-encoding") == "chunked":
            self.body = io.BufferedReader(_Chunked(self.file))
        else:
            self.body = io.BufferedReader(io.BytesIO(self.file.read(int(self.headers.get("content-length", "0")))))

    def close(self):
        self.file.close()
        self.sock.close()

    def rest(self):
        return self.body.read()

    def reader(self, load, request):
        return proto.Reader(self.body, {load["id"]: proto.awaiting(load), request["id"]: proto.awaiting(request)})

    def whole(self, load, request):
        """Every frame, then the outcome record, which its mark tells from a frame:
        `(frames, document, lines)`."""
        frames = []
        if "whizbang-model-files" in self.headers:
            reader = self.reader(load, request)
            while self.body.peek(4)[:4] != front.RECORD_MARK:
                frames.append(reader.read())
        document, lines = front.read_record(self.body)
        return frames, document, lines


def answer_of(sock, load, request):
    answer = Answer(sock)
    try:
        return (answer,) + answer.whole(load, request)
    finally:
        answer.close()


def wait_until(predicate, seconds=kit.WAIT_SECONDS):
    deadline = time.monotonic() + seconds
    while not predicate():
        if time.monotonic() > deadline:
            raise AssertionError("what was waited for did not come")
        time.sleep(0.01)
