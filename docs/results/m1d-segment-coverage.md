# Checkpoint 1D: timings of phrase, point and box on SAM 3 and Tiny

Written by `scripts/model-evidence.sh` (plan 1D, task V1, item 4) from one run of
`scripts/test-models.sh coverage` (`crates/whizbang/tests/it/coverage.rs`) on the owner's
Mac, under the heavy lock, the thread guard at 200 and the watch on resident memory;
`evidence::the_coverage_record_names_a_commit_of_this_tree_and_every_timing` holds the
fields below to the tree. The picture is the model tier's (`circle.png`, drawn by a
program), asked the phrase `circle`, the point 420 200 or the box 300 80 540 320. No
picture of the owner's.

For each model and each kind of query, a worker of its own: `first` is the first
request from the action's side, the worker's start, the cached check of the installation's
files, the runtime's import and the load included; `load` is the worker's `load` call
alone, by the supervisor's status; `second` is a second request of the same kind, the
model resident. SAM 3's point and box load its tracker on the first of them, inside the
request. The GPU's memory is what the driver holds for the worker
(`torch.mps.driver_allocated_memory`), said by the adapter `sam3` beside each answer on
the GPU: after a phrase (`Sam3Model` resident) and after a point that followed it (both
models resident). The resident memory is the watch's peak over the whole tree (cargo, the
test, the worker), which does not count the GPU's. Times are of one machine on one day,
reported, not compared.

```toml
record = "m1d-segment-coverage"
measured = "2026-11-19"
commit = "4dd4a7b2e798f2818a25b36815b749b9b0b35432"
tree = "clean"
sam3_revision = "2026-11.2"
sam3_adapter_revision = "2"
sam3_adapter_source = "sha256:0c83e562d8c5a402b64fd891d61b6ea762a3c4c4a6e889822f5c274d54403945"
tiny_revision = "2026-11.1"
tiny_adapter_revision = "1"
tiny_adapter_source = "sha256:fb4f273ae69f9ea086aa757b1709990d2325eb5d556bcb2c5218d9828e505053"
device = "mps"
sam3_phrase_load_seconds = "2.98"
sam3_phrase_first_seconds = "4.00"
sam3_phrase_second_seconds = "0.75"
sam3_point_load_seconds = "2.93"
sam3_point_first_seconds = "4.40"
sam3_point_second_seconds = "0.66"
sam3_box_load_seconds = "2.96"
sam3_box_first_seconds = "4.44"
sam3_box_second_seconds = "0.68"
tiny_phrase_load_seconds = "2.32"
tiny_phrase_first_seconds = "2.81"
tiny_phrase_second_seconds = "0.22"
tiny_point_load_seconds = "2.22"
tiny_point_first_seconds = "2.58"
tiny_point_second_seconds = "0.09"
tiny_box_load_seconds = "2.15"
tiny_box_first_seconds = "2.50"
tiny_box_second_seconds = "0.08"
sam3_one_model_gpu_mib = 4269
sam3_two_models_gpu_mib = 6371
peak_resident_kib = 672992
most_threads_in_one_process = 17
still_running_after = 0
```
