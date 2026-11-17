"""The frames of `pack/1`, written and read strictly (docs/adr/0020-the-pack-protocol.md).

Written from that record and the vectors under `pack/conformance/vectors/` alone, so that it
is a second codec beside whizbang's own and the vectors hold the two to one another. The
standard library only (D10 of docs/plans/m1c-local-pack.md).

A message is a `dict` of the fields its header holds, by the same names, but for three:

- a number with a fraction is a `float` (on the wire, 16 hex digits of its double);
- a load's `settings` is a `dict` of each name's value, in the order of the names;
- a request's `image` and an instance's `plane` are each a `Payload`: the entry's width,
  height and binding with the samples. Its layout, data type, colour and alpha rule have
  one value where it is, and are not held.

`Reader(stream, open_requests).read()` reads one frame, or `None` at the end of the input
before a frame's first byte, and raises `Refused` with one reason of the record's list.
`encode(message)` writes one frame: it sorts a result's instances into the fixed order,
cleans each text of the protocol, cuts diagnostics so that a result never meets its
header's bound, and raises `Refused` for a message a reader would refuse.
"""

import json
import math
import re
import struct

VERSION = "pack/1"

# Bounds (ADR 0020, "Bounds").
HEADER_LIMIT = 65_536
INSTANCE_ALLOWANCE = 512
MOST_INSTANCES = 64 * 64
RESULT_HEADER_LIMIT = HEADER_LIMIT + INSTANCE_ALLOWANCE * MOST_INSTANCES
PIXEL_LIMIT = 50_000_000
PAYLOAD_CEILING = 1 << 30
SETTINGS_LIMIT = 32
QUERIES_LIMIT = 64
INSTANCES_LIMIT = 64
PHRASE_LIMIT = 512
EVENTS_LIMIT = 256
DIAGNOSTICS_LIMIT = 65_536
# Where a writer cuts diagnostics: under the 64 KiB a reader takes, with room in the
# header's own 64 KiB for the longest id and device there can be.
DIAGNOSTICS_CUT = 63 * 1024
SENTENCE_LIMIT = 240
ID_LIMIT = 64
DEVICE_LIMIT = 32
MODEL_LIMIT = 64
THREADS_LIMIT = 65_535
SETTING_NAME_LIMIT = 64
SETTING_TEXT_LIMIT = 1_024
DEPTH_LIMIT = 128

FAULTS = (
    "cancelled",
    "unsupported",
    "credential",
    "rejected",
    "unavailable",
    "timed_out",
    "malformed",
    "too_large",
)

REASONS = (
    "truncated",
    "header_too_large",
    "not_json",
    "unknown_message",
    "version",
    "unknown_kind",
    "unknown_field",
    "missing_field",
    "bad_value",
    "over_bound",
    "out_of_order",
    "not_canonical",
    "unexpected",
    "wrong_size",
    "wrong_binding",
    "too_large",
    "wrong_length",
)

# What a text of the protocol may hold: what a fault's sentence holds after cleaning.
TEXT_CHARACTERS = frozenset(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,;:'()%+-_"
)

# The fields of each object, in the order they are written.
_HELLO_WHIZBANG = ("type", "id", "version", "role")
_HELLO_WORKER = _HELLO_WHIZBANG + ("device", "threads")
_REQUEST = {
    "load": ("type", "id", "kind", "model", "installation", "settings"),
    "unload": ("type", "id", "kind", "model", "installation"),
    "segment": ("type", "id", "kind", "model", "installation", "image", "queries", "max_instances"),
}
_RESULT = {
    "load": ("type", "id", "kind", "device", "threads", "diagnostics"),
    "unload": ("type", "id", "kind", "device", "threads", "diagnostics"),
    "segment": ("type", "id", "kind", "device", "threads", "diagnostics", "instances"),
}
_EVENT = ("type", "id", "text")
_CANCEL = ("type", "id")
_ERROR = ("type", "id", "fault", "sentence")
_ENTRY = ("width", "height", "layout", "data", "colour", "alpha", "binding")
_INSTANCE = ("query", "score", "iou", "box", "plane")
_SETTING = ("name", "value")
_QUERY_KINDS = ("phrase", "point", "box")

# The rules of an entry beside its dimensions and binding: (layout, data, colour, alpha).
_IMAGE_RULES = ("rgba", "u8", "srgb", "straight")
_PLANE_RULES = ("plane", "u8", "none", "none")

