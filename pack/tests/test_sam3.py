"""The adapter `sam3` (docs/plans/m1c-local-pack.md, task R2a), against stand-ins for its
runtime that the tests give it: no runtime is imported here (D10).

Two stand-ins. `StandIn` takes the place of the runtime as a whole (`sam3.Torch`), and
answers each phrase with the candidates a test gives it, counting what it was asked. The
fake modules `fake_torch` and `fake_transformers` take the place of PyTorch and the
library under `sam3.Torch` itself, so that its placing on a device, its pools and its
offline load are seen as the calls they make.
"""

import os
import re
import types
import unittest

import kit
from whizbang_pack import adapter, proto, sam3, worker

INSTALLATION = "sha256:" + "37ab46c2f87b600f4654dd972ce85a27b1024b4fc92901b11464bd6fa9664d32"
BINDING = "sha256:" + "92b2fa58028958317e408bd84ecfa70f5ee35b121991dbc232c49d166353708b"
WIDTH, HEIGHT = 8, 6
WIRING_RS = os.path.join(os.path.dirname(kit.PACK), "crates", "whizbang", "src", "wiring.rs")


def image():
    return proto.Payload(WIDTH, HEIGHT, BINDING, bytes(4 * WIDTH * HEIGHT))


def request(queries, most=4):
    return {
        "type": "request",
        "id": 9,
        "kind": "segment",
        "model": "sam3",
        "installation": INSTALLATION,
        "image": image(),
        "queries": queries,
        "max_instances": most,
    }


def phrases(*texts):
    return [{"phrase": text} for text in texts]


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


class StandIn:
    """The runtime, standing in: `answers` gives each phrase its candidates, each a score
    and a box in the picture's pixels; `places` gives a point or a box its candidates,
    each a predicted IoU and a box; a candidate's plane is on inside its box."""

    def __init__(self, answers, device="mps", places=()):
        self.answers = answers
        self.place_answers = list(places)
        self.device = device
        self.folder = None
        self.tracker_folders = []
        self.features_made = 0
        self.places_made = 0
        self.asked = []
        self.asked_places = []
        self.planes = []
        self.unloaded = False

    def load(self, folder):
        self.folder = folder

    def load_tracker(self, folder):
        self.tracker_folders.append(folder)

    def places(self, width, height, samples):
        self.places_made += 1
        return ("places", width, height, len(samples))

    def place_candidates(self, places, kind, given):
        assert places == ("places", WIDTH, HEIGHT, 4 * WIDTH * HEIGHT)
        self.asked_places.append((kind, given))
        return ("place",), [(index, iou) for index, (iou, _) in enumerate(self.place_answers)]

    def features(self, width, height, samples):
        self.features_made += 1
        return ("features", width, height, len(samples))

    def candidates(self, features, phrase):
        assert features == ("features", WIDTH, HEIGHT, 4 * WIDTH * HEIGHT)
        self.asked.append(phrase)
        return phrase, [(index, score, box) for index, (score, box) in enumerate(self.answers.get(phrase, []))]

    def plane(self, features, answer, index):
        self.planes.append((answer, index))
        if answer == ("place",):
            return box_plane(self.place_answers[index][1])
        return box_plane(self.answers[answer][index][1])

    def unload(self):
        self.unloaded = True


def loaded(runtime, settings=None):
    made = sam3.Sam3(runtime=lambda: runtime)
    installation = adapter.Installation("/models", "sam3", INSTALLATION)
    said = made.load(installation, {} if settings is None else settings, Asked())
    return made, said


def answered(found):
    return [(item.query, item.score, item.box) for item in found]


