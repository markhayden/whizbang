# Plan: the rest of checkpoint 1D

Draft 4, 2026-11-18, from `main` at 4f4ca15 (the first slices merged). What checkpoint
1D still owes after the slices of `docs/plans/m2a-first-slices.md` (closed; its table
"What 1D still owes after R"), in the order the owner asked for it while trying real
pictures: first point and box requests on SAM 3 and picking one instance out of several,
from the command line; then the faults found in use; then the rest of `SPEC.md` section
16, row 1D: "SAM 3 answers phrase, point and box requests; Tiny passes conformance for
the features it declares; parity fixtures and a mask-agreement tolerance are recorded;
timings are measured". It ends with the note of 1D. Five pull requests, no more.

## How the work is split

| Pull request | What | Builds on | Size |
|---|---|---|---|
| F1 | Fixes found in use: the env file, the judge's spend and printed reason, CI on Linux; on the owner's yes, the runtime on Linux | `main` | Medium: about 1,500 changed lines, 2,000 with Linux |
| Q1 | Point, box and pick on SAM 3, from the command line | `main` | Large: about 2,400 |
| T1 | Tiny and the `add-adapter` skill | Q1 | Large: about 2,000 |
| U1 | A user's weights and a user's adapter | T1 | Large: about 2,000 |
| V1 | The device setting, parity fixtures, the tolerance, timings, diagnostics to the run, the note | T1 | Large: about 2,200 |

F1 and Q1 start together from `main`; they share only `crates/whizbang/src/rascal.rs`
and `wiring.rs`, where F1 moves where the env file is asked for (a few lines), and their
own carry-over rows and boxes here; the second to land merges `main`. T1 waits for Q1
(the descriptor, the shared files, the pack refresh). U1 and V1 both start from T1 and
share only `pack/src/whizbang_pack/worker.py` (U1 the adapter table, V1 the device
setting) and ADR 0020; the second to land merges `main`.

**The evidence record is retaken by each pull request that changes what it checks.**
`evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`
compares the committed record with the adapter, its revision and `pack/uv.lock`. So Q1
(the adapter), T1 (the table of adapters), V1 (the device) and F1 on a yes to Linux (the
lock) each retake it with `scripts/model-evidence.sh` on their own clean head before
they push, and name that test in their "Done when"; this never overflows. When two of
them are open at once, the second to land merges `main` and retakes it on the combined
tree. Only the new lines of point and box (Q1 item 9) may move to V1.

**Overflow, never a sixth pull request.** If a pull request would pass about 2,500
changed lines, its named overflow moves to the named later pull request, and its builder
says so in the hand-back: Q1's guide and the new lines of point and box (items 8 and 9)
to V1; T1's model test of a point and a box to V1; F1's Linux runtime (item 6) to V1;
U1's user adapter (item 3) to a carry-over row owned by milestone 2, with the owner
told; V1's diagnostics to the run (item 3) to a carry-over row owned by milestone 2. Nothing splits into two pull
requests.

