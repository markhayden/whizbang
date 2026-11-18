# Checkpoint 1C: SAM 3 answers a phrase on the Mac's GPU

Written by `scripts/model-evidence.sh` (plan 1C, task R2b; decision D15); the numbers are in
`m1c-local-pack.toml` beside this file, and
`evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has` fails
when they are not of the tree's adapter, lock and fixture.

**What ran.** The model tier, `just test-models` (`crates/whizbang/tests/it/model_tier.rs`),
three times, on the owner's Mac, under the heavy lock, the thread guard at 200 and the
watch on resident memory at 24 GiB: a request for the phrase `circle` about the fixture
picture (`docs/results/m1c-local-pack/circle.png`, drawn by a program: a red disc
on a grey ground beside a blue square), through the binary's wiring, the kind `pack`,
the queue and the supervisor of the one worker, the worker the installation of
`whizbang setup --extra torch` runs, and the adapter `sam3`, with the CPU fallback of the
GPU's runtime off. The run's record is read back by the real binary (`whizbang runs
show`). No command of the binary sends a phrase (no action is added in 1C), so a test
action asks for the segment capability.

| Field | Value |
|---|---|
| Measured | 2026-11-18 |
| Commit measured | `029da8f77d4d5964d45f178024cad4816f8bec92` (clean) |
| Lock file | `sha256:b844779adf5bf884fef63e33cc315db267ed61c3f67368922f5e8aef2d5f9215` |
| Adapter | `sam3`, revision `2`, source `sha256:81293bbaa9bb12ffd3ace1e02b4bd6c0f8e1091397f602ba1a5f15c897b1f258` |
| Installation | `sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652` |
| Device | `mps`, in every run's record |
| CPU fallback | Off: the worker's environment does not set `PYTORCH_ENABLE_MPS_FALLBACK` |
| Hosts contacted | None: the worker runs with `HF_HUB_OFFLINE=1` and loads with `local_files_only=True` from the installation's folder |

| Each run | 1 | 2 | 3 |
|---|---|---|---|
| Run of the phrase test | 1795020182287-1 | 1795020198406-1 | 1795020215915-1 |
| Instances | 1 | 1 | 1 |
| Best instance's score | 0.946 | 0.946 | 0.946 |
| Its box (left top right bottom) | 329.7 110.0 510.1 290.0 | 329.7 110.0 510.1 290.0 | 329.7 110.0 510.1 290.0 |
| Its mask | sha256:c49dcca1141934019bc00fec7875e866488bcf3c9413b723afa46b2b21f17028 | sha256:c49dcca1141934019bc00fec7875e866488bcf3c9413b723afa46b2b21f17028 | sha256:c49dcca1141934019bc00fec7875e866488bcf3c9413b723afa46b2b21f17028 |
| Pixels on | 25410 | 25410 | 25410 |
| Start of the first request to the load written, s | 0.07 | 0.07 | 0.08 |
| Load, s | 3.38 | 3.26 | 4.20 |
| Start of the first request to the model loaded, s | 3.45 | 3.33 | 4.28 |
| A full check's record found before the first load | yes | yes | yes |
| First request after the load, s | 0.87 | 0.86 | 0.87 |
| Second request, s | 0.67 | 0.66 | 0.66 |
| Loads for two requests | 1 | 1 | 1 |
| Run of the point and the box (plan 1D, task Q1) | 1795020187212-1 | 1795020202966-1 | 1795020219914-1 |
| Point and box in one request, s | 4.69 | 5.13 | 6.26 |
| Point at 420 200: candidates | 3 | 3 | 3 |
| Its best predicted IoU | 0.992 | 0.992 | 0.992 |
| Its box | 330.0 110.0 510.0 290.0 | 330.0 110.0 510.0 290.0 | 330.0 110.0 510.0 290.0 |
| Box 300 80 540 320: candidates | 3 | 3 | 3 |
| Its best predicted IoU | 0.774 | 0.774 | 0.774 |
| Its box | 330.0 107.0 510.0 293.0 | 330.0 107.0 510.0 293.0 | 330.0 107.0 510.0 293.0 |
| Peak resident memory of the tree, KiB | 585296 | 577824 | 619536 |
| Most threads in one process | 17 | 17 | 17 |
| Processes left running after | 0 | 0 | 0 |

The fixture's disc spans 330 to 510 across and 110 to 290 down; the loose box drawn by
hand around it, which the test holds an instance's box to, is 300, 80, 540, 320, and is
the box asked of the tracker; the point asked is the disc's centre, 420, 200. The
times are of one machine on one day and are reported, not compared: the load is the
worker's `load` call (weights from the disk onto the GPU), and the first request's time
beside it includes the processor's first run. The time before the load is written holds
the check of the installation's files, the worker's start and its import of the runtime.
The check is the cached one of plan 2A, task R1: a run of the model tier before the
three, whose numbers are not kept, hashed the installation's 3.4 GB in full and left
the record of that check, so each round's first load compares each file's size, time
of last write and inode against it and reads no file whole. The memory is resident
memory of the
whole tree (cargo, the test, the worker); the weights are in the GPU's memory, which
macOS does not count as resident. The thread count is the most in one process, the
worker's runtime pools at 8 each.
