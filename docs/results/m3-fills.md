# Fills through fal: the live call of task F1

Plan m3, task F1, item 8: one `whizbang rascal fill` by hand, on the owner's Mac, through
the debug binary built from the commit below with F1's first pull request's changes on it
(`cargo build -p whizbang`, the binary target, so with no feature of the tests), under a
config that names fal's filler alone (`[backends.fill]`, `kind = "service"`, `provider =
"fal"`, `model = "bria-eraser"`, `key_env = "FAL_KEY"`), `call_cap_usd = 0.10`, and one
approved recipient, `https://fal.run`, for `source_images` and `derived_images`; with the
owner's env file given by `--env-file`, in a store made for it with `whizbang spend reset
--allowance 0.20`.

**The picture** was made for the call, not taken: a synthetic drawing of 256 by 192, a red
square of 60 by 60 on a blue gradient, and a mask whose core is a block of 68 by 68 over the
square. Neither is a photograph, and neither is committed. The call was made once and
answered on the first attempt; the filled picture shows the gradient where the square was.
Every pixel outside the mask's support is the original's (all 44,528; the 4,624 inside it
were painted), counted from the two exported files, and held by the executor before the
run committed (`derived_from`).

The `remove-prompt` of item 8 is the second block, below, made with F1's second pull
request. No reply and no header is kept.

## fill

```toml
commit = "630304854cde373d07f3fda38afd5f3b4d01e08c"
whizbang = "0.0.1"
action = "fill 1.0.0"
backend = "fal"
adapter_version = "1"
model = "bria-eraser"
model_version = "1+fit1-png-binary1-2048"
price_read = "2026-11-18"
run = "1795070854800-1"
outcome = "completed"
request_ms = "10135"
picture = "256x192"
painted_pixels = "4624"
outside_support_original = "true"
request_bound_micros = "40000"
reserved_micros = "40000"
settled_micros = "40000"
egress_recipient = "https://fal.run"
egress_classes = "source_images derived_images"
```

## remove-prompt

One `whizbang rascal remove-prompt --prompt "red circle"` by hand, on the owner's Mac, on 2026-11-19,
through the debug binary built from the commit below with F1's second pull request's
changes on it (`cargo build -p whizbang`), under a config that names SAM 3 in the pack for
segment (`kind = "pack"`, `model = "sam3"`, from a copy-on-write clone of the owner's
models directory) and fal's filler for fill as above, `call_cap_usd = 0.10`, the one
approved recipient `https://fal.run` for `source_images` and `derived_images`, the owner's
env file given by `--env-file`, in a store made for it with `whizbang spend reset
--allowance 0.20`; under `scripts/memory-watch.sh` and the thread guard at 200 (peak
resident 641,696 KiB, 13 threads in one process).

**The picture** was made for the call: a synthetic drawing of 256 by 192, a red disc of
radius 34 on a blue gradient over a grey floor; not a photograph, and not committed. The
same recipe with `--until feather` was run first, SAM 3 alone and nothing sent, to see that
a mask came back; then the whole recipe once, its eight steps each completed, its one fill
answered on the first attempt. The filled picture shows the gradient where the disc was.
Every pixel the mask weighs nothing is the original's (all 40,146; of the 9,006 the mask
weighs, 7,595 changed), counted from the files `store export` wrote of the two pictures and
of the mask (`--mask`, its grey picture).

```toml
commit = "890057bdd6ee8c2c7da2185411649172bfabc510"
whizbang = "0.0.1"
action = "remove_prompt 1.0.0"
recipe_digest = "sha256:956beac538e8cb09487015fc9b504d57dfa0f7adb37880292750bb8564b5a992"
steps = "8"
segmenter_model = "sam3"
segmenter_version = "sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652"
segmenter_device = "mps"
backend = "fal"
adapter_version = "1"
model = "bria-eraser"
model_version = "1+fit1-png-binary1-2048"
price_read = "2026-11-18"
run = "1795081901350-1"
fill_run = "1795081906083-9"
outcome = "completed"
run_ms = "15814"
request_ms = "10847"
picture = "256x192"
support_pixels = "9006"
outside_support_original = "true"
request_bound_micros = "40000"
reserved_micros = "40000"
settled_micros = "40000"
egress_recipient = "https://fal.run"
egress_classes = "source_images derived_images"
```
