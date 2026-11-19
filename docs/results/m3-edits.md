# The edits' rung 2: task success and preservation

Plan m3, task B1, item 1 (`SPEC.md` section 8, "The ladder"). The synthetic edit set
(`labs/m3-edits/edits.toml`) was drawn by `whizbang eval edits draw` from the pictures
of the set below, each held to its job's pin: on each picture a shaded red ball with a
box (`remove`, and `remove_prompt` asked for the phrase below), a thin light scratch
with a stroke along it (`repair`), and a dark spot with a box (`repair_area`), the
drawn defect's exact mask kept as truth and the clean picture as the reference. Each
recipe ran as the lab `m3-edits-<recipe>` (`labs/m3-edits/`), and `whizbang eval edits
score` read its runs from the store. Only aggregates are kept, never an item.

- **Preservation**: pixels whose weight in the result's mask is 0 and which differ from
  the picture the recipe was given, summed over the items (0 is required).
- **Task success**: the IoU of the result's mask, where its weight is at least a half,
  with the drawn defect's mask (for `remove_prompt`, the whole recipe's shaped mask);
  and the fill's mean error per channel (0 to 255) inside the drawn defect's mask
  against the clean picture, beside that of a plain blur fill of the same mask.
  Beside the IoU, the defect's coverage (the share of the drawn defect inside the
  mask, where its weight is at least a half) and the mask's size as a multiple of
  the defect's: each recipe grows and feathers its mask on purpose, so IoU alone
  understates a mask that covers the whole defect.

## The run

```toml
commit = "e8cead96c078ab529dfa8a7b1330a5a2a5160789"
whizbang = "0.0.1"
set = "source-fidelity/scenes-v1"
seed = "20261119"
phrase = "a red ball"
pictures = "24"
items = "96"
settled_micros = "3840000"
unresolved_micros = "0"
```

## remove

```toml
recipe = "remove"
recipe_digest = "sha256:4255dc1c806237148ab4aebc45e0925aed9064a89eea44f3feb7862ef8691e7b"
constant_sets = "none"
lab = "m3-edits-remove"
protocol_digest = "sha256:0ed20dbc6987b08bf1a102421d284109f84b88a1f8fa66a01d7d5f13a57107b7"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "none"
items = "24"
completed = "24"
failed = "0"
not_run = "0"
changed_outside_mask = "0"
items_preserved = "24"
mask_iou_mean = "0.5450"
mask_iou_min = "0.4870"
coverage_mean = "1.0000"
coverage_min = "1.0000"
size_ratio_mean = "1.8425"
fill_error_mean = "29.1200"
blur_error_mean = "38.3773"
fill_beats_blur = "20"
settled_micros = "960000"
unresolved_micros = "0"
```

## remove_prompt

```toml
recipe = "remove_prompt"
recipe_digest = "sha256:956beac538e8cb09487015fc9b504d57dfa0f7adb37880292750bb8564b5a992"
constant_sets = "none"
lab = "m3-edits-remove_prompt"
protocol_digest = "sha256:65487642493271c25c8ee66830ec85f4b4d21817b9d24c98843174a9532ab48c"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "pack sam3 sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
items = "24"
completed = "24"
failed = "0"
not_run = "0"
changed_outside_mask = "0"
items_preserved = "24"
mask_iou_mean = "0.7156"
mask_iou_min = "0.6454"
coverage_mean = "1.0000"
coverage_min = "1.0000"
size_ratio_mean = "1.4029"
fill_error_mean = "28.4338"
blur_error_mean = "36.1880"
fill_beats_blur = "16"
settled_micros = "960000"
unresolved_micros = "0"
```

## repair

```toml
recipe = "repair"
recipe_digest = "sha256:4b0d8fe7c27cb074661460fa939ae690505f4c795e1442080b068e33d9fde0a6"
constant_sets = "tighten-v1 sha256:42a1e28957d1604cf063f5e8d60dc7fe45a81899548fdd411cc75f17ba4ac488"
lab = "m3-edits-repair"
protocol_digest = "sha256:2fd26292c60a3859f64d38eff44c7634644377a716278e9944919efc1e9baad9"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "none"
items = "24"
completed = "24"
failed = "0"
not_run = "0"
changed_outside_mask = "0"
items_preserved = "24"
mask_iou_mean = "0.2281"
mask_iou_min = "0.0991"
coverage_mean = "0.9958"
coverage_min = "0.9528"
size_ratio_mean = "5.4026"
fill_error_mean = "18.9367"
blur_error_mean = "23.6337"
fill_beats_blur = "15"
settled_micros = "960000"
unresolved_micros = "0"
```

## repair_area

```toml
recipe = "repair_area"
recipe_digest = "sha256:c973be2dc1d0f6b6d92e2b519ecd8d75fc625d79f85ed5d10a92c16ee4d56342"
constant_sets = "none"
lab = "m3-edits-repair_area"
protocol_digest = "sha256:dac24ed17d355417f36550f861149078939aa5837395a0963d353858c1b552e3"
filler = "fal bria-eraser 1+fit1-png-binary1-2048"
finder = "none"
items = "24"
completed = "24"
failed = "0"
not_run = "0"
changed_outside_mask = "0"
items_preserved = "24"
mask_iou_mean = "0.5904"
mask_iou_min = "0.5115"
coverage_mean = "1.0000"
coverage_min = "1.0000"
size_ratio_mean = "1.7004"
fill_error_mean = "27.1919"
blur_error_mean = "30.9960"
fill_beats_blur = "14"
settled_micros = "960000"
unresolved_micros = "0"
```
