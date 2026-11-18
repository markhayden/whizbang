"""The worker whizbang starts for a local model (docs/plans/m1c-local-pack.md, P2).

    python -m whizbang_pack.worker --models-dir <folder> [--max-lifetime-s <seconds>]

It speaks `pack/1` (docs/adr/0020-the-pack-protocol.md): frames on its standard input and
output; its error output is diagnostics only. Anything else in the process that prints
goes to the error output, so that nothing but frames is ever written where frames are.

Three threads of its own, and no pool:

- the reader reads frames, answers the hello, hands a request to the call thread, and sets
  a call's flag when a `cancel` names it. At the end of its input it ends the process
  then and there, whatever the call is doing: inside a `load`, or inside a call that
  ignores its cancel, it waits for nothing (`os._exit`);
- the call thread runs one call at a time. A request that comes while one runs is answered
  `unavailable`: whizbang's supervisor sends one at a time (D4);
- the main thread waits for the end, and ends the process at `--max-lifetime-s` if nothing
  else has.

One model is resident: a `load` of another unloads the first. The worker is given the
models directory when it is started, and finds an installation by the model's name and
the identity a `load` names. The adapter is the one the installation's record names,
the record held to that identity, and a user's adapter only by the hash the record gives
(`installed`; plan 1D, task U1). It reads no variable of its environment.

How it ends, by its exit code: 0 at the end of its input, or when whizbang's error names
the conversation; 2 when a frame is refused, or its output cannot be written; 3 at its
lifetime. A command line it does not take ends it with 2 before anything starts.
"""

import os
import queue
import sys
import threading

from whizbang_pack import fake_models, installed, owlv2_sam2, proto, sam3
from whizbang_pack.adapter import Cancelled, Fault, Installation, Stopped, runtime_device

END_CODE = 0
REFUSED_CODE = 2
LIFETIME_CODE = 3
LIFETIME_MOST = 31_536_000

# The adapters of real models, by the adapter's name in the catalogue; each imports its
# runtime only when its model is loaded (task R2a: `sam3`; plan 1D, task T1: `owlv2-sam2`).
REAL = {"sam3": sam3.Sam3, "owlv2-sam2": owlv2_sam2.Owlv2Sam2}
# The shipped catalogue's models, each by the name of its adapter there.
SHIPPED = {"sam3": "sam3", "tiny": "owlv2-sam2"}
# The adapters the worker answers for, by model name. The fakes answer for names the
# shipped catalogue never holds; each model of the shipped catalogue joins them.
ADAPTERS = dict(fake_models.MODELS, **{model: REAL[adapter] for model, adapter in SHIPPED.items()})
# The adapters the worker answers for, by the adapter's name an installation's record gives
# (plan 1D, task U1): the real ones and the fakes, each under its own name. An adapter a
# user added is in no table: it is found by the hash its installation's record names.
BY_ADAPTER = dict(fake_models.MODELS, **REAL)

# Reasons a refused frame ends the conversation as `too_large` (ADR 0020, "What a refusal
# says"); every other reason is `malformed`.
_TOO_LARGE = ("header_too_large", "over_bound", "too_large")


class Usage(Exception):
    """A command line the entry point does not take."""


def parse(argv, faults=()):
    """The options of a command line: `--models-dir`, an absolute folder, given once;
    `--max-lifetime-s`, whole seconds from 1, at most once; and `--fake-fault`, only when
    `faults` names what it may be."""
    names = {"--models-dir": "models_dir", "--max-lifetime-s": "lifetime"}
    if faults:
        names["--fake-fault"] = "fault"
    given = {}
    args = list(argv)
    while args:
        name = args.pop(0)
        if name == "--fake-fault" and not faults:
            raise Usage("this entry point takes no --fake-fault: only whizbang_pack.fakes does")
        if name not in names:
            raise Usage("an option this entry point does not take")
        if not args:
            raise Usage("%s needs a value" % name)
        if names[name] in given:
            raise Usage("%s is given twice" % name)
        given[names[name]] = args.pop(0)
    models_dir = given.get("models_dir")
    if models_dir is None or not os.path.isabs(models_dir):
        raise Usage("--models-dir must name an absolute folder")
    lifetime = given.get("lifetime")
    if lifetime is not None:
        if not (lifetime.isascii() and lifetime.isdigit() and 1 <= int(lifetime) <= LIFETIME_MOST):
            raise Usage("--max-lifetime-s must be whole seconds from 1 to %d" % LIFETIME_MOST)
        lifetime = int(lifetime)
    fault = given.get("fault")
    if fault is not None and fault not in faults:
        raise Usage("--fake-fault names no fault this entry point has")
    return {"models_dir": models_dir, "lifetime": lifetime, "fault": fault}


