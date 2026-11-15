# 0012. The ways out: a recipient, four classes of data, and three interfaces

Status: accepted. Changes ADR 0006 ("Declarations and features": a declaration may name
who receives its backend's requests) and ADR 0010 (what a roster answers for a backend
that states no price)
Date: 2026-11-13
Amended: 2026-11-14, after the review of its pull request, before it merged. Changed: a
host that is a number is four plain decimal parts or is refused, and the sentence that
said a host is one host however it was typed; a secret is looked for again in the
origin as it is written, after the host is lowered; a query follows a path; a
credential's constructor is told the secrets; a recipient's upstreams are kept in one
order, and equality does not depend on the order they were given in; what a sink's
"no" means. Added: "For later tasks"; a limit on the rest of an address, pinned.
Amended: 2026-11-14, with task B3. Added: "The fakes".
Amended: 2026-11-14, with task C2, after the config's reader was built on these types.
Changed: an address is looked at for a secret a third time, as it is written, its folded
origin and then the rest. Lowering the host, or leaving out a port that is the scheme's
own, can join a host and a path into a secret that neither the text as given nor the
origin alone holds (`https://HOST/Path` or `https://host:443/Path` for a secret
`host/Path`); such an address is refused as one that holds a secret
(`ways_out::a_secret_that_folding_an_address_makes_across_its_host_and_its_path_is_refused`).
Spec: section 4, "Backends"; section 6, "Spend"; section 10, "Egress"

## Context

`SPEC.md` section 10 says: data leaves only to an approved recipient, "an endpoint, the
credential or account used with it, and any upstream it forwards to"; config lists which
classes of data each recipient may receive, "source images, derived images, prompts,
evidence"; every hop is checked, "redirects, upload services and returned download
addresses", and "credentials are never forwarded to another origin"; "model downloads
and probes are separate, approved traffic and carry no user data"; and every run records
where data went.

Checkpoint 1B builds that against fakes, and its plan puts the pieces in different
hands: the gate that denies (tasks I5, E2, E3), the record of where data went (B4, E4),
the installer that downloads (the models strand). They need one set of types to meet at
before any of them starts. This record decides those types. **It enforces nothing:** no
request is refused here for where it goes.

What there was: a backend's `Declaration` (ADR 0006) names a model and features, and
since ADR 0010 a price. Nothing said who receives a backend's requests, and ADR 0010
left open which prices go with which recipients. No network client is in the build, and
none is added in 1B.

What was read, at the commits `docs/sources.toml` pins:

- Squint's `privacy.rs` (`crates/squint-providers/src/privacy.rs`): `Consent`, bound to
  a profile, an account and a list of upstreams; `redact_account`, which showed an
  account's prefix and the first two characters of its id; `Credential`, read from one
  environment variable, whose `Debug` and `Display` print no value.
- Squint's `http.rs` (`same_origin`): a redirect was followed only to the same scheme,
  host and port as the first address, the port being the scheme's own when none was
  written.
- Squint's `provider.rs` (`recipient`): an adapter said who receives its requests, or
  nobody "when nothing leaves the machine".

## Decision

The code is `crates/whizbang-core/src/egress/` (`recipient.rs`, `classes.rs`,
`ways.rs`), with a recipient on `Declaration` and one rule of `Roster`
(`capability/declare.rs`). The tests are in
`crates/whizbang-core/tests/it/egress/ways_out.rs`, named below as `ways_out::…`; one is
inside the crate, as `egress::ways::tests::…`.

Every type here has private fields and is made through a constructor that checks it.
None is deserialised. Each constructor that takes a text from outside the build is told
the executor's secrets, and **looks at the whole text for a secret before it parses,
cuts or bounds it**: a text that holds one is refused as `NotAllowed::Secret`, whatever
else is wrong with it, and nothing of it is shown
(`ways_out::an_account_or_an_origin_that_holds_a_secret_is_refused_whole_and_not_shown`).
Where a constructor changes the text, it looks again at what it made: an origin's host
is lowered, so the origin as it is written is looked at too, and a secret that the
lowering made is refused the same way
(`ways_out::a_secret_that_lowering_a_host_makes_is_refused_and_not_shown`). No other
constructor changes a text it keeps: an address's rest and an account are kept as they
were given, and a credential's header and value are. A credential's constructor is told
the secrets for its header's name, which is printed; its value is the secret, and is
never printed
(`ways_out::a_credential_is_told_the_secrets_and_its_header_holds_none`).
A refusal is a `NotAllowed`: a kind and a sentence of fixed words. It never quotes the
text that was refused, secret or not
(`ways_out::a_refusal_of_an_address_shows_nothing_of_the_address`).

