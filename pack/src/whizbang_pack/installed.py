"""What the worker reads of an installation before a `load` (plan 1D, task U1): its record,
which names the adapter it was installed with; and, for an adapter a user added, the copy
of it in the models directory, by its hash.

    <models>/models/<model>/installations/<64 hex digits>/RECORD    installation/1
    <models>/adapters/<64 hex digits>.py                             a user's adapter, by its hash

**The record is bound to the identity a `load` names.** The identity of an installation is
the SHA-256 of one encoding of its record (ADR 0017, "The identity of an installation");
the record is read, and its identity worked out again here, and a record whose identity is
not the one the `load` names is refused. So the adapter a load runs is the one the
admitted installation was installed with, and no other.

**A user's adapter is code the user chose, and is run only by its hash.** The record names
it as `user-<name>`, and its revision is the SHA-256 of the copy `whizbang models adapter
add` made, as 64 hex digits. The copy is read once, as bytes, through a name that is no
link, from the models directory and from nowhere else; its hash is worked out from those
bytes and compared with the record's before anything of it runs; and the bytes that were
hashed are the bytes that are compiled. A copy whose hash is not the record's is refused,
and nothing of it is imported. Nothing is fetched: a copy that is not there is refused.
A model of the shipped catalogue never runs a user's adapter.
"""

import hashlib
import json
import os
import re
import stat
import types

from whizbang_pack.adapter import Fault

INSTALLATION_FORMAT = "installation/1"
# The most bytes of a record, as the binary bounds it (`INSTALLATION_MAX_BYTES`).
RECORD_MAX_BYTES = 32 << 10
# The most bytes of a user's adapter (`ADAPTER_MAX_BYTES` of the binary's registry).
ADAPTER_MAX_BYTES = 1 << 20
# The prefix of every adapter a user added: no adapter the pack ships has it.
USER = "user-"
# What a user's adapter module names its adapter's class by.
ENTRY = "ADAPTER"

_HEX = re.compile(r"[0-9a-f]{64}\Z")


def _read(path, most):
    """The bytes of a regular file under its own name, at most `most` and one more; `None`
    when nothing has the name. A link at the name is not followed: it is refused."""
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0))
    except FileNotFoundError:
        return None
    except OSError:
        raise Fault("rejected", "a file of the installation cannot be opened")
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise Fault("rejected", "a file of the installation is not a regular file")
        chunks, left = [], most + 1
        while left > 0:
            chunk = os.read(descriptor, min(left, 1 << 16))
            if not chunk:
                break
            chunks.append(chunk)
            left -= len(chunk)
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def identity_of(record):
    """The identity of the installation this record describes, as the binary works it out
    (`InstallationRecord::identity`): `sha256:` and the hex of the SHA-256 of the bytes
    `whizbang:installation/1` and a zero byte, then the model, the catalogue's revision,
    the adapter and the adapter's revision, each as bytes (`b`, a length of 8 bytes and the
    bytes); the count of files (`u`, 8 bytes); and for each file in the order of its name,
    its name (`b`), its size (`u`) and its hash (`d` and 32 bytes)."""
    encoding = bytearray(b"whizbang:" + INSTALLATION_FORMAT.encode("ascii") + b"\0")

    def text(value):
        data = value.encode("ascii")
        encoding.extend(b"b" + len(data).to_bytes(8, "big") + data)

    def whole(value):
        encoding.extend(b"u" + value.to_bytes(8, "big"))

    for key in ("model", "catalogue_revision", "adapter", "adapter_revision"):
        text(record[key])
    files = sorted(record["files"], key=lambda file: file["name"].encode("ascii"))
    whole(len(files))
    for file in files:
        text(file["name"])
        whole(file["size"])
        encoding.extend(b"d" + bytes.fromhex(file["sha256"][len("sha256:") :]))
    return "sha256:" + hashlib.sha256(bytes(encoding)).hexdigest()


