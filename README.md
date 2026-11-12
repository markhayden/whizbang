# Whizbang

One toolkit for working on images with models: finding and editing things in an image
(`rascal`), judging an image against rules (`squint`), and a playground for both.

**State:** nothing usable yet. The design is in [SPEC.md](SPEC.md); the work in progress is in
[docs/plans/](docs/plans/). It replaces four earlier projects, which stay available as
reference.

What a build can do is listed in [docs/reference/actions.md](docs/reference/actions.md), which
is generated from `whizbang describe --json` and never edited by hand.

```
just setup    # once, on a fresh clone
just check    # format check, lint, fast tests
just docs     # regenerate docs/reference/ after changing what `whizbang describe` reports
```

Licence: Apache-2.0.
