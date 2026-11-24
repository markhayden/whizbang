"""The worker with one model more, for the front's tests: `fake-say`, which says two events
and then answers as `fake-ok` does, so that a test sees a stream of events over HTTP. The
model is the tests' own, so the pack's fakes, which whizbang's conformance table names one
by one, are as they were. Not a test file: `run.py` finds only `test_*.py`."""

import os

from whizbang_pack import fake_models, worker


class FakeSay(fake_models.FakeOk):
    def segment(self, request, call):
        call.event("looking")
        call.event("found what the request determines")
        return fake_models.determined(request), ""


ADAPTERS = dict(worker.ADAPTERS, **{"fake-say": FakeSay})


def make(argv, stdin, stdout, stderr, exit=os._exit, adapters=None):
    return worker.make(argv, stdin, stdout, stderr, exit, ADAPTERS)


if __name__ == "__main__":
    worker.main(make=make)
