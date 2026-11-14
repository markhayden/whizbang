# 0010. What a backend says of cost: a quote, what a request cost, and how far it got

Status: accepted. Changes ADR 0006 ("Declarations and features": a declaration may state
a price; "Faults": a fault says how far its request got, and each response may carry a
charge) and names ADR 0007 ("The estimate": what `Backends::bound` answers from).
Changed by ADR 0012: a backend that names a recipient and states no price has no bound
Date: 2026-11-13
Amended: 2026-11-13, with task B1b, which this record said would add to it. Added: "How
far a request got, and what it cost"; two consequences and two alternatives. Changed:
the title, which named the quote alone; the status, which now names ADR 0006's
"Faults" as well; and the sentence that left this to B1b. And from the review of the
first pull request: a quote's amount is read as a bound or as an estimate's figure,
by two accessors, in place of one that answered either; a limit is stated in the
host's billing unit; and the limit that a roster's answer is of one request.
Amended: 2026-11-13, with ADR 0012 (task B2). Changed: the limit that said a backend
with no price answers nothing whoever receives its requests, which now holds for a
local backend alone; and the status line, which names ADR 0012.
Amended: 2026-11-14, from the review of the pull request for task B1b. Changed: a fault
that states no stage reads as unresolved whatever its kind, `unsupported` too, which
read as not sent and so could count a backend's own fault as free; the handles mark
their own `unsupported` refusals; and the sentence on what an adapter does when in
doubt, which was true of every kind but that one. Added: that a stage and a charge
come through redaction, and through a wait stated after them; what no charge on an
answered fault means. Named here, which an earlier line left out: the heading "Limits,
each pinned" became "Limits of the quote and the price, each pinned" when task B1b
added its own limits.
Amended: 2026-11-14, with task B3. Added: a charge made of a whole number
(`Charge::whole`).
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

- **An estimate's figure cannot be read as a bound.** `Quote::bound_micros` answers
  the amount of a bound and nothing for the other two kinds; `Quote::estimated_micros`
  answers the figure of an estimate only, which is for showing to a person. No accessor
  answers an amount whatever the kind. Code that counts money reads `Quote::most`
  (`price::a_quote_says_its_kind_and_its_amount`).

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
  A host that bills in whole seconds or whole minutes charges for a unit it has begun,
  so the limit is stated in the host's billing unit, rounded up: a limit of 90 seconds
  on a host that bills by the minute is stated as 120,000 milliseconds. Nothing here
  knows a host's unit; `Price::charged_at_most` is the way to state a firmer maximum.
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

### Limits of the quote and the price, each pinned

- **Two offers of one model are not told apart.** `Backends::bound` is asked with the
  capability and the model, not with the name the call chose (ADR 0007). When two
  backends offered for one capability declare the same model with different prices,
  the answer is the largest, and no bound when either has none: over, never under
  (`price::known_two_backends_of_one_capability_that_declare_one_model_are_bounded_by_the_dearer`).
- **A roster's answer is of one request, and a call may send several.**
  `Backends::bound` is asked for the most one call's use of a capability can cost (ADR
  0007). A roster answers the quote of one request: it does not know how many requests
  a call sends its backend. For a call that sends several to one backend, a judge
  loop's turns or a retry, the estimate from a roster alone is under what the call may
  cost
  (`price::known_the_bound_a_roster_answers_is_of_one_request_however_many_a_call_sends`).
  Task S4 owns the rest: "the bound a backend states for a need covers every request
  the call sends it", in a wrapper around the roster. Until S4, no executor with a
  ledger is built from a roster outside a test.
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
- **A backend that states no price answered nothing,** whoever received its requests,
  when this record was written. ADR 0012 decided which declarations go together: that
  now holds for a local backend alone, and a backend that names a recipient and states
  no price has no bound.

### How far a request got, and what it cost

Added by task B1b. The code is `capability/charge.rs` and `capability/fault.rs`, with a
charge on each response (`fill.rs`, `segment.rs`, `judge.rs`) and the handles' own
faults (`mod.rs`). The tests are in `crates/whizbang-core/tests/it/capability/`, named
below by their file, as `charge::…`, `fill::…`, `segment::…` and `judge::…`; three are
inside the crate, beside the tally they test, as `capability::tests::…`.

What was read for it, at the commits `docs/sources.toml` pins: rascal's
`crates/rascal-mask/src/filler/mod.rs`, whose `Charge` said of every call whether it
was `not_sent` (free), `known` (the filler answered) or `unresolved` (sent, and no
answer settles it: "counted at the price, as unresolved; never as zero"), and whose
`spent` counted a call with no record as unresolved; and table-manners `CONTRACT.md`,
section 4.5, which has the same three and the rule that an answer which reports a
charge or usage is counted at that.

