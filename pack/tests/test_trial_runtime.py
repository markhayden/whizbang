"""Task R1's trial of the runtime (pack/trial_runtime.py): the environment it runs in is the
worker's, and it is checked before the runtime is imported. No runtime is installed here:
importing the trial imports none."""

import os
import re
import signal
import subprocess
import sys
import tempfile
import time
import unittest

import kit

sys.path.insert(0, kit.PACK)
import trial_runtime  # noqa: E402

WORKER_RS = os.path.join(
    os.path.dirname(kit.PACK), "crates", "whizbang-backends", "src", "pack", "worker.rs"
)


def the_supervisors_names():
    """The names the supervisor sets in a worker's environment, from its own source."""
    with open(WORKER_RS, encoding="utf-8") as file:
        source = file.read()
    body = source[source.index("pub fn environment(") :]
    body = body[: body.index("for (name, value) in set")]
    return sorted(re.findall(r'\("([A-Z_]+)",', body))


class Environment(unittest.TestCase):
    def test_the_trials_environment_is_the_one_the_supervisor_gives_a_worker(self):
        made = trial_runtime.environment({}, "/home-of-the-trial")
        self.assertEqual(sorted(made), the_supervisors_names())
        self.assertEqual(made["PYTORCH_MPS_HIGH_WATERMARK_RATIO"], "0.5")
        self.assertEqual(made["HF_TOKEN_PATH"], "/home-of-the-trial/token")
        for name in ("HOME", "TMPDIR", "XDG_CACHE_HOME", "HF_HOME"):
            self.assertEqual(made[name], "/home-of-the-trial")
        self.assertEqual(trial_runtime.problems(made), [])

    def test_the_locale_is_handed_on_and_nothing_else_of_the_parents(self):
        parent = {"LANG": "C", "LC_ALL": "C", "HF_TOKEN": "hf_madeup", "HOME": "/elsewhere", "PYTHONPATH": "/x"}
        made = trial_runtime.environment(parent, "/h")
        self.assertEqual(made["LANG"], "C")
        self.assertNotIn("HF_TOKEN", made)
        self.assertNotIn("PYTHONPATH", made)
        self.assertEqual(made["HOME"], "/h")

    def test_the_fallback_a_key_or_a_changed_value_is_refused(self):
        made = trial_runtime.environment({}, "/h")
        self.assertEqual(trial_runtime.problems(dict(made, PYTORCH_ENABLE_MPS_FALLBACK="1")), ["PYTORCH_ENABLE_MPS_FALLBACK"])
        self.assertEqual(trial_runtime.problems(dict(made, HF_TOKEN="hf_madeup")), ["HF_TOKEN"])
        self.assertEqual(trial_runtime.problems(dict(made, HF_HUB_OFFLINE="0")), ["HF_HUB_OFFLINE"])
        self.assertEqual(trial_runtime.problems(dict(made, HF_HOME="/elsewhere")), ["HF_HOME"])
        without = dict(made)
        del without["DO_NOT_TRACK"]
        self.assertEqual(trial_runtime.problems(without), ["DO_NOT_TRACK"])

    def test_the_text_encoding_macos_adds_to_every_program_is_not_refused(self):
        made = trial_runtime.environment({}, "/h")
        self.assertEqual(trial_runtime.problems(dict(made, __CF_USER_TEXT_ENCODING="0x1F5:0x0:0x0")), [])

    def test_a_watermark_not_under_one_is_refused(self):
        for ratio in ("1.0", "0", "1.7", "lots"):
            made = trial_runtime.environment({}, "/h", ratio)
            self.assertEqual(trial_runtime.problems(made), ["PYTORCH_MPS_HIGH_WATERMARK_RATIO"], ratio)

    def test_the_home_must_be_an_empty_folder_of_its_owners_alone(self):
        with tempfile.TemporaryDirectory() as made:
            os.chmod(made, 0o700)
            self.assertIsNone(trial_runtime.home_problem(made))
            os.chmod(made, 0o755)
            self.assertIn("alone", trial_runtime.home_problem(made))
            os.chmod(made, 0o700)
            open(os.path.join(made, "token"), "w").close()
            self.assertIn("not empty", trial_runtime.home_problem(made))
        self.assertIn("absolute", trial_runtime.home_problem("relative"))

    def test_importing_the_trial_imports_no_runtime(self):
        self.assertNotIn("torch", sys.modules)
        self.assertNotIn("transformers", sys.modules)


# A stand-in for a worker whose runtime draws a progress bar: it starts the standard
# library's resource tracker, as `tqdm`'s lock of `multiprocessing` does in the real
# worker (ADR 0022), writes the tracker's process id to its file, then waits for the end of
# its input, for 30 seconds at most.
WORKER_WITH_A_TRACKER = """
import os, sys
from multiprocessing import resource_tracker
resource_tracker.ensure_running()
with open(sys.argv[1] + ".new", "w") as file:
    file.write(str(resource_tracker._resource_tracker._pid))
os.rename(sys.argv[1] + ".new", sys.argv[1])
import select
select.select([sys.stdin], [], [], 30)
"""


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


class Processes(unittest.TestCase):
    def worker(self, folder):
        """Starts the stand-in, its streams pipes as the supervisor's are; returns it and
        its tracker's process id. Killed and waited for in the test's cleanup, and the
        tracker killed then too if it is still there."""
        noted = os.path.join(folder, "tracker")
        process = subprocess.Popen(
            [sys.executable, "-I", "-B", "-c", WORKER_WITH_A_TRACKER, noted],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=kit.child_environment(),
            close_fds=True,
        )

        def kill_and_wait():
            if process.poll() is None:
                process.kill()
            process.wait(kit.WAIT_SECONDS)
            for pipe in (process.stdin, process.stdout, process.stderr):
                pipe.close()

        self.addCleanup(kill_and_wait)
        limit = time.monotonic() + kit.WAIT_SECONDS
        while not os.path.exists(noted):
            self.assertIsNone(process.poll(), "the stand-in ended before its tracker started")
            self.assertLess(time.monotonic(), limit, "the tracker was not started in time")
            time.sleep(0.01)
        with open(noted) as file:
            tracker = int(file.read())
        self.addCleanup(lambda: alive(tracker) and os.kill(tracker, signal.SIGKILL))
        self.assertTrue(alive(tracker))
        return process, tracker

    def assert_gone_with_the_streams_closed(self, process, tracker):
        process.wait(kit.WAIT_SECONDS)
        # The tracker held copies of the worker's streams: they reach their end once it is
        # gone, so the supervisor's readers end too.
        for pipe in (process.stdout, process.stderr):
            self.assertTrue(kit.FdStream(pipe.fileno()).at_end())
        limit = time.monotonic() + kit.WAIT_SECONDS
        while alive(tracker):
            self.assertLess(time.monotonic(), limit, "the tracker outlived its worker")
            time.sleep(0.01)

    def test_the_resource_tracker_ends_with_its_worker_killed_or_at_the_end_of_its_input(self):
        with tempfile.TemporaryDirectory() as folder:
            os.mkdir(os.path.join(folder, "killed"))
            process, tracker = self.worker(os.path.join(folder, "killed"))
            process.kill()
            self.assert_gone_with_the_streams_closed(process, tracker)
            os.mkdir(os.path.join(folder, "ended"))
            process, tracker = self.worker(os.path.join(folder, "ended"))
            process.stdin.close()
            self.assert_gone_with_the_streams_closed(process, tracker)


if __name__ == "__main__":
    unittest.main()
