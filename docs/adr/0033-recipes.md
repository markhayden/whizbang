# 0033. Recipes: the format, the runner, `run/5`

Status: accepted.
Date: 2026-11-18
Spec: section 3 ("Primitives, recipes and loops", "Nested calls", "What every run records"),
sections 4 and 6; plan m3 (`docs/plans/m3-milestone-3.md`), task R1; ADR 0003, ADR 0005,
ADR 0008, ADR 0009, ADR 0013, ADR 0016, ADR 0025, ADR 0032

## Context

`SPEC.md` section 3 makes a recipe a named, typed sequence of steps over primitives and
other recipes, the level that is advertised first. Rascal ran its tasks as recipe files
(`crates/rascal-core/src/recipe.rs` and `docs/recipes.md` at the commit `docs/sources.toml`
pins), each step an op of its registry, with its own journal. Whizbang has the pieces a
runner needs already: an action of the kind `Recipe` (ADR 0003), nested calls through the
executor in a context, each a run of its own under its caller with a share of the caller's
allowance (ADR 0008, ADR 0016), and a run that freezes the actions it calls (ADR 0009).

## Decision

**The format, `recipe/1`, is rascal's file** (`crates/whizbang-core/src/recipe/mod.rs`): the
keys `version` (1), `name`, `use_case` and `projection` (read and kept), `[inputs]` (each an
artifact type or `Text`, and `optional`), `[[step]]` (an `id`, an `op` of the build, `inputs`
binding each port to `inputs.<name>` or `<step>.<output>`, `constants`, and every other key an
argument), and `[outputs]`. Two keys more are required of a recipe whizbang runs: `purpose`,
what it is for, and `use`, when to use it, each one line under the rule of a purpose. A file
is at most 64 KiB, and a recipe has 1 to 32 steps, each setting at most 16 arguments.
Rascal's `[outputs] render` is read and refused when the recipe is checked: a rendering is a
step of the action `render`.

**Checked when it is read, against the descriptors** (`Recipe::check`): every op an action of
the build; step ids names of the rule of an action, each once; every binding an earlier
step's output or a declared input, of the port's type; a port that takes several bound to an
output that returns several, and one that takes one to one that returns one (rascal's rule;
its `<port>_<n>`, one artifact of several, is not taken); a text input bound only to a text
or a choice argument, never a port; an optional input only where the port or the argument
may be left out; every required port and argument given; every argument the action's, of its
type (a whole number is a number where the action takes a number, as rascal resolved it); a
constant set the action's argument `constants`, a choice of its sets' names. Every refusal
names the recipe and the step.

**The runner** (`crates/whizbang-core/src/recipe/runner.rs`) makes a checked recipe an action
of the kind `Recipe`: an argument for each text input and a port for each artifact input;
`set`, the overrides of one run; `until`, a choice of the step ids; an output for each of the
recipe's, which applies only without `until`; and the needs of its steps' actions, one need of
each capability, for as many requests as the steps send together. Its run:

- notes the digest of the recipe's text as the setting `recipe.digest`, so every run pins the
  file it ran;
- refuses, before anything runs and naming the step, an override whose step the recipe has
  not, or whose argument the step's action does not take, or whose value its type refuses;
- freezes its calls to its steps' actions (ADR 0009), so it calls nothing else and gives a
  step only its own inputs and what an earlier step committed;
- calls each step through the executor in its context, in order, with values passed by their
  identities, an absent optional input leaving its port or argument out; each is a run of
  its own, a child of the recipe's, whose share is taken from the recipe's allowance: the
  recipe was reserved for all its steps once, and nothing is reserved twice;
- looks at its signal before each step: a cancelled recipe's step is cancelled with it (its
  signal is under the recipe's), and no other starts;
- ends with the step that failed, naming it, when one does not complete;
- with `until`, stops after that step and hands back nothing of its own; the step's outputs
  are what its run committed, which the record names.

