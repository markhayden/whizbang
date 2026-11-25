"""The worker of the fake host's side for a card (plan m7, task B1, item 6; ADR 0048): the
pack's fakes, with one fake model more, `fake-cuda`, which answers as `fake-ok` does and
names the device `cuda`, and a stand-in for the runtime the worker asks for the card's
name. So the worker's own code names the card on the `load`, as it does on a GPU host,
and the front hands that on.

Not a test file, and not the pack's: nothing ships it. The pack's source is never changed
by it: what it adds is added to the tables of this process alone."""

import runpy
import sys
import types

from whizbang_pack import fake_models, worker

# The card the stand-in names.
CARD = "NVIDIA Fake A10G"


class FakeCuda(fake_models.FakeOk):
    """`fake-ok`, on a card: as a model its runtime placed on an NVIDIA card would be."""

    device = "cuda"


def _runtime():
    """What the worker reads the card's name from (`adapter.card_name`): a runtime that
    has one card, this one."""
    cuda = types.SimpleNamespace(
        is_available=lambda: True,
        current_device=lambda: 0,
        get_device_name=lambda index: CARD,
    )
    torch = types.ModuleType("torch")
    torch.cuda = cuda
    return torch


sys.modules["torch"] = _runtime()
worker.ADAPTERS["fake-cuda"] = FakeCuda
worker.BY_ADAPTER["fake-cuda"] = FakeCuda
runpy.run_module("whizbang_pack.fakes", run_name="__main__", alter_sys=True)
