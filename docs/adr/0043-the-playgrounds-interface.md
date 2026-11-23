# 0043. The playground's interface: Preact and Vite, built and committed

Status: accepted.
Date: 2026-11-21
Spec: section 3 ("The playground"), section 10 ("Two HTTP surfaces"), section 12 (`ui/`),
section 13 (the fast tier's UI type checks and unit tests, the slow tier's Chrome end to
end), section 14 (`ci`); plan `docs/plans/m5-the-playground.md`, "Choices this plan makes"
and task U1, which names this record ADR 0042. That number was taken by milestone E's
"Submission v2" before this one was written, so this record is 0043.

## Context

Milestone 5 gives whizbang its third door, the playground: a page in Chrome served by the
`whizbang` process over loopback (ADR 0041, the host). The page has seven screens and about
thirty components, drawn in the Paper file "whizbang playground" in light and dark. The
spec does not choose a front-end stack, say how an interface ships inside the one binary,
or where a Chrome tier runs inside the budgets of section 13. Whizbang's users install one
archive and need nothing else: no Node, no package manager, no network at run time. A user
of the page must load nothing from anywhere but the host, whose content security policy
says `default-src 'self'`.

## Decision

**The stack.**

| Need | Choice |
|---|---|
| Components a designer can move to Storybook later | Preact function components, each in its own file under `ui/src/components/`, taking props only: no request and no application state. Their styles are one sheet, `ui/src/components/components.css`, a section a component |
| Tokens | `ui/src/tokens.css`: CSS custom properties by the Paper file's names (`--color-panel`, `--text-md`, `--radius-md`), at its exact values; dark under `:root`, light under `[data-theme="light"]`. The page is dark only (`<html data-theme="dark">`); the light set is kept so light can return. The few colours the Paper components use as literal values (the ink on a filled tag, a mark's label, a shadow) are named tokens too, in both sets |
| Type checks and unit tests (fast tier) | `tsc --noEmit` and Vitest under `happy-dom`, comparing text and attributes, never pixels |
| Chrome end to end (slow tier) | Playwright's runner driving the Google Chrome installed on the machine (`channel: "chrome"`); no browser is downloaded |
| Build | Vite, to fixed names (`index.html`, `app.js`, `app.css`, `fonts/…`), no source maps, no hash in a name |
| Fonts | Geist, Geist Mono and Bricolage Grotesque (SIL Open Font License 1.1), the Latin subset of the weights used, as WOFF2 files under `ui/public/fonts/`, served by the host. Nothing is loaded from a font service |

The packages are those the owner approved and no other: `preact` (MIT), which ships in the
bundle; and, for development only, `vite`, `typescript`, `vitest`, `happy-dom`,
`@preact/preset-vite` and `@playwright/test`. `ui/package-lock.json` pins every package
and what each brings, all from npm's registry; `npm ci` installs them, with no install
script run and no browser downloaded. `marked`, also approved, is not added here: task W3
adds it, with the one wrapper that may turn a tool's record into markup, and a unit test
(`ui/src/markup.test.ts`) fails until that wrapper is allowed by name. Node is pinned in `scripts/tool-versions.txt` (26, the
long-term release current when this was written, by its exact version), checked by `just
setup`, and set up in CI by `actions/setup-node` pinned to a commit. Node's own types,
which the tests use to read the sources, come with Vitest's and happy-dom's dependencies
and are not a package of their own.

**How it ships.** `ui/dist/` is committed. `just ui-build` writes it; `just ui-check` (in
`just check`) builds it again into a scratch folder and fails when the two differ, and
fails when `crates/whizbang-serve/src/play/assets.rs`, which embeds each file with
`include_bytes!`, does not list exactly the files of `ui/dist/`. So `cargo build`, the
release build and the archive need no Node; the archive still holds five files; and the
bytes that ship are the bytes that were reviewed and scanned. The licences of what the
bundle holds, Preact's and the fonts', are committed under `ui/licences/` with a
list, `ui/licences/list.tsv`, that `scripts/third-party-licences.sh` writes into the
archive's `THIRD-PARTY-LICENSES.md` and holds to `ui/package.json`, `ui/package-lock.json`
and the font files; each font file's source (the npm package and version it was taken
from) is in the list, and its SHA-256 in `ui/licences/fonts.sha256`, which the script
checks.

