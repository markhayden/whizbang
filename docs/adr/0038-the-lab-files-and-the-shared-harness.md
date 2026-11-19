# 0038. The lab's files, and the shared harness

Status: accepted.
Date: 2026-11-19
Spec: section 7 ("The lab": "Kept, on the shared harness"), section 9 ("Run resume"),
section 16 row 3; plan m3 (`docs/plans/m3-milestone-3.md`), task L1, with L2 and L3 after
it; ADR 0007, ADR 0033

## Context

Rascal's lab compared two ways of making a mask on the owner's private corpus, blind, from
three committed files (`crates/rascal-core/src/lab/corpus.rs`, `protocol.rs`, `matrix.rs`,
`cell.rs` and `docs/lab.md` at the commit `docs/sources.toml` pins) and a cell loop of its own
in `lab_run.rs`. Whizbang already has a resumable runner of cells: E1's matrices
(`crates/whizbang/src/evaluation.rs`), which ran only `check`. The spec keeps the lab "on the
shared harness", so a lab round and an evaluation matrix are run by one runner. The pictures
of the corpus and the owner's picks are the owner's: they never enter the repository, and no
file of the lab may name a path of the owner's.

## Decision

**The shared harness** (`crates/whizbang/src/harness.rs`). A cell is anything that says its
identity, the request one attempt of it is (or none, for a cell that runs nothing), its
forecast, and the group it is prepared with. An attempt is recognised by the cell's identity
in any run's record, by a recogniser the caller gives: a run of `check` by its argument
`cell`, as E1's matrices always were, so their stores resume as before and `eval matrix` is
unchanged; a cell of any other action by its request id, `cell.<scope>.<identity>.<attempt>`
(ADR 0007), which the run's record holds. The rules of E1 stand for every cell: completed is
not run again, failed is kept and not run again, interrupted is run again as the next
attempt; the forecast of the cells still to run is set against what the allowance has left
before anything runs; at most `at_once` runs are open. A judge's cell is forecast from the
study's rates, as before; a lab's cell from its filler's price per call for each fill the
recipe sends and its finder's for each find; a reference arm's cell at nothing.

**The lab's three files** (`crates/whizbang-eval/src/lab/`), read strictly and checked before
anything runs.

- `corpus.toml` is rascal's manifest (`version`, and per picture `id`, `file`, `sha256` as
  `sha256:<hex>`, `group`, `licence`), so rascal's `seam-03` manifest reads as it is. `file`
  is a plain path relative to the corpus's root, which the user gives on the command line
  (`whizbang lab corpus verify --manifest <file> --root <dir>`); the manifest names nothing
  else of the machine. `group` may be left out for a picture the matrix only binds (a mask,
  the clean picture of a reference arm).
- `protocol.toml` is rascal's protocol: name, seed, the two arms compared (`arms = [a, b]`,
  wins are a's), `min_decided` (10 by default), the rules as fractions, and the groups. Its
  digest is in every cell's identity and in the scope of every run of the round
  (`lab-<12 hex of the name's SHA-256>-<16 hex of the protocol's digest>`); a store holding
  runs of the lab's name under another digest refuses the protocol: a change is a new lab.
- `matrix.toml` is rascal's matrix with three changes. An arm runs a recipe of the build by
  name, its filler and finder being backends of the config (`filler`, `finder`), passed to
  the recipe's input that chooses each; overriding that input is refused, as the runner
  refuses it. Every file an image binds is a picture of the corpus by id (`inputs =
  { protect = "<corpus id>" }`), never a path, so it is pinned by digest and verified with
  the rest. Two bindings rascal lacked: `texts`, a text for each image (`remove_prompt`'s
  `prompt`), and the reference arm (`reference = "<input>"`), which hands back the picture
  the image binds at that input and runs nothing. The matrix runs exactly the protocol's two
  arms: an arm the protocol does not compare is refused, as is one it compares that the
  matrix lacks. The dependent-finder rule is rascal's, a finder being a step whose action
  needs a segmenter.

**A cell's identity** is the SHA-256 of its components: the protocol's digest, the picture's
digest, each bound input's digest and each bound text, the overrides as applied (the arm's,
then the image's), the recipe as it runs (its name, the digest of its text and the runner's
version) or the reference's input, the name and digest of each constant set a step selects,
the version of each action, and the filler's and each finder's model, revision and resolved
settings. An arm's name, a backend's name in the config and a file's name are not in it.

**Room for a lighter way to pick, not built.** The owner may later want fewer pairs, or a
judge to pre-sort the pairs so that the owner settles only the close calls. L3's files leave
room for it: a pre-sort would be a file of its own beside the frozen assignment,
`presort.json` (the assignment's digest, the judge, and for each pair id its leaning and
how sure it is), and the protocol would gain a `[presort]` table (the judge, the threshold
below which a pair goes to the owner, how many pairs are drawn); since the protocol is
frozen, adding it is a new lab. The picks stay the owner's: a pre-sort never writes into the
picks file, the decode reads the owner's picks alone, and a pair the pre-sort settled is
counted apart, never as the owner's pick.

## Consequences

