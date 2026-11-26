"""The adapter `owlv2-sam2` of the model `tiny` (docs/plans/m2b-checkpoint-1d.md, task T1),
against stand-ins for its runtime that the tests give it: no runtime is imported here (D10).

`StandIn` takes the place of the runtime as a whole (`owlv2_sam2.Torch`): OWLv2 answers each
phrase with the proposals a test gives it, and SAM 2.1 answers a box or points with the
candidates a test's `draw` makes, each a predicted IoU and a box its plane is on in. Fake
modules take the place of PyTorch, the library and the weights' reader under
`owlv2_sam2.Torch` itself, so that what it builds each model from is seen as the calls it
makes.
"""

import json
import os
import re
import shutil
import tempfile
import types
import unittest

import kit
from whizbang_pack import adapter, owlv2_sam2, proto, worker

INSTALLATION = "sha256:" + "5be1ad7c4b6c1a2fd4b7d02f00e2bb2f0fd0b7d9a1fd28c3f6d0b7e8a9c1d2e3"
BINDING = "sha256:" + "92b2fa58028958317e408bd84ecfa70f5ee35b121991dbc232c49d166353708b"
WIDTH, HEIGHT = 20, 16
WIRING_RS = os.path.join(os.path.dirname(kit.PACK), "crates", "whizbang", "src", "wiring.rs")


def image():
    return proto.Payload(WIDTH, HEIGHT, BINDING, bytes(4 * WIDTH * HEIGHT))


def request(queries, most=4):
    return {
        "type": "request",
        "id": "segment-9",
        "kind": "segment",
        "model": "tiny",
        "installation": INSTALLATION,
        "image": image(),
        "queries": queries,
        "max_instances": most,
    }


class Asked:
    """The `call` an adapter is handed, never cancelled."""

    def cancelled(self):
        return False


def box_plane(box):
    """The samples of a plane on at every pixel whose centre is inside `box`."""
    left, top, right, bottom = box
    return bytes(
        255 if left <= x + 0.5 < right and top <= y + 0.5 < bottom else 0 for y in range(HEIGHT) for x in range(WIDTH)
    )


def three(box, points):
    """What SAM 2.1 stands in with: three candidates, each a predicted IoU and the box its
    plane is on in. The first is the box asked, or a 2 by 2 square at the first point."""
    if box is None:
        x, y = points[0]
        box = [x - 1, y - 1, x + 1, y + 1]
    left, top, right, bottom = box
    return [(0.9, list(box)), (0.6, [left, top, (left + right) / 2, bottom]), (0.3, [0, 0, WIDTH, HEIGHT])]


class StandIn:
    """The runtime, standing in, counting what it was asked."""

    def __init__(self, proposals=None, draw=three, device="mps"):
        self.proposals = proposals or {}
        self.draw = draw
        self.device = device
        self.files = None
        self.detected = []
        self.embedded = 0
        self.asked = []
        self.planes = 0
        self.unloaded = False

    def load(self, files, device="auto"):
        self.files = files
        self.asked_device = device

    def detect(self, width, height, samples, phrases):
        self.detected.append(list(phrases))
        return [self.proposals.get(phrase, []) for phrase in phrases]

    def embed(self, width, height, samples):
        self.embedded += 1
        return ("features", width, height)

    def outlines(self, features, box, given):
        assert features == ("features", WIDTH, HEIGHT)
        self.asked.append((None if box is None else [round(edge, 3) for edge in box], list(given)))
        candidates = self.draw(box, given)
        return candidates, [(index, iou) for index, (iou, _) in enumerate(candidates)]

    def plane(self, features, answer, index):
        self.planes += 1
        drawn = answer[index][1]
        return drawn if isinstance(drawn, bytes) else box_plane(drawn)

    def unload(self):
        self.unloaded = True


def loaded(runtime, settings=None):
    made = owlv2_sam2.Owlv2Sam2(runtime=lambda: runtime)
    installation = adapter.Installation("/models", "tiny", INSTALLATION)
    said = made.load(installation, {} if settings is None else settings, Asked())
    return made, said


def answered(found):
    return [(item.query, item.score, item.iou, item.box) for item in found]


