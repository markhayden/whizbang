# The transcript format

A transcript is one judged decision written down in a form no provider owns: what the judge
was sent, what it answered, what each of its calls came to, and how the decision ended. The
baseline gate of milestone 2 (SPEC section 16) replays each recorded decision and compares
the transcript whizbang produces with the recorded one.

This page is version 1 of the format. `transcript_version` in a transcript names the version
it follows. A change to what a field means, or to what is compared, is a new version.

Transcripts of the baseline hold evaluation items and recorded model exchanges, so they are
owner-only, like the package they come from. They are never committed. What is public is this
page, the scripts, the comparison code and `audit.toml`.

## Where the pieces are

| Piece | Where | What it does |
|---|---|---|
| `scripts/baseline/convert.py` | this repo | Turns each replay fixture of a package into a transcript |
| `scripts/baseline/audit.py` | this repo | Checks every transcript against the package, and writes `audit.toml` |
| `whizbang_eval::baseline::compare` | `crates/whizbang-eval` | Compares a replayed transcript with a recorded one |
| `audit.toml` | beside this page | The result of the audit: counts, versions and digests only |

The scripts need Python 3.9 or later and nothing outside the standard library. They read the
package and never write into it.

```
scripts/baseline/convert.py --package <package root> --out <transcripts folder>
scripts/baseline/audit.py --package <package root> --transcripts <transcripts folder> \
    --manifest docs/baseline/manifest.toml --summary docs/baseline/audit.toml
```

Both print counts only. The audit writes what it found, per transcript, to
`audit-findings.jsonl` in the transcripts folder, which is private.

## The file

One JSON file per decision: UTF-8, keys sorted, two-space indent, one trailing line feed. The
same fixture always converts to the same bytes.

`example-transcript.json`, beside this page, is a complete transcript of an invented decision:
a response cut at the token limit that held a call, an evidence call that ran, a submission
that did not validate, and the final submission. It is what the converter writes for a
synthetic fixture, and the tests fail when it is not, so it cannot drift from the code. The
schema below is what `scripts/baseline/common.py` (`schema_errors`) enforces, in the audit and
on that example. A field this page does not name is not part of the format: the schema
refuses it and the comparison counts it as a difference.

**Numbers keep their text.** A number is compared by the characters it was written with:
`1` and `1.0` differ, and so do `0.1` and `0.10`. The converter writes every number exactly as
the stored file has it, and a replay must write the same characters. "Whole number" below
means digits only.

One thing about a number's text is not compared: how its exponent is marked. The reader the
comparison uses writes `E` as `e`, and a `+` where the exponent has no sign, before anything
is compared. So `1E2`, `1e2` and `1e+2` are equal. The exponent's digits are kept: `1e2` and
`1e02` differ.

### Top level

| Field | Type | Compared | Holds |
|---|---|---|---|
| `transcript_version` | whole number | yes | `1` |
| `origin` | text | no | `historical` for a decision of the old runs, `recaptured` for one captured again later |
| `complete` | true or false | yes | Whether the record is whole (see "Complete and incomplete") |
| `incomplete` | list of text | yes | What is missing; empty when `complete` is true |
| `subject` | object | yes | Which decision this is (below) |
| `preparation` | object | yes | How every image was prepared (below) |
| `evidence_tools` | list of text | yes | The names of the evidence tools the judge was offered |
| `submission_tool` | text | yes | The name of the one tool offered that gathers no evidence: the one a verdict is submitted with |
| `images` | list of objects | yes | Every image the judge was sent (below) |
| `turns` | list of objects | yes, except each `meta` | The ordered requests and responses (below) |
| `repairs` | list of objects | yes | Each repair in order (below) |
| `terminal` | object | yes | How the decision ended (below) |
| `volatile` | list of text | yes | The places that are not compared (below) |
| `files` | list of objects | no | Every file of the package the transcript was read from (below) |
| `meta` | object | no | Where the transcript came from: the package digest, the fixture's name and folder, the run, the harness and its version, the prompt version, the converter's version |

`subject`, `preparation` and `submission_tool` may hold `null` where a value could not be
read; the transcript is then incomplete.

### `subject`

| Field | Type | Holds |
|---|---|---|
| `set` | text | The evaluation set |
| `item` | text | The item of the set |
| `model` | text | The model, as the old config named it (`provider:model`) |
| `arm` | text | The arm of the old matrix |
| `repetition` | whole number | Which repetition |
| `use_case` | text | The use case that was judged |

### `preparation`

