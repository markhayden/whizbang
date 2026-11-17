"""The pack's codec against the fixed vectors of `pack/1`, and the edges of a strict reader
that Python's own `json` would let through (docs/plans/m1c-local-pack.md, P1's notes for P2).
"""

import io
import json
import random
import struct
import unittest

import kit
from whizbang_pack import proto
from whizbang_pack.proto import Payload, Reader, Refused

BINDING = "sha256:" + "92b2fa58028958317e408bd84ecfa70f5ee35b121991dbc232c49d166353708b"
INSTALLATION = "sha256:" + "77f358414083a02e8d99d0d097f53bb7c63890ce465acbb742020e63961699fd"


def frame_of(header):
    if isinstance(header, str):
        header = header.encode("utf-8")
    return len(header).to_bytes(4, "big") + header


def read(frame, open_requests=None):
    return Reader(io.BytesIO(frame), open_requests or {}).read()


def reason(frame, open_requests=None):
    try:
        read(frame, open_requests)
    except Refused as refused:
        return refused.reason
    return None


def segment_open(queries=3, most=2, width=8, height=6):
    return {
        "r1": {
            "id": "r1",
            "kind": "segment",
            "events": 0,
            "text": 0,
            "width": width,
            "height": height,
            "binding": BINDING,
            "queries": queries,
            "max_instances": most,
        }
    }


def instance(query, score, box, iou=None, fill=0, width=8, height=6):
    return {
        "query": query,
        "score": score,
        "iou": iou,
        "box": list(box),
        "plane": Payload(width, height, BINDING, bytes([fill]) * (width * height)),
    }


def result(instances, diagnostics="", id="r1", device="cpu"):
    return {
        "type": "result",
        "id": id,
        "kind": "segment",
        "device": device,
        "threads": 3,
        "diagnostics": diagnostics,
        "instances": instances,
    }


def cancel_with_id(id_json):
    return frame_of('{"type":"cancel","id":%s}' % id_json)


def load_with_pad(pad_json):
    return frame_of(
        '{"type":"request","id":"r1","kind":"load","model":"fake-ok","installation":"%s",'
        '"settings":[{"name":"pad","value":%s}]}' % (INSTALLATION, pad_json)
    )


def hello_with_threads(threads_json):
    return frame_of(
        '{"type":"hello","id":"h1","version":"pack/1","role":"worker","device":"cpu","threads":%s}'
        % threads_json
    )


class EveryVector(unittest.TestCase):
    def test_parses_and_is_written_back_byte_for_byte(self):
        seen = 0
        for vector in kit.vectors():
            if "message" not in vector:
                continue
            seen += 1
            with self.subTest(vector["name"]):
                frame = bytes.fromhex(vector["frame"])
                stream = io.BytesIO(frame)
                reader = Reader(stream, kit.open_table(vector))
                message = reader.read()
                self.assertEqual(kit.described(message), vector["message"])
                self.assertEqual(stream.tell(), len(frame), "the whole frame is read, no more")
                self.assertIsNone(reader.read(), "the input ends after the frame")
                self.assertEqual(proto.encode(message), frame)
        self.assertEqual(seen, 12)


class Refusals(unittest.TestCase):
    def test_each_refusal_vector_is_refused_for_its_stated_reason(self):
        seen = 0
        for vector in kit.vectors():
            if "refused" not in vector:
                continue
            seen += 1
            with self.subTest(vector["name"]):
                self.assertIn(vector["refused"], proto.REASONS)
                frame = bytes.fromhex(vector["frame"])
                self.assertEqual(reason(frame, kit.open_table(vector)), vector["refused"])
        self.assertEqual(seen, 50)

    def test_a_refusal_says_fixed_words_and_quotes_only_a_plain_version(self):
        try:
            read(frame_of('{"type":"hello","id":"h1","version":"pack/2","role":"whizbang"}'))
        except Refused as refused:
            self.assertEqual(refused.reason, "version")
            self.assertIn("pack/2", refused.sentence)
            self.assertIn("pack/1", refused.sentence)
        else:
            self.fail("another version was read")
        try:
            read(frame_of('{"type":"hello","id":"h1","version":"x/../y","role":"whizbang"}'))
        except Refused as refused:
            self.assertNotIn("..", refused.sentence)
        else:
            self.fail("another version was read")