**The token.** The page reads the launch token from `#k=`, keeps it in the tab's session
storage and takes it out of the address bar at once; it does the same for a `#k=` that a
hash change brings, and takes out of the bar any hash that begins with `#k=`, a token or
not. Session storage lasts for a reload of the same tab and ends with the tab: each tab
`whizbang play` opens carries its own `#k=`, and no other tab or later launch reads it. The
page acts on nothing else in its address: a route only chooses a screen.

**The brand.** The wordmark, the mark, the emblem and the crew illustration are the owner's
art and arrive with task W3 (plan m5, open question 3). Until then the components
`Wordmark` and `BrandArt` set the word in the display face and leave an empty frame; each
takes the art's address when it is committed under `ui/public/brand/`.

**The tests.** The unit tests hold every component in each of its states; that text a run
hands the page (`<img onerror>` in a reason) is shown as those characters; that the tokens
are the Paper file's values and that no source but `tokens.css` writes a colour (in CSS, a
style object or an SVG attribute); that no source writes markup (`innerHTML` and its
kin); and that each text and ground the components use is at least 4.5 to 1 in the dark
theme, with a test that fails when the styles give text a colour no checked pair covers. Their
seconds count under the fast tier's one limit of 300 (`scripts/timing.sh`). The Chrome
cases run in `ui/e2e/` against the fixture host (`crates/whizbang/examples/play_fixture.rs`),
one worker, no retry, 30 seconds a case, the whole run held to 180 seconds of test time by
`scripts/e2e.sh`, in the CI jobs `e2e (macos-15)` and `e2e (ubuntu-22.04)` beside the
others, so the fast tier's 300 seconds are untouched.

**Amended 2026-11-22, by the owner's decision at the end of milestone 5.** The Chrome tier
is held to 240 seconds of test time in all, raised from the 180 above. Since the live fixes
of milestone 5 (#282) CI runs it in two shards on each runner, the jobs
`e2e (<runner>, shard <k>)`, and each shard is held to 120 seconds of its own test time, half
of the 240 (until now 90, half of the 180): the macOS shard 1 had come to 88.9 seconds of its
90. A local `just e2e` runs every case, held to the 240.

## Consequences

- Every machine that runs `just check` needs the pinned Node, and npm's registry is an
  origin that CI and developers fetch from. A user of the binary needs neither.
- A change under `ui/` is not done until `ui/dist/` is built again and committed with it;
  the check says so.
- The bundle may trip the clean-room scan by chance, being minified; then the offending
  package is replaced or the word kept out by the build, and the owner is told.
- The light theme's tokens, as the Paper file holds them, leave fourteen pairs of text and
  ground under 4.5 to 1 (pass and volt on white among them). The contrast test records
  each by name, fails on a new one, and fails when one is fixed and left on the list; the
  design settles them before light is shown.
- Reversing this is reverting the pull request of task U1: `ui/` and the `e2e` jobs go,
  and `just check` loses `ui-check`.

## Alternatives

- **TypeScript with no framework.** Seven screens and thirty stateful components would
  grow a framework of their own.
- **React.** About ten times Preact's size, for nothing this needs.
- **Svelte or Lit.** A compiler or custom elements are one more thing to learn and to move
  to Storybook.
- **A CSS framework.** The tokens already are the system.
- **Building the bundle in `cargo build`** (a build script calling npm). Every build, the
  release's among them, would need Node and the network.
- **WebKit or Firefox in the Chrome tier.** Chrome is the only supported browser (section 3).
- **Fonts from a font service.** The page's policy allows its own host only.
