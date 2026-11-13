# 0003. The action descriptor: checked when it is made, and the whole of the static description

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, after the review of the code that came with it. Five rules were
tightened: a `text` default is one plain line; an image port has no `belongs_to`; the image
that `belongs_to` or `derived_from` names is exactly one image, returned whenever the port
that names it is; a version part has no leading zero; no call needs a capability twice. A
description's fields became private, its actions are listed by kind, and its version went
from 2 to 3. "What changes the version" was added, with what this record does not decide
about marks and about conditions on a value.
Amended: 2026-11-12, with ADR 0004. The list of what this record does not decide now names
ADR 0004 for preparing a call, and no longer promises one record for all that remained.
Nothing this record decides changed.
Amended: 2026-11-12, with ADR 0006. Added: a need may name the argument that chooses its
backend (`chosen_by`), with its rule. The description's version went from 3 to 4.
Spec: section 3, "The execution contract" and "Primitives, recipes and loops"; section 6,
"Readiness"; section 14, "Documentation that cannot drift"

## Context

`SPEC.md` section 3 makes the action descriptor the first part of the execution contract:
static and deterministic, holding a name, a kind, typed arguments, typed artifact ports
(required, optional, variadic), outputs with their bounds, the capabilities and features the
action needs, and whether it may run concurrently. Section 6 gives the static description
the question "what can this build do?", answered without a network, and leaves "can this
exact call run now?" to prepare.

Every door, the reference, every tool and every run record names actions through this type,
so its shape is costly to change once tools are ported onto it.

Three things were read, at the commits `docs/sources.toml` pins:

- Rascal's manifest (`crates/rascal-core/src/op.rs`) and its task descriptions
  (`crates/rascal-mask/src/describe.rs`). A manifest is a struct of public fields with a
  `check` the registry calls. A task's description is built from the configuration: the
  `filler` argument is an `enum` of the fillers configured on that machine.
- Squint's tool manifest (`crates/squint-core/src/tool/manifest.rs`): the same shape, read
  from a file, with guidance fields for the prompt.
- Table-manners' description schema (`schemas/core/description.json`).

And one rule already existed here only in a script. The reference prints an action's name
and purpose as they are, so `scripts/docs.sh` refuses any that Markdown or HTML could read
as markup. Nothing stopped such an action from being registered (`docs/carry-over.md`,
milestone 0).

The plan for checkpoint 1A names what the descriptor must be able to say before any tool is
ported: several outputs of different types; an optional protect input; a bounded number of
instances; a capability needed only for some arguments; a mask rebound onto a composite.

## Decision

The code is in `crates/whizbang-core/src/contract/`; the tests that hold it to this record
are in `crates/whizbang-core/tests/it/contract/`.

### A descriptor exists only when it is valid

`ActionDescriptor` has private fields, is written through a `Draft`, and is returned by
`Draft::build` only when every rule below holds; otherwise the first rule broken is the
error, which names the action. It is serialised and never deserialised. So a registry, a
description and the reference can only hold descriptors that keep the rules, and there is
no separate validation step for a caller to forget.

### What it holds

| Field | Meaning | Rule |
|---|---|---|
| `name` | The action's name | A lower-case letter, then lower-case letters, digits, `_` or `-`; 64 characters at most |
| `version` | The version of the action itself, which an artifact's origin records with the name | Three numbers with dots between them, each with no leading zero and nine digits at most |
| `kind` | `primitive`, `recipe` or `loop`: the three levels of section 3 | |
| `purpose` | What it is for | The purpose rule, below |
| `arguments` | Named values that are not artifacts | Below |
| `inputs` | Typed artifact ports a call gives | Below |
| `outputs` | Typed artifact ports the action returns | Below |
| `needs` | The capabilities it can need, each with features and a condition | Below |
| `concurrent` | Whether two runs of it may execute at the same time | False unless stated |

**The purpose rule,** for the purpose of an action, an argument and a port: one line of 1 to
200 characters that starts with an ASCII letter, does not end with a space, holds only ASCII
letters, digits, spaces and `. , ; : ' " ( ) / % + -`, and holds neither `://` nor `www.`
in any letter case. It is the rule at the top of `scripts/docs.sh`, now also in
`contract::text`. One table of accepted and refused names and purposes
(`scripts/tests/fixtures/names-and-purposes.tsv`) is run against both, by
`scripts/tests/test-docs.sh` and by the Rust tests.

