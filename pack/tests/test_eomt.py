"""The adapter `eomt-ade20k` (plan mE, task T2; ADR 0052), against a stand-in for its runtime
that the tests give it: no runtime is imported here (D10).

`StandIn` takes the place of `eomt_ade20k.Torch`: it labels each pixel `sky` where its red
sample is 128 or more and `floor` elsewhere, with the confidences 0.8 and 0.6, and counts
how often it was asked to label a picture. Fake modules take the place of PyTorch and the
library under `Torch.load`, so that what it loads from is seen as the calls it makes.
"""

import io
import json
import os
import re
import shutil
import tempfile
import types
import unittest

import kit
from whizbang_pack import adapter, conformance, eomt_ade20k, proto, worker

INSTALLATION = "sha256:" + "e0" * 32
BINDING = "sha256:" + "92b2fa58028958317e408bd84ecfa70f5ee35b121991dbc232c49d166353708b"
OTHER = "sha256:" + "a1" * 32
WIDTH, HEIGHT = 8, 6
SKY, FLOOR = eomt_ade20k.CLASSES.index("sky"), eomt_ade20k.CLASSES.index("floor")
WIRING_RS = os.path.join(os.path.dirname(kit.PACK), "crates", "whizbang", "src", "wiring.rs")


def picture():
    """Red 255 in the three left columns, 0 elsewhere."""
    return bytes(sample for _ in range(HEIGHT) for x in range(WIDTH) for sample in ((255 if x < 3 else 0), 10, 20, 255))


def request(queries, binding=BINDING, most=1):
    return {
        "type": "request",
        "id": "segment-4",
        "kind": "segment",
        "model": "eomt-ade20k",
        "installation": INSTALLATION,
        "image": proto.Payload(WIDTH, HEIGHT, binding, picture()),
        "queries": queries,
        "max_instances": most,
    }


class Asked:
    def cancelled(self):
        return False


