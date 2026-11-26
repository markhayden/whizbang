# 0052. The fixed-class segmenter: EoMT on ADE20K as a catalogue entry, the feature `labels`, and a model named by a tool

Status: accepted. Changes ADR 0006: a segment backend may declare `labels`, which no request
needs by what it holds. Changes ADR 0032: a tool's ask may name a model and may ask for
`labels`. Changes ADR 0017: the shipped catalogue gains a third model.
Date: 2026-11-25
Spec: section 4, "Segmenters" and "Experimental tools"; section 16, row E; section 18,
criterion 4; plan mE, task T2, items 1 to 3

## Context

Row E brings `regions`, `classes` and `outline` in as experimental tools, "on SAM 3 where
that works and on their old segmenter as a catalogue entry where it does not". Task T1 put
`outline` and `regions` on SAM 3's phrase requests. Two things SAM 3 cannot do: `classes`
asks which classes cover a box, on each side of a pair, and `regions`' class disagreement
asks where the class of a pixel changed. Both need a segmenter that labels every pixel of a
picture from a closed list. That was squint's segmenter: EoMT large trained on ADE20K at
512 (squint, `tools/python/MODEL.md` and `src/squint_tools/segmenter.py`, at the commit
`docs/sources.toml` pins), exported by squint to ONNX and run on the CPU.

What binds the choice: a model is one adapter and one catalogue entry, added with no edit
to an existing tool's logic (section 4; section 18, criterion 4); a new origin is the
owner's; `pack/1` is a version, and a new kind of request would be `pack/2` (ADR 0020),
which the GPU host (ADR 0046) speaks too; a tool file reaches models only through its
context, as nested calls (ADR 0008, ADR 0032).

## Decision

### The model and its entry

`eomt-ade20k`, the adapter of the same name (`pack/src/whizbang_pack/eomt_ade20k.py`,
revision `1`), from the upstream safetensors of `tue-mps/ade20k_semantic_eomt_large_512` at
the commit squint pinned, `aeb262f5a0bc609236d27f1277c3173ce57aeb27`, through PyTorch and
`transformers` (`EomtForUniversalSegmentation`) on the lock the pack has. Not squint's ONNX
export: the pack has one runtime (ADR 0022), and the export was never published. The entry
holds `model.safetensors` (1,264,130,192 bytes, its SHA-256 the listing's, the one squint's
export script held the same weights to), `config.json` and the model card `README.md`,
which states the licence, MIT; the repository holds no licence file, and the backbone's
DINOv2 weights it was trained from are Apache-2.0. The origins are the Hub and
`us.aws.cdn.hf.co`, those SAM 3's entry already uses: no new origin. It runs on
`macos-arm64`, as the lock does, on the GPU there.

The adapter does what squint's did: the picture's longer edge resized to 512, the shorter
padded with its mean colour, ImageNet's normalisation; each class's score at a pixel the sum
over the model's queries of the query's class probability (without the no-object column)
times its mask's sigmoid; the pixel's class the highest, its confidence that class's share
of the summed scores. Here the scores are resized to the input before the choice and the
choice is brought to the picture's own size, so every pixel of the picture has a class:
squint read them at a quarter of the input. The load holds the installation's
`config.json` to the adapter's list of 150 classes, or refuses.

**It is a catalogue entry, not the default.** SAM 3 answers any phrase; this model answers
150 of them and no point or box. It is asked only when something names it.

### The feature `labels`

A segment backend that declares `labels` beside `phrase` is a fixed-class segmenter: it
labels every pixel of a picture with exactly one class of a closed list; it answers a
phrase that is one of its classes with at most one instance, the pixels so labelled, scored
by the class's mean confidence there; and it refuses any other phrase, `rejected`, naming
where the list is (`whizbang_pack.eomt_ade20k.CLASSES`). So the planes of every class of one
picture are disjoint and cover it.

`labels` is no kind of query, and `pack/1` is unchanged: a picture's labels are the answers
to its classes as phrases, at one instance each, in requests of at most 64. The adapter keeps
the last picture's labels, by its binding and size, so the requests of one picture compute
it once. The conformance suite asks an adapter that declares `labels` its first class as its
phrase, and then every class of its list, and holds the planes to be disjoint and to cover
the picture.

**The action `labels`** (`whizbang_rascal::labels`, version 1.0.0) asks them: an image and
1 to 255 classes, one a line, of one backend chosen by `backend` or the default, whose need
is `segment` with `phrase` and `labels`, so a backend that declares no `labels` is not ready
for it and nothing runs (ADR 0006, "Resolution"). It commits a mask for each class some pixel
has, in the order named, and hands back the classes with each one's confidence and count of
pixels as its measurements. A mask names its class as its source, so it is not the mask
`segment` makes of the same pixels, which the store would refuse a second action's
committing. An answer that labels a pixel twice fails the run.

### A tool's reach

A tool's ask (`tool/1`) may name `model`, a model of the catalogue, and may be of the
capability `labels`, with its `classes`. Whizbang routes a named model to the segment backend
of the config whose `model` is that name: the default when it is one of them, and otherwise
the first by name; none refuses the ask, saying so. The ask is then a nested call of
`segment` or of `labels` with that `backend`, refused as the call is: a backend that is not
ready (a model not installed, not in the catalogue) or does not declare the feature. A
`labels` answer is one plane of the image's size, a byte a pixel, `k` where the `k`th class
of the answer's list labels it and 0 where none asked does, with each class's confidence:
the tool's context gives it as `Labels`. A tool file names `segment` and `labels` among the
calls its run is held to.

## Consequences

- `classes`, and the methods `eomt` of `regions` and `outline`, can be written as tool files
  on this entry (task T2's second pull request).
- The model tier gains one test (`model_tier::eomt_labels_every_pixel_of_the_fixture_on_the_mac`),
  so `just test-models`, the candidate's copy of the models and a release's local leg need
  `eomt-ade20k` installed in the models directory: about 1.3 GB more.
- Under `check`, only `segment` is frozen beside a run's tool files (`check.rs`); a tool file
  that asks for labels there is refused until the run freezes `labels` too, with the tools
  that use it.
- A user's adapter may declare `labels` and `classes`, and is held to them by the suite.

## Alternatives

- **A `labels` request in the protocol, one answer of a class per pixel.** Turned down: a new
  kind of request is `pack/2` (ADR 0020), which the remote worker and the GPU host would have
  to speak at once, for no answer that phrases cannot carry.
- **Squint's ONNX export on onnxruntime.** Turned down: a second runtime in the pack, and an
  artifact that was never published, so a new origin.
- **The class list in the catalogue.** Turned down: the list is the model's (its
  `config.json`), and the adapter holds the installation to it; a tool names the classes it
  asks for.
- **A tool's labels as 150 asks of `segment`.** Turned down: a run asks at most 64 times, and
  each would be a run of its own.
