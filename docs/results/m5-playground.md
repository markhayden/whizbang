# The loop by hand in the playground: what happened

Plan m5, task E1, items 1 and 2. The by-hand check of the loop `remove_checked` that
milestone 4 left to the playground (`docs/carry-over.md`, "Acceptance checks moved here"),
run live in `whizbang play` on the owner's Mac, with SAM 3 on the Mac finding the phrase,
Bria Eraser through fal filling and a judge in the cloud checking; and one quit with a paid
run going. Each run's figures were read from its record. Only aggregates and run ids are
kept here, never a picture, its name, a path or a key; each result screen's screenshot is
kept out of the repository.

**What this is not.** It scores nothing. It shows that the playground runs the loop, shows
its tries, says why a run was not checked, and accounts for a quit, as a person sees them.
Whether a retry makes a result better is milestone E's.

**Pending.** The runs are not made yet: every field below says `pending` until they are,
and then holds its value. `evidence::the_playgrounds_record_holds_only_aggregates_and_run_ids`
holds the page either way.

## The runs

```toml
cases = "pending"
settled_micros = "pending"
```

## Three cases by hand

The cases of the carry-over list: `pass`, a clean pass, or a retry then a pass; `vague`, a
prompt that names nothing in the picture, showing its tries and its flag; `no-key`, an env
file without the judge's key, leaving the run unchecked and saying why.

### `pass`

```toml
case = "pass"
run = "pending"
commit = "pending"
whizbang = "pending"
outcome = "pending"
verdict = "pending"
attempts = "pending"
kept = "pending"
ended_as = "pending"
settled_micros = "pending"
reserved_micros = "pending"
```

### `vague`

```toml
case = "vague"
run = "pending"
commit = "pending"
whizbang = "pending"
outcome = "pending"
verdict = "pending"
attempts = "pending"
kept = "pending"
ended_as = "pending"
settled_micros = "pending"
reserved_micros = "pending"
```

### `no-key`

```toml
case = "no-key"
run = "pending"
commit = "pending"
whizbang = "pending"
outcome = "pending"
verdict = "pending"
attempts = "pending"
kept = "pending"
ended_as = "pending"
unchecked_why = "pending"
settled_micros = "pending"
reserved_micros = "pending"
```

## A quit during a paid fill

One loop started from the page and the playground stopped from the terminal while its fill
was going: its record says cancelled, `whizbang spend status` names nothing open, and the
store takes a writer at once.

### `quit`

```toml
case = "quit"
run = "pending"
commit = "pending"
whizbang = "pending"
outcome = "pending"
open_reservations = "pending"
store_took_a_writer = "pending"
settled_micros = "pending"
reserved_micros = "pending"
```
