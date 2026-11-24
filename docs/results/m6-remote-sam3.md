# SAM 3 on the GPU host: what happened

Plan m6, task L1, items 2, 2b, 3, 5 and 6: SAM 3 on Modal, run live on 2026-11-24 by the
orchestrator from the owner's Mac. The host was deployed by `scripts/gpu-host.sh` from
clean checkouts of `main`. Whizbang called it through the kind `remote`. Only aggregates, run
ids and the host's own log lines are kept here. No endpoint, workspace, token, path or
picture is kept, and the dispatch ids of the host's log are replaced by `<dispatch>`.
User story 1, run against the same host, has its own record,
`docs/results/m6-user-story-1.md`. Both are held by
`evidence::the_records_of_milestone_6_hold_only_aggregates_and_run_ids`.

**What this is not.** It measures one card of one host on one day. The tolerance was met on
an A10G alone. Whether another card or another host meets it is not shown here.

## The host

```toml
commits = [
  "6f54442fce82f06aba5d924c5d30ed09b68d5ea9",
  "fc63a86b9b665808f0ae9f8f1f1e353f00021132",
  "deb8267be77f0ce082dfa33ae19e5c8427950c6e",
]
card = "NVIDIA A10G"
card_memory_mib = 23028
driver = "620.70.06"
driver_cuda = "13.5"
torch = "2.15.0"
cuda_runtime = "13.2"
modal_client = "1.6.1"
image_builder = "2025.06"
installation = "sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
price_microusd_per_second = 350
host_limit_s = 600
attempts = 2
bound_micros = 420000
```

**The commits.** The host was deployed three times, each from a clean checkout of `main`:

- 6f54442f, the merge of #297: item 2b's timings, parity's round A, and user story 1's first
  attempt.
- fc63a86b, the merge of #298: parity's round B, user story 1 as recorded, and the first
  three tries at a cancel.
- deb8267b, the merge of #299: item 5's cancel.

Item 6's stopped host was checked with the host down, after fc63a86b.

**The card.** An NVIDIA A10G with 23,028 MiB, driver 620.70.06, whose CUDA is 13.5. The
worker runs PyTorch 2.15.0 with the CUDA runtime 13.2 (`nvidia-cuda-runtime` 13.2.86), from
the host's lock, `deploy/gpu-host/requirements.txt`. The card, the driver and its CUDA were
read with `nvidia-smi` in a one-off A10G function on Modal. The worker's own load line names
the card, but the run record says `device cuda` alone (a follow-up, below).

**The client.** Modal's client was 1.6.1. The script pins the image builder at 2025.06,
since 1.6.1 refuses the workspace's default builder, 2026.10 (#297).

**SAM 3's files.** The installation above is the one the catalogue names and the Mac holds.
It was copied with `gpu-host.sh weights`, and all nine files in the volume match the local
sizes. The probe said "the host holds sam3 by the catalogue's hashes … checked by the
front".

**Prices**, read on 2026-11-24 from Modal's pricing page:

- the A10, 0.000306 dollars a second;
- the CPU, 0.0000131 dollars a core a second;
- memory, 0.00000222 dollars a GiB a second.

The config's `price_microusd_per_second` was 350, the card and the front together, rounded
up. `host_limit_s` was 600, the sum of ADR 0046's `LIMITS` (300 + 240 + 60). So each segment
request is charged at its bound of 0.42 dollars: 350 × 600 × 2 attempts.

**Addresses.**

- Modal's client used `api.modal.com` from the owner's Mac: the deploy, the volume's put,
  the app's list and logs, and the billing.
- Whizbang sent to the front's own endpoint alone, the web endpoint Modal names by the
  workspace, the app and the function. Its name is in the config outside the repository and
  is not kept here.
- The fills went to `fal.run`.

Probes start no card: the host's log shows only the front's `GET /pack/1/health` for them.

## Item 2b: cold calls against Modal's 150 seconds

The whole time of `whizbang rascal segment --phrase chair` against the host at 6f54442f,
whose front then sent its headers only once the files were checked:

| Call | Seconds | Note |
|---|---|---|
| The first ever | 161.0 | Pulling the new image; it succeeded |
| Cold 1 | 31.0 | After 150 seconds idle |
| Cold 2 | 52.2 | After 150 seconds idle |
| Cold 3 | 22.5 | After 150 seconds idle |
| Warm 1 to 3 | 2.6 each | |

