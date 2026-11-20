# The release of 0.1.0

Plan m4, task N1, item 1. The first version of whizbang, `0.1.0`, published on the
repository's GitHub releases on 2026-11-20 and marked there as a pre-release (the owner's
decision 1 of plan m4). This record was written after the release, from outside it: it
lands on `main` after the released commit and is in no archive of `0.1.0`. Everything
below was read from the runs, the candidate and the release by the orchestrator, who alone
tags and releases (`.claude/skills/release/SKILL.md`).

## The release

```toml
version = "0.1.0"
version_tag = "v0.1.0"
commit = "3ee263a82ffe45f343aaf4e81955e0b2cae33363"
candidate = "v0.1.0-rc.1"
candidates = 1
ci_run = "44776170549"
prerelease_run = "44779028704"
release_run = "44780876409"
receipt_sha256 = "a82f037418ddc0f36ae3e184b731c2c3ebba82f41120e7e1473ebe485b8afaff"
published = "pre-release"
```

The commit is the merge of #257 (plan m4, task E1) on `main`. The version tag and the one
candidate are both on it.

## Each candidate tagged

| Candidate | Commit | How it ended |
|---|---|---|
| `v0.1.0-rc.1` | `3ee263a82ffe45f343aaf4e81955e0b2cae33363` | passed: the `prerelease` workflow (run 44779028704) and the local leg, whose receipt is attached to the candidate; released as `v0.1.0` |

There were no earlier candidates of `0.1.0`, so no candidate failed and no number was
skipped.

## Before the candidate's tag

- **CI on the commit:** `main`'s run 44776170549, all eight checks passed: `check`,
  `release` and `scripts` on `macos-15` and on `ubuntu-22.04`, `secrets`, and the
  aggregate `ci`.
- **The scans:** `just scan-range m3 3ee263a82ffe45f343aaf4e81955e0b2cae33363` was clean on
  secrets, attribution and the clean room. `m3` is the tag of the last milestone, since
  there was no version tag before this one (the skill `release`, step 2).

## The `prerelease` workflow

Run 44779028704, on the tag `v0.1.0-rc.1`, succeeded.

| Job | Runner | Result |
|---|---|---|
| `build` | `macos-15` | success |
| `build` | `ubuntu-22.04` | success |
| `install` | `macos-15` | success |
| `install` | `ubuntu-22.04` | success |
| `candidate` | `ubuntu-22.04` | success |

`build` made each target's archive with `just package`; `install` installed it on a fresh
runner with the bootstrap and the install check, with no Rust toolchain and no build; and
`candidate` made the draft pre-release `v0.1.0-rc.1`, holding both archives and
`SHA256SUMS`.

## The archives

The candidate's `SHA256SUMS`, which is also the release's, byte for byte:

