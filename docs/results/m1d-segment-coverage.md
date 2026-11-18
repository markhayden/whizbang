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
measured = "2026-11-18"
commit = "67eb42d5cc43bd25abaec79e9c247f8b296ddadd"
tree = "clean"
sam3_revision = "2026-11.2"
sam3_adapter_revision = "2"
sam3_adapter_source = "sha256:89cb76a160810418d7e33da9ccb47d47d5ff486fa88a46e29741dafefd32a410"
tiny_revision = "2026-11.1"
tiny_adapter_revision = "1"
tiny_adapter_source = "sha256:fb4f273ae69f9ea086aa757b1709990d2325eb5d556bcb2c5218d9828e505053"
device = "mps"
sam3_phrase_load_seconds = "3.26"
sam3_phrase_first_seconds = "4.17"
sam3_phrase_second_seconds = "0.66"
sam3_point_load_seconds = "3.24"
sam3_point_first_seconds = "4.68"
sam3_point_second_seconds = "0.60"
sam3_box_load_seconds = "2.65"
sam3_box_first_seconds = "4.08"
sam3_box_second_seconds = "0.60"
tiny_phrase_load_seconds = "2.32"
tiny_phrase_first_seconds = "2.79"
tiny_phrase_second_seconds = "0.21"
tiny_point_load_seconds = "1.98"
tiny_point_first_seconds = "2.32"
tiny_point_second_seconds = "0.09"
tiny_box_load_seconds = "1.99"
tiny_box_first_seconds = "2.31"
tiny_box_second_seconds = "0.09"
sam3_one_model_gpu_mib = 4269
sam3_two_models_gpu_mib = 6363
peak_resident_kib = 653936
most_threads_in_one_process = 18
still_running_after = 0
```