class Phrase(unittest.TestCase):
    def test_features_are_computed_once_for_a_batch_of_phrases(self):
        runtime = StandIn({"cat": [(0.9, [0, 0, 2, 2])], "dog": [(0.8, [4, 0, 6, 2])], "cow": []})
        made, _ = loaded(runtime)
        found, _ = made.segment(request(phrases("cat", "dog", "cow")), Asked())
        self.assertEqual(runtime.features_made, 1)
        self.assertEqual(runtime.asked, ["cat", "dog", "cow"])
        self.assertEqual(answered(found), [(0, 0.9, [0.0, 0.0, 2.0, 2.0]), (1, 0.8, [4.0, 0.0, 6.0, 2.0])])
        # A second request computes its own picture's features, once.
        made.segment(request(phrases("cat", "dog")), Asked())
        self.assertEqual(runtime.features_made, 2)

    def test_instances_under_the_threshold_are_left_out_and_the_rest_are_in_the_fixed_order(self):
        answers = {
            "a": [(0.6, [4, 2, 6, 4]), (0.45, [0, 0, 1, 1]), (0.9, [2, 2, 4, 4]), (0.6, [4, 0, 6, 2])],
            "b": [(0.9, [0, 0, 2, 2]), (0.2, [6, 4, 8, 6]), (0.5, [6, 0, 8, 2])],
        }
        made, said = loaded(StandIn(answers))
        self.assertEqual(said, "loaded on mps; threshold 0.5")
        found, _ = made.segment(request(phrases("a", "b")), Asked())
        # Best score first; among equal scores, by query, then by the box's top and left. A
        # score at the threshold is kept, as rascal kept it.
        self.assertEqual(
            answered(found),
            [
                (0, 0.9, [2.0, 2.0, 4.0, 4.0]),
                (1, 0.9, [0.0, 0.0, 2.0, 2.0]),
                (0, 0.6, [4.0, 0.0, 6.0, 2.0]),
                (0, 0.6, [4.0, 2.0, 6.0, 4.0]),
                (1, 0.5, [6.0, 0.0, 8.0, 2.0]),
            ],
        )
        # The worker's writer takes that order as it is: the frame's instances are the same.
        self.assertEqual(found, sorted(found, key=sam3._fixed))
        # The setting moves the threshold: at 0.4 the candidate at 0.45 is in, at 0.95 none is.
        made, said = loaded(StandIn(answers), {"threshold": "0.4"})
        self.assertEqual(said, "loaded on mps; threshold 0.4")
        found, _ = made.segment(request(phrases("a", "b")), Asked())
        self.assertIn((0, 0.45, [0.0, 0.0, 1.0, 1.0]), answered(found))
        self.assertEqual(len(found), 6)
        made, _ = loaded(StandIn(answers), {"threshold": "0.95"})
        self.assertEqual(made.segment(request(phrases("a", "b")), Asked())[0], [])
        # A threshold that is not a text of a number from 0 to 1 is refused at the load.
        for wrong in ("1.5", "-0.1", "half", True, 1):
            with self.assertRaises(adapter.Fault) as refused:
                loaded(StandIn(answers), {"threshold": wrong})
            self.assertEqual(refused.exception.fault, "rejected")
        # The other settings of the kind are taken, not used, and the load says so.
        _, said = loaded(StandIn(answers), {"min_cover": "0.2", "outline": "whole", "pad": "0.05"})
        self.assertEqual(said, "loaded on mps; threshold 0.5; min_cover, outline, pad taken and not used by sam3")

    def test_candidates_are_cut_to_the_most_instances_asked_before_a_full_size_plane_is_made(self):
        # Ten candidates over the threshold for one phrase, three for the other; two asked
        # for each. A plane is made for the best two of each, and for no other.
        many = [(round(0.5 + index / 20, 2), [index % 8, 0, index % 8 + 1, 1]) for index in range(10)]
        few = [(0.7, [0, 4, 1, 5]), (0.95, [1, 4, 2, 5]), (0.8, [2, 4, 3, 5])]
        runtime = StandIn({"many": many, "few": few})
        made, _ = loaded(runtime)
        found, _ = made.segment(request(phrases("many", "few"), most=2), Asked())
        self.assertEqual(sorted(runtime.planes), [("few", 1), ("few", 2), ("many", 8), ("many", 9)])
        self.assertEqual([(item.query, item.score) for item in found], [(0, 0.95), (1, 0.95), (0, 0.9), (1, 0.8)])
        # A plane with nothing on gives no instance, and no other is made in its place.
        empty = StandIn({"thin": [(0.9, [3, 3, 3, 3]), (0.8, [0, 0, 1, 1])]})
        made, _ = loaded(empty)
        found, _ = made.segment(request(phrases("thin"), most=1), Asked())
        self.assertEqual((found, empty.planes), ([], [("thin", 0)]))


# Three candidates for a place, as the tracker gives them: its predicted IoU and its box.
THREE = [(0.31, [0, 0, 8, 6]), (0.99, [2, 1, 5, 4]), (0.24, [3, 2, 4, 3])]