class Phrases(unittest.TestCase):
    def test_owlv2_boxes_over_the_threshold_are_padded_cut_and_drawn_in_the_fixed_order(self):
        proposals = {
            "cat": [
                (0.9, [2, 2, 10, 10]),
                # Overlaps the first by more than half: suppressed.
                (0.85, [3, 3, 11, 11]),
                (0.5, [12, 2, 18, 8]),
                # Over the threshold, and cut: two are asked for.
                (0.45, [2, 12, 8, 15]),
                # Under a pixel wide: dropped.
                (0.4, [0, 0, 0.5, 4]),
                # At the threshold and under it: left out.
                (0.2, [14, 12, 18, 14]),
                (0.1, [0, 0, 20, 16]),
            ],
            "dog": [(0.3, [0, 10, 6, 16]), (0.95, [12, 10, 20, 16])],
            "cow": [],
        }
        runtime = StandIn(proposals)
        made, said = loaded(runtime)
        self.assertEqual(said, "loaded on mps; threshold 0.2; pad 0.05; outline whole; min_cover 0.2")
        found, said = made.segment(request([{"phrase": p} for p in ("cat", "dog", "cow")], most=2), Asked())
        # OWLv2 is asked every phrase at once, SAM 2.1's features are made once.
        self.assertEqual((runtime.detected, runtime.embedded), ([["cat", "dog", "cow"]], 1))
        # Only the boxes kept are drawn, each padded by 5% of its side on each axis and held
        # to the picture; nothing is drawn for a box cut, dropped or left out.
        self.assertEqual(
            sorted(runtime.asked),
            [
                ([0.0, 9.7, 6.3, 16.0], []),
                ([1.6, 1.6, 10.4, 10.4], []),
                ([11.6, 9.7, 20.0, 16.0], []),
                ([11.7, 1.7, 18.3, 8.3], []),
            ],
        )
        # Three candidates made full size for each box drawn, for the rule to pick from.
        self.assertEqual(runtime.planes, 12)
        # The score is OWLv2's, the IoU the mask's, the box the bounds of what is on; best
        # score first, then by query.
        self.assertEqual(
            answered(found),
            [
                (1, 0.95, 0.9, [12.0, 10.0, 20.0, 16.0]),
                (0, 0.9, 0.9, [2.0, 2.0, 10.0, 10.0]),
                (0, 0.5, 0.9, [12.0, 2.0, 18.0, 8.0]),
                (1, 0.3, 0.9, [0.0, 10.0, 6.0, 16.0]),
            ],
        )
        self.assertEqual(found, sorted(found, key=owlv2_sam2._fixed))
        self.assertEqual(found[1].samples, box_plane([1.6, 1.6, 10.4, 10.4]))
        self.assertEqual(said, "4 instances on mps; 0 retried, 0 retries kept")
        # A proposal at the threshold is not over it: at 0.5, the one at 0.5 is left out.
        made, _ = loaded(StandIn(proposals), {"threshold": "0.5"})
        self.assertEqual([item.score for item in made.segment(request([{"phrase": "cat"}]), Asked())[0]], [0.9])
        # The setting moves the threshold; with nothing over it, SAM 2.1 is asked nothing.
        runtime = StandIn(proposals)
        made, _ = loaded(runtime, {"threshold": "0.92"})
        found, _ = made.segment(request([{"phrase": "cat"}]), Asked())
        self.assertEqual((found, runtime.embedded, runtime.asked), ([], 0, []))
        # A mask with nothing on gives no instance.
        runtime = StandIn(proposals, draw=lambda box, points: [(0.9, [3, 3, 3, 3])] * 3)
        made, _ = loaded(runtime)
        self.assertEqual(made.segment(request([{"phrase": "cat"}]), Asked())[0], [])


