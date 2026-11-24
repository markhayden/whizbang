# The GPU host's packages and their licences

What `deploy/gpu-host/requirements.txt` installs into the GPU host's image (`docs/plans/m6-http-and-remote-hosting.md`, task D1, item 1; the owner's decision 3 of that plan): the pack's runtime for Linux on x86-64 with CUDA. It is built on the host's side, from that lock, and nothing of it ships in whizbang's release archive, the Mac's lock (`pack/uv.lock`) or `THIRD-PARTY-LICENSES.md`.

Each package's licence is given as its own metadata on pypi.org states it, read on 2026-11-23 for the version pinned: its `License-Expression` where it has one, else its `License` field, else its licence classifiers. "None stated" means the metadata names none. NVIDIA's packages (`nvidia-*`, `cuda-*`) come under NVIDIA's own licence terms, which their files carry, whether or not their metadata names them; a host that installs them accepts those terms. When the lock is made again (`scripts/gpu-host.sh lock`), this table is read again from the new versions' metadata.

| Package | Version | Licence, as its metadata states it | Field |
|---|---|---|---|
| annotated-doc | 0.0.6 | MIT | License-Expression |
| annotated-types | 0.8.0 | MIT | License-Expression |
| anyio | 4.16.0 | MIT | License-Expression |
| click | 8.5.1 | BSD-3-Clause | License-Expression |
| cuda-bindings | 13.5.0 | Apache-2.0 | License-Expression |
| cuda-pathfinder | 1.9.0 | Apache-2.0 | License-Expression |
| cuda-toolkit | 13.2.2 | None stated | - |
| filelock | 4.2.10 | MIT | License-Expression |
| fsspec | 2026.9.0 | BSD-3-Clause | License-Expression |
| h11 | 0.16.0 | MIT | License |
| hf-xet | 1.8.0 | Apache-2.0 | License-Expression |
| httpcore2 | 2.16.0 | BSD-3-Clause | License-Expression |
| httpx2 | 2.16.0 | BSD-3-Clause | License-Expression |
| huggingface-hub | 2.4.0 | Apache-2.0 | License |
| idna | 3.20 | BSD-3-Clause | License-Expression |
| jinja2 | 3.1.6 | BSD License | classifier |
| markdown-it-py | 4.3.0 | MIT License | classifier |
| markupsafe | 3.0.4 | BSD-3-Clause | License-Expression |
| mdurl | 0.1.2 | MIT License | classifier |
| mpmath | 1.3.0 | BSD | License |
| networkx | 3.7 | BSD-3-Clause | License-Expression |
| numpy | 2.5.5 | BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0 | License-Expression |
| nvidia-cublas | 13.4.1.3 | None stated | - |
| nvidia-cuda-cccl | 13.3.5.0.6 | LicenseRef-NVIDIA-Proprietary | License-Expression |
| nvidia-cuda-cupti | 13.2.86 | None stated | - |
| nvidia-cuda-nvrtc | 13.2.86 | None stated | - |
| nvidia-cuda-runtime | 13.2.86 | None stated | - |
| nvidia-cudnn-cu13 | 9.26.0.51 | None stated | - |
| nvidia-cufft | 12.2.0.57 | None stated | - |
| nvidia-cufile | 1.17.1.22 | None stated | - |
| nvidia-cusolver | 12.2.0.11 | None stated | - |
| nvidia-cusparse | 12.7.10.12 | None stated | - |
| nvidia-cusparselt-cu13 | 0.8.1 | NVIDIA Proprietary Software | License |
| nvidia-nccl-cu13 | 2.30.7 | None stated | - |
| nvidia-nvjitlink | 13.5.37 | LicenseRef-NVIDIA-Proprietary | License-Expression |
| nvidia-nvshmem-cu13 | 3.7.2 | None stated | - |
| nvidia-nvtx | 13.2.86 | None stated | - |
| packaging | 26.3 | Apache-2.0 OR BSD-2-Clause | License-Expression |
| pillow | 13.0.0 | MIT-CMU | License-Expression |
| pydantic | 2.14.3 | MIT | License-Expression |
| pydantic-core | 2.50.4 | MIT | License-Expression |
| pygments | 2.21.0 | BSD-2-Clause | License-Expression |
| pyyaml | 6.0.3 | MIT | License |
| regex | 2026.11.15 | Apache-2.0 AND CNRI-Python | License-Expression |
| rich | 15.0.0 | MIT | License |
| safetensors | 0.8.0 | Apache Software License | classifier |
| setuptools | 84.2.1 | MIT | License-Expression |
| shellingham | 1.5.4 | ISC License | License |
| sympy | 1.14.0 | BSD | License |
| tokenizers | 0.23.3 | Apache Software License | classifier |
| torch | 2.15.0 | Apache-2.0 AND Apache-2.0 WITH LLVM-exception AND BSD-2-Clause AND BSD-3-Clause AND BSL-1.0 AND ISC AND MIT AND Zlib | License-Expression |
| torchvision | 0.29.2 | BSD | License |
| tqdm | 4.70.1 | MPL-2.0 AND MIT | License |
| transformers | 5.23.0 | Apache 2.0 License | License |
| triton | 3.9.0 | MIT | License |
| truststore | 0.10.5 | MIT | License-Expression |
| typer | 0.29.1 | MIT | License-Expression |
| typing-extensions | 4.16.0 | PSF-2.0 | License-Expression |
| typing-inspection | 0.4.5 | MIT | License-Expression |
