"""Task R1's trial of candidate A (docs/plans/m1c-local-pack.md, D8; ADR 0022): SAM 3 through
PyTorch and the `transformers` library, loaded from an installation's folder and asked one
phrase on one picture, twice, on the Mac's GPU with the CPU fallback off.

Run once by hand, on the owner's Mac, with the Python of the environment `whizbang setup
--extra torch` made, through the heavy lock, the thread guard at 200 and the watch on
resident memory:

    WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh r1 scripts/memory-watch.sh \\
      <env>/bin/python -I -B pack/trial_runtime.py --files <installation>/files --home <empty>

It runs outside the supervisor, so it brings the supervisor's bounds itself:

- Its own end at 10 minutes, set before anything else (`faulthandler`).
- A cleared environment: it starts itself again (`os.execve`, the same process) with the
  environment the supervisor gives a worker (`whizbang_backends::pack::worker::environment`)
  and nothing else: the locale of the parent's, but for a key's; `HOME`, `TMPDIR`,
  `XDG_CACHE_HOME` and `HF_HOME` the empty folder it is given; `HF_TOKEN_PATH` a file in it
  that is not there; the offline and no-telemetry variables of D11; the pools at 8; the
  allocator's watermark on the Apple GPU (0.5 unless `--watermark` says otherwise); and no
  `PYTORCH_ENABLE_MPS_FALLBACK`. Started again, it checks that environment before the
  runtime is imported, and refuses any other.
- It places the model on the device by name (`mps`) and uses no `device_map`.
- It asks the runtime for 8 threads in each of its two pools.

It prints `name: value` lines and nothing of a path, so that the fields can be read into the
ADR and the pull request. The picture is made here: a grey ground, a red disc and a blue
square, so nothing about it is anyone's.

The runtime is imported only inside `trial`, so the pack's tests import this file with no
runtime installed.
"""

import faulthandler

# First: from here on the process ends at 10 minutes, whatever it is doing. Only when run
# as a program: imported by the pack's tests, it leaves their runner's own limit alone.
LIMIT_SECONDS = 600
if __name__ == "__main__":
    faulthandler.dump_traceback_later(LIMIT_SECONDS, exit=True)

import os  # noqa: E402
import resource  # noqa: E402
import stat  # noqa: E402
import sys  # noqa: E402
import threading  # noqa: E402
import time  # noqa: E402

# The variables of the parent's environment handed on, but for a key's: the locale.
PASSED = ("LANG", "LC_ALL", "LC_CTYPE")
# What macOS itself adds to the environment of a program it starts, whatever it was given:
# the user's text encoding, a number pair and no secret. Seen by the first run of this trial.
ADDED_BY_THE_SYSTEM = ("__CF_USER_TEXT_ENCODING",)
POOL_THREADS = "8"
WATERMARK = "0.5"
CLEARED = "--cleared"
PHRASE = "circle"


def is_key(name):
    upper = name.upper()
    return any(upper.endswith(end) for end in ("_KEY", "_TOKEN", "_SECRET"))


def environment(parent, home, watermark=WATERMARK, progress_bars=False):
    """The worker's environment, as the supervisor makes it: the locale of `parent`, then
    the supervisor's own variables, with `home` the worker's empty folder. With
    `progress_bars`, the one variable that turns the libraries' progress bars off is left
    out, to show what they start."""
    chosen = {name: value for name, value in parent.items() if name in PASSED and not is_key(name)}
    chosen.update(
        {
            "HOME": home,
            "TMPDIR": home,
            "XDG_CACHE_HOME": home,
            "HF_HOME": home,
            "HF_TOKEN_PATH": os.path.join(home, "token"),
            "HF_HUB_OFFLINE": "1",
            "HF_HUB_DISABLE_TELEMETRY": "1",
            "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1",
            "DO_NOT_TRACK": "1",
            "PATH": "/usr/bin:/bin",
            "OMP_NUM_THREADS": POOL_THREADS,
            "MKL_NUM_THREADS": POOL_THREADS,
            "OPENBLAS_NUM_THREADS": POOL_THREADS,
            "VECLIB_MAXIMUM_THREADS": POOL_THREADS,
            "TOKENIZERS_PARALLELISM": "false",
            "PYTORCH_MPS_HIGH_WATERMARK_RATIO": watermark,
            # The runtime refuses a low watermark above the high one, and its default low
            # one, 1.4, is above any high one under 1.0: so the low one is set equal to it.
            "PYTORCH_MPS_LOW_WATERMARK_RATIO": watermark,
            "PYTHONDONTWRITEBYTECODE": "1",
            # The Hub's and the library's progress bars off: drawing one makes a lock of
            # `multiprocessing`, and with it a process (see `tracker`).
            "HF_HUB_DISABLE_PROGRESS_BARS": "1",
        }
    )
    if progress_bars:
        del chosen["HF_HUB_DISABLE_PROGRESS_BARS"]
    return chosen


