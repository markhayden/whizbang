# Backends

A backend is what answers a capability: a local model the pack's worker runs, a model on
a GPU host of your own (the kind `remote`, below), or a provider somewhere else. The config file names them (`docs/guides/config.md`).
`whizbang backends status` says, for each one the config offers, whether it is ready, and
with `--probe` checks that it answers. The playground's Models screen shows the same list
without a probe, with whether each key is set (`docs/guides/playground.md`, "Models and
keys").

```text
whizbang backends status [--probe] [--config <file> [--env-file <file>]] [--models-dir <dir>]
```

## What it prints

A count, then one line for each backend, apart by tabs: its capability, its name, its
kind, its model, and whether it is ready; `default` at the end of the capability's
default. With a config of one local backend whose model is installed:

```text
$ whizbang backends status --config whizbang.toml
backends: 1
segment	default	pack	sam3	ready: installation sha256:<64 hex digits>	default
```

A backend that is not ready says why, in the words a call that chose it would be told:
its kind is not one this build has, its kind does not take its table, the env file does
not set the variable its key is in, or its model is not installed, is out of date, or is
damaged (`whizbang models list` says more, and `whizbang models get` installs it). A remote
backend that is ready says where it sends: `ready: sends to https://…`.

**Without `--probe` nothing is started and no host is contacted.** The config and the env
file are read, the models directory is looked in, and that is all.

## `--probe`

After the list, one line for each backend: `probe <capability> <name>: …`.

- **A local backend:** the pack's worker is started, says hello, and is stopped before the
  command ends. The line is its hello: the protocol's version, the device its work runs on
  before a model is loaded, and how many threads it held. No model is loaded. The worker
  is started once for the command, whatever the number of local backends. It needs the
  pack installed by `whizbang setup`; without it the line says to run that.
- **A remote backend that is ready:** probed only when the config approves probing its
  recipient (`probe = true` in its table under `[egress.recipients]`); otherwise the line
  says it is not approved, and nothing is asked. A probe is a `GET` that carries no key, no
  token and no data, and follows no redirect; the line is the status it was answered
  with. A backend of the kind `remote` is probed at its front's health,
  `<url>/pack/1/health`, which needs no token and wakes no card, and the line says whether
  the host holds the model the config asks, by the catalogue's hashes:
  `reached: the host holds sam3 by the catalogue's hashes, installation sha256:…, checked
  by the front`, or `reached: the host does not hold sam3 by the catalogue's hashes`.
- **A remote backend that is not ready** is not probed.

```text
$ whizbang backends status --probe --config whizbang.toml
backends: 1
segment	default	pack	sam3	ready: installation sha256:<64 hex digits>	default
probe segment default: the pack's worker said hello: pack/1, device cpu, 3 threads; it was stopped
```

Exit codes: 0 the backends were listed, and probed when asked, whatever each came to; 1
the config could not be used or the models directory could not be opened, and the
complaint says why; 2 the command line was not understood; 3 what was to be printed could
not be written.

The fast tests show each case with the pack's fake models and a stand-in for the prober
(`crates/whizbang/tests/it/backends.rs`), and the probe of a remote backend against the
pack's own front on loopback (`crates/whizbang/tests/it/remote.rs`); nothing in them
leaves the machine.

## The kind `remote`

SAM 3 on a GPU host: the pack's front (`python -m whizbang_pack.front`, ADR 0046) runs on
the host beside the card, and whizbang asks it over HTTPS for each segment, under the
gate, the ledger and the run's record exactly as it asks the local worker. The app's
server needs no GPU and no Python. Deploying such a host, on a machine of your own or on
Modal, is [its own guide](gpu-host.md).

```toml
[backends.segment]
kind = "remote"
url = "https://gpu-host.example"
token_env = "SEGMENT_TOKEN"
model = "sam3"

[backends.segment.settings]
price_microusd_per_second = 300
host_limit_s = 600
```

- **`url`** is the front's base address, `https` only; whizbang adds `/pack/1/request`,
  `/pack/1/cancel` and `/pack/1/health` to it.
- **`token_env`** names the variable the front's token is in. **The token is a secret of
  both sides:** the front is started with the same value (`--token-env`), and whizbang
  sends it as `Authorization: Bearer …` to the url's origin and to nowhere else.
- **`model`** is a model of the catalogue. The host must hold the installation of it that
  `whizbang models get` would make: every answer names the digest of the files it came
  from, and an answer of other files is refused whole.
- **The price is required.** `price_microusd_per_second` is what the host charges a second,
  in millionths of a dollar, and `host_limit_s` the most one attempt may be billed for (on
  Modal, the GPU function's startup limit, its timeout and its idle window added up). A
  call is charged at most `price × host_limit_s × 2`: the attempt whizbang makes and one
  restart of the host's. With the example, 0.36 dollars. **Without both settings every
  call is refused:** a backend that sends data and states no price has no bound. One
  without the other, or a zero, is refused when the config is read.
- **Its settings** may also be the model's own, as the kind `pack` takes them:
  `threshold`, `pad`, `min_cover`, `outline` and `device`, each sent in the model's
  `load`.
- **What it sends:** the picture's pixels and the words, point or box of each query. Its
  origin must be approved for `source_images` and `prompts` (`docs/guides/config.md`, "Who
  may receive data"); `probe = true` lets `--probe` ask its health.
- **What is charged:** a call refused before anything left costs nothing; every call that
  left is charged at its bound, an answered one too, since the host's bill is not known.
- **Cancelling:** when a run is cancelled or its time runs out, whizbang closes the stream
  and sends the host a `cancel` naming the request; a host that does not answer it within
  5 seconds is let go of.
