# 0003. The action descriptor: checked when it is made, and the whole of the static description

Status: accepted
Date: 2026-11-12
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
| `version` | The version of the action itself, which an artifact's origin records with the name | Three numbers with dots between them |
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
`contract::text`; the accepted and refused cases of `scripts/tests/test-docs.sh` are run
against the Rust functions too.

**Names of arguments and ports** are an action's name without `-`. A call gives arguments
and input ports under one set of names, so no two of them share one; output names are a
second set.

**An argument** has a type and is one of three things: required; optional with a default;
or optional without one, which a call may leave with no value. A required argument has no
default, and a default must be a value its type accepts. The types are `number` and
`integer` (each with optional inclusive bounds; a number is finite), `boolean`, `text` and
`choice` (a list of distinct names, not empty). A type that accepts nothing (a lower bound
above the upper, a bound that is not finite, no choices) is refused. A value is accepted by
its own type only: a whole number is not a `number`.

**An input port** has an artifact type (ADR 0002) and a count: exactly one (required), none
or one (optional), or a stated minimum and an optional maximum (variadic). It may say that
what it takes belongs to the image at another input port, which must take exactly one image.

**An output port** has an artifact type and a count whose maximum is always stated: these
are the bounds of section 3. It may say:

- `when`: that it returns something only for some calls (below);
- `belongs_to`: the image, among the inputs or the other outputs, that what it returns
  belongs to;
- `derived_from`, on an image only: the image, among the inputs or the other outputs, that
  it was made from pixel for pixel in place, as a fill's composite is (ADR 0002). A chain of
  derivations never returns to its start;
- `rebound_onto`, on a mask only: an output image that is derived from the very image the
  mask belongs to. When the action returns that image, the mask is returned rebound onto it.

**A need** is a capability (`segment`, `fill` or `judge`), the features it must have, each
a name, and a condition. The same capability is not stated twice under the same condition.

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
order they were registered. A second action of the same name is refused and changes
nothing. The binary builds its registry in one function (`crates/whizbang/src/wiring.rs`)
that its tests call too; a test then registers its fakes, which live in
`whizbang-testkit`, a development dependency only (`scripts/crate-graph.txt`).

### The static description, version 2

`whizbang describe --json` is one object: `description_version` (now 2), `name`, `version`,
`actions` and `artifact_types`.

- `actions` holds each registered action's whole descriptor, in registration order, with
  every field present; one that is not set is `null`.
- `artifact_types` holds the six types in ADR 0002's order, each with its `name`, a
  `purpose` that keeps the purpose rule, and `travels_as`, which is `canonical_file` for
  all six: a copy of the file is the artifact, and `whizbang store import` reads it.

## Consequences

- The five things the plan asks for are written as two fixtures, descriptors for rascal's
  `seam` and `remove_prompt` built from their recipes and the common properties
  (`tests/it/contract/fixtures.rs`). Neither recipe is ported.
- An action whose name or purpose breaks the rule cannot be registered, because its
  descriptor cannot be made. The rule is now written twice, in `scripts/docs.sh` and in
  `contract::text`; a case added to one belongs in the tests of both.
- The description changed shape, so its version is 2 and a reader of version 1 must be
  changed. Nothing outside this repo reads it yet.
- A descriptor cannot be loaded from a file. A user's own recipe or an experimental tool
  file (section 3, section 4) will need a reader that goes through `Draft::build`; that is
  a new function, not a change to the rules.
- Changing a rule later is cheap where it loosens (a new argument type, a new condition)
  and costly where it tightens or renames, because every ported action and the generated
  reference follow the descriptor.

What this record does not decide, and the code does not do:

- How a call is prepared, admitted and executed, the outcome, and the run record. They are
  the rest of the execution contract and get their own record.
- What a feature is. A need names features; the capability interfaces say which exist.
- A result that is not an artifact, such as a verdict. An output port returns one of the
  six artifact types (`docs/carry-over.md`).
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
  Turned down until a tool needs it: every case in the two fixtures is "this argument is
  given, or it is not", and that can be decided without running anything.
- **A descriptor built from the configuration,** as rascal's was. Turned down: section 6
  wants the static description identical on every machine for the same build.
- **Keeping description version 1 and adding fields.** Turned down: an action went from two
  fields to nine, and a reader that knows version 1 would not know what it is not reading.
