# A GPU host of your own

SAM 3 can run off the machine that runs whizbang: on a GPU host that you deploy, with your
own account and your own tokens, which whizbang reaches through the backend kind `remote`
([the backends guide](backends.md), "The kind `remote`"). It is how a hosted app's server
that has no GPU and no Python segments a picture ([the engine API](engine-api.md)). Whizbang
runs no host for you: you host your own copy, with the deployment and this guide. Whizbang's
own deployment on Modal is for its testing. The decisions behind it are in ADR 0046.

Each example names its tier: **the fast tier** (`just check`, against fakes, contacting
nothing), or **live**, against a real host and a real card, which only you run, and which
spends.

## The front and the worker

A host has two parts.

- **The front** is a small HTTP server of the pack, `python -m whizbang_pack.front`. It
  speaks `pack/1` over HTTP, checks the bearer token, holds a queue of four requests, checks
  the model's files against their record before each load, and answers a probe of its
  health without a card. It uses Python's standard library alone.
- **The worker** is the pack's own worker, the same one whizbang starts on a Mac, with SAM 3
  loaded on an NVIDIA card. The front supervises it: one call at a time, a cancel with a
  deadline of 5 seconds, a restart that fails what was in flight and replays nothing.

Each request carries a dispatch id that whizbang made. Before any model work the worker's
side claims that id in a store of the host's own, so a request delivered again is answered
as a replay and runs no model. Every answer names the digest of the model files it came
from, and whizbang refuses an answer whose files are not the catalogue's.

## On a machine of your own

Any Linux machine on x86-64 with an NVIDIA card and Python 3.12 can be a host:

1. Install the runtime from the host's lock, every file pinned by its hash, from pypi.org
   alone: `pip install --require-hashes -r deploy/gpu-host/requirements.txt`. It holds
   PyTorch's CUDA build and NVIDIA's packages, under their own licences
   (`deploy/gpu-host/LICENSES.md`).
2. Put SAM 3's installation in a models directory there, as "SAM 3's files" below says.
3. Start the front with the worker beside it, on HTTPS:

   ```text
   SEGMENT_TOKEN=<token> python -m whizbang_pack.front --listen 0.0.0.0:8443 \
       --models-dir /srv/whizbang-models --token-env SEGMENT_TOKEN --worker local \
       --host-limit-s 600 --tls-cert cert.pem --tls-key key.pem
   ```

   with `pack/src` on `PYTHONPATH`. The kind `remote` takes an `https` address only: give
   the front a certificate, or stand it behind a proxy of your own that ends TLS. Without
   `--tls-cert` it speaks plain HTTP.

The front and its supervision are held by the pack's tests (`pack/tests/test_front.py`, fast
tier), and whizbang's side of it against the pack's own front on loopback with fake models
(`remote::a_call_is_sent_through_the_gate_with_its_token_bound_to_its_origin`, fast tier).

## On Modal

`deploy/modal/sam3_host.py` deploys the same front and worker to Modal, into whichever
workspace your Modal client is signed in to. Nothing in the file names a workspace, a token
or an endpoint.

- **The front** runs as a CPU function, one container that takes many connections at once,
  so a probe needs no card and a cancel is never queued behind a running call.
- **The bridge** is one GPU class on a 24 GB card (`A10G`), at most one container, which
  takes two inputs at once: the call, and a control input for its cancel. It keeps SAM 3
  loaded between calls, claims each dispatch id in the Modal `Dict` `whizbang-claims` before
  any model work, and asks Modal for no retry.
- **The volume** `whizbang-models` is the models directory, mounted on both.
- **The secret** `whizbang-front` holds the front's token under `SEGMENT_TOKEN`.

The bridge's parts are shown against a stand-in for Modal in the pack's tests, fast tier:
`test_front.Bridge.test_under_one_call_and_one_control_input_at_a_time_a_cancel_is_not_queued_behind_the_call`,
`test_front.Bridge.test_a_front_replaced_mid_call_leaves_the_bridges_call_to_end_at_its_limit`
and
`test_front.Bridge.test_two_generations_never_overlap_a_deploy_waits_for_the_old_one_to_stop_and_a_redelivery_after_it_finds_its_claim`.
The deployment itself is imported with Modal's client faked, as the container imports it,
from `/root` with no checkout above it, and as the deployer's checkout does
(`pack/tests/test_sam3_host.py`, fast tier).

