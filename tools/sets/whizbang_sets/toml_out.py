"""A small TOML writer for the shapes the builder writes: tables, arrays of tables, inline
tables, texts, whole numbers, numbers with a fraction, true or false, and lists of them.

Ported from squint `scripts/sets/src/squint_sets/toml_out.py`.
"""

from __future__ import annotations

from typing import Any


def _text(v: str) -> str:
    out = []
    for ch in v:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ord(ch) < 0x20 or ord(ch) == 0x7F:
            out.append("\\u%04x" % ord(ch))
        else:
            out.append(ch)
    return '"' + "".join(out) + '"'


def value(v: Any) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return repr(float(v))
    if isinstance(v, str):
        return _text(v)
    if isinstance(v, (list, tuple)):
        return "[" + ", ".join(value(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{k} = {value(x)}" for k, x in v.items() if x is not None) + " }"
    raise TypeError(f"cannot write {type(v).__name__} as TOML")


def table(name: str, fields: dict[str, Any], array: bool = False) -> str:
    lines = [f"[[{name}]]" if array else f"[{name}]"]
    lines += [f"{k} = {value(v)}" for k, v in fields.items() if v is not None]
    return "\n".join(lines) + "\n"


def document(comment: str, tables: list[tuple[str, dict[str, Any], bool]]) -> str:
    head = "".join(f"# {line}\n" for line in comment.splitlines())
    return head + "\n".join(table(n, f, a) for n, f, a in tables)
