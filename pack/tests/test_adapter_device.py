"""The device an adapter asks its runtime for (ADR 0046, "Its device"; plan m6, task P1, item
4): CUDA, then the Apple GPU, then the CPU, with a fake runtime in place of PyTorch, which the
pack's tests never import."""

import io
import sys
import types
import unittest

from whizbang_pack import adapter, fake_models, proto, worker


def fake_torch(cuda=False, mps=False, card="NVIDIA L4"):
    """A runtime that has what it is told: `cuda.is_available`, `backends.mps.is_available`
    and the flags of TF32, both on, as PyTorch's defaults leave them on some versions."""
    return types.SimpleNamespace(
        cuda=types.SimpleNamespace(
            is_available=lambda: cuda,
            current_device=lambda: 0,
            get_device_name=lambda index: card,
        ),
        backends=types.SimpleNamespace(
            mps=types.SimpleNamespace(is_available=lambda: mps),
            cuda=types.SimpleNamespace(matmul=types.SimpleNamespace(allow_tf32=True)),
            cudnn=types.SimpleNamespace(allow_tf32=True),
        ),
    )


class OnCuda(fake_models.FakeOk):
    """A fake whose runtime placed it on CUDA, as `sam3` names the device its parameters are
    on."""

    device = "cuda"


class Device(unittest.TestCase):
    def test_auto_takes_cuda_then_mps_then_cpu(self):
        self.assertEqual(adapter.device_name(fake_torch(cuda=True, mps=True), "auto"), "cuda")
        self.assertEqual(adapter.device_name(fake_torch(cuda=False, mps=True), "auto"), "mps")
        self.assertEqual(adapter.device_name(fake_torch(cuda=False, mps=False), "auto"), "cpu")
        # A runtime with no CUDA module at all, as a Mac's build may be.
        bare = fake_torch(mps=True)
        del bare.cuda
        self.assertEqual(adapter.device_name(bare, "auto"), "mps")

    def test_cpu_forces_the_cpu_where_cuda_is_there(self):
        torch = fake_torch(cuda=True, mps=True)
        self.assertEqual(adapter.device_name(torch, "cpu"), "cpu")
        # Nothing of CUDA's was set for a model on the CPU.
        self.assertTrue(torch.backends.cuda.matmul.allow_tf32)
        self.assertTrue(torch.backends.cudnn.allow_tf32)
        self.assertEqual(adapter.device_of({"device": "cpu"}), "cpu")

    def test_on_cuda_tf32_is_off_and_the_card_is_reported(self):
        torch = fake_torch(cuda=True, card="NVIDIA L4 " + "x" * 100)
        self.assertEqual(adapter.device_name(torch, "auto"), "cuda")
        self.assertFalse(torch.backends.cuda.matmul.allow_tf32)
        self.assertFalse(torch.backends.cudnn.allow_tf32)
        self.assertEqual(adapter.card_name(torch), ("NVIDIA L4 " + "x" * 100)[: adapter.CARD_NAME_MOST])
        self.assertIsNone(adapter.card_name(fake_torch(cuda=False)))
        # The worker says the card beside the device a result names: the runtime is the one
        # the adapter imported, found where the import put it.
        saved = sys.modules.get("torch")
        sys.modules["torch"] = fake_torch(cuda=True)
        try:
            made = worker.Worker("/nowhere", None, io.BytesIO(), io.BytesIO(), io.StringIO(), exit=lambda code: None, adapters={"fake-ok": OnCuda})
            call = worker.Call(made, {"type": "request", "id": "r1", "kind": "load", "model": "fake-ok", "installation": "sha256:" + "11" * 32, "settings": {}})
            message = made._perform(call)
            # A load of the model already resident names the card too (ADR 0048).
            again = made._perform(worker.Call(made, dict(call.request, id="r2")))
        finally:
            if saved is None:
                del sys.modules["torch"]
            else:
                sys.modules["torch"] = saved
        self.assertEqual(message["device"], "cuda")
        self.assertIn("card NVIDIA L4", message["diagnostics"])
        proto.encode(message)
        self.assertEqual(again["diagnostics"], "already loaded; card NVIDIA L4")
        proto.encode(again)


if __name__ == "__main__":
    unittest.main()
