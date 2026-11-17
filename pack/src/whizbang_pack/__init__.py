"""The Python pack: the worker whizbang starts for a local model.

- `proto`: the frames of `pack/1` (docs/adr/0020-the-pack-protocol.md);
- `worker`: the worker and its entry point, `python -m whizbang_pack.worker`;
- `adapter`: what a model's adapter is to the worker;
- `fake_models`: the six fake models, answered by name;
- `fakes`: the entry point that takes `--fake-fault`, for the tests of the supervisor.

The standard library only: a model's runtime comes as an extra, where a model runs.
"""
