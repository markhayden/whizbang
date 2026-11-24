# User story 1, live: what happened

Plan m6, task L1, item 4. `SPEC.md` section 1, user story 1: "An app lets people upload an
image, name or click a thing to remove, and download the result. The app server runs
`whizbang serve --http` with no GPU and no Python. Segmentation runs on SAM 3 on a GPU host;
the fill runs on Bria through fal." It was run live on 2026-11-24 by the orchestrator, from
the owner's Mac, against SAM 3 on Modal (`docs/results/m6-remote-sam3.md`) and Bria Eraser
through fal. Only aggregates and run ids are kept here. No endpoint, workspace, token, path,
picture, or name of a picture of the set is kept, and the results were looked at by eye and
are kept out of the repository. This record and the host's are held by
`evidence::the_records_of_milestone_6_hold_only_aggregates_and_run_ids`.

**What this is not.** It scores nothing. It shows that the story runs end to end through the
engine API, with no local worker. How good each removal is belongs to milestone E.

## The setup

- `whizbang serve --http 127.0.0.1:<port>` was built at fc63a86b and run against the host
  deployed from the same commit.
- It ran in a new home, with an empty models directory (`models list`: sam3 not installed),
  no pack installed, and a new store with an allowance of 10 dollars.
- No pack worker ran at any point: `pgrep` found none.
- The app was a short Python client outside the repository that speaks the engine API alone:
  an upload, `tools/call`, a download.
- The pictures were three sources of milestone 3's edit set (`labs/m3-edits/edits.toml`, drawn
  again from its set with seed 20261119, which gave the committed manifest), called A, B and
  C here.
- **Named:** `remove_prompt` with the words "a red ball".
- **Clicked:** `segment` of the kind `point` at the ball's centre with `max_instances` 1, then
  `fill` with that mask.

## The runs

```toml
commit = "fc63a86b9b665808f0ae9f8f1f1e353f00021132"
runs = 9
completed = 9
local_workers = 0
segment_bound_micros = 420000
fill_micros = 40000
story_micros = 2760000
```

| How | Picture | Tool | Run | Seconds | Outcome | Charged |
|---|---|---|---|---|---|---|
| Named | A | `remove_prompt` | 1795543984279-1 | 417.2, cold | completed | Through its steps: segment 0.42, fill 0.04 |
| Clicked | A | `segment` | 1795544401527-10 | 11.9 | completed | 0.42 |
| Clicked | A | `fill` | 1795544413393-11 | 26.0 | completed | 0.04 |
| Named | B | `remove_prompt` | 1795544439470-12 | 42.1 | completed | 0.42 and 0.04 |
| Clicked | B | `segment` | 1795544481575-21 | 14.1 | completed | 0.42 |
| Clicked | B | `fill` | 1795544495732-22 | 34.5 | completed | 0.04 |
| Named | C | `remove_prompt` | 1795544530253-23 | 38.4 | completed | 0.42 and 0.04 |
| Clicked | C | `segment` | 1795544568664-32 | 11.9 | completed | 0.42 |
| Clicked | C | `fill` | 1795544580568-33 | 22.2 | completed | 0.04 |

```toml
[[run]]
how = "named"
picture = "A"
tool = "remove_prompt"
run = "1795543984279-1"
seconds = 417.2
outcome = "completed"
charged_micros = 460000
recipients = ["gpu", "fal"]

[[run]]
how = "clicked"
picture = "A"
tool = "segment"
run = "1795544401527-10"
seconds = 11.9
outcome = "completed"
charged_micros = 420000
recipients = ["gpu"]

[[run]]
how = "clicked"
picture = "A"
tool = "fill"
run = "1795544413393-11"
seconds = 26.0
outcome = "completed"
charged_micros = 40000
recipients = ["fal"]

[[run]]
how = "named"
picture = "B"
tool = "remove_prompt"
run = "1795544439470-12"
seconds = 42.1
outcome = "completed"
charged_micros = 460000
recipients = ["gpu", "fal"]

[[run]]
how = "clicked"
picture = "B"
tool = "segment"
run = "1795544481575-21"
seconds = 14.1
outcome = "completed"
charged_micros = 420000
recipients = ["gpu"]

[[run]]
how = "clicked"
picture = "B"
tool = "fill"
run = "1795544495732-22"
seconds = 34.5
outcome = "completed"
charged_micros = 40000
recipients = ["fal"]

[[run]]
how = "named"
picture = "C"
tool = "remove_prompt"
run = "1795544530253-23"
seconds = 38.4
outcome = "completed"
charged_micros = 460000
recipients = ["gpu", "fal"]

[[run]]
how = "clicked"
picture = "C"
tool = "segment"
run = "1795544568664-32"
seconds = 11.9
outcome = "completed"
charged_micros = 420000
recipients = ["gpu"]

[[run]]
how = "clicked"
picture = "C"
tool = "fill"
run = "1795544580568-33"
seconds = 22.2
outcome = "completed"
charged_micros = 40000
recipients = ["fal"]
```

**Recipients.** Each run's record names where its data went:

- Each segment, whether its own run or a step of `remove_prompt`, went to the GPU host
  (`gpu`) with the classes `source_images` and `prompts`.
- Each fill went to fal (`fal`) with `source_images` and `derived_images`.

**Time.** The first run met a cold card and took 417.2 seconds in all. It completed, since
the front now sends its headers at once (#298). Warm, a named removal took 38 to 42 seconds;
a clicked one took 34 to 49, its two runs together.

## The ledger

```toml
story_micros = 2760000
warm_up_micros = 420000
cancel_attempt_unresolved_micros = 420000
ledger_micros = 3600000
```

The store counted 3.60 dollars:

- the story's six segment requests at their bound of 0.42 and its six fills at 0.04, 2.76;
- one warm-up segment before a cancel attempt, 0.42;
- the cancel attempt itself, 0.42, which is unresolved.

Each segment is charged at its bound, since a timeout does not prove the host stopped
billing (`SPEC.md` section 6). What Modal metered for the whole day was 0.56 dollars
(`docs/results/m6-remote-sam3.md`, "Spend").

## The results, by eye

- The balls were removed in all six.
- The clicked path leaves a faint red fringe at the ball's edge. `fill` paints the mask as
  `segment` found it, with no grow or feather, where a named removal grows and feathers its
  mask first. That is the carry-over row on a clicked removal, which milestone E
  owns; it is now seen live.

## The first attempt

On the host at 6f54442f, before #298, 7 of the 9 runs completed. The first two failed at
150.8 seconds each. The GPU cold start took about five minutes, and Modal's web endpoint
redirects a request whose answer has not begun within 150 seconds. The front then sent its
headers only once the card's side had checked the files, so the kind `remote` met the
redirect and refused it ("the front pointed the request elsewhere"). The third call took
129.3 seconds. That attempt led to #298, and the story above was run again from its merge.
