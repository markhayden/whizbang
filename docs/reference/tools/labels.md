# labels

The tool record of `labels` (`SPEC.md` sections 4 and 8; plan
`docs/plans/mE-evidence-and-edit-quality.md`, task T2; ADR 0052). It asks a fixed-class
segmenter, the catalogue's `eomt-ade20k`, which classes it labels each pixel of a picture
with. Its examples here are answered by the test kit's fake segmenter, declaring what a
fixed-class segmenter declares, and run as tests
(`crates/whizbang-rascal/tests/it/labels.rs`). The model's own answer on the fixture is the
model tier's (`model_tier::eomt_labels_every_pixel_of_the_fixture_on_the_mac`, on the Mac).

## What it does

Asks a segmenter that labels every pixel with one class of a closed list (the feature
`labels`) for the pixels of each class named. It commits one mask for each class some pixel
has, and hands back each class's confidence and count of pixels.

- **Use it** where a tool needs what is where in a picture by class: which classes cover a
  box, or where a pixel's class changed between two pictures. A tool file reaches it through
  its context (`context.labels`), which gives one plane of each pixel's class.
- **Do not use it** to find a thing by an open phrase: `segment` with SAM 3 does that. A
  fixed-class segmenter knows its classes alone, ADE20K's 150 for `eomt-ade20k`
  (`whizbang_pack.eomt_ade20k.CLASSES`).
- **Kind.** A model-based tool: all four rungs apply. The local pack's model charges nothing.

The action `labels`, version 1.0.0. No door has a command for it: a tool file asks it, and
a recipe or the engine API may call it.

| Name | Meaning |
|---|---|
| `image` (input) | The picture to label. A rendering is refused |
| `classes` | Required. The classes to ask, one a line, 1 to 255, none twice; each one of the segmenter's own, or the segmenter refuses the call |
| `backend` | Optional. The segmenter, by its name in the config; the default for segment when not given. It must declare `phrase` and `labels`, or the call is refused before anything runs |

**Output.** One mask at the port `mask` for each class some pixel has, in the order the
classes are named. Each is on the image's own binding, its core the class's pixels, its
origin naming `labels`, its version, the class and the image, and its source the class (the
digest of `labels class <name>`), so it is never the mask `segment` makes of the same
pixels. The run's measurements list the same classes, each its `name`, its `score` (the
segmenter's mean confidence over its pixels) and its `pixels`; its diagnostic `labels.txt`
says the same a line a class. The classes are asked in requests of at most 64, at one
instance each; the model computes the picture once for them.

## Examples

A picture of 24 by 16, which the fake labels `sky` on its top 10 rows and `floor` on the 6
below. Each mask is drawn three times its size, its core white.

| `sky`: 240 pixels | `floor`: 144 pixels |
|---|---|
| ![The top ten rows, white](labels/sky.png) | ![The bottom six rows, white](labels/floor.png) |

| Asked | Requests | Masks, in order | Measurements |
|---|---|---|---|
| 70 classes, `floor` fourth and `sky` last | 2 (64 and 6) | `floor` (144 pixels), `sky` (240) | each score 0.9 |
| `sky` and `wall`, where the fake labels a corner `wall` over `sky` | 1 | none: the run fails, since two classes label one pixel | none |
| `sky`, of a segmenter that declares no `labels` | 0 | none: the call is refused before any run exists | none |

What to look for: a class no pixel has (the 68 others) gives no mask; the masks of one
picture never overlap; a segmenter that is not a fixed-class one is never asked.

## Rung 2 results

Not scored here. The comparison of the catalogue's segmenters on the truth masks of
`segmentation/segment-01` is plan mE's task K1.

## Speed and cost

The local pack's model charges nothing. A call is at most 4 requests of the backend, for 255
classes; ADE20K's 150 are 3, and the model computes the picture once for them. Its time on
the Mac's GPU is the model tier's, written by its test (`model-tier/out/eomt.txt`) and not
yet recorded here.

## Alternatives

`segment` asks any segmenter one phrase, point or box, and answers several instances of
it; it says nothing of the pixels no instance covers. SAM 3 answers open phrases and no
closed list. A tool that needs one thing found asks `segment`; one that needs what covers
each pixel asks `labels`.

## Limits

| Limit | Example |
|---|---|
| The classes are the model's: a thing outside ADE20K's 150 is labelled as the nearest of them | No example here: the model tier's fixture is a grey ground, a red disc and a blue square |
| A phrase that is not one of the model's classes is refused | `eomt-ade20k` refuses `a red thing` (`test_eomt.Phrases`) |
| 255 classes a call at most | ADE20K's 150 is one call |
| The first call of the model loads it into the worker | About 1.3 GB of weights |

## Mark and history

**Experimental**: brought in by milestone E (`SPEC.md` section 16, row E: the old
segmenter of `regions`, `classes` and `outline` comes back as a catalogue entry). It runs
only when a call or a tool names it.

- 2026-11-25, plan mE task T2: the action, over the catalogue's new `eomt-ade20k`; this
  record's examples (rung 1), run against the fake segmenter.
