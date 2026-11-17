# Setup: installing the Python pack

`whizbang setup` installs the Python pack, the worker whizbang starts for a local model,
and checks that the worker says hello. The decision behind it is ADR 0021.

```
whizbang setup [--extra <name>] [--models-dir <dir>] [--config <file> [--env-file <file>]]
               [--uv <file>] [--python <file>]
```

## What it installs

The pack this build of whizbang carries: the pack's `pyproject.toml`, its lock file
(`uv.lock`), the Python version it pins (`.python-version`, 3.12.15) and its source. With
no extra that is the pack's core, which takes no third-party package. With `--extra torch`
it is also the runtime local models run on: PyTorch, `torchvision` and `transformers`, at
the versions the pack pins, and everything they need, each at the version and hash the lock
file names. That is about 760 MB on a Mac. An extra is never installed unless it is named.

The lock holds the runtime extra for macOS on Apple silicon alone, so it installs there and
nowhere else in this build; for Linux on x86-64 the lock holds the core alone. On any other
platform, Linux among them, `whizbang setup --extra torch` is refused before anything is
downloaded or made, with a sentence saying the extra is locked for macOS on Apple silicon
only; `whizbang setup` with no extra still installs the core, which needs no package. The
runtime on Linux comes later (ADR 0021, "Amended: the Mac alone").

## What it needs

- `uv` at the version `scripts/tool-versions.txt` pins (0.12.23). It is the first `uv` on
  `PATH`, or the one `--uv` names by an absolute path. Another version is refused.
- The Python the pack pins, installed by `uv`. `setup` downloads no Python: where it is
  missing, install it once with
  `uv python install --no-config --no-bin --mirror https://github.com/astral-sh/python-build-standalone/releases/download 3.12.15`,
  which downloads it from GitHub's releases and from nowhere else. `--python` names another
  Python 3.12 by an absolute path.

## Where it installs

In the models directory (see [Models](models.md) for where that is: `--models-dir`, then
the config's `[models] dir`, then the data folder for whizbang on this machine), in a folder
`pack` made for its owner alone:

- `pack/project`: the copy of the pack that was installed;
- `pack/env`: the environment, with its Python at `pack/env/bin/python`.

Nothing is written anywhere else but a temporary folder of `setup`'s own, which is removed
when it ends. To remove the pack, delete the folder `pack`.

## Every place it contacts

| Asked | Contacts |
|---|---|
| `whizbang setup` | Nothing: every step is offline |
| `whizbang setup --extra torch` | `files.pythonhosted.org`, the Python package index's file host, for the wheels the lock file names, and no other host |

`setup` reads no configuration of `uv`'s, uses no cache of `uv`'s (a temporary one in its
own folder), builds no package from source and downloads no Python.

## What it does, in order

1. Checks the version of `uv`.
2. Checks, offline, that the lock file matches the pack. A lock that does not is refused
   and nothing in the models directory is touched.
3. Installs the pack from the lock into `pack/env`, with the extra if one was named.
4. Writes the copy of the pack into `pack/project`, where a file differs.
5. Starts the worker from the environment, with a lifetime of 120 seconds, hears its
   hello, and stops it.

Running it again with the same extra changes nothing. Running it with no extra after an
install with one removes that extra's packages.

Each step has a limit: 60 seconds, or 30 minutes for the install of an extra. `uv` is
stopped past it and `setup` says so.

## What it prints

On success, the folder it installed in, the extra, and what the worker said of itself:

```
the pack is installed in <models dir>/pack
extra: torch
the worker it made said hello (lifetime 120 s): device none, 3 threads
```

The device is `none` until the worker has a model loaded.

Exit codes: 0 done; 1 not done, and the complaint says why (with the last lines `uv` wrote,
where it wrote some); 2 the command line was not understood; 3 what was to be printed could
not be written.
