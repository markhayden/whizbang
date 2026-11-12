"""Shared pieces of the baseline scripts: the transcript format's constants and schema,
JSON read and written without changing a number's text, hashing, the fixtures of a package,
and the listing-and-digest method of docs/baseline/README.md.

Python 3.9 or later, standard library only.
"""

import hashlib
import json
import os
import sys

sys.dont_write_bytecode = True

TRANSCRIPT_VERSION = 1
TOOLS_VERSION = "2"  # the version of convert.py and audit.py, recorded in what they write

DISPOSITIONS = ("executed", "rejected", "excluded", "final")
TERMINAL_KINDS = ("verdict", "error", "cancelled")
EXCLUSIONS = ("truncated", "refusal", "no_call")
ORIGINS = ("historical", "recaptured")
ROLES = ("user", "assistant", "system")

# What a transcript declares volatile (docs/baseline/transcript.md, "Volatile fields").
VOLATILE = ["/origin", "/meta", "/files", "/turns/*/meta", "/turns/*/calls/*/meta"]

LISTING = "TRANSCRIPTS.sha256"
DIGEST = "TRANSCRIPTS.digest"
FINDINGS = "audit-findings.jsonl"


class Num(str):
    """A number that is not a plain integer, kept as the text it was written with, so that
    reading a file and writing it again never changes a number."""


def _no_constant(name):
    raise ValueError("not JSON: %s" % name)


def loads(text):
    return json.loads(text, parse_float=Num, parse_constant=_no_constant)


def load_json(path):
    with open(path, "rb") as f:
        return loads(f.read().decode("utf-8"))


def load_lines(path):
    """A file of one JSON value per line."""
    with open(path, "rb") as f:
        return [loads(line) for line in f.read().decode("utf-8").split("\n") if line.strip()]


def _write(value, depth, out):
    pad = "  " * (depth + 1)
    if isinstance(value, Num):
        out.append(str(value))
    elif value is None or isinstance(value, (bool, int, str)):
        out.append(json.dumps(value, ensure_ascii=False))
    elif isinstance(value, float):
        raise ValueError("a number lost its text")
    elif isinstance(value, dict):
        if not value:
            out.append("{}")
            return
        out.append("{\n")
        for index, key in enumerate(sorted(value)):
            out.append(pad + json.dumps(key, ensure_ascii=False) + ": ")
            _write(value[key], depth + 1, out)
            out.append(",\n" if index + 1 < len(value) else "\n")
        out.append("  " * depth + "}")
    elif isinstance(value, (list, tuple)):
        if not value:
            out.append("[]")
            return
        out.append("[\n")
        for index, item in enumerate(value):
            out.append(pad)
            _write(item, depth + 1, out)
            out.append(",\n" if index + 1 < len(value) else "\n")
        out.append("  " * depth + "]")
    else:
        raise ValueError("not JSON: %r" % type(value))


def canonical(value):
    """The bytes a transcript is written as: keys sorted, two-space indent, UTF-8, one
    trailing line feed, every number with the text it was read with."""
    out = []
    _write(value, 0, out)
    out.append("\n")
    return "".join(out).encode("utf-8")


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_private(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "wb") as f:
        f.write(data)


def listing(root, leave_out=()):
    """One line per regular file under root: hash, two spaces, relative path, line feed;
    paths sorted by their UTF-8 bytes. A symlink is an error."""
    paths = []
    for folder, dirs, files in os.walk(root):
        for name in dirs + files:
            if os.path.islink(os.path.join(folder, name)):
                raise ValueError("a symlink is not allowed here")
        for name in files:
            rel = os.path.relpath(os.path.join(folder, name), root).replace(os.sep, "/")
            if rel in leave_out:
                continue
            paths.append(rel)
    paths.sort(key=lambda p: p.encode("utf-8"))
    lines = ["%s  %s\n" % (sha256_file(os.path.join(root, p)), p) for p in paths]
    return "".join(lines).encode("utf-8"), len(paths)


def digest_of_listing(listing_bytes):
    return "sha256:" + sha256_bytes(listing_bytes)


