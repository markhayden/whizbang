# 0019. The gate: a config's approvals as the executor's policy, and the check behind a send

Status: accepted. Names ADR 0011 (an `Approval`), ADR 0012 (a recipient) and ADR 0015
("A policy before a send, and a checked send"), and changes none of them
Date: 2026-11-14
Spec: section 10, "Egress"

## Context

`SPEC.md` section 10 says: "Data leaves only to an approved recipient", "Config lists
which classes of data each recipient may receive", and "A refusal never falls back to
another remote backend".

Three records hold the pieces. ADR 0011 reads a config's `egress.recipients` table into
approvals: a recipient, the classes of data it may receive, whether it may be probed.
ADR 0012 says who a recipient is and what a request states of the data it holds. ADR
0015 gives the executor a policy (`DispatchPolicy`), which approves nothing until it is
told otherwise, and a checked sender that asks four things of every address before a
check it is given (`HopCheck`), whose default approves no address.

Nothing joined them: no function made a policy of a config's approvals, ADR 0011 left
open "how an approval is matched to a backend", and the only check that approved an
address was a fake. This record is the egress strand's, and joins them. It is added to,
one section for each task of the strand (E3, E4 and E6 of the plan of checkpoint 1B).

What was read, at the commit `docs/sources.toml` pins: squint's `consent`
(`crates/squint-providers/src/privacy.rs`), which let a run go on only when the config
allowed cloud calls and named an account with the prefix of the provider's profile.
Nothing of it is ported:

| In squint | Here |
|---|---|
| `cloud_calls = true`, one switch for everything that leaves | Dropped. A config that names no recipient approves none, and there is no switch that approves all |
| The account must start with the profile's prefix | Rewritten. An approval is of an origin and an account, and a backend's recipient is that approval's only when its origin, its account and its upstreams are the same, each compared whole |
| `Consent` and its list of upstreams | Rewritten by ADR 0012 as `Recipient`, and by ADR 0011 as `Approval` |
| `redact_account` | Rewritten by ADR 0012 as `Account::shown` |

## Decision

The code is `crates/whizbang-core/src/egress/approvals.rs` and `hops.rs`. The tests are
in `crates/whizbang-core/tests/it/egress/gate.rs`, named below as `gate::…`; each builds
an executor with the policy a config's text makes, this record's check, and the test
kit's fake sender. Nothing is sent anywhere.

### The policy a config's approvals make

`DispatchPolicy::from_approvals` takes the approvals of a config and answers the policy
they make, or a refusal. It is the one way from a config to a policy.

- **Each recipient may receive the classes its approval lists, and may be probed when
  its approval says so.** A recipient named with `classes = []` and no probing is
  approved for nothing, as one that was never named is, and has no entry in the policy.
  Probing is approved apart from any class: a recipient that may be probed and nothing
  else may receive nothing, not even a request that holds no data
  (`gate::the_policy_a_configs_egress_table_makes_approves_each_recipient_for_its_classes_and_no_other`).
- **A config that names no recipient makes the policy that approves nothing,** which is
  the policy an executor has when it is given none
  (`gate::a_config_that_names_no_recipient_makes_the_policy_that_approves_nothing`).
- **Approvals that cannot all be taken make no policy.** There is no policy of some of
  them: the answer is a refusal, and whoever asked holds the policy it had, or none.
  Two things are refused. More approvals than a policy names recipients (64,
  `APPROVED_LIMIT`), counted as approvals and not as those that approve something, and
  no more than one past the limit is looked at. And an origin and an account that an
  earlier approval named, whatever the upstreams of either: a config's reader refuses
  such a file (ADR 0011), and approvals that come from two files are held to the same
  rule, because which of the two held would be a guess
  (`gate::approvals_that_name_one_recipient_twice_or_more_than_a_policy_holds_make_no_policy`).

### Which approval a backend has

A backend says who receives its requests in its declaration (ADR 0012). **Its recipient
is the one an approval names only when the origin, the account and the upstreams are
all the same.** Nothing looser is tried: no approval is found by its origin alone.

- **Two approvals of one origin under two accounts are two recipients,** each with its
  own classes and its own probing. An account is compared whole, never by what is shown
  of it.
- **A backend that declares no account has the approval that names no account, and no
  other.** When a file approves an origin under accounts only, a backend that declares
  none is approved for nothing; and an approval with no account approves no backend
  that declares one
  (`gate::two_approvals_of_one_origin_under_two_accounts_are_two_recipients_and_a_backend_with_no_account_gets_only_the_approval_with_none`).
- **The upstreams are the approval's, exactly.** A backend that declares one more, one
  other, or none where the approval names one, is another recipient and is approved for
  nothing
  (`gate::a_backend_that_declares_other_upstreams_than_its_approval_is_approved_for_nothing`).
  Whoever makes a backend from a config makes its recipient from the approval itself.

### The check behind a send, for a first address

`SendCheck` is the `HopCheck` an executor that sends is given. The executor has asked
its four questions before it (ADR 0015): the backend declared a recipient, the request
brings no credential, the address is at the recipient's own origin or one of its
upstreams, and the policy approves the recipient for every class of data the request
holds. The check is asked last and can only refuse
(`gate::under_the_strands_check_a_request_is_still_held_to_what_its_backend_declared_and_reaches`).

**It approves an address while no answer has pointed its backend elsewhere in the run,
and refuses every address after one has,** in fixed words. Following an answer that
points elsewhere is task E3's, and until then nothing is followed. An upstream is
approved as a first address: an upload address comes inside an answer, and the request
to it is one no answer pointed to
(`gate::the_check_approves_a_first_address_and_no_address_an_answer_pointed_to`).