In user story 1's first attempt, on the same host, one GPU cold start took about five
minutes. Two calls in a row failed at 150.8 seconds, each refused as a redirect ("the front
pointed the request elsewhere"), and the third took 129.3 seconds.

**The decision:** the front sends its headers at once, then keep-alives (#298; ADR 0046,
"The headers at once"). The carry-over row "A cold call to the GPU host on Modal" closed with
it. After #298, user story 1's first call, cold, took 417.2 seconds in all and completed.
Modal logged that request to the front as 388.9 seconds, with no redirect.

## Item 3: parity, two rounds of three

`remote_parity::sam3_answers_each_fixture_inside_the_tolerance_from_a_remote_worker`, run by
`scripts/test-models.sh parity-remote`. Each round asks each fixture three times, one request
each holding its phrase, its point and its box. Every answer is held to the committed one
within ADR 0027's tolerance: an IoU of at least 0.99 for every pair, scores within 0.01, and
the same counts. Every answer named `cuda`. Round B was run again after #298 changed the wire.

| Fixture | Least IoU | Widest score gap |
|---|---|---|
| circle | 1.000000 | 0.000001 |
| shapes | 1.000000 | 0.000002 |
| chair | 1.000000 | 0.000001 |

The figures are the same in both rounds. Each call's time follows, in milliseconds, its
three rounds in order.

### Round A

```toml
round = "A"
commit = "6f54442fce82f06aba5d924c5d30ed09b68d5ea9"
outcome = "passed"
seconds = 46.3
device = "cuda"
fixtures = ["circle", "shapes", "chair"]
least_iou = [1.0, 1.0, 1.0]
widest_score_gap = [0.000001, 0.000002, 0.000001]
call_ms = [[7344, 6351, 6080], [4260, 4872, 3379], [5657, 3567, 3559]]
```

### Round B

```toml
round = "B"
commit = "fc63a86b9b665808f0ae9f8f1f1e353f00021132"
outcome = "passed"
seconds = 85.7
device = "cuda"
fixtures = ["circle", "shapes", "chair"]
least_iou = [1.0, 1.0, 1.0]
widest_score_gap = [0.000001, 0.000002, 0.000001]
call_ms = [[57179, 4413, 4515], [2707, 2799, 4018], [2629, 3414, 2866]]
```

Round B's first call, circle's, was cold: 57.2 seconds, with the headers at once.

## Item 5: a cancel that reaches the card's worker

**The first tries.** Warm calls were cancelled 0.2 to 2.5 seconds after their runs began, on
the hosts at fc63a86b and deb8267b:

- Whizbang recorded each as cancelled and charged it at its bound.
- The cancel overtook its own request. At deb8267b the front logged "not open here; the host
  is not asked", and whizbang then closed the request's upload.
- The request never reached the front, and the card ran nothing for it.

That is what #299 fixed: a cancel that overtakes its request is now kept, where before it was
silently dropped. On a warm call the upload is most of the time, so a cancel during the
card's own compute is hard to hit. A cold start shows the control path end to end.

**The recorded cancel**, on the host at deb8267b:

```toml
run = "1795549608909-1"
tool = "segment"
fixture = "chair"
cancelled_after_seconds = 15.9
outcome = "cancelled"
charged_micros = 420000
call_ended_after_cancel_seconds = 5.117
worker = "killed"
```

`segment` on the fixture chair was cancelled 15 seconds in, during a GPU cold start.
Whizbang recorded it as cancelled after 15.9 seconds and charged it at its bound of 0.42
dollars. The host's log, in order:

1. The front: `cancel <dispatch>: handed to the worker's side`
2. The GPU container: `cancel <dispatch>: cancel frame written to worker 6`
3. The front: `cancel <dispatch>: handed to the host's control input, which cancelled its
   call`
4. `worker 6 ended: killed; the next call starts another`
5. `cancel <dispatch>: the call ended 5.117 s after its cancel, cancelled, the worker
   killed`

The cancel frame reached the worker, and the worker was killed at the cancel's deadline of 5
seconds. That is the plan's other way: "or the worker restarted at its deadline".

## Item 6: the probe, and a stopped host

- With the host up, `backends status --probe` named the model it holds (above). Repeated
  probes allocated no card.
- After `gpu-host.sh down`, the probe said "reached: its health answered with status 404".
- After `gpu-host.sh down`, `whizbang rascal remove-prompt` ended as an error at its segment
  step, in one second: "dispatch …: the connection failed; nothing was committed". Nothing
  fell back to another backend, and no pack worker started.
- That run was charged at its bound of 0.42 dollars although its connection failed: safe,
  and a follow-up below.

## Spend

| Where | Dollars |
|---|---|
| Modal, metered for the whole day | 0.56, all covered by the workspace's credits: 0.00 billed |
| fal, about 20 fills over every attempt at 0.04 each | about 0.80 |
| Whizbang's ledgers, every request at its bound, over every store of the round | about 13 |

The bound is far above what Modal metered: about 13 dollars counted against 0.56 metered.
Milestone 6's live spend was under 2 dollars in real money, against the plan's estimate of
8.50 and its stop at 40. Modal meters by the day here, so what each parity round cost apart
is not known. Only the day's total is compared with the ledgers.

## Found live

Fixed before this record, each in its own pull request:

- #297, four defects of the first deploy:
  - the front crashed in the container, reading a checkout that is not there;
  - the endpoint's pattern missed Modal's region label;
  - the deploy left bytecode that the next deploy refused;
  - the image builder was not pinned.
- #298: the headers go out at once, then keep-alives, with bounds that keep-alives cannot
  stretch (item 2b).
- #299: a cancel that overtakes its request is kept, and every cancel is a line of the
  host's log (item 5).

Follow-ups, each a row of `docs/carry-over.md`:

- The worker's card name ("card NVIDIA A10G" on its load) is not in the run record, which
  says `device cuda` alone.
- A segment request whose connection never opened is still charged at its bound.
- `backends status --probe` says "not probed: it is not ready" for fal's fill, which has no
  `probe = true`. The words mislead.
- The pack's test budget was raised to 12 seconds in #298, and the macOS runner takes about
  10.2 seconds.

`gpu-host.sh` has none left: #297 fixed the region pattern, the bytecode and the builder's
pin.