class StrictJson(unittest.TestCase):
    """Each case Python's `json` takes and `pack/1` does not (ADR 0020, "What a reader takes
    as JSON"), with the reason it is refused for."""

    def test_the_header_is_strict_utf8_before_it_is_json(self):
        # `json.loads` of bytes decodes with surrogatepass: a surrogate encoded as bytes
        # would come through as a character.
        self.assertEqual(reason(frame_of(b'{"type":"cancel","id":"r\xed\xa0\x80"}')), "not_json")
        self.assertEqual(reason(frame_of(b'{"type":"cancel","id":"r\xed\xbf\xbf"}')), "not_json")
        self.assertEqual(reason(frame_of(b'{"type":"cancel","id":"r\xc0\xb1"}')), "not_json")
        # An escaped lone surrogate, of either half, names no character; a pair does, and is
        # then not the canonical writing of its character.
        self.assertEqual(reason(cancel_with_id('"r\\udc00"')), "not_json")
        self.assertEqual(reason(cancel_with_id('"r\\ud800x"')), "not_json")
        self.assertEqual(reason(cancel_with_id('"r\\ud800\\u0041"')), "not_json")
        self.assertEqual(reason(cancel_with_id('"r\\ud83d\\ude00"')), "bad_value")
        # A lone surrogate in a key is no character either.
        self.assertEqual(reason(frame_of('{"type":"cancel","id":"r1","\\udfff":1}')), "not_json")

    def test_a_byte_order_mark_is_refused(self):
        self.assertEqual(reason(frame_of(b'\xef\xbb\xbf{"type":"cancel","id":"r1"}')), "not_json")

    def test_nan_infinities_and_numbers_outside_a_double_are_refused(self):
        for text in ("NaN", "Infinity", "-Infinity", "1e400", "-1e400", "1.5e309"):
            with self.subTest(text):
                self.assertEqual(reason(cancel_with_id(text)), "not_json")
                self.assertEqual(reason(load_with_pad(text)), "not_json")
        # A whole number past a double's range, and one past the 4,300 digits that
        # Python 3.11 and later refuse to make into an int: each is no JSON number here.
        for digits in (309, 400, 4301, 5000):
            with self.subTest(digits=digits):
                self.assertEqual(reason(load_with_pad("9" * digits)), "not_json")
                self.assertEqual(reason(load_with_pad("-" + "9" * digits)), "not_json")
        # Within a double's range is a number, and past 64 bits it breaks the rule of a value.
        self.assertEqual(reason(load_with_pad("9" * 300)), "bad_value")
        self.assertEqual(reason(load_with_pad("1e300")), "bad_value")

    def test_a_field_given_twice_is_refused(self):
        self.assertEqual(reason(frame_of('{"type":"cancel","id":"r1","id":"r2"}')), "not_json")
        self.assertEqual(
            reason(load_with_pad('1,"value":2')), "not_json", "twice inside a setting's object"
        )

    def test_nesting_is_taken_to_127_and_refused_at_128(self):
        def nested(depth):
            # The header's own object is the first.
            inner = "[" * (depth - 1) + "]" * (depth - 1)
            return cancel_with_id(inner)

        self.assertEqual(reason(nested(127)), "bad_value", "127 is JSON, and no id")
        self.assertEqual(reason(nested(128)), "not_json")
        self.assertEqual(reason(nested(400)), "not_json")
        # Brackets inside a text are not nesting.
        self.assertEqual(reason(cancel_with_id('"' + "[" * 200 + '"')), "bad_value")

    def test_what_json_has_not_is_refused(self):
        for text in ("01", "1.", ".5", "+1", "1e", '"\\x"', "1,", "'r1'"):
            with self.subTest(text):
                self.assertEqual(reason(cancel_with_id(text)), "not_json")
        self.assertEqual(reason(frame_of('{"type":"cancel","id":"r1",}')), "not_json")
        self.assertEqual(reason(frame_of('{"type":"cancel",id:"r1"}')), "not_json")
        self.assertEqual(reason(frame_of('{"type":"cancel","id":"r\x011"}')), "not_json")
        self.assertEqual(reason(frame_of('{"type":"cancel","id":"r1"} {}')), "not_json")
        self.assertEqual(reason(frame_of("[]")), "not_json")
        self.assertEqual(reason(frame_of('"cancel"')), "not_json")

    def test_negative_zero_true_and_false_are_no_whole_numbers(self):
        # Python reads -0 as the int 0, and `True` passes `isinstance(x, int)`.
        for text in ("-0", "true", "false", "1.0", "1e2", "-0.0", '"3"', "null"):
            with self.subTest(text):
                self.assertEqual(reason(hello_with_threads(text)), "bad_value")
        # A setting's value may be true or false, but not -0 or a number with a fraction.
        for text in ("-0", "1.0", "1e2", "-0.0", "null"):
            with self.subTest(text):
                self.assertEqual(reason(load_with_pad(text)), "bad_value")
        self.assertEqual(reason(hello_with_threads("3")), None)
        # A setting may be true or false, and the reader keeps it a boolean, not 1 or 0.
        message = read(load_with_pad("true"))
        self.assertIs(message["settings"]["pad"], True)
        message = read(load_with_pad("-5"))
        self.assertEqual(message["settings"]["pad"], -5)
        self.assertIs(type(message["settings"]["pad"]), int)