def count_threads():
    """How many threads the process holds now, the runtime's included: from the system
    where it says (Linux, macOS), and otherwise those Python started."""
    try:
        return len(os.listdir("/proc/self/task"))
    except OSError:
        pass
    if sys.platform == "darwin":
        counted = _darwin_threads()
        if counted:
            return counted
    return threading.active_count()


def _darwin_threads():
    try:
        import ctypes
        import struct

        # proc_pidinfo(pid, PROC_PIDTASKINFO, 0, buffer, size): a `struct proc_taskinfo` of
        # six 64-bit fields and twelve 32-bit ones, of which `pti_threadnum` is the tenth.
        size = 6 * 8 + 12 * 4
        buffer = ctypes.create_string_buffer(size)
        library = ctypes.CDLL(None)
        written = library.proc_pidinfo(os.getpid(), 4, ctypes.c_uint64(0), buffer, size)
        if written != size:
            return None
        return struct.unpack_from("<i", buffer.raw, 6 * 8 + 9 * 4)[0]
    except (OSError, AttributeError, ImportError):
        return None


class Resident:
    """The model in the worker: its name, its installation and its adapter."""

    def __init__(self, model, identity, installation, adapter):
        self.model = model
        self.identity = identity
        self.installation = installation
        self.adapter = adapter


class Call:
    """What an adapter is handed for one request: its events go out through it, and its
    waits end at its cancel or at the worker's end."""

    def __init__(self, worker, request):
        self.worker = worker
        self.request = request
        self.id = request["id"]
        self.events = 0
        self.text = 0
        self._cancelled = False
        self._wake = threading.Event()

    def cancelled(self):
        return self._cancelled

    def cancel(self):
        self._cancelled = True
        self._wake.set()

    def wake(self):
        self._wake.set()

    def event(self, text):
        """Something the call says before it ends. Cleaned, and dropped once the request's
        256 events or 64 KiB of text are spent."""
        self.worker._event(self, text)

    def wait_for_cancel(self):
        """Waits until the call is cancelled, and raises `Cancelled`; or until the worker
        ends."""
        self._wake.wait()
        if self.worker.stopping():
            raise Stopped()
        raise Cancelled()

    def wait_for_stop(self):
        """Waits, deaf to a cancel, until the worker ends."""
        self.worker._stopping.wait()
        raise Stopped()

    def end_process(self, code):
        """Ends the worker's process now (`fake-exit`)."""
        self.worker.stop(code)
        raise Stopped()


def _write_all(stream, data):
    view = memoryview(data)
    while view:
        written = stream.write(view)
        if written is None:
            raise OSError("the output would block")
        view = view[written:]


