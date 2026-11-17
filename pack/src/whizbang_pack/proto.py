"""The frames of `pack/1`, written and read strictly (docs/adr/0020-the-pack-protocol.md).

Written from that record and the vectors under `pack/conformance/vectors/`, as a second
codec beside whizbang's own; the vectors hold the two to one another. Where one frame
breaks rules of two steps, it names the reason whizbang's reader names: the steps and their
order are the record's ("What a refusal says"). The standard library only (D10 of
docs/plans/m1c-local-pack.md).

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
# A whole number a header's count or size field holds: 32 bits. Past it, the value is of
# the wrong type (`bad_value`); within it and past its bound, it is `over_bound`.
WHOLE_MOST = (1 << 32) - 1

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
_PLAIN_VERSION = re.compile(r"[a-z0-9][a-z0-9./_-]{0,31}\Z")
_SURROGATE = re.compile("[\ud800-\udfff]")
# The characters that say where a JSON text starts and ends, and where nesting does.
_SPECIAL = re.compile(r'[\[\]{}"\\]')

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
    """How deep objects and lists nest in a JSON text, outside its texts: one pass over
    the characters that matter (brackets, quotes and backslashes), so linear in the
    header's length whatever it holds."""
    depth = deepest = 0
    inside = False
    escaped = -1  # the place of the character a backslash escapes
    for found in _SPECIAL.finditer(text):
        at = found.start()
        character = found.group()
        if inside:
            if at == escaped:
                continue
            if character == "\\":
                escaped = at + 1
            elif character == '"':
                inside = False
        elif character == '"':
            inside = True
        elif character in "[{":
            depth += 1
            if depth > deepest:
                deepest = depth
        elif character in "]}":
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
    if text.startswith("\N{ZERO WIDTH NO-BREAK SPACE}"):
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


# --- taking a header apart ------------------------------------------------------------------
#
# Two passes, as ADR 0020 orders them. The first takes each object apart: a field that is
# not its own, then one that is missing, then each field's type, field by field in the order
# below (for a result, its instances before its other fields), with the rules a setting, a
# query and an entry's words keep where they are read. The second holds the message to the
# rules of its values and counts. A message that passes both is then held to the fixed
# order of its instances.


def _bad(what, rule, id=None):
    return Refused("bad_value", "%s must be %s" % (what, rule), id)


def _over(what, most, id=None):
    return Refused("over_bound", "%s is over its bound of %d" % (what, most), id)


def _is_whole(value):
    # `bool` is an `int` to Python, and `-0` was read as no whole number.
    return type(value) is int


class _Object:
    """An object of a header being taken apart: its fields are exactly its own before any
    is taken. A field that is not its own is refused first, then one that is missing."""

    def __init__(self, fields, names, id):
        for name in fields:
            if name not in names:
                raise Refused("unknown_field", "an object holds a field that is not its own", id)
        for name in names:
            if name not in fields:
                raise Refused("missing_field", "an object lacks the field " + name, id)
        self.fields = fields
        self.id = id

    def take(self, name):
        return self.fields[name]

    def text(self, name):
        value = self.fields[name]
        if not isinstance(value, str):
            raise _bad(name, "a text", self.id)
        return value

    def whole(self, name):
        """A whole number of 32 bits at most."""
        value = self.fields[name]
        if not _is_whole(value) or not 0 <= value <= WHOLE_MOST:
            raise _bad(name, "a whole number in its range", self.id)
        return value

    def list(self, name):
        value = self.fields[name]
        if not isinstance(value, list):
            raise _bad(name, "a list", self.id)
        return value

    def entry(self, name, rules):
        value = self.fields[name]
        if not isinstance(value, dict):
            raise _bad(name, "an object", self.id)
        entry = _Object(value, _ENTRY, self.id)
        width = entry.whole("width")
        height = entry.whole("height")
        for field, word in zip(_ENTRY[2:6], rules):
            if entry.text(field) != word:
                raise _bad(field, "the one rule its entry has", self.id)
        binding = entry.text("binding")
        if not _DIGEST.match(binding):
            raise _bad("binding", "sha256: and 64 lower-case hex digits", self.id)
        return Payload(width, height, binding)


