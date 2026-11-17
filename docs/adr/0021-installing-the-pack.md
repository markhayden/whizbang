# 0021. Installing the pack: a locked `uv` install into the models directory

Status: accepted.
Date: 2026-11-17
Amended: 2026-11-17, by the owner's decision of that day, in their words "drop it": Linux
on x86-64 is dropped from the runtime in the pack's lock for checkpoint 1C, so the lock holds
the extra's packages for macOS on Apple silicon alone. On Linux the index's `torch` is the
CUDA build and pulls in NVIDIA's packages: three under NVIDIA's proprietary licence and
thirteen whose metadata states none. 1C installs and runs the runtime only on the owner's
Mac, and the public lock names no proprietary package nobody has tested. `whizbang setup
--extra` is refused on any other platform before anything is downloaded; the core still
installs on Linux. Linux comes back when a Linux story needs it (`docs/carry-over.md`,
"Linux runtime extra"). See "Amended: the Mac alone", below.
Spec: section 4, "The Python pack"; section 17, "Offline bundles for installing the pack"

## Context

Whizbang runs a local model in one Python worker (ADR 0020). The worker's core takes no
third-party package, so the fast tier and CI run it with a pinned Python and nothing else
(plan 1C, D10). A real model needs a runtime, PyTorch with the `transformers` library for
candidate A of the plan's D8, and that runtime is gigabytes on some platforms. Both old
repos installed their pack from an offline bundle: rascal built one for each platform with
`scripts/build-bundle.sh` (the pack as a wheel, a wheelhouse of every locked dependency, a
manifest of digests) and installed it with `crates/rascal-mcp/src/provision.rs`, offline,
against that wheelhouse alone (rascal at the commit `docs/sources.toml` pins; read as
reference for what is dropped, nothing ported).

Task Y1 of checkpoint 1C builds `whizbang setup` and makes the lock file.

## Decision

### A plain install from a committed lock (D9)

`whizbang setup [--extra <name>]` installs the pack with `uv` at its pinned version
(`scripts/tool-versions.txt`) from `pack/uv.lock`, which is committed. The lock pins every
package of every extra by version and SHA-256, for two platforms: macOS on Apple silicon
and Linux on x86-64 (`tool.uv.environments` in `pack/pyproject.toml`). Linux on ARM is
dropped (`SPEC.md` section 17), and no other platform is locked.

No offline bundle is built or hosted. A bundle of a GPU runtime is gigabytes for each
platform and one more thing to build, sign and host, and nothing in the spec's stories
installs the pack without a network: the hosted engine of story 1 runs no Python.

### The core takes no package; the runtime is an extra (D10)

`dependencies` is empty and `tool.uv.package = false`: the project is neither built nor
installed, so no build backend is fetched. The runtime is one extra, `torch`, which pins
`torch==2.15.0`, `torchvision==0.29.2` and `transformers==5.23.0`. `torchvision` is in it
because the library's image processor for SAM 3 imports it (`image_processing_sam3.py`
imports `torchvision.ops`). With no extra, `setup` installs nothing from a package index.

The pinned Python, 3.12.15, runs the locked versions: every package of the lock has a
wheel for CPython 3.12, or one for any Python, on both platforms, and on the Mac the
installed environment imports `torch`, `transformers` and its classes `Sam3Model`,
`Sam3Processor` and `Sam3TrackerModel` (open question 4 of the plan).

On Linux, the lock needs no second index (open question 5): the `torch` wheel for Linux
on x86-64 on the Python package index is a CUDA build, and the CUDA libraries it depends on
(fifteen `nvidia-*` packages, `cuda-bindings`, `cuda-pathfinder`, `cuda-toolkit` and
`triton`) are on the same index. 1C
installs on the Mac only.

### What the binary carries, and where it installs

The binary carries the pack: its `pyproject.toml`, its `uv.lock`, its `.python-version`
and every module of `pack/src/whizbang_pack/`, compiled in from the repository
(`whizbang::pack_setup::Pack::carried`; the test
`setup::the_pack_this_build_carries_is_the_repositorys` holds the list to the folder).
Nothing of the pack is read from a path at run time.

The environment is in the models directory (ADR 0017), beside the models' own folder, in a
folder `pack` that `setup` makes for its owner alone:

| Path | What |
|---|---|
| `pack/project/` | The copy of the pack that was installed: the three files and `src/` |
| `pack/env/` | The environment `uv` made (`UV_PROJECT_ENVIRONMENT`), with its Python, `bin/python` |