**Review rule.** As in the first slices: a separate reviewer reads a pull request only
for four things: the handling of a token or key, egress (what leaves the machine and to
whom), processes or threads left running, and what ships in the binary or the release.
F1 (the key, when the env file is asked for, and on a yes a new origin and the lock)
gets one; Q1 (a second model resident in the worker; what the catalogue ships; files
shared between installations) gets one; T1 (new downloads and what the catalogue ships)
gets one; U1 (a user's file run by the worker) gets one. V1 is checked by the
orchestrator and merged on green CI, with `just scan-range` over `base..head`, unless
F1's Linux item moved there, which brings a reviewer for it. A "Done when" is a list
of tests by name.

**Test time.** Every model test is `#[ignore]`d, run by `just test-models` on the Mac,
and never by CI. New tests in CI are fakes and small committed planes: each pull request
reports its `just check` and CI test times, and stops and says so if the hosted macOS
runner passes about 270 s of its 300 s, or Linux or a local run about 150 s of its 180 s.

## F1. Fixes found in use (medium)

1. **The env file only when a credential will be used.** Today a config that names a
   credential (`key_env`) refuses every command without `--env-file`, even `whizbang
   rascal segment` on the kind `pack`, which uses no credential
   (`Loaded::of_files_knowing`, `LoadRefused::NoEnvFile`). The reason stands: the env
   file's values are what the executor knows to keep out of the record, the diagnostics,
   the events and every fault, and a config that holds a key in place of a variable's
   name is refused whole. So the env file is asked for when the command has chosen the
   backends its run will use, before any is built and before anything but the config is
   read, and only when one of them names a credential; a command that uses no backend
   (`store`, `runs`, `spend`, `models list`, the operations of `rascal`) never asks for
   it. A config read without an env file hands no backend a key. Its refusal for a
   missing env file names the backend and the setting `key_env`, never the setting's
   value: ADR 0011 says a key of capital letters and digits cannot be told from a
   variable's name, so the variable is named only once the env file is read and its
   values are known to be kept out (the first slices' review: a key pasted into `key_env`
   is never printed). With an env file, its values are known first, as now. ADR 0011
   says when the env file is asked for.
2. **The judge's spend, from the review of the first verdict.** A reply with a status
   of success the adapter does not list is charged its bound, as an unreadable reply of
   200 is, not nothing. A turn whose reported input passes the price row's long-prompt
   line (100,000 tokens) is charged the long-prompt prices of the same reading, with its
   date, so the charge is never below the price. A turn cut off at the output ceiling is
   not retried with the same limit: when the ceiling leaves no larger limit, the loop
   ends with a typed fault naming the setting to raise. Each judge turn's record keeps
   the input the adapter counted beside the usage the provider reported. ADR 0026 is
   amended for each.
3. **The operations' tests, after the fact.** The first slices' review found the tests
   of `select`, `plug` and `feather` written after their code and no mutant tried. Three
   mutants that matter are tried, with `just test-only`: the exclude box's centre rule,
   `plug`'s share comparison and `feather`'s protect. Each survivor gets a test.
4. **The models directory's check records.** R1's check records are not among the
   folders `ModelsDir::unlinked` sweeps, since that would contradict the link test.
   ADR 0017 says why, and what removes a check record.
5. **CI on Linux, the entry check of 1D (carry-over row on the Linux job over 10
   minutes).** The `check (ubuntu-22.04)` job's time is measured from its log on `main`,
   then cut as C0b cut macOS: the script tests in a job of their own on that runner. If
   that cannot bring it under 10 minutes, the owner's budget is the item below.
7. **The verdict's reason, whole.** `squint check` prints each item of the judge's
   reason cut mid-word at about 300 characters, so the end of the explanation is lost:
   `source_fidelity::verdict_of` cuts each item to 300 and joins them with "; ", and the
   record keeps the verdict's reason to 4,096. So the reason's items are kept apart: the
   action writes them, redacted first, to a run diagnostic of their own, one item to a
   line, bounded per item and in all; the verdict's reason in the record stays the short
   summary it is, under its 4,096. The door prints each item whole from that diagnostic,
   on a line of its own, escaped after redaction, cut only where the door's print limit
   (`door.rs`, `PRINT_LIMIT`) cuts, with its three dots. The pull request that adds `squint check --brief` (#179) lands
   first; F1 merges `main` and keeps `--brief`'s output to the same rule.
6. **The runtime on Linux, on the CPU, only on the owner's yes** (second item under
   "What the owner must answer"; if the yes comes after F1 has opened, this item is
   V1's). The owner dropped Linux from the pack's lock on 2026-11-17 over NVIDIA's
   licences (ADR 0021, "Amended: the Mac alone"). The one licence-clean way found is a
   CPU-only build of `torch` from PyTorch's own index, `https://download.pytorch.org`,
   which pulls in no NVIDIA package; that host is new. On a yes: the extra's marker
   names Linux on x86-64 with that index for that platform alone, the lock is made again
   with only approved origins, `EXTRA_HOST` becomes the list of the lock's platforms,
   and the carry-over row's test is changed to what was decided. On a no, Linux stays on
   its carry-over row and 1D closes without it; nothing else here depends on it.

- [ ] Done when:
  `config::a_config_naming_a_credential_loads_without_an_env_file_and_hands_no_backend_a_key`,
  `config::a_key_of_capitals_and_digits_pasted_into_key_env_is_never_said_back_without_the_env_file`
  (the refusal, the record, the diagnostics and the error output searched for it),
  `cli::rascal_segment_on_a_pack_backend_runs_without_the_env_file_under_a_config_naming_a_judge_key`,
  `cli::commands_that_use_no_backend_run_without_the_env_file_under_a_config_naming_a_key`,
  `cli::squint_check_without_the_env_file_is_refused_naming_the_backend_and_not_the_value`
  (the first slices' `cli::squint_check_without_the_env_file_is_refused_naming_the_variable`,
  changed to what this item decides),
  `record::a_run_without_the_env_file_holds_no_byte_of_a_key_pasted_into_the_config`,
  `anthropic::an_unlisted_success_status_is_charged_its_bound`,
  `spend::judge::a_turn_whose_reported_input_passes_the_long_prompt_line_is_charged_the_long_prompt_prices`,
  `check::a_turn_cut_off_at_the_output_ceiling_is_not_retried_with_the_same_limit`,
  `record::each_judge_turn_records_its_counted_input_beside_its_usage`,
  `cli::squint_check_prints_each_reason_item_whole_on_its_own_line` (an item over 300
  characters, read back from the stored run),
  `cli::reason_items_over_4096_characters_in_all_are_each_printed_whole_from_the_stored_run`,
  `cli::a_reason_item_over_the_print_limit_is_cut_there_and_escaped`,
  `record::a_reason_item_is_redacted_before_it_is_cut_or_escaped`,
  `cli::squint_check_brief_prints_the_same_items`,
  `select::a_pixel_whose_centre_is_on_an_exclude_boxs_edge_is_cleared`,
  `plug::a_hole_of_exactly_its_share_is_filled`,
  `feather::protect_is_kept_where_the_blend_would_lower_it`,
  `scripts/tests/test-ci-jobs.sh` holding the new job on both runners, and the carry-over
  row's line with `check (ubuntu-22.04)` under 10 minutes on `main` after the merge (or
  the owner's new budget, quoted); on the owner's yes to Linux,
  `setup::the_extra_for_linux_is_the_cpu_build_and_the_lock_names_no_nvidia_or_cuda_package`,
  `setup::the_locks_origins_are_the_approved_ones`, and `whizbang setup --extra torch`
  ending well on the Linux runner with the worker's hello, recorded as fields in the
  carry-over row, and the evidence record retaken
  (`evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`).

## Q1. Point, box and pick on SAM 3 (large)

### Goal

`whizbang rascal segment --image <id> --point "X Y"` or `--box "L T R B"` answers from
SAM 3 as `--phrase` does now, and `whizbang rascal select --run <id> --pick N` or
`--at "X Y"` keeps one instance out of the several a request answered, so a cut-out is
three commands: segment, select, render. All local; nothing is paid and nothing leaves
the machine.

1. **The adapter.** `sam3` (`pack/src/whizbang_pack/sam3.py`) declares `phrase`,
   `point` and `box`. A point or a box is asked of `Sam3TrackerModel` of the same
   library and the same files (ADR 0022, "Consequences": no second runtime, no new
   download), loaded on the first such request and kept resident beside `Sam3Model`
   under the worker's watermark; the picture's features are computed once for a batch.
   A point or a box answers the tracker's candidates (at most three), best predicted IoU
   first, cut to `max_instances`; each instance's score is that predicted IoU, and the
   candidates are cut to the limit before any is made full size (as the phrase path is,
   `test_candidates_are_cut_to_the_most_instances_asked_before_a_full_size_plane_is_made`).
   The adapter's revision moves, and the binary's table with it. A box used as an
   exemplar ("find more like this") is not a request of the core and is not built.
2. **The catalogue.** SAM 3's entry gains the features `point` and `box` at a new
   revision with the same files.
3. **A requirement that follows the query.** Today a `Requirement` names fixed
   features, and an action descriptor holds two requirements of one capability only as
   a complementary pair (`Given(x)` and `NotGiven(x)`). The core gains a requirement
   whose feature is chosen by an argument's value (the query's kind), checked when the
   call is prepared and again at admission, so a backend that declares `phrase` alone
   still answers phrases and is refused a point. ADR 0003 and ADR 0004 are amended.
4. **Files shared between installations (carry-over row on two installs sharing a
   file).** The new revision names the same 3.4 GB the owner has installed, so an
   install whose file, by its size and hash, is already held by another installation in
   the models directory links it and downloads nothing; two installs of entries sharing a
   file at once download it once; and removing one installation keeps a file another
   holds. ADR 0017 says how the link is made and counted.
5. **The upgrade the owner runs.** `whizbang setup` writes the pack's new copy (the
   adapter that declares point and box) and keeps the installed runtime; then
   `whizbang models get sam3` finds every file held, links them and fetches nothing. The
   token is asked for only when a file must be downloaded, so this upgrade needs none.
