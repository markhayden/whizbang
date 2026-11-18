---
name: add-adapter
description: "Add a model to whizbang: its adapter in the pack, its catalogue entry, its conformance run, its model test, its licence files and the origins to ask for. For the builder."
---

# add-adapter

For the builder, when a task adds a local model (`SPEC.md` section 5, "Segmenters": a model
is one adapter and one catalogue entry, with its conformance fixtures; adding one edits no
existing tool's logic). It is a checklist on top of `kickoff`: follow `kickoff` for the
worktree, the commits, the push, the pull request and CI, and this for what is specific to
a model. Written for task T1 of checkpoint 1D (`tiny`, the adapter `owlv2-sam2`) before it
was followed, and corrected after.

## Inputs

- **Model:** its name in the catalogue (lower-case letters, digits and hyphens) and its
  title.
- **Adapter:** its name (the same rule; the pack's module holds `_` where it has `-`), and
  whether it is new or an adapter the pack has.
- **Repositories:** where the weights are, each by its address and the commit to pin. A
  host not yet approved for the project is a new origin, and the owner's to approve.
- **Features:** which of `phrase`, `point` and `box` the adapter answers.
- And the three inputs of `kickoff`: task id, plan file, base commit.

If one is missing, ask the orchestrator. Do not guess.

## Checklist

- [ ] 1. Start with `kickoff`, steps 1 to 4.
- [ ] 2. **The origins first.** Every host a download of the files reaches is named in the
      plan's origins, with the owner's approval and its date. Read each repository's
      public listing at the commit, once, with no token; find where its large files are
      moved with one `HEAD` request (`curl -sS -I`), which downloads nothing. A host that
      is not approved stops the task: say so to the orchestrator.
- [ ] 3. **The licence.** Read each repository's licence: its licence file, or its model
      card's `license:` line where it has no file. State it in the hand-back. A licence that
      is not the one the plan expects stops the task. The file that states it is installed
      beside the weights (a `LICENSE`, or the card, `README.md`).
- [ ] 4. **The files.** Safetensors only: never a `.bin`, `.pt`, `.pth`, `.pkl` or `.ckpt`,
      which are pickles. Only the files the runtime reads, and the licence's. A
      catalogue's file names are unique and hold no folder, so where two repositories
      hold a file of one name, each file is named under its model's prefix
      (`owlv2.config.json`), its `from` still the repository's own path at the commit.
- [ ] 5. **The hashes.** Write a draft entry of one model (`catalogue/2`) whose sizes are
      those of the listing, and run `whizbang models pin <draft>` on it to fix each small
      file's SHA-256 from the bytes it fetches once. A large file's SHA-256 is the one the
      public listing states for it, when the listing states it, so the weights are
      downloaded once, by `get`, which checks them; where the listing masks it, `pin`
      fetches it. Record how each was fixed in the comment on `wiring::catalogue`.
- [ ] 6. **The entry** in `crates/whizbang/src/catalogue.json`: name, title, a revision,
      the adapter, the licence, the platforms the lock runs on, the features, the origins,
      and `needs_token` only for an origin whose files need a sign-in. The test of the
      entry names every field and the weights' sizes and hashes
      (`models::the_catalogues_<model>_…` in `crates/whizbang/tests/it/models.rs`); the
      tests that name the shipped catalogue's models, and the models guide's `list`, gain
      its line.
- [ ] 7. **The adapter**, tests first, in `pack/src/whizbang_pack/<adapter>.py` with
      `_` for `-`: the interface of `adapter.py`; its runtime in one class imported only at
      `load` (the core imports no runtime, `test_worker.Imports`; add its modules to
      `RUNTIME` there); its models built from the installation's files by name with the
      network off, never from a folder the library looks up. It refuses a point or a box
      outside the picture before anything is computed, cuts each query's candidates to
      `max_instances` before a full-size plane is made, and answers in the fixed order.
      Its tests (`pack/tests/test_<adapter>.py`) give it stand-ins for its runtime and
      fake modules for the library, and import neither.
- [ ] 8. **The tables.** The binary's table of adapters (`wiring::adapters`) names it at its
      revision, shipped; the worker names it in `REAL` and the model in `SHIPPED`
      (`pack/src/whizbang_pack/worker.py`); the pack the binary carries holds its module
      (`CARRIED` in `crates/whizbang/src/pack_setup.rs`).
- [ ] 9. **Conformance.** Its declared features, alone and together, through the worker,
      held by `kit.conforming` (`pack/tests/kit.py`) against its stand-ins. A user who
      writes an adapter is given the same suite: `python -m whizbang_pack.conformance
      <adapter.py> <files>` (`docs/guides/models.md`, "A model of your own").
- [ ] 10. **The model test**, `#[ignore]`d, in `crates/whizbang/tests/it/model_tier.rs`:
      the model asked each feature it declares about the fixture picture, through the
      binary's wiring, on the Mac's GPU. Install it once into a models directory of your
      own with `whizbang models get <model>` (a copy of one that has the pack, made with
      `cp -cR`), and run `just test-models` with `WHIZBANG_MODELS_DIR` naming it.
- [ ] 11. **The record.** Add the model's lines to `scripts/model-evidence.sh` and to
      `evidence::the_committed_record_names_the_adapters_revision_and_the_lock_the_tree_has`;
      commit, then retake the record on that clean head and commit it.
- [ ] 12. **No existing tool's logic is edited**: show the diff below empty in the hand-back.
      Update ADR 0017's shipped catalogue, the models guide and the carry-over list where
      the model changes them.
- [ ] 13. Finish with `kickoff`, steps 6 to its last. Never merge a pull request: only the
      orchestrator does.

## Commands

```sh
curl -sS "https://huggingface.co/api/models/<owner>/<repo>/tree/<commit>"
curl -sS -I "https://huggingface.co/<owner>/<repo>/resolve/<commit>/model.safetensors"
target/debug/whizbang models pin <draft.json>
target/debug/whizbang models get <model> --models-dir <your models directory>
just test-pack
just test-only -p whizbang -E 'test(/models::the_catalogues_/)'
WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh test-models scripts/test-models.sh
WHIZBANG_THREAD_LIMIT=200 scripts/heavy.sh model-evidence scripts/model-evidence.sh
git diff --stat <base>..HEAD -- crates/whizbang-rascal/src crates/whizbang-squint/src
```

## Hand-back format

```text
model <name>, adapter <adapter> revision <n>; features <list>
repositories: <owner/repo>@<commit>, ...; licence of each: <licence> (from <file>)
origins reached: <hosts>; approved by the owner on <date>
files: <count>, <bytes> in all; how each hash was fixed: <pin / listing>
downloaded once into a models directory of the task's own: yes / no
conformance: <test> green; model test: <test> green on the Mac (<device>)
record retaken on <commit>
no existing tool's logic edited: <the diff above, empty>
```