def problems(env, progress_bars=False):
    """The names of the variables of `env` that the trial refuses, in order: one the
    worker's environment does not have, one with another value, one it lacks. The folder
    the homes name is checked by `home_problem`."""
    home = env.get("HOME", "")
    wanted = environment({}, home, env.get("PYTORCH_MPS_HIGH_WATERMARK_RATIO", WATERMARK), progress_bars)
    found = []
    for name in sorted(env):
        if (name in PASSED and not is_key(name)) or name in ADDED_BY_THE_SYSTEM:
            continue
        if name not in wanted or env[name] != wanted[name]:
            found.append(name)
    for name in sorted(wanted):
        if name not in env:
            found.append(name)
    try:
        ratio = float(wanted["PYTORCH_MPS_HIGH_WATERMARK_RATIO"])
    except ValueError:
        ratio = -1.0
    if not 0.0 < ratio < 1.0 and "PYTORCH_MPS_HIGH_WATERMARK_RATIO" not in found:
        found.append("PYTORCH_MPS_HIGH_WATERMARK_RATIO")
    return found


def home_problem(home):
    """Why `home` cannot be the worker's folder, or None: it must be an absolute path to a
    folder of this user's alone, empty but for nothing, and its token file must not be
    there."""
    if not os.path.isabs(home):
        return "the home folder is not an absolute path"
    try:
        held = os.lstat(home)
    except OSError:
        return "the home folder is not there"
    if not stat.S_ISDIR(held.st_mode):
        return "the home folder is not a folder"
    if held.st_uid != os.getuid() or held.st_mode & 0o077:
        return "the home folder is not its owner's alone"
    if os.listdir(home):
        return "the home folder is not empty"
    return None


def argument(argv, flag, default=None):
    if flag in argv:
        at = argv.index(flag)
        if at + 1 < len(argv):
            return argv[at + 1]
    return default


def say(name, value):
    print("%s: %s" % (name, value), flush=True)


# The events by which Python starts a process (the standard library's audit events), and
# what was started: each event's name and the last part of the program's name, no path.
STARTS = ("subprocess.Popen", "os.posix_spawn", "os.fork", "os.forkpty", "os.exec", "os.system", "os.spawn")
started = []


def watch_starts():
    """Notes every process Python is asked to start, from here on, and says so at once."""

    def hook(event, args):
        if event in STARTS:
            program = args[0] if args else ""
            if isinstance(program, (list, tuple)):
                program = program[0] if program else ""
            if isinstance(program, bytes):
                program = program.decode("utf-8", "replace")
            name = os.path.basename(str(program)) or "-"
            started.append((event, name))
            say("process_started", "%s %s" % (event, name))

    sys.addaudithook(hook)


def children():
    """Whether this process has a child, by the system's own answer and starting nothing:
    `waitpid` on any child, not waiting. A child that has ended is reaped and counted."""
    try:
        pid, _ = os.waitpid(-1, os.WNOHANG)
    except ChildProcessError:
        return "none"
    return "one ended" if pid else "running"


def tracker():
    """Whether the standard library's resource tracker (`multiprocessing.resource_tracker`),
    a process `multiprocessing` starts for a lock or shared memory, was started here, and
    whether it is a child of this process that is still running."""
    module = sys.modules.get("multiprocessing.resource_tracker")
    pid = getattr(getattr(module, "_resource_tracker", None), "_pid", None)
    if not pid:
        return "not started"
    try:
        ended, _ = os.waitpid(pid, os.WNOHANG)
    except ChildProcessError:
        return "started, not a child"
    return "a child, ended" if ended else "a child, running"


def picture():
    """640 by 480, RGB: a grey ground, a red disc of radius 90 at (200, 240), a blue square
    of side 160 at (400, 160)."""
    from PIL import Image, ImageDraw

    made = Image.new("RGB", (640, 480), (128, 128, 128))
    draw = ImageDraw.Draw(made)
    draw.ellipse((110, 150, 290, 330), fill=(220, 30, 30))
    draw.rectangle((400, 160, 560, 320), fill=(30, 60, 220))
    return made