**A charge** (`Charge`) is what a request was charged, in millionths of a US dollar. It
is made from a number as a quote's amount is (`Charge::of_micros`): refused when the
number is not finite, is less than nothing, or is 2 to the power 64 or more, so that no
charge is the largest amount; and a fraction of the unit is rounded up, so that what is
counted is never under what was charged
(`charge::a_charge_cannot_be_made_negative_or_over_the_largest_amount`,
`charge::a_charge_is_a_whole_count_of_the_unit_rounded_up`). Whoever counts in whole
millionths makes a charge of the whole number (`Charge::whole`), exactly: a float holds
every whole number only up to 2 to the power 53, and `Charge::of_micros` rounds one
past that as the float does
(`charge::a_charge_of_a_whole_number_is_that_number_exactly`). A charge may be over the
bound that was quoted: it is a fact about money already spent, and nothing here compares
the two.

**A failed request says how far it got.** A `Fault` has a `Stage`:

| Stage | Name | Says | What the ledger is to make of it (task S5b) |
|---|---|---|---|
| Not sent | `not_sent` | The request did not leave whizbang: nothing was handed to anyone | Nothing was charged |
| Answered | `answered` | An answer came that settles the request, and may say what it cost | The charge the answer gives; its bound when it gives none |
| Unresolved | `unresolved` | The request left, or may have, and no answer settles it | Its bound, counted as unresolved |

An adapter states the stage when it makes the fault: `Fault::not_sent()`,
`Fault::answered(charge)` with the charge when the answer says one, and
`Fault::unresolved()`. The stage stated last is the stage, and only an answered fault
holds a charge (`charge::each_of_the_three_stages_can_be_read_from_a_fault`,
`charge::only_an_answered_fault_holds_a_charge`,
`charge::a_kind_does_not_bind_a_stage_that_is_stated`).

**A fault that states no stage** is every fault made before this task, and every fault
an adapter makes with `Fault::new` alone. **It reads as unresolved, whatever its kind:**

| Kind | Stage when none is stated | Why |
|---|---|---|
| Each of the eight: `cancelled`, `unsupported`, `credential`, `rejected`, `unavailable`, `timed_out`, `malformed`, `too_large` | Unresolved | No kind says how far a request got. `unavailable` is both a backend never reached and one that was sent the request and did not answer; a `credential` or a `rejected` answer usually costs nothing, but only the adapter that read it knows. `unsupported` means "it was not sent" where a handle makes it (ADR 0006), and a backend that returns it was called: it may have found out what it does not support after the request left. Unresolved is the reading that never counts a charge as nothing |

No kind is read as not sent. A fault is not sent only when whoever made it says so.

**That is the conservative reading, on purpose.** An unresolved request may have been
charged, so it is counted at its bound; reading a fault as unresolved can count too
much and can never count too little. A fake or an adapter that knows better must state
the stage: `not_sent` when it knows that nothing left, `answered` when an answer settles
the request. One that states nothing is charged for its silence, at the bound. One test
goes through the eight kinds of ADR 0006's table
(`charge::a_fault_that_states_no_stage_reads_as_unresolved_whatever_its_kind`),
and a stage that is stated is the stage whatever the kind
(`charge::a_kind_does_not_bind_a_stage_that_is_stated`).

**The handle states the stage of the faults it makes itself.**

- A fault a handle makes before it calls its backend is not sent, and the handle says
  so of each: the run was cancelled, its deadline had passed, the backend does not
  declare a feature the request needs, or the conversation holds another adapter's
  part
  (`fill::a_request_the_handle_refuses_before_its_filler_is_called_is_not_sent`,
  `segment::a_request_the_handle_refuses_before_its_segmenter_is_called_is_not_sent`,
  `judge::a_request_the_handle_refuses_before_its_judge_is_called_is_not_sent`); and,
  for a run's own handles, the run's action had come back, the run has sent as many
  requests as it may, or it has no room for the exchange
  (`capability::tests::a_request_refused_once_the_action_has_come_back_is_not_sent`,
  `capability::tests::a_request_over_the_most_a_run_may_send_is_not_sent`,
  `capability::tests::a_request_whose_exchange_the_run_has_no_room_for_is_not_sent`).
- A fault a handle makes of an answer it does not take is answered, with the charge the
  answer carried: a fill of another size, a segmenter's answer that does not fit its
  request, a turn with another adapter's part, a turn over the handle's limit
  (`fill::a_fill_the_handle_does_not_take_is_answered_with_the_charge_it_carried`,
  `segment::an_answer_the_handle_does_not_take_is_answered_with_the_charge_it_carried`,
  `judge::a_turn_the_handle_does_not_take_is_answered_with_the_charge_it_carried`).