class Prompts(unittest.TestCase):
    def test_a_box_and_a_point_are_drawn_by_sam2_alone(self):
        runtime = StandIn({"cat": [(0.9, [2, 2, 10, 10])]})
        made, _ = loaded(runtime, {"pad": "0.5", "threshold": "0.95"})
        self.assertEqual(made.features, frozenset(("phrase", "point", "box")))
        queries = [{"box": [4.0, 2.0, 12.0, 10.0]}, {"point": [6.5, 6.5]}]
        found, _ = made.segment(request(queries, most=2), Asked())
        # OWLv2 is asked nothing; SAM 2.1 is asked the box as it was given, unpadded, and
        # the point alone, in the picture's pixels; its features once for the batch.
        self.assertEqual(runtime.detected, [])
        self.assertEqual(runtime.asked, [([4.0, 2.0, 12.0, 10.0], []), (None, [(6.5, 6.5)])])
        self.assertEqual(runtime.embedded, 1)
        # Each answers its candidates best predicted IoU first, cut to two before any is
        # made full size; the score and the IoU are both that prediction; no threshold.
        self.assertEqual(runtime.planes, 4)
        self.assertEqual(
            answered(found),
            [
                (0, 0.9, 0.9, [4.0, 2.0, 12.0, 10.0]),
                (1, 0.9, 0.9, [5.0, 5.0, 7.0, 7.0]),
                (0, 0.6, 0.6, [4.0, 2.0, 8.0, 10.0]),
                (1, 0.6, 0.6, [5.0, 5.0, 6.0, 7.0]),
            ],
        )
        # With a phrase in the batch, SAM 2.1's features are still made once.
        runtime = StandIn({"cat": [(0.9, [2, 2, 10, 10])]})
        made, _ = loaded(runtime)
        made.segment(request([{"phrase": "cat"}, {"point": [1.0, 1.0]}]), Asked())
        self.assertEqual((runtime.detected, runtime.embedded), ([["cat"]], 1))
        # A point outside the picture, or a box not inside it, is refused before anything
        # is computed.
        runtime = StandIn({"cat": [(0.9, [2, 2, 10, 10])]})
        made, _ = loaded(runtime)
        for query in ({"point": [20.0, 1.0]}, {"point": [1.0, 16.0]}, {"box": [0.0, 0.0, 21.0, 6.0]}, {"box": [2, 2, 2, 4]}):
            with self.assertRaises(adapter.Fault) as refused:
                made.segment(request([{"phrase": "cat"}, query]), Asked())
            self.assertEqual(refused.exception.fault, "rejected")
        self.assertEqual((runtime.detected, runtime.embedded, runtime.asked), ([], 0, []))

    def test_the_outline_rule_and_least_cover_are_the_models_settings(self):
        # SAM 2.1's three candidates for a box: a 2 by 2 square it is sure of, the whole box
        # less sure, and the picture least sure. A retry (with points) gives the box again.
        def draw(box, points):
            if points:
                return [(0.8, list(box)), (0.2, [0, 0, 1, 1]), (0.1, [0, 0, 2, 2])]
            return [(0.95, [4, 4, 6, 6]), (0.7, list(box)), (0.4, [0, 0, WIDTH, HEIGHT])]

        proposals = {"cat": [(0.9, [2, 2, 10, 10])]}
        cases = [
            # `whole` takes the largest of those inside the box and sure enough.
            ({}, [1.6, 1.6, 10.4, 10.4], 0.7, 1),
            # `best` takes the surest; at a least cover of 0 nothing is drawn again.
            ({"outline": "best", "min_cover": "0"}, [4.0, 4.0, 6.0, 6.0], 0.95, 1),
            # Covering 4 of the box's 64 pixels is under the least cover: drawn again in
            # the box padded twice as far, with a point on the square; the retry is larger
            # and inside its box, so it is kept.
            ({"outline": "best"}, [1.2, 1.2, 10.8, 10.8], 0.8, 2),
        ]
        for settings, kept, iou, asked in cases:
            runtime = StandIn(proposals, draw=draw)
            made, _ = loaded(runtime, settings)
            found, said = made.segment(request([{"phrase": "cat"}]), Asked())
            self.assertEqual(len(runtime.asked), asked, settings)
            self.assertEqual(found[0].samples, box_plane(kept), settings)
            self.assertEqual(found[0].iou, iou, settings)
        self.assertEqual(runtime.asked[1], ([1.2, 1.2, 10.8, 10.8], [(4, 4)]))
        self.assertEqual(said, "1 instances on mps; 1 retried, 1 retries kept")
        # A mask in two pieces, the larger under 80% of it, is drawn again however much it
        # covers, with a point on each piece; a retry that is no larger is not kept.
        halves = bytes(
            255 if (x < 6 or x >= 8) and 2 <= x < 10 and 2 <= y < 10 else 0 for y in range(HEIGHT) for x in range(WIDTH)
        )
        runtime = StandIn(proposals, draw=lambda box, points: [(0.9, halves), (0.5, [2, 2, 3, 3]), (0.1, [2, 2, 3, 3])])
        made, _ = loaded(runtime, {"min_cover": "0"})
        found, said = made.segment(request([{"phrase": "cat"}]), Asked())
        self.assertEqual(runtime.asked[1][1], [(3, 5), (8, 5)])
        self.assertEqual((found[0].samples, said), (halves, "1 instances on mps; 1 retried, 0 retries kept"))
        # Settings that break their rules are refused at the load.
        for wrong in ({"threshold": "1.5"}, {"pad": "-0.1"}, {"min_cover": "most"}, {"outline": "largest"}, {"pad": 0.1}, {"device": "mps"}):
            with self.assertRaises(adapter.Fault) as refused:
                loaded(StandIn(), wrong)
            self.assertEqual(refused.exception.fault, "rejected")
        # The setting `device` reaches the runtime (plan 1D, task V1): `auto` when not given.
        for settings, asked in (({}, "auto"), ({"device": "cpu"}, "cpu"), ({"device": "auto"}, "auto")):
            runtime = StandIn()
            loaded(runtime, settings)
            self.assertEqual(runtime.asked_device, asked)


