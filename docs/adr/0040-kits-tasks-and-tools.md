# 0040. Kits, tasks and tools: the words people see

Status: accepted.
Date: 2026-11-20
Spec: section 1 ("Kits", formerly "Top-level tools"), section 3 ("Primitives, recipes and
loops"), section 8 ("Tool records"), section 15, section 16 (row 5); the playground's design
for milestone 5

## Context

The spec used "tool" at two levels. Section 1 called rascal and squint "top-level tools",
while section 3 calls `grow`, `view` and `segment` primitives, section 8 gives "every tool
and recipe" a record, and squint's judge calls `view` and `align` its evidence tools. The
playground's design work for milestone 5 needs one set of words for its navigation, its
labels and its help text, and the set of top-level families will grow (Pixel is the known
next one).

## Decision

These are the words the playground, the guides and the generated reference use. The internal
words stay where they are already the names of formats and code.

| Word | Means | Examples | Internal word, where one differs |
|---|---|---|---|
| **kit** | A family of work with one job, shipped in whizbang | Rascal (edits pictures), Squint (checks pictures), later Pixel | formerly "top-level tool" |
| **tool** | One granular operation | `grow`, `feather`, `segment`, `fill`, `view`, `align` | primitive; squint's evidence tools are tools |
| **task** | A packaged job made of tools | Remove what you name, Repair a spot, Check an edit, Reality check | recipe; squint's use cases are its tasks |
| **task that uses another kit** | A task whose steps call a second kit | Remove, checked (Rascal, uses Squint) | loop |
| **action** | Anything that can be called: a tool, a task or a loop | | action; never shown in the playground |
| **model** | What does model work for a tool | SAM 3, Bria through fal, Gemini 3.8 Flash | capability and backend |
| **run** | One execution: a draft before it starts, an attempt once it runs, then its result | | run |

- A kit is never called a tool, and a tool never a kit.
- The command line keeps its shape: `whizbang rascal …` and `whizbang squint …` name kits.
- File formats, crate names and code keep "primitive", "recipe" and "loop"; a description a
  person reads says "tool" and "task".

## Consequences

- `SPEC.md` section 1 says "Kits" where it said "Top-level tools", and the sections that said
  "top-level tool" say "kit". Section 3 keeps its internal names and gains a line that maps
  them to these words.
- The playground's navigation, labels and help text (milestone 5) use only kit, task and tool.
- Guides and records written from now on use these words. Older guides are reworded as they
  are next edited; the carry-over list holds the rest.
- Reversing costs a rename of the playground's labels and the guides; no format changes.

## Alternatives

- **"App" for a kit.** Turned down: an app inside the playground (and later inside a Mac app)
  reads as a separate program.
- **"Agent" for a kit.** Turned down: it fits Pixel and Squint's judge but not Rascal, which is
  mostly deterministic.
- **"Tool" for the families and "operation" for the granular level.** Turned down by the
  owner, who reads a tool as the granular thing a kit uses.
