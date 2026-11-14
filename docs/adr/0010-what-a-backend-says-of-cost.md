# 0010. What a backend says of cost: a quote is free, a bound, or an estimate only

Status: accepted. Changes ADR 0006 ("Declarations and features": a declaration may state
a price) and names ADR 0007 ("The estimate": what `Backends::bound` answers from)
Date: 2026-11-13
Spec: section 4, "Backends"; section 6, "Spend"

## Context

`SPEC.md` section 6 says: "A limit is enforced; an estimate is advice. Each backend
states which of its numbers is an enforceable upper bound on what a request can cost. A
paid request with no enforceable bound is refused before dispatch." And of a price by
time: "GPU time on a self-hosted backend has a configured price per second. Its bound
must come from a limit the host itself enforces, or a fixed maximum charge; a timeout on
whizbang's side is not a bound."

What checkpoint 1A left, at the tag `m1a`:

- ADR 0007 fixed the unit, millionths of a US dollar, and the type that carries it,
  `Estimate`. Prepare sums `Backends::bound(capability, model)` over a call's needs; a
  sum that passes the largest amount stops there, and that value is not an amount: an
  executor with a ledger refuses such a call with `Refusal::NoBound`. `Backends::bound`
  answers nothing unless an implementation says otherwise, and no implementation in the
  build did. "Which of a backend's numbers is an enforceable bound" was left to 1B.