def _peek(obj, name, id):
    """A text field of an object whose other fields are not known yet (`type`, `version`,
    `role`, `kind`): `missing_field` when absent and `bad_value` when it is no text."""
    if name not in obj:
        raise Refused("missing_field", "an object lacks the field " + name, id)
    value = obj[name]
    if not isinstance(value, str):
        raise _bad(name, "a text", id)
    return value


def _number(value, what, id):
    """A number with a fraction, from its 16 lower-case hex digits; any double. That it is
    finite is its field's rule, held in the second pass."""
    if not isinstance(value, str) or not _FRACTION.match(value):
        raise _bad(what, "a number, as 16 lower-case hex digits", id)
    return struct.unpack(">d", bytes.fromhex(value))[0]


def _numbers(value, count, what, id):
    if not isinstance(value, list):
        raise _bad(what, "a list", id)
    if len(value) != count:
        raise _bad(what, "a list of its own length", id)
    return [_number(item, what, id) for item in value]


def _named(value, pattern, limit):
    return len(value) <= limit and pattern.match(value) is not None


def _check_setting(name, value, id):
    if not _named(name, _SETTING_NAME, SETTING_NAME_LIMIT):
        raise _bad("a setting's name", "a name of lower-case letters, digits and _", id)
    if isinstance(value, str) and not (
        1 <= len(value) <= SETTING_TEXT_LIMIT
        and all(" " <= c <= "~" and c not in "/\\" for c in value)
        and not value.startswith(" ")
        and not value.endswith(" ")
    ):
        raise _bad("a setting's text", "1 to 1024 printable ASCII characters, no space at an end, no separator", id)


def _read_settings(items, id):
    settings = {}
    last = None
    for item in items:
        if not isinstance(item, dict):
            raise _bad("a setting", "an object", id)
        setting = _Object(item, _SETTING, id)
        name = setting.text("name")
        value = setting.take("value")
        if isinstance(value, (bool, str)):
            pass
        elif _is_whole(value) and _I64_LEAST <= value <= _I64_MOST:
            pass
        else:
            raise _bad("a setting's value", "a scalar", id)
        _check_setting(name, value, id)
        if last is not None and last >= name.encode("utf-8"):
            raise Refused("out_of_order", "settings are not in the order of their names, each once", id)
        last = name.encode("utf-8")
        settings[name] = value
    return settings


def _read_queries(items, id):
    queries = []
    for item in items:
        if not isinstance(item, dict):
            raise _bad("a query", "an object", id)
        if any(name not in _QUERY_KINDS for name in item):
            raise Refused("unknown_field", "a query holds a field that is not its own", id)
        if len(item) > 1:
            raise _bad("a query", "one of a phrase, a point and a box", id)
        if not item:
            raise Refused("missing_field", "a query lacks a phrase, a point or a box", id)
        ((kind, given),) = item.items()
        if kind == "phrase":
            if not isinstance(given, str):
                raise _bad("phrase", "a text", id)
            if not given:
                raise _bad("a phrase", "1 byte at least", id)
            if len(given.encode("utf-8")) > PHRASE_LIMIT:
                raise _over("a phrase's bytes", PHRASE_LIMIT, id)
            queries.append({"phrase": given})
        elif kind == "point":
            x, y = _numbers(given, 2, "point", id)
            if not (math.isfinite(x) and math.isfinite(y) and x >= 0 and y >= 0):
                raise _bad("a point", "two finite numbers, neither under zero", id)
            queries.append({"point": [x, y]})
        else:
            edges = _numbers(given, 4, "box", id)
            left, top, right, bottom = edges
            if not (all(math.isfinite(edge) and edge >= 0 for edge in edges) and left < right and top < bottom):
                raise _bad("a box", "four finite numbers, none under zero, with area", id)
            queries.append({"box": edges})
    return queries


