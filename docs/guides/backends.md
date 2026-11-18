# Backends

A backend is what answers a capability: a local model the pack's worker runs, or, from
milestone 2, a provider somewhere else. The config file names them (`docs/guides/config.md`).
`whizbang backends status` says, for each one the config offers, whether it is ready, and
with `--probe` checks that it answers.

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
  says it is not approved, and nothing is asked. A probe carries no key and no data.
  **This build has no network client,** so even an approved probe contacts nothing: the
  line says so. Milestone 2 brings the first remote kind, and with it a real probe.
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
(`crates/whizbang/tests/it/backends.rs`); nothing in them leaves the machine.
