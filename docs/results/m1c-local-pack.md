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
| Commit measured | `11c251d6827373e518b02c344e854df514c61403` (changed) |
| Lock file | `sha256:b844779adf5bf884fef63e33cc315db267ed61c3f67368922f5e8aef2d5f9215` |
| Adapter | `sam3`, revision `1`, source `sha256:43ec5dbd3773d7d0b032b3a0368c09b04107decd2ee170e59447f825bcbe87bc` |
| Installation | `sha256:37ab46c2f87b600f4654dd972ce85a27b1024b4fc92901b11464bd6fa9664d32` |
| Device | `mps`, in every run's record |
| CPU fallback | Off: the worker's environment does not set `PYTORCH_ENABLE_MPS_FALLBACK` |
| Hosts contacted | None: the worker runs with `HF_HUB_OFFLINE=1` and loads with `local_files_only=True` from the installation's folder |

| Each run | 1 | 2 | 3 |
|---|---|---|---|
| Run of the phrase test | 1794970762498-1 | 1794970831985-1 | 1794970903610-1 |
| Instances | 1 | 1 | 1 |
| Best instance's score | 0.946 | 0.946 | 0.946 |
| Its box (left top right bottom) | 329.7 110.0 510.1 290.0 | 329.7 110.0 510.1 290.0 | 329.7 110.0 510.1 290.0 |
| Its mask | sha256:c49dcca1141934019bc00fec7875e866488bcf3c9413b723afa46b2b21f17028 | sha256:c49dcca1141934019bc00fec7875e866488bcf3c9413b723afa46b2b21f17028 | sha256:c49dcca1141934019bc00fec7875e866488bcf3c9413b723afa46b2b21f17028 |
| Pixels on | 25410 | 25410 | 25410 |
| Start of the first request to the load written, s | 26.28 | 26.56 | 27.08 |
| Load, s | 6.55 | 6.13 | 7.67 |
| First request after the load, s | 0.90 | 0.94 | 1.03 |
| Second request, s | 0.66 | 0.67 | 0.69 |
| Loads for two requests | 1 | 1 | 1 |
| Peak resident memory of the tree, KiB | 572416 | 592400 | 599104 |
| Most threads in one process | 17 | 17 | 17 |
| Processes left running after | 0 | 0 | 0 |

The fixture's disc spans 330 to 510 across and 110 to 290 down; the loose box drawn by
hand around it, which the test holds an instance's box to, is 300, 80, 540, 320. The
times are of one machine on one day and are reported, not compared: the load is the
worker's `load` call (weights from the disk onto the GPU), and the first request's time
beside it includes the processor's first run. The time before the load is written holds
the check of the installation's files, 3.4 GB hashed before a load (decision D14), the
worker's start and its import of the runtime. The memory is resident memory of the
whole tree (cargo, the test, the worker); the weights are in the GPU's memory, which
macOS does not count as resident. The thread count is the most in one process, the
worker's runtime pools at 8 each.