A user's own recipes are files listed under `[recipes] paths` in the config, at most 16
absolute paths (not in the config's digest: the run records the file's), each read once as a
regular file under its own name, not a link, not writable by its group or others, and checked
as a shipped recipe is. A recipe file holds no code and names only actions of the build, so
it runs nothing the build does not. The shipped recipes are files under
`crates/whizbang-rascal/recipes/`, built in, read and checked the same way.

**`describe` and the server.** A descriptor may say `use` (description version 8); a recipe
does. A description lists recipes first already (version 3); `whizbang serve` now lists its
tools in that order too.

**`run/5` (amends ADR 0013, after ADR 0025).** A step of a recipe is a step of the kind
`recipe`: the call's `run`, `action` and `status` as before, the step's `id`, the `arguments`
the recipe gave the call (overrides applied, the constant set apart, each text with the
secrets taken out and cut at 64 characters, at most 16), and `constants`, the set it named or
null. A record is written in the oldest version that holds all of it: one that lists a step
of a recipe is `run/5`; any other is `run/4`, as before, so no
record and no test of a run that is no recipe changes. `run/5` is read beside `run/4`,
`run/3` and `run/2`; a step of a recipe in a record of another version is refused, and a
step that says what its kind does not is not written back as it was, and is refused. The
schema is `crates/whizbang-core/schemas/run-5.json`; the fixed vector is `of_a_recipe` in
`crates/whizbang-core/tests/it/contract/record.rs`. A record's steps of a recipe add at most
32 × 16 × about 450 bytes, about 230,000, inside the room ADR 0025's sum left.

**How a parent reads what a child committed (amends ADR 0008).** The names a child committed
are resolved once, when its end is published, from the generation current then; a look at
the parent's store then adds them with no read (`context::a_childs_names_are_resolved_once_and_a_look_reads_no_file`).
Where that read fails, the names are left to the next look, which reads as every look did.

**How many calls a run makes (ADR 0008, kept).** `STEPS_LIMIT` stays 256. Measured on the
owner's Mac in the tests' build, one run of nested calls of an action that does nothing, in a
store that held nothing else (`store::volume`): 32 calls in 1,932 ms, 256 in 18,879 ms (about
74 ms a call, two publications each, every one synced), the last generation 149,793 bytes
(585 bytes a run); a first try at 300 took 23,294 ms. A thousand would take over a minute, so
the limit is not raised: a recipe has at most 32 steps, well under it, and what a call costs
is the syncs of its two publications more than the size of the generation. CI runs eight
calls, under a second (`store::volume::a_run_of_many_nested_calls_publishes_within_the_measured_bound`);
the run at the limit is `#[ignore]`d and run by hand.

**The order of a segmenter's answer** is fixed by `segment` (version 2.1.0), whatever backend
answered: best score first, then the box's top, left, right and bottom, then the predicted
IoU, none first and then the higher, then the plane's bytes (`segment::in_order`).

## Consequences

The tasks of rascal and the use cases of squint are recipe files from here on (O1 to O5, Q1).
Two needs of one capability across steps are one need of the recipe, with every feature any
step asks: a recipe whose steps ask one capability of two backends is handed the first's
(the carry-over row on two needs of one capability). The executor built with a digest alone
(`Engine::over`) is not changed here (the carry-over row on `Engine::over`).

## Alternatives

- **Recipes as Rust code.** No file a user can write; rejected by `SPEC.md` section 3 and by
  rascal's practice.
- **`until` changing the recipe's outputs.** The executor holds what a run hands back to its
  descriptor's ports; a step's outputs are not the recipe's. The step's run already committed
  them, and the record names that run.
- **Always writing `run/5`.** Every record and every fixed vector of a run that is no recipe
  would change for nothing it says.
- **The overrides as a structured argument.** An argument is a number, a whole number, a
  boolean or a text; the overrides are one text, a line each, which the record keeps whole.
