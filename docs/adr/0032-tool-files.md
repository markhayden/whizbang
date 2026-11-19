# 0032. Tool files and the tool worker

Status: accepted.
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
| `evidence` | `measurements`, the one kind at `tool-file/1` |
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
then, so an edit misses.

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
own (`process_group(0)`, which the standard library sets before the program starts). Every
stop of it closes its input: at a tool's failure, a cancellation, the run's time, a frame
that breaks `tool/1`, the end of a door's input and the drop of its supervisor. At the end of
its input the worker sends `SIGKILL` to its whole group, itself last; a process a tool
started, one holding the worker's pipes among them, ends with it, and whizbang waits for the
worker and for its pipes' readers. A worker not ended 5 seconds after its input closed is
killed and waited for; only then may a process it started outlive it, and its readers are let
go and no worker is started while they live. The group's end is the worker's, not whizbang's:
a signal to a group takes `libc` or `unsafe`, which the workspace forbids, or the system's
`kill` program. A worker started outside a group of its own ends itself alone.

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