_ID = re.compile(r"[a-z0-9][a-z0-9._-]*\Z")
_MODEL = re.compile(r"[a-z0-9][a-z0-9._-]*\Z")
_DEVICE = re.compile(r"[a-z][a-z0-9._:-]*\Z")
_SETTING_NAME = re.compile(r"[a-z][a-z0-9_]*\Z")
_DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
_FRACTION = re.compile(r"[0-9a-f]{16}\Z")
_PLAIN_VERSION = re.compile(r"[a-z][a-z0-9_-]{0,15}/[0-9]{1,4}\Z")
_SURROGATE = re.compile("[\ud800-\udfff]")
# A JSON text, its quotes and what is between them, to be taken out before brackets are
# counted. A header that does not close a text is refused as JSON anyway.
_JSON_TEXT = re.compile(r'"(?:[^"\\]|\\.)*"', re.DOTALL)
_NOT_BRACKET = re.compile(r"[^\[\]{}]+")

_I64_LEAST = -(1 << 63)
_I64_MOST = (1 << 63) - 1


class Refused(Exception):
    """A frame refused for one reason of ADR 0020's list, with a sentence of fixed words.

    `id` is the refused header's id when it was read and is a valid id, so that the side
    that refused can name the conversation in its error; it is never in the sentence."""

    def __init__(self, reason, sentence, id=None):
        assert reason in REASONS, reason
        super().__init__("%s: %s" % (reason, sentence))
        self.reason = reason
        self.sentence = sentence
        self.id = id


class Payload:
    """Raw samples and the entry that describes them: an image's (`rgba`, four samples a
    pixel) or a plane's (one sample a pixel)."""

    __slots__ = ("width", "height", "binding", "samples")

    def __init__(self, width, height, binding, samples=None):
        self.width = width
        self.height = height
        self.binding = binding
        self.samples = samples

    def __eq__(self, other):
        return isinstance(other, Payload) and (
            self.width,
            self.height,
            self.binding,
            self.samples,
        ) == (other.width, other.height, other.binding, other.samples)

    def __repr__(self):
        size = None if self.samples is None else len(self.samples)
        return "Payload(%d, %d, %s, %s bytes)" % (self.width, self.height, self.binding, size)


# --- numbers with a fraction ----------------------------------------------------------------


def fraction(value):
    """The 16 lower-case hex digits of a finite double's 64 bits, most significant first."""
    value = float(value)
    if not math.isfinite(value):
        raise ValueError("a number with a fraction must be finite")
    return struct.pack(">d", value).hex()


def _total(value):
    """IEEE 754's total order of a double, as a whole number: its bits read with a sign,
    the 63 lower bits of one under zero flipped. So -0 is less than 0."""
    bits = struct.unpack(">q", struct.pack(">d", value))[0]
    return bits ^ 0x7FFF_FFFF_FFFF_FFFF if bits < 0 else bits


def _header_key(item):
    """The fixed order of an answer, by what the header shows: best score first, then the
    query, the box's top, left, right and bottom, then the predicted IoU, none first."""
    box = item["box"]
    iou = item["iou"]
    return (
        -_total(item["score"]),
        item["query"],
        _total(box[1]),
        _total(box[0]),
        _total(box[2]),
        _total(box[3]),
        (0, 0) if iou is None else (1, _total(iou)),
    )


def _order_key(item):
    """The whole fixed order: the header's keys, then the plane's bytes, the lesser first."""
    return _header_key(item) + (item["plane"].samples or b"",)


# --- reading JSON strictly ------------------------------------------------------------------


class _NotJson(Exception):
    pass


class _NegativeZero:
    """`-0` as JSON writes it: a number, and no whole number (ADR 0020)."""

    __slots__ = ()


_NEGATIVE_ZERO = _NegativeZero()


def _whole_or_not_json(text):
    # A whole number past a double's range is not JSON here, as a number with a fraction
    # past it is. `float` of the text rounds as a double does, and has no limit of digits;
    # `int` refuses past 4,300 digits on Python 3.11 and later, and is not reached then.
    if math.isinf(float(text)):
        raise _NotJson()
    if text == "-0":
        return _NEGATIVE_ZERO
    return int(text)


