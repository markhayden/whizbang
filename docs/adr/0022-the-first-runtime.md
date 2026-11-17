# 0022. The first runtime: PyTorch through the `transformers` library, on the Apple GPU

Status: accepted.
Date: 2026-11-17
Spec: section 17, "Models run through ONNX on CPU only"; section 4, "The Python pack"

## Context

Section 17 replaces the old repos' ONNX on the CPU with one first runtime, chosen at
checkpoint 1C, that runs a SAM 3 phrase request on the Mac's GPU and has documented
support for point and box requests and for CUDA. The plan of 1C (D8) lists four
candidates in order, takes the first that meets that bar, and writes its tasks for
candidate A: N5 installed A's files (the installation
`sha256:37ab46c2f87b600f4654dd972ce85a27b1024b4fc92901b11464bd6fa9664d32`, nine files, with
`model.safetensors` as Meta published it) and Y1 locked A's packages (ADR 0021). Task R1
tries A on the owner's Mac.

"On the Mac's GPU" is shown with the CPU fallback off (`PYTORCH_ENABLE_MPS_FALLBACK` unset)
and the device the model reports (D8).

R1 contacted no host. The pages below were read for the plan on 2026-11-15 ("What was
looked up"); R1 read again only what is on the Mac: the installed packages' own source and
metadata.

## Decision

Candidate A is the runtime: SAM 3 through PyTorch and the `transformers` library, its model
placed on the Apple GPU (`mps`) by name, with no `device_map`.

### The runtime's packages

The runtime is the pack's extra `torch` (`pack/pyproject.toml`), as `pack/uv.lock` pins it; the
licence of each is from its own installed metadata (`License-Expression`, else `License`),
as Y1's pull request (number 158) also listed them. The test
`setup::the_runtime_the_adr_names_is_the_one_the_lock_pins` holds this table to the extra
and the lock.

| Package | Version | Licence, from its metadata |
|---|---|---|
| `torch` | 2.15.0 | Apache-2.0 AND Apache-2.0 WITH LLVM-exception AND BSD-2-Clause AND BSD-3-Clause AND BSL-1.0 AND ISC AND MIT AND Zlib |
| `torchvision` | 0.29.2 | BSD |
| `transformers` | 5.23.0 | Apache 2.0 License |

What they bring on the Mac, 37 packages more, with their licences from the same metadata:
`annotated-doc` MIT; `annotated-types` MIT; `anyio` MIT; `click` BSD-3-Clause; `filelock`
MIT; `fsspec` BSD-3-Clause; `h11` MIT; `hf-xet` Apache-2.0; `httpcore2` BSD-3-Clause;
`httpx2` BSD-3-Clause; `huggingface-hub` Apache-2.0; `idna` BSD-3-Clause; `jinja2` BSD (by
its classifier); `markdown-it-py` MIT (classifier); `markupsafe` BSD-3-Clause; `mdurl` MIT
(classifier); `mpmath` BSD; `networkx` BSD-3-Clause; `numpy` BSD-3-Clause AND 0BSD AND MIT
AND Zlib AND CC0-1.0; `packaging` Apache-2.0 OR BSD-2-Clause; `pillow` MIT-CMU; `pydantic`
MIT; `pydantic-core` MIT; `pygments` BSD-2-Clause; `pyyaml` MIT; `regex` Apache-2.0 AND
CNRI-Python; `rich` MIT; `safetensors` Apache-2.0 (classifier); `setuptools` MIT;
`shellingham` ISC; `sympy` BSD; `tokenizers` Apache-2.0 (classifier); `tqdm` MPL-2.0 AND MIT;
`truststore` MIT; `typer` MIT; `typing-extensions` PSF-2.0; `typing-inspection` MIT. None is
copyleft beyond file-level MPL-2.0 (`tqdm`), which binds changes to its own files only. The
weights' own licence is the SAM License (the plan's "What was looked up"; the installation
holds its copy).

### Each candidate against the bar

| Candidate | Phrase on the Apple GPU | Point and box, documented | CUDA, documented | Verdict |
|---|---|---|---|---|
| A. PyTorch, through `transformers` | **Shown by R1**, below, with the fallback off | Yes: `Sam3TrackerModel` takes `input_points` and `input_boxes` (`https://huggingface.co/docs/transformers/model_doc/sam3_tracker`; the installed `transformers/models/sam3_tracker/`) | Yes: the model card's examples choose CUDA (`https://huggingface.co/docs/transformers/model_doc/sam3`), and the lock's `torch` for Linux on x86-64 is a CUDA build with its CUDA libraries from the same index (ADR 0021) | **Meets the bar: chosen** |
| B. ONNX Runtime, Core ML provider | Not tried | No: the old export is phrase only | Yes, its CUDA provider | Not reached |
| C. MLX port | Documented by its author | Boxes; points not stated (`https://huggingface.co/mlx-community/sam3-image`) | No | Fails the bar |
| D. Meta's own package | No: CUDA is a stated prerequisite (`https://github.com/facebookresearch/sam3`; pull request 400 open) | Yes | Yes | Fails the bar |