def fake_modules(calls):
    """PyTorch, the library and the weights' reader, as far as `owlv2_sam2.Torch` asks them:
    what each was asked, in order, in `calls`."""

    class Model:
        def __init__(self, kind):
            self.kind = kind

        def to(self, device):
            calls.append(("to", self.kind, device.type))
            return self

        def eval(self):
            calls.append(("eval", self.kind))

        def parameters(self):
            return iter([types.SimpleNamespace(device=types.SimpleNamespace(type="mps"))])

    def model(kind):
        def from_pretrained(name, **options):
            calls.append(("model", kind, name, sorted(options)))
            assert options["config"] == ("config", kind) and options["local_files_only"] is True
            return Model(kind)

        return types.SimpleNamespace(from_pretrained=from_pretrained)

    def config(kind):
        def from_json_file(path):
            calls.append(("config", kind, path))
            return ("config", kind)

        return types.SimpleNamespace(from_json_file=from_json_file)

    def made(kind):
        def make(**given):
            calls.append((kind, sorted(given)))
            return types.SimpleNamespace(given=given)

        return make

    torch = types.SimpleNamespace(
        device=lambda name: types.SimpleNamespace(type=name),
        get_num_threads=lambda: 8,
        get_num_interop_threads=lambda: 8,
        backends=types.SimpleNamespace(mps=types.SimpleNamespace(is_available=lambda: True)),
    )
    transformers = types.SimpleNamespace(
        Owlv2ForObjectDetection=model("owlv2"),
        Owlv2Config=config("owlv2"),
        Sam2Model=model("sam2"),
        Sam2Config=config("sam2"),
        CLIPTokenizer=made("tokenizer"),
        Owlv2ImageProcessor=made("owlv2_images"),
        Sam2ImageProcessor=made("sam2_images"),
        Sam2Processor=made("sam2_processor"),
    )

    def load_weights(path):
        calls.append(("weights", path))
        return {}

    return torch, transformers, load_weights


class Layout(unittest.TestCase):
    def test_both_models_load_from_their_prefixed_files_with_the_network_off(self):
        models = tempfile.mkdtemp(prefix="whizbang-tiny-")
        try:
            installation = adapter.Installation(models, "tiny", INSTALLATION)
            files = os.path.dirname(installation.file("owlv2.config.json"))
            os.makedirs(files)
            # Each file under its model's prefix: what the catalogue installs.
            written = {
                "owlv2.vocab.json": {"a": 0},
                "owlv2.tokenizer_config.json": {"model_max_length": 16, "pad_token": "!", "do_lower_case": True},
                "owlv2.special_tokens_map.json": {"bos_token": {"content": "<|startoftext|>"}, "pad_token": "!"},
                "owlv2.preprocessor_config.json": {"image_processor_type": "Owlv2ImageProcessor", "size": 960},
                "sam2.preprocessor_config.json": {"image_processor_type": "Sam2ImageProcessorFast", "size": 1024},
            }
            for name, value in written.items():
                with open(os.path.join(files, name), "w", encoding="utf-8") as file:
                    file.write(json.dumps(value))
            with open(os.path.join(files, "owlv2.merges.txt"), "w", encoding="utf-8") as file:
                file.write("#version: 0.2\na b\nab c\n")
            self.assertEqual(
                sorted(prefix + "." + name for prefix, names in owlv2_sam2.FILES.items() for name in names),
                sorted(
                    [
                        "owlv2.added_tokens.json",
                        "owlv2.config.json",
                        "owlv2.merges.txt",
                        "owlv2.model.safetensors",
                        "owlv2.preprocessor_config.json",
                        "owlv2.special_tokens_map.json",
                        "owlv2.tokenizer_config.json",
                        "owlv2.vocab.json",
                        "sam2.config.json",
                        "sam2.model.safetensors",
                        "sam2.preprocessor_config.json",
                    ]
                ),
            )
            calls = []
            runtime = owlv2_sam2.Torch(*fake_modules(calls))
            made = owlv2_sam2.Owlv2Sam2(runtime=lambda: runtime)
            said = made.load(installation, {}, Asked())
            self.assertTrue(said.startswith("loaded on mps;"))
            at = os.path.join(files, "")
            # Each model from its config file and its weights file, by name; the library is
            # given no name to look up (`None`) and the network off.
            self.assertEqual(
                [call for call in calls if call[0] in ("config", "weights", "model")],
                [
                    ("config", "owlv2", at + "owlv2.config.json"),
                    ("weights", at + "owlv2.model.safetensors"),
                    ("model", "owlv2", None, ["config", "local_files_only", "state_dict"]),
                    ("config", "sam2", at + "sam2.config.json"),
                    ("weights", at + "sam2.model.safetensors"),
                    ("model", "sam2", None, ["config", "local_files_only", "state_dict"]),
                ],
            )
            self.assertIn(("to", "owlv2", "mps"), calls)
            self.assertIn(("to", "sam2", "mps"), calls)
            # The tokenizer and the processors from their own files' contents, not a folder.
            tokenizer = runtime.tokenizer.given
            self.assertEqual(tokenizer["vocab"], {"a": 0})
            self.assertEqual(tokenizer["merges"], [("a", "b"), ("ab", "c")])
            self.assertEqual(
                (tokenizer["pad_token"], tokenizer["bos_token"], tokenizer["model_max_length"]),
                ("!", "<|startoftext|>", 16),
            )
            self.assertNotIn("do_lower_case", tokenizer)
            self.assertEqual(runtime.detector_images.given, {"size": 960})
            self.assertEqual(runtime.segmenter_processor.given["image_processor"].given, {"size": 1024})
            # Nothing it was given names a folder: every path is one of the prefixed files.
            paths = [part for call in calls for part in call if isinstance(part, str) and os.sep in part]
            self.assertTrue(paths and all(os.path.dirname(path) == files for path in paths), paths)
        finally:
            shutil.rmtree(models)
        # Without the runtime installed, a load is `unavailable` and says what to run.
        with self.assertRaises(adapter.Fault) as refused:
            owlv2_sam2.Owlv2Sam2().load(adapter.Installation("/models", "tiny", INSTALLATION), {}, Asked())
        self.assertEqual(refused.exception.fault, "unavailable")