class Prompts(unittest.TestCase):
    def test_a_point_is_asked_in_the_pictures_frame_and_answers_its_candidates_best_first(self):
        runtime = StandIn({}, places=THREE)
        made, _ = loaded(runtime)
        self.assertEqual(made.features, frozenset(("phrase", "point", "box")))
        found, said = made.segment(request([{"point": [3.5, 2.25]}]), Asked())
        # Asked as the request gave it, in the picture's pixels: the runtime places it.
        self.assertEqual(runtime.asked_places, [("point", [3.5, 2.25])])
        # Best predicted IoU first; the score and the IoU are both that prediction, and
        # each box is the bounds of what its plane has on.
        self.assertEqual(
            [(item.query, item.score, item.iou, item.box) for item in found],
            [
                (0, 0.99, 0.99, [2.0, 1.0, 5.0, 4.0]),
                (0, 0.31, 0.31, [0.0, 0.0, 8.0, 6.0]),
                (0, 0.24, 0.24, [3.0, 2.0, 4.0, 3.0]),
            ],
        )
        self.assertEqual(found[0].samples, box_plane([2, 1, 5, 4]))
        self.assertEqual(said, "3 instances on mps")
        # No threshold is of an IoU: the setting is the phrase's.
        made, _ = loaded(StandIn({}, places=THREE), {"threshold": "0.95"})
        self.assertEqual(len(made.segment(request([{"point": [1.0, 1.0]}]), Asked())[0]), 3)
        # Cut to the most asked before any plane is made: two asked, two planes.
        runtime = StandIn({}, places=THREE)
        made, _ = loaded(runtime)
        found, _ = made.segment(request([{"point": [1.0, 1.0]}], most=2), Asked())
        self.assertEqual([item.score for item in found], [0.99, 0.31])
        self.assertEqual(runtime.planes, [(("place",), 1), (("place",), 0)])
        # A candidate whose plane has nothing on gives no instance.
        runtime = StandIn({}, places=[(0.9, [3, 3, 3, 3]), (0.5, [0, 0, 1, 1])])
        made, _ = loaded(runtime)
        found, _ = made.segment(request([{"point": [1.0, 1.0]}]), Asked())
        self.assertEqual([item.score for item in found], [0.5])

    def test_a_box_is_asked_as_its_corners_and_answers_its_candidates_best_first(self):
        runtime = StandIn({"cat": [(0.9, [0, 0, 2, 2])]}, places=THREE)
        made, _ = loaded(runtime)
        queries = [{"box": [1.0, 0.5, 6.0, 5.0]}, {"phrase": "cat"}, {"point": [2.0, 2.0]}]
        found, _ = made.segment(request(queries), Asked())
        self.assertEqual(runtime.asked_places, [("box", [1.0, 0.5, 6.0, 5.0]), ("point", [2.0, 2.0])])
        self.assertEqual(runtime.asked, ["cat"])
        # In the fixed order of the answer: best score first, then the query's place.
        self.assertEqual(
            [(item.query, item.score) for item in found],
            [(0, 0.99), (2, 0.99), (1, 0.9), (0, 0.31), (2, 0.31), (0, 0.24), (2, 0.24)],
        )
        self.assertEqual(found, sorted(found, key=sam3._fixed))
        # The picture's features, once for each model the batch asks: one of each.
        self.assertEqual((runtime.features_made, runtime.places_made), (1, 1))

    def test_the_tracker_is_loaded_once_on_the_first_point_or_box(self):
        runtime = StandIn({"cat": [(0.9, [0, 0, 2, 2])]}, places=THREE)
        made, _ = loaded(runtime)
        # Phrases alone load no tracker, and compute no feature of its.
        made.segment(request(phrases("cat")), Asked())
        self.assertEqual((runtime.tracker_folders, runtime.places_made), ([], 0))
        # The first point loads it, from the installation's own folder; the next box and
        # the next point do not load it again.
        made.segment(request([{"point": [1.0, 1.0]}]), Asked())
        made.segment(request([{"box": [0.0, 0.0, 4.0, 4.0]}, {"point": [2.0, 2.0]}]), Asked())
        self.assertEqual(runtime.tracker_folders, [FILES])
        # Points and boxes of one batch share the picture's features: two batches, two.
        self.assertEqual(runtime.places_made, 2)
        # After an unload and a load, the tracker is loaded again on the next point.
        made.unload(Asked())
        self.assertTrue(runtime.unloaded)
        made.load(adapter.Installation("/models", "sam3", INSTALLATION), {}, Asked())
        made.segment(request([{"point": [1.0, 1.0]}]), Asked())
        self.assertEqual(runtime.tracker_folders, [FILES, FILES])
        # On the runtime itself: the same library's tracker, from the same folder, offline,
        # on the device the model is on.
        torch = fake_torch(mps=True)
        runtime = sam3.Torch(torch, fake_transformers(torch))
        runtime.load(FILES)
        torch.calls.clear()
        runtime.load_tracker(FILES)
        self.assertEqual(
            torch.calls,
            [
                ("tracker", FILES, {"local_files_only": True}),
                ("to", "mps"),
                ("eval",),
                ("tracker_processor", FILES, {"local_files_only": True}),
            ],
        )
        runtime.unload()
        self.assertIsNone(runtime.tracker)

    def test_a_point_outside_the_picture_is_refused_as_unfit(self):
        runtime = StandIn({"cat": [(0.9, [0, 0, 2, 2])]}, places=THREE)
        made, _ = loaded(runtime)
        for query in (
            {"point": [8.0, 1.0]},
            {"point": [1.0, 6.0]},
            {"box": [0.0, 0.0, 9.0, 6.0]},
            {"box": [2.0, 2.0, 2.0, 4.0]},
        ):
            with self.assertRaises(adapter.Fault) as refused:
                made.segment(request(phrases("cat") + [query]), Asked())
            self.assertEqual(refused.exception.fault, "rejected")
        # Refused before anything is computed or loaded.
        self.assertEqual((runtime.features_made, runtime.places_made, runtime.tracker_folders), (0, 0, []))
        # The picture's last pixel, and a box of the whole picture, are inside it.
        made.segment(request([{"point": [7.5, 5.5]}, {"box": [0.0, 0.0, 8.0, 6.0]}]), Asked())

    def test_the_worker_refuses_a_kind_an_adapter_does_not_declare(self):
        class PhraseOnly(sam3.Sam3):
            features = frozenset(("phrase",))

        runtime = StandIn({}, places=THREE)
        made = PhraseOnly(runtime=lambda: runtime)
        made.load(adapter.Installation("/models", "sam3", INSTALLATION), {}, Asked())
        work = worker.Worker("/models", None, None, None, None, exit=lambda code: None, adapters={"sam3": lambda: made})
        work.resident = worker.Resident("sam3", INSTALLATION, None, made)
        call = worker.Call(work, request([{"point": [1.0, 1.0]}]))
        with self.assertRaises(adapter.Fault) as refused:
            work._segment(call)
        self.assertEqual((refused.exception.fault, refused.exception.sentence), ("unsupported", "the model takes no point"))
        self.assertEqual((runtime.places_made, runtime.tracker_folders), (0, []))


