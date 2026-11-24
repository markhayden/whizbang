"""SAM 3 on Modal: the GPU host of the kind `remote` (docs/plans/m6-http-and-remote-hosting.md,
task D1, item 2; ADR 0046; docs/guides/gpu-host.md).

The only file in the repo that names Modal. It is deployed by `scripts/gpu-host.sh deploy`,
from a clean checkout of a named commit, into the workspace of whoever's Modal token the
command-line client holds: no workspace, token or endpoint is written here. It needs Modal's
client 1.6.1 or later.

- **The image**: Modal's Debian image with the pack's Python, the host's lock
  (`deploy/gpu-host/requirements.txt`: PyTorch's Linux CUDA build, its NVIDIA packages,
  `torchvision` and `transformers`, every file pinned by its hash, from pypi.org and
  files.pythonhosted.org alone), and the pack's sources from the checkout. The Python is the
  minor version `pack/.python-version` pins (Modal's image chooses its patch release).
- **The volume** `whizbang-models` is the models directory, mounted at `/models` on both
  functions: `scripts/gpu-host.sh weights` copies SAM 3's installed files into it from the
  deployer's own machine. Nothing is downloaded from Hugging Face here.
- **The front** (`front`): a CPU function that runs the pack's front (`python -m
  whizbang_pack.front`) as its web server, the bearer token from the secret `whizbang-front`
  under `SEGMENT_TOKEN`; at most one container, which takes many inputs at once (the front's
  own connections), so a probe and a cancel reach the one front that holds the queue and
  are never queued behind a running request. Its health route needs no card.
- **The bridge** (`Bridge`): one GPU class, a 24 GB card, at most one container, which takes
  two inputs at once (the call and a control input for its cancel; the worker still runs
  one call at a time). It runs the pack's worker beside the card (`whizbang_pack.front
  .LocalSide`), keeps the model resident between calls, and claims each dispatch id in the
  named `Dict` `whizbang-claims` before any model work: a delivery after the claim is
  answered as a replay and runs no model. It asks for no retry.

**The bound of one attempt** is `LIMITS` below: its sum is the config's `host_limit_s`, and
`scripts/gpu-host.sh deploy` reads the three back from the tree it deploys and refuses when
one is missing or the sum differs (ADR 0046, "What bounds a request's bill on Modal").
Restarts after a crash or a preemption beyond the price's allowance of 2 attempts are not
bounded by whizbang; the workspace's spend limit is the backstop.
"""

import os
import pathlib
import subprocess
import sys

import modal

# --- what a deployer may change, and nothing else names ------------------------------------

APP = "whizbang-sam3"
# The card: a 24 GB one.
CARD = "A10G"
# The bound of one attempt of the GPU function, in seconds. Their sum is `host_limit_s`.
LIMITS = {
    "startup_timeout": 300,
    "timeout": 240,
    "scaledown_window": 60,
}
HOST_LIMIT_S = 600
# The secret that holds the front's token, and the variable it is under: the variable the
# config's `token_env` names.
SECRET = "whizbang-front"
TOKEN_ENV = "SEGMENT_TOKEN"
VOLUME = "whizbang-models"
CLAIMS = "whizbang-claims"

# --- the rest -------------------------------------------------------------------------------

MODELS = "/models"
PACK = "/root/pack"
PORT = 8000
# The front's own container: its limit of one input (one connection), and how long it is
# kept idle. It is CPU-billed, and counted in the config's price per second.
FRONT_TIMEOUT_S = HOST_LIMIT_S
FRONT_IDLE_S = 60
FRONT_INPUTS = 64

if sum(LIMITS.values()) != HOST_LIMIT_S:
    raise SystemExit("sam3_host: the sum of LIMITS is not HOST_LIMIT_S")

app = modal.App(APP)
volume = modal.Volume.from_name(VOLUME, create_if_missing=True)

# The image is built from the deployer's checkout, which holds this file at
# deploy/modal/sam3_host.py beside the pack and the host's lock. In a container the file is
# /root/sam3_host.py, imported by Modal's entrypoint and again by the front's own process
# (`--worker sam3_host:side`), where `modal.is_local()` is True: there the definition is read
# again but not built, and the checkout is not there. So the checkout decides, by its files
# being there, and nothing here reads a file outside it.
_HERE = pathlib.Path(__file__).resolve()
_ROOT = _HERE.parent.parent.parent
_CHECKOUT = (
    _HERE == _ROOT / "deploy" / "modal" / "sam3_host.py"
    and (_ROOT / "pack" / ".python-version").is_file()
    and (_ROOT / "deploy" / "gpu-host" / "requirements.txt").is_file()
    and (_ROOT / "pack" / "src" / "whizbang_pack").is_dir()
)
if _CHECKOUT:
    _PYTHON = ".".join((_ROOT / "pack" / ".python-version").read_text().strip().split(".")[:2])
else:
    _ROOT = pathlib.Path("/whizbang-checkout-is-not-here")
    _PYTHON = "3.12"
_REQUIREMENTS = str(_ROOT / "deploy" / "gpu-host" / "requirements.txt")
_SOURCES = str(_ROOT / "pack" / "src" / "whizbang_pack")

image = (
    modal.Image.debian_slim(python_version=_PYTHON)
    .pip_install_from_requirements(_REQUIREMENTS)
    .env(
        {
            "PYTHONPATH": PACK + ":/root",
            "HF_HUB_OFFLINE": "1",
            "HF_HUB_DISABLE_TELEMETRY": "1",
            "DO_NOT_TRACK": "1",
        }
    )
    # Caches are never uploaded; `scripts/gpu-host.sh deploy` refuses a tree with any file
    # git ignores there.
    .add_local_dir(_SOURCES, PACK + "/whizbang_pack", ignore=["**/__pycache__", "**/*.pyc"])
)


@app.cls(image=image, gpu=CARD, volumes={MODELS: volume}, max_containers=1, **LIMITS)
@modal.concurrent(max_inputs=2)
class Bridge:
    @modal.enter()
    def start(self):
        from whizbang_pack import front

        claims = modal.Dict.from_name(CLAIMS, create_if_missing=True)
        store = front.StoreClaims(lambda key, value: claims.put(key, value, skip_if_exists=True))
        self.front = front
        self.side = front.LocalSide(MODELS, claims=store)

    @modal.method()
    def call(self, job):
        yield from self.front.relay(self.side, job)

    @modal.method()
    def cancel(self, dispatch):
        return self.side.cancel(dispatch)

    @modal.exit()
    def stop(self):
        self.side.close()


def side(models_dir, options):
    """The front's worker's side (`--worker sam3_host:side`): the bridge, by its name in the
    deployed app."""
    from whizbang_pack import front

    bridge = modal.Cls.from_name(APP, "Bridge")()
    return front.Bridged(lambda job: bridge.call.remote_gen(job), lambda dispatch: bridge.cancel.remote(dispatch))


@app.function(
    image=image,
    volumes={MODELS: volume},
    secrets=[modal.Secret.from_name(SECRET)],
    max_containers=1,
    timeout=FRONT_TIMEOUT_S,
    scaledown_window=FRONT_IDLE_S,
)
@modal.concurrent(max_inputs=FRONT_INPUTS)
@modal.web_server(PORT, startup_timeout=60)
def front():
    command = [sys.executable, "-m", "whizbang_pack.front", "--listen", "0.0.0.0:%d" % PORT]
    command += ["--models-dir", MODELS, "--token-env", TOKEN_ENV, "--worker", "sam3_host:side"]
    command += ["--host-limit-s", str(HOST_LIMIT_S)]
    subprocess.Popen(command, env=dict(os.environ))