def _fraction_or_not_json(text):
    value = float(text)
    if not math.isfinite(value):
        raise _NotJson()
    return value


def _constant(_name):
    # NaN, Infinity and -Infinity, which Python's `json` takes and JSON has not.
    raise _NotJson()


def _object(pairs):
    names = [name for name, _ in pairs]
    if len(set(names)) != len(names):
        raise _NotJson()
    return dict(pairs)


def _deepest(text):
    """How deep objects and lists nest in a JSON text, outside its texts."""
    brackets = _NOT_BRACKET.sub("", _JSON_TEXT.sub("", text))
    depth = deepest = 0
    for character in brackets:
        if character in "[{":
            depth += 1
            if depth > deepest:
                deepest = depth
        else:
            depth -= 1
    return deepest


def _holds_surrogate(value):
    if isinstance(value, str):
        return _SURROGATE.search(value) is not None
    if isinstance(value, dict):
        return any(_holds_surrogate(name) or _holds_surrogate(item) for name, item in value.items())
    if isinstance(value, list):
        return any(_holds_surrogate(item) for item in value)
    return False


def _parse_json(header):
    """One JSON object, strictly (ADR 0020, "What a reader takes as JSON"), or `not_json`."""
    try:
        # Strict UTF-8 first: `json.loads` of bytes would decode with surrogatepass.
        text = header.decode("utf-8", "strict")
    except UnicodeDecodeError:
        raise _not_json("is not strict UTF-8") from None
    if text.startswith("﻿"):
        raise _not_json("starts with a byte order mark")
    if _deepest(text) >= DEPTH_LIMIT:
        raise _not_json("nests %d deep or more" % DEPTH_LIMIT)
    try:
        value = json.loads(
            text,
            object_pairs_hook=_object,
            parse_int=_whole_or_not_json,
            parse_float=_fraction_or_not_json,
            parse_constant=_constant,
            strict=True,
        )
    except (_NotJson, ValueError, RecursionError):
        raise _not_json("is not one JSON value, or gives a field twice") from None
    if not isinstance(value, dict):
        raise _not_json("is not one JSON object")
    if _holds_surrogate(value):
        raise _not_json("escapes a surrogate that is not half of a pair")
    return value


def _not_json(what):
    return Refused("not_json", "the header " + what)


# --- the rules of values --------------------------------------------------------------------


def _bad(what, rule, id=None):
    return Refused("bad_value", "%s must be %s" % (what, rule), id)


def _over(what, most, id=None):
    return Refused("over_bound", "%s is over its bound of %d" % (what, most), id)


def _is_whole(value):
    # `bool` is an `int` to Python, and `-0` was read as no whole number.
    return type(value) is int


def _whole(value, what, least, most, id, over=False):
    if not _is_whole(value) or value < least:
        raise _bad(what, "a whole number from %d to %d" % (least, most), id)
    if value > most:
        if over:
            raise _over(what, most, id)
        raise _bad(what, "a whole number from %d to %d" % (least, most), id)
    return value


def _named(value, pattern, limit, what, rule, id):
    if not isinstance(value, str) or len(value) > limit or not pattern.match(value):
        raise _bad(what, rule, id)
    return value


def _check_id(value, id=None):
    return _named(value, _ID, ID_LIMIT, "an id", "1 to 64 of lower-case letters, digits and . _ -", id)


def _check_device(value, id):
    return _named(value, _DEVICE, DEVICE_LIMIT, "a device", "a name of lower-case letters, digits and . _ - :", id)


def _check_model(value, id):
    return _named(value, _MODEL, MODEL_LIMIT, "a model", "a name of lower-case letters, digits and _ . -", id)


def _check_digest(value, what, id):
    if not isinstance(value, str) or not _DIGEST.match(value):
        raise _bad(what, "sha256: and 64 lower-case hex digits", id)
    return value


def _check_text(value, what, least, most, id, counted):
    if not isinstance(value, str) or len(value) < least or not all(c in TEXT_CHARACTERS for c in value):
        raise _bad(what, "a text of ASCII letters, digits, spaces and . , ; : ' ( ) % + - _", id)
    if len(value) > most:
        if counted:
            raise _over(what, most, id)
        raise _bad(what, "%d characters at most" % most, id)
    return value