6. **The command.** `whizbang rascal segment` takes exactly one of `--phrase`,
   `--point "X Y"` and `--box "L T R B"` (pixels of the image, as `segment` prints a
   box). The `segment` action takes the query as its argument, and its version moves, so
   a mask's origin names which was asked. A backend that does not declare the feature is
   refused when the call is prepared, and nothing runs.
7. **Pick.** `whizbang rascal select` gains `--run <id>` (the masks of a `segment` run,
   in the order it printed them), `--pick N` (the Nth of those, from 1; once for each)
   and `--at "X Y"` (those whose core holds that pixel), beside `--mask` and `--exclude`.
   A pick past the run's masks, or a point that no mask holds, is refused naming what
   the run has.
8. **The guide.** `docs/guides/models.md` gains the three commands of a cut-out with a
   point, as the owner would type them.
9. **Evidence, once, on the Mac.** `just test-models` asks SAM 3 a point and a box on a
   synthetic picture made for the test (no photograph), and the record of
   `scripts/model-evidence.sh` gains their lines; taken on a clean head.

- [x] Done when:
  `test_sam3.Prompts.test_a_point_is_asked_in_the_pictures_frame_and_answers_its_candidates_best_first`,
  `test_sam3.Prompts.test_a_box_is_asked_as_its_corners_and_answers_its_candidates_best_first`,
  `test_sam3.Prompts.test_the_tracker_is_loaded_once_on_the_first_point_or_box`,
  `test_sam3.Prompts.test_a_point_outside_the_picture_is_refused_as_unfit`,
  the conformance suite's runs of `point` and `box` against the fake models,
  `models::the_catalogues_sam3_declares_phrase_point_and_box`,
  `install::a_file_already_held_by_another_installation_is_linked_and_not_downloaded`,
  `install::two_installs_sharing_a_file_at_once_download_it_once`,
  `install::an_install_whose_files_are_all_held_asks_for_no_token`,
  `setup::setup_again_writes_the_packs_new_adapter_and_keeps_the_runtime`,
  `descriptor::a_requirement_chosen_by_an_argument_is_checked_at_prepare_and_at_admission`,
  `prepare::a_phrase_only_backend_answers_a_phrase_and_is_refused_a_point`,
  `remove::removing_one_of_two_installations_sharing_a_file_keeps_the_file`,
  `segment::a_point_answered_with_three_candidates_writes_three_masks_best_first`,
  `segment::a_backend_lacking_point_or_box_is_refused_at_prepare`,
  `segment::a_masks_origin_names_the_query_asked`,
  `cli::rascal_segment_takes_exactly_one_of_phrase_point_and_box`,
  `cli::rascal_segment_with_a_box_prints_each_masks_identity_and_score_under_a_fake_segmenter`,
  `select::the_run_and_pick_keep_the_nth_mask_of_a_segment_run`,
  `select::at_keeps_the_masks_whose_core_holds_the_pixel`,
  `select::a_pick_past_the_runs_masks_is_refused_naming_the_count`,
  `models::sam3_answers_a_point_and_a_box_on_the_mac` (`#[ignore]`, green under
  `just test-models` on the Mac),
  `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`
  on the retaken record, `just docs` regenerated, and the carry-over rows on
  shared files and on resizing before the cut closed.
  **Reworded with the task:** "the conformance suite's runs of `point` and `box`
  against the fake models" is
  `pack::conformance::the_real_pack_with_a_fake_model_answers_a_segment_request_whose_masks_are_those_the_request_determines`,
  which asks a phrase, a point and a box; "`install::a_file_already_held…`" and
  "`install::two_installs_sharing…`" are `models::install::…` of
  `whizbang-backends`; "`install::an_install_whose_files_are_all_held_asks_for_no_token`"
  is `models::an_install_whose_files_are_all_held_asks_for_no_token` of `whizbang`,
  where `get` reads the token; "`remove::…`" is `models::remove::…`; and
  "`models::sam3_answers_a_point_and_a_box_on_the_mac`" is
  `model_tier::sam3_answers_a_point_and_a_box_on_the_mac`, since `just test-models`
  runs `model_tier::` alone. Each was named by the plan's module where the test now
  lives under another.

