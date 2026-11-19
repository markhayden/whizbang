# Plan: milestone 3

Draft 6, 2026-11-18, from `main` at 57eeec2 (the merge of F1's second pull request, #198, and
the note of milestone 2, `docs/plans/m2-note.md`). Milestone 3 is `SPEC.md` section 16, row 3,
"Rascal and edit evaluation": "Each task and operation in section 7 passes its acceptance
case, and each model-based recipe has its ladder record with rungs 1 and 2 scored or
credited from earlier evidence, with SAM 3 for masks and Bria through fal for fills; a mask
exports and imports; rascal's tasks and squint's use cases run as recipes on the core's
runner, and `describe` lists recipes before primitives; a lab round runs a recipe and filler
matrix, resumes after an interruption, presents blinded pairs, collects the picks, checks
them against the frozen assignment and decodes the outcomes". Beside the row it takes every
carry-over row milestone 2 left to milestone 3 (the table "Carry-over rows owned by
milestone 3", below), the live record of the two new judges, and the third of the
evaluation's verdicts that are `unsure`. It ends with the note of milestone 3.

**What the owner asked for, and the order it sets.** The owner tries what lands on their own
pictures, on the Mac, so the work is ordered for real things early: first the recipe runner
with `remove_prompt`'s mask on SAM 3 (R1) and fills through Bria with the `fill` and `paste`
actions (F1), which together remove what a phrase names from a picture of the owner's; in
the same wave the gates (G1) and the attack on `unsure` (U1), measured with E1's matrices
and decided by E2's gate; and, before any builder starts, the live record of the Google and
OpenAI judges (L0), run by the orchestrator. Then the rest of rascal's operations and tasks
(O1 to O5) and squint's use cases as recipes (Q1); then the lab (L1 to L3) and the records of
the operations (D1, D2); then the ladder records of the edits, the rows left and the note (B1).

## How the work is split

Sizes are budgets from the sources at the tag: rascal's lines of each module ported, times
2.1 for the port with its tests (plan 2A's task R2 ported `select`, `plug`, `feather` and
`render`, 706 lines of rascal, in 1,512 lines of whizbang's source and tests), plus about 100
lines an action for its descriptor, command and wiring. A tool record is about 100 lines
written by hand beside what `scripts/tool-record.sh` generates, so the records of the
operations and of the tasks that use no model are two pull requests of their own (D1, D2), and an
operation's pull request carries its vectors and acceptance tests, not its record.

| Pull request | What | Builds on | Size |
|---|---|---|---|
| L0 | The live record of `gemini-3.8-flash` and `gpt-6.1-terra`: the orchestrator's, no code | `main` | Small: about 60 changed lines |
| R1 | Recipes: the format and the runner in the core; `close`; `subtract` of a mask; `remove_prompt`'s recipe | `main` | Large: about 2,500 (runner 1,250, ops 450, recipe and command 300, tests 500) |
| F1 | Fills through fal (Bria Eraser); the `fill` and `paste` actions; masks exported and imported | `main` | Large: about 2,500 (adapter 700, `fill` and `paste` 650, export and import 500, tests 650) |
| G1 | Gates run by `eval run --gate`; the baseline's live rerun; the gate of details first; squint's wire for Google and OpenAI | `main` | Large: about 2,000 |
| U1 | The attack on `unsure`: its causes counted, `tiles`, the second look, the screen and its gate | `main`; its gate run after G1 | Large: about 2,300 |
| O1 | `line`, `rect`, `stroke`, `rasterize`, `shrink`, `invert`, `subtract` of a location; the tasks `remove`, `repair_area`, `mask_rect`, `protect` | R1, F1 | Large: about 2,450 (rascal 863 lines; no records) |
| O2 | `cut`, `put`, `window`, `unwindow`, `apply`; a fill put back through a window or a cut | R1, F1 | Large: about 2,550 (rascal 918 lines, and the heal's 300) |
| Q1 | Squint's use cases as recipes, their records, and what an evidence tool hands a later step | R1, U1 | Large: about 2,500 |
| O3 | `fan`, `profile`, `ramp`; the task `seam` | O1, O2 | Large: about 2,500 (rascal 1,119 lines), the constant sets counted apart |
| O4 | `tighten` and the task `repair`; `inspect`; `paste`'s modes `hard` and `poisson` | O1, O2 | Large: about 2,400 (rascal 1,131 lines; `poisson` its seam) |
| O5 | `measure`; pictures from callers; large results' encoding | O3, O4 | Large: about 2,250 (rascal 639 lines, and the rows' 900) |
| L1 | The shared harness (E1's runner made to run cells of any action) and the lab's corpus, protocol and matrix, with text bindings and a reference arm | O3, F1 | Large: about 2,400 (its budget below) |
| L2 | `lab run` and its resume, and the decode | L1 | Large: about 2,200 |
| L3 | Blinded pairs and picks, the check against the frozen assignment, and a round of `seam-03` | L2 | Large: about 2,300 |
| D1 | Fourteen records: R2's five operations, `close`, `subtract`, `line`, `rect`, `stroke`, `paste`, `fill`, `mask_rect`, `protect` | O1, F1 | Large: about 2,400 (14 records of about 100 hand-written lines, and their hooks in `scripts/tool-record.sh` and tests, about 70 each; the generated images and numbers counted apart) |
| D2 | Fourteen records: `rasterize`, `shrink`, `invert`, `cut`, `put`, `window`, `unwindow`, `apply`, `fan`, `profile`, `ramp`, `tighten`, `inspect`, `measure` | O5, D1 | Large: about 2,400 (as D1) |
| B1 | The rung 2 of the edits, the ladder records of the fill recipes, the rows left, and the note | all of the above | Large: about 2,300 |

**Sixteen builders' pull requests, where the owner asked for about eight.** Milestone 3 is
about 37,500 changed lines by the budgets above: nineteen of rascal's operations still to
port and `paste` registered as an action (about 10,500), 28 records of operations and tasks
without a model (4,800), the seven tasks, the runner, fills, gates, the attack on `unsure`,
the use cases as recipes, the lab and the edits' records. At about 2,500 a pull request that
is sixteen; eight would need about 4,700 lines each. The owner's two standing limits cannot
both hold for this scope, so the choice is the owner's (first item under "What the owner must
answer"): sixteen in this plan, or this plan cut at the first eight (R1, F1, G1, U1, O1, O2,
Q1, O3, ending with a note of that part) and a second plan, reviewed as this one is, for the
other eight. Wave 1 is the same
under either answer, so it starts at once; wave 2 and after wait for the answer. L0 is the
orchestrator's record of one results file and a plan line, and is not counted.

**Waves.** Seven, each started from `main` once what it builds on has landed. L0 lands before
the first wave starts.

1. **R1, F1, G1 and U1** from `main`. U1 builds its changes and runs its screen on E1's
   runner; it merges `main` once G1 has landed and only then freezes and runs its gate.
2. **O1 and O2** when R1 and F1 have landed; **Q1** when R1 and U1 have landed.
3. **O3** when O1 and O2 have landed; **O4** when O1 and O2 have landed (it adds sets to
   O2's digest list); **D1** when O1 and F1 have landed.
4. **O5** when O3 and O4 have landed (`measure` reads `fan`'s windows; its test that all 25
   operations are actions needs O4's); **L1** when O3 and F1 have landed.
5. **D2** when O5 and D1 have landed; **L2** when L1 has landed.
6. **L3** when L2 has landed.
7. **B1** when the other fifteen have landed.

**Shared files.** Every operation's pull request touches the registry, the command and the
reference. To keep that to a line each: each operation's module holds its own descriptor,
its own `register` function and its own clap subcommand struct, and the shared files gain
one line each: `crates/whizbang-rascal/src/lib.rs` (the module),
`crates/whizbang/src/wiring.rs` (the `register` call), `crates/whizbang/src/rascal.rs` (the
subcommand's variant). Tests in `crates/whizbang/tests/it/cli.rs` are functions of each pull
request's own. R1 makes this layout for `close` and `subtract`, and moves R2's four operations
to it in the same commit. The generated reference (`docs/reference/`) is made again by `just
docs` after merging `main`, never merged by hand. In each wave the second to land merges
`main` (a merge commit), runs `just docs` again, and reruns its checks. Beyond those: G1 owns
`crates/whizbang-eval/src/gate.rs` and `eval run`; U1 owns the arm settings in
`crates/whizbang-eval/src/matrix.rs` and `crates/whizbang-squint/src/details.rs`; G1 and U1
both touch `crates/whizbang-squint/src/check.rs` only if G1's wire item needs it, and then G1
lands first. O1 owns `crates/whizbang-core/src/artifact/location.rs`; O2 owns the mappings in
`crates/whizbang-core/src/identity/`. Q1 owns `crates/whizbang-core/src/contract/descriptor.rs`'s
result kinds.

**Copies are counted apart.** Rascal's constant sets (`crates/rascal-ops/constants/*.toml`,
eleven files, about 400 lines) are copied byte for byte by the pull request that ports the
operation reading each, held by `constants::every_set_rascal_shipped_is_here_byte_for_byte`
against a committed list of their digests (written by O2 with `window-v1`, each later pull
request adding its sets' lines), and counted apart in the hand-back. Rascal's `seam-03`
corpus manifest (L3, about 330 lines, digests and ids only) are counted
apart the same way. The generated reference (`docs/reference/description.json` and
`actions.md`, about 55 lines an action) is counted apart as `just docs` writes it. The owner
is told of these three here; no other pull request has an exception.

**Seams.** Each large pull request names where it splits if it passes about 2,500 changed
lines, into a second pull request of its own that lands before anything that builds on it:
R1, the user's own recipe files (`[recipes] paths`, its item 6); F1, masks exported and
imported (its item 7); G1, the wire for Google and OpenAI (its item 4); U1, the second look
(its item 4, with its arms in the screen); O1, `rect` and `stroke` with `mask_rect`; O2, the
fill put back (its item 3); Q1, the squint recipes' records (its item 5); O3, `ramp`, with
the seam task waiting on it; O4, `poisson`, which the carry-over row allows dropping with a
line, since no lab round of rascal's used it (`docs/results/poisson-bench.md` at the tag is
a benchmark); O5, large results' encoding (its item 3); L1, `lab corpus verify` (with its test, to L2, which
lands before anything that reads it); L2, the forecast of a round; L3, the round itself (its item 4); D1, the records of R2's five
operations; D2, `inspect`'s and `measure`'s records; B1, the rows left (its item 4). A split is said in the hand-back, and the owner is told the
count. Nothing of the row is dropped to keep the count.

**Review rule.** As in milestone 2: a separate reviewer reads a pull request only for the
handling of a token or key, egress (what leaves the machine, to whom, and in what form),
processes or threads left running, a user's own code or files that whizbang runs or reads,
and what ships in the binary or the release. F1 (a new key and a new recipient) gets one. R1
(a user's recipe file read from config; nested calls under one reservation) gets one. G1 (the
wire's transformation of what is sent to Google and OpenAI) and U1 (new crops and tiles sent
to every judge) each get one for what they send, the rest checked by the orchestrator. O5
(a recipient matched with `upstreams`, and how a caller's picture and its metadata are read)
gets one for those. L2 (the lab's fills of the owner's corpus) and L3 (the pairs page, a file
for a browser) each get one. L1, O1 to O4, Q1, D1 and D2 are checked by the orchestrator. Each is merged on green CI, with `just
scan-range` over `base..head`. A "Done when" is a list of tests by name.

**Test time.** The hosted macOS runner's tests have taken 60 to 260 s of their 300 over the
last pushes of milestone 2; the time is noisy, and the plan does not lean on the low end.
Milestone 3 may add at most 40 s there, about 3 s a pull request. Every test that calls a
provider, a model or the network beyond loopback is `#[ignore]`d and run by hand or by `just
test-models` on the Mac, never by CI. Pictures in tests are synthetic and at most 256 pixels
on a side; fillers and judges are scripted or served by a recorded fake on a loopback
listener; a recipe in a test runs over the fake segmenter and a scripted filler; sets and
corpora in tests are three to six items made in the test. Each pull request reports its
`just check` and CI test times, says in its hand-back how it kept them lean, and stops and
says so if the hosted macOS runner passes about 270 s, or Linux or a local run about 150 s of
its 180 s.

**Live runs are the orchestrator's.** In milestone 2 a builder's live call on the new judges
was refused by the permission system (plan 2C, P1's status). So every live run this plan
asks for (a judge, a fill, a matrix, a gate, a lab round) is made by the orchestrator on the
owner's Mac, from the command the builder hands back with the branch's head, and the record
is committed to the branch by the orchestrator or by the builder from what the orchestrator
gives it. A builder makes no live call. Runs on the owner's baseline package are the
owner's: no agent reads it.

**Spend.** The owner's standing allowance is 100 US dollars a milestone without asking;
milestone 3 has spent nothing. The live runs this plan asks for come to about 60 dollars,
itemised under "Spend"; each pull request that spends records what its ledger settled in its
results file, and stops and says so if its own spend passes twice its estimate.

## L0. The live record of the two new judges (small; the orchestrator's)

Plan 2C, P1, item 7, re-owned to milestone 3 (carry-over row on P1's live evidence): each of
the five use cases asked once of `gemini-3.8-flash` and once of `gpt-6.1-terra`, on A1's
synthetic pictures, with the owner's env file, by the commands A1's record names. The
recipients were approved by the owner on 2026-11-18 (plan 2C, "Origins (P)"). The fields go
in `docs/results/m2c-use-cases.md` beside Anthropic's: the commit, the use case, the model,
the verdict, the tokens and the spend settled, the run, the recipient. No reply text. A
refusal by a provider, or an answer a schema does not read, is recorded as it came and is
G1's (item 4), not fixed here.

- [ ] Done when:
  `evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret` passes on
  the record with both judges' lines; P1's last clause in `docs/plans/m2c-milestone-2.md`
  ticked with a line naming this pull request; the carry-over row on P1's live evidence
  closed.

## R1. Recipes: the format and the runner in the core (large)

### Goal

A recipe is a named, typed sequence of steps over primitives and other recipes, run by one
runner in the core with values passed between steps and one budget (`SPEC.md` section 3,
"Primitives, recipes and loops"); `whizbang describe` and the server list recipes first; and
the owner can make `remove_prompt`'s mask of a picture of their own on SAM 3:

```
whizbang rascal remove-prompt --image <id> --prompt "leather collar" [--until <step>] [--set <step>.<param>=<value> …]
whizbang rascal run <recipe-file> --input <name>=<id|text> …
```

1. **The format, `recipe/1`** (rascal's `crates/rascal-core/src/recipe.rs` and
   `docs/recipes.md`, by `port-module`, at the commit `docs/sources.toml` pins), in a new
   ADR: a name, what it is for and when to use it (row on what a manifest and a task
   description say), typed inputs (any artifact type, or text; optional ones), steps (an id,
   an action, input bindings `inputs.<name>` or `<step>.<output>`, parameters, a constant
   set), and typed outputs. A recipe is checked when it is read: every binding reaches a
   step before it or an input, every type matches the action's descriptor, every parameter
   is the action's, no step id repeats. Rascal's dependent-finder rule is kept. An optional
   input left out is bound as absent, and only to an optional port; an action given an absent
   optional input does as rascal's did (`subtract` with no `protect` hands its mask back
   unchanged), so every step runs and every binding has an output.
2. **The runner, in the core.** A recipe is an action of the kind `Recipe` (ADR 0003); each
   step is a nested call through the executor (ADR 0008), drawing on the recipe run's one
   reservation, so nothing is reserved twice (`SPEC.md` section 6); a cancelled recipe
   cancels the step that runs and starts no other. Each step's run is a child of the
   recipe's, and the recipe's record lists its steps in order with their arguments, the
   constant set and each result's reference (row on the run record's lists: a new record
   version by ADR 0005, with its schema and fixed vectors). A child's names are resolved
   once for each child that commits (row on a parent reading a child's names). The limit of
   256 calls a run (row on `STEPS_LIMIT`) is measured at 1,000 nested calls of a fake action
   and raised to what that measurement allows, or kept with a line; the generation's cost
   (row on every publication writing the whole generation) is measured with it.
3. **Overrides and a partial run.** `--set <step>.<param>=<value>` and `--set
   <step>.constants=<set>` change one step's parameter for one run, refused naming the step
   when the recipe has no such step or parameter, and recorded in the run (row on step
   overrides). `--until <step>` runs the recipe up to and including that step and hands back
   that step's outputs: how a task whose recipe ends in `fill` gives its mask alone.
4. **`close`, and `subtract` of a mask** (`crates/rascal-ops/src/close.rs`, `subtract.rs`,
   and the erosion of `refine.rs` and `primitives.rs`, in whole numbers where `grow` showed
   the way: row on what the port of `grow` left), each an action with its descriptor and its
   record by `scripts/tool-record.sh`. `subtract` takes a mask here; its location form needs
   `rasterize::location_core` and is O1's.
5. **`remove_prompt`'s recipe** (rascal's `crates/rascal-mask/recipes/remove-prompt.toml`):
   `segment`, `select`, `grow`, `close`, `plug`, `subtract` (of `protect`, absent or given),
   `feather`, shipped in `crates/whizbang-rascal/recipes/` and addressable by name. The
   segmenter's answer is read in a fixed order, decided here (row on the order of a
   segmenter's answer).
6. **A user's own recipe.** Files listed under `[recipes] paths` in config, read and checked
   as a shipped one is, each run pinning its file's digest; a recipe file names only actions
   of the build and holds no code, so it runs nothing the build does not.
7. **`describe` and the server.** Recipes are listed first, each with what it is for and when
   to use it (description version 3 already orders by kind); `whizbang serve` offers each
   recipe as a tool with its schema.
8. **The executor built with a digest alone** (row on `Engine::over`): compiled for tests
   alone, or backed by one walk, as the row says.

- [ ] Done when:
  `recipe::a_recipe_whose_binding_reaches_no_earlier_step_is_refused_when_read`,
  `recipe::a_parameter_the_action_does_not_take_is_refused_naming_the_step`,
  `recipe::the_ported_recipes_of_rascal_read_as_rascal_read_them`,
  `runner::each_step_is_a_child_run_on_the_recipes_one_reservation`,
  `runner::a_cancelled_recipe_cancels_its_running_step_and_starts_no_other`,
  `runner::values_pass_between_steps_by_reference`,
  `runner::an_absent_optional_input_binds_only_to_an_optional_port`,
  `subtract::with_no_mask_to_subtract_the_mask_is_handed_back_unchanged`,
  `task::remove_prompt_completes_with_and_without_protect`,
  `record::a_recipe_run_lists_its_steps_in_order_with_arguments_constants_and_results`,
  `record::the_new_record_version_reads_a_record_of_the_version_before`,
  `context::a_childs_names_are_resolved_once_and_a_look_reads_no_file`,
  `store::volume::a_run_of_many_nested_calls_publishes_within_the_measured_bound`
  (`#[ignore]`d at 1,000 calls, its numbers in the ADR, with a run of 300 in CI),
  `runner::an_override_naming_no_step_of_the_recipe_is_refused_before_anything_runs`,
  `runner::until_a_step_hands_back_that_steps_outputs`,
  `close::the_ported_vectors_give_rascals_masks`,
  `subtract::the_ported_vectors_of_a_mask_give_rascals_masks`,
  `segment::instances_are_read_in_the_order_the_interface_fixes`,
  `recipe::a_users_recipe_from_config_runs_and_its_digest_is_in_the_run`,
  `describe::recipes_are_listed_before_primitives_with_purpose_and_use`,
  `serve::initialize_lists_each_recipe_before_the_primitives`,
  `cli::rascal_remove_prompt_makes_a_mask_under_the_fake_segmenter`,
  `model_tier::remove_prompt_makes_a_mask_with_sam3_on_the_mac` (`#[ignore]`, under `just
  test-models`), `just docs` regenerated, and the carry-over rows above closed or narrowed
  with a line. The records of `close` and `subtract` are D1's.

**Keeping tests lean (R1).** Recipes of two to seven steps over the fake segmenter, pictures
of 64 by 64; the volume measurement `#[ignore]`d and run once.

### Origins (R)

None: SAM 3 as installed, on the Mac.

## F1. Fills through fal; `fill` and `paste`; masks in and out (large)

### Goal

`[backends.fill]` names Bria Eraser on fal (`SPEC.md` section 5: fill, paid service,
milestone 3); `whizbang rascal fill --image <id> --mask <id>` hands back the filled picture
and the mask rebound onto it; `paste` is an action of its own; with R1, `whizbang rascal
remove-prompt` hands the owner a picture with the phrase's subject removed. A mask leaves a
store as a file and comes into another with the same identity.

1. **The kind.** `kind = "service"`, `provider = "fal"`, `model = "bria-eraser"`, `key_env =
   "FAL_KEY"`. A request is one `POST` to `https://fal.run/fal-ai/bria/eraser` with the
   picture and the mask as base64 data URIs in `image_url` and `mask_url`, and `sync_mode =
   true`, so the reply carries the picture as a data URI and nothing is fetched from fal's
   media hosts; a reply holding any other URL is a fault, and nothing is downloaded. The
   endpoint, the fields and `sync_mode` were read from fal's model page on 2026-11-18; the
   builder reads them again, and the header's form from fal's API reference, before writing
   the wire.
2. **The key.** Only to `https://fal.run`, in the `Authorization` header in the form fal's
   reference gives (`Key <key>`), never in a URL, a record, a diagnostic or a fault; a fault
   holds nothing of the body, the headers or the address, and its stage is `not_sent` only
   when no byte left.
3. **Spend.** Bria Eraser is priced per call: 0.04 dollars a generation on fal's model page,
   read 2026-11-18. A price per call is a fixed maximum charge (`SPEC.md` section 6), so the
   bound of a request is that price, recorded with its date in the catalogue's price row, ADR
   0010 amended for a fill priced per call; a call is settled at its bound, since the reply
   states no usage. A stated price for one request is enough, and the row on image
   preparation and a quote says so in a line.
4. **The picture sent** (rows on what a filler was sent beside an image and one plane, and on
   image preparation and its version): the backend makes what its model wants over the whole
   support, a PNG of the picture and a binary mask, scaled inside itself to the model's
   largest side when the picture is larger (the limit read from fal's page by the builder,
   or 2,048 as rascal's hosted example had it), and returns pixels of the request's size;
   the preparation and its version are in the run. A fill is checked by the core's egress
   gate as any image is (ADR 0019).
5. **`fill` and `paste`.** `paste` (rascal's `paste.rs`, its `feather` mode, which is
   whizbang's `composite.rs`) is registered as an action: a base image, a fill of its size and
   a mask go in, the composite comes out; `mode` is an argument whose one value here is
   `feather` (O4 adds `hard` and `poisson`). `fill` takes an image and a mask, asks the fill
   capability, and pastes the answer by `paste`'s code: the composite and the mask rebound
   onto it come out, made by `Derivation::checked` (row on a mask rebound only where a
   composite was made); every pixel outside the support and every protected pixel is the
   original's. **What is handed back is held** (row on what an action hands back): the
   executor holds every output to each relation its port states, before publication: that a
   mask is on the image its port names (`OutputPort::on`), that an image is derived from the
   one its port names (`derived_from`), that a mask was rebound onto the image it names
   (`rebound_onto`), that its origin names the action that handed it back, and that it is
   inside the executor's decode limits. Each refused with a test; none is left trusted. **An
   unchanged result** (a fill whose answer is the picture it was sent, an `apply` with no
   prescription or one of no effect) has no derivation, since `Derivation::checked` refuses
   identical pictures: the image handed back is the input's identity, its mask keeps the
   binding it had, and the check of `rebound_onto` and `derived_from` accepts an output whose
   image is the input's identity, as `composite.rs` does today. The encoder that writes a composite as an
   image artifact moves into the core, pinned once (row on the encoder in `whizbang-squint`).
6. **The tasks end in `fill`.** Landing after R1, F1 adds the step `fill` to
   `remove_prompt`'s recipe, with the filler resolved as any capability is; `--until
   feather` gives the mask alone. Each later task's recipe that fills ends in `fill` the same
   way.
7. **Masks out and in** (rows on a mask leaving a store only as its object file, on the file
   limit of `store import`, on opening a file without waiting, on `.` and `..` as names, and
   on the normal form of a folder's name on macOS): `whizbang store export <id> --to <file>`
   and the actions `export` and `import` write and read an artifact's canonical file; a file
   imported into another store has the same identity; the limit on a file follows from the
   bounds on what it holds, decided with the row on a mask's planes in memory or kept with a
   line; a FIFO at the path is refused without waiting; the export names its file by no store
   name as given.
8. **Evidence, by the orchestrator, once.** On the owner's yes to the recipient (second item
   under "What the owner must answer"): one `fill` of a synthetic picture, and one
   `remove-prompt` on a synthetic picture with SAM 3 and Bria; the fields in
   `docs/results/m3-fills.md` (the commit, the action, the model, the spend settled, the run,
   the recipient, whether the pixels outside the support are the original's). The exchange of
   the first, its picture synthetic and its key absent, is the recorded exchange the
   conformance test reads, committed by the builder. Until it comes, the conformance reads a
   fixture written from fal's published schema, and the evidence clause stays open.

- [ ] Done when:
  `fal::conformance::a_request_is_written_as_fals_reference_describes_it`,
  `fal::conformance::the_recorded_exchange_is_read_back_to_its_pixels`,
  `fal::a_reply_holding_a_url_is_a_fault_and_nothing_is_fetched`,
  `fal::a_fault_holds_nothing_of_the_body_the_headers_or_the_address`,
  `fal::the_key_goes_only_to_the_approved_origin_in_its_header`,
  `wired::a_fill_to_a_recipient_the_config_does_not_name_is_refused_before_a_byte_leaves`,
  `spend::fill::a_bria_call_reserves_its_price_and_settles_at_it`,
  `fal::a_picture_larger_than_the_models_side_is_scaled_inside_and_returned_at_its_size`,
  `paste::feather_keeps_every_pixel_outside_the_mask_and_every_protected_one`,
  `cli::rascal_paste_composites_a_fill_onto_its_base`,
  `fill::pixels_outside_the_support_and_protected_ones_are_the_originals`,
  `fill::the_mask_is_rebound_onto_the_composite_through_a_checked_derivation`,
  `outputs::a_mask_on_another_image_than_its_port_names_is_refused`,
  `outputs::an_image_not_derived_from_the_one_its_port_names_is_refused`,
  `outputs::a_mask_not_rebound_onto_the_image_its_port_names_is_refused`,
  `outputs::an_output_whose_origin_names_another_action_is_refused`,
  `outputs::an_output_past_the_decode_limits_is_refused_before_publication`,
  `fill::a_fill_answered_with_the_picture_it_was_sent_keeps_the_masks_binding`,
  `encode::the_core_encoder_keeps_the_four_pins_and_the_squint_vectors`,
  `cli::rascal_remove_prompt_until_feather_hands_back_the_mask`,
  `cli::rascal_remove_prompt_fills_under_a_scripted_filler`,
  `export::a_mask_exported_imports_into_another_store_with_the_same_identity`,
  `import::a_fifo_at_the_path_is_refused_and_nothing_waits`,
  `import::a_file_over_the_limit_its_contents_allow_is_refused`,
  `export::a_request_id_of_a_dot_or_two_names_no_file_by_it`,
  `gate::a_store_under_a_folder_in_another_normal_form_is_kept_out`,
  `record::a_fill_run_holds_no_byte_of_its_key`,
  `evidence::the_fills_record_names_a_commit_of_this_tree_and_holds_no_secret`,
  `just docs` regenerated (the records of `paste` and `fill` are D1's), and the carry-over rows above closed or narrowed with a line.

**Keeping tests lean (F1).** A scripted filler in process; one loopback listener with a test
certificate for the conformance; pictures of 64 by 64.

### Origins (F)

`https://fal.run`, for filling, with the classes `source_images` and `derived_images` (the
picture and the mask); nothing else of fal's: replies are read with `sync_mode`, and fal's
media hosts are never contacted. A new recipient: nothing is sent until the owner says yes
(second item under "What the owner must answer"). On a no, the kind lands with its
conformance, which sends nothing; every live fill of this plan waits, and the milestone's row
waits on it.

## G1. Gates, the baseline's live rerun, and squint's wire (large)

### Goal

What plan 2C's E2 and F1 left (carry-over rows on gates as files, and on the live rerun and
the one gate run): a gate frozen in a file is run by `whizbang eval run --gate <file>` and
decided as `SPEC.md` section 8 says; the baseline's descriptive live rerun is a command; the
gate of details first is run.

1. **`eval run --gate`** (`gate.rs`'s rest): a gate file names its matrix; E1's runner runs
   it; the gate's digest is in every run it makes; scoring reads labels from a path given on
   the command line, writes nothing into the sets' tree, and the receipt holds aggregates
   only; too little data is "inconclusive".
2. **The live rerun, descriptive** (plan 2C, E2, item 8): `whizbang eval baseline --live
   --package <dir> --receipt <file>` runs arm C of `claude-haiku-5-5` on
   `source-fidelity/blind-v3` fresh, one repetition, and prints two numbers: its agreement
   with the recorded run and the recorded run's self-disagreement; no item. Run by the owner
   on the Mac (the package is owner-only; third item under "What the owner must answer"),
   the two numbers in `docs/results/m2c-baseline.md`.
3. **The gate of details first** (plan 2C, E2, item 9): arm `F` against arm `C` on the five
   dev sets, frozen in `evalsets/gates/details-first.toml`, with `claude-haiku-5-5` (three
   repetitions), `gemini-3.8-flash` and `gpt-6.1-terra` (one each), run by the orchestrator;
   its aggregates in `docs/results/m3-gates.md`. Dev sets admit nothing; the result says
   whether the details helped.
4. **The wire's transformation for Google and OpenAI** (row on what squint's wire did to a
   tool's schema and a call's arguments): every tool of the build's actions is sent to each
   of the three judges in a test against the recorded fakes, and each recorded and live
   call's arguments read as the schema says; or squint's transformation is ported with its
   tests. U1's `view` crops are among the tools once U1 lands; whichever lands second adds
   them to the test.

- [ ] Done when:
  `gate::the_frozen_gates_digest_is_in_every_run_it_makes`,
  `gate::scoring_a_synthetic_blind_set_leaves_the_sets_tree_unchanged_and_writes_only_aggregates`,
  `gate::a_gate_with_too_few_families_is_inconclusive_in_its_receipt`,
  `cli::eval_run_gate_runs_a_three_item_gate_under_a_scripted_judge_and_writes_its_receipt`,
  `baseline::the_live_rerun_prints_its_agreement_and_the_recorded_self_disagreement_and_no_item`
  (over a synthetic package and a scripted judge),
  `google::every_tool_of_the_build_is_sent_and_its_arguments_read_by_its_schema`,
  `openai::every_tool_of_the_build_is_sent_and_its_arguments_read_by_its_schema`,
  `evidence::the_gates_record_holds_the_details_first_receipt_and_only_aggregates`,
  `evidence::the_baseline_record_holds_the_live_reruns_two_numbers`,
  and the carry-over rows on gates as files, on the live rerun and the one gate run, and on
  squint's wire closed.

**Keeping tests lean (G1).** Gates over three to six items made in the test, 200 resamples;
scripted judges; pictures of at most 128 pixels.

### Origins (G)

None new: the three judges approved in milestone 2.

## U1. The attack on `unsure` (large)

### Goal

The third of the verdicts that are `unsure` (`docs/results/m2c-evaluation.md`: 33.2% of the
completed, 29 of the 71 in reality-checks and 21 in reference-consistency) is counted by
cause and attacked by two changes, each measured by E1's matrices and decided by a gate
frozen in a file.

1. **What makes a verdict `unsure`.** `whizbang runs stats --unsure-by rule` counts, over a
   filter, the unsure verdicts by the rule and the disposition that made each one unsure,
   read from the checklist each run records: aggregates only, never an item. Run by the
   orchestrator over the store of F1's evaluation of milestone 2, its table in
   `docs/results/m3-unsure.md`, before either change is tuned.
2. **Arm settings.** A matrix arm gains two settings beside its tools: `details` (a list of
   details to measure first, each with the use cases it applies to) and `second_look` (`on`
   or `off`). Both are part of the cell's identity and of the run's arguments. A matrix whose
   arm names a detail for a use case that cannot take it is refused before anything runs.
3. **First change: the picture in tiles, for a use case with one image.** A use case with no
   pair (reality-checks; brief-compliance with no reference) gets nothing from `details =
   first` today. The detail `tiles` hands the judge, before the first turn, the candidate cut
   into four overlapping tiles at full resolution by `view`, each labelled with its place.
   Named by `--detail tiles`; given for a use case with a pair, it is refused on the command
   line naming the use case. Experimental, with its record (rung 1: two worked examples).
4. **Second change: a second look.** When a submitted checklist leaves a mandatory item
   tentative, so that the verdict would be `unsure`, `check` gives the judge one more turn,
   once a run: `view` crops at full resolution of the regions the judge cited for that item
   (at most four), and the question to settle that item alone, confident either way or kept
   tentative. Inside the run's budget of turns and spend; recorded as a turn of its own;
   `--second-look off` (the default until a gate on fresh data says otherwise) runs as today.
   The verdict is still worked out by squint's table from the dispositions: the code decides,
   never the judge's opinion.
5. **The screen.** `evalsets/matrix/unsure-screen.toml`, by E1's runner: on
   reality-checks/people-v1 and reality-checks/adversarial-v1, the arms `F`, `F` with
   `tiles`, `F` with the second look, and both; on reference-consistency/pets-v1 and
   reference-consistency/adversarial-v1, `F` and `F` with the second look (88 items in all),
   `claude-haiku-5-5`, three repetitions; the unsure rate and balanced accuracy of each arm
   by set in `docs/results/m3-unsure.md`.
6. **The gate.** The arm the screen favours, with its settings per use case, frozen against
   `F` in `evalsets/gates/unsure.toml` and run by G1's `eval run --gate` on the five dev sets,
   Haiku three times, Flash and Terra once: the primary measure balanced accuracy; the
   guardrails the unsure rate of each use case, the false rejects and the cost per decision.
   The screen and the gate are on dev sets the evaluation of milestone 2 also used, so the
   result is descriptive and admits nothing: a change that helps here becomes a default only
   through a gate on fresh data, milestone E's. A change that does not help is kept as an
   experimental option or dropped, and recorded either way.

- [ ] Done when:
  `stats::unsure_by_rule_counts_each_unsure_verdict_by_the_rule_that_made_it`,
  `stats::unsure_by_rule_prints_aggregates_and_no_item`,
  `matrix::an_arms_details_and_second_look_are_in_its_cells_identity_and_run`,
  `matrix::an_arm_naming_a_detail_for_a_use_case_that_cannot_take_it_is_refused_before_anything_runs`,
  `matrix::an_arm_with_tiles_measures_the_tiles_first_in_each_of_its_runs`,
  `matrix::an_arm_with_the_second_look_gives_one_on_a_tentative_item`,
  `details::tiles_hands_the_judge_four_overlapping_tiles_at_full_resolution_before_the_first_turn`,
  `cli::squint_check_detail_tiles_on_a_pair_is_refused_naming_the_use_case`,
  `check::a_tentative_mandatory_item_gets_one_second_look_with_the_cited_crops`,
  `check::the_second_look_is_inside_the_runs_turns_and_spend`,
  `check::second_look_off_runs_as_before`,
  `check::a_verdict_after_a_second_look_is_still_worked_out_from_the_dispositions`,
  `matrix::the_unsure_screen_and_its_gate_are_read`,
  `evidence::the_unsure_record_names_a_commit_of_this_tree_and_holds_only_aggregates`,
  `scripts/tests/test-tool-records.sh` holding the record of `tiles`.

**Keeping tests lean (U1).** Scripted judges; sets of three to six items made in the test;
pictures of at most 128 pixels.

### Origins (U)

None new: the three judges approved in milestone 2. The tiles and crops are parts of the
pictures they were already sent, in the classes `source_images` and `derived_images`.

## O1. Locations and mask shaping (large)

### Goal

Rascal's operations that make a location or shape a mask are actions with descriptors and
records, and the tasks `remove`, `repair_area`, `mask_rect` and `protect` run as recipes.

1. **Six operations** (`crates/rascal-ops/src/line.rs`, `rect.rs`, `stroke.rs`,
   `rasterize.rs`, `shrink.rs`, `invert.rs`, by `port-module`), each an action with its
   descriptor, exact against rascal's vectors; and `subtract` of a location, by
   `rasterize::location_core`. Their records are D1's.
2. **What rascal's `mask.rs` and `location.rs` held beside the record** (rows on the counts
   per zone, the box of weighted pixels and emptiness; on a polygon whose edges cross and
   what ties a location's parts together): ported with the operation that reads each, or
   dropped with a line.
3. **The four tasks** (rascal's `remove.toml`, `repair-area.toml`, `rect.toml` as
   `mask_rect`, `protect.toml`): `remove` and `repair_area` end in `fill`; `mask_rect` and
   `protect` make a mask and fill nothing. Each task's acceptance case is a test of its
   recipe under the fake segmenter and a scripted filler, with its preservation shown, with
   and without `protect` where it takes one. `mask_rect` and `protect` use no model; their
   records are D1's.

- [ ] Done when:
  `line::the_ported_vectors_give_rascals_locations`,
  `rect::the_ported_vectors_give_rascals_locations`,
  `stroke::the_ported_vectors_give_rascals_locations`,
  `rasterize::the_ported_vectors_give_rascals_masks`,
  `shrink::the_ported_vectors_give_rascals_masks`,
  `invert::the_ported_vectors_give_rascals_masks`,
  `subtract::the_ported_vectors_of_a_location_give_rascals_masks`,
  `location::a_polygon_whose_edges_cross_is_refused`,
  `mask::counts_per_zone_and_the_box_of_weighted_pixels_are_rascals`,
  `task::remove_fills_inside_its_mask_and_keeps_every_other_pixel`,
  `task::remove_completes_with_and_without_protect`,
  `task::repair_area_fills_inside_its_mask_and_keeps_every_other_pixel`,
  `task::mask_rect_makes_the_rectangle_as_a_mask`,
  `task::protect_inverts_an_earlier_mask_onto_its_composite`,
  `cli::rascal_remove_with_a_box_fills_under_a_scripted_filler`, `just docs` regenerated,
  and the carry-over rows above closed or narrowed with a line.

**Keeping tests lean (O1).** Rascal's vectors as small committed planes; pictures of at most
128 pixels.

## O2. Canvases, and a fill put back (large)

### Goal

The operations that move a picture or a mask onto another canvas and back are actions, with
every location carried through their mappings and every mapped location's identity the same
on both CI runners; and a fill made on a canvas is put back onto the picture it came from.

1. **Five operations** (`cut.rs`, `put.rs`, `window.rs`, `unwindow.rs`, `apply.rs`), each with
   its descriptor; `window-v1.toml` copied byte for byte, the first line of the constant
   sets' digest list. `apply` makes the working image from an image and a mask with a
   prescription, as rascal's `heal_once` did before it filled, and hands back with it the
   mask rebound onto the working image through `Derivation::checked` (rascal shared one frame
   between the two; whizbang's mask belongs to one image, `Mask::require_image`), so a later
   step fills the working image through a mask that is its own. A mask without a
   prescription, or one of no effect, leaves the image as it is and the mask's binding as it
   was (F1, item 5, "An unchanged result").
2. **What rascal's `frame.rs` and `mapping.rs` held beside the mappings** (row on the strip
   width and the mapping of locations): ported with `cut` and `put`. **Locations through
   every binding** (rows on a location made by mapping another, and on `libm` for what is
   hashed): a fixed vector for each kind still owed one, a rotation and a wrap, the same on
   both runners; sine, cosine and the arc functions by `libm` where the result is hashed.
3. **A fill put back through a window or a cut** (row on rascal's `heal.rs`): `window`,
   `fill`, `unwindow`, and `cut`, `fill`, `put`; every pixel of the original that the mask
   never reaches, and every protected one, is the original's.

- [ ] Done when:
  `cut::the_ported_vectors_give_rascals_canvas_and_plan`,
  `put::a_canvas_put_back_through_its_plan_is_the_original_outside_the_support`,
  `window::the_ported_vectors_give_rascals_window`,
  `unwindow::a_window_unwound_is_the_original_outside_the_support`,
  `apply::the_ported_vectors_give_rascals_working_image`,
  `apply::a_mask_without_a_prescription_leaves_the_image_and_the_masks_binding_unchanged`,
  `apply::a_ramp_of_no_effect_leaves_the_image_and_the_masks_binding_unchanged`,
  `apply::the_mask_handed_back_is_rebound_onto_the_working_image`,
  `mapping::a_location_through_a_wrap_and_back_lands_where_it_began`,
  `identity::a_mapped_location_has_its_fixed_identity_for_a_rotation_and_a_wrap`,
  `heal::a_fill_through_a_window_keeps_every_pixel_the_mask_never_reaches`,
  `heal::a_fill_through_a_wrapped_cut_keeps_every_pixel_the_mask_never_reaches`,
  `constants::every_set_rascal_shipped_is_here_byte_for_byte` (over the sets landed, the
  rest listed as owed), `just docs` regenerated, and the carry-over rows above closed or
  narrowed with a line.

**Keeping tests lean (O2).** Equirectangular pictures of 256 by 128 at most; a scripted
filler.

## Q1. Squint's use cases as recipes (large)

### Goal

Each of the five use cases is a recipe on the core's runner (`SPEC.md` section 7, "Use cases
expressed as recipes on the shared mechanism"): its rules, the tools that run before the
judge, and the tools the judge may call; `whizbang squint check <use-case>` runs that recipe
and gives the verdicts it gave before.

1. **Five recipes** in `crates/whizbang-squint/recipes/`, each of two steps: `details`, then
   the judge's step. A recipe's steps are fixed, but the work of details is not: one pair or
   up to eight (one for each reference), a pair that does not register (then `align` hands
   back its failure and no aligned frame, and no comparison runs on that pair, as
   `check`'s details step does today), and `details = none`, which measures nothing. So
   `details` is one action, the step that does today's details work: it calls `align` for
   each pair the use case relates and each named detail on each pair that registered, as
   nested calls, and hands back in order the measurements, the failures and the images the
   judge is shown first (each aligned frame, each comparison's map, U1's tiles) as image
   outputs of its own, bound to the judge's step as R1's steps pass artifacts. The judge's
   step is
   the loop with the use case's rules, the tools it may call and U1's second look as a
   setting. A `--set` override reaches `details`' arguments as R1's do.
   The cache key (ADR 0031) takes the recipe's digest; an unchanged job still hits and a
   changed recipe misses.
2. **What an evidence tool hands a later step, and the verdict** (row on a result that is not
   an artifact): a new ADR adds result kinds to the descriptor beside artifact ports: a
   verdict, and an evidence tool's measurements (`align`'s homography, residual and quality;
   `change_map`'s and `edge_similarity`'s numbers; a tool file's declared output). A step's
   measurements are bound to a later step's input as an artifact is, so the judge's step is
   handed `align`'s measurements by the step that made them, never by working them out again
   (as A2's `check` does today, plan 2C, A2's rewording). An action that does not declare a
   verdict cannot hand one back; a caller of the recipe is given the verdict with the outcome.
3. **A tool's image only its own run reads** (rows on a tool's result and on deleting a run's
   artifacts): a reference of a child that only the root run it belongs to reads, in the store
   and in the commit step: the recipe run, its `details` step handing its images to the
   judge's step, and the judge's later `view` crops of them, all inside one root run; a call
   from another root run for it is refused; a run holds a reference to each input, so a
   deletion removes what no run and no import holds.
4. **What squint's `view` had around its pixels, and its decoding** (rows on `view` and on
   decoding each time): the crop's place on its source returned in a form a caller maps
   through, and a run keeping the pixels of an image it reads more than once, with the time in
   `view`'s record.
5. **The records of the five recipes** (`docs/reference/tools/<use-case>.md`): rung 1, two
   worked examples each, from A1's synthetic pictures under a scripted judge, run as tests;
   rung 2 credited from milestone 2's evaluation (`docs/results/m2c-evaluation.md`: arm F,
   Haiku, by set), which ran the same steps as `check` (item 1's test that each recipe gives
   the verdicts `check` gave is what the credit rests on), with the data, the method and the
   numbers cited. Each unverified.
6. **The rest of what squint's port left**: the loop `check` keeps (row on `check` sending
   squint's requests: kept, with the row's line saying why, the replay holding squint's), the
   rest of a tool manifest (row on a manifest's fields), the cited calls of a submission (row
   on what the port of the submission does not hold), the old configs' keys (row on what the
   old configs held), squint's milestone 4 follow-ups and `resize`'s comparison of methods:
   each built with a test or dropped with a line. A stored run of `check` as a transcript (row
   on that test) is B1's.
7. **Evidence, by the orchestrator, once**: each use case's recipe once on Anthropic's judge
   on A1's pictures, the fields added to `docs/results/m2c-use-cases.md`.

- [ ] Done when:
  `use_case::each_use_case_is_a_recipe_with_its_rules_details_and_tools`,
  `check::each_use_case_through_its_recipe_gives_the_verdict_it_gave_before`,
  `details::a_pair_that_does_not_register_is_judged_with_its_failure_and_no_comparison`,
  `details::details_none_measures_nothing_and_the_judge_step_still_runs`,
  `details::three_references_are_each_aligned_in_the_order_given`,
  `cache::a_changed_use_case_recipe_misses_and_an_unchanged_one_hits`,
  `descriptor::an_action_that_does_not_say_it_answers_with_a_verdict_cannot_hand_one_back`,
  `runner::the_judge_step_is_handed_aligns_measurements_and_align_runs_once_a_pair`,
  `descriptor::an_evidence_tools_measurements_are_a_declared_result_kind`,
  `check::a_tools_image_is_a_reference_only_its_parent_reads`,
  `runner::the_aligned_frames_and_tiles_of_details_reach_the_judge_step`,
  `check::the_judge_crops_an_aligned_frame_handed_on_by_details`,
  `store::another_root_run_cannot_read_a_childs_image_of_this_one`,
  `store::a_deletion_removes_what_no_run_and_no_import_holds`,
  `view::a_crops_place_on_its_source_maps_back_through_its_binding`,
  `view::a_run_reading_an_image_twice_decodes_it_once`,
  `submission::a_cited_call_that_is_not_a_completed_call_of_the_run_is_refused`,
  `describe::the_five_use_cases_are_listed_as_recipes_first`,
  `serve::check_over_stdio_returns_the_verdict_of_each_use_case_under_a_scripted_judge`
  (unchanged, through the recipes),
  `evidence::the_use_cases_record_names_a_commit_of_this_tree_and_holds_no_secret` on the
  record with the recipes' lines, `scripts/tests/test-tool-records.sh` holding the five
  recipes' records, `just docs` regenerated, and the carry-over rows above closed, narrowed
  or dropped with a line.

**Keeping tests lean (Q1).** Scripted judges in process; the existing `check::` and `serve::`
tests run through the recipes unchanged, so few new ones are needed.

## O3. The seam (large)

### Goal

Rascal's seam recipe, the one its lab rounds were about, runs on whizbang's runner.

1. **Three operations** (`fan.rs`, `profile.rs`, `ramp.rs`), each with its descriptor; their
   constant sets (`fan-v1` to `fan-v3`, `fan-thin-v1`, `profile-v1`,
   `ramp-v1`, and `feather-v1` if `feather` reads it) copied byte for byte and added to the
   digest list.
2. **The task `seam`** (rascal's `seam.toml` and the graph of `heal_once` in
   `crates/rascal-mask/src/heal.rs`): `cut`, `line`, `fan`, `profile`, `ramp` (a mask with a
   prescription), `apply` (the working image, with the tone step, and the mask rebound onto
   it), `fill` of the working image through `apply`'s mask, `put` back; `fan-v3` the default
   set, as at rascal's tag.

- [ ] Done when:
  `fan::the_ported_vectors_give_rascals_band_for_each_set`,
  `profile::the_ported_vectors_give_rascals_mask`,
  `ramp::the_ported_vectors_give_rascals_prescription`,
  `task::seam_applies_a_nonzero_ramp_once_to_the_pixels_the_filler_is_sent`,
  `task::seam_fills_the_working_image_through_the_mask_apply_rebound_under_the_executors_checks`,
  `task::seam_fills_the_wrap_edge_and_keeps_every_other_pixel`,
  `cli::rascal_seam_fills_a_synthetic_panorama_under_a_scripted_filler`,
  `constants::every_set_rascal_shipped_is_here_byte_for_byte`, and `just docs` regenerated.

**Keeping tests lean (O3).** Synthetic panoramas of 256 by 128.

## O4. `tighten`, `repair`, `inspect` and `paste`'s modes (large)

### Goal

1. **`tighten`** (`tighten.rs`, `tighten-v1.toml`) and the task **`repair`** (rascal's
   `repair.toml`: a stroke tightened to the defect's edges, feathered, filled).
2. **`inspect`** (`inspect.rs`): its report of a mask.
3. **`paste`'s modes** (row on the two other ways of pasting a fill): `hard` a mode of
   `paste` with the same preservation and its own tests; `poisson` (`poisson.rs`,
   `poisson-v1.toml`) likewise, or dropped with a line on the row (the named seam).

- [ ] Done when:
  `tighten::the_ported_vectors_give_rascals_masks`,
  `task::repair_fills_along_the_stroke_and_keeps_every_other_pixel`,
  `inspect::the_ported_vectors_give_rascals_report`,
  `paste::hard_writes_the_fill_where_the_core_is_set_and_nothing_else`,
  `paste::poisson_keeps_every_pixel_outside_its_domain` (or the row's line dropping it),
  `constants::every_set_rascal_shipped_is_here_byte_for_byte`, `just docs` regenerated, and
  the carry-over row on `paste`'s modes closed or dropped with a line.

**Keeping tests lean (O4).** Rascal's vectors; `poisson`'s benchmark `#[ignore]`d.

## O5. `measure`, pictures from callers, and large results (large)

1. **`measure`** (`measure.rs`, `measures-v1.toml`; it reads `fan`'s windows): its measures of
   a mask and a result. With it, all 25 of rascal's operations are actions.
2. **Pictures from callers** (rows on colour profiles that are not RGB matrices, on what
   squint's `image.rs` held beside identity, on a JPEG whose scans are short, on where an
   imported picture came from, and on a recipient approved with `upstreams`; the owner's own
   pictures are the first a door takes beyond the sets): each decided as its row asks, with
   a test.
3. **Large results** (row on encoding images and one large page): the time of encoding a
   result measured at three sizes, and the encoding chosen from the measurement, in
   `docs/results/m3-encoding.md`.

- [ ] Done when:
  `measure::the_ported_vectors_give_rascals_measures`,
  `describe::each_of_rascals_25_operations_is_an_action`,
  `pixels::a_jpeg_whose_scans_are_short_is_refused`,
  `pixels::a_picture_with_a_grey_or_lookup_table_profile_is_read_as_its_adr_says`,
  `store::an_imported_picture_keeps_where_it_came_from_by_a_name_of_its_own`,
  `wired::a_recipient_with_upstreams_is_sent_to_and_one_without_is_refused_naming_upstreams`,
  `evidence::the_encoding_record_names_a_commit_of_this_tree`,
  `constants::every_set_rascal_shipped_is_here_byte_for_byte` (all eleven, or `poisson-v1`
  named as dropped), `just docs` regenerated, and the carry-over rows above closed or narrowed with a line.

## L1. The shared harness, and the lab's files (large)

### Goal

The lab's corpus, protocol and matrix are read and checked, and E1's runner runs cells of
any action, so a lab round can run on the shared harness (`SPEC.md` section 7: "on the shared
harness"). Budget: the runner's change about 450; rascal's `lab/cell.rs`, `corpus.rs`,
`protocol.rs` and `matrix.rs` (758 lines) about 1,600 with tests; the two new bindings about
350.

0. **The shared harness.** E1's runner (`crates/whizbang/src/evaluation.rs`) builds calls of
   `check`, stores squint jobs, recognises attempts of `check` alone and forecasts a judge's
   cost. It is made to run cells of any action: a cell is an action, its arguments and a
   cell identity, an attempt is recognised by that identity in any run's record, and a
   forecast is asked of the cell's kind (a judge's from the study's rates, as now; a fill's
   from its price per call). `eval matrix` runs through it unchanged. A lab cell's identity
   is everything that changes the work, as rascal's `CellComponents` (`lab/cell.rs`) had it:
   the image's digest, each bound input's digest or text, the overrides as applied, the
   recipe as it runs, the contents of each constant set a step selects, the version of each
   action, and the filler's name, revision and resolved settings, with each finder's.
1. **The files** (rascal's `crates/rascal-core/src/lab/` and `docs/lab.md`): a corpus manifest
   (ids, SHA-256 digests, groups; the pictures never committed; `whizbang lab corpus verify`
   checks a folder against it), a protocol frozen before the run (its digest in every cell;
   a change after a run is a new lab), a matrix of arms (a recipe with its overrides, and a
   filler). A matrix is checked before anything runs as rascal's was, the dependent-finder
   rule included.
2. **Two bindings rascal's matrix lacked.** A per-image text binding (rascal's matrix took
   files only, so `remove_prompt`, whose prompt is text, could not be an arm), and a
   reference arm (`reference = "<input>"`): an arm that hands back an image the matrix binds
   for that item, the clean picture of B1's set, with no recipe run and no filler call. Both
   are part of the cell's identity.

- [ ] Done when:
  `matrix::eval_matrix_still_resumes_and_keeps_a_failed_attempt_through_the_shared_runner`,
  `runner::a_cell_of_any_action_is_recognised_by_its_identity_in_a_runs_record`,
  `runner::a_fill_cells_forecast_is_its_price_per_call`,
  `lab::a_change_of_override_input_text_constant_set_action_version_or_filler_makes_a_new_cell`,
  `lab::a_matrix_naming_an_arm_the_protocol_does_not_is_refused_before_anything_runs`,
  `lab::a_dependent_finder_is_refused_naming_both_steps`,
  `lab::a_protocol_changed_after_a_run_is_refused`,
  `lab::a_text_binding_reaches_remove_prompts_prompt`,
  `lab::a_reference_arm_hands_back_its_bound_image_and_calls_no_filler`,
  `lab::corpus_verify_names_each_missing_and_changed_picture`.

**Keeping tests lean (L1).** Corpora of three images of 64 by 64; scripted fillers.

## L2. `lab run`, its resume, and the decode (large)

### Goal

A lab round runs a recipe and filler matrix, resumes after an interruption, and its picks are
decoded. Budget: rascal's `lab_run.rs` (646 lines), most of whose cell loop and resume the
shared harness replaces, about 700; `lab/focus.rs` (217) and the decode of
`lab/pairs.rs`'s rules about 900; tests about 600.

1. **Running** (rascal's `lab_run.rs`): `whizbang lab run` runs each cell as a run of its
   recipe, or of a reference arm, through L1's harness, so a cell is resumable and a failed
   attempt is kept (`SPEC.md` section 9, "Run resume"); the round's spend is forecast before
   the first cell, as E1's matrices are.
2. **The decode** (rascal's protocol rules): each group decoded from a picks file as rascal's
   protocol does: `inconclusive` below `min_decided` decided pairs, then `preferred`,
   `not_worse` or `worse` by the protocol's rules; `same` and `neither` count toward neither
   side.

- [ ] Done when:
  `lab::an_unchanged_cell_resumes_without_another_call`,
  `lab::an_interrupted_round_resumes_and_a_failed_cell_is_kept`,
  `lab::a_rounds_forecast_passing_the_allowance_left_is_refused_before_its_first_cell`,
  `lab::a_group_below_min_decided_is_inconclusive`,
  `lab::a_group_at_the_preferred_rule_is_preferred_and_one_below_it_is_not`,
  `lab::a_group_at_the_not_worse_rule_is_not_worse_and_one_past_it_is_worse`,
  `lab::the_decode_gives_rascals_outcomes_on_rascals_picks`,
  `cli::lab_run_runs_a_three_image_round_under_a_scripted_filler_and_resumes`.

**Keeping tests lean (L2).** Rounds of three images of 64 by 64; picks files made in the test.

## L3. Blinded pairs and picks, and a round (large)

### Goal

A lab round presents blinded pairs, collects the picks and checks them against the frozen
assignment; and a round is run on the owner's corpus. Budget: rascal's `lab/pairs.rs` (721
lines over its two crates) and `layout.rs` (239) about 2,000 with tests, the 342-line page
about 300.

3. **Pairs and picks** (rascal's `lab/pairs.rs`, `lab/layout.rs`): `whizbang lab pairs` writes
   a static page of blinded pairs, in an order and with sides drawn from the protocol's seed,
   the assignment frozen beside it by digest, for two recipe arms or for a recipe arm and a
   reference arm; the page loads nothing from the network, runs no script from elsewhere, and
   writes the picks as a file the owner saves. `whizbang lab decode` (L2's) first checks the
   picks against the frozen assignment: a pick for a pair not in it, a missing pair, an
   assignment whose digest differs: refused.
4. **A round, by the orchestrator and the owner**: on rascal's `seam-03` corpus (its manifest
   ported as a file from `labs/seam-03/` at the tag), under a protocol of its own,
   `labs/m3-seam/protocol.toml`, with `seam-03`'s groups, rules and `min_decided`, frozen and
   committed before the run, and a matrix whose arms are the shipped `seam` as it is,
   `fan-v3` (`seam-bria`), against `fan-thin-v1` (`thin-bria`), both filled by Bria: the shipped recipe is what is scored,
   since rascal's `seam-03` ran `fan-v1`, the default when it ran. Interrupted once and
   resumed; the owner picks the pairs; the decode's counts by group in
   `docs/results/m3-lab.md`, beside rascal's `seam-03` (`fan-v1`) and `objects-02` (`fan-v3`
   against `fan-v1`), each named with its constant set.

- [ ] Done when:
  `lab::the_pairs_are_blinded_in_the_order_and_sides_the_seed_draws`,
  `lab::the_pairs_page_loads_nothing_from_the_network`,
  `lab::picks_for_a_pair_not_in_the_frozen_assignment_are_refused`,
  `lab::an_assignment_whose_digest_differs_is_refused`,
  `lab::the_committed_m3_seam_matrix_is_valid_against_its_protocol`,
  `cli::lab_run_pairs_and_decode_remove_prompt_against_a_reference_arm_under_a_scripted_filler`,
  `cli::lab_run_pairs_and_decode_a_three_image_round_under_a_scripted_filler`,
  `evidence::the_lab_record_names_a_commit_of_this_tree_and_holds_only_counts`.

**Keeping tests lean (L3).** Rounds of three images of 64 by 64; picks files made in the test.

### What the owner provides (L)

The pictures of rascal's `seam-03` corpus in a folder on the Mac (checked by hash against its
manifest, never committed, read only by the run), and the picks of the round's pairs (about
45), on the page item 3 writes (fourth item under "What the owner must answer").

## D1 and D2. The records of the operations (large, two)

Every tool is finished only with its record (`SPEC.md` section 8, "Tool records"; a pixel
operation is complete when its record shows rungs 1 and 2). Each writes fourteen records by
`scripts/tool-record.sh`, a hook for each, sharing one helper for a pixel operation's vectors
and times (D1 writes it): rung 1, two worked examples each, run as tests; rung 2, exact
against rascal's vectors and timed at three sizes on the Mac. D1: `segment`, `select`,
`plug`, `feather` and `render` (row on their records; the three profiles ported or dropped
with a line), `close`, `subtract`, `line`, `rect`, `stroke`, `paste`, `fill` (its rung 2 is
B1's, since it is model-based), `mask_rect` and `protect`. D2: `rasterize`, `shrink`,
`invert`, `cut`, `put`, `window`, `unwindow`, `apply`, `fan`, `profile`, `ramp`, `tighten`,
`inspect` and `measure`.

- [ ] Done when (D1):
  `scripts/tests/test-tool-records.sh` holding D1's fourteen records, each with rungs 1 and 2
  (`fill`'s rung 1 alone), and the carry-over row on the records of R2's operations closed.
- [ ] Done when (D2):
  `scripts/tests/test-tool-records.sh` holding D2's fourteen records, each with rungs 1 and 2,
  and `describe::each_pixel_operation_and_task_without_a_model_shows_its_record_complete`.

**Keeping tests lean (D1, D2).** The records' tests compare committed images byte for byte
and the measurements that do not depend on the machine; timing runs only in the script.

## B1. The edits' rung 2, the records, the rows left, and the note (large)

### Goal

Each model-based recipe of rascal's has its ladder record with rungs 1 and 2 (the squint
recipes' are Q1's), every carry-over row owned by milestone 3 is closed, re-owned or dropped,
and the milestone's note is written.

1. **Rung 2 of the edits** (`SPEC.md` section 8, "The ladder": task success, preservation
   outside the mask, and visual quality). A synthetic edit set: 24 pictures from the sets
   already fetched (`source-fidelity/scenes-v1`'s sources, public, fetched by hash), each with
   a defect drawn by a committed test program and the clean picture kept as the reference:
   for `remove`, a shaded shape pasted with its box; for `remove_prompt`, the same shapes
   named by a phrase SAM 3 is asked for ("a red ball"); for `repair`, a scratch with the stroke
   along it; for `repair_area`, a spot with its box. Per recipe: **preservation**, the count of
   pixels changed outside the mask (0 required); **task success**, the IoU of the recipe's
   mask with the drawn defect's (for `remove_prompt`, the whole recipe's shaped mask, not
   SAM 3's alone) and the fill's mean error inside the mask against the clean picture beside a
   plain blur fill's; **visual quality**, a lab round whose arms are the recipe and a reference arm
   holding the clean picture (L1, item 2; `remove_prompt`'s phrase by a text binding), the owner picking the better, decoded `not_worse`
   or not by rascal's rule under a protocol frozen before it runs. `seam`'s rung 2 is scored
   on the shipped recipe by L3's round (blinded quality, and preservation counted on its
   results), with rascal's `seam-03` and `objects-02` cited beside it as earlier evidence on
   the sets they ran, never as the shipped recipe's own; task success for `seam` is that
   round's decode against the fixed-width band.
2. **The records** of `remove_prompt`, `remove`, `repair`, `repair_area` and `seam`
   (`docs/reference/tools/<name>.md`): rung 1, two worked examples each on synthetic pictures,
   run as tests; rung 2 as item 1; each unverified once both are held. SAM 3's agreement
   with reference masks (ADR 0027 and 1D's record) is cited for the segment step only.
3. **A stored run of `check` as a transcript** (row on that test, handed on by Q1).
4. **The rows left.** Every carry-over row owned by milestone 3 that the pull requests above
   did not close: built with a test, or re-owned or dropped with a line saying why. The
   Google price rows (row on introductory prices) read again on or after their dates if B1
   runs after 2026-11-30, else re-owned with a line; two needs of one capability, a real
   prober, and the generation's names (rows owned "3 or later") each decided with a line.
5. **The note.** `docs/plans/m3-note.md`, in the form of `m2-note.md`: each clause of row 3
   with the tests that show it, the spend, what a reader should not assume, what moved to the
   carry-over list, and what milestone 4 and E start from.

- [ ] Done when:
  `edits::preservation_outside_the_mask_is_counted_exactly`,
  `edits::the_synthetic_set_is_drawn_by_its_program_from_fixed_seeds`,
  `edits::task_success_is_the_masks_iou_and_the_fills_error_beside_a_blur_fill`,
  `transcript::a_run_of_check_is_written_as_a_transcript_the_comparison_reads`,
  `evidence::the_edits_record_names_a_commit_of_this_tree_and_holds_only_aggregates`,
  `evidence::each_model_based_recipes_record_holds_rungs_1_and_2`,
  `evidence::each_recipes_rung_2_names_the_shipped_recipes_and_constant_sets_digests`,
  `scripts/tests/test-tool-records.sh` holding the five recipes' records,
  `scripts/tests/test-notes.sh` reading `m3-note.md`, and every carry-over row owned by
  milestone 3 closed, re-owned or dropped with a line.

**Keeping tests lean (B1).** The scoring program's tests on three pictures of 64 by 64; the
set's fills and SAM 3's masks run by the orchestrator, their numbers held by the evidence
test.

### What the owner provides (B)

The picks of the visual-quality pairs: 24 pictures for each of four recipes, about 96 (fourth
item under "What the owner must answer").

## Carry-over rows owned by milestone 3, and who takes each

| Row (by its subject in `docs/carry-over.md`) | Taken by |
|---|---|
| P1's live evidence | L0 |
| The run record's lists (recipe steps); `STEPS_LIMIT`; a parent reading a child's names; step overrides; what a manifest and a task description say; the order of a segmenter's answer; an executor built with a digest alone; what the port of `grow` left | R1 |
| What a filler was sent; image preparation and its version; what an action hands back; a mask rebound only where a composite was made; the encoder in `whizbang-squint`; a mask leaving a store only as its file; `store import`'s file limit; opening a file without waiting; `.` and `..` as names; a folder's normal form on macOS | F1 |
| Gates as files; the live rerun and the one gate run; squint's wire for Google and OpenAI | G1 |
| What rascal's `mask.rs` held; the two rules of `location.rs` | O1 |
| The records of `segment`, `select`, `plug`, `feather`, `render` | D1 |
| Rascal's `frame.rs` and `mapping.rs`; a mapped location's identity on both runners; a fill put back through a window or a cut | O2 |
| A result that is not an artifact; a tool's result; deleting a run's artifacts; `view` around its pixels; `view` decoding each time; `check` and squint's loop; a tool manifest's fields; a submission's cited calls; the old configs' keys; squint's milestone 4 follow-ups; `resize`'s methods | Q1 |
| `paste`'s other modes | O4 |
| Colour profiles; squint's `image.rs`; a JPEG whose scans are short; where an imported picture came from; a recipient with upstreams; large results' encoding | O5 |
| A stored run as a transcript; Google's introductory prices; two needs of one capability; a real prober; every publication writing the whole generation ("3 or later"); and any overflow | B1 |

## What the owner must answer

1. I need your yes to sixteen builders' pull requests of about 2,500 lines in this plan, where
   you asked for about eight: milestone 3 is about 37,500 changed lines. No means this plan
   stops after its first eight (R1, F1, G1, U1, O1, O2, Q1, O3) with a note, and the other
   eight come in a second plan, reviewed the same way. Wave 1 starts either way; wave 2 waits for
   this answer: yes/no.
2. I need `https://fal.run` approved as a recipient for filling (the classes `source_images`
   and `derived_images`: test pictures, the sets' pictures, your corpus's pictures in the lab
   round, and the masks made on them), and `FAL_KEY` put by you in the env file whizbang reads
   by `--env-file`: yes/no. No other fal host is contacted.
3. I need you to run, on your Mac, the baseline gate milestone 2 left with you
   (`docs/plans/m2-note.md`, "What the owner still has to do") and, once G1 lands, its live
   rerun (`whizbang eval baseline --live`, about 0.50 dollars to Anthropic), and to hand the
   orchestrator what each prints and writes: yes/no.
4. I need you to place the pictures of rascal's `seam-03` corpus in a folder on your Mac, and
   to pick the blinded pairs of two rounds on the pages `whizbang lab pairs` writes (about 45
   for `seam-03`, about 96 for the edits' visual quality): yes/no.

## Spend

All on approved recipients, on the owner's Mac, by the orchestrator (the live rerun by the
owner), each pull request's settled spend in its results file and the total in the note.

| Pull request | What is called | Estimate |
|---|---|---|
| L0 | Five use cases on each of `gemini-3.8-flash` and `gpt-6.1-terra`, arm F | 0.50 |
| R1 | Nothing paid: SAM 3 on the Mac | 0 |
| F1 | Two fills of synthetic pictures; the owner's own tries, about 100 fills | 4.50 |
| G1 | The gate of details first: arms C and F on the 100 dev items, Haiku three times, Flash and Terra once; the live rerun, Haiku on 143 items | 15.50 |
| U1 | The screen: 1,320 decisions, Haiku; the gate: two arms on the 100 dev items, Haiku three times, Flash and Terra once | 31.00 |
| O1 to O5 | One acceptance fill of each filling task, on synthetic pictures | 0.50 |
| Q1 | Five use cases' recipes once, Haiku | 0.10 |
| L2, L3 | The `seam-03` round: about 90 fills, a resume repeating none | 3.60 |
| B1 | The edit set: 24 pictures, four recipes, one fill each | 4.00 |
| | **Total** | **about 60 dollars, of the 100** |

The judges' rates are the study's (`docs/baseline/manifest.toml`): about 0.0033 dollars a
decision for Haiku, 0.023 for Flash and 0.017 for Terra on arm C, arm F and its changes
counted at twice that and the second look at three times; Bria Eraser is 0.04 dollars a fill
(fal's model page, read 2026-11-18); rascal's `objects-02` settled 2.08 dollars for 52 fills.

## Risks

1. **The fixes for `unsure` are tuned on sets already seen.** The screen and both gates run on
   the dev sets that measured the 33%. A gain there is reported as descriptive; nothing is made
   a default until a gate on fresh data clears it (milestone E).
2. **A second look raises cost and may push a judge to a confident wrong answer.** It is one
   turn, inside the run's budget; the gate's guardrails are the false rejects and the cost per
   decision.
3. **fal's reply or limits differ from its page.** The builder reads them again before writing
   the wire; the orchestrator's first live fill is the recorded exchange conformance reads;
   a reply that names a URL is a fault, never a download.
4. **The ports do not reproduce rascal's masks.** Each operation is held to rascal's vectors;
   a difference is a decision brought to the orchestrator, recorded in the operation's record,
   never a silent change.
5. **The lab rounds wait on the owner.** The corpus and the picks are the owner's; until they
   come, the lab is shown on a synthetic round of three images and L3's and B1's evidence
   clauses stay open.
6. **The runner raises the call limit of a run.** The measurement comes first, and the limit
   follows it.
7. **The budgets are estimates.** A pull request that passes about 2,500 splits at its named
   seam; the owner is told the count.

### What milestone 4, E and later owe after 3

| Item | Left for |
|---|---|
| Remove with a check and bounded retries, as nested calls on one budget | Milestone 4 |
| `tiles` and the second look, if they help, admitted on fresh data; the model-based recipes up rungs 3 and 4 | Milestone E |
| Report viewer and export | Milestone 5 |
| Local fill | Milestone 8 |
| The runtime on Linux, on an external GPU host | Its carry-over row (the owner's decision of 2026-11-18) |

## Review rounds

The first outside round, on draft 1, had 11 findings: 1 blocker (dependencies that could not
hold: `subtract` reads `rasterize`, `measure` reads `fan`, O1's tasks need `fill`, and two
overflows ran backwards) and 10 majors (the seam's `apply` before its fill; the screen's arms
not expressible in a matrix and `tiles` refused on pairs; the live rerun not built; B1's and
O4's sizes; `paste` not an action; records of every recipe; visual quality on rung 2; the
measurements an evidence tool hands a later step; the lab's `worse`; the reviewer for G1's
egress). All are taken into draft 2: the work is twelve pull requests, with the owner asked.
The second, on draft 2, had 8: 1 blocker (O4 used O2's digest list and O5's inventory test
needed O4, neither a dependency) and 7 majors (a skipped step leaving its binding without an
output; the seam's mask not the working image's after `apply`; E1's runner not able to run a
lab's cells; O1's size with thirteen records; building twelve before the owner's answer, and
a fallback of oversized pull requests; the relations of what an action hands back; O5's
recipient change without a reviewer). All are taken into draft 3: O4 and O5 wait on what
they read; an absent optional input is passed through as rascal did; `apply` rebinds the
mask; the lab is two pull requests with the runner's change budgeted; the records are D1's;
fourteen pull requests, with the owner choosing fourteen or a second plan, and only wave 1
before the answer; each relation refused with a test; O5 reviewed. The third, on draft 3,
had 5, all major, no blocker (the seam's rung 2 scored on `fan-v1` while `fan-v3` ships;
squint's details, whose work varies, as fixed recipe steps; an unchanged result that
`Derivation::checked` refuses; a lab cell's identity missing what changes its work; D1's
size at 28 records). All are taken into draft 4: L2's round scores the shipped recipe;
`details` is one action doing today's work as nested calls; an unchanged result keeps its
mask's binding; a cell's identity is rascal's `CellComponents`; the records are D1 and D2,
fifteen pull requests in all. The fourth, on draft 4, had 3, all major, no blocker (how
`details`' images reach the judge's step when a child's image is its parent's alone; a lab
matrix that cannot bind `remove_prompt`'s text or hold a clean picture as an arm; L1's size by
the plan's own rate). All are taken into draft 5: `details` hands its images on as outputs
read only inside one root run; L1 adds text bindings and a reference arm; the lab is three
pull requests, sixteen in all. The fifth, on draft 5, had 2, both major (the round's arm
names did not match `seam-03`'s protocol copied as it was; L1's seam moved the reference arm
behind the pull requests that need it). Both are taken into draft 6: the round has a protocol
of its own, frozen before it runs, with a test that the committed matrix is valid against it;
L1's seam is `lab corpus verify`, which nothing before L2 reads. The sixth, on draft 6, had none:
no blocker and no major.