The kind `pack` (task P6a) starts its worker as `pack/env/bin/python` with `pack/project/src`
on its path (`whizbang::pack_setup::at`). One models directory has one pack. A flag
(`--models-dir`), the config's `[models] dir` and the platform's data folder say where the
models directory is, as for `whizbang models`.

### What `setup` does

In this order, each step past the first only when the one before ended well:

1. The pack is written into a folder of `setup`'s own, in the system's temporary folder,
   for its owner alone, and removed on every path out.
2. `uv --version` must name the pinned version.
3. `uv`'s `lock --check`, offline: the lock must match the pack's own file. A lock that
   does not is refused here, before anything in the models directory is made or changed
   (`setup::setup_refuses_a_lock_file_that_does_not_match_the_pack_and_changes_nothing`).
   This step is there because `sync --locked` makes the environment's folder before it
   finds that the lock does not match.
4. `uv`'s `sync --locked --no-build`, with `--extra=<name>` when one was named, into
   `pack/env`. With no extra it is offline. With one it downloads that extra's wheels from
   the addresses the lock names; it builds nothing.
5. `pack/project` is brought to the pack that was installed: a file is written only where
   it differs, as a new file renamed over the old, and a module the pack no longer has is
   removed. A second `setup` of the same pack and extra changes nothing
   (`setup::a_second_setup_changes_nothing`).
6. The worker is started from the environment with `--max-lifetime-s 120`, a value of
   `setup`'s own, since no config is read for it; it says hello, and is stopped and waited
   for (`setup::the_worker_setup_starts_for_its_hello_is_given_a_lifetime_of_120_seconds_and_is_gone_when_setup_ends`).

### How each `uv` is started

Under the row of the plan's "Rules" on `uv`:

- A cleared environment: the locale of the worker's allow-list (`LANG`, `LC_ALL`,
  `LC_CTYPE`, but for a key's name), then only what `setup` sets, sixteen names at most
  (`whizbang::pack_setup::UV_NAMES`): `PATH=/usr/bin:/bin`; `TMPDIR`, a folder of
  `setup`'s own; `UV_NO_CONFIG`, `UV_NO_CACHE`, `UV_NO_BUILD`; `UV_PYTHON_DOWNLOADS=never`;
  `UV_CONCURRENT_DOWNLOADS=4`, `UV_CONCURRENT_INSTALLS=4`, `UV_CONCURRENT_BUILDS=1`;
  `UV_PROJECT_ENVIRONMENT`; `UV_OFFLINE=1` unless an extra is installed; and, when
  `--python` names no Python, `UV_MANAGED_PYTHON=1` and `UV_PYTHON_INSTALL_DIR`, the folder
  `uv` keeps its Pythons in, as the caller's environment says (its own
  `UV_PYTHON_INSTALL_DIR`, else `uv/python` under `XDG_DATA_HOME`, else under
  `.local/share` in the home folder). No home folder, token, proxy, index, or Python
  variable of the caller's is handed on
  (`setup::uv_is_given_the_allow_list_and_no_users_config`).
- On its command line, every time: `--no-config`, `--no-cache`, `--no-python-downloads`,
  and `--offline` unless an extra is installed; and `--python <file>` or
  `--managed-python`.
- Its output goes to two files in `setup`'s folder, read afterwards (at most the last
  4 KiB of each), so no thread is made for it.
- 60 seconds for each `uv` with nothing to download; 30 minutes for an install with an
  extra. Past it the process is killed and waited for, and `setup` says so; on every
  other path out, a panic among them, the value that holds it kills it and waits for it
  (`setup::a_uv_that_does_not_end_within_its_limit_is_killed_and_waited_for`).

### What `setup` contacts

| With | Host | What for |
|---|---|---|
| No extra | None | Everything is offline |
| `--extra torch` | `files.pythonhosted.org` | The wheels the lock names, each checked against its SHA-256 by `uv` |

Making the lock, by hand on the owner's Mac (once for Y1, and again when a pin changes),
contacts `pypi.org` and `files.pythonhosted.org`, the origins the owner approved on
2026-11-15 for the Mac and never for CI. It is run from `pack/`, through
`scripts/heavy.sh` with the thread guard's limit at 200:

```
env -i PATH="$PATH" HOME="$HOME" TMPDIR="$TMPDIR" UV_NO_CONFIG=1 UV_PYTHON_DOWNLOADS=never \
  UV_CONCURRENT_DOWNLOADS=4 UV_CONCURRENT_INSTALLS=4 UV_CONCURRENT_BUILDS=1 \
  uv --no-config --no-python-downloads lock
```

The lock names no index but `https://pypi.org/simple` and no file but on
`https://files.pythonhosted.org/`, and the pack's file names no index, source or other
place to find a package (`setup::the_lock_file_names_no_index_but_the_approved_one`).
CI contacts no package index: its tests of `setup` install the core from the lock,
offline.

### What the lock holds

60 packages: the pack itself, 40 that the Mac installs with the extra, and 19 for Linux
alone. Their licences, read from each package's own metadata (the installed
distributions on the Mac; for the Linux-only ones, the index's metadata of the locked
version), are listed in the pull request of task Y1. R1 records them for the runtime it
chooses (open question 6). On the Mac, the environment with the extra is about 760 MB.

### Amended: the Mac alone

By the owner's decision of 2026-11-17 ("drop it"), each package of the extra `torch` in
`pack/pyproject.toml` carries the marker
`sys_platform == 'darwin' and platform_machine == 'arm64'`, and the lock was made again from
it (on the owner's Mac, by the command above, offline). `tool.uv.environments` still names
both platforms: `uv sync` refuses a platform the lock is not made for, the core's install
among them, so Linux on x86-64 stays, for the core alone; there the extra resolves to
nothing. Where this record says the lock pins the runtime for two platforms, read the Mac
alone. The new lock holds 41 packages: the pack and the 40 the Mac installs with the extra,
at the same versions as before (`torch` 2.15.0, `torchvision` 0.29.2, `transformers`
5.23.0). The 19 Linux-only packages (fifteen `nvidia-*`, `cuda-bindings`,
`cuda-pathfinder`, `cuda-toolkit` and `triton`), `torch`'s dependencies marked for Linux,
and the Linux wheels of the packages both platforms shared are gone; nothing was added but
the new markers
(`setup::the_extra_is_for_the_mac_alone_and_the_lock_names_no_nvidia_or_cuda_package`).

Why: on Linux on x86-64 the index's `torch` is the CUDA build, and it pulls in NVIDIA's
packages, three under NVIDIA's proprietary licence and thirteen whose metadata states no
licence. 1C installs and runs the runtime only on the owner's Mac, so the public lock
should name no proprietary package that nobody has tested.

What a Linux user gets: `whizbang setup` with no extra installs the core, offline, as
before; CI's Linux runner shows it. `whizbang setup --extra <name>` on any platform but
macOS on Apple silicon is refused in a sentence that says the extra is locked for that
platform only in this build, before any `uv` is started, so nothing is downloaded and
nothing is made (`whizbang::pack_setup::EXTRA_HOST`;
`setup::an_extra_on_a_platform_the_lock_does_not_cover_is_refused_before_anything_runs`,
which gives `install` a made-up platform).

The runtime on Linux comes back when a Linux story needs it: its "done when" starts with
the owner deciding on NVIDIA's licences, or on a CPU-only build from a host the owner
approves (`docs/carry-over.md`, "Linux runtime extra").

## Consequences

- Installing needs `uv` at its pin and, for the runtime, the network. No platform but the
  two locked can install the runtime; the core installs on any platform where the pinned
  Python does.
- A change of a pin is a change of `pack/pyproject.toml` and a new lock, made on the Mac;
  the binary carries both, so the lock it checks is the one it was built with.
- `uv` checks every file it downloads against the lock's hash. What `uv` itself verifies
  beyond that (the index's TLS) is `uv`'s.
- The environment's Python is a link to the Python `uv` manages; removing that Python
  breaks the environment, and a second `setup` makes it again.
- Reverting is deleting the folder `pack` of the models directory.

## Alternatives

- **Offline bundles, as rascal had.** Dropped: see "A plain install". The row of
  `docs/carry-over.md`, "Not ported", records it.
- **Resolving at install time, with no lock.** Turned down: an install would differ from
  machine to machine and from day to day, and nothing could hold it to the hashes.
- **The environment in the platform's data folder, apart from the models directory.**
  Turned down: a second place to name, flag and test, where the models directory already
  has its flag, its config key, its default and its owner's rules (ADR 0017).
- **A lock for every platform `uv` knows.** Turned down: it adds Windows and Linux on ARM,
  which whizbang does not run on, to every lock and its review.