**Names of arguments and ports** are an action's name without `-`. A call gives arguments
and input ports under one set of names, so no two of them share one; output names are a
second set.

**An argument** has a type and is one of three things: required; optional with a default;
or optional without one, which a call may leave with no value. A required argument has no
default, and a default must be a value its type accepts. A `text` default also keeps the
purpose rule, because a description prints it: one plain line of at most 200 characters.
The types are `number` and
`integer` (each with optional inclusive bounds; a number is finite), `boolean`, `text` and
`choice` (a list of distinct names, not empty). A type that accepts nothing (a lower bound
above the upper, a bound that is not finite, no choices) is refused. A value is accepted by
its own type only: a whole number is not a `number`.

**An input port** has an artifact type (ADR 0002) and a count: exactly one (required), none
or one (optional), or a stated minimum and an optional maximum (variadic). A port that is
not an image may say that what it takes belongs to the image at another input port, which
must take exactly one image. An image port never says so: an image is what the others
belong to (ADR 0002), so no port belongs to itself and no two belong to each other.

**An output port** has an artifact type and a count whose maximum is always stated: these
are the bounds of section 3. It may say:

- `when`: that it returns something only for some calls (below);
- `belongs_to`, on anything but an image: the image, among the inputs or the outputs, that
  what it returns belongs to;
- `derived_from`, on an image only: the image, among the inputs or the other outputs, that
  it was made from pixel for pixel in place, as a fill's composite is (ADR 0002). A chain of
  derivations never returns to its start;
- `rebound_onto`, on a mask only: an output that returns exactly one image, derived from
  the very image the mask belongs to. When the action returns that image, the mask is
  returned rebound onto it.

"The image" that `belongs_to` or `derived_from` names is one image that is there whenever
the port that names it returns something: an input that takes exactly one image, or an
output that returns exactly one image either for every call or under the same condition as
the port that names it. An output that returns up to eight images, or one returned only
for other calls, is refused. `rebound_onto` alone may name an image returned only for some
calls: the rebind happens when the image is returned.

**A need** is a capability (`segment`, `fill` or `judge`), the features it must have, each
a name, and a condition. No call needs a capability twice: a capability is stated once, or
twice under `given` and `not_given` of one argument, which no call satisfies together, and
which may ask for different features. Stated `always` and again under a condition, or under
conditions on two different arguments, it is refused. So two things cannot be said: features
that vary with two arguments, and a capability needed when either of two arguments is given
("fill when `f` is given" beside "fill when `g` is given").