class Writing(unittest.TestCase):
    def test_a_number_with_a_fraction_is_the_sixteen_hex_digits_of_its_double(self):
        message = result([instance(0, 0.5, (0.0, -0.0, 2.0, 2.5), iou=0.75)])
        header = proto.encode(message)
        self.assertIn(b'"score":"3fe0000000000000"', header)
        self.assertIn(b'"iou":"3fe8000000000000"', header)
        self.assertIn(b'"box":["0000000000000000","8000000000000000","4000000000000000","4004000000000000"]', header)
        for value in (0.1, 1e-7, 5e-324, 1.7976931348623157e308, -0.0):
            self.assertEqual(proto.fraction(value), struct.pack(">d", value).hex())
        for value in (float("nan"), float("inf"), float("-inf")):
            with self.assertRaises(ValueError):
                proto.fraction(value)

    def test_the_writer_sorts_instances_in_the_total_order_whatever_order_they_came_in(self):
        box = (0.0, 0.0, 2.0, 2.0)
        ordered = [
            instance(1, 0.9, box),
            instance(0, 0.5, box),
            instance(1, 0.5, box),
            instance(1, 0.5, (0.0, 1.0, 2.0, 2.0)),
            instance(1, 0.5, (1.0, 1.0, 2.0, 2.0)),
            instance(1, 0.5, (1.0, 1.0, 3.0, 2.0)),
            instance(1, 0.5, (1.0, 1.0, 3.0, 3.0)),
            instance(1, 0.5, (1.0, 1.0, 3.0, 3.0), iou=-0.0),
            instance(1, 0.5, (1.0, 1.0, 3.0, 3.0), iou=0.0),
            instance(1, 0.5, (1.0, 1.0, 3.0, 3.0), iou=0.25, fill=0),
            instance(1, 0.5, (1.0, 1.0, 3.0, 3.0), iou=0.25, fill=255),
            # Best first by the bits' order: 0 before -0, though -score would tie them.
            instance(0, 0.0, box),
            instance(0, -0.0, box),
            # Among edges -0 comes before 0.
            instance(1, -0.0, (-0.0, 0.0, 2.0, 2.0)),
            instance(1, -0.0, (0.0, 0.0, 2.0, 2.0)),
        ]
        expected = proto.encode(result(ordered, id="r1"))
        shuffler = random.Random(20201115)
        for _ in range(20):
            shuffled = list(ordered)
            shuffler.shuffle(shuffled)
            self.assertEqual(proto.encode(result(shuffled)), expected)
        opened = segment_open(queries=2, most=64)
        message = read(expected, opened)
        self.assertEqual(
            [kit.described(item) for item in message["instances"]],
            [kit.described(item) for item in ordered],
        )
        scores = [struct.pack(">d", item["score"]).hex() for item in message["instances"]]
        self.assertEqual(scores[-4:], ["0000000000000000", "8000000000000000", "8000000000000000", "8000000000000000"])

    def test_a_result_whose_planes_are_out_of_order_is_refused_once_they_are_read(self):
        box = (0.0, 0.0, 2.0, 2.0)
        frame = proto.encode(result([instance(0, 0.5, box, fill=0), instance(0, 0.5, box, fill=255)]))
        self.assertIsNotNone(read(frame, segment_open()))
        first = frame.index(bytes(48))
        second = frame.index(bytes([255]) * 48)
        swapped = frame[:first] + frame[second : second + 48] + frame[first + 48 : second] + frame[first : first + 48]
        self.assertEqual(len(swapped), len(frame))
        self.assertEqual(reason(swapped, segment_open()), "out_of_order")

    def test_the_writer_cleans_texts_to_what_a_faults_sentence_may_hold(self):
        error = {"type": "error", "id": "r1", "fault": "rejected", "sentence": '  could not open /tmp/x\\y "quoted"\n'}
        frame = proto.encode(error)
        message = read(frame)
        self.assertNotIn("/", message["sentence"])
        self.assertNotIn("\\", message["sentence"])
        self.assertNotIn('"', message["sentence"])
        self.assertTrue(message["sentence"].startswith("could not open "))
        self.assertEqual(read(proto.encode(dict(error, sentence="x" * 300)))["sentence"], "x" * 240)
        self.assertEqual(read(proto.encode(dict(error, sentence=" \n\t")))["sentence"], "no reason was given")
        event = {"type": "event", "id": "r1", "text": "read C:\\models\u00e9 \u2028done"}
        opened = {"r1": {"id": "r1", "kind": "load", "events": 0, "text": 0}}
        text = read(proto.encode(event), opened)["text"]
        self.assertTrue(all(character in proto.TEXT_CHARACTERS for character in text), text)
        self.assertEqual(read(proto.encode(dict(event, text="\u00e9")), dict(opened))["text"], "no text was given")
        loaded = {"type": "result", "id": "r1", "kind": "load", "device": "cpu", "threads": 3, "diagnostics": "a/b"}
        self.assertEqual(read(proto.encode(loaded), {"r1": dict(opened["r1"])})["diagnostics"], "a b")

    def test_diagnostics_are_cut_so_a_result_never_meets_its_headers_bound(self):
        longest_id = "a" * 64
        longest_device = "a" * 32
        message = {
            "type": "result",
            "id": longest_id,
            "kind": "load",
            "device": longest_device,
            "threads": 65535,
            "diagnostics": "d" * 70_000,
        }
        frame = proto.encode(message)
        self.assertLess(len(frame) - 4, proto.HEADER_LIMIT)
        def opened():
            return {longest_id: {"id": longest_id, "kind": "load", "events": 0, "text": 0}}

        diagnostics = read(frame, opened())["diagnostics"]
        self.assertEqual(len(diagnostics), proto.DIAGNOSTICS_CUT)
        self.assertGreaterEqual(proto.DIAGNOSTICS_CUT, 63 * 1024)
        # A result with diagnostics at the full 64 KiB, which a reader takes, is not
        # its own canonical writing cut, and is read whole.
        full = dict(message, diagnostics="d" * 65_000)
        header = proto.header(full)
        self.assertEqual(read(frame_of(header), opened())["diagnostics"], "d" * 65_000)

    def test_the_writer_refuses_what_a_reader_would(self):
        with self.assertRaises(Refused):
            proto.encode({"type": "hello", "id": "h1", "version": "pack/2", "role": "whizbang"})
        with self.assertRaises(Refused):
            proto.encode({"type": "cancel", "id": "R1"})
        with self.assertRaises(Refused):
            plane = Payload(8, 6, BINDING, bytes(47))
            proto.encode(result([dict(instance(0, 0.5, (0.0, 0.0, 1.0, 1.0)), plane=plane)]))
        with self.assertRaises(ValueError):
            proto.encode(result([instance(0, float("nan"), (0.0, 0.0, 1.0, 1.0))]))