```text
08acead1718665780dc6a7a5547071cf08e857cb6d688d366a12760f6b331754  whizbang-0.1.0-aarch64-apple-darwin.tar.gz
2bb4f7e6ace9f2e5dc6a9040a8e837467a1b7f50135e380b569397aac7aad566  whizbang-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

The Mac's archive is for macOS 15 or later on Apple silicon, and its binary is not signed
with a developer identity or notarised by Apple (the owner's decision 3 of plan m4). The
Linux archive is for Ubuntu 22.04 or later on x86-64, and runs no local model there
(`SPEC.md` section 5).

## The local leg

`scripts/prerelease-local.sh` ran on the owner's Mac against the candidate's own Mac
archive, from a new clean checkout of the tag, in a new store with the decision cache off.
Every required check passed, the receipt passed `scripts/receipt-check.sh`, and the leg
attached it to the candidate. The model tests ran 11 of 11, all passed.

| Check | What the receipt holds |
|---|---|
| `model-tests` | exit 0; a new, clean checkout of the commit, tree `cfe12ba394bb77108cfd6773f28e1f85ce1af0d6`; the 11 tests the filter names, all run, none failed, none timed out |
| `setup` | exit 0; the extra `torch`; the worker's hello (device `cpu`, 3 threads), and heard again by the probe |
| `mask` | completed; `pack` `sam3`, on `mps` |
| `fill` | completed; `fal` `bria-eraser`, not from a cache; one request answered; 0.040000 dollars settled |
| `check-anthropic` | completed; `claude-haiku-5-5`, not from a cache; 3 requests answered; verdict `unsure`; 0.002555 dollars settled |
| `check-google` | completed; `gemini-3.8-flash`, not from a cache; 4 requests answered; verdict `pass`; 0.012586 dollars settled |
| `check-openai` | completed; `gpt-6.1-terra`, not from a cache; 2 requests answered; verdict `pass`; 0.004548 dollars settled |
| `loop` | completed; its plan checks; the kept attempt, the first, checked, verdict `unsure`; its check completed, not from a cache, with 5 requests to `anthropic` answered, 0.004130 dollars settled by their lines |

The receipt itself, as it was attached to the candidate (SHA-256
`a82f037418ddc0f36ae3e184b731c2c3ebba82f41120e7e1473ebe485b8afaff`). It holds no reply
text, no path and no key; its `run` fields are the leg's own store's run identities.

```json
{
  "schema": "prerelease-receipt/1",
  "tag": "v0.1.0-rc.1",
  "version": "0.1.0",
  "candidate": 1,
  "commit": "3ee263a82ffe45f343aaf4e81955e0b2cae33363",
  "archives": {
    "aarch64-apple-darwin": "08acead1718665780dc6a7a5547071cf08e857cb6d688d366a12760f6b331754",
    "x86_64-unknown-linux-gnu": "2bb4f7e6ace9f2e5dc6a9040a8e837467a1b7f50135e380b569397aac7aad566"
  },
  "binary": {
    "sha256": "7a8ff6a7889e481400bab802c4b3a341da8071a123c42d537306cdb3993c0e71",
    "version_line": "whizbang 0.1.0 (commit 3ee263a82ffe45f343aaf4e81955e0b2cae33363)"
  },
  "store": {
    "new": true,
    "cache": "off"
  },
  "checks": [
    {
      "name": "model-tests",
      "exit": 0,
      "checkout": {
        "new": true,
        "clean": true,
        "commit": "3ee263a82ffe45f343aaf4e81955e0b2cae33363",
        "tree": "cfe12ba394bb77108cfd6773f28e1f85ce1af0d6"
      },
      "expected": [
        "coverage::the_first_load_and_a_second_request_of_each_query_on_sam3_and_tiny",
        "model_tier::a_located_retry_finds_the_phrase_inside_a_crop_with_sam3",
        "model_tier::a_second_request_for_the_same_model_loads_nothing",
        "model_tier::per_subject_comparison_answers_the_records_examples_on_the_mac",
        "model_tier::remove_checked_makes_its_mask_with_sam3_under_a_scripted_judge",
        "model_tier::remove_prompt_makes_a_mask_with_sam3_on_the_mac",
        "model_tier::sam3_answers_a_phrase_on_the_gpu_with_an_instance_inside_the_fixtures_box_and_the_record_names_the_device",
        "model_tier::sam3_answers_a_point_and_a_box_on_the_mac",
        "model_tier::segment_answers_the_records_examples_on_the_mac",
        "model_tier::tiny_answers_a_phrase_a_point_and_a_box_on_the_mac",
        "parity::sam3_and_tiny_answer_each_fixture_inside_the_tolerance_on_mps_and_on_cpu"
      ],
      "ran": [
        "coverage::the_first_load_and_a_second_request_of_each_query_on_sam3_and_tiny",
        "model_tier::a_located_retry_finds_the_phrase_inside_a_crop_with_sam3",
        "model_tier::a_second_request_for_the_same_model_loads_nothing",
        "model_tier::per_subject_comparison_answers_the_records_examples_on_the_mac",
        "model_tier::remove_checked_makes_its_mask_with_sam3_under_a_scripted_judge",
        "model_tier::remove_prompt_makes_a_mask_with_sam3_on_the_mac",
        "model_tier::sam3_answers_a_phrase_on_the_gpu_with_an_instance_inside_the_fixtures_box_and_the_record_names_the_device",
        "model_tier::sam3_answers_a_point_and_a_box_on_the_mac",
        "model_tier::segment_answers_the_records_examples_on_the_mac",
        "model_tier::tiny_answers_a_phrase_a_point_and_a_box_on_the_mac",
        "parity::sam3_and_tiny_answer_each_fixture_inside_the_tolerance_on_mps_and_on_cpu"
      ],
      "failed": [],
      "timed_out": [],
      "report_sha256": "6dbde5918802d96d7f4640195180f89d91823ee3c837f36ec1f7881b6c7c8822"
    },
    {
      "name": "setup",
      "exit": 0,
      "extra": "torch",
      "hello": {
        "device": "cpu",
        "threads": 3
      },
      "probe_hello": true
    },
    {
      "name": "mask",
      "exit": 0,
      "run": "1795194517480-1",
      "status": "completed",
      "backend": "pack",
      "model": "sam3",
      "model_version": "sha256:4c1428553b40dcc493f53221fb1134c487432e43f3a39b5a8a5ab5c60f3ed652",
      "device": "mps"
    },
    {
      "name": "fill",
      "exit": 0,
      "run": "1795194523636-1",
      "status": "completed",
      "backend": "fal",
      "model": "bria-eraser",
      "model_version": "1+fit1-png-binary1-2048",
      "cache_hit": false,
      "requests": [
        {
          "backend": "fal",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 40000
        }
      ],
      "sent_to": [
        "https://fal.run"
      ],
      "tokens": null,
      "settled_micros": 40000
    },
    {
      "name": "check-anthropic",
      "exit": 0,
      "run": "1795194539068-1",
      "status": "completed",
      "backend": "anthropic",
      "model": "claude-haiku-5-5",
      "model_version": "1+fit1-jpeg90-2576",
      "cache_hit": false,
      "requests": [
        {
          "backend": "anthropic",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 661
        },
        {
          "backend": "anthropic",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 934
        },
        {
          "backend": "anthropic",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 960
        }
      ],
      "sent_to": [
        "https://api.anthropic.com"
      ],
      "tokens": {
        "input": 15163,
        "output": 2075
      },
      "settled_micros": 2555,
      "verdict": "unsure"
    },
    {
      "name": "check-google",
      "exit": 0,
      "run": "1795194549847-1",
      "status": "completed",
      "backend": "google",
      "model": "gemini-3.8-flash",
      "model_version": "1+fit1-jpeg90-1536",
      "cache_hit": false,
      "requests": [
        {
          "backend": "google",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 4427
        },
        {
          "backend": "google",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 2353
        },
        {
          "backend": "google",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 2565
        },
        {
          "backend": "google",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 3241
        }
      ],
      "sent_to": [
        "https://generativelanguage.googleapis.com"
      ],
      "tokens": {
        "input": 12217,
        "output": 544
      },
      "settled_micros": 12586,
      "verdict": "pass"
    },
    {
      "name": "check-openai",
      "exit": 0,
      "run": "1795194570602-1",
      "status": "completed",
      "backend": "openai",
      "model": "gpt-6.1-terra",
      "model_version": "1+fit1-jpeg90-2048",
      "cache_hit": false,
      "requests": [
        {
          "backend": "openai",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 2785
        },
        {
          "backend": "openai",
          "ended": "answered",
          "stage": "answered",
          "settled_micros": 1763
        }
      ],
      "sent_to": [
        "https://api.openai.com"
      ],
      "tokens": {
        "input": 2616,
        "output": 364
      },
      "settled_micros": 4548,
      "verdict": "pass"
    },
    {
      "name": "loop",
      "exit": 0,
      "run": "1795194579900-1",
      "status": "completed",
      "plan_checks": true,
      "kept": {
        "attempt": 1,
        "verdict": "unsure",
        "unchecked": null
      },
      "check": {
        "run": "1795194595807-15",
        "status": "completed",
        "verdict": "unsure",
        "cache_hit": false,
        "requests": [
          {
            "backend": "anthropic",
            "ended": "answered",
            "stage": "answered",
            "settled_micros": 565
          },
          {
            "backend": "anthropic",
            "ended": "answered",
            "stage": "answered",
            "settled_micros": 612
          },
          {
            "backend": "anthropic",
            "ended": "answered",
            "stage": "answered",
            "settled_micros": 572
          },
          {
            "backend": "anthropic",
            "ended": "answered",
            "stage": "answered",
            "settled_micros": 1159
          },
          {
            "backend": "anthropic",
            "ended": "answered",
            "stage": "answered",
            "settled_micros": 1222
          }
        ],
        "sent_to": [
          "https://api.anthropic.com"
        ]
      }
    }
  ]
}
```

## The `release` workflow, and what was published

Run 44780876409, on the tag `v0.1.0`, succeeded. Its gate, `scripts/release-gate.sh`,
printed:

```
release-gate: passed: v0.1.0 is of commit 3ee263a8…, whose candidate v0.1.0-rc.1 passed the prerelease workflow and the local leg
```

It then published the candidate's own two archives and `SHA256SUMS` under `v0.1.0`,
building nothing. The release
(`https://github.com/markhayden/whizbang/releases/tag/v0.1.0`) is not a draft and is
marked as a pre-release. Its files were downloaded into a new folder and
`scripts/release-publish.sh check` passed on them, and its `SHA256SUMS` is the
candidate's byte for byte.

## Spend

The local leg is the only paid step of the release. What its receipt itemises: the fill,
0.040000 dollars; the three checks, 0.002555 by Anthropic's judge, 0.012586 by Google's
and 0.004548 by OpenAI's; and the five requests of the loop's check, 0.004130: 0.063819
dollars in all. The loop's own fill is not itemised in the receipt; at Bria's rate of 0.04
dollars a fill it is an estimate, so the leg cost about 0.10 dollars, against the plan's
estimate of 0.30 a candidate (plan m4, "Spend").