It goes by the moves of the whole run (`Hop::total_moves`), which never go back, and
not by the moves in a row, which an answered request of the same backend sets back to
nothing: with two requests through one handle at once, the address an answer pointed
to would otherwise pass for a first one
(`gate::a_request_that_was_answered_after_an_answer_pointed_elsewhere_does_not_make_the_next_address_a_first_one`).

It holds nothing and counts nothing. A check is still asked for a sender that was kept
past its run (ADR 0015), and being asked changes nothing.

### What the tests of the plan's task show, with this policy and this check

- With the policy of a config that names no recipient, every request of a backend that
  names a recipient is refused, whatever it holds and whichever of the recipient's
  origins it is for, and the sending fake is handed none
  (`gate::with_the_empty_policy_every_request_to_a_backend_with_a_recipient_is_refused_and_the_sending_fake_is_handed_none`).
- A recipient approved for prompts and not for source images is refused a request that
  holds a source image, alone or beside a prompt, and is sent one that holds a prompt
  alone, with the credential the executor holds for that origin and no byte of it in
  the store
  (`gate::a_recipient_approved_for_prompts_and_not_for_source_images_is_refused_a_source_image_and_sent_a_prompt_alone`).
- A part with no stated class is refused unless every class is approved: approved for
  any three of the four, the recipient is refused it
  (`gate::a_part_with_no_stated_class_is_refused_unless_every_class_is_approved`).
- A refusal answers the action with a fault, `rejected`, which states that nothing was
  sent and states no charge; and the executor asks no other backend in its place, of
  those the action was handed: not a local one, and not a remote one whose recipient is
  approved for everything
  (`gate::a_refusal_answers_the_action_with_a_fault_and_no_other_backend_is_asked_in_its_place`).
- A run whose action catches the refusal and completes has it in its record: among its
  refusals, as `request_not_approved`, with the origin, the reason and the classes; and
  in where its data went, as `refused`, with the origin and the account cut. Nothing of
  the address but its origin is in the store
  (`gate::a_run_whose_action_catches_the_refusal_and_completes_has_it_in_its_record_with_the_recipient_and_the_reason`).

### Limits, each pinned or stated

- **A refused backend is still called.** The plan refuses a request at the gate's
  place, before its backend is called and before the meter's place sees it. That place
  is task I3's and is not built (ADR 0015, "Limits"). So three things the plan's task
  asks are not shown here, and are task I3's to show (`docs/carry-over.md`): that the
  backend's own count of requests is none; that a fake meter in the place after the
  gate saw nothing; and that a refused request has no dispatch record. What is shown is
  that nothing is handed to a sender, and that the fault states no charge. The pin
  `gate::known_until_the_gates_place_is_built_a_backend_that_is_refused_was_still_called`
  holds what is so today, and task I3 turns it round.
- **"No other backend is asked" is shown of the executor as it is,** which has no
  fallback to take: a run is handed the backend its call resolved (ADR 0009). The test
  holds that a refusal adds none. It cannot show that a backend refused at the gate's
  place is not replaced, since there is no such place yet.
- **Once an answer has pointed a backend elsewhere, every later request of that
  backend in that run is refused,** a first address among them. The counts a check is
  shown are for one backend of one run, not for one request (ADR 0015). Task E3 decides
  what is followed.
- **A class is what whoever built the piece stated** (ADR 0012). Nothing here knows a
  source image from a derived one.
- **Nothing here reads a config or gives an executor its policy.** That is tasks I6
  and C5, and the binary has no ways out until task W1.
- **`may_probe` is answered and nothing asks it yet.** The checked probe and the
  checked download are task E3's.

### For later tasks

- **Tasks C4 and C5.** A backend's declared recipient is made from the approval
  (`Approval::recipient`), not put together a second time from a backend's address:
  another account or another list of upstreams is another recipient, approved for
  nothing. A refusal of `from_approvals` is a config that gives no snapshot, not a
  policy of what could be taken.
- **Task I3.** The gate's place asks the same policy the same question,
  `may_receive(recipient, classes)`, with the classes a capability's request holds
  before any piece is made; a request it refuses does not reach the checked sender.
- **Task E3.** `SendCheck` is where a limit on a chain of answers goes.

## Consequences

- An executor built with `Ways::through(sender).checked_by(SendCheck).under(policy)`
  sends a first address to a recipient the config approves for what the request holds,
  and nothing else.
- A config's `account` and `upstreams` are part of what is approved. Changing either in
  the file and not in the backend, or the other way, approves nothing.
- Reversing this costs the tasks that make a snapshot of a config (C5) and the rest of
  the egress strand.

## Alternatives

- **An approval found by its origin when the account does not match.** Turned down: an
  approval of one account would then let data go under another, which is the one thing
  an account in an approval is there to stop.
- **A backend with no account takes the only approval of its origin.** Turned down for
  the same reason: which account a request is made under is the backend's to say, and
  one that says none is not known to use the approved one.
- **The upstreams of a backend a subset of its approval's.** Turned down for now:
  equality is ADR 0012's and ADR 0015's rule, and a looser one can be added when a
  backend needs it; a stricter one could not be added later without breaking a config.
- **A policy of the approvals that could be taken, with the rest left out.** Turned
  down: a config that is half applied is not the config a person wrote.
- **The check asks the policy and the recipient again.** Turned down: the executor
  asks them first, of every address, so a second asking could never refuse and no test
  could hold it.
- **The check goes by the moves in a row.** Turned down: a request answered meanwhile
  sets them back.
