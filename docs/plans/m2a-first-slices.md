# Plan: the first slices of milestone 2 and checkpoint 1D

Draft 2, 2026-11-18, from `main` at the tag `m1c` (0516d5d). Two thin slices, built side
by side, each ending in a command the owner runs on real images. Not the plan of
milestone 2 or of 1D: each slice ends with a table of what is left for those plans.

## How the work is split

| Pull request | Slice | Builds on | Size |
|---|---|---|---|
| P0 | Shared | `main` | Small: one command, a day |
| S1 | Squint | P0 | Large: the sender, the kind, the wire, the picture sent |
| S2 | Squint | S1 | Large: spend, the record, the use case, the command |
| R1 | Rascal | P0 | Large: the action, the command, the cached check |
| R2 | Rascal | R1 | Large: four operations and the PNG |

Each slice is held to two pull requests, as the owner asked. Both reviews judged S1 and
R2 the largest; if either passes about 2,500 changed lines, its builder splits it at the
named seam (S1: the sender alone, then the kind; R2: the three operations, then `render`
and the export) and says so in the hand-back.

S and R run in parallel from P0, sharing only `crates/whizbang/src/main.rs` (a line
each), their own carry-over rows and boxes here; either merges `main` when the other lands.

**Review rule.** A separate reviewer reads a pull request only for four things: the
handling of a token or key, egress (what leaves the machine and to whom), processes or
threads left running, and what ships in the binary or the release. S1 (the key, egress)
and S2 (the key in the record, spend) each get one; R1 (the worker, the cached check)
gets one. P0 and R2 are checked by the orchestrator and merged on green CI, with
`just scan-range` over `base..head`. A "Done when" is a list of tests by name.

## P0. Pictures into the store

Both slices start from a picture file, and `whizbang store import` takes only a canonical
artifact file (ADR 0002). P0 adds `whizbang store import --picture <file> --store <dir>
[--create]`: a PNG or JPEG is decoded under the core's decode limits, encoded again as a
PNG from its pixels alone (so no metadata is kept), committed as an `Image` with the
role of a picture, and its identity printed.

- [x] Done when:
  `import::a_png_and_a_jpeg_are_committed_as_pictures_and_their_identities_printed`,
  `import::a_picture_over_the_decode_limits_is_refused_before_it_is_committed`,
  `import::the_committed_picture_holds_no_metadata_of_the_file`, and the reference
  (`just docs`) regenerated.

## Slice S: one squint verdict, on real images, from Anthropic

### Goal

`whizbang squint check source-fidelity --source <id> --candidate <id> --store <dir>
--config <file> --env-file <file>` (`SPEC.md` section 11: `squint check <use-case>`)
prints `pass`, `fail` or `unsure` with its reason, from one live call to Anthropic,
through the real sender behind the egress gate, metered against the store's ledger and
recorded as a run.

**Why source fidelity.** It is the use case the old squint used most, and the only one
with recorded evidence: the whole baseline package is source fidelity
(`docs/results/2026-11-tool-study.md`: the set is `source-fidelity/blind-v3`; 1,144
recorded decisions in `docs/baseline/audit.toml`). Its rules, prompt, Anthropic adapter
and recorded adapter fixtures (`fixtures/adapters/anthropic-messages/`) are at squint's
pinned commit, so the slice ports rather than invents. **The model:**
`claude-haiku-5-5`, the cheapest Anthropic model in the study (286 decisions for 0.94
USD, `docs/baseline/manifest.toml`).

### S1. The Anthropic judge, through the real sender (large)

1. **The real sender.** The N1 client (`whizbang_backends::net`) learns one `POST` with
   a body of known length, over the same `rustls` and narrow HTTP/1.1, one request per
   connection, no redirect followed. It is a `Sender` (`whizbang_core::egress::ways`)
   and the binary's executor gets it with `SendCheck::new().keeping_out(root)` over it,
   the root of its store (carry-over row "The real network client").