class StandIn:
    """The runtime, standing in: `sky` on the red pixels, `floor` on the rest."""

    def __init__(self, device="mps"):
        self.device = device
        self.folder = None
        self.labelled = 0
        self.unloaded = False

    def load(self, folder, device="auto"):
        self.folder, self.asked_device = folder, device

    def label(self, width, height, samples):
        self.labelled += 1
        return [SKY if samples[4 * at] >= 128 else FLOOR for at in range(width * height)], width

    def present(self, labelled):
        labels, width = labelled
        found = []
        for index, score in ((SKY, 0.8), (FLOOR, 0.6)):
            on = [at for at, label in enumerate(labels) if label == index]
            if on:
                xs, ys = [at % width for at in on], [at // width for at in on]
                found.append((index, score, [min(xs), min(ys), max(xs) + 1, max(ys) + 1]))
        return found

    def plane(self, labelled, index):
        labels, _ = labelled
        return bytes(255 if label == index else 0 for label in labels)

    def unload(self):
        self.unloaded = True


def installed_config(folder, classes=eomt_ade20k.CLASSES):
    """A folder of an installation's files holding a `config.json` of these labels, as the
    model's holds them (one with a trailing space). Its `files` is a link to a folder beside
    it, as the conformance suite makes one: a real `files/` with no record is refused."""
    files = os.path.join(folder, "files")
    real = folder + ".files"
    os.makedirs(real, exist_ok=True)
    os.makedirs(folder, exist_ok=True)
    os.symlink(real, files)
    labels = {str(index): (name + " " if name == "bed" else name) for index, name in enumerate(classes)}
    with open(os.path.join(real, "config.json"), "w", encoding="utf-8") as file:
        json.dump({"id2label": labels}, file)
    return real


def loaded(test, runtime, settings=None, classes=eomt_ade20k.CLASSES):
    scratch = tempfile.mkdtemp(prefix="eomt-")
    test.addCleanup(shutil.rmtree, scratch, True)
    installation = adapter.Installation(scratch, "eomt-ade20k", INSTALLATION)
    installed_config(installation.folder, classes)
    made = eomt_ade20k.EomtAde20k(runtime=lambda: runtime)
    said = made.load(installation, {} if settings is None else settings, Asked())
    return made, said


class Load(unittest.TestCase):
    def test_the_installations_classes_are_the_adapters_or_nothing_is_placed(self):
        runtime = StandIn()
        made, said = loaded(self, runtime, {"threshold": "0.3", "device": "cpu"})
        self.assertEqual(said, "loaded on mps; 150 classes; threshold taken and not used by eomt-ade20k")
        self.assertEqual((runtime.asked_device, os.path.basename(runtime.folder)), ("cpu", "files"))
        self.assertEqual(made.features, frozenset(("phrase", "labels")))
        self.assertEqual(made.classes, eomt_ade20k.CLASSES)
        # Another list, or one in another order, is refused before the runtime is asked.
        for classes in (eomt_ade20k.CLASSES[:-1], eomt_ade20k.CLASSES[::-1]):
            runtime = StandIn()
            with self.assertRaises(adapter.Fault) as refused:
                loaded(self, runtime, classes=classes)
            self.assertEqual(refused.exception.fault, "unavailable")
            self.assertIsNone(runtime.folder)


class Phrases(unittest.TestCase):
    def test_a_class_is_its_pixels_and_any_other_phrase_is_refused(self):
        runtime = StandIn()
        made, _ = loaded(self, runtime)
        found, said = made.segment(request([{"phrase": "floor"}, {"phrase": "sky"}, {"phrase": "wall"}]), Asked())
        # One instance a class, its pixels, its confidence, the bounds of its plane; a class
        # no pixel has gives none; best score first.
        self.assertEqual(
            [(item.query, item.score, item.iou, item.box) for item in found],
            [(1, 0.8, None, [0.0, 0.0, 3.0, 6.0]), (0, 0.6, None, [3.0, 0.0, 8.0, 6.0])],
        )
        self.assertEqual(found[0].samples, bytes(255 if at % WIDTH < 3 else 0 for at in range(WIDTH * HEIGHT)))
        self.assertEqual(said, "2 of 3 classes asked are in the picture, on mps")
        # A second batch of the same picture labels nothing again; another picture does.
        made.segment(request([{"phrase": "ceiling"}]), Asked())
        self.assertEqual(runtime.labelled, 1)
        made.segment(request([{"phrase": "sky"}], binding=OTHER), Asked())
        self.assertEqual(runtime.labelled, 2)
        # A phrase that is not a class, and a point, are refused before anything is computed.
        for queries, fault in (([{"phrase": "a red thing"}], "rejected"), ([{"point": [1.0, 1.0]}], "unsupported")):
            with self.assertRaises(adapter.Fault) as refused:
                made.segment(request(queries, binding="sha256:" + "b2" * 32), Asked())
            self.assertEqual(refused.exception.fault, fault)
        self.assertIn(eomt_ade20k.LISTED, "the phrase is none of the 150 classes eomt-ade20k labels, listed in " + eomt_ade20k.LISTED)
        self.assertEqual(runtime.labelled, 2)
        # The sentence is a text of the protocol.
        self.assertEqual(proto.clean(refused.exception.sentence, 240), refused.exception.sentence)
        made.unload(Asked())
        self.assertTrue(runtime.unloaded)


class Conformance(unittest.TestCase):
    def test_the_worker_answers_its_features_and_every_class_labels_one_pixel(self):
        # Through the worker, as `pack/1` brings a load and segment requests: a class alone
        # and every class of the list in batches of 64, each answer within the contract, and
        # the planes of all of them disjoint and covering the picture.
        made = lambda: eomt_ade20k.EomtAde20k(lambda: StandIn())
        scratch = tempfile.mkdtemp(prefix="eomt-worker-")
        self.addCleanup(shutil.rmtree, scratch, True)
        installed_config(adapter.Installation(scratch, "eomt-ade20k", INSTALLATION).folder)
        work = worker.Worker(scratch, None, None, None, None, exit=lambda code: None, adapters={"eomt-ade20k": made})
        load = {"type": "request", "id": "load-3", "kind": "load", "model": "eomt-ade20k", "installation": INSTALLATION, "settings": {}}
        self.assertEqual(work._perform(worker.Call(work, load))["device"], "mps")
        asked = request([{"phrase": "sky"}])
        kit.conforming(self, asked, work._perform(worker.Call(work, asked)))
        answered = []
        for asked in conformance.labels_requests(eomt_ade20k.CLASSES):
            asked = dict(asked, model="eomt-ade20k", installation=INSTALLATION)
            answered.append((asked, work._perform(worker.Call(work, asked))))
        self.assertEqual([len(asked["queries"]) for asked, _ in answered], [64, 64, 22])
        conformance.check_labels(answered)
        # Two classes on one pixel, or a pixel of none, break `labels`.
        first = answered[0][1]["instances"][0]
        twice = dict(answered[1][1], instances=[dict(first, query=0)])
        with self.assertRaises(conformance.Nonconforming) as broken:
            conformance.check_labels(answered + [(answered[1][0], twice)])
        self.assertIn("two classes label one pixel", str(broken.exception))
        with self.assertRaises(conformance.Nonconforming) as broken:
            conformance.check_labels(answered[1:])
        self.assertIn("a pixel no class labels", str(broken.exception))

    def test_the_suite_a_user_is_given_asks_labels_of_an_adapter_that_declares_them(self):
        # The suite's own run, over a folder of the model's files: it asks the first class
        # as the phrase, then every class.
        scratch = tempfile.mkdtemp(prefix="eomt-files-")
        self.addCleanup(shutil.rmtree, scratch, True)
        files = installed_config(os.path.join(scratch, "installation"))
        lines, whole = conformance.run(lambda: eomt_ade20k.EomtAde20k(lambda: StandIn()), files)
        self.assertEqual(lines, ["phrase: conforms", "labels: conforms"])
        self.assertTrue(whole)


def fake_modules(calls):
    """PyTorch and the library, as far as `Torch.load` reaches them, recording each call."""

    class Parameter:
        device = types.SimpleNamespace(type="mps")

    class Model:
        def to(self, device):
            calls.append(("to", device))
            return self

        def eval(self):
            calls.append(("eval",))

        def parameters(self):
            return iter([Parameter()])

    def from_pretrained(folder, **options):
        calls.append(("from_pretrained", folder, options))
        return Model()

    torch = types.SimpleNamespace(
        get_num_threads=lambda: 1,
        set_num_threads=lambda n: calls.append(("threads", n)),
        get_num_interop_threads=lambda: 1,
        set_num_interop_threads=lambda n: calls.append(("interop", n)),
        device=lambda name: "device:" + name,
        cuda=types.SimpleNamespace(is_available=lambda: False),
        backends=types.SimpleNamespace(mps=types.SimpleNamespace(is_available=lambda: True)),
    )
    transformers = types.SimpleNamespace(EomtForUniversalSegmentation=types.SimpleNamespace(from_pretrained=from_pretrained))
    return torch, transformers


class Layout(unittest.TestCase):
    def test_the_model_loads_from_the_installations_folder_with_the_network_off(self):
        calls = []
        torch, transformers = fake_modules(calls)
        runtime = eomt_ade20k.Torch(torch, transformers, None, None)
        runtime.load("/models/files", "auto")
        self.assertEqual(
            calls,
            [
                ("threads", 8),
                ("interop", 8),
                ("from_pretrained", "/models/files", {"local_files_only": True}),
                ("to", "device:mps"),
                ("eval",),
            ],
        )
        self.assertEqual(runtime.device, "mps")


class Revision(unittest.TestCase):
    def test_the_adapters_revision_is_the_one_the_binarys_table_names(self):
        with open(WIRING_RS, encoding="utf-8") as file:
            source = file.read()
        body = source[source.index("pub fn adapters()") :]
        body = body[: body.index("\n}\n")]
        table = re.findall(r'named\("([a-z0-9_-]+)", "([^"]+)", (true|false)\)', body)
        self.assertIn(("eomt-ade20k", eomt_ade20k.EomtAde20k.revision, "true"), table)
        self.assertIs(worker.ADAPTERS["eomt-ade20k"], eomt_ade20k.EomtAde20k)


if __name__ == "__main__":
    unittest.main()