| Field | Type | Holds |
|---|---|---|
| `version` | text | The preparation version the run's journal recorded |
| `settings` | object | What that version stands for, for this model, as values: the long edge, the resize filter, the format and its quality, the decode bounds |

The old recorder wrote the version and not the settings. The settings come from the package:
they were derived from the historical config, catalogue and code, and checked by preparing
every image again (`README.md`, "The preparation check").

`settings` is compared, so a replay does not write its own: it copies the package's object
for the model, whole and unchanged. That object is the value under `settings`, then the
model's name without its provider, in the package's `assembly/generated/preparation.json`.
The format does not fix its keys; the audit requires the object to equal the package's. In
the 2026-11 package every model's object holds exactly these keys:

| Key | Type |
|---|---|
| `config_max_long_edge` | whole number or `null` |
| `decode_max_bytes` | whole number |
| `decode_max_pixels` | whole number |
| `formats_accepted` | list of text |
| `jpeg_quality` | whole number |
| `long_edge_px` | whole number |
| `metadata` | text |
| `model` | text |
| `native_long_edge_px` | whole number |
| `plan` | text |
| `png_when_alpha` | text |
| `prep_version` | text |
| `resize` | text |

The example's object holds two invented keys, because it describes no real model.

### `images`

One entry per image: the job's images first, then tool images in the order their calls appear
in the transcript.