def fake_torch(mps):
    """PyTorch as far as `sam3.Torch` asks it: what it was asked, in order, in `calls`."""
    calls = []
    pools = {"intra": 16, "inter": 16}

    def device(name):
        calls.append(("device", name))
        return types.SimpleNamespace(type=name)

    def setter(pool):
        def set_threads(count):
            calls.append(("set_" + pool, count))
            pools[pool] = count

        return set_threads

    def is_available():
        calls.append(("mps_available",))
        return mps

    return types.SimpleNamespace(
        calls=calls,
        device=device,
        get_num_threads=lambda: pools["intra"],
        set_num_threads=setter("intra"),
        get_num_interop_threads=lambda: pools["inter"],
        set_num_interop_threads=setter("inter"),
        backends=types.SimpleNamespace(mps=types.SimpleNamespace(is_available=is_available)),
        mps=types.SimpleNamespace(empty_cache=lambda: calls.append(("empty_cache",))),
    )


def fake_transformers(torch, lands=None):
    """The library as far as `sam3.Torch` asks it. Its model's parameters are on the device
    it was moved to, or on `lands` when given: a runtime that put them elsewhere."""

    class Model:
        def __init__(self):
            self.on = types.SimpleNamespace(type="cpu")

        @classmethod
        def from_pretrained(cls, folder, **options):
            torch.calls.append(("model", folder, options))
            return cls()

        def to(self, device):
            torch.calls.append(("to", device.type))
            self.on = types.SimpleNamespace(type=lands or device.type)
            return self

        def eval(self):
            torch.calls.append(("eval",))

        def parameters(self):
            return iter([types.SimpleNamespace(device=self.on)])

    class Processor:
        @classmethod
        def from_pretrained(cls, folder, **options):
            torch.calls.append(("processor", folder, options))
            return cls()

    class Tracker(Model):
        @classmethod
        def from_pretrained(cls, folder, **options):
            torch.calls.append(("tracker", folder, options))
            return cls()

    class TrackerProcessor:
        @classmethod
        def from_pretrained(cls, folder, **options):
            torch.calls.append(("tracker_processor", folder, options))
            return cls()

    return types.SimpleNamespace(
        Sam3Model=Model, Sam3Processor=Processor, Sam3TrackerModel=Tracker, Sam3TrackerProcessor=TrackerProcessor
    )