### What you set up once

1. **A Modal workspace with a payment method.** Its spend counts on your bill, not
   whizbang's.
2. **Modal's client and its token on your machine.** Install the client, version 1.6.1 or
   later (`scripts/gpu-host.sh` refuses an older one: it reads the client's JSON as 1.6.1
   prints it, and the bridge's claim needs `Dict.put` to say whether it put), for example
   with `uv tool install modal==1.6.1` or `pip install modal==1.6.1`. Then run
   `modal token new` once. The client keeps the token in its own file;
   `scripts/gpu-host.sh` never reads it.
3. **The front's token:** make a random token of at least 32 printable characters, for
   example with `openssl rand -hex 32`. Put it in whizbang's env file as
   `SEGMENT_TOKEN=<token>`, and give the same value to Modal as the secret `whizbang-front`,
   key `SEGMENT_TOKEN`, in Modal's dashboard (Secrets, a custom secret), so that it is not
   left in a shell's history.
4. **A spend limit on the workspace.** Set one in Modal's settings; 50 dollars a month is a
   sensible start. It is the backstop for what whizbang cannot bound (below).
5. **SAM 3's files in the volume:** `scripts/gpu-host.sh weights`, below, once the app is
   deployed.

### Deploying, probing, stopping

`scripts/gpu-host.sh` runs Modal's client for you (live; its refusals are held with Modal's
command faked by `scripts/tests/test-gpu-host.sh`, fast tier):

```text
scripts/gpu-host.sh deploy <commit> --config <config> --endpoint-file <file>
scripts/gpu-host.sh weights --models-dir <models directory>
scripts/gpu-host.sh probe --config <config> --env-file <env file>
scripts/gpu-host.sh down
```

- **`deploy`** deploys from a clean checkout at the commit named, and refuses unless the
  three limits of one attempt in the file are there and add up to the config's
  `host_limit_s`. It refuses, too, unless Modal lists the app as stopped with no container
  left, or lists none of that name: an app that is deployed, even with no container running,
  is not stopped. So run `down` before every deploy after the first; two deployments never
  run at once. The front's endpoint, which names your workspace, is written to the
  endpoint file, outside the repo, and not printed: put it in the config as `url`. It
  deploys with Modal's image builder 2025.06 (`MODAL_IMAGE_BUILDER_VERSION`), one that the
  client 1.6.1 supports, whatever your workspace's default is, and with Python writing no
  bytecode beside the deployment, so the next deploy finds no cache to refuse.
- **The Modal environment** is the client's active one. If you keep an environment for
  whizbang, set `MODAL_ENVIRONMENT` to its name for every command of the script.
- **`weights`** checks SAM 3's installation with `whizbang models verify sam3` and copies it
  into the volume at the same place. A container already running when the files arrive does
  not see them until it starts again: run `weights` straight after the first deploy, before
  any probe or call, or run `down` and deploy again after it.
- **`probe`** is `whizbang backends status --probe`: it asks the front's health, with no
  token and no user data, and says whether the host holds the model the config asks.
- **`down`** stops the app, then waits until Modal lists no container of it, and fails if
  one is left after 15 minutes. Run it when you are done, so nothing bills in between.

### Modal's limits that matter

A web endpoint on Modal answers a request for at most 150 seconds, then redirects. Whizbang
follows no redirect: such a call ends as an error and is charged at its bound. What counts is
how long the answer's headers take, so the front sends them at once, once it has checked the
token and read the request, before anything waits on the card: the GPU container's start, the
claim and the worker's hello come after them. The identity of the files the card's side
checked follows in the body, ahead of the first frame, and a failure in the outcome record, as
before. While the card starts, the front sends a keep-alive (eight zero bytes, which the
client passes over) after every 10 seconds of nothing else, under the client's limit between
two reads: for a call, `host_limit_s` (600 seconds on Modal), and 30 seconds for any other
answer the client reads. Its first keep-alive goes with the headers, so a proxy that
holds headers until the body begins passes them on at once.