A need may also name the argument that chooses its backend (`chosen_by`; ADR 0006, "How a
call names its choice"). That argument is declared, takes a text, and is one a call may
leave without a value. A need that names none is answered by the default backend.

**A condition** (`when`) is `always`, `given` an argument, or `not_given` an argument. The
argument must be one a call may leave without a value. So which capabilities a call needs
and which outputs it returns are decided from the names of the arguments it gives and from
nothing else: `needs_for` and `outputs_for` compute them, without a store, a configuration
or a model.

### Nothing in it depends on the machine

A descriptor holds no configured backend, installed model or credential. Rascal's `filler`
was an `enum` of the configured fillers; here it is a `text`, and whether the filler a call
names exists is a question for prepare.

### Registration

`Registry::register` takes an action, which gives its descriptor, and keeps actions in the
order they were registered. The description does not list them in that order (below). A second action of the same name is refused and changes
nothing. The binary builds its registry in one function (`crates/whizbang/src/wiring.rs`)
that its tests call too; a test then registers its fakes, which live in
`whizbang-testkit`, a development dependency only (`scripts/crate-graph.txt`).

### The static description, version 4

`whizbang describe --json` is one object: `description_version` (now 4), `name`, `version`,
`actions` and `artifact_types`. A `Description` is made from a registry and in no other
way: its fields are private, so one cannot be written by hand with a name listed twice.

- `actions` holds each registered action's whole descriptor, with every field present; one
  that is not set is `null`. Recipes come first, then loops, then primitives, each kind in
  the order its actions were registered (section 3: recipes are what is advertised, and
  primitives follow). The order does not depend on the order a build registers its tools in.
  Version 2 listed actions in registration order. Version 4 gave each need of an action
  the field `chosen_by`.
- `artifact_types` holds the six types in ADR 0002's order, each with its `name`, a
  `purpose` that keeps the purpose rule, and `travels_as`, which is `canonical_file` for
  all six: a copy of the file is the artifact, and `whizbang store import` reads it.

### What changes the version

`description_version` goes up by one with any change a reader of the document could
observe: a field added, removed, renamed or given another type; a new value of an existing
field (a new kind, argument type, capability or condition); or a change in what an existing
field or its order means. There are no optional fields, since every field is always
present, so an added field is such a change and bumps the version like any other. A new
action, a new artifact type or a new version of whizbang is content, not shape, and does
not. A rule that only refuses descriptors that were accepted before does not either: every
document written under it is still a document a reader of the same version understands. A
rule that is loosened does bump it: a reader that held values to the old rule could meet
one it refuses (a `text` default that is not a plain line, say).

The rule is kept from the first change on, though nothing reads a description yet. Until
the first release, no older description version is supported: a reader is changed with
the writer.

## Consequences

- The five things the plan asks for are written as two fixtures, descriptors for rascal's
  `seam` and `remove_prompt` built from their recipes and the common properties
  (`tests/it/contract/fixtures.rs`). Neither recipe is ported.
- An action whose name or purpose breaks the rule cannot be registered, because its
  descriptor cannot be made. The rule is now written twice, in `scripts/docs.sh` and in
  `contract::text`; the one table of cases is what holds the two together, and a new case
  goes there.
- The description changed shape twice, so its version is 3 and a reader of version 1 or 2
  must be changed. Nothing outside this repo reads it yet.
- A descriptor cannot be loaded from a file. A user's own recipe or an experimental tool
  file (section 3, section 4) will need a reader that goes through `Draft::build`; that is
  a new function, not a change to the rules.
- Changing a rule later is cheap where it loosens (a new argument type, a new condition)
  and costly where it tightens or renames, because every ported action and the generated
  reference follow the descriptor.

What this record does not decide, and the code does not do:

- How a call is prepared: ADR 0004 (`docs/adr/0004-preparing-a-call.md`). How it is
  admitted and executed, and the run record, are not decided yet.
- What a feature is. A need names features; the capability interfaces say which exist.
- A result that is not an artifact, such as a verdict. An output port returns one of the
  six artifact types (`docs/carry-over.md`).
- A tool's mark. Section 8 says `whizbang describe` shows whether a tool is experimental,
  unverified or admitted; the descriptor has a kind and no mark (`docs/carry-over.md`).
- A condition on the value of an argument, on the configuration, or on an optional input
  port. Rascal's `remove_prompt` has one: its `check` argument, a boolean that defaults to
  true, asks for a judge when it is true and a checker is configured. The fixture leaves
  `check` out (`docs/carry-over.md`).
- Bounds on the size of what is returned (bytes, the long edge of an image), guidance text
  for a model, cost and duration hints, and list-valued arguments. The old manifests have
  them (`docs/carry-over.md`).
- How the reference page shows a descriptor. The page prints an action's name and purpose;
  the rest is in `description.json` (`docs/carry-over.md`).

## Alternatives

- **Public fields and a `check` method,** as in rascal. Turned down: a descriptor could
  then exist unchecked, and each place that receives one would have to remember to check.
- **Check at registration only.** Turned down for the same reason; and a descriptor is also
  used by fixtures and tests that never reach a registry.
- **A JSON Schema for the arguments,** as rascal, squint and table-manners publish. Turned
  down for the descriptor itself: a schema can say far more than an executor checks, and
  the conditions here ("needed when `filler` is given") have no plain form in one. A door
  that needs a schema can derive it from the typed arguments.
- **Capabilities and outputs conditional on argument values or on arbitrary expressions.**
  Turned down until a tool that is being ported needs it. Every case the two fixtures hold
  is "this argument is given, or it is not", which can be decided without running
  anything; that is so only because the `remove_prompt` fixture leaves out the recipe's
  `check` argument, whose value decides whether a judge is needed. Nothing in checkpoint 1A
  depends on a value, so the question goes with rascal's recipes to milestone 3.
- **A descriptor built from the configuration,** as rascal's was. Turned down: section 6
  wants the static description identical on every machine for the same build.
- **Keeping description version 1 and adding fields.** Turned down: an action went from two
  fields to nine, and a reader that knows version 1 would not know what it is not reading.