def _check_fraction(value, what, id):
    if not isinstance(value, str) or not _FRACTION.match(value):
        raise _bad(what, "16 lower-case hex digits of a double", id)
    number = struct.unpack(">d", bytes.fromhex(value))[0]
    if not math.isfinite(number):
        raise _bad(what, "finite", id)
    return number


def _check_fields(obj, fields, id):
    """A field that is not its own, then one that is missing (ADR 0020, the order of rules)."""
    for name in obj:
        if name not in fields:
            raise Refused("unknown_field", "an object holds a field that is not its own", id)
    for name in fields:
        if name not in obj:
            raise Refused("missing_field", "an object lacks the field " + name, id)


def _check_object(value, fields, what, id):
    if not isinstance(value, dict):
        raise _bad(what, "an object", id)
    _check_fields(value, fields, id)
    return value


def _check_list(value, what, id):
    if not isinstance(value, list):
        raise _bad(what, "a list", id)
    return value


def _named_field(obj, name, id):
    """A field that says which fields its object has (`type`, `version`, `role`, `kind`):
    `missing_field` when absent and `bad_value` when it is no text."""
    if name not in obj:
        raise Refused("missing_field", "an object lacks the field " + name, id)
    value = obj[name]
    if not isinstance(value, str):
        raise _bad("the field " + name, "a text", id)
    return value


def _check_entry(value, what, rules, id):
    entry = _check_object(value, _ENTRY, what, id)
    width = _whole(entry["width"], "a width", 1, PIXEL_LIMIT, id, over=True)
    height = _whole(entry["height"], "a height", 1, PIXEL_LIMIT, id, over=True)
    if width * height > PIXEL_LIMIT:
        raise _over("an entry's pixels", PIXEL_LIMIT, id)
    if (entry["layout"], entry["data"], entry["colour"], entry["alpha"]) != rules:
        raise _bad(what, "a layout, data type, colour and alpha rule of " + "/".join(rules), id)
    binding = _check_digest(entry["binding"], "a binding", id)
    return Payload(width, height, binding)


def _check_setting_value(value, id):
    if isinstance(value, bool):
        return value
    if _is_whole(value):
        if not _I64_LEAST <= value <= _I64_MOST:
            raise _bad("a setting's value", "a whole number that fits in 64 bits with a sign", id)
        return value
    if isinstance(value, str):
        if (
            not 1 <= len(value) <= SETTING_TEXT_LIMIT
            or not all(" " <= c <= "~" for c in value)
            or value != value.strip(" ")
            or "/" in value
            or "\\" in value
        ):
            raise _bad("a setting's text", "1 to 1024 printable ASCII characters, no space at an end, no separator", id)
        return value
    raise _bad("a setting's value", "true, false, a whole number or a text", id)


def _check_settings(value, id):
    items = _check_list(value, "settings", id)
    if len(items) > SETTINGS_LIMIT:
        raise _over("settings", SETTINGS_LIMIT, id)
    settings = {}
    names = []
    for item in items:
        item = _check_object(item, _SETTING, "a setting", id)
        name = _named(item["name"], _SETTING_NAME, SETTING_NAME_LIMIT, "a setting's name", "a name of lower-case letters, digits and _", id)
        settings[name] = _check_setting_value(item["value"], id)
        names.append(name.encode("ascii"))
    if any(first >= second for first, second in zip(names, names[1:])):
        raise Refused("out_of_order", "settings are not in the order of their names, or one is given twice", id)
    return settings


def _check_queries(value, id):
    items = _check_list(value, "queries", id)
    if not items:
        raise _bad("queries", "1 to 64 of them", id)
    if len(items) > QUERIES_LIMIT:
        raise _over("queries", QUERIES_LIMIT, id)
    queries = []
    for item in items:
        if not isinstance(item, dict):
            raise _bad("a query", "an object", id)
        if not item:
            raise Refused("missing_field", "a query lacks a phrase, a point or a box", id)
        if len(item) > 1 or any(name not in _QUERY_KINDS for name in item):
            raise Refused("unknown_field", "a query holds a field that is not its own", id)
        (kind, given), = item.items()
        if kind == "phrase":
            if not isinstance(given, str) or not given:
                raise _bad("a phrase", "a text of 1 to 512 bytes", id)
            if len(given.encode("utf-8")) > PHRASE_LIMIT:
                raise _over("a phrase's bytes", PHRASE_LIMIT, id)
            queries.append({"phrase": given})
            continue
        count = 2 if kind == "point" else 4
        numbers = _check_list(given, "a " + kind, id)
        if len(numbers) != count:
            raise _bad("a " + kind, "%d numbers" % count, id)
        numbers = [_check_fraction(number, "a " + kind + "'s number", id) for number in numbers]
        if any(number < 0 for number in numbers):
            raise _bad("a " + kind, "not under zero", id)
        if kind == "box" and not (numbers[0] < numbers[2] and numbers[1] < numbers[3]):
            raise _bad("a box", "its left under its right and its top under its bottom", id)
        queries.append({kind: numbers})
    return queries


