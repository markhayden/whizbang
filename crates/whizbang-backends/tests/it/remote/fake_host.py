"""The fake host's worker's side, for the conformance suite over HTTP (ADR 0046, "The
kind `remote`"; docs/plans/m6-http-and-remote-hosting.md, task R1, item 4).

The front a test starts is the pack's own (`whizbang_pack.front`), run as its command
line runs it, with `--worker fake_host:side`: this module's `side` makes the worker's
side the front hands each request to. It is the pack's own `LocalSide`, one for each
fake fault a case needs, each starting the pack's fakes (`whizbang_pack.fakes`) with
that fault, and with limits short enough for the fast tier. Which side answers a
request is read from the model its `load` names: `<fault>--<fake model>`, where the
fault `none` is the fakes with no fault. The installation of each such model is made
by the test, with a record that names the fake model as its adapter, so the worker
loads the fake model the name gives.

What a test reads of it: for each request its side began, a file
`<models>/seen/<dispatch id>`, the model and the settings of its `load` as JSON, written
before the request is handed on; `<models>/ended/<dispatch id>` once its side has
ended it; and for each cancel the front handed to its side, a file
`<models>/cancelled/<dispatch id>`.

Not a test file, and not the pack's: nothing ships it. The pack's source is never
changed by it."""

import atexit
import json
import os
import signal
import sys
import threading

from whizbang_pack import front

# The limits of every side, in seconds: the front's own but for these, which the
# cases of the suite reach and the fast tier cannot wait for.
LIMITS = {
    "hello": 1.0,
    "load": 0.6,
    "unload": 0.6,
    "call": 0.6,
    "cancel": 0.3,
    "end": 0.5,
    "restart_pause": 0.5,
}

# How a model's name says its fault and its fake model.
SEPARATOR = "--"
NO_FAULT = "none"

# The most sides one host makes: one for each fault of the pack's fakes, and one more.
SIDES_MOST = 16


class Router:
    """The worker's side the front is given: a `LocalSide` for each fault a request's
    model names, made at its first request, and the one control input over all of
    them."""

    def __init__(self, models_dir, secrets):
        self.models_dir = models_dir
        self._secrets = secrets
        self._sides = {}
        self._lock = threading.Lock()
        self.device = None
        self.checked = {}
        # Every worker a side started ends with the front's process, a deaf one too.
        atexit.register(self.close)

    def _side(self, fault):
        with self._lock:
            side = self._sides.get(fault)
            if side is None:
                if len(self._sides) >= SIDES_MOST:
                    return None
                more = () if fault == NO_FAULT else ("--fake-fault", fault)
                spawn = front.spawner(self.models_dir, "whizbang_pack.fakes", more, lifetime_s=120)
                side = front.LocalSide(self.models_dir, spawn=spawn, limits=front.Limits(**LIMITS), secrets=self._secrets)
                self._sides[fault] = side
            return side

    def call(self, job, emit):
        try:
            load, _ = front._job_frames(job["frames"])
        except Exception:
            emit(front.outcome("failed", "malformed"))
            return
        _note(self.models_dir, "seen", job["dispatch"], {"model": load["model"], "settings": load["settings"]})
        fault, separator, _ = load["model"].partition(SEPARATOR)
        side = self._side(fault) if separator else None
        if side is None:
            emit(front.outcome("failed", "rejected"))
            return
        try:
            side.call(job, emit)
        finally:
            _note(self.models_dir, "ended", job["dispatch"], {})
            if side.device is not None:
                self.device = side.device
            self.checked.update(side.checked)

    def cancel(self, dispatch):
        _note(self.models_dir, "cancelled", dispatch, {})
        with self._lock:
            sides = list(self._sides.values())
        return any([side.cancel(dispatch) for side in sides])

    def close(self):
        with self._lock:
            sides = list(self._sides.values())
        for side in sides:
            side.close()


def _note(models_dir, folder, dispatch, document):
    """Writes what a test reads, whole, by renaming it into place."""
    if not front.DISPATCH.match(dispatch or ""):
        return
    folder = os.path.join(models_dir, folder)
    os.makedirs(folder, exist_ok=True)
    path = os.path.join(folder, dispatch)
    with open(path + ".new", "w") as written:
        json.dump(document, written)
    os.replace(path + ".new", path)


def _stop_at_end_of_input():
    """The test ends the front by closing its input: the front then stops as at a
    `SIGTERM`, and its workers are killed on the way out."""
    try:
        while sys.stdin.buffer.read(4096):
            pass
    finally:
        os.kill(os.getpid(), signal.SIGTERM)


def side(models_dir, options):
    """What `--worker fake_host:side` makes of the models directory and the front's
    options."""
    threading.Thread(target=_stop_at_end_of_input, daemon=True).start()
    return Router(models_dir, options.get("secrets", ()))