### What the owner provides (Q)

Nothing new: SAM 3 as installed. The owner runs `whizbang setup` and then `whizbang
models get sam3` once Q1 lands; no file is fetched and no token is asked for.
**Origins (Q): none.**

## T1. Tiny and the `add-adapter` skill (large)

### Goal

A second catalogue entry, `tiny` (OWLv2 finds boxes for a phrase, SAM 2.1 Hiera-Tiny
draws each mask), installs and answers on the Mac, and passes the conformance suite for
the features it declares: `phrase`, `point` and `box`. It is added by following the
`add-adapter` skill, so the skill is written before its first use and corrected after
it, and adding it shows `SPEC.md` section 18, item 4: no edit to any existing tool's
logic.

1. **The skill.** `.claude/skills/add-adapter/SKILL.md` (`SPEC.md` section 14): the
   adapter, its catalogue entry, its conformance fixtures, the model test, the licence
   files and the origins to ask for; listed in `AGENTS.md`.
2. **The adapter `owlv2-sam2`** (a model name must be lower-case letters, digits and
   hyphens, so not rascal's `owlv2+sam2`) in the pack, on `Owlv2ForObjectDetection` and
   `Sam2Model` of the same `transformers` (5.23.0 in the lock; no new package). A phrase
   asks OWLv2 for boxes over the threshold, padded by `pad`, cut to `max_instances`,
   each drawn by SAM 2.1 with the `outline` rule and `min_cover` (rascal's Tiny, read
   under the clean-room rule of `AGENTS.md`; settings are the model's, sent in `load`, as
   ADR 0020 has them). A box goes to SAM 2.1 alone; a point too.
3. **The entry and its layout.** `tiny`, Apache-2.0 (both models), the Mac alone (as
   the lock is), its files pinned by commit revision and hash with `models pin`, its
   licence files installed beside them, no token. Both repositories hold a
   `config.json` and a `model.safetensors`, and a catalogue's file names are unique and
   hold no folder, so each file is installed under its model's prefix
   (`owlv2.config.json`, `sam2.model.safetensors`, and so on, `from` still the
   repository's own path), and the adapter builds each model from its config file and
   its weights file by name, with the hub's network off, never from a folder the
   library would look up.

- [ ] Done when:
  `test_tiny.Phrases.test_owlv2_boxes_over_the_threshold_are_padded_cut_and_drawn_in_the_fixed_order`,
  `test_tiny.Prompts.test_a_box_and_a_point_are_drawn_by_sam2_alone`,
  `test_tiny.Prompts.test_the_outline_rule_and_least_cover_are_the_models_settings`,
  `test_tiny.Layout.test_both_models_load_from_their_prefixed_files_with_the_network_off`,
  the conformance suite green for `tiny`'s declared features against the fake models,
  `models::the_catalogues_tiny_is_pinned_licensed_and_needs_no_token`,
  `models::tiny_answers_a_phrase_a_point_and_a_box_on_the_mac` (`#[ignore]`, under
  `just test-models` on the Mac),
  `git diff --stat <base>..<head> -- crates/whizbang-rascal/src crates/whizbang-squint/src`
  empty, shown in the hand-back (no existing tool's logic edited),
  `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`
  on the retaken record, and the skill in `AGENTS.md`'s list.

### What the owner provides (T)

The answer to the first item under "What the owner must answer". Nothing else: Tiny
needs no sign-in.

### Origins (T)

`https://huggingface.co`, already in the gate for SAM 3, for the two public repositories
`google/owlv2-base-patch16` and `facebook/sam2.1-hiera-tiny`, pinned by commit; and
whichever Hugging Face storage host those repositories' large files are sent from on the
day they are pinned. The owner approved that storage host for SAM 3 on 2026-11-16 ("no
need to approve"); for Tiny it is asked again below, since the approval named SAM 3. If
either repository has no files `transformers` reads, the builder stops: another host for
Tiny's weights is a new origin and the owner's to approve. No upstream, no probe; CI
contacts nothing.

## U1. A user's weights and a user's adapter (medium)

The carry-over row: "A user's weights are installed and run under a known adapter from
config alone, and an adapter a user wrote passes the conformance test the skill gives
them, each with a test against a fake model."

1. **The worker finds an adapter by the installation, with `pack/1` unchanged.** Today
   `worker.py`'s table is keyed by the model's name, so an entry of another name cannot
   reuse `sam3`. The worker already knows the models directory it was launched with; on
   `load` it reads the admitted installation's record there (the entry's `adapter`, and
   for a user's adapter its hash) and picks the adapter by that. `load` keeps its fields
   and `pack/1` its version (ADR 0020: a new field is a new version, and both codecs
   refuse an unknown one).
2. **A user's weights.** A user's catalogue entry, a file in the models directory read
   by the same reader as the binary's (the same names, origins and hash rules; an
   origin is used only if the config's egress allows it), naming a built-in adapter.
   `whizbang models get <name>` installs it and the kind `pack` runs it.
3. **A user's adapter: one registry, read by the binary and the worker.**
   `whizbang models adapter add <file> --name <name>` copies one Python file into the
   models directory, outside the pack that `whizbang setup` refreshes, as an owner-only,
   read-only copy named by its hash, and records `user-<name>` and that hash in the
   directory's adapter registry. The registry is what the installer
   (`model_commands::revision_of`), the inventory (`Installations::ready`, so no
   `NoAdapter`) and the worker read: an installation records the hash of the adapter it
   was installed with, and the worker checks the copy's hash against the admitted
   installation's record before it imports it, refusing a mismatch. Adding a changed
   file under the same name is a new hash and a new revision; an earlier installation
   keeps its copy until it is removed, so a call already admitted runs the code it was
   admitted with. ADR 0017 and ADR 0020 say how.

- [ ] Done when:
  `test_worker.Adapters.test_the_adapter_is_the_one_the_installations_record_names`,
  the `pack/1` conformance vectors unchanged,
  `models::a_users_entry_for_a_known_adapter_installs_and_runs_from_config_alone`,
  `models::a_users_entry_naming_an_origin_the_config_does_not_allow_is_refused`,
  `cli::a_users_adapter_is_added_its_model_installed_and_a_segment_runs_through_admission_against_a_fake_model`,
  `models::a_users_adapter_survives_setup_again`,
  `test_worker.Adapters.test_an_adapter_copy_whose_hash_is_not_the_installations_is_refused_before_import`,
  `models::an_adapter_replaced_while_a_call_is_admitted_leaves_that_call_on_the_earlier_copy`,
  `models::a_users_adapter_passes_the_conformance_suite_against_a_fake_model`, and the
  carry-over row closed.

## V1. Parity, the tolerance, timings, the run's diagnostics and the note (large)

0. **The device, a setting of the model.** The pack's settings gain `device` (`auto`,
   the default, or `cpu`), sent in `load` among the model's scalars (ADR 0020), so the
   same adapter can be asked on the Mac's CPU for the tolerance below; tests and
   evidence use it, CI runs no model.

1. **Parity fixtures.** `fixtures/parity/segment/`: for SAM 3 and Tiny, a handful of
   synthetic pictures made for the purpose and rascal's `chair` fixture (ported by
   `port-module`), each with a phrase, a point and a box, and the instances the model
   answered on the Mac's GPU: each mask as a PNG plane, its score and box, the model's
   revision and the adapter's.
2. **The mask-agreement tolerance.** Two answers agree when they have the same count of
   instances and, paired in order, each pair's IoU is at least the tolerance and its
   score within a stated margin. The tolerance is measured, not chosen: each fixture is
   asked three times on the GPU and three times on the Mac's CPU, and the tolerance is
   the worst agreement found, rounded down to 0.01. If that is under 0.90, the builder
   stops and says so. The tolerance and its measurement are recorded in an ADR; it is
   the bar a remote worker meets in milestone 6 (`SPEC.md` section 18, item 3).
3. **A backend's diagnostics and predicted IoU to its run (carry-over row on what an
   adapter gave).** A segmenter answers its diagnostics beside its response, one way for
   every kind; the `segment` action writes them and each instance's predicted IoU into
   the run's diagnostic, and `whizbang runs show` prints them. The core's `Instance`
   stays as it is (P3b's decision).
4. **Timings (carry-over row on the speed of local models).** On the Mac, under
   `scripts/memory-watch.sh`: the first load and a second request of each of phrase,
   point and box, on SAM 3 and on Tiny, and the peak memory with SAM 3's two models
   resident, in `docs/results/m1d-segment-coverage.md` as fields: the commit, the
   models' revisions, each time, the memory. No picture of the owner's.
5. **The note.** `docs/plans/m1d-note.md`, in the form of `m1c-note.md`: each clause of
   row 1D with the tests that show it, what a reader should not assume, what moved to
   the carry-over list, and what milestone 2 and 3 start from.

- [ ] Done when:
  `pack::a_device_the_kind_does_not_know_is_refused_when_the_config_is_read`,
  `test_sam3.Device.test_a_device_of_cpu_runs_on_the_cpu_and_says_so`,
  `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`
  on the retaken record,
  `parity::agreement_is_the_count_and_each_pairs_iou_and_score`,
  `parity::an_answer_one_instance_short_does_not_agree`,
  `parity::the_committed_fixtures_name_the_revisions_the_catalogue_holds`,
  `parity::sam3_and_tiny_answer_each_fixture_inside_the_tolerance_on_mps_and_on_cpu`
  (`#[ignore]`, under `just test-models` on the Mac, asserting each answer's reported
  device),
  `segment::the_backends_diagnostics_and_each_predicted_iou_are_in_the_runs_diagnostic`,
  `pack::the_diagnostics_a_worker_answered_reach_the_segmenters_caller`,
  `evidence::the_coverage_record_names_a_commit_of_this_tree_and_every_timing`,
  `scripts/tests/test-notes.sh` reading `m1d-note.md`, and the carry-over rows on what
  an adapter gave and on speed closed or re-owned with a line.

## What the owner must answer

1. I need Hugging Face's storage host approved for Tiny's files (public repositories,
   Apache-2.0, no sign-in), as you approved it for SAM 3 on 2026-11-16: yes/no.
2. I need to know if Linux comes into 1D on the CPU, with `download.pytorch.org` as a new
   origin for a CPU-only `torch` (no NVIDIA package): yes/no. No leaves Linux out of 1D,
   on its carry-over row.
3. I need, only if F1 cannot bring CI on Linux under 10 minutes by splitting a job, the
   budget of `check (ubuntu-22.04)` raised to 15 minutes: yes/no.

## Risks

1. **Two SAM 3 models resident.** The tracker beside the phrase model may double what
   the worker holds. It is loaded only on the first point or box; V1 measures the peak
   under `memory-watch.sh`, and if it passes the watermark the adapter unloads one
   before loading the other, as a decision in ADR 0022.
2. **The tracker's weights.** ADR 0022 says `Sam3TrackerModel` reads the same files.
   If a key is missing, Q1 stops and says so: a second file is a download and its origin
   is the owner's.
3. **Tiny's numbers differ from rascal's.** Rascal's Tiny ran ONNX exports; this one runs
   PyTorch. The conformance suite holds the contract, not rascal's pixels; parity is
   against the fixtures V1 records, not against rascal's.
4. **GPU answers wander.** If the GPU's answers differ run to run beyond the CPU's, the
   tolerance is wide; V1 stops under 0.90 rather than record a bar that holds nothing.

### What milestone 2, 3 and 6 still owe after 1D

| Item | Left for |
|---|---|
| `align` and the other squint tools; the squint follow-ups not listed in F1 | Milestone 2 |
| The tool records of `segment`, `select`, `plug`, `feather`, `render` (carry-over row) | Milestone 3 |
| A box as an exemplar of SAM 3 (find more like this) | Milestone 3, if a recipe needs it |
| Parity of a remote worker inside the 1D tolerance | Milestone 6 |
| The runtime on Linux, on a no to the second item | Its carry-over row |
| Weights hosted by the owner for customers | When the owner asks; 1D fetches from `huggingface.co` |

## Review rounds

Three outside rounds, kept as counts. The first, on draft 1, had 7 findings, all major
(4 on what was not buildable as written: the descriptor's fixed features, the upgrade of
SAM 3, Tiny's layout and name, a user's adapter; 1 on the key; 1 on the count of pull
requests; 1 on the CPU runs of the tolerance). The second, on draft 2, had 3, all major,
all on U1 (the wire of `pack/1`, the registry the binary reads, the adapter's copy). The
third, on draft 3, had 2, both major (the reason's items through the record; the evidence
record retaken by each pull request that changes it). None was a blocker. All are taken
into draft 4.