Measured on an A10G in the live round of plan m6 (task L1), with the headers sent once the
files were checked, as the front did then: a whole call took 22 to 52 seconds cold as a rule
and 2.6 seconds warm; the first call after a new image took 161 seconds; and one cold start
took about 5 minutes, in which two calls in a row failed at 150.8 seconds on the redirect and
the third took 129 seconds. With the headers at once, a cold call no longer meets the 150
seconds whatever its start takes, up to the bridge's `startup_timeout` of 300 seconds.

The front logs each cancel and where it went, each cancel frame written to a worker and how
long after it the call ended, and each worker started and ended, a line each in `modal app
logs`, naming the dispatch id and never a token, an image or a prompt. On the front's
container, a cancel the bridge did not send to the card says why: no call of it was open
there (it is kept, and a request of that dispatch that comes after it ends `cancelled`
without waking the card), or the card's cancel raised, named by the exception's type alone.

## The config

```toml
[backends.segment]
kind = "remote"
url = "https://<the front's endpoint>"
token_env = "SEGMENT_TOKEN"
model = "sam3"

[backends.segment.settings]
price_microusd_per_second = 300
host_limit_s = 600

[egress.recipients.gpu]
origin = "https://<the front's endpoint>"
classes = ["source_images", "prompts"]
probe = true
```

The `url` and the `origin` are the endpoint the deploy wrote. The token is read from the env
file and sent to that origin alone.

## The price, and what `host_limit_s` covers

`price_microusd_per_second` is what the host costs a second, the card's and the front's
together, in millionths of a dollar: read it from your host's prices. `host_limit_s` is the
longest one attempt may be billed for. On Modal it is the bridge's `startup_timeout`,
`timeout` and `scaledown_window` added up, which the deployment sets in one table (`LIMITS`,
300, 240 and 60 seconds, 600 in all), and `deploy` refuses a config whose `host_limit_s`
differs. Whizbang charges each call at most `price × host_limit_s × 2`, an attempt of its own
and one restart of the host's: 0.36 dollars with the example. A call that left is charged
at that bound whatever it came to; the ledger then holds at least what Modal billed for it,
but for the residual below.

**What whizbang does not bound.** Modal gives no bound for one request: it restarts a
container that crashed, without end, and an input whose container was preempted, with no
stated count, each with a new startup. Whizbang's bound covers two attempts. More restarts
than that, after a crash or a preemption, a container that crashes as it starts among them,
are not bounded by whizbang, and can cost more than the ledger says. **The workspace's spend
limit is the backstop**: set one (50 dollars a month is a sensible start). It caps the
workspace for the month, not a request, and Modal does not state how fast it stops work.

## SAM 3's files

Whizbang hosts no copy of SAM 3's files. You download them yourself, from the maker's page
on Hugging Face, with your own sign-in, after accepting the SAM License there
([the models guide](models.md), "A model whose files need a token"); the host itself never
signs in to Hugging Face and downloads nothing. Today `whizbang models get sam3` runs on a Mac
with Apple silicon: install it there, then copy it to the host with `scripts/gpu-host.sh
weights`, which checks every file first. On a machine of your own, copy the same folder,
`models/sam3/installations/<identity>/` with its `RECORD` and its `files/`, into the host's
models directory.

**What you are told of SAM 3's licence before its files are used:** it is the SAM License,
whose terms you accept on the maker's page before the download; the copy the catalogue pins
is kept beside the files as `LICENSE`; and whoever publishes results of research done with
the model acknowledges its use in that publication ([the models guide](models.md), "The
licences, and what a publication owes"). This guide is not legal advice.

## The token

The front's token is a secret of both sides: the front reads it from its variable once, at
start, and whizbang sends it as `Authorization: Bearer …` to the front's origin and nowhere
else. It is taken out of every line of the worker's error output the front hands on. A
changed token needs the front started again (on Modal, a new deploy after `down`).