class Worker:
    def __init__(self, models_dir, lifetime, stdin, stdout, stderr, exit=os._exit, adapters=None, count_threads=count_threads):
        self.models_dir = models_dir
        self.lifetime = lifetime
        self.stdin = stdin
        self.stdout = stdout
        self.stderr = stderr
        self.adapters = ADAPTERS if adapters is None else adapters
        # A test that gives its own adapters gives them by both names.
        self.by_adapter = BY_ADAPTER if adapters is None else adapters
        self.code = None
        self.threads = []
        self.resident = None
        self.conversation = None
        self._exit = exit
        self._count_threads = count_threads
        self._calls = queue.SimpleQueue()
        self._current = None
        self._lock = threading.Lock()
        self._writing = threading.Lock()
        self._stopping = threading.Event()
        self._ended = threading.Event()

    # --- the three threads ---------------------------------------------------------------

    def start(self):
        """Starts the reader and the call thread, both daemons: the call thread first.

        whizbang sends its hello as soon as the process is started, and the reader's answer
        counts the threads the process holds. Started first, the reader could answer before
        the call thread was there and count two of the three. `threads` keeps its order,
        the reader first and then the call."""
        for name, target in (("whizbang-pack-reader", self._read_frames), ("whizbang-pack-call", self._run_calls)):
            self.threads.append(threading.Thread(target=target, name=name, daemon=True))
        for thread in reversed(self.threads):
            thread.start()
            self._started(thread)

    def _started(self, thread):
        """Nothing; a test waits here between one start and the next."""

    def wait(self):
        """The main thread's part: waits for the end, and ends the worker at its lifetime."""
        if not self._ended.wait(self.lifetime):
            self.stop(LIFETIME_CODE)
        return self.code

    def stopping(self):
        return self._stopping.is_set()

    def stop(self, code):
        """Ends the worker with `code`: the process, through its exit, at once, waiting for
        no thread and no stream."""
        with self._lock:
            if self.code is not None:
                return
            self.code = code
            current = self._current
        self._stopping.set()
        if current is not None:
            current.wake()
        self._calls.put(None)
        self._ended.set()
        # Nothing is waited for here, the error output least of all: a thread blocked on a
        # full pipe of it holds its lock, and a flush would wait for that thread.
        self._exit(code)

    def _read_frames(self):
        reader = proto.Reader(self.stdin)
        while not self._stopping.is_set():
            try:
                message = reader.read()
            except proto.Refused as refused:
                fault = "too_large" if refused.reason in _TOO_LARGE else "malformed"
                self._end_conversation(self.conversation or refused.id, fault, refused.sentence)
                return
            if message is None:
                self._end_of_input()
                return
            if self._stopping.is_set():
                return
            self._take(message)

    def _run_calls(self):
        while True:
            call = self._calls.get()
            if call is None or self._stopping.is_set():
                return
            try:
                chunks = self._answer(call)
            except Stopped:
                return
            # The call is over before its answer is written, so that the next request,
            # sent once the answer is read, finds the worker free.
            with self._lock:
                self._current = None
            if self._stopping.is_set():
                return
            self._write(chunks)

    # --- what the reader does with a frame ------------------------------------------------

    def _take(self, message):
        kind_of_message = message["type"]
        if self.conversation is None:
            if kind_of_message != "hello" or message["role"] != "whizbang":
                self._end_conversation(message["id"], "malformed", "the first frame must be whizbang's hello")
                return
            self.conversation = message["id"]
            self._say_hello()
            return
        if kind_of_message == "request":
            self._begin(message)
        elif kind_of_message == "cancel":
            with self._lock:
                current = self._current
            if current is not None and current.id == message["id"]:
                current.cancel()
        elif kind_of_message == "error":
            # The worker asks nothing of whizbang, so an error names the conversation or
            # nothing it waits for.
            if message["id"] == self.conversation:
                self.stop(END_CODE)
        else:
            self._end_conversation(self.conversation, "malformed", "a frame this side does not take")

    def _say_hello(self):
        self._write(
            proto.chunks(
                {
                    "type": "hello",
                    "id": self.conversation,
                    "version": proto.VERSION,
                    "role": "worker",
                    "device": runtime_device(),
                    "threads": self._threads(),
                }
            )
        )

    def _end_of_input(self):
        self.stop(END_CODE)

    def _end_conversation(self, id, fault, sentence):
        if id is not None:
            self._write(proto.chunks({"type": "error", "id": id, "fault": fault, "sentence": sentence}))
        self.stop(REFUSED_CODE)

    def _begin(self, request):
        call = Call(self, request)
        with self._lock:
            busy = self._current is not None
            if not busy:
                self._current = call
        if busy:
            self._write(self._error(request["id"], "unavailable", "another call is running"))
            return
        self._calls.put(call)

    # --- one call -------------------------------------------------------------------------

    def _answer(self, call):
        """The frame that ends a call: its result, or an error."""
        self._before_call(call)
        try:
            message = self._perform(call)
        except Cancelled:
            return self._error(call.id, "cancelled", "the call was cancelled")
        except Fault as fault:
            name = fault.fault if fault.fault in proto.FAULTS else "unavailable"
            return self._error(call.id, name, fault.sentence)
        except Exception as failure:
            return self._error(call.id, "unavailable", "the model failed with " + type(failure).__name__)
        try:
            chunks = proto.chunks(message)
        except (proto.Refused, ValueError, TypeError, KeyError, AttributeError):
            return self._error(call.id, "unavailable", "the model gave an answer that cannot be written")
        if message["kind"] == "segment" and self.resident is not None:
            chunks = self.resident.adapter.spoil(chunks)
        return chunks

    def _before_call(self, call):
        """Nothing; a fake fault may say something here."""

    def _perform(self, call):
        request = call.request
        kind = request["kind"]
        found = None
        if kind == "load":
            diagnostics = self._load(call)
        elif kind == "unload":
            diagnostics = self._unload(call)
        else:
            found, diagnostics = self._segment(call)
        room = min(proto.DIAGNOSTICS_CUT, proto.DIAGNOSTICS_LIMIT - call.text)
        message = {
            "type": "result",
            "id": call.id,
            "kind": kind,
            "device": self.resident.adapter.device if self.resident is not None else runtime_device(),
            "threads": self._threads(),
            "diagnostics": proto.clean(diagnostics or "", room),
        }
        if kind == "segment":
            image = request["image"]
            message["instances"] = [
                {
                    "query": item.query,
                    "score": float(item.score),
                    "iou": None if item.iou is None else float(item.iou),
                    "box": [float(edge) for edge in item.box],
                    "plane": proto.Payload(image.width, image.height, image.binding, bytes(item.samples)),
                }
                for item in found
            ]
        return message

    def _load(self, call):
        request = call.request
        model, identity = request["model"], request["installation"]
        resident = self.resident
        if resident is not None and (resident.model, resident.identity) == (model, identity):
            return "already loaded"
        installation = Installation(self.models_dir, model, identity)
        # By the installation's record, held to the identity this load names (task U1).
        make = installed.adapter_of(self.models_dir, installation, self.adapters, self.by_adapter, SHIPPED)
        if make is None:
            raise Fault("rejected", "no adapter answers for this model")
        said = []
        if resident is not None:
            self.resident = None
            resident.adapter.unload(call)
            said.append("unloaded " + resident.model)
        adapter = make()
        diagnostics = adapter.load(installation, request["settings"], call)
        self.resident = Resident(model, identity, installation, adapter)
        if diagnostics:
            said.append(str(diagnostics))
        return "; ".join(said)

    def _unload(self, call):
        request = call.request
        resident = self.resident
        if resident is None or (resident.model, resident.identity) != (request["model"], request["installation"]):
            return "that installation was not loaded"
        self.resident = None
        return resident.adapter.unload(call)

    def _segment(self, call):
        request = call.request
        resident = self.resident
        if resident is None or (resident.model, resident.identity) != (request["model"], request["installation"]):
            raise Fault("unavailable", "the model is not loaded")
        for query in request["queries"]:
            (kind,) = query
            if kind not in resident.adapter.features:
                raise Fault("unsupported", "the model takes no " + kind)
        return resident.adapter.segment(request, call)

    # --- writing --------------------------------------------------------------------------

    def _event(self, call, text):
        if self._stopping.is_set():
            raise Stopped()
        cleaned = proto.clean(text, proto.DIAGNOSTICS_CUT)
        if not cleaned:
            return
        if call.events >= proto.EVENTS_LIMIT or call.text + len(cleaned) > proto.DIAGNOSTICS_LIMIT:
            return
        call.events += 1
        call.text += len(cleaned)
        self._write(proto.chunks({"type": "event", "id": call.id, "text": cleaned}))

    def _error(self, id, fault, sentence):
        return proto.chunks({"type": "error", "id": id, "fault": fault, "sentence": sentence})

    def _write(self, chunks):
        failed = False
        with self._writing:
            try:
                for chunk in chunks:
                    _write_all(self.stdout, chunk)
                self.stdout.flush()
            except (OSError, ValueError):
                failed = True
        if failed:
            self.stop(REFUSED_CODE)

    def _threads(self):
        return max(1, min(self._count_threads(), proto.THREADS_LIMIT))


def make(argv, stdin, stdout, stderr, exit=os._exit, adapters=None):
    """A worker from a command line, not started; `Usage` for one it does not take."""
    options = parse(argv)
    return Worker(options["models_dir"], options["lifetime"], stdin, stdout, stderr, exit, adapters)


def main(argv=None, make=make):
    """The process: frames on the standard input and output, until the end."""
    argv = sys.argv[1:] if argv is None else argv
    # Frames go to a copy of the standard output; the standard output itself becomes the
    # error output, so that what a library prints is never read as a frame.
    frames = os.fdopen(os.dup(1), "wb", buffering=0)
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    try:
        made = make(argv, stdin=sys.stdin.buffer, stdout=frames, stderr=sys.stderr)
    except Usage as usage:
        sys.stderr.write("whizbang_pack: %s\n" % usage)
        sys.stderr.flush()
        os._exit(REFUSED_CODE)
    made.start()
    code = made.wait()
    os._exit(code)


if __name__ == "__main__":
    main()
