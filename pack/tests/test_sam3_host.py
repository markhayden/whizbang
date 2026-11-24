"""The GPU host's deployment, `deploy/modal/sam3_host.py`, read as Python with Modal's client
faked (docs/plans/m6-http-and-remote-hosting.md, tasks D1 and L1): nothing here imports
Modal's client or contacts a host.

In a container the file is `/root/sam3_host.py`, with no checkout above it, and it is
imported twice: by Modal's entrypoint, and by the front's own process, which names it as its
worker's side (`--worker sam3_host:side`) and where `modal.is_local()` is True. Both must
import it; only the deployer's checkout builds the image."""

import importlib.util
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock

from whizbang_pack import front

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEPLOYMENT = ROOT / "deploy" / "modal" / "sam3_host.py"


def fake_modal():
    """A stand-in for Modal's client with the names the deployment uses: decorators that
    hand back what they are given, an image that records its layers, and `Cls.from_name`
    that records what it was asked. `is_local()` says True, as in the front's process."""
    modal = types.ModuleType("modal")
    modal.asked = []
    modal.layers = []

    def identity(*_args, **_kwargs):
        return lambda thing: thing

    class Image:
        @staticmethod
        def debian_slim(python_version):
            modal.layers.append(("python", python_version))
            return Image()

        def pip_install_from_requirements(self, path):
            modal.layers.append(("requirements", path))
            return self

        def env(self, values):
            return self

        def add_local_dir(self, path, remote, ignore=()):
            modal.layers.append(("sources", path))
            return self

    class App:
        def __init__(self, name):
            self.name = name

        cls = staticmethod(identity)
        function = staticmethod(identity)

    class Named:
        @staticmethod
        def from_name(*names, **_kwargs):
            return names

    class Remote:
        def __init__(self):
            self.remote = lambda *args: None
            self.remote_gen = lambda *args: iter(())

    class Cls:
        @staticmethod
        def from_name(app, name):
            modal.asked.append((app, name))
            return lambda: types.SimpleNamespace(call=Remote(), cancel=Remote())

    modal.is_local = lambda: True
    modal.App = App
    modal.Image = Image
    modal.Volume = modal.Secret = modal.Dict = Named
    modal.Cls = Cls
    modal.concurrent = modal.web_server = modal.enter = modal.method = modal.exit = identity
    return modal


class Deployment(unittest.TestCase):
    def setUp(self):
        self.modal = fake_modal()
        patched = mock.patch.dict(sys.modules, {"modal": self.modal})
        patched.start()
        # Restores sys.modules whole: the deployment imported under its own name leaves.
        self.addCleanup(patched.stop)

    def test_copied_where_no_checkout_is_above_it_it_imports_and_the_worker_its_front_names_resolves(self):
        folder = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, folder, True)
        root = os.path.join(folder, "root")
        os.mkdir(root)
        shutil.copy(DEPLOYMENT, os.path.join(root, "sam3_host.py"))
        sys.path.insert(0, root)
        self.addCleanup(sys.path.remove, root)
        sys.modules.pop("sam3_host", None)

        deployment = importlib.import_module("sam3_host")
        self.assertTrue(self.modal.is_local())
        # Nothing of the image names the folder it was copied to: no checkout is read there.
        self.assertFalse([layer for layer in self.modal.layers if folder in str(layer[1])])
        # The worker the front starts with is the one it names, and it resolves to the bridge.
        with mock.patch.object(subprocess, "Popen") as popen:
            deployment.front()
        command = popen.call_args[0][0]
        worker = command[command.index("--worker") + 1]
        self.assertEqual(worker, "sam3_host:side")
        side = front.side_of(worker, "/models", {})
        self.assertIsInstance(side, front.Bridged)
        self.assertEqual(self.modal.asked, [(deployment.APP, "Bridge")])

    def test_in_the_checkout_the_image_is_built_from_its_lock_its_sources_and_its_python(self):
        spec = importlib.util.spec_from_file_location("sam3_host_checkout", DEPLOYMENT)
        deployment = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(deployment)
        python = ".".join((ROOT / "pack" / ".python-version").read_text().strip().split(".")[:2])
        self.assertEqual(
            self.modal.layers,
            [
                ("python", python),
                ("requirements", str(ROOT / "deploy" / "gpu-host" / "requirements.txt")),
                ("sources", str(ROOT / "pack" / "src" / "whizbang_pack")),
            ],
        )


if __name__ == "__main__":
    unittest.main()