def _check_instances(value, id):
    items = _check_list(value, "instances", id)
    if len(items) > MOST_INSTANCES:
        raise _over("instances", MOST_INSTANCES, id)
    instances = []
    for item in items:
        item = _check_object(item, _INSTANCE, "an instance", id)
        query = _whole(item["query"], "a query's place", 0, QUERIES_LIMIT - 1, id)
        score = _check_fraction(item["score"], "a score", id)
        if not 0 <= score <= 1:
            raise _bad("a score", "from 0 to 1", id)
        iou = item["iou"]
        if iou is not None:
            iou = _check_fraction(iou, "a predicted IoU", id)
            if not 0 <= iou <= 1:
                raise _bad("a predicted IoU", "from 0 to 1, or null", id)
        box = _check_list(item["box"], "a box", id)
        if len(box) != 4:
            raise _bad("a box", "4 numbers", id)
        box = [_check_fraction(edge, "an edge of a box", id) for edge in box]
        plane = _check_entry(item["plane"], "a plane's entry", _PLANE_RULES, id)
        instances.append({"query": query, "score": score, "iou": iou, "box": box, "plane": plane})
    for first, second in zip(instances, instances[1:]):
        if _header_key(first) > _header_key(second):
            raise Refused("out_of_order", "instances are not in the fixed order", id)
    total = sum(item["plane"].width * item["plane"].height for item in instances)
    if total > PAYLOAD_CEILING:
        raise Refused("too_large", "a result's payloads together are over %d bytes" % PAYLOAD_CEILING, id)
    return instances


def _message(obj):
    """The message a parsed header holds, its rules looked at in ADR 0020's order: `type`;
    a hello's `version` and `role`, or a request's or a result's `kind`; in each object a
    field that is not its own, then one that is missing; then the values."""
    raw_id = obj.get("id")
    id = raw_id if isinstance(raw_id, str) and len(raw_id) <= ID_LIMIT and _ID.match(raw_id) else None

    kind_of_message = _named_field(obj, "type", id)
    if kind_of_message == "hello":
        version = _named_field(obj, "version", id)
        if version != VERSION:
            shown = version if _PLAIN_VERSION.match(version) else "another"
            raise Refused("version", "the hello states the version %s, and this side speaks %s" % (shown, VERSION), id)
        role = _named_field(obj, "role", id)
        if role not in ("whizbang", "worker"):
            raise _bad("a role", "whizbang or worker", id)
        _check_fields(obj, _HELLO_WORKER if role == "worker" else _HELLO_WHIZBANG, id)
        message = {"type": "hello", "id": _check_id(obj["id"], id), "version": version, "role": role}
        if role == "worker":
            device = obj["device"]
            message["device"] = None if device is None else _check_device(device, id)
            message["threads"] = _whole(obj["threads"], "a count of threads", 1, THREADS_LIMIT, id)
        return message

    if kind_of_message == "request":
        kind = _named_field(obj, "kind", id)
        if kind not in _REQUEST:
            raise Refused("unknown_kind", "a request of a kind %s has not" % VERSION, id)
        _check_fields(obj, _REQUEST[kind], id)
        message = {
            "type": "request",
            "id": _check_id(obj["id"], id),
            "kind": kind,
            "model": _check_model(obj["model"], id),
            "installation": _check_digest(obj["installation"], "an installation", id),
        }
        if kind == "load":
            message["settings"] = _check_settings(obj["settings"], id)
        elif kind == "segment":
            message["image"] = _check_entry(obj["image"], "an image's entry", _IMAGE_RULES, id)
            message["queries"] = _check_queries(obj["queries"], id)
            message["max_instances"] = _whole(obj["max_instances"], "the most instances", 1, INSTANCES_LIMIT, id, over=True)
        return message

    if kind_of_message == "result":
        kind = _named_field(obj, "kind", id)
        if kind not in _RESULT:
            raise Refused("unknown_kind", "a result of a kind %s has not" % VERSION, id)
        _check_fields(obj, _RESULT[kind], id)
        message = {
            "type": "result",
            "id": _check_id(obj["id"], id),
            "kind": kind,
            "device": _check_device(obj["device"], id),
            "threads": _whole(obj["threads"], "a count of threads", 1, THREADS_LIMIT, id),
            "diagnostics": _check_text(obj["diagnostics"], "diagnostics", 0, DIAGNOSTICS_LIMIT, id, counted=True),
        }
        if kind == "segment":
            message["instances"] = _check_instances(obj["instances"], id)
        return message

    if kind_of_message == "event":
        _check_fields(obj, _EVENT, id)
        return {
            "type": "event",
            "id": _check_id(obj["id"], id),
            "text": _check_text(obj["text"], "an event's text", 1, DIAGNOSTICS_LIMIT, id, counted=True),
        }

    if kind_of_message == "cancel":
        _check_fields(obj, _CANCEL, id)
        return {"type": "cancel", "id": _check_id(obj["id"], id)}

    if kind_of_message == "error":
        _check_fields(obj, _ERROR, id)
        fault = obj["fault"]
        if fault not in FAULTS:
            raise _bad("a fault", "one of the eight names", id)
        return {
            "type": "error",
            "id": _check_id(obj["id"], id),
            "fault": fault,
            "sentence": _check_text(obj["sentence"], "a sentence", 1, SENTENCE_LIMIT, id, counted=False),
        }

    raise Refused("unknown_message", "a message of a type %s has not" % VERSION, id)