### An origin

An `Origin` is a scheme, a host and a port, as squint's `same_origin` compared them.
`Origin::of` reads one from an address:

- the address is at most 2,048 bytes (`ADDRESS_MAX_BYTES`) of printable ASCII, with no
  space and no backslash;
- the scheme is `http` or `https`, in either case of its letters;
- the host is a name of 253 characters at most, of labels of 1 to 63 letters, digits and
  hyphens, none starting or ending with a hyphen, separated by single dots. It is kept
  in lower case, so a name is one host in either case of its letters;
- **a host that is a number is written one way, or refused.** A client reads a host
  whose last label is a number as an address in numbers, in any of many spellings:
  `127.1`, `0x7f.1`, `2130706433` and `017700000001` are all the machine `127.0.0.1`,
  and `00000127.0.0.1` is another machine than it looks. So a host whose last label is
  a number (digits alone, or `0x` and hex digits or none) is taken only when the whole
  host is exactly four decimal parts of 0 to 255, none with a leading zero. Every
  other is refused as no host
  (`ways_out::a_host_that_is_a_number_is_four_plain_parts_or_is_refused`);
- the port is 1 to 65,535, written without a sign or a leading zero; with none written
  it is the scheme's own, 80 or 443, and `https://host` and `https://host:443` are one
  origin;
- a path may follow, and is no part of the origin. The host and the port end at the
  first `/` and nowhere else: a query written straight after the host is refused, in
  an origin and in an address alike
  (`ways_out::a_query_follows_a_path_and_never_the_host`);
- an `@` anywhere, which is how a user and a password are written, is refused; so are a
  query (`?`) and a fragment (`#`).

(`ways_out::an_origin_is_a_scheme_a_host_and_a_port`,
`ways_out::a_recipient_cannot_be_made_from_an_address_with_a_user_a_password_a_query_or_a_fragment`,
`ways_out::an_address_that_is_not_one_plain_origin_is_refused`.)

### A recipient

A `Recipient` is who receives data: **an endpoint's origin, the account used with it,
and the upstreams it may forward to.**

- **The origin.** Two origins that differ in scheme, host or port are two recipients
  (`ways_out::two_origins_that_differ_in_scheme_host_or_port_are_two_recipients`). Two
  endpoints at one origin are one recipient: approval is of a place, not of a path.