| Field | Type | On | Holds |
|---|---|---|---|
| `id` | text | all | The name a request uses for the image (`image_id`, or an entry of a result's `images`) |
| `frame` | text | all | The name the tools use for it |
| `origin` | text | all | `job`, or the `id` of the call that returned the image |
| `width`, `height` | whole numbers | all | Its size in pixels |
| `prepared` | object | all | The bytes that were sent to the judge: `sha256` (text) and `bytes` (whole number) |
| `role` | text | job images | The image's part in the job, for example `source` |
| `source` | object | job images | The job's file: `sha256` of the file and `pixel_sha256` of its decoded pixels |
| `parent`, `root` | text | tool images | The frame it was cut from or warped into, and that frame's root image |
| `mapping` | object | tool images | How it sits in its parent, as the tool reported it |

### `turns`

One entry per request, numbered from 1.

| Field | Type | Holds |
|---|---|---|
| `turn` | whole number | The turn's number |
| `request` | object | What the judge was sent, whole (below). `null` only when the stored request is missing or was cut short |
| `response` | object | What came back (below). `null` when there was no answer, or the stored response is missing or was cut short |
| `excluded` | `null` or object | `null` when the response entered the history. Otherwise an object with one field, `reason`: `truncated` (the model stopped at its token limit), `refusal` or `no_call` |
| `calls` | list of objects | One entry per call of the response, in the response's order (below) |
| `meta` | object | Not compared. Whether the turn was answered, token usage, cost, the bound, the attempts, the provider's request id, and the hashes of the stored request and response |

`request`:

| Field | Type | Holds |
|---|---|---|
| `system` | text | The system text |
| `history` | list of messages | Every message so far |
| `tools` | list of objects | Each tool as the arm projected it: `name` (text), `description` (text), `input_schema` (object) |
| `max_output_tokens` | whole number | The output limit of this request |

`response`:

| Field | Type | Holds |
|---|---|---|
| `content` | list of content blocks | The answer whole, in the provider's order, including blocks only that provider can read |
| `text` | list of text | The plain text parts, one entry per part |
| `tool_calls` | list of objects | Each call: `id` (text), `name` (text), `arguments` (any JSON) |
| `stop_reason` | text, or an object | `tool_use`, `end_turn`, `max_tokens` or `refusal`; anything else as `{"other": "<the provider's word>"}` |

A message has a `role` (`user`, `assistant` or `system`) and `content`, a list of content
blocks. A content block has a `type` and the fields of that type:

| `type` | Fields |
|---|---|
| `text` | `text` (text) |
| `image` | `image_id` (text): the `id` of an image |
| `tool_use` | `id` (text), `name` (text), `arguments` (any JSON) |
| `tool_result` | `tool_use_id` (text): the `id` of the call it answers; `content` (text); `is_error` (true or false); `images` (list of image ids, may be absent) |
| `opaque` | `adapter` (text): the adapter that may read it; `block` (any JSON): the provider's block, untouched |

### `calls`

| Field | Type | Holds |
|---|---|---|
| `id` | text | The provider's id of the call |
| `name` | text | The tool called |
| `arguments` | any JSON | The arguments as the model gave them |
| `disposition` | text | One of the four below. `null` only in an incomplete transcript, for a call the record does not account for |
| `result` | `null` or object | What the judge was shown for the call: `content` (text), `is_error` (true or false), `images` (list of image ids) |
| `meta` | object | Not compared. For an executed call: the old harness's own call id, the tool's status, its duration and the hash of the stored structured result. For a rejected call: the reason |

| Disposition | Means | `result` |
|---|---|---|
| `executed` | The tool ran | Present, `is_error` false |
| `rejected` | The call was answered without the tool's result: bad arguments, a submission that did not validate, a call out of place | Present, `is_error` true |
| `excluded` | Never dispatched, because its turn was excluded | `null` |
| `final` | The submission that ended the run | `null`: nothing is sent after it |

A decision that ends in a verdict has exactly one `final` call, in its last turn: the call of
the submission tool that the old run accepted. A decision that ends in the error
`submission_invalid`, because its last submission did not validate and no repair was left,
also has one: that submission ended the run. No other call is ever `final`. A call left
without a result by a run that ended some other way (cancelled, say) has no disposition, and
its transcript is incomplete.

The verdict is not an argument of the final call. The old harness worked it out from the
submission; it is the transcript's terminal outcome.

### `repairs`

| Field | Type | Holds |
|---|---|---|
| `after_turn` | whole number | The turn the repair followed |
| `form` | text | `reissue` (the request was sent again) or `rejected` (the call was answered as an error) |
| `reason` | text | Why: `truncated`, `no_call`, `refusal`, `invalid_submission`, `mixed_batch`, `stray_call` |

### `terminal`

| `kind` | Other fields |
|---|---|
| `verdict` | `verdict` (text) |
| `error` | `class` (text), `message` (text) |
| `cancelled` | `class` (text), `message` (text) |

A recorded error is an outcome like any other. It is kept as it is.

### `files`

| Field | Type | Holds |
|---|---|---|
| `role` | text | What the file is: the journal, the old result, decision and report, a request or response by turn, an image, a call's stored result |
| `path` | text | Its path inside the package |
| `sha256`, `bytes` | text, whole number | Its hash and size; `null` when the file is missing |
| `truncated` | true or false | Whether the recorder cut it short |

### `volatile`

The places that are not compared, as paths from the top of the file, with `*` for any one
position in a list. Version 1 declares exactly:

```
/origin
/meta
/files
/turns/*/meta
/turns/*/calls/*/meta
```

Everything else is compared. A path names one place: a field called `meta` inside a call's
arguments is compared like any other argument. A transcript whose list is anything else is
not version 1.

## Complete and incomplete

A transcript is **complete** when every file it was read from exists, has the hash the old
run recorded and was stored whole, and the record accounts for everything in it: the
subject, the preparation, a terminal outcome, and a disposition for every call.

- **A stored file the recorder cut short is incomplete.** The old recorder cut text bodies
  over 2 MiB and marked them. The converter leaves that request or response `null` and adds
  nothing in its place.
- **A response the model itself cut short is complete.** A response that stopped at its token
  limit is what the model said. It is kept as recorded, with its stop reason, and its turn is
  marked excluded as the old run marked it.
- **A decision that ended in an error is complete** when its record is whole. It is never run
  again to get a verdict.
- **Nothing is added to a history** to make a transcript pass. The audit checks a transcript
  against the stored files; it does not repair one.

An incomplete decision is listed by the audit. It is captured again with the frozen squint,
stored apart under `recaptured/`, converted with `--origin recaptured --fixtures recaptured`,
audited like any other, and left out of the historical study numbers.

## The audit

`audit.py` reads the package and the transcripts, and puts each transcript in one of three
states.

| State | Means |
|---|---|
| complete | The record is whole and every rule below holds |
| incomplete | A file is missing, cut short by the recorder, or not the recorded bytes; or the converter could not account for part of the record |
| invalid | The record is whole and breaks a rule: a defect to be understood, never waived |

It exits 0 only when every fixture of the package has a complete transcript; 3 when some are
incomplete and none is invalid; 1 when any is invalid or the coverage below is wrong.

**Coverage, checked before anything else:**

- The transcripts folder has the listing and digest the converter wrote, and they match the
  files. A transcript changed, added or removed after conversion is refused, and so is a
  folder without its listing.
- Every fixture of the package has exactly one transcript, and no transcript is without a
  fixture. A fixture is the one attempt an old result names; a second attempt beside it, or
  a second journal inside it, is an error and nothing is chosen.
- Each transcript names the package it was converted from, by digest.
- With `--manifest`, the manifest names the same package, and counts as many decisions for
  the arm as the package holds fixtures.

**The rules for one transcript:**

1. **The format.** The transcript has exactly the fields of this page, with their types.
2. **Itself.** It is marked complete exactly when nothing in it is marked missing.
3. **Files.** Every file it names exists in the package with the recorded size and hash, and
   none is marked cut short.
4. **Faithful.** Each request and response equals the stored file it was read from.
5. **The journal, read again.** The audit does not trust the converter's reading. From the
   journal it takes again, and compares: the terminal outcome, the repairs, the evidence
   tools, the turns with the stored request and response of each, which turns were
   excluded, every image with its prepared bytes, which calls ran and the stored result of
   each. The journal's events must be numbered without a gap and end with its one terminal
   event. The turns must also be the turns of the old run's own report.
6. **The old result.** The subject is the old result's. The terminal outcome is the old
   result's: the verdict, or the error's class and message.
7. **Preparation.** The version is the journal's and the settings are the package's for the
   model.
8. **In order.** Turns are numbered from 1 without a gap. Each request's history begins with
   the history of the request before it. A response that was kept is the next message of
   the history; a response that was excluded never appears in it. Only the last turn may
   lack a response.
9. **Calls.** The calls of a turn are the calls of its response, in order, and no call id is
   used twice. Each call satisfies its disposition:
   - executed: exactly one result in the history, equal to the call's `result`, not an
     error; the call names an evidence tool; the journal says it returned; and the result
     shown is the result of its own stored structured result (the same call, status and
     summary, the same images);
   - rejected: exactly one result in the history, equal to the call's `result`, an error;
   - excluded: its turn is excluded, and no result anywhere;
   - final: no result anywhere, in the last turn, a call of the submission tool, in a run
     that ended in a verdict or in `submission_invalid`. With a verdict it is the
     submission the journal accepted, with as many items and findings as were accepted.

   No result in a history answers a call the transcript does not hold.
10. **Images.** Every image a history names is listed once, with prepared bytes that are
    among the files.

`audit.toml` records the result: the format and tool versions, the package digest, a digest
over the transcripts (computed like the package digest in `README.md`, leaving out the
listing, its digest and the findings file), and counts of fixtures, decisions, turns, calls
by disposition, terminal outcomes, stop reasons, excluded turns and repairs. It holds no item
names, file names or verdicts. It is written only for a package the manifest counts.

## The comparison

`whizbang_eval::baseline::compare(recorded, replayed)` is what the gate runs. Its version,
`COMPARISON_VERSION`, goes into the gate receipt (`README.md`).

- **Everything is compared except the five volatile places.** The whole ordered sequence of
  requests, responses, calls and results, the images, the subject, the preparation, the
  repairs, the terminal outcome, and any field this page does not name. Any difference
  fails. Differences are reported by place, in the order of the recorded transcript, and
  never carry the values.
- **Numbers are compared by their text,** except how an exponent is marked (above).
- **A transcript cannot make the gate blind.** The comparison refuses a recorded transcript
  whose volatile list is not exactly the one above. A replayed transcript's list is compared
  like any other field.
- **An incomplete transcript is not a baseline.** The comparison refuses a recorded
  transcript that is not marked complete, or that lists something missing. A replayed
  transcript that is not complete differs.
- **A pair with no turns or no terminal outcome is refused,** so two transcripts with the
  same hole never compare as equal. An empty list of turns is no turns, and an outcome
  without a `kind` is no outcome.
- **Text and `null` are compared exactly.** Two texts that differ in case or by a space
  differ, and `null` is not `false`.

It is written in Rust, in `whizbang-eval`, because the gate is driven from the Rust
workspace: the replay produces its transcript in the same process that compares it. Its
tests run in `just check` on synthetic transcripts and on the example. An unchanged
transcript passes. Each of these fails: a changed prompt, image, tool definition or opaque
block; a changed final verdict; a verdict replaced by an error; a changed tool name or
argument; two calls in the other order; a missing turn; an extra turn. A field declared
volatile does not fail. One test walks every place of a transcript and shows that changing
it, removing it, or adding a field beside it fails unless the place is one of the five. The
synthetic transcript also holds an excluded turn with a call in it, a final submission and
a response cut at the token limit; a transcript with a stored file cut short is refused as
a baseline.

The converter and the audit are tested on a synthetic package built in a temporary folder
(`scripts/baseline/tests/`, run by `scripts/tests/test-baseline.sh`).
