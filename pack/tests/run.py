"""Runs the pack's tests. `just test-pack` starts it, through scripts/test-pack.sh.

    python -I -B pack/tests/run.py [folder]

It runs every `test_*.py` of `folder` (this file's folder when none is given), with the
pack's `src` first on the path, and exits 0 only when every test passed. It holds the row
of "Rules" on the pack's Python tests (docs/plans/m1c-local-pack.md):

- It ends itself at its limit, WHIZBANG_PACK_TEST_LIMIT seconds (60 when unset, and never
  more): the standard library's `faulthandler` prints the stack of every thread and ends
  the process, whatever the tests are doing, a test that blocks for ever included. Nothing
  outside the process is needed for that.
- A thread a test starts is a daemon thread. One that is not, still running when the tests
  are done, fails the run; and the run ends by `os._exit`, so no thread keeps it alive.
- The whole run has 12 seconds; over that, it fails and says how long it took. It was 10
  until the front's tests of its early headers and keep-alives (plan m6, task L1, item 2b)
  took the hosted macOS runner to 10.5.

It uses the standard library only (D10), and runs on Python 3.9 and later, so that the
script tests can run it with the runner's own `python3`.
"""

import faulthandler
import os
import sys
import threading
import time
import unittest

LIMIT_DEFAULT = 60
BUDGET_SECONDS = 12


def limit_from_environment():
    text = os.environ.get("WHIZBANG_PACK_TEST_LIMIT", str(LIMIT_DEFAULT))
    if not (text.isdigit() and text.isascii() and 1 <= int(text) <= LIMIT_DEFAULT):
        return None
    return int(text)


def main(argv):
    limit = limit_from_environment()
    if limit is None:
        print(
            "run.py: WHIZBANG_PACK_TEST_LIMIT must be a whole number of seconds from 1 to %d"
            % LIMIT_DEFAULT,
            file=sys.stderr,
        )
        return 2
    # First, before any test is imported: from here on the process ends at the limit.
    faulthandler.dump_traceback_later(limit, exit=True)

    here = os.path.dirname(os.path.abspath(__file__))
    sys.path.insert(0, os.path.join(os.path.dirname(here), "src"))
    folder = os.path.abspath(argv[1]) if len(argv) > 1 else here

    began = time.monotonic()
    suite = unittest.defaultTestLoader.discover(folder, pattern="test_*.py", top_level_dir=folder)
    result = unittest.TextTestRunner(stream=sys.stderr, verbosity=2).run(suite)
    took = time.monotonic() - began

    status = 0 if result.wasSuccessful() else 1
    if result.testsRun == 0:
        print("run.py: no test was found under %s" % folder, file=sys.stderr)
        status = 1
    for thread in threading.enumerate():
        if thread is not threading.main_thread() and not thread.daemon and thread.is_alive():
            print(
                "run.py: a test left a thread that is not a daemon: %s" % thread.name,
                file=sys.stderr,
            )
            status = 1
    if took > BUDGET_SECONDS:
        print(
            "run.py: the tests took %.1f seconds, over their %d" % (took, BUDGET_SECONDS),
            file=sys.stderr,
        )
        status = 1
    print("run.py: %d tests in %.2f seconds" % (result.testsRun, took), file=sys.stderr)
    return status


if __name__ == "__main__":
    code = main(sys.argv)
    sys.stdout.flush()
    sys.stderr.flush()
    # Not sys.exit: a thread a test left behind must not keep the process alive.
    os._exit(code)