def _payloads(message):
    """Each payload a message describes, with its count of bytes, in the order written."""
    if message["type"] == "request" and message["kind"] == "segment":
        image = message["image"]
        return [(image, 4 * image.width * image.height)]
    if message["type"] == "result" and message["kind"] == "segment":
        return [(item["plane"], item["plane"].width * item["plane"].height) for item in message["instances"]]
    return []


# --- the canonical writing ------------------------------------------------------------------

_ESCAPES = {code: "\\u%04x" % code for code in range(0x20)}
_ESCAPES.update({0x08: "\\b", 0x09: "\\t", 0x0A: "\\n", 0x0C: "\\f", 0x0D: "\\r", 0x22: '\\"', 0x5C: "\\\\"})


def _text(value):
    return '"' + value.translate(_ESCAPES) + '"'


def _whole_text(value):
    if not _is_whole(value):
        raise ValueError("a whole number is asked")
    return str(value)


def _scalar(value):
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, str):
        return _text(value)
    return _whole_text(value)


def _fraction_text(value):
    return '"' + fraction(value) + '"'


def _entry_text(payload, rules):
    layout, data, colour, alpha = rules
    return '{"width":%s,"height":%s,"layout":%s,"data":%s,"colour":%s,"alpha":%s,"binding":%s}' % (
        _whole_text(payload.width),
        _whole_text(payload.height),
        _text(layout),
        _text(data),
        _text(colour),
        _text(alpha),
        _text(payload.binding),
    )


def _query_text(query):
    ((kind, given),) = query.items()
    if kind == "phrase":
        return '{"phrase":%s}' % _text(given)
    return '{%s:[%s]}' % (_text(kind), ",".join(_fraction_text(number) for number in given))


def _instance_text(item):
    return '{"query":%s,"score":%s,"iou":%s,"box":[%s],"plane":%s}' % (
        _whole_text(item["query"]),
        _fraction_text(item["score"]),
        "null" if item["iou"] is None else _fraction_text(item["iou"]),
        ",".join(_fraction_text(edge) for edge in item["box"]),
        _entry_text(item["plane"], _PLANE_RULES),
    )


def _field_text(message, name):
    value = message[name]
    if name == "settings":
        names = sorted(value, key=lambda each: each.encode("utf-8"))
        return "[%s]" % ",".join('{"name":%s,"value":%s}' % (_text(each), _scalar(value[each])) for each in names)
    if name == "image":
        return _entry_text(value, _IMAGE_RULES)
    if name == "queries":
        return "[%s]" % ",".join(_query_text(query) for query in value)
    if name == "instances":
        return "[%s]" % ",".join(_instance_text(item) for item in value)
    if name == "device" and value is None:
        return "null"
    if isinstance(value, str):
        return _text(value)
    return _whole_text(value)