class Frames(unittest.TestCase):
    def test_a_frame_cut_at_any_byte_is_truncated_and_never_a_shorter_message(self):
        for name in ("request-segment", "result-segment", "request-load", "hello-worker"):
            vector = next(item for item in kit.vectors() if item["name"] == name)
            frame = bytes.fromhex(vector["frame"])
            for cut in range(1, len(frame)):
                with self.subTest(name=name, cut=cut):
                    self.assertEqual(reason(frame[:cut], kit.open_table(vector)), "truncated")
            self.assertIsNone(read(b""))

    def test_a_header_is_bounded_by_the_largest_request_open_and_then_by_its_own(self):
        big = segment_open(queries=64, most=64)
        bound = proto.HEADER_LIMIT + 512 * 64 * 64
        self.assertEqual(bound, 2_162_688)
        self.assertEqual(reason(bound.to_bytes(4, "big") + b"x", big), "truncated")
        self.assertEqual(reason((bound + 1).to_bytes(4, "big"), big), "header_too_large")
        self.assertEqual(reason((proto.HEADER_LIMIT + 1).to_bytes(4, "big")), "header_too_large")
        # A header that is no result is held to 64 KiB once its type is read.
        padded = '{"type":"event","id":"r1","text":"%s"}' % ("a" * proto.HEADER_LIMIT)
        self.assertEqual(reason(frame_of(padded), big), "header_too_large")

    def test_events_and_their_text_are_counted_for_their_request_and_a_result_closes_it(self):
        opened = segment_open()
        event = proto.encode({"type": "event", "id": "r1", "text": "a" * 100})
        stream = io.BytesIO(event * 3)
        reader = Reader(stream, opened)
        for _ in range(3):
            reader.read()
        self.assertEqual(opened["r1"]["events"], 3)
        self.assertEqual(opened["r1"]["text"], 300)
        done = proto.encode(result([]))
        reader = Reader(io.BytesIO(done + done), opened)
        reader.read()
        self.assertNotIn("r1", opened)
        with self.assertRaises(Refused) as caught:
            reader.read()
        self.assertEqual(caught.exception.reason, "unexpected")

    def test_a_request_is_noted_as_open_with_what_its_answer_is_held_to(self):
        image = Payload(8, 6, BINDING, bytes(192))
        request = {
            "type": "request",
            "id": "r1",
            "kind": "segment",
            "model": "fake-ok",
            "installation": INSTALLATION,
            "image": image,
            "queries": [{"phrase": "a dog"}, {"point": [1.5, 2.0]}],
            "max_instances": 3,
        }
        self.assertEqual(
            proto.awaiting(request),
            {"id": "r1", "kind": "segment", "events": 0, "text": 0, "width": 8, "height": 6, "binding": BINDING, "queries": 2, "max_instances": 3},
        )
        load = {"type": "request", "id": "r2", "kind": "load", "model": "fake-ok", "installation": INSTALLATION, "settings": {}}
        self.assertEqual(proto.awaiting(load), {"id": "r2", "kind": "load", "events": 0, "text": 0})


if __name__ == "__main__":
    unittest.main()
