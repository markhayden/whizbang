# 0026. What a judge of a service may cost: three ceilings, one row of prices, settled at usage

Status: accepted. Names ADR 0010 (what a backend says of cost) and ADR 0016 (the ledger
and the meter), and changes neither.
Date: 2026-11-17
Spec: section 6, "Money"; plan 2A, task S2, item 1
Amended: 2026-11-18, with task F1 of plan 1D (`docs/plans/m2b-checkpoint-1d.md`, item 2),
from the review of the first verdict. Changed under "What a request cost": a status of
success the adapter does not list costs its bound, where it cost nothing. Added: "A long
prompt", "A turn cut off at the output ceiling" and "What a turn records".

## Context

A backend states its price before any request exists (`capability/price.rs`): the
executor reserves what a call may cost at its admission, from the bound of one request
times the requests the action says its need is for (`Requirement::requests`; `check`
says 16), and holds that to the config's cap of one call (`spend/cap.rs`). A judge of a
service is paid by the token, and what a request costs is known only once it is
answered. So the bound of one request has to be fixed by something the adapter holds
before anything is sent, and what a request cost is worked out after.

## Decision

**Three ceilings, each held before a byte is sent** (`whizbang_backends::anthropic::spend`):

- `max_input_tokens`, 40,000 when the table gives none, 1 to 100,000. The adapter counts
  a request's input as an upper bound by construction: one token for each byte of its
  words as written (the JSON, keys and all), and for each picture the provider's
  published formula on its prepared size (one token for each patch of 28 by 28 pixels,
  rounded up on each side, 4,784 at most), and 16 tokens for each message and each
  picture. A request counted over the ceiling is refused `not_sent`.
- `max_output_tokens`, 2,048, 1 to 128,000. A request asking more is sent asking the
  ceiling: the retry of a cut-off turn, which `check` asks with four times the limit,
  included.
- `max_pictures`, 24, 1 to 64. A request of more pictures is refused `not_sent`.

**One row of prices,** for `claude-haiku-5-5`, the model of the study, from squint's
catalogue, read from the provider's page of prices on 2026-10-26: 0.10 dollars a million
input tokens, 0.50 a million output, 0.01 a million read from the cache and 0.125 a
million written to it. A model with no row is refused by the kind.

**The bound of one request** is the input ceiling at the dearer of the input and the
cache-write prices, and the output ceiling at the output price. At the defaults:

    40,000 × 0.125 + 2,048 × 0.50 = 5,000 + 1,024 = 6,024 millionths of a dollar
    16 turns of `check` × 6,024 = 96,384 millionths, about 0.10 dollars

which is within the example config's `call_cap_usd = 2.00` (`docs/guides/config.md`)
twenty times over (`spend::the_example_configs_whole_call_reservation_fits_its_cap`). A
config that raises the ceilings raises the bound, and must raise its cap with it.

**What a request cost** is its answer's usage at the same row, rounded up to a whole
millionth: input, output, cache reads and cache writes each at its price
(`spend::a_judge_turn_reserves_its_fixed_bound_and_settles_at_its_usage`). An answer of
200 that reports no usage, or cannot be read, costs its bound, as the meter charges
any answer that says no charge (`spend::a_judge_turn_with_no_usage_is_charged_its_bound`).
**A status of success the adapter does not list** (2xx other than 200) costs its bound,
as an unreadable answer of 200 does: the provider may have charged it, so it is never
settled at nothing (`answered`, with no charge;
`anthropic::an_unlisted_success_status_is_charged_its_bound`). **An answer of an error
status costs nothing:** the provider charges no request it refused or failed, and the
fault says so (`answered`, at a charge of nothing). A retry is
a request of its own, reserved its own bound and settled at its own usage
(`spend::a_retry_with_a_larger_limit_is_admitted_as_its_own_attempt`). A call whose
reservation is over the cap is refused at its admission, before anything is sent
(`spend::a_judge_call_whose_bound_is_over_the_call_cap_is_refused_before_dispatch`).

**A long prompt.** The row's prices hold for a prompt of at most 100,000 tokens of
input; over that line squint's catalogue gives the long prompt's prices of the same
reading, 2026-10-26: 0.50 a million input, 2.50 output and 0.05 read from the cache. It
gives none for a cache write, which is taken as 0.625, five times the short price, the
factor each long price it does give has, so that the charge is never below the price.
The input ceiling is held under the line, so a turn passes it only when the provider
reports more input than the adapter counted; such a turn's reported input (what was
read from the cache, written to it, and neither, together) over 100,000 is charged
every count at the long prompt's prices, and one on the line at the row's
(`spend::judge::a_turn_whose_reported_input_passes_the_long_prompt_line_is_charged_the_long_prompt_prices`).
Such a charge can pass the bound reserved; the meter takes the excess as for any charge
over its bound (ADR 0016).

**A turn cut off at the output ceiling** is not asked for again with the same limit.
Each turn of the kind says the ceiling it was sent under (`Turn::output_ceiling`), and
`check`, which asks again with four times its limit, ends when that retry would be sent
with no more than the cut-off turn was: a failure of the kind `judge_output_ceiling`
that names the setting to raise, `max_output_tokens` in the judge's table, after one
request (`check::a_turn_cut_off_at_the_output_ceiling_is_not_retried_with_the_same_limit`).

**What a turn records.** The input the adapter counted for a request before it was sent
is kept on its turn (`Turn::counted_input`) and written into the run's exchange of that
turn as `counted_input_tokens`, beside the usage the provider reported, so the two can be
compared turn by turn (`record::each_judge_turn_records_its_counted_input_beside_its_usage`).
An exchange written before has no such field, and is read as having none.

**More than 20 pictures** in one request are each fitted to 2,000 pixels a side at most,
as squint's catalogue states for the model (`many_image_threshold`,
`many_image_max_dimension_px`).

## Consequences

- The count of input holds only while the provider's formula holds (plan 2A, risk 1):
  `anthropic::the_input_count_is_at_least_the_usage_of_every_recorded_exchange` holds it
  to every one of squint's recorded exchanges, and the live call's evidence sets the
  usage it reported beside the bound.
- The long-prompt prices (over 100,000 input tokens) are not needed while the input
  ceiling is under that, and a table that sets it higher is refused.
- A long loop of many `view` turns can pass 40,000 input tokens or 24 pictures and end
  without a verdict (plan 2A, risk 3): the owner raises the settings, and the cap.
- A price row is a constant of the build: a change of the provider's prices is a change
  of this file, its date and its row.

## Alternatives

- **A quote that grows with the request.** Turned down by the plan: a price is stated
  before any request exists, and changing the core's quote is out of S2's reach.
- **Charging an error status its bound.** Turned down: it would count against the
  allowance money the provider does not take; an unreadable answer of 200 is still
  charged its bound, since it may have been charged.