def _fields_of(message):
    kind_of_message = message["type"]
    if kind_of_message == "hello":
        return _HELLO_WORKER if message.get("role") == "worker" else _HELLO_WHIZBANG
    if kind_of_message == "request":
        return _REQUEST[message["kind"]]
    if kind_of_message == "result":
        return _RESULT[message["kind"]]
    return {"event": _EVENT, "cancel": _CANCEL, "error": _ERROR}[kind_of_message]


def header(message):
    """The canonical writing of a message's header, as it is: nothing sorted, cleaned or
    cut, and nothing checked but that it can be written."""
    fields = _fields_of(message)
    text = "{%s}" % ",".join('"%s":%s' % (name, _field_text(message, name)) for name in fields)
    return text.encode("utf-8")


# --- writing --------------------------------------------------------------------------------


def clean(text, limit):
    """A text cut to `limit` characters that a text of the protocol may hold: white space at
    its ends is dropped, and every other character is a space (ADR 0006, "Faults"; a `?`,
    which that record puts in its place, is no character of a protocol's text)."""
    kept = "".join(c if c in TEXT_CHARACTERS else " " for c in str(text).strip())
    return kept.strip()[:limit]


def _prepared(message):
    """What a writer writes of a message: instances in the fixed order, texts cleaned,
    diagnostics cut."""
    prepared = dict(message)
    kind_of_message = prepared["type"]
    if kind_of_message == "result":
        prepared["diagnostics"] = clean(prepared["diagnostics"], DIAGNOSTICS_CUT)
        if "instances" in prepared:
            prepared["instances"] = sorted(prepared["instances"], key=_order_key)
    elif kind_of_message == "event":
        prepared["text"] = clean(prepared["text"], DIAGNOSTICS_CUT) or "no text was given"
    elif kind_of_message == "error":
        prepared["sentence"] = clean(prepared["sentence"], SENTENCE_LIMIT) or "no reason was given"
    return prepared


def chunks(message):
    """One frame, as the bytes of its length and header and then of each payload, so that
    no copy is made of a payload. Raises `Refused` for a frame a reader would refuse by
    what it holds (what a reader has open is not known here), and `ValueError` for a
    number that cannot be written."""
    prepared = _prepared(message)
    written = header(prepared)
    # A writer of a segment's result does not know its request: it holds the header to
    # the most any request allows.
    bound = RESULT_HEADER_LIMIT if prepared["type"] == "result" else HEADER_LIMIT
    if len(written) > bound:
        raise Refused("header_too_large", "the header is over its bound of %d bytes" % bound)
    # What is written is read back by the same rules a reader holds it to.
    _message(_parse_json(written))
    parts = [len(written).to_bytes(4, "big") + written]
    for payload, size in _payloads(prepared):
        samples = payload.samples
        if samples is None or len(samples) != size:
            raise Refused("wrong_length", "a payload's samples are not the count its entry gives")
        parts.append(size.to_bytes(8, "big"))
        parts.append(samples)
    return parts


def encode(message):
    """One frame, whole."""
    return b"".join(chunks(message))


# --- reading --------------------------------------------------------------------------------


def awaiting(request):
    """What the answer to a request is held to, as a reader that asks keeps it open."""
    entry = {"id": request["id"], "kind": request["kind"], "events": 0, "text": 0}
    if request["kind"] == "segment":
        image = request["image"]
        entry.update(
            width=image.width,
            height=image.height,
            binding=image.binding,
            queries=len(request["queries"]),
            max_instances=request["max_instances"],
        )
    return entry


def _allowed(entry):
    """A result's header bound, from its request."""
    if entry["kind"] != "segment":
        return HEADER_LIMIT
    return HEADER_LIMIT + INSTANCE_ALLOWANCE * entry["queries"] * entry["max_instances"]