def record_of(installation):
    """The record of this installation, held to the identity its `load` names; `None` when
    the installation's folder holds no record (the pack's own tests, which install
    nothing). A record that is there and cannot be read, or whose identity is another, is
    refused."""
    data = _read(os.path.join(installation.folder, "RECORD"), RECORD_MAX_BYTES)
    if data is None:
        return None
    unread = Fault("rejected", "the installation's record cannot be read")
    if len(data) > RECORD_MAX_BYTES:
        raise unread
    try:
        record = json.loads(data.decode("ascii"))
        if record.get("format") != INSTALLATION_FORMAT or record.get("model") != installation.model:
            raise unread
        for key in ("catalogue_revision", "adapter", "adapter_revision"):
            if not isinstance(record.get(key), str):
                raise unread
        files = record.get("files")
        if not isinstance(files, list):
            raise unread
        for file in files:
            if not (isinstance(file.get("name"), str) and type(file.get("size")) is int and file["size"] >= 0):
                raise unread
            if not (isinstance(file.get("sha256"), str) and _HEX.match(file["sha256"][len("sha256:") :])):
                raise unread
        identity = identity_of(record)
    except (ValueError, UnicodeError, AttributeError, TypeError, OverflowError):
        raise unread
    if identity != installation.identity:
        raise Fault("rejected", "the installation's record is not the one its identity names")
    return record


def user_adapter(models_dir, revision):
    """The class of a user's adapter, from its copy in the models directory, by its hash:
    the copy's bytes are read once, their SHA-256 is compared with `revision`, and only
    those bytes, matched, are compiled and run, as a module of their own that names its
    adapter's class `ADAPTER`. Refused before anything of it runs: a revision that is not
    64 hex digits, a folder `adapters` that is a link, a copy that is not there, is a link,
    or is too long, and a copy whose hash is not the revision."""
    if not isinstance(revision, str) or not _HEX.match(revision):
        raise Fault("rejected", "the installation names a user's adapter by no hash")
    folder = os.path.join(models_dir, "adapters")
    try:
        if stat.S_ISLNK(os.lstat(folder).st_mode):
            raise Fault("rejected", "the folder of users' adapters is a link, and nothing is run through it")
    except FileNotFoundError:
        raise Fault("rejected", "the user's adapter the installation was installed with is not in the models directory")
    data = _read(os.path.join(folder, revision + ".py"), ADAPTER_MAX_BYTES)
    if data is None:
        raise Fault("rejected", "the user's adapter the installation was installed with is not in the models directory")
    if len(data) > ADAPTER_MAX_BYTES or hashlib.sha256(data).hexdigest() != revision:
        raise Fault("rejected", "the copy of the user's adapter is not the one the installation was installed with")
    return compiled(data, revision, os.path.join(folder, revision + ".py"))


def compiled(data, revision, path):
    """The class `ADAPTER` of a user's adapter, from these bytes, which the caller hashed:
    they are compiled and run as a module of their own, which is put nowhere the import
    system looks. What runs here is the user's code."""
    module = types.ModuleType("whizbang_user_adapter_" + revision)
    module.__file__ = path
    exec(compile(data, path, "exec"), module.__dict__)
    made = getattr(module, ENTRY, None)
    if not callable(made):
        raise Fault("rejected", "the user's adapter names no class ADAPTER")
    return made


def adapter_of(models_dir, installation, by_model, by_adapter, shipped):
    """What makes the adapter of this installation: the one its record names, from
    `by_adapter` or, for `user-<name>`, the copy of that user's adapter by its hash; and,
    for an installation with no record, the one `by_model` has for its model's name. A
    model of the shipped catalogue (`shipped`) is never run by a user's adapter. `None`
    when nothing answers."""
    record = record_of(installation)
    if record is None:
        return by_model.get(installation.model)
    name = record["adapter"]
    if name.startswith(USER):
        if installation.model in shipped:
            raise Fault("rejected", "a model of the shipped catalogue is never run by a user's adapter")
        return user_adapter(models_dir, record["adapter_revision"])
    return by_adapter.get(name)