def _read_instances(items, id):
    instances = []
    for item in items:
        if not isinstance(item, dict):
            raise _bad("an instance", "an object", id)
        instance = _Object(item, _INSTANCE, id)
        query = instance.whole("query")
        score = _number(instance.take("score"), "score", id)
        iou = instance.take("iou")
        iou = None if iou is None else _number(iou, "iou", id)
        box = _numbers(instance.take("box"), 4, "box", id)
        plane = instance.entry("plane", _PLANE_RULES)
        instances.append({"query": query, "score": score, "iou": iou, "box": box, "plane": plane})
    return instances


def _read_model(fields, message):
    model = fields.text("model")
    if not _named(model, _MODEL, MODEL_LIMIT):
        raise _bad("model", "a model's name", fields.id)
    installation = fields.text("installation")
    if not _DIGEST.match(installation):
        raise _bad("installation", "an identity", fields.id)
    message["model"] = model
    message["installation"] = installation


def _take_apart(obj, id):
    """The first pass (whizbang's reader's `parse`)."""
    kind_of_message = _peek(obj, "type", id)
    if kind_of_message == "hello":
        version = _peek(obj, "version", id)
        if version != VERSION:
            shown = version if _named(version, _PLAIN_VERSION, 32) else "a version that is not a name"
            raise Refused("version", "this side speaks %s and the other said %s" % (VERSION, shown), id)
        role = _peek(obj, "role", id)
        if role not in ("whizbang", "worker"):
            raise _bad("role", "whizbang or worker", id)
        fields = _Object(obj, _HELLO_WORKER if role == "worker" else _HELLO_WHIZBANG, id)
        message = {"type": "hello", "id": fields.text("id"), "version": version, "role": role}
        if role == "worker":
            device = fields.take("device")
            if device is not None and not isinstance(device, str):
                raise _bad("device", "a text or null", id)
            message["device"] = device
            message["threads"] = fields.whole("threads")
        return message

    if kind_of_message == "request":
        kind = _peek(obj, "kind", id)
        if kind not in _REQUEST:
            raise Refused("unknown_kind", "a request or a result is of a kind this version has not", id)
        fields = _Object(obj, _REQUEST[kind], id)
        message = {"type": "request", "id": fields.text("id"), "kind": kind}
        _read_model(fields, message)
        if kind == "load":
            message["settings"] = _read_settings(fields.list("settings"), id)
        elif kind == "segment":
            message["image"] = fields.entry("image", _IMAGE_RULES)
            message["queries"] = _read_queries(fields.list("queries"), id)
            message["max_instances"] = fields.whole("max_instances")
        return message

    if kind_of_message == "result":
        kind = _peek(obj, "kind", id)
        if kind not in _RESULT:
            raise Refused("unknown_kind", "a request or a result is of a kind this version has not", id)
        fields = _Object(obj, _RESULT[kind], id)
        # A result's instances are taken before its other fields.
        instances = _read_instances(fields.list("instances"), id) if kind == "segment" else None
        message = {
            "type": "result",
            "id": fields.text("id"),
            "kind": kind,
            "device": fields.text("device"),
            "threads": fields.whole("threads"),
            "diagnostics": fields.text("diagnostics"),
        }
        if instances is not None:
            message["instances"] = instances
        return message

    if kind_of_message == "event":
        fields = _Object(obj, _EVENT, id)
        return {"type": "event", "id": fields.text("id"), "text": fields.text("text")}

    if kind_of_message == "cancel":
        fields = _Object(obj, _CANCEL, id)
        return {"type": "cancel", "id": fields.text("id")}

    if kind_of_message == "error":
        fields = _Object(obj, _ERROR, id)
        message = {"type": "error", "id": fields.text("id")}
        fault = fields.text("fault")
        if fault not in FAULTS:
            raise _bad("fault", "one of the eight faults", id)
        message["fault"] = fault
        message["sentence"] = fields.text("sentence")
        return message

    raise Refused("unknown_message", "a message is of none of the six types", id)