class Reader:
    """Reads frames from a binary stream, strictly. `open_requests` maps the id of each
    request the reader has open to what its answer is held to (as `awaiting` gives it, or as
    a vector's `open` states it); an event counts toward its request, and a result or an
    error closes it."""

    def __init__(self, stream, open_requests=None):
        self.stream = stream
        self.open = {} if open_requests is None else open_requests

    def _take(self, count):
        parts = []
        left = count
        while left:
            part = self.stream.read(min(left, 1 << 20))
            if not part:
                break
            parts.append(part)
            left -= len(part)
        return b"".join(parts)

    def read(self):
        try:
            head = self._take(4)
        except OSError:
            raise Refused("truncated", "the input failed before a frame") from None
        if not head:
            return None
        if len(head) < 4:
            raise Refused("truncated", "the input ended inside a header's length")
        size = int.from_bytes(head, "big")
        bound = max([HEADER_LIMIT] + [_allowed(entry) for entry in self.open.values()])
        if size == 0 or size > bound:
            raise Refused("header_too_large", "a header's length is 0 or over its bound of %d bytes" % bound)
        written = self._read_or_truncated(size)
        obj = _parse_json(written)
        if obj.get("type") != "result" and size > HEADER_LIMIT:
            raise Refused("header_too_large", "a header is over its bound of %d bytes" % HEADER_LIMIT)
        message = _message(obj)
        if header(message) != written:
            raise Refused("not_canonical", "the header is not the canonical writing of what it holds", message["id"])
        self._held_to_open(message, size)
        for payload, expected in _payloads(message):
            length = int.from_bytes(self._read_or_truncated(8), "big")
            if length != expected:
                raise Refused("wrong_length", "a payload's length is not the count its entry gives", message["id"])
            payload.samples = self._read_or_truncated(length)
        if message["type"] == "result" and message["kind"] == "segment":
            instances = message["instances"]
            for first, second in zip(instances, instances[1:]):
                if _order_key(first) > _order_key(second):
                    raise Refused("out_of_order", "instances are not in the fixed order", message["id"])
        self._note(message)
        return message

    def _read_or_truncated(self, count):
        try:
            data = self._take(count)
        except OSError:
            raise Refused("truncated", "the input failed inside a frame") from None
        if len(data) < count:
            raise Refused("truncated", "the input ended inside a frame")
        return data

    def _held_to_open(self, message, size):
        """What the reader has open: a result or an event names an open request, a result is
        of its kind and within what it allows, and what is sent beside it is counted."""
        kind_of_message = message["type"]
        if kind_of_message not in ("result", "event"):
            return
        id = message["id"]
        entry = self.open.get(id)
        if entry is None:
            raise Refused("unexpected", "a %s names a request that is not open" % kind_of_message, id)
        if kind_of_message == "event":
            if entry["events"] >= EVENTS_LIMIT:
                raise _over("the events of a request", EVENTS_LIMIT, id)
            if entry["text"] + len(message["text"]) > DIAGNOSTICS_LIMIT:
                raise _over("the text of a request's events and diagnostics", DIAGNOSTICS_LIMIT, id)
            return
        if message["kind"] != entry["kind"]:
            raise Refused("unexpected", "a result is of another kind than its request", id)
        if size > _allowed(entry):
            raise Refused("header_too_large", "a result's header is over what its request allows", id)
        if message["kind"] == "segment":
            counts = {}
            for item in message["instances"]:
                if item["query"] >= entry["queries"]:
                    raise _bad("a query's place", "a place in its request's queries", id)
                counts[item["query"]] = counts.get(item["query"], 0) + 1
                if counts[item["query"]] > entry["max_instances"]:
                    raise _over("a query's instances", entry["max_instances"], id)
            for item in message["instances"]:
                plane = item["plane"]
                if (plane.width, plane.height) != (entry["width"], entry["height"]):
                    raise Refused("wrong_size", "a plane is not the size of its request's image", id)
                if plane.binding != entry["binding"]:
                    raise Refused("wrong_binding", "a plane names another binding than its request's image", id)
        if entry["text"] + len(message["diagnostics"]) > DIAGNOSTICS_LIMIT:
            raise _over("the text of a request's events and diagnostics", DIAGNOSTICS_LIMIT, id)

    def _note(self, message):
        kind_of_message = message["type"]
        id = message.get("id")
        if kind_of_message == "event":
            entry = self.open[id]
            entry["events"] += 1
            entry["text"] += len(message["text"])
        elif kind_of_message in ("result", "error"):
            self.open.pop(id, None)