- One runner holds the rules of resuming and spend for every cell; the lab's cell loop is
  not ported (L2 runs `lab run` on it).
- The owner's pictures and picks stay outside the repository: the committed files are
  manifests, protocols and matrices with ids and digests only, and every path is relative
  to a root given when the command runs. Tests use synthetic pictures.
- A lab arm names a recipe this build ships, or (from task L2) one of the config's own
  recipe files by its name.
- Each attempt of a lab's cell adds the two names of a request id to the store's
  generation (`docs/carry-over.md`, the row on publications writing the whole generation):
  about a hundred for a round of `seam-03`'s size, against the tens of thousands that row
  measures. An evaluation matrix still gives its runs none.
- Reversing the request-id recognition would orphan the runs of rounds already made; the
  scope's form is therefore fixed here.

## Amended with task L2

- **An id alone is no attempt.** A run is a cell's attempt only when its request id names
  the cell and an attempt, its action is the one that attempt calls, and its admission holds
  the fingerprint of the very request the cell makes for that attempt; any other run under
  such an id is passed over, and the cell's next attempt takes the next number whose id no
  run holds. No caller may give an id that starts `cell.`: the executor refuses it at the
  door it hands a caller's id through, so the CLI and the MCP server refuse it alike
  (`RESERVED_REQUEST_PREFIX`).
- **Backends from the config.** `whizbang lab run` resolves each arm's filler and finder
  from what the config offers: the model, the revision (an installation's identity for a
  local model), the settings, the provider (the kind, where the config names none) and the
  address, all in the cell's identity, and the most one request may cost as its forecast. A
  backend the config does not give, one not ready, and one that names a recipient and
  states no price are refused, never priced at nothing; so is a recipe that needs a
  capability other than fill and segment, which a forecast would not count. Two cells of
  one identity in a matrix are refused.
- **The corpus brought in.** Every picture the matrix binds is read from the corpus's
  folder through no link, held to the manifest's digest, and brought into the store as the
  type the recipe's input declares: a picture as an image, a mask or a location as its
  canonical file. What `lab run` and `lab decode` print is counts; a refusal names a file
  by its path under its folder, and a file not of its form by the place, never by the
  reader's words, which could quote the owner's line.

## Amended with task L3

- **Pairs and the frozen assignment.** `whizbang lab pairs` draws one pair for each image
  the protocol judges, its order and sides from the protocol's seed by rascal's draw
  (splitmix64, Fisher–Yates, a bit a pair), and writes `assignment.json`, the page and its
  pictures in the folder `--out` names (by default `pairs/<a>-vs-<b>` in the corpus's
  folder), never in the store's and never inside a git work tree unless `--force`. The
  picks carry the digest of the assignment file; `lab decode` refuses picks made on
  another, and an assignment that is not what the seed draws (its images, order or sides
  changed), before it counts anything.
- **The page.** It loads nothing from the network: its policy is `default-src 'none'`,
  pictures from its own place or a local file, its own script and style by their digests.
  Every string is escaped for where it goes, its script writes text and never markup, and
  the picks leave it only as a file the owner saves; nothing is kept in the browser.
  What a picker who inspects the folder could still find: the files' sizes, which can
  differ by arm, and `assignment.json`, which names the arms and is not to be opened
  before the picks are saved.
- **A lighter round** (`--limit <n>`, the owner's wish of 2026-11-18): `n` pairs drawn by
  the protocol's groups, each its share of `n` by the largest remainder, its images drawn
  from the seed's own stream; the order and sides are drawn as for a whole round. The rules
  do not change: the assignment says it is a subset, every group is decoded, and a group
  with fewer decided pairs than `min_decided` is `inconclusive`, never a win; `lab pairs`
  says so of each group before anything is picked. A judge's pre-sort stays unbuilt, as
  above.
- **What a refusal may name.** A refusal of the lab may name a corpus id, a group, an arm
  or a lab by its name, and nothing else read from a file: each is held to a plain name
  (1 to 64 letters, digits, `-`, `_` and `.`) by the reader of the file that holds it
  before it can be said, so it carries no path, line break or markup; and each is from a
  file committed with the lab (the manifest, holding ids and digests only, the protocol,
  the matrix) or from the assignment drawn from them, whose pair ids and groups are held to
  plain names, unique, when it is read. A pick naming a pair not in the assignment is
  named by its place in the list, since the picks file is the owner's; a file is named by
  its path under its root; a file's contents are never quoted.
- **An id held by a run of the cell's own request** is not passed over: the harness passes
  over only an id whose run's admission holds another request's fingerprint. A run of the
  same request, or one whose admission cannot be read, is left to the executor, which
  answers the request it already ran rather than start a new, paid attempt.

## Alternatives

- **The cell in an argument of every action**, as `check` has: each action's descriptor
  would need an argument it never reads; a request id is already recorded for any action.
- **A lab directory with its own record** (rascal's `lab.json`): a second store beside the
  store, which the spec's runs already are.
- **Matrix inputs as files beside the matrix** (rascal's): a committed matrix would name the
  owner's files; a corpus id names a picture pinned by digest instead.
