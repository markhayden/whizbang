# 0012. The ways out: a recipient, four classes of data, and three interfaces

Status: accepted. Changes ADR 0006 ("Declarations and features": a declaration may name
who receives its backend's requests) and ADR 0010 (what a roster answers for a backend
that states no price)
Date: 2026-11-13
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
  in lower case, so a host is one host however it was typed. A dotted IPv4 address is
  such a name;
- the port is 1 to 65,535, written without a sign or a leading zero; with none written
  it is the scheme's own, 80 or 443, and `https://host` and `https://host:443` are one
  origin;
- a path may follow, and is no part of the origin;
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
  is another recipient.
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
`ways_out::what_is_sent_is_an_address_bounded_pieces_and_a_credential_of_its_own_origin`).

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
  memory. A download and a probe are bounded by a cancellation signal; they are not
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
  one origin. Only the origin is ever compared.

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