class Conformance(unittest.TestCase):
    def test_the_worker_answers_tinys_declared_features_within_the_contract(self):
        # Through the worker, as a load and a segment of `pack/1` come to it: each feature
        # Tiny declares, alone and together, at one and at three instances a query.
        proposals = {"cat": [(0.9, [2, 2, 10, 10]), (0.5, [12, 2, 18, 8]), (0.4, [0, 10, 6, 16])]}
        work = worker.Worker(
            "/models", None, None, None, None, exit=lambda code: None, adapters={"tiny": lambda: owlv2_sam2.Owlv2Sam2(lambda: StandIn(proposals))}
        )
        load = {"type": "request", "id": "load-3", "kind": "load", "model": "tiny", "installation": INSTALLATION, "settings": {}}
        self.assertEqual(work._perform(worker.Call(work, load))["device"], "mps")
        batches = [
            [{"phrase": "cat"}],
            [{"point": [6.5, 6.5]}],
            [{"box": [4.0, 2.0, 12.0, 10.0]}],
            [{"phrase": "cat"}, {"point": [6.5, 6.5]}, {"box": [4.0, 2.0, 12.0, 10.0]}, {"phrase": "dog"}],
        ]
        for queries in batches:
            for most in (1, 3):
                asked = request(queries, most)
                result = work._perform(worker.Call(work, asked))
                kit.conforming(self, asked, result)
                self.assertTrue(result["instances"], queries)
        # The answer is written as a frame of `pack/1`, and read back the same.
        chunks = proto.chunks(result)
        self.assertTrue(chunks)


class Revision(unittest.TestCase):
    def test_the_adapters_revision_is_the_one_the_binarys_table_names(self):
        with open(WIRING_RS, encoding="utf-8") as file:
            source = file.read()
        body = source[source.index("pub fn adapters()") :]
        body = body[: body.index("\n}\n")]
        table = re.findall(r'named\("([a-z0-9_-]+)", "([^"]+)", (true|false)\)', body)
        self.assertIn(("owlv2-sam2", owlv2_sam2.Owlv2Sam2.revision, "true"), table)
        self.assertEqual(owlv2_sam2.Owlv2Sam2.revision, "1")
        # The worker answers for the model `tiny` with it, and for `sam3` with its own.
        self.assertIs(worker.ADAPTERS["tiny"], owlv2_sam2.Owlv2Sam2)
        self.assertEqual(worker.SHIPPED, {"sam3": "sam3", "tiny": "owlv2-sam2", "eomt-ade20k": "eomt-ade20k"})


if __name__ == "__main__":
    unittest.main()