### What was measured

By `pack/trial_runtime.py`, run by hand on the owner's Mac (Apple silicon, 16 hardware
threads, macOS's recommended maximum for the GPU 53,084 MiB) through `scripts/heavy.sh`,
the thread guard at 200 (`WHIZBANG_THREAD_LIMIT=200`) and `scripts/memory-watch.sh` at its
24 GiB, in a cleared environment (`env -i`), with the Python of the environment
`whizbang setup --extra torch` made. The trial starts itself again in the environment the
supervisor gives a worker (below) and refuses any other. One phrase, `circle`, on one
picture the trial draws (640 by 480: a grey ground, a red disc, a blue square), twice.

| Field | Value |
|---|---|
| Device | `mps`: every parameter and buffer, and the output masks |
| CPU fallback | Off: `PYTORCH_ENABLE_MPS_FALLBACK` unset, and refused by the trial if set; no operation was unsupported |
| Dtype | `float32` (the config's), 840,376,374 parameters |
| Load (from the installation's folder to the GPU, synchronised) | 4.66 s on the first run that reached the GPU; 2.20 and 1.90 s on the two after it |
| First request (processor, model, post-processing, synchronised) | 0.85 s (1.65 s on the first run) |
| Second request | 0.70 s (0.76 s on the first run) |
| Answer | One instance, score 0.934, box (110, 150, 291, 330) around the disc drawn at (110, 150, 290, 330); the same both times |
| Peak resident memory, the worker's tree | 566,672 KiB (553 MiB), by the watch; the weights are in the GPU's memory, which macOS does not count as resident |
| GPU memory | 5,301 MiB held by the driver, 3,398 MiB allocated, at the end |
| Peak threads in one process | 21, by the watch (the thread guard's limit is 200) |
| Watermark used | High 0.5 and low 0.5 of the recommended maximum, about 26 GiB; unchanged from the start |
| Processes besides the worker | None with the progress bars off; one, the standard library's resource tracker, with them on (below) |
| The environment's size | 758 MB (`du` of `pack/env`) |
| Hosts contacted, files wanted from the Hub | None: everything loaded from the installation's folder with `HF_HUB_OFFLINE=1` |

### The watermark: 0.5, and the low one with it

The supervisor sets `PYTORCH_MPS_HIGH_WATERMARK_RATIO=0.5` (the plan's "Rules", "GPU memory")
and now also `PYTORCH_MPS_LOW_WATERMARK_RATIO=0.5`. With the high one alone, the runtime
refused to place the model: "invalid low watermark ratio 1.4". Its default low watermark,
1.4 on this Mac, must not be above the high one, so any high one under 1.0 needs a low one
set too. Equal is the least change that passes the runtime's check. 0.5 left about 20 GiB
unused by one load and two requests, so it stays.

### Resident memory

The worker's tree peaked at 553 MiB resident; the weights live in the GPU's memory, which
the watermark caps. No cap on a process's resident memory that macOS is documented to hold
to was found in what R1 could read without contacting a host, so none is set in ordinary use
(the plan's "Rules"), and `scripts/memory-watch.sh` stays the cap in the model tier, at
24 GiB.

### Every thread pool the runtime has, and what holds each

| Pool | Held by | Source |
|---|---|---|
| PyTorch's intra-op pool (OpenMP on this build: "ATen parallel backend: OpenMP") | `OMP_NUM_THREADS=8`, and `torch.set_num_threads(8)`, called by the trial and to be called by the adapter (R2a) | `torch.__config__.parallel_info()` in the run: `at::get_num_threads() : 8`, `omp_get_max_threads() : 8` |
| PyTorch's inter-op pool | `torch.set_num_interop_threads(8)`, called before any parallel work | The same: `at::get_num_interop_threads() : 8` |
| Apple's Accelerate (`vecLib`) | `VECLIB_MAXIMUM_THREADS=8` | Its variable; the plan's "Rules" |
| PyTorch's `pthreadpool`, for the NNPACK and QNNPACK kernels of the CPU (`USE_PTHREADPOOL`, `USE_NNPACK=ON` in this build) | PyTorch sizes it from the intra-op setting, as its source has it (not read again here: the library on the Mac is compiled); the watch's count of 21 threads in the whole process bounds it in the run | `torch.__config__.show()` |
| MKL, OpenBLAS | `MKL_NUM_THREADS=8`, `OPENBLAS_NUM_THREADS=8`; the Mac's build has neither (`USE_MKL=OFF`, `BLAS_INFO=accelerate`), and both are set for Linux | `torch.__config__.show()` |
| The tokenizer's pool (`tokenizers`, Rayon) | `TOKENIZERS_PARALLELISM=false` | Its variable |
| `transformers`' loading of weights: a `ThreadPoolExecutor` for the load only | `min(4, cpu_count)` threads, fixed in the library (`GLOBAL_WORKERS`); `HF_DEACTIVATE_ASYNC_LOAD` turns it off. Four threads, ended with the load | `transformers/core_model_loading.py` |
| The GPU's own queues (Metal) | The system's; no setting | Counted by the watch within the 21 |

The worker's own three threads (reader, call, main) are the pack's (plan, "Rules").

### Processes the runtime starts

From the source: the progress bar `transformers` draws while it loads weights is `tqdm`'s,
and `tqdm` makes a lock of `multiprocessing` at its first bar (`tqdm/std.py`,
`TqdmDefaultWriteLock.create_mp_lock`). A lock of `multiprocessing` starts the standard
library's resource tracker (`multiprocessing.resource_tracker`): one Python process, a
child of the worker, that holds copies of the worker's three standard streams and ends at
the end of a pipe only the worker writes, so when the worker ends, however it ends.
Nothing else in the run started a process: `torch` prints a note from its
`torch.distributed.elastic` module at import and writes an empty folder,
`torchinductor_<user>`, in the temporary folder (the worker's own, which the supervisor
removes), and starts no compiler or pool, since nothing is compiled.

From the run, looked at four ways: the watch lists the worker's tree by parent id every
0.2 s, and when the worker has ended asks whether any process it saw is still running; the
trial asks the system for a child of its own (`os.waitpid(-1, os.WNOHANG)`, which starts
nothing) before the load, after it, after each request and at the end; it notes every
process Python is asked to start (the standard library's audit events for starting a
process); and it asks the resource tracker for its process id.

| Run | Children of the worker | Resource tracker | Seen in the tree | Still running when the worker had ended |
|---|---|---|---|---|
| Progress bars on | None before the load; one from the load to the end | A child, running, from the load on | One besides the worker | None |
| Progress bars off (`HF_HUB_DISABLE_PROGRESS_BARS=1`), the worker's environment | None, at every look | Not started | None | None |

No audit event was raised in either run: the tracker is started by the standard library
below them (`_posixsubprocess`). So the worker's environment turns the progress bars off,
and the runtime then starts no process. Should one start anyway, the tracker ends with the
worker, killed or at the end of its input, and the worker's streams then reach their end, so
the supervisor's readers are not kept waiting:
`test_trial_runtime.Processes.test_the_resource_tracker_ends_with_its_worker_killed_or_at_the_end_of_its_input`,
in the pack's tests on both CI runners.

### What the runtime needed that the plan did not expect

- The low watermark (above).
- The progress bars off, for the process they start (above).
- macOS adds `__CF_USER_TEXT_ENCODING` to the environment of every program it starts,
  whatever it was given; the trial accepts it, as the supervisor's test already does.
- Its error output at load carries two notes that are not faults: the one from
  `torch.distributed.elastic`, and a deprecation of a key of the model's config. The
  supervisor keeps the error output bounded and redacted, so neither needs more.
- Nothing from the Hub: no file was missing and no remote configuration was asked for.

## Consequences

- R2a writes the adapter `sam3` on `Sam3Model` and `Sam3Processor`, placed with
  `.to(torch.device("mps"))`, falling back to the CPU only when `mps` is not available, and
  calling `torch.set_num_threads` and `torch.set_num_interop_threads` before its first
  parallel work.
- Point and box requests (1D) use `Sam3TrackerModel` of the same library and the same
  files: no second runtime and no new download.
- The supervisor's environment gains two variables (`whizbang_backends::pack::worker`,
  `worker::the_workers_environment_holds_the_allow_list_and_no_key`).
- What the choice rules out: nothing. A second runtime is another adapter. Candidate A loads
  `model.safetensors` and no graph, so the old repos' check of a graph's external data is
  dropped (`docs/carry-over.md`).
- Reverting is a new ADR and a new extra; the lock and the adapter go with it.

## Alternatives

- **B, ONNX Runtime with Core ML.** Not reached: A met the bar first. It would need a new
  export, its hosting and its own lock, and the old export takes phrases only.
- **C, the MLX port.** Fails the bar: no CUDA, so Linux would need a second runtime.
- **D, Meta's own package.** Fails the bar: it requires CUDA until its pull request for the
  Apple GPU is merged.
- **The CPU fallback on.** Not needed: no operation fell back.
- **A smaller watermark.** Not needed: one model at 0.5 used under a fifth of what it
  allows.