def package_digest(package):
    path = os.path.join(package, "PACKAGE.digest")
    if not os.path.isfile(path):
        return None
    with open(path) as f:
        return "sha256:" + f.read().strip()


class FixtureError(Exception):
    pass


def fixtures(package, subdir):
    """The fixtures of a package: for each old result, the one attempt it names. Returns a
    list of (name, decision folder, attempt folder), name being the decision folder's path
    under `subdir`. More than one attempt where one is expected is an error: nothing is
    chosen for the old run."""
    base = os.path.join(package, subdir)
    found = []
    for folder, dirs, files in os.walk(base):
        dirs.sort()
        if "result.json" not in files:
            continue
        result = load_json(os.path.join(folder, "result.json"))
        named = os.path.basename(str(result.get("dir") or "").rstrip("/"))
        attempts = sorted(d for d in dirs if os.path.isfile(os.path.join(folder, d, "decision.json")))
        if attempts != [named]:
            raise FixtureError(
                "a result names one attempt and its folder holds %d (fixture %d)" % (len(attempts), len(found) + 1)
            )
        decision = os.path.join(folder, named)
        runs = sorted(
            d for d in os.listdir(decision) if os.path.isfile(os.path.join(decision, d, "journal.jsonl"))
        )
        if len(runs) != 1:
            raise FixtureError("an attempt holds %d journals, not one (fixture %d)" % (len(runs), len(found) + 1))
        name = os.path.relpath(decision, base).replace(os.sep, "/")
        found.append((name, decision, os.path.join(decision, runs[0])))
        dirs[:] = []
    found.sort()
    return found


def manifest_counts(path):
    """The package digest and the decisions per arm that docs/baseline/manifest.toml names."""
    digest = None
    arms = {}
    arm = None
    with open(path) as f:
        for raw in f:
            line = raw.strip()
            if line == "[[cell]]":
                arm = None
            elif line.startswith("digest = ") and digest is None:
                digest = line.split("=", 1)[1].strip().strip('"')
            elif line.startswith("arm = "):
                arm = line.split("=", 1)[1].strip().strip('"')
            elif line.startswith("decisions = ") and arm is not None:
                arms[arm] = arms.get(arm, 0) + int(line.split("=", 1)[1])
    return digest, arms


# --- the schema of version 1 ---------------------------------------------------------------
#
# docs/baseline/transcript.md describes these fields. schema_errors() is what the audit
# enforces and what the example transcript is tested against.


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def _is_str(v):
    return isinstance(v, str) and not isinstance(v, Num)


