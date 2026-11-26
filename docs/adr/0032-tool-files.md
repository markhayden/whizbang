# 0032. Tool files and the tool worker

Status: accepted. Amended by ADR 0053 (below, "Amended: a model by name, and labels").
Date: 2026-11-18
Spec: sections 4 ("Experimental tools"), 7, 8 and 10; plan m2c (`docs/plans/m2c-milestone-2.md`),
task X1, items 1 to 5 and 7; ADR 0008, ADR 0017, ADR 0020, ADR 0031

## Context

`SPEC.md` section 4 asks for perception tools written as one Python file each, which declare
a tool description and one function, reach images and capabilities only through a context,
are validated against their declared bounds, are listed under `[tools] paths`, are pinned by
digest in each run, and are picked up when edited with nothing rebuilt. It leaves their shape
to "the first decision of milestone 2, made before any tool is written". The model's worker
(`pack/1`, ADR 0020) runs one call at a time, so a tool inside it that asked for SAM 3 would
wait on itself. A tool file is code the user chose; it must be the user's explicit choice,
pinned, and never fetched, as a user's adapter is (ADR 0017, task U1).

## Decision

**The shape: one Python file, a declaration and a function.** The declaration is TOML in a
block of comments at the top of the file, between `# /// whizbang-tool` and `# ///`, in the
form of Python's inline script metadata, so the binary reads it without running anything:

| Key | Is |
|---|---|
| `format` | `tool-file/1` |
| `name`, `version`, `purpose` | Its action's, by the descriptor's rules |
| `evidence` | `measurements`, the one kind at `tool-file/1` (and `regions` beside it, "Amended: regions") |
| `bindings` | The images it reads, 1 to 4, each an input port of its action |
| `measurements` | 1 to 16, each a `name`, a `unit`, and its bounds `min` and `max` |
| `readings_most` | The most readings an output holds, 1 to 64 |
| `primary`, `changed`, `reduce` | The measurement rung 2 scores, which way (`larger` or `smaller`) is changed, and how readings reduce to one: `worst` |
| `time_limit_s` | The longest a run takes, 1 to 600 seconds |
| `segments_most` | The most segments a run asks for: what its admission reserves for; 0 when not given |
| `arguments` | At most 8, each a `name`, a `type` (`text` or `number`), a `purpose` and `required` |

Every key but `segments_most` and `arguments` is required: a declaration without a bound is
refused when the config is read, and nothing of the file runs. The function is
`run(context)`. The context gives the run's `arguments`, `image(name)` for each binding (its
size and its samples, RGBA) and `segment(image, phrase=|point=|box=, most=)`. Nothing else:
the tool is handed no path, no store and no key. Its output is an object of `readings`, each
a `subject` (a text of at most 200 printable characters) and `values` of declared
measurements, and `not_applicable`, the subjects it could not read; anything else is refused.
Each value is held to its measurement's bounds before anything uses it: the judge, a run's
diagnostic, rung 2. The primary measurement of a run is the worst of its readings.

**Discovery and pinning.** `[tools] paths` lists absolute paths, at most 16, each once; the
user writes each, and whizbang never fetches or copies one. Each run reads the file's bytes
once, through a name that is no link, from a regular file its group and others cannot write,
of at most 1 MiB; works out their SHA-256; notes it in the run's record (the setting
`tool_file_digest`, or `tool_file:<name>` in a run of `check`); and sends those bytes and
that digest to the tool worker, which hashes them again and compiles only bytes that match.
An edited file is the next run's. An edit that changes a tool's name, images or arguments,
which made its action when the registry was built, is refused until whizbang starts again.
The decision cache keys a run of `check` that names a tool file by the file's digest as it is
then, so an edit misses; a run whose call of a tool file ran another digest (the file edited
while it ran) keeps no key.

**Where a tool file runs: the tool worker, a second process of the pack's runtime.** The one
worker of a process (`OneWorker`, of the kind `pack`) holds it beside the model's worker: at
most one, started at the first run of a tool file, from the Python and the pack's source
`whizbang setup` installed in the models directory the wiring opened, and stopped with the
kind. Its entry point is `whizbang_pack.tool_worker`, started as the model's worker is: by
its absolute path, isolated, the boot closing every descriptor above the three, in a cleared
environment holding the locale and the supervisor's own variables and no key, no token and
no home of the user's. One run at a time; a run that finds another waits, looking at its own
cancel and deadline. The process holds two workers' three threads at most (the supervisors'
spawner allows six, ADR 0008 as amended).