def trial(files, phrase):
    """Loads SAM 3 from `files` on the Apple GPU and asks `phrase` twice; prints the fields."""
    import torch

    say("torch", torch.__version__)
    say("mps_available", torch.backends.mps.is_available())
    say("mps_built", torch.backends.mps.is_built())
    if not torch.backends.mps.is_available():
        say("verdict", "stop: the Apple GPU is not available to the runtime")
        return 3
    torch.set_num_threads(int(POOL_THREADS))
    torch.set_num_interop_threads(int(POOL_THREADS))
    say("intra_op_threads", torch.get_num_threads())
    say("inter_op_threads", torch.get_num_interop_threads())
    for line in torch.__config__.parallel_info().splitlines():
        line = line.strip()
        if line and "/" not in line and ":" in line:
            say("parallel_info", line)

    import transformers
    from transformers import Sam3Model, Sam3Processor

    say("transformers", transformers.__version__)
    device = torch.device("mps")
    say("children_before_load", children())

    began = time.monotonic()
    model = Sam3Model.from_pretrained(files, local_files_only=True)
    model = model.to(device)
    model.eval()
    processor = Sam3Processor.from_pretrained(files, local_files_only=True)
    torch.mps.synchronize()
    say("load_seconds", "%.2f" % (time.monotonic() - began))
    devices = sorted({p.device.type for p in model.parameters()} | {b.device.type for b in model.buffers()})
    dtypes = sorted({str(p.dtype) for p in model.parameters()})
    say("parameter_devices", ",".join(devices))
    say("parameter_dtypes", ",".join(dtypes))
    say("parameters", sum(p.numel() for p in model.parameters()))
    say("children_after_load", children())
    say("resource_tracker_after_load", tracker())

    image = picture()
    for request in (1, 2):
        began = time.monotonic()
        inputs = processor(images=image, text=phrase, return_tensors="pt").to(device)
        with torch.inference_mode():
            outputs = model(**inputs)
        results = processor.post_process_instance_segmentation(
            outputs,
            threshold=0.5,
            mask_threshold=0.5,
            target_sizes=inputs.get("original_sizes").tolist(),
        )[0]
        torch.mps.synchronize()
        took = time.monotonic() - began
        say("request_%d_seconds" % request, "%.2f" % took)
        say("request_%d_output_device" % request, outputs.pred_masks.device.type)
        say("request_%d_instances" % request, len(results["scores"]))
        if len(results["scores"]):
            box = [round(v) for v in results["boxes"][0].tolist()]
            say("request_%d_best_score" % request, "%.3f" % float(results["scores"].max()))
            say("request_%d_first_box" % request, box)
        say("children_after_request_%d" % request, children())

    say("mps_driver_allocated_mib", torch.mps.driver_allocated_memory() // (1024 * 1024))
    say("mps_current_allocated_mib", torch.mps.current_allocated_memory() // (1024 * 1024))
    say("mps_recommended_max_mib", torch.mps.recommended_max_memory() // (1024 * 1024))
    say("python_threads", threading.active_count())
    # On macOS `ru_maxrss` is in bytes.
    say("peak_resident_mib", resource.getrusage(resource.RUSAGE_SELF).ru_maxrss // (1024 * 1024))
    say("children_at_end", children())
    say("resource_tracker_at_end", tracker())
    say("processes_started", len(started))
    say("verdict", "ran on mps with the fallback off")
    return 0


def main(argv):
    files = argument(argv, "--files")
    home = argument(argv, "--home")
    phrase = argument(argv, "--phrase", PHRASE)
    watermark = argument(argv, "--watermark", WATERMARK)
    progress_bars = "--progress-bars" in argv
    if not files or not home:
        print("usage: trial_runtime.py --files <folder> --home <empty folder> [--phrase <text>] [--watermark <ratio>] [--progress-bars]", file=sys.stderr)
        return 2
    why = home_problem(home)
    if why:
        print("trial_runtime: %s" % why, file=sys.stderr)
        return 2
    if CLEARED not in argv:
        env = environment(dict(os.environ), home, watermark, progress_bars)
        os.execve(sys.executable, [sys.executable, "-I", "-B", os.path.abspath(__file__)] + argv[1:] + [CLEARED], env)
    wrong = problems(dict(os.environ), progress_bars)
    if wrong:
        print("trial_runtime: refused, these variables are not the worker's: %s" % " ".join(wrong), file=sys.stderr)
        return 2
    watch_starts()
    say("watermark", os.environ["PYTORCH_MPS_HIGH_WATERMARK_RATIO"])
    say("fallback", os.environ.get("PYTORCH_ENABLE_MPS_FALLBACK", "unset"))
    say("progress_bars", "on" if progress_bars else "off")
    say("phrase", phrase)
    return trial(files, phrase)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