- ADR 0006 gave a backend a `Declaration`: the model that answers, and features. It had
  no place for a price ("Not carried over from squint's provider interface: a backend's
  prices"), and its fields are private, so it can gain one.

What was read, at the commits `docs/sources.toml` pins:

- Squint's `Pricing` (`crates/squint-core/src/ledger.rs`): `bound(shape)`, the upper
  bound in US dollars, as a 64-bit float, of a request of a stated shape (input tokens,
  output tokens, images), "or an error when it cannot be bounded"; and `cost(usage)`.
  Its catalogue (`crates/squint-providers/src/catalog.rs`, `Prices`, `bound_tier`) priced
  a request from per-million-token prices, with a dearer tier for a long prompt. Every
  bound there was a number computed from a shape; a price that is advice only had no
  form of its own.
- table-manners `CONTRACT.md`, section 4.5: a call's worst case is reserved before any
  paid step, and "unknown spend is never reported as zero".

No backend with a price exists in the build. This record decides the types; the fakes
that quote are task B3's, and the ledger that reads a quote is the spend strand's.

## Decision

The code is `crates/whizbang-core/src/capability/price.rs`, with one method added to
`Declaration` and one to `Roster` (`declare.rs`) and one constructor to `Estimate`
(`contract/prepare.rs`). The tests are in
`crates/whizbang-core/tests/it/capability/price.rs`; each is named below as `price::…`.

### A quote

A `Quote` is what one request to a backend may cost, in millionths of a US dollar, and
is one of three kinds (`QuoteKind`):

| Kind | Says | The most it may cost (`Quote::most`) |
|---|---|---|
| Free | The backend said that a request costs nothing | Nothing |
| Bound | The request is never charged more than this amount, and something other than whizbang's own patience holds that: a fixed maximum charge, or a limit the backend's host enforces | The amount |
| Estimate only | About this much is expected, and nothing holds a request to it | The amount that is no bound |

- **An amount is a whole number of the unit, under the largest.** `Quote::bound` and
  `Quote::estimate_only` take a number as a 64-bit float, which is how a config and an
  adapter's arithmetic hold one. A number that is not finite, or is less than nothing,
  is refused with an `Unfit`, whose sentence is fixed words and the number
  (`price::a_quote_cannot_be_made_from_a_number_that_is_not_finite_or_is_negative`,
  `price::a_refusal_of_a_number_says_which_number_and_what_is_allowed`). So is a number
  of 2 to the power 64 or more: the largest value a 64-bit whole number holds is ADR
  0007's "no bound", and no quote holds it as an amount
  (`price::a_quote_is_under_the_largest_amount_which_is_no_amount`).
- **A fraction of the unit is rounded up** to the next whole unit, never down: a bound
  that was rounded down would be under what may be charged
  (`price::a_quote_made_from_a_fraction_of_the_unit_is_rounded_up`).
- **Free is a word, not the number nothing.** A bound of nothing is a bound, and is not
  equal to free (`price::a_quote_says_its_kind_and_its_amount`). Both cost nothing. The
  difference is who may be believed: task B2 decides that a backend with a recipient is
  free only when it says so.
- **A quote becomes the answer of `Backends::bound` through `Quote::most`:** nothing for
  free, the amount for a bound, and for an estimate only the largest amount
  (`Estimate::unbounded`), whatever the estimate was, an estimate of nothing included
  (`price::the_most_a_quote_may_cost_is_nothing_its_bound_or_no_bound`). So a call that
  needs a backend with an estimate only has an estimate with no bound, by the sum ADR
  0007 already makes, and an executor with a ledger refuses it with `Refusal::NoBound`
  by the check ADR 0007 already has
  (`admit::a_call_whose_cost_has_no_bound_is_not_admitted_and_the_ledger_is_not_asked`,
  as at the tag). Nothing in the executor changed for this.

A quote has private fields, is made only through its three constructors, and is never
deserialised.

### A backend's statement of its price

A `Price` is what a backend states, from its catalogue entry or its config, before any
request exists. It is one of four statements, and `Price::quote` gives the quote of one
request under it:

| Statement | Made by | Its quote |
|---|---|---|
| Free | `Price::free()` | Free |
| A fixed maximum charge for a request | `Price::at_most(micros)` | A bound of that amount |
| An estimate | `Price::about(micros)` | An estimate only |
| A price by time | `Price::by_time(micros_per_second, limit)` | Under a limit its host enforces, a bound of price times limit; under a timeout of whizbang's own, an estimate only, of price times timeout |

**A price by time, and what makes it a bound.** A `TimeLimit` is a count of
milliseconds, one or more, and says who enforces it: the backend's host
(`TimeLimit::of_host`), which stops the work and the billing at the limit whatever
whizbang does; or whizbang (`TimeLimit::own_timeout`), which stops waiting and proves
nothing about the host (`SPEC.md` section 6: "a timeout does not prove a remote host
stopped billing").

- Under a host's limit the quote is a bound of the price times the limit
  (`price::a_price_by_time_with_a_hosts_limit_gives_the_bound_of_price_times_limit`).
- Under whizbang's own timeout alone it is an estimate only, and is no bound when asked
  through `Backends::bound`
  (`price::a_price_by_time_with_only_a_timeout_of_whizbangs_own_gives_an_estimate`).
- **A fixed maximum charge** may be added to any price (`Price::charged_at_most`), and
  makes its quote a bound: of that charge, or of the smaller of it and a bound the
  price had already. A free price stays free
  (`price::a_fixed_maximum_charge_makes_a_price_by_time_a_bound`,
  `price::a_fixed_maximum_charge_bounds_any_price_and_leaves_a_free_one_free`).
- The price a second is rounded up to a whole unit where it is stated, and the product
  is worked out in whole numbers, `price × milliseconds / 1000`, rounded up
  (`price::a_price_by_time_is_rounded_up_where_the_rate_enters_and_where_the_product_is_made`).
  The product is worked out once, when the price is stated, and one that reaches the
  largest amount is refused there, so that `Price::quote` cannot fail
  (`price::a_price_by_time_that_reaches_the_largest_amount_is_refused_when_it_is_stated`).
  A limit of no time is refused (`price::a_limit_of_no_time_is_refused`).

**Whizbang cannot check who enforces a limit.** `TimeLimit::of_host` is a statement by
whoever configures the backend, as a mask is the statement of whoever made it (ADR
0006). The type makes the two limits two constructors, so that a timeout cannot become a
bound by being passed along; it does not make a host stop.

**A declaration may state a price** (`Declaration::priced`; read back by
`Declaration::price` and, as a quote, `Declaration::quote`). A declaration that states
none is as it was (`price::a_declaration_says_the_price_it_was_given`).

**`Roster` answers `Backends::bound` from the prices declared.** Asked for a capability
and a model, it looks at each backend it offers for that capability that declares that
model:

- when none states a price, or none declares the model, the answer is nothing, as at
  the tag `m1a` (`price::a_backend_of_before_that_states_no_price_answers_as_it_did`);
- otherwise it is `Quote::most` of the price stated
  (`price::a_quote_of_an_estimate_only_asked_through_backends_bound_is_the_amount_that_is_no_bound`,
  `price::a_bound_and_a_backend_that_says_it_is_free_are_answered_through_backends_bound`),
  and a price is asked only of the capability and the model it was declared for
  (`price::a_price_is_asked_of_the_capability_and_the_model_it_was_declared_for`).

What `{:?}` prints of a quote, a limit and a price is its kind and its numbers
(`price::debug_of_each_new_type_shows_no_more_than_its_numbers`). None of them holds a
text.

### Limits, each pinned

- **Two offers of one model are not told apart.** `Backends::bound` is asked with the
  capability and the model, not with the name the call chose (ADR 0007). When two
  backends offered for one capability declare the same model with different prices,
  the answer is the largest, and no bound when either has none: over, never under
  (`price::known_two_backends_of_one_capability_that_declare_one_model_are_bounded_by_the_dearer`).
- **A number that arithmetic made a little too large costs a whole unit more.** 0.0079
  dollars is 7,900 millionths; worked out as `0.0079 × 1,000,000` in floating point it
  is a little over, and its quote is 7,901
  (`price::known_a_fraction_that_floating_point_arithmetic_made_is_rounded_up_a_whole_unit`).
  Whoever reads dollars from a config converts in whole numbers; that reader is the
  config strand's.
- **A stated price is for one request, and does not look at the request.** Squint's
  bound was worked out from a request's shape: its tokens and images. Here a backend
  priced by usage states the most one request may be charged (`Price::at_most`), which
  its adapter works out from its own limits on a request, or it states an estimate.
  Quoting from a request's shape comes with the first backend that needs it
  (`docs/carry-over.md`). No test pins this: there is no code to be wrong.
- **A backend that states no price answers nothing,** whoever receives its requests.
  That is the tag's behaviour, kept until task B2 decides which declarations go
  together.

### What this record leaves to others

- **What a request cost, and how far a failed one got,** is task B1b's, and is added to
  this record by its pull request.
- **Which prices go with which recipients** (a backend with a recipient and no price has
  no bound; free with a recipient is said, never assumed) is task B2's.
- **Refusing a request when it is about to be sent, the per-call cap, and the ledger**
  are the spend strand's (tasks S3, S4 and S5 of the plan). Nothing here reserves,
  records or refuses.

## Consequences

- `Backends::bound` has its first implementation that answers something. Every
  implementation of 1A answers as it did, and so does every roster whose declarations
  state no price: no test of 1A changed.
- A backend with only an estimate cannot be used by an executor with a ledger. That is
  what `SPEC.md` section 6 asks; the way out for such a backend is a fixed maximum
  charge, which is a number its operator stands behind.
- `Declaration` compares and prints its price, so two declarations of one model with two
  prices are not equal.
- The estimate of a prepared call can now change when a price does; admission already
  compares it (`Stale::Estimate`, ADR 0007).
- Reversing this costs every backend that states a price. None exists but in tests.

## Alternatives

- **Amounts as whole numbers only, with no constructor from a float.** Turned down: a
  price from a config and a price worked out from a rate are fractions somewhere, and
  each caller would round its own way. One place rounds, upward.
- **Round to the nearest unit.** Turned down: half the time a bound would be under what
  may be charged.
- **An estimate only as an `Option` with no amount.** Turned down: the amount is what a
  person is shown when the call is refused, and what `whizbang` can say a call would
  have cost.
- **A bound for a price by time from whizbang's own timeout,** as the simplest reading
  of "price times limit". Turned down by `SPEC.md` section 6.
- **One `TimeLimit` constructor with a flag.** Turned down: a flag is passed along; two
  constructors with two names are chosen where the limit is known.
- **Have `Backends::bound` take the name a call chose,** so that two offers of one model
  are told apart. Turned down here: it changes ADR 0007's interface and every
  implementor for a case no config has yet, and the answer without it errs upward.
- **Answer "no bound" for every backend that states no price.** Turned down for this
  task: every local, free backend of 1A would have to say so before it could run under
  a ledger, and which backends must say is B2's decision, with the recipient.