def _check_id(value, id):
    if not _named(value, _ID, ID_LIMIT):
        raise _bad("an id", "1 to 64 of lower-case letters, digits and . _ -", id)


def _check_device(value, id):
    if not _named(value, _DEVICE, DEVICE_LIMIT):
        raise _bad("a device", "a name of lower-case letters, digits and . _ - :", id)


def _check_threads(value, id):
    if not 1 <= value <= THREADS_LIMIT:
        raise _bad("a count of threads", "from 1 to 65535", id)


def _check_text(value, what, least, most, id):
    if len(value) < least or not all(c in TEXT_CHARACTERS for c in value):
        raise _bad(what, "a text of ASCII letters, digits, spaces and . , ; : ' ( ) % + - _", id)
    if len(value) > most:
        if most == DIAGNOSTICS_LIMIT:
            raise _over(what, most, id)
        raise _bad(what, "within its length", id)


def _check_entry(payload, id):
    pixels = payload.width * payload.height
    if pixels == 0:
        raise _bad("a payload's width or height", "at least 1", id)
    if pixels > PIXEL_LIMIT:
        raise _over("a payload's count of pixels", PIXEL_LIMIT, id)


def _check_unit(value, what, id):
    if not 0 <= value <= 1:
        raise _bad(what, "a number from 0 to 1", id)


def _check(message, id):
    """The second pass (whizbang's reader's `check`): every rule a message keeps that its
    header alone shows."""
    kind_of_message = message["type"]
    _check_id(message["id"], id)
    if kind_of_message == "hello":
        if message["role"] == "worker":
            if message["device"] is not None:
                _check_device(message["device"], id)
            _check_threads(message["threads"], id)
    elif kind_of_message == "request":
        kind = message["kind"]
        if kind == "load":
            if len(message["settings"]) > SETTINGS_LIMIT:
                raise _over("the count of settings", SETTINGS_LIMIT, id)
        elif kind == "segment":
            _check_entry(message["image"], id)
            queries = message["queries"]
            if not queries:
                raise _bad("the list of queries", "of one query at least", id)
            if len(queries) > QUERIES_LIMIT:
                raise _over("the count of queries", QUERIES_LIMIT, id)
            if message["max_instances"] == 0:
                raise _bad("the most instances", "at least 1", id)
            if message["max_instances"] > INSTANCES_LIMIT:
                raise _over("the most instances", INSTANCES_LIMIT, id)
    elif kind_of_message == "result":
        _check_device(message["device"], id)
        _check_threads(message["threads"], id)
        _check_text(message["diagnostics"], "a result's diagnostics", 0, DIAGNOSTICS_LIMIT, id)
        instances = message.get("instances")
        if instances is not None:
            if len(instances) > MOST_INSTANCES:
                raise _over("the count of instances", MOST_INSTANCES, id)
            for item in instances:
                _check_unit(item["score"], "a score", id)
                if item["iou"] is not None:
                    _check_unit(item["iou"], "a predicted IoU", id)
                if not all(math.isfinite(edge) for edge in item["box"]):
                    raise _bad("a box", "four finite numbers", id)
                _check_entry(item["plane"], id)
    elif kind_of_message == "event":
        _check_text(message["text"], "an event's text", 1, DIAGNOSTICS_LIMIT, id)
    elif kind_of_message == "error":
        _check_text(message["sentence"], "an error's sentence", 1, SENTENCE_LIMIT, id)