- A backend's own fault is handed on with the stage and the charge it has, and an
  answer that is taken keeps its charge
  (`fill::a_fill_that_is_taken_keeps_its_charge_and_a_fillers_fault_keeps_its_stage`,
  `segment::an_answer_that_is_taken_keeps_its_charge_and_a_segmenters_fault_keeps_its_stage`,
  `judge::a_turn_that_is_taken_keeps_its_charge_and_a_judges_fault_keeps_its_stage`).

**What a request that succeeded cost.** Each response (`Painted`, `SegmentResponse`,
`Turn`) may carry a charge (`charged`, read by `charge`;
`charge::each_response_may_carry_a_charge`). The adapter states it: the charge the
provider's answer reports, or one it works out from the usage the answer reports at the
prices it quoted from. A response without one cost its bound; applying that is the
ledger's (task S5b). A response made as checkpoint 1A makes it carries none
(`charge::a_response_made_as_before_carries_no_charge`).

What `{:?}` prints of a fault gains its stage and its charge, and of each response its
charge, and nothing else
(`charge::debug_of_a_fault_and_of_a_response_shows_the_stage_and_the_charge`). Two
faults are equal when their stage and their charge are too, and so are two responses
(`charge::two_faults_that_differ_in_stage_or_in_charge_are_not_equal`). The stage and
the charge are private fields, as ADR 0006 said a fault could gain. Neither is words:
both come through unchanged when a handle takes a run's secrets out of a fault's
sentence, and when a wait is stated after them
(`asked::the_stage_and_the_charge_of_a_fault_reach_an_action_through_redaction`,
`charge::a_wait_stated_after_a_stage_keeps_the_stage_and_the_charge`).

Limits, each pinned:

- **The stage and the charge are the adapter's word.** Nothing checks that a fault
  called not sent was not sent, or that a charge is what was charged. The rule for an
  adapter: `not_sent` only when no byte of the request was handed to anything outside
  the process; when in doubt it states nothing, which reads as unresolved for a fault
  of any kind. The first
  real adapter is held to it with a test (`docs/carry-over.md`). No test here pins it:
  there is no adapter.
- **A fault a backend returns as `cancelled`, `timed_out` or `unsupported` with no
  stage is unresolved,** though the same kinds made by the handle are not sent: the
  backend was called, and may have sent the request before it saw the signal or found
  what it does not support
  (`fill::a_fillers_own_fault_with_no_stage_is_unresolved_whatever_its_kind`,
  `segment::a_segmenters_own_unsupported_fault_with_no_stage_is_unresolved`,
  `judge::a_judges_own_unsupported_fault_with_no_stage_is_unresolved`).
- **An answered fault with no charge is not a fault that cost nothing.** It says that
  an answer settled the request and did not say what it cost: it settles at the bound.
  An answer known to have cost nothing says a charge of nothing.
- **A turn and a fault read back from data carry no charge and no stated stage.** The
  written form of a script and of an exchange (ADR 0009) has no place for either and
  is not changed here: what a request cost is the ledger's record, not the exchange's
  (`charge::known_a_turn_and_a_fault_read_back_from_data_carry_no_charge_and_no_stated_stage`).
- **Nothing is settled, and a run's record says neither.** The record of a run lists
  each request and the kind of fault it ended in (ADR 0009), as before; the stage and
  the charge of a paid request are `run/3`'s (task B4) and the ledger's.

### What this record leaves to others

- **Settling a request by its stage and its charge** is task S5b's, and the place of
  both in a run's record is task B4's.
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
- A fault made with `Fault::new` alone, of any kind, reads as unresolved, and will be charged at its bound once a ledger settles it. An adapter
  that knows better says so; one that says nothing errs upward.
- The handles' own refusals are free by their stage, not by their kind: a ledger does
  not need to know which kinds a handle makes.

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
- **Read the stage from the kind alone,** with `unavailable` split into two kinds.
  Turned down: every kind can be met before or after the request left, so each would
  need splitting, and ADR 0006's eight kinds say whose fault it
  was and whether to ask again, which is another question.
- **Read a fault with no stage as not sent,** since most of 1A's faults were made
  before anything was sent. Turned down: the faults a handle makes are marked where
  they are made, and what is left is a backend's, where a wrong "not sent" releases
  money that was spent.
- **Answer "no bound" for every backend that states no price.** Turned down for this
  task: every local, free backend of 1A would have to say so before it could run under
  a ledger, and which backends must say is B2's decision, with the recipient.