2. **The kind.** `kind = "service"`, `provider = "anthropic"`, a `model`, and
   `key_env`. The key goes only to that backend, in the `x-api-key` header, and to the
   recipient's origin only. Registered in `wiring::kinds` beside `pack`.
3. **The wire.** A port of squint's Anthropic Messages adapter
   (`crates/squint-providers/src/anthropic/`, by `port-module`): the core's
   `JudgeRequest` to the Messages body and back to a `Turn`, its usage and a typed
   fault. Each fault's sentence is fixed words and typed fields, never the provider's
   body, a header or an address (carry-over row on the fault's sentence), and its stage
   is `not_sent` only when no byte left the process (row on whether a failed call may
   have been charged).
4. **The picture sent.** Before the first picture leaves, it is prepared as squint
   prepared it (`crates/squint-providers/src/prepare.rs`): fitted to the model's long
   edge, then JPEG at quality 90, or PNG with alpha. The check behind a send learns that
   encoder's output, so the row on the uncompressed encoder (ADR 0019) is closed here.
   Metadata is removed (`SPEC.md` section 10).
5. **What is checked is what is sent.** The gate checks pieces (`egress/hops.rs`) and
   knows an image only at a piece's start, and the Messages body carries pictures as
   base64 inside JSON. So the body is built from the checked pieces: each prepared
   picture is a piece of its own, checked as an image, and the serializer writes the
   base64 of exactly that piece's bytes into the body; no picture is copied, decoded or
   encoded after its check. A request holds at most 64 pieces (`egress/ways.rs`); the
   kind's `max_pictures` (S2) keeps a conversation under it, and a request that would
   pass it is refused `not_sent`.

- [x] Done when:
  `net::sender::a_post_carries_its_body_once_and_follows_no_redirect`,
  `net::sender::a_request_that_never_connected_is_not_sent_and_one_cut_after_writing_is_unresolved`,
  `anthropic::conformance::each_recorded_exchange_of_squint_is_read_back_to_its_turn`
  (the adapter fixtures served by a recorded fake on a loopback listener with a test
  certificate, as N1's tests serve downloads),
  `anthropic::conformance::a_request_is_written_as_the_recorded_request_was`,
  `anthropic::a_fault_holds_nothing_of_the_body_the_headers_or_the_address`,
  `anthropic::the_key_goes_only_to_the_approved_origin_in_its_header`,
  `wired::a_judge_call_to_a_recipient_the_config_does_not_name_is_refused_before_a_byte_leaves`,
  `wired::a_redirect_to_another_origin_is_refused_and_the_key_is_not_sent_there`,
  `wired::the_binarys_sender_refuses_a_file_from_inside_the_store_root`,
  `prepare::a_picture_is_fitted_and_sent_as_jpeg_and_the_check_knows_it`,
  `anthropic::each_picture_in_the_sent_body_decodes_to_the_bytes_of_its_checked_piece`.

**Reworded with the task** (S1's second pull request, 2026-11-17). S1 was split at its
seam: the sender, its first pull request; the kind, the wire and the picture sent, its
second. The tests sit where the crate can reach a test root: `net::sender::…` and
`anthropic::…` are `net::tests::sender::…` and `anthropic::tests::…` inside
`whizbang-backends`, beside N1's `client::…`; `anthropic::the_key_goes_only_…` is in
`crates/whizbang/tests/it/anthropic.rs`, through an executor over the files; and
`prepare::…` is `egress::prepare::…` of the core, where the encoder is. One clause was
"`anthropic::conformance::a_request_is_written_as_the_recorded_request_was`", and is
now: that test, over the 26 of squint's 29 recorded requests that name no control a
`JudgeRequest` does not carry, compared as JSON with the recorded body less each tool's
`strict` (squint's catalog set it; a `JudgeRequest` has no place for it), and with each
picture compared by its block, its type and its size, not its bytes. Why: three
recordings name a forced tool choice (`final-turn-forced/02`) or a removal of tools
(`final-turn-tool-removal/02`, `03`), which a request of the core does not say; a
picture is held as pixels and written by whizbang's own encoder, so its bytes are not
squint's; and the 21 pictures of 2,000 by 1,500 of `many-images` are made a tenth the
size on each side in the test, since encoding them in a test build takes longer than a
test may run (`egress::prepare::…` holds the fitting). Every read-back of a turn is of
all 29.

### S2. Spend, the record, source fidelity and the command (large)

1. **Spend, with no change to the core's quote.** A `Price` is stated before any
   request exists (`capability/price.rs`), so the bound is fixed per request, from three
   settings of the kind, each enforced by the adapter before a byte is sent:
   `max_input_tokens` (default 40,000), `max_output_tokens` (default 2,048) and
   `max_pictures` (default 24). A request asking more output than the ceiling is sent
   with the ceiling, a retry's larger limit included; one whose counted input or
   pictures are over theirs is refused `not_sent`, and the loop ends with that fault.
   The input count is an upper bound by construction: one token for each byte of text,
   and for each picture the provider's published formula on its prepared size, rounded
   up, plus a fixed overhead per message. The bound is the two ceilings at one price row
   for the model (per million tokens, from the old provider catalogue, with the date it
   was read). Admission reserves the request bound times `check`'s 16 turns
   (`contract/prepare.rs`) against `call_cap_usd` (`spend/cap.rs`), so the defaults and
   the example's cap are chosen together, the arithmetic written in the ADR and held by a
   test. Each turn is settled from the reply's usage, at its bound when nothing settles
   it.
2. **The record.** The judge's exchanges are recorded as the core already records them
   (`capability/written.rs`: the normalised turns, pictures by digest), not the HTTP
   bodies; the run records where data went (the recipient and the classes). Headers are
   never recorded. Every secret the executor was told is redacted from the record, the
   diagnostics, the events and every fault.
3. **The use case.** `source-fidelity`: its rules and checklist and prompt ported from
   squint into `whizbang-squint`, the `check` action given a `use_case` argument, and
   the judge's submission read into a verdict with its reasons. The loop, its limits and
   the tools stay as `check.rs` has them (`view` is offered; `align` is not ported yet).
4. **The command.** `whizbang squint check source-fidelity --source <id> --candidate
   <id>` with `--store`, `--config`, `--env-file`, `--request-id`. It prints the verdict
   and its reason, and the run's identity; exit codes as `rascal grow` has them.
5. **Evidence, by hand, once.** The owner runs it on the Mac on one pair of real
   images (a source and an edit of it, the owner's own). The result goes in
   `docs/results/m2a-first-verdict.md` as fields: the commit, the model, the verdict, the
   tokens and the spend the ledger settled, the run's identity, and the egress record's
   recipient. No image, no item name, no reply text.

- [x] Done when:
  `spend::a_judge_turn_reserves_its_fixed_bound_and_settles_at_its_usage`,
  `spend::a_judge_turn_with_no_usage_is_charged_its_bound`,
  `spend::a_judge_call_whose_bound_is_over_the_call_cap_is_refused_before_dispatch`,
  `spend::a_retry_with_a_larger_limit_is_admitted_as_its_own_attempt`,
  `spend::the_example_configs_whole_call_reservation_fits_its_cap`,
  `anthropic::a_request_counted_over_its_input_or_picture_ceiling_is_refused_not_sent`,
  `anthropic::a_request_asking_more_output_than_the_ceiling_is_sent_with_the_ceiling`,
  `anthropic::the_input_count_is_at_least_the_usage_of_every_recorded_exchange`,
  `record::the_runs_record_diagnostics_and_faults_hold_no_byte_of_the_key`,
  `record::the_run_records_the_recipient_and_the_classes_sent`,
  `squint::check::source_fidelity_reaches_a_verdict_from_a_scripted_judge`,
  `squint::check::a_submission_that_is_not_a_verdict_is_reported_as_invalid`,
  `cli::squint_check_prints_the_verdict_and_the_run_under_a_recorded_fake`,
  `cli::squint_check_without_the_env_file_is_refused_naming_the_variable`,
  and the evidence file, checked by
  `evidence::the_first_verdict_record_names_a_commit_of_this_tree_and_holds_no_secret`.

**Reworded with the task** (S2, 2026-11-18). The tests sit where their crate reaches what
they need: `spend::…` are `spend::judge::…` of the binary's tests; `anthropic::…` are
`anthropic::tests::…` and `anthropic::tests::conformance::…` inside `whizbang-backends`,
beside S1's; `squint::check::…` are `check::…` of `whizbang-squint`'s tests; and
`cli::squint_check_prints_…` drives the binary's door in process (`door::check`), as
R1's `cli::` tests do, since the binary that ships cannot trust a test's root. Item 5
said "the owner runs it on the Mac on one pair of real images (a source and an edit of
it, the owner's own)", and is now: the builder ran it once on the owner's Mac, with the
owner's env file, on a synthetic source and an edit of it made for the call, as the
orchestrator directed on 2026-11-18 (no personal photograph). Why: the orchestrator's
brief for S2 gave the call to the builder, with pictures it makes itself.

### What the owner provides (S)

Exactly this, once, before S2's live call:

- **The env file: provided on 2026-11-18,** as `ANTHROPIC_API_KEY`, in the owner's
  whizbang config folder (mode 600, outside every repo; its path is in the builder's
  brief, never in a tracked file). The live call passes it as `--env-file`. Only
  whizbang reads it: no agent opens, prints or copies it, and CI never uses it.
- **The config file,** passed as `--config` (also put in `docs/guides/config.md`):

```toml
[backends.judge]
kind = "service"
provider = "anthropic"
model = "claude-haiku-5-5"
key_env = "ANTHROPIC_API_KEY"

[spend]
call_cap_usd = 2.00

[egress.recipients.anthropic]
origin = "https://api.anthropic.com"
classes = ["source_images", "derived_images", "prompts", "evidence"]
```

- **The allowance:** `whizbang spend reset --allowance 10 --store <dir>`; and **two
  images** of the owner's own, a source and an edit of it.

### Origins (S)

`https://api.anthropic.com`, for judging, with the classes above. `SPEC.md` section 10
names Anthropic for judging. **Approved by the owner on 2026-11-18,** told that the slice
sends test images and squint's questions there; no further ask. No upstream, no probe,
no other origin. CI contacts nothing: the recorded fake runs on loopback.

### Risks (S)

1. **The provider's picture formula changes.** The input count is an upper bound only
   while the published formula holds. The conformance test holds the count to at least
   the usage of every recorded exchange, and the evidence sets the live call's reported
   usage beside the count.
2. **The fixtures' age.** Squint's recorded exchanges may predate a field the live API
   sends now; unknown fields are read as opaque, and the live call proves the wire.
3. **The ceilings cut a long loop.** A conversation of many `view` turns can pass
   40,000 input tokens or 24 pictures and end without a verdict. The owner raises the
   settings, and the cap with them.

### What milestone 2 still owes after S

| Item (`SPEC.md` section 7 or 16) | Left for |
|---|---|
| Google and OpenAI judge kinds, each with provider conformance | Milestone 2 |
| Reference consistency, reality checks, brief compliance, product fidelity | Milestone 2 |
| Stdio MCP door; files verified again in a long-lived process (carry-over row on D14); the decision cache (`SPEC.md` section 9), which nothing in S reads or writes | Milestone 2 |
| Tools `resize`, `align`, `change_map`, `edge_similarity`; trying a tool without a rebuild | Milestone 2 |
| The baseline gate, and the transcript format choice of the carry-over row on scaled tool images | Milestone 2 |
| Evaluation sets, matrices, gates; `runs stats` and `runs export` over 100 runs; per-subject comparison | Milestone 2 |
| The ledger's cost per paid call, request ids in volume, the reference page's needs, two needs of one capability (carry-over rows); a quote that sees the request | Milestone 2 |

## Slice R: segment a picture with a phrase, and cut it out

### Goal

`whizbang rascal segment --image <id> --phrase <text> --store <dir> --config <file>
--models-dir <dir>` runs the `segment` action (`SPEC.md` section 7, "`segment`, as an
action over the segment capability") on the kind `pack` and SAM 3, writes one `Mask`
per instance to the store and prints their identities and scores. Then a handful of
operations make a usable cut-out, and one command writes a PNG the owner can open. All
local; nothing is paid and nothing leaves the machine.

**Which operations, and why.** Rascal's operations at its pinned commit
(`crates/rascal-ops/src/`, read under the clean-room rule of `AGENTS.md`), chosen for
the path from instances to a clean cut-out:

| Operation | Why |
|---|---|
| `select` | SAM 3 answers several instances for a phrase; a cut-out needs the chosen ones as one mask (rascal: the union of the kept instance planes, minus any exclude boxes) |
| `plug` | Masks from a segmenter have small holes; `plug` fills a hole up to a share of the blob that owns it, which a cut-out needs and `grow` does not give without spreading the edge |
| `feather` | A hard edge looks cut with scissors; `feather` sets the blend from the core's boundary with one sigma, protect kept |
| `render` | The way a mask becomes a picture (rascal: a rendering, typed as an `Image`): `alpha` (the image with the mask as its alpha channel: the cut-out) and `plain` (the mask in greyscale) are the two profiles ported; the others wait for milestone 3 |

`grow` is built; `close`, `shrink`, `subtract` and the rest wait for milestone 3.

### R1. The `segment` action and its command (large)

1. **The action.** `segment` in `whizbang-rascal`: an image and one phrase in, one
   `Mask` per instance out, with `max_instances` and the backend's choice as arguments;
   the core's `SegmentRequest` sent through the executor to the configured segmenter.
   A run's output holds an artifact's identity only (`contract/record.rs`) and a `Mask`
   holds no score or box, so each instance's score and box go in the run's diagnostics,
   as text the action writes, which `whizbang runs show` prints and the command prints
   beside each mask. The kind `pack` and SAM 3 are as 1C left them.
2. **The command.** `whizbang rascal segment` as above, with `--request-id`, exit codes
   as `grow`.
3. **The cached check (carry-over row on the 26-second check).** The pack keeps, beside
   an installation, a record of its last full check: each file's size, its modification
   time and its inode, and the hashes found. A later process that finds every file
   unchanged against that record skips the full hash; any change hashes again. The
   record is written only after a full check passes, atomically, owner-only.
4. **The evidence taken again on a clean head (carry-over row on the 1C record).**
   `scripts/model-evidence.sh` refuses a tree with changes, and is run on R1's head once
   it is committed; the record shows the first load's time without the full hash.

- [x] Done when:
  `segment::a_phrase_answered_with_three_instances_writes_three_masks`,
  `segment::no_instance_is_an_outcome_and_writes_no_mask`,
  `segment::a_backend_lacking_phrase_is_refused_at_prepare`,
  `cli::rascal_segment_prints_each_masks_identity_and_score_under_a_fake_segmenter`,
  `models::an_unchanged_installation_checked_before_is_loaded_without_a_full_hash`,
  `models::a_file_changed_since_the_check_is_hashed_again_and_refused`,
  `models::a_check_record_that_is_damaged_is_ignored_and_the_full_hash_runs`,
  `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`
  passing on a record taken by the script on a clean head, and `just test-models` green
  on the Mac with the record's first load under 10 s.

### R2. Three operations and the PNG (large)

1. **`select`, `plug`, `feather`,** each an action over `Mask`, ported with the tests
   that guard their behaviour (`port-module`), each a `whizbang rascal <op>` command as
   `grow` is.
2. **`render`** with the profiles `alpha` and `plain`, an action that writes an `Image`
   with the role of a rendering.
3. **The PNG out.** `whizbang store export --image <id> --out <file>` decodes the
   image and writes its pixels as a PNG, whatever form it is held in, refusing to
   overwrite and refusing a path inside the store.
4. **Evidence, by hand.** The owner runs segment, select, plug, feather, render and
   export on one picture of their own and opens the PNG. A line in
   `docs/results/m2a-first-cutout.md`: the commit, the commands, the times, no image.
5. **The tool records are owed.** `SPEC.md` section 8: "A tool is not finished ...
   without its record." `segment`, `select`, `plug`, `feather` and `render` ship for the
   owner to try, unfinished: a carry-over row gives each its record at
   `docs/reference/tools/<name>.md` (as `grow` has) in milestone 3, where section 7
   places the operations. Section 7 places mask export in milestone 3 too; this PNG
   export is a viewer's, not the mask format's.

- [x] Done when:
  `select::the_kept_instances_union_minus_an_exclude_box_is_the_core`,
  `plug::a_hole_up_to_its_share_of_its_owner_is_filled_and_a_larger_one_is_not`,
  `plug::a_hole_touching_the_edge_is_not_filled`,
  `feather::the_blend_falls_off_outward_with_sigma_and_protect_wins`,
  `render::alpha_is_the_image_with_the_masks_weight_as_alpha`,
  `render::plain_is_the_masks_weight_in_grey`,
  `cli::store_export_writes_the_png_and_refuses_to_overwrite_or_write_inside_the_store`,
  `cli::store_export_writes_a_png_for_an_image_held_as_jpeg`,
  `just docs` regenerated with the four actions, and the carry-over row for the five
  records.

### What the owner provides (R)

Nothing new: SAM 3, the pack's runtime and 1C's config and models directory are reused.
For the evidence: one picture of the owner's own, and a phrase. **Origins (R): none.**

### Risks (R)

1. **The cached check trusts metadata.** A file changed with its size, time and inode
   kept would be loaded unchecked. The ADR states it, and the record lives in the
   owner-only models directory; the full check stays a flag away
   (`whizbang models verify <model>`, added in R1 if cheap, otherwise a carry-over row).
2. **Memory on a real picture.** A low threshold on a large image yields many
   full-size planes. 1C's adapter cuts candidates to `max_instances` before resizing;
   R1's command defaults `max_instances` to 8 and the model tier runs under
   `scripts/memory-watch.sh`.
3. **Porting the operations' exact pixels.** `plug` and `feather` have tie-breaking and
   rounding rules; the ported tests carry rascal's expected planes, and a difference is
   a decision written in the task's ADR, not a silent change.

### What 1D still owes after R

| Item (`SPEC.md` section 16, row 1D, and carry-over rows) | Left for |
|---|---|
| Point and box requests on SAM 3; Tiny (OWLv2 with SAM 2.1 Tiny) with its conformance | 1D |
| Parity fixtures and a mask-agreement tolerance; timings of point, box and Tiny | 1D |
| The runtime on Linux (the owner's licence decision first) | 1D |
| The `add-adapter` skill; a user's weights and a user's adapter | 1D |
| A backend's diagnostics and predicted IoU reaching its run (carry-over row on what an adapter gave) | 1D |
| Two installs sharing a file download it once | 1D, with the second entry |
| The tool records of `segment`, `select`, `plug`, `feather`, `render` | Milestone 3 (carry-over row) |
| The Linux CI job over 10 minutes | 1D's entry check |

## Review rounds

Two outside rounds on draft 1, kept as counts: the first had 7 findings (6 on what was
not buildable or a gap at the gate, 1 on size), the second 4 (3 on the spend bound and
the gate's piece limit, 1 on size). All are taken into draft 2; the size findings by the
split seams under "How the work is split".
