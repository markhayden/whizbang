# 0028. The job

Status: accepted.
Date: 2026-11-18
Spec: section 7 (the five use cases); section 3, "The execution contract"; plan 2C, task A1,
items 2 to 5

## Context

Squint judged a job, not a pair of pictures: a use case, a brief in words, structured
requirements (each with an id, a kind, a scope, the images it takes as evidence, and for an
exemption what it waives and when), and images, each with an id, a role and a label
(`crates/squint-core/src/job.rs` at the commit `docs/sources.toml` pins). The job chooses the
obligations, the ids of the checklist and the words of the prompt, and every evaluation set
squint published is a list of job files. Plan 2A's `check` took two images and a brief
under `source-fidelity` alone, with the roles fixed by the order. The four other use cases
need roles a position cannot carry (any number of references, each labelled), and the sets
need their requirements whole.

## Decision

**`check` takes a job.** Under each of the five use cases of the table
(`whizbang_squint::use_case`), the call gives the argument `job`, a text holding the job as
JSON in the contract `job/1` (`whizbang_squint::job`): squint's `[job]` table, field for field
(`use_case`, `brief`, `requirements`, `images`), and a field `contract` that says `job/1`. A
job's images are bound to the call's images in the job's order: the first image of the job
is the first input, and so on. Each image's `source` (a path and its SHA-256) and
`projection` are kept as the file gave them; `check` reads neither. A job is checked as
squint checked it: one candidate, at most one source, ids that are plain tokens and unique;
and then resolved under the use case's rules, which refuses a job that lacks a role the use
case requires or names one it does not take, a requirement whose id is not `RQ` and a number,
an exemption without its condition, and a reference no mandatory obligation consults. A job
that does not fit its call (another use case, another count of images, `brief-compliance`
without a brief) is refused the same way. Each refusal is an invalid call, before anything
is sent (`check::a_call_missing_a_required_role_is_refused_at_prepare_and_sends_nothing`).

**The command line makes a job.** `whizbang squint check` builds one as squint's command line
did (`Job::from_shorthand`): the candidate, then the source when one is given, then each
`--reference` in the order given, as `reference-1`, `reference-2`, … labelled `reference 1`,
`reference 2`, …; `--brief` is the job's brief, and there is no structured requirement.
`--job <file>` reads a squint job file whole, brings each picture into the store from the file
its `source.path` names (relative to the job file's folder), refuses a file whose SHA-256 is
not the one the job pins, and passes the job on unchanged
(`cli::squint_check_job_takes_a_squint_job_file_whole`). A picture given inline (`base64`) is
not taken. `--job-json` takes a job as a run's record holds it, its images by `--image`, by
identity, in the job's order (plan m5, task W2: the playground's copied command of a check).

**What the run keeps.** The job is an argument, so the run's record holds it as given, the
brief and each image's role and label among it; the run's diagnostic `judged.json` holds what
each image was bound to, the checklist, each repair and each retry, and each answer's
disposition, confidence, row and location
(`record::the_brief_and_each_bindings_role_and_label_are_in_the_run`). The record's own
groups are unchanged: the run record keeps its version.

## Consequences

E1's runner and E2's replay pass each set's jobs to `check` as they are, so a set's
requirements and labels reach the judge as squint gave them, and the decision cache (M1) can
key on the job's text. The action's version is 1.2.0. A call of `check` under a use case
without a job, as plan 2A's command made, is refused: there is no second way in.

A job file squint wrote with `expect` (an image reference to verify) is refused, as an
unknown field; no set uses it (`docs/carry-over.md`).

## Alternatives

- **The bindings as an argument of their own, and the requirements in another.** Two
  arguments that must agree with each other and with the inputs, where one job is one thing
  to check and to key a cache on.
- **The images inside the job, by identity.** A job would then name the store's artifacts
  and could not be one of squint's files passed unchanged; the call's inputs already name
  them, and the executor checks them.
- **The job as TOML in the argument.** JSON is what every other contract of whizbang is
  written in, and what the record and the door already read.