class _Check:
    def __init__(self):
        self.errors = []

    def bad(self, path, text):
        self.errors.append("%s: %s" % (path or "/", text))

    def keys(self, value, path, required, optional=()):
        if not isinstance(value, dict):
            self.bad(path, "not an object")
            return False
        for key in required:
            if key not in value:
                self.bad(path, "no %s" % key)
        for key in value:
            if key not in required and key not in optional:
                self.bad(path, "unknown field %s" % key)
        return all(key in value for key in required)

    def text(self, value, path):
        if not _is_str(value):
            self.bad(path, "not text")

    def whole(self, value, path):
        if not _is_int(value):
            self.bad(path, "not a whole number")

    def flag(self, value, path):
        if not isinstance(value, bool):
            self.bad(path, "not true or false")

    def one_of(self, value, path, allowed):
        if not _is_str(value) or value not in allowed:
            self.bad(path, "not one of: %s" % ", ".join(allowed))

    def each(self, value, path, check):
        if not isinstance(value, list):
            self.bad(path, "not a list")
            return
        for index, item in enumerate(value):
            check(item, "%s/%d" % (path, index))

    def texts(self, value, path):
        self.each(value, path, self.text)

    def stored(self, value, path):
        if value is None:
            return
        if self.keys(value, path, ("sha256", "bytes")):
            self.text(value["sha256"], path + "/sha256")
            self.whole(value["bytes"], path + "/bytes")

    def block(self, value, path):
        if not isinstance(value, dict) or not _is_str(value.get("type")):
            self.bad(path, "not a content block")
            return
        kind = value["type"]
        if kind == "text":
            if self.keys(value, path, ("type", "text")):
                self.text(value["text"], path + "/text")
        elif kind == "image":
            if self.keys(value, path, ("type", "image_id")):
                self.text(value["image_id"], path + "/image_id")
        elif kind == "tool_use":
            if self.keys(value, path, ("type", "id", "name", "arguments")):
                self.text(value["id"], path + "/id")
                self.text(value["name"], path + "/name")
        elif kind == "tool_result":
            if self.keys(value, path, ("type", "tool_use_id", "content", "is_error"), ("images",)):
                self.text(value["tool_use_id"], path + "/tool_use_id")
                self.text(value["content"], path + "/content")
                self.flag(value["is_error"], path + "/is_error")
                self.texts(value.get("images", []), path + "/images")
        elif kind == "opaque":
            if self.keys(value, path, ("type", "adapter", "block")):
                self.text(value["adapter"], path + "/adapter")
        else:
            self.bad(path, "unknown block type")

    def message(self, value, path):
        if self.keys(value, path, ("role", "content")):
            self.one_of(value["role"], path + "/role", ROLES)
            self.each(value["content"], path + "/content", self.block)

    def tool(self, value, path):
        if self.keys(value, path, ("name", "description", "input_schema")):
            self.text(value["name"], path + "/name")
            self.text(value["description"], path + "/description")
            if not isinstance(value["input_schema"], dict):
                self.bad(path + "/input_schema", "not an object")

    def tool_call(self, value, path):
        if self.keys(value, path, ("id", "name", "arguments")):
            self.text(value["id"], path + "/id")
            self.text(value["name"], path + "/name")

    def result(self, value, path):
        if value is None:
            return
        if self.keys(value, path, ("content", "is_error", "images")):
            self.text(value["content"], path + "/content")
            self.flag(value["is_error"], path + "/is_error")
            self.texts(value["images"], path + "/images")

    def call(self, value, path):
        if self.keys(value, path, ("id", "name", "arguments", "disposition", "result", "meta")):
            self.text(value["id"], path + "/id")
            self.text(value["name"], path + "/name")
            if value["disposition"] is not None:
                self.one_of(value["disposition"], path + "/disposition", DISPOSITIONS)
            self.result(value["result"], path + "/result")
            if not isinstance(value["meta"], dict):
                self.bad(path + "/meta", "not an object")

    def stop(self, value, path):
        if _is_str(value):
            return
        if isinstance(value, dict) and list(value) == ["other"] and _is_str(value["other"]):
            return
        self.bad(path, "not a stop reason")

    def turn(self, value, path):
        if not self.keys(value, path, ("turn", "request", "response", "excluded", "calls", "meta")):
            return
        self.whole(value["turn"], path + "/turn")
        request = value["request"]
        if request is not None and self.keys(
            request, path + "/request", ("system", "history", "tools", "max_output_tokens")
        ):
            self.text(request["system"], path + "/request/system")
            self.each(request["history"], path + "/request/history", self.message)
            self.each(request["tools"], path + "/request/tools", self.tool)
            self.whole(request["max_output_tokens"], path + "/request/max_output_tokens")
        response = value["response"]
        if response is not None and self.keys(
            response, path + "/response", ("content", "text", "tool_calls", "stop_reason")
        ):
            self.each(response["content"], path + "/response/content", self.block)
            self.texts(response["text"], path + "/response/text")
            self.each(response["tool_calls"], path + "/response/tool_calls", self.tool_call)
            self.stop(response["stop_reason"], path + "/response/stop_reason")
        excluded = value["excluded"]
        if excluded is not None and self.keys(excluded, path + "/excluded", ("reason",)):
            self.one_of(excluded["reason"], path + "/excluded/reason", EXCLUSIONS)
        self.each(value["calls"], path + "/calls", self.call)
        if not isinstance(value["meta"], dict):
            self.bad(path + "/meta", "not an object")

    def image(self, value, path):
        if not isinstance(value, dict):
            self.bad(path, "not an object")
            return
        common_keys = ("id", "frame", "origin", "width", "height", "prepared")
        if value.get("origin") == "job":
            ok = self.keys(value, path, common_keys + ("role", "source"))
            if ok:
                self.text(value["role"], path + "/role")
                if self.keys(value["source"], path + "/source", ("sha256", "pixel_sha256")):
                    self.text(value["source"]["sha256"], path + "/source/sha256")
                    self.text(value["source"]["pixel_sha256"], path + "/source/pixel_sha256")
        else:
            ok = self.keys(value, path, common_keys + ("parent", "root", "mapping"))
            if ok:
                self.text(value["parent"], path + "/parent")
                self.text(value["root"], path + "/root")
        if ok:
            self.text(value["id"], path + "/id")
            self.text(value["frame"], path + "/frame")
            self.text(value["origin"], path + "/origin")
            self.whole(value["width"], path + "/width")
            self.whole(value["height"], path + "/height")
            self.stored(value["prepared"], path + "/prepared")

    def repair(self, value, path):
        if self.keys(value, path, ("after_turn", "form", "reason")):
            self.whole(value["after_turn"], path + "/after_turn")
            self.text(value["form"], path + "/form")
            self.text(value["reason"], path + "/reason")

    def file(self, value, path):
        if self.keys(value, path, ("role", "path", "sha256", "bytes", "truncated")):
            self.text(value["role"], path + "/role")
            self.text(value["path"], path + "/path")
            if value["sha256"] is not None:
                self.text(value["sha256"], path + "/sha256")
            if value["bytes"] is not None:
                self.whole(value["bytes"], path + "/bytes")
            self.flag(value["truncated"], path + "/truncated")

    def terminal(self, value, path):
        if value is None:
            return
        kind = value.get("kind") if isinstance(value, dict) else None
        if kind == "verdict":
            if self.keys(value, path, ("kind", "verdict")):
                self.text(value["verdict"], path + "/verdict")
        elif kind in ("error", "cancelled"):
            if self.keys(value, path, ("kind", "class", "message")):
                self.text(value["class"], path + "/class")
                self.text(value["message"], path + "/message")
        else:
            self.bad(path, "not a terminal outcome")


