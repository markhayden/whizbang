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

The `remove-prompt` of item 8 waits for R1's recipe; its block is added then. No reply and
no header is kept.

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
