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
    --summary docs/baseline/audit.toml
```

Both print counts only. The audit writes what it found, per transcript, to
`audit-findings.jsonl` in the transcripts folder, which is private.

## The file

One JSON file per decision: UTF-8, keys sorted, two-space indent, one trailing line feed. The
same fixture always converts to the same bytes.

| Field | Compared | Holds |
|---|---|---|
| `transcript_version` | yes | `1` |
| `origin` | no | `historical` for a decision of the old runs, `recaptured` for one captured again later |
| `complete`, `incomplete` | yes | Whether the record is whole, and if not, what is missing (see "Complete and incomplete") |
| `subject` | yes | The set, the item, the model, the arm, the repetition and the use case |
| `preparation` | yes | The image preparation version the run recorded, and the settings it stands for, as values |
| `evidence_tools` | yes | The names of the evidence tools the judge was offered |
| `images` | yes | Every image the judge was sent (below) |
| `turns` | yes, except `meta` | The ordered requests and responses (below) |
| `repairs` | yes | Each repair in order: the turn it followed, its form and its reason |
| `terminal` | yes | How the decision ended (below) |
| `volatile` | yes | The fields this transcript declares volatile (below) |
| `files` | no | Every file of the package the transcript was read from: its role, its path inside the package, its hash and size, and whether the recorder cut it short |
| `meta` | no | Where the transcript came from: the package digest, the run, the harness and its version, the converter's version |

### Images

One entry per image, job images first, then tool images in the order their calls returned.

- `id` is the name a request uses for the image. `frame` is the name the tools use.
- `origin` is `job`, or the id of the call that returned the image.
- `prepared` is the hash and size of the bytes that were sent to the judge.
- A job image also has its `role`, its size in pixels and the hashes of its `source` file.
- A tool image also has its size in pixels, its `parent` and `root` frames and its `mapping`
  into the parent.

How every image was prepared is the transcript's `preparation`.

### Turns

One entry per request, numbered from 1 in `turn`.

- `request` is what the judge was sent, whole: `system` (the system text), `history` (every
  message so far, each with a role and content blocks), `tools` (each tool's name,
  description and input schema, as the arm projected them) and `max_output_tokens`.
- `response` is what came back: `content` kept whole in the provider's order, including
  blocks only that provider can read (`type: opaque`, with the adapter's name and the block
  untouched); the plain `text`; the `tool_calls` (id, name, arguments); and `stop_reason`.
  It is `null` only when the record of it is missing.
- `excluded` is `null`, or the reason the response never entered the history: `truncated`
  (the model stopped at its token limit), `refusal` or `no_call`.
- `calls` has one entry per call of the response, in the response's order (below).
- `meta` holds what is expected to differ between runs: token usage, cost, the bound, the
  number of attempts and the provider's request id.

A content block is one of: `text`; `image` with an `image_id`; `tool_use` with an id, a name
and arguments; `tool_result` with the id of the call it answers, its `content`, `is_error`
and the ids of the `images` returned with it; or `opaque`.

### Calls

Every call a judge made has an `id`, a `name`, its `arguments` and exactly one disposition.

| Disposition | Means | `result` |
|---|---|---|
| `executed` | The tool ran | What the judge was shown: `content`, `is_error` (false) and the ids of the `images` |
| `rejected` | The call was answered without the tool's result: bad arguments, a submission that did not validate, a call out of place | What the judge was shown, with `is_error` true |
| `excluded` | Never dispatched, because its turn was excluded | `null` |
| `final` | The submission that ended the run | `null`: nothing is sent after it |

A decision that ends in a verdict has exactly one `final` call, in its last turn. A decision
that ends in an error because its last submission did not validate and no repair was left
also has one: that submission ended the run.

A call's `meta` holds what differs between runs: for an executed call, the old harness's own
call id, the tool's status, its duration and the hash of the stored structured result; for a
rejected call, the reason.

### The terminal outcome

`terminal` has a `kind`:

- `verdict`, with the `verdict`;
- `error`, with the error's `class` and `message`;
- `cancelled`, with the same two fields.

A recorded error is an outcome like any other. It is kept as it is.

### Volatile fields

`volatile` lists the places that are not compared, as paths from the top of the file, with
`*` for any one position in a list. Version 1 declares exactly:

```
/origin
/meta
/files
/turns/*/meta
/turns/*/calls/*/meta
```

Everything else is compared. A path names one place: `meta` inside a call's arguments, say,
is compared like any other argument.

## Complete and incomplete

A transcript is **complete** when every file it was read from exists, has the hash the old
run recorded, and was stored whole.

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
stored apart under `recaptured/`, converted with `--origin recaptured`, audited like any
other, and left out of the historical study numbers.

## The audit

`audit.py` reads each transcript and the package, and puts each transcript in one of three
states.

| State | Means | Exit status |
|---|---|---|
| complete | The record is whole and every rule below holds | 0 when all are complete |
| incomplete | A file is missing, cut short by the recorder, or not the recorded bytes | 3 when some are, and none is invalid |
| invalid | The record is whole and breaks a rule: a defect to be understood, never waived | 1 |

The rules:

1. **Files.** Every file the transcript names exists in the package with the recorded size
   and hash, and none is marked cut short.
2. **Faithful.** Each request and response in the transcript equals the stored file it was
   read from.
3. **In order.** Turns are numbered from 1 without a gap. Each request's history begins with
   the history of the request before it. A response that was kept is the next message of the
   history; a response that was excluded never appears in it. Only the last turn may lack a
   response.
4. **Calls.** The calls of a turn are the calls of its response, in order, and no call id is
   used twice. Each call satisfies its disposition:
   - executed: exactly one result in the history, equal to the call's `result`, not an
     error, with a stored structured result, and the call names an evidence tool;
   - rejected: exactly one result in the history, equal to the call's `result`, an error;
   - excluded: its turn is excluded, and no result anywhere;
   - final: no result anywhere, in the last turn, and not an evidence tool.

   No result in a history answers a call the transcript does not hold. A verdict has exactly
   one final call.
5. **Images.** Every image a history names is listed once, with prepared bytes that are
   among the files. The preparation has a version and settings.
6. **The outcome.** The terminal outcome equals the old run's own result for that decision.
7. **Itself.** The transcript is marked complete exactly when nothing in it is marked
   missing, and its volatile list is the one above.

The audit also checks the transcripts folder against the listing the converter wrote, so a
transcript changed after conversion is refused.

`audit.toml` records the result: the format and tool versions, the package digest, a digest
over the transcripts (computed like the package digest in `README.md`, leaving out the
listing, its digest and the findings file), and counts of decisions, turns, calls by
disposition, terminal outcomes, stop reasons, excluded turns and repairs. It holds no item
names, file names or verdicts.

## The comparison

`whizbang_eval::baseline::compare(recorded, replayed)` is what the gate runs. Its version,
`COMPARISON_VERSION`, goes into the gate receipt (`README.md`).

- **It compares everything that is not volatile:** the whole ordered sequence of requests,
  responses, calls and results, the images, the subject, the preparation, the repairs and
  the terminal outcome. Any difference fails. Differences are reported by place, in the
  order of the recorded transcript, and never carry the values.
- **Only the recorded transcript says what is volatile.** A replayed transcript's own list
  is compared like any other field.
- **A transcript cannot make the gate blind.** The comparison refuses a recorded transcript
  whose volatile list names, or reaches above or below, anything the gate exists to
  compare: the requests, the responses, the calls' names, arguments, dispositions and
  results, the images, the subject, the preparation, the repairs, the terminal outcome,
  completeness, and the list itself.
- **An incomplete transcript is not a baseline.** The comparison refuses a recorded
  transcript that is not marked complete. A replayed transcript that is not complete
  differs.

It is written in Rust, in `whizbang-eval`, because the gate is driven from the Rust
workspace: the replay produces its transcript in the same process that compares it. Its
tests run in `just check` on synthetic transcripts. An unchanged transcript passes. Each of
these fails: a changed prompt, image, tool definition or opaque block; a changed final
verdict; a verdict replaced by an error; a changed tool name or argument; two calls in the
other order; a missing turn; an extra turn. A field declared volatile does not fail. The
synthetic transcript also holds an excluded turn with a call in it, a final submission and
a response cut at the token limit; a transcript with a stored file cut short is refused as
a baseline.

The converter and the audit are tested on a synthetic package built in a temporary folder
(`scripts/baseline/tests/`, run by `scripts/tests/test-baseline.sh`).