**`tool/1`.** Frames as `pack/1`'s are: four bytes of a header's length, a header of at most
64 KiB that is one JSON object, then each payload its header lists after eight bytes of its
length. Only whizbang's frames carry payloads: a `run` carries the file's bytes and each
image's samples; an `answer` carries each mask. The worker's frames are headers alone:
`hello`; `ask` (`segment`, an image the run handed the tool, one query, at most 64
instances); `result` (the output); `error` (a sentence, cleaned and cut to 240 characters).
Whizbang answers an `ask` with `answer` or `refused`. There is no `cancel`: whizbang ends a
run it gives up on by ending the worker. `pack/1` is unchanged.

**A capability is a nested call.** Whizbang answers a tool's `segment` by calling the action
`segment` through the executor of the run's context (ADR 0008): a run of its own under the
tool's run, recorded among its steps, under its allowance and its policy, cancelled with it.
A run of a tool file reserves at its admission what `segments_most` asks of the default
segmenter; one that declares none reserves nothing, and an ask of a segmenter that costs is
then refused for want of allowance. One ask is open at a time: an `ask` read while another
is being answered is refused, and a run asks at most 64 times. A mask in the store holds no
score, so an answer gives none.

**Nothing left running.** The tool worker is started as the leader of a process group of its
own (`process_group(0)`, which the standard library sets before the program starts); the
group's id is the worker's process id, recorded when it is started. Every stop of it closes
its input: at a tool's failure, a cancellation, the run's time, a frame that breaks `tool/1`,
the end of a door's input and the drop of its supervisor. At the end of its input the worker
sends `SIGKILL` to its whole group, itself last; a process a tool started, one holding the
worker's pipes among them, ends with it, and whizbang waits for the worker and for its
pipes' readers. A worker that has not ended 5 seconds after its input closed could not act
(a tool holding Python's lock in C code keeps its reader from the end of its input): whizbang
then ends the group itself, with the system's `kill` program, `/bin/kill -KILL -- -<group>`
started by its absolute path in a cleared environment, waits for the program, and kills and
waits for the worker; the drop of a running worker does the same. Only the group whizbang
made for that worker is named, never 0, 1 or whizbang's own. **The owner approved whizbang
signalling its tool worker's group so on 2026-11-18**: the workspace forbids the `unsafe` a
signal to a group would take in Rust, and a probe of the review showed a tool's child
outliving a worker that could not end its group. A process a tool moved out of the group is
not ended; its readers are let go, and no worker is started while they live.

**One worker for one file.** A run of a tool file whose digest is not that of the file the
running worker last ran ends the worker and starts another, so that nothing one file's code
left in the process (a module, a thread, a global) is there for another file, or for an edit
of the same one.

**A tool file is offered to a judge only when a run names it** (`squint check --tool`, or an
arm of a matrix): it is on offer in `check`'s toolbox with the schema its declaration gives,
and a judge's call of it runs it under `check`'s own context, so its capabilities are nested
calls of the `check` run, which freezes `segment` beside its tools when it names a tool file
(the judge is never offered `segment` itself). Its answer is its output as checked, with the
digest it ran. `whizbang try` and `whizbang eval tool` take a tool file whose images are
`source` and `candidate`; one that takes the argument `job` is given each item's job, whose
requirements it reads and never its label.

## Consequences

- **A tool file runs with the user's privileges, as a user's adapter does** (`SPEC.md`
  section 10): it is given no key, no home and no store, and reaches whizbang only through
  its context, but nothing keeps it from the network, from the user's files, or from
  starting processes; a process it starts that leaves the worker's group is not ended with
  it. Whizbang bounds what comes back (the frames, the output, the time) and what it starts
  (one worker), not what the user's code does. `docs/guides/config.md` says so.
- A paid capability under a tool file needs `segments_most`; under `check`, a tool file's
  segments are under `check`'s reservation, which is its judge's, so a segmenter that costs
  is refused there and a local one is not.
- The tool file of this milestone is per-subject comparison (`pack/tools/per_subject.py`).
  Its record, with rungs 1 and 2 on the Mac, is owed (`docs/carry-over.md`).

## Amended: regions (2026-11-25)

Plan mE, task T1, item 0. `outline` and `regions`, ported from squint, locate what changed:
their evidence is regions on a frame as well as numbers. `tool-file/1` is widened, and a file
written to it before still reads as it did.

- **`evidence`** is `measurements`, as before, or the list `["measurements", "regions"]`.
  Measurements stay required: every tool file declares the one number `eval tool` scores it
  by.
- **Two bounds and a kind, for a tool whose evidence names regions, and only for one:**
  `max_regions`, the most regions an output holds, 1 to 64; `max_region_label_bytes`, the
  longest label of one, 1 to 200; and `region_kind`, the kind of evidence its regions are,
  as the rules' `evidence_kinds` name kinds (`change_region`), 1 to 32 lower-case letters
  or `_` (added by T1's second pull request). A declaration that lacks one is refused when
  the config is read, as one lacking any bound is.
- **The output** may then hold `regions`, beside `readings` and `not_applicable`. Each is
  an object of `frame`, the binding of an image the run handed the tool (`source`,
  `candidate`); exactly one of `box`, `[left, top, right, bottom]` with area, and `polygon`,
  3 to 64 points `[x, y]` with area, in pixels of that image and inside it (a polygon whose
  signed area by the shoelace sums to exactly zero is refused: points on one line, and a
  figure eight whose two loops cancel; one with any area, however small, is taken); `label`,
  printable ASCII of 1 to the declared bytes; and `score`, from 0 to 1. Nothing else. Each is held to
  these rules before anything uses it, as a reading is: a region past the declared count, of
  another frame, outside its image, or of a tool that declares none fails the run, and
  nothing it read is stated.
- **Where they go.** The run's diagnostic `tool_file.json` holds them beside the readings,
  and a judge that called the tool is shown them in its answer. The checklist takes regions
  as evidence items, each bound to the mandatory instances on its image whose evidence
  kinds hold the tool's `region_kind` (`whizbang_squint::checklist::ingest`, squint's,
  plan mE, task T1, item 4). Under a use case, `check`'s loop calls it on each result of
  such a tool (plan mE, task F1): each region on the raw frame of an image of the job is
  an item the judge is told of in the tool's answer and must give a disposition, at most
  24 in a run (squint's budget, a result past it not presented); v2's and v3's tools are
  made again for them, and v1's, which names no item, is sent as it was.
- **Hints.** A rule file's `[hints]` (`[hints.regions]` `preserve` and `ignore`,
  `[hints.outline]` `classes`) is given to a tool file as the arguments of text it declares
  under those names (`preserve`, `ignore`; `phrases` for `outline`), by `eval tool`, and by
  `check` under a use case unless the judge's call gave the argument (plan mE, task F1);
  `try` does not.
  No base rule file holds one.
- **What a tool file may import.** It runs in the tool worker, from the pack: besides the
  standard library it may import the pack's own modules. `pack/tools/outline.py` and
  `pack/tools/regions.py` read their subjects through `whizbang_pack.subjects`, per-subject
  comparison's rule, shared, and their masks through `whizbang_pack.masks`.

`tool/1` is unchanged: an output is still one JSON object of at most the header's bound.
Plan mE's task T2 widens the context again, for a model named by the request and a labels
answer.

## Amended: a model by name, and labels (2026-11-25)

Plan mE, task T2, item 2; ADR 0053. The context reaches a model of the catalogue by its
name, and a fixed-class segmenter's labels of every pixel.

- **An `ask` may hold `model`**, a model's name (a lower-case letter, then lower-case
  letters, digits, `-` and `_`, 64 at most): `context.segment(..., model=)`. Whizbang routes
  it to the segment backend of the config whose `model` is that name, the default first
  when it is one of them and otherwise the first by name, as the nested call's `backend`;
  a name no backend of the config is refuses the ask. An ask without one is the default's,
  as before.
- **An `ask` may be of the capability `labels`**, holding `classes`, 1 to 255 phrases of
  one line, none twice, in place of `query` and `most`: `context.labels(image, classes,
  model=)`. It is a nested call of the action `labels` (ADR 0053). Its `answer` holds the
  image's `width` and `height` and `labels`, the classes some pixel has, each a `name` and
  a `score`, its confidence; and one payload, a byte a pixel, `k` where the `k`th of those
  labels the pixel and 0 where none asked does. The context gives it as `Labels`
  (`width`, `height`, `classes`, `plane`, `name_at(x, y)`).
- **The calls a tool file's run is held to** are `segment` and `labels`.