def _message(obj):
    """The message a parsed header holds, held to every rule its header alone shows."""
    raw_id = obj.get("id")
    id = raw_id if isinstance(raw_id, str) and _named(raw_id, _ID, ID_LIMIT) else None
    message = _take_apart(obj, id)
    _check(message, id)
    instances = message.get("instances") if message["type"] == "result" else None
    if instances:
        for first, second in zip(instances, instances[1:]):
            if _header_key(first) > _header_key(second):
                raise Refused("out_of_order", "the instances of a result are not in the fixed order", id)
    return message


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
    its ends is dropped, and every other character is a space (ADR 0006, "Faults", puts a
    `?` in its place, which is no character of a protocol's text: ADR 0020, as amended)."""
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
    error closes it.

    The steps, in ADR 0020's order: the header's length against the largest bound among
    the requests open; that it is JSON; the two passes over its fields and values, then the
    fixed order of its instances; its canonical writing; what is open; each payload's
    length; and last, the order of planes the header showed tied."""

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
        message = _message(_parse_json(written))
        id = message["id"]
        if header(message) != written:
            raise Refused("not_canonical", "a header is not the canonical writing of what it holds", id)
        self._held_to_open(message, size)
        for payload, expected in _payloads(message):
            length = int.from_bytes(self._read_or_truncated(8), "big")
            if length != expected:
                raise Refused("wrong_length", "a payload's length is not the count its entry gives", id)
            payload.samples = self._read_or_truncated(length)
        if message["type"] == "result" and message["kind"] == "segment":
            instances = message["instances"]
            for first, second in zip(instances, instances[1:]):
                if _order_key(first) > _order_key(second):
                    raise Refused("out_of_order", "the instances of a result are not in the fixed order", id)
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
        """What the reader has open. A result: that its request is open and of its kind,
        its header within what the request allows, its diagnostics within what is left of
        the text; then instance by instance its query's place, the count for that query,
        its plane's size and binding; and last, its planes together within 1 GiB. An event:
        that its request is open, its header within 64 KiB, the count of events and the
        text. Any other message: its header within 64 KiB."""
        kind_of_message = message["type"]
        id = message["id"]
        if kind_of_message == "result":
            entry = self.open.get(id)
            if entry is None:
                raise Refused("unexpected", "the request a result names is not open", id)
            if message["kind"] != entry["kind"]:
                raise Refused("unexpected", "a result is of another kind than its request", id)
            if size > _allowed(entry):
                raise Refused("header_too_large", "a result's header is over what its request allows", id)
            if entry["text"] + len(message["diagnostics"]) > DIAGNOSTICS_LIMIT:
                raise _over("the text sent for one request", DIAGNOSTICS_LIMIT, id)
            if message["kind"] != "segment":
                return
            counts = {}
            together = 0
            for item in message["instances"]:
                query = item["query"]
                if query >= entry["queries"]:
                    raise _bad("an instance's query", "one its request holds", id)
                counts[query] = counts.get(query, 0) + 1
                if counts[query] > entry["max_instances"]:
                    raise _over("the count of instances for a query", entry["max_instances"], id)
                plane = item["plane"]
                if (plane.width, plane.height) != (entry["width"], entry["height"]):
                    raise Refused("wrong_size", "a plane is not the size of its request's image", id)
                if plane.binding != entry["binding"]:
                    raise Refused("wrong_binding", "a plane names another binding than its request's image", id)
                together += plane.width * plane.height
            if together > PAYLOAD_CEILING:
                raise Refused("too_large", "a result's payloads together are over %d bytes" % PAYLOAD_CEILING, id)
            return
        if kind_of_message == "event":
            entry = self.open.get(id)
            if entry is None:
                raise Refused("unexpected", "the request an event names is not open", id)
            if size > HEADER_LIMIT:
                raise Refused("header_too_large", "a header is over its bound of %d bytes" % HEADER_LIMIT, id)
            if entry["events"] >= EVENTS_LIMIT:
                raise _over("the count of events for one request", EVENTS_LIMIT, id)
            if entry["text"] + len(message["text"]) > DIAGNOSTICS_LIMIT:
                raise _over("the text sent for one request", DIAGNOSTICS_LIMIT, id)
            return
        if size > HEADER_LIMIT:
            raise Refused("header_too_large", "a header is over its bound of %d bytes" % HEADER_LIMIT, id)

    def _note(self, message):
        kind_of_message = message["type"]
        id = message.get("id")
        if kind_of_message == "event":
            entry = self.open[id]
            entry["events"] += 1
            entry["text"] += len(message["text"])
        elif kind_of_message in ("result", "error"):
            self.open.pop(id, None)
