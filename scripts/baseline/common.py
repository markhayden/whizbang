"""Shared pieces of the baseline scripts: the transcript format's constants, canonical JSON,
hashing, and the listing-and-digest method of docs/baseline/README.md.

Python 3.9 or later, standard library only.
"""

import hashlib
import json
import os

TRANSCRIPT_VERSION = 1
TOOLS_VERSION = "1"  # the version of convert.py and audit.py, recorded in what they write

DISPOSITIONS = ("executed", "rejected", "excluded", "final")
TERMINAL_KINDS = ("verdict", "error", "cancelled")

# What a transcript declares volatile (docs/baseline/transcript.md, "Volatile fields").
VOLATILE = ["/origin", "/meta", "/files", "/turns/*/meta", "/turns/*/calls/*/meta"]


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def canonical(value):
    """The bytes a transcript is written as: sorted keys, two-space indent, UTF-8, one
    trailing line feed. The same value always gives the same bytes."""
    text = json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False)
    return (text + "\n").encode("utf-8")


def load_json(path):
    with open(path, "rb") as f:
        return json.loads(f.read().decode("utf-8"))


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