- **The account** (`Account`), when there is one: a name of 1 to 128 printable ASCII
  characters with no space, no `"` and no `\` (`ACCOUNT_LIMIT`), as a model's name is
  (ADR 0006). It is compared whole and **shown cut**: its first four characters, and
  `...` when there are more (`Account::shown`). That is what `{:?}` prints and what a
  record is to hold (task B4)
  (`ways_out::an_account_is_a_bounded_name_and_is_shown_cut`).
- **The upstreams:** at most 16 origins (`UPSTREAM_LIMIT`), each named once and none the
  recipient's own. `Recipient::reaches` answers whether an origin is the recipient's or
  one of them, which is the question a hop is checked with (task E3)
  (`ways_out::a_recipient_is_an_origin_an_account_and_the_upstreams_it_may_forward_to`,
  `ways_out::a_recipients_upstreams_are_bounded_other_than_itself_and_named_once`).
- Two recipients are equal when all three are. The same endpoint under another account
  is another recipient. **The upstreams are a set:** they are kept in the order of
  their text, whatever order they were given in, so a recipient a config lists one
  way and a backend declares another is one recipient
  (`ways_out::a_recipient_is_the_same_recipient_in_whatever_order_its_upstreams_were_given`).
- What `{:?}` prints of a recipient is its origin and its cut account, and nothing else:
  not its upstreams
  (`ways_out::what_is_printed_of_a_recipient_is_its_origin_and_a_cut_account_and_nothing_else`).

The credential is not part of a recipient. SPEC names "the credential or account used
with it"; a recipient is compared, printed and recorded, and a credential is none of
those. The account stands for it.

### An address, and a credential

An `Address` is where one request goes: an origin, and the rest of the address, its
path and its query, kept as it was given. A hop's address may have a query, as a signed
upload address does; nothing has an `@` or a fragment. What `{:?}` prints is the origin
and how long the rest is, never the rest
(`ways_out::an_address_is_an_origin_and_the_rest_and_shows_only_its_origin`).

A `Credential` is a header's name, its value, and **the one origin it is bound to**. It
prints its header's name and its origin, never its value. Each of the three requests
below takes a credential only when the credential's origin is the origin of the
request's address (`NotAllowed::Elsewhere` otherwise): so a request to another origin
cannot be built with it, which is how "credentials are never forwarded to another
origin" is held by the types
(`ways_out::a_credential_is_bound_to_one_origin_and_is_never_printed`,
`ways_out::what_is_sent_is_an_address_bounded_pieces_and_a_credential_of_its_own_origin`,
`ways_out::a_credential_of_another_scheme_on_the_same_host_and_port_is_refused_by_each_request`).

### The four classes of data

`DataClass` is one of `source_images`, `derived_images`, `prompts` and `evidence`
(`ways_out::there_are_four_classes_of_data`). `Classes` is a set of them
(`ways_out::a_set_of_classes_holds_each_class_or_not`).

**Who states the class of each part of a request: whoever puts the part in.** What is
sent is a list of pieces (`Piece`), and each piece is made with its class
(`Piece::of`) or with none (`Piece::unclassed`). A backend's adapter builds the pieces
from the capability's request, so it is the adapter that states them, by this rule:

| What the piece holds | Class |
|---|---|
| The pixels of an image a call was given as an input, as they are | `source_images` |
| Any other pixels: a crop, a composite, a rendering, a support plane or a mask | `derived_images` |
| Words a caller or an action wrote: a fill's prompt, a segment request's phrases, a conversation's texts, its system prompt, its tool definitions and its tool calls | `prompts` |
| What a tool returned to a judge: a tool result's text and pictures | `evidence` |

**A part with no stated class needs every class approved.** `Outgoing::needs` is the set
of classes a recipient must be approved for before it is handed the request: the classes
of its pieces, and all four when any piece states none. `Classes::needed_for` is the
same rule for whoever holds the classes of a request's parts without their bytes
(`ways_out::a_part_with_no_stated_class_needs_every_class_approved`).

### Three interfaces

Each is one request to one address, answered once. **None follows a redirect:** an
answer that points elsewhere is handed back as the address it points to, so that
whoever drives the hops can check each before anything is handed to it (tasks I5 and
E3). An upload address and a returned download address are addresses in an answer, and
reaching either is another request through the same interface.

| Interface | Its request holds | It answers |
|---|---|---|
| `Sender::send`, for data | `Outgoing`: an address, up to 64 pieces (`PIECES_LIMIT`), and a credential bound to the address's origin | `Reply`: a status and a body of at most 16 MiB (`REPLY_MAX_BYTES`), or the address it was moved to |
| `Downloader::download`, for a model's file | `DownloadRequest`: an address, the most bytes wanted, and a credential bound to the address's origin | `Downloaded`: how many bytes it handed over, or the address it was moved to |
| `Prober::probe`, for reaching a backend | `ProbeRequest`: an address, and a credential bound to the address's origin | `Probed`: the status it reached, or the address it was moved to |

- **A download has no place for user data.** `DownloadRequest` has three fields: the
  address, the most bytes, the credential. It has none for a body, a picture or a
  prompt, so a download cannot be built that carries one. A test inside the crate takes
  the type apart field by field, and stops compiling when one is added; another prints
  it (`egress::ways::tests::a_download_and_a_probe_have_these_fields_and_no_other`,
  `ways_out::a_download_asks_for_an_address_and_a_most_and_a_credential_of_its_own_origin`).
  No test that fails to compile is used.
- **A probe has none either:** an address and a credential
  (`ways_out::a_probe_asks_for_an_address_and_a_credential_of_its_own_origin_and_nothing_else`).
  It is approved apart from sending: whether a recipient may be probed is a question of
  its own to the policy (task I5), not a class of data.
- **A download's bytes go to a sink,** piece by piece, and the sink says whether to go
  on: the installer stops at the size its catalogue states and need not hold a file in
  memory. **A sink's "no" means this:** the piece it answered no to was handed over,
  so the sink has seen it and it is counted in `Downloaded::Bytes`; nothing is handed
  over after it; and the download ends there as one that was answered, not as a
  fault. The count a caller is given back is the count of what its sink was handed. A download and a probe are bounded by a cancellation signal; they are not
  part of a run. A send is part of a backend's request and is handed that request's
  `Bounds` (ADR 0006)
  (`ways_out::a_download_hands_its_bytes_to_a_sink_that_can_stop_it_and_a_probe_says_what_it_reached`,
  `ways_out::a_backend_sends_through_the_interface_under_the_bounds_of_its_request`,
  `ways_out::a_reply_is_a_status_and_a_bounded_body_or_another_address`).
- What `{:?}` prints of a request is its address's origin, each piece's class and size,
  and the credential's header and origin: never a byte of a piece, of an address's
  rest, or of a credential.

The core holds no implementation. The test kit's fakes are tasks B3's and M2's.

### Where a backend says who receives its requests

In its declaration: `Declaration::sending_to(recipient)`, read by
`Declaration::recipient`. **A backend that names no recipient is local**
(`Declaration::is_local`): nothing it is asked leaves the machine
(`ways_out::a_declaration_with_no_recipient_reads_as_local`).

### Which declarations go together

| Recipient | Price | What `Roster` answers as its bound (`Backends::bound`) |
|---|---|---|
| None | None | Nothing: local and free |
| None | Stated | The price's quote, as ADR 0010 has it |
| Named | None | **The amount that is no bound.** A request that leaves the machine with no stated price is a paid request with no bound, and is refused as any such request is (ADR 0007, `Refusal::NoBound`) |
| Named | Free (`Price::free`) | Nothing. Free for a backend with a recipient is said, never assumed |
| Named | Any other | The price's quote |

(`ways_out::a_backend_with_no_recipient_and_no_price_is_local_and_free`,
`ways_out::a_backend_that_names_a_recipient_and_states_no_price_has_no_bound`,
`ways_out::a_remote_backend_with_no_price_beside_a_local_one_of_the_same_model_has_no_bound`.)

This changes ADR 0010, which answered nothing for every backend with no price: that
stands for a local backend alone.

### The fakes

Added by task B3. `whizbang-testkit` gives the fakes of ADR 0006 what a real backend
says of cost (ADR 0010) and of where its requests go, and adds a fake of the sending
interface. None of them spends or sends anything. The tests are in
`crates/whizbang-testkit/tests/it/`, named below as `priced::…` and `sending::…`.

- **Each fake of a capability** (`FakeFiller`, `FakeSegmenter`, `FakeJudge`) can be
  built with a price (`priced`) and with a recipient (`sending_to`), which it declares
  and does nothing else with: the roster a build makes of the fakes answers
  `Backends::bound` from them
  (`priced::a_fake_built_with_each_kind_of_price_quotes_it_for_a_request`,
  `priced::a_fake_with_a_recipient_names_it_and_one_without_is_local`). Both stay when
  a fake is told what else to declare
  (`priced::a_price_and_a_recipient_stay_when_a_fake_is_told_what_else_to_declare`).
- **It reports a charge with every answer** when told one (`charging`), in whole
  millionths; a test chooses a charge under the bound, over it, or none
  (`priced::a_fake_reports_the_charge_it_was_told_to_under_its_bound_over_it_or_none`,
  `priced::a_fake_reports_a_charge_past_what_a_float_holds_exactly`).
- **It fails every request at a named stage** when told to (`failing_at`): the fault is
  `unavailable` and states that stage, and told to fail as answered it carries the
  charge the fake reports. A request is counted once, whether it is answered or fails;
  a judge that fails keeps the request and plays no entry of its script
  (`priced::a_fake_told_to_fail_at_each_stage_gives_a_fault_that_says_that_stage`,
  `priced::a_fake_counts_a_request_once_whether_it_answers_or_fails`).
- **The fakes state the stage of the faults they make themselves:** a request cancelled
  while it was held open at a barrier, and a judge past the end of its script, are
  unresolved, said and not left to be read. What a fake does at a barrier is as it was
  (`priced::a_priced_fake_stopped_at_its_barrier_holds_its_request_open_and_sees_the_cancellation`,
  `priced::a_judge_past_the_end_of_its_script_says_that_nothing_settles_the_request`).
- **`FakeSender`** (`sending.rs`) plays scripted hops: each request it is handed
  through `Sender::send` is answered with the next entry of its script, a reply or a
  fault, and a reply may be a redirect or an answer that names an upload address or a
  download address. It does not look at what it is handed and follows nothing itself.
  It keeps every request, in order, with the address it was for, its pieces and the
  headers it would have carried, which are the credential's and no other
  (`sending::the_sending_fake_records_each_hop_in_order_with_the_headers_it_would_have_carried`,
  `sending::the_sending_fake_plays_a_fault_where_its_script_has_one_and_keeps_the_hop`).
  Past the end of its script it answers `malformed`, unresolved, and does not panic;
  a request the handle refused never reaches it; and what is printed of it holds no
  credential
  (`sending::the_sending_fake_past_the_end_of_its_script_fails_and_does_not_panic`,
  `sending::the_sending_fake_sends_nothing_for_a_run_that_was_cancelled`,
  `sending::what_is_printed_of_the_sending_fake_and_of_a_hop_holds_no_credential`).

Limits: a fake's `Hop::headers` gives a credential's value to the test that asks, which
is what lets a test show that a credential went to one origin and no other; the test
kit is a development dependency and nothing that ships holds it. The fakes of a
download and of a probe are not here: a download's is task M2's, and a probe's comes
with the task that first probes (E3).

### Limits, each pinned or stated

- **Nothing is enforced.** A backend that names no recipient and sends all the same,
  and a backend handed no `Sender` that opens a socket of its own, are not stopped by
  anything here. The gate is task E2's; adapters are trusted code (`SPEC.md` section
  10). No test pins it: there is nothing to run.
- **A class is the adapter's word,** as a stage is (ADR 0010). A piece called a prompt
  that holds pixels is sent as a prompt. A piece with no class errs toward refusal.
- **An IPv6 address in brackets, and a host with letters outside ASCII, are refused.**
  An address is one of them only as a name in ASCII
  (`ways_out::an_address_that_is_not_one_plain_origin_is_refused`, which holds both).
  Either comes with the first recipient that needs it.
- **An `@` is refused anywhere in an address,** in a path or a query too, where it would
  be harmless: one rule that cannot be parsed around.
- **A secret is found only when the constructor was told of it.** A key in a path that
  the executor never heard of is kept in an address's rest, and is not printed.
- **The rest of an address is not normalised.** `/a/../b` and `/b` are two addresses of
  one origin. Only the origin is ever compared
  (`ways_out::known_the_rest_of_an_address_is_not_normalised`).

### For later tasks

What this record leaves open on purpose, for the tasks that build on it. None of it is
code here.

- **Nothing at a handle knows a source image from a derived one.** A `Picture` has no
  class, and the rule above puts the statement with whoever builds a piece. Tasks I3
  and E2 decide who that is for a capability's request, before the test that a
  recipient approved for prompts and not for source images is refused a source image.
- **`Credential::expose` is public.** The types stop a credential being forwarded by
  accident, not by intent: code that holds one can read its value and put it anywhere.
  Task E3 keeps credentials inside the checked sender, and hands a backend none.
- **A real client makes a redirect's `Address` itself,** from what a server sent, and
  needs the executor's secrets to make it.
- **`Sender::send` is given no recipient.** It is one request to one address. Task I5
  builds a sender for each backend, which knows that backend's recipient and checks
  each address against it.
- **`Account::shown` shows four characters,** which may be the first four of a secret
  when an account is one. An account that holds a secret the constructor was told of is
  refused whole; one it was not told of is shown cut like any other.

## Consequences

- The gate, the record and the installer meet at these types: a policy is asked about a
  `Recipient` and `Classes`; a record holds an origin, a cut account and classes; an
  installer is handed a `Downloader`.
- A remote backend with no price cannot run under a ledger until its config states one,
  or says it is free.
- `Declaration` compares and prints its recipient.
- A real network client, when one is added (1C at the earliest), implements three small
  traits and never decides where it may go.
- Reversing this costs every backend that names a recipient and every task of the
  egress strand. Nothing but tests uses it yet.

## Alternatives

- **A URL library for an origin.** Turned down for now: the build has none, a library
  accepts far more than an origin needs (other schemes, user info, international
  hosts), and the rule here is short enough to state whole and test whole.
- **The credential as part of a recipient.** Turned down: a recipient is printed and
  recorded.
- **One interface for all three,** with the kind of traffic as a field. Turned down by
  `SPEC.md` section 10: downloads and probes are separate traffic, and a request type
  with a body cannot promise that a download carries none.
- **Interfaces that follow redirects themselves.** Turned down: the check of each hop
  would be inside every implementation, where a test of the gate cannot see it.
- **A class on a `Picture`.** Turned down: whether pixels are a source or derived is
  known where a request is built, not where pixels are held, and one picture is a
  source in one call and derived in the next.
- **A piece with no class treated as needing none.** Turned down: default deny.
- **Show a refused address escaped and cut.** Turned down: a refusal that quotes
  nothing cannot leak what the constructor was not told is a secret.