def loaded_on(torch, transformers):
    return loaded(sam3.Torch(torch, transformers))


FILES = "/models/models/sam3/installations/" + INSTALLATION[len("sha256:") :] + "/files"


class Device(unittest.TestCase):
    def test_the_device_reported_is_the_one_the_runtime_gave(self):
        torch = fake_torch(mps=True)
        made, said = loaded_on(torch, fake_transformers(torch))
        self.assertEqual(made.device, "mps")
        self.assertEqual(said, "loaded on mps; threshold 0.5")
        # The pools first, at 8 each; then the GPU asked for, by name; the model loaded from
        # the installation's own files alone, and placed on it with `.to`, no `device_map`.
        self.assertEqual(
            torch.calls,
            [
                ("set_intra", 8),
                ("set_inter", 8),
                ("mps_available",),
                ("device", "mps"),
                ("model", FILES, {"local_files_only": True}),
                ("to", "mps"),
                ("eval",),
                ("processor", FILES, {"local_files_only": True}),
            ],
        )
        # What is reported is where the parameters are, not what was asked for.
        torch = fake_torch(mps=True)
        made, said = loaded_on(torch, fake_transformers(torch, lands="cpu"))
        self.assertEqual((made.device, said), ("cpu", "loaded on cpu; threshold 0.5"))
        # A pool already at 8 is not asked again: the inter-op pool can be sized once.
        torch = fake_torch(mps=True)
        torch.set_num_interop_threads(8)
        torch.calls.clear()
        loaded_on(torch, fake_transformers(torch))
        self.assertNotIn(("set_inter", 8), torch.calls)

    def test_with_no_gpu_the_adapter_runs_on_the_cpu_and_says_so(self):
        torch = fake_torch(mps=False)
        made, said = loaded_on(torch, fake_transformers(torch))
        self.assertEqual((made.device, said), ("cpu", "loaded on cpu; threshold 0.5"))
        self.assertIn(("device", "cpu"), torch.calls)
        self.assertIn(("to", "cpu"), torch.calls)
        self.assertNotIn(("device", "mps"), torch.calls)

    def test_the_worker_says_the_adapters_device_in_its_results_and_its_own_in_its_hello(self):
        # Before a model is loaded the worker's work runs on the CPU (task P7a): its hello
        # says `cpu`. Once `sam3` is loaded, every result names the device its runtime gave.
        torch = fake_torch(mps=True)
        made = sam3.Sam3(runtime=lambda: sam3.Torch(torch, fake_transformers(torch)))
        work = worker.Worker("/models", None, None, None, None, exit=lambda code: None, adapters={"sam3": lambda: made})
        self.assertEqual(adapter.runtime_device(), "cpu")
        load = {"type": "request", "id": 3, "kind": "load", "model": "sam3", "installation": INSTALLATION, "settings": {}}
        result = work._perform(worker.Call(work, load))
        self.assertEqual((result["device"], result["diagnostics"]), ("mps", "loaded on mps; threshold 0.5"))
        unload = {"type": "request", "id": 4, "kind": "unload", "model": "sam3", "installation": INSTALLATION}
        self.assertEqual(work._perform(worker.Call(work, unload))["device"], "cpu")
        self.assertIn(("empty_cache",), torch.calls)

    def test_without_the_runtime_installed_a_load_is_unavailable_and_says_to_install_it(self):
        made = sam3.Sam3()
        installation = adapter.Installation("/models", "sam3", INSTALLATION)
        with self.assertRaises(adapter.Fault) as refused:
            made.load(installation, {}, Asked())
        self.assertEqual(refused.exception.fault, "unavailable")
        self.assertEqual(refused.exception.sentence, "the runtime is not installed; run whizbang setup --extra torch")


class Revision(unittest.TestCase):
    def test_the_adapters_revision_is_the_one_the_binarys_table_names(self):
        with open(WIRING_RS, encoding="utf-8") as file:
            source = file.read()
        body = source[source.index("pub fn adapters()") :]
        body = body[: body.index("\n}\n")]
        table = re.findall(r'named\("([a-z0-9_-]+)", "([^"]+)", (true|false)\)', body)
        # The table's row of `sam3`; `owlv2-sam2` beside it is held by `test_tiny`.
        self.assertEqual([row for row in table if row[0] == "sam3"], [("sam3", sam3.Sam3.revision, "true")])
        self.assertEqual(sam3.Sam3.revision, "2")
        # And the worker answers for it under that name.
        self.assertIs(worker.ADAPTERS["sam3"], sam3.Sam3)


if __name__ == "__main__":
    unittest.main()