def schema_errors(t):
    """Every way a value is not a version 1 transcript. An empty list means it is one."""
    c = _Check()
    top = (
        "transcript_version", "origin", "complete", "incomplete", "subject", "preparation",
        "evidence_tools", "submission_tool", "images", "turns", "repairs", "terminal",
        "volatile", "files", "meta",
    )
    if not c.keys(t, "", top):
        return c.errors
    if t["transcript_version"] != TRANSCRIPT_VERSION or not _is_int(t["transcript_version"]):
        c.bad("/transcript_version", "not %d" % TRANSCRIPT_VERSION)
    c.one_of(t["origin"], "/origin", ORIGINS)
    c.flag(t["complete"], "/complete")
    c.texts(t["incomplete"], "/incomplete")
    subject = ("set", "item", "model", "arm", "repetition", "use_case")
    if c.keys(t["subject"], "/subject", subject):
        for key in subject:
            if t["subject"][key] is None:
                continue  # unknown: the transcript is then incomplete
            if key == "repetition":
                c.whole(t["subject"][key], "/subject/repetition")
            else:
                c.text(t["subject"][key], "/subject/" + key)
    if c.keys(t["preparation"], "/preparation", ("version", "settings")):
        if t["preparation"]["version"] is not None:
            c.text(t["preparation"]["version"], "/preparation/version")
        if t["preparation"]["settings"] is not None and not isinstance(t["preparation"]["settings"], dict):
            c.bad("/preparation/settings", "not an object")
    c.texts(t["evidence_tools"], "/evidence_tools")
    if t["submission_tool"] is not None:
        c.text(t["submission_tool"], "/submission_tool")
    c.each(t["images"], "/images", c.image)
    c.each(t["turns"], "/turns", c.turn)
    c.each(t["repairs"], "/repairs", c.repair)
    c.terminal(t["terminal"], "/terminal")
    if t["volatile"] != VOLATILE:
        c.bad("/volatile", "not the format's list")
    c.each(t["files"], "/files", c.file)
    if not isinstance(t["meta"], dict):
        c.bad("/meta", "not an object")
    return c.errors
