# 0019. The gate: a config's approvals as the executor's policy, and the check behind a send

Status: accepted. Names ADR 0011 (an `Approval`), ADR 0012 (a recipient) and ADR 0015
("A policy before a send, and a checked send"), and changes none of them but for two
accessors on ADR 0015's `Hop`: `Hop::pieces`, the pieces a request holds, and
`Hop::request_moves`, the moves of one request, which are counted in the bounds each
request is handed; and for one question more of ADR 0015's policy,
`DispatchPolicy::why_not`, which the checked sender asks where it asked `may_receive`
Date: 2026-11-14
Amended: 2026-11-14, with task E6. Added: "Nothing but pixels", and its rows in the
table of what was read.
Amended: 2026-11-14, with task E3, its first pull request. Added: "Every hop". Changed:
the check's rule for an answer that points elsewhere, which was that nothing is
followed, and the two tests that held it; and, after the review of task E6, the limits
of "Nothing but pixels", a folder kept out with a slash at its end, and the most bytes
of the encoder's one chunk.
Amended: 2026-11-14, with task E3, its second pull request. Added: "A download and a
probe". Changed, after the review of the first: one more limit of "Every hop" (a
backend that sends from several threads at once), and a folder that is nothing but
slashes is refused where it was kept. ADR 0015 is amended with it (what a check is
shown, and which count is one request's).
Amended: 2026-11-14, with task E4. Added: "The run's recipients", and why a recipient
is not approved, said in one of three sentences. Changed: the test of a backend that
declares another account, which held the one sentence there was.
Amended: 2026-11-14, after the reviews of tasks E3 and E4. Changed in "A download and a
probe": a download's credential is the first request's at every address of its origin,
where it was the one of the request before and was lost after an address at another
origin; and each wrapper has a test of a signal set between two addresses. In "The
run's recipients": the two sentences that said an origin "is approved", which is not
so of one approved for probing alone. In "Consequences": the wrappers are not in use
until 1C.
Amended: 2026-11-15, with task I3 (ADR 0015, "A wrapper around each request"). Changed
under "Limits": a refused backend is no longer called, the pin that held that it was is
turned round, and what the gate's place knows of a request's classes.
Amended: 2026-11-15, with task I4b (ADR 0015). Changed in "The run's recipients": a
run that a process left after it had sent data is shown recovered, and the limit
that said it was not is restated.
Amended: 2026-11-15, with task W1 (ADR 0015, "The binary wired"). Changed under
"Limits" and "For later tasks": the binary's executor is given its config's policy,
and the check over a sender goes to the task that gives it a sender.
Spec: section 10, "Egress"
Amended: 2026-11-15, at the exit check of checkpoint 1B (task Z1): notes after sentences
that said something was still to come; no decision changed.

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
| `same_origin` (`crates/squint-providers/src/http.rs`): a redirect was followed only to the scheme, host and port of the first address | Rewritten by ADR 0012 (`Origin`) and ADR 0015 (`Recipient::reaches`): to the recipient's own origin, or an upstream its approval names. Nothing follows a redirect by itself: it comes back to the backend as an address |
| `MAX_REDIRECTS`, 5 for one request | Kept as `MOVES_LIMIT`, 5 for one request of a backend |
| `scan`, which searched a recorded request for a marker and walked JSON so that image bytes were left out | Rewritten as the check's look at each part of a request before it is handed over: a part that is the encoder's file is pixels and is not read as words, and every other part is |
| `FORBIDDEN_MARKERS`: how a path starts (a person's folders, the temporary folders, a private folder, `C:` and a backslash, an address of a file) | Rewritten: the same starts, and a drive of any one letter |
| `FORBIDDEN_MARKERS`: `fixtures/` | Dropped: squint's own folder of recordings |
| `FORBIDDEN_MARKERS`: the names of metadata blocks (`Exif`, `eXIf`, the XMP and panorama marks) | Dropped as marks to search for. An image leaves as the encoder writes its pixels, which has no block to hold them, and an image file in any other form is refused whole |

## Decision

The code is `crates/whizbang-core/src/egress/approvals.rs`, `hops.rs`, `pixels.rs` and
`chains.rs`, each named in its section. The tests of the first two sections are
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
  A recipient is compared as a recipient and not as a text: the same origin with its
  host in capitals or with the scheme's own port written, and the same upstreams in
  another order, is the same recipient, refused as named twice and approved as the
  one a backend declares
  (`gate::a_recipient_written_another_way_is_the_same_recipient_named_twice_and_the_one_approved`).

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

### The check behind a send

`SendCheck` is the `HopCheck` an executor that sends is given. The executor has asked
its four questions before it (ADR 0015): the backend declared a recipient, the request
brings no credential, the address is at the recipient's own origin or one of its
upstreams, and the policy approves the recipient for every class of data the request
holds. The check is asked last and can only refuse
(`gate::under_the_strands_check_a_request_is_still_held_to_what_its_backend_declared_and_reaches`).

**It approves an address while its request has been pointed elsewhere five times at
most** ("Every hop", below), in fixed words when it does not. An upstream is approved
as a first address: an upload address comes inside an answer, and the request to it is
one no answer pointed to
(`hops::an_upload_address_and_a_download_address_outside_the_recipients_are_each_refused`).

It holds nothing that changes and counts nothing. A check is still asked for a sender
that was kept past its run (ADR 0015), and being asked changes nothing. What `{:?}`
prints of it is its name and how many folders it keeps out, never one of them
(`gate::what_is_printed_of_the_check_is_its_name_and_how_many_folders_it_keeps_out`).

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

### Every hop

Task E3, its first pull request. The code is the limit in `hops.rs`; one count in a
request's bounds (`capability/mod.rs`: `Bounds::of_one_request`, and the line in each
of the three handles that hands a backend its bounds); and `Hop::request_moves` in
`checked.rs`. The tests are in `crates/whizbang-core/tests/it/egress/hops.rs`, named
below as `hops::…`, against the test kit's sending fake.

`SPEC.md` section 10: "Every hop is checked: redirects, upload services and returned
download addresses. Credentials are never forwarded to another origin."

**Nothing is followed by the sender.** An answer that points elsewhere comes back to
the backend as an address (ADR 0012), and the request the backend then makes of it is
a new request to the checked sender, asked everything again from the first question
(ADR 0015). An upload address and a returned download address are addresses inside an
answer, and reaching either is the same. So:

- **A redirect to an origin that is neither the recipient's own nor one of its
  upstreams is refused, and nothing is sent to it**
  (`hops::a_redirect_to_an_origin_that_is_not_the_recipients_or_an_upstream_is_refused_and_nothing_is_sent_to_it`).
- **A redirect to an approved upstream is followed without the credential.** The
  executor attaches the credential it holds for the origin of each address, or none: an
  upstream is handed the recipient's own never, and its own only when the executor was
  given one bound to it, which in turn never goes to the recipient's own origin
  (`hops::a_redirect_to_an_approved_upstream_is_followed_without_the_credential`,
  `hops::a_credential_bound_to_an_upstream_goes_to_that_upstream_and_never_to_the_recipients_own_origin`).
- **An upload address and a download address outside the recipient's are each
  refused,** and ones inside it are sent
  (`hops::an_upload_address_and_a_download_address_outside_the_recipients_are_each_refused`).
- **A chain longer than the limit is refused.** One request of a backend is pointed
  elsewhere 5 times at most (`MOVES_LIMIT`): the number squint followed
  (`MAX_REDIRECTS`), counted as squint counted it, for one request. The address a
  sixth answer names is handed nothing, and every later address of that request is
  refused too (`hops::a_chain_longer_than_the_limit_is_refused`).

**The count is one request's** (`Hop::request_moves`). A handle hands its backend, with
each request, bounds that carry a count of their own, which starts at nothing; the
checked sender adds one to the count of the bounds a send came with when its answer
points elsewhere, and shows the check that count. It never goes back while the request
lasts, an answered request between two hops included. It is not shared with another
request through the same handle, at once or one after another; and a clone of a
request's bounds holds the same count, so a backend does not start again by cloning
(`hops::the_moves_of_one_request_are_its_own_and_a_clone_of_its_bounds_holds_the_same_count`,
`hops::two_requests_through_one_handle_at_once_each_count_their_own_moves`,
`hops::each_request_of_each_capability_is_handed_a_count_of_its_own`). A run that is
pointed elsewhere once for each of thirty requests is not refused
(`hops::a_long_run_that_is_pointed_elsewhere_once_for_each_request_is_not_refused`).
`Hop::moves` and `Hop::total_moves` are as ADR 0015 has them, and this check uses
neither.

Limits of it, each stated:

- **The limit is on what a server does, not on what a backend does.** The count is of
  the bounds a send comes with. A backend that sends with the bounds of another of its
  requests is counted there, and one that makes a new request starts at nothing:
  adapters are trusted code, and a backend can ask as often as its run's bound on
  requests lets it (ADR 0009).
- **A real client makes a redirect's `Address` itself** from what a server sent, with
  the executor's secrets (ADR 0012), and writes the `/` before a query: an address
  with a query straight after its host is refused when it is made.
- **A request with no answer that points elsewhere has no limit on its sends** but the
  run's own: an upload in many parts is many answered requests.
- **A request whose backend sends from several threads at once may be followed a few
  times more.** The count is read before a send and added to after its answer, so
  sends that are in the air together each see the count as it was: at most one more
  for each such thread, after which every address of that request is refused.
- **The probe and the download are not checked here,** but each by a wrapper of its
  own ("A download and a probe").

### A download and a probe

Task E3, its second pull request. The code is
`crates/whizbang-core/src/egress/chains.rs`: `CheckedDownloader` and `CheckedProber`.
The executor and the handles are not changed, and nothing in the build makes either
yet: there is no network client in 1B. The tests are in
`crates/whizbang-core/tests/it/egress/chains.rs`, named below as `chains::…`, against
the test kit's `FakeDownloader` and a fake that probes, `FakeProber`
(`crates/whizbang-testkit/src/sending.rs`), which is added here and has its own tests
(`crates/whizbang-testkit/tests/it/sending.rs`, `sending::…`).

`SPEC.md` section 10: "Model downloads and probes are separate, approved traffic and
carry no user data."

**Each is its interface around another of its kind.** A `Downloader` and a `Prober` are
one request to one address and follow nothing (ADR 0012). A checked one is handed
another, asks its questions of the first address, and is the one that goes on to an
address an answer points to, after asking the same questions of it. So it never
answers that a request moved, and whoever is handed one in place of a bare client
needs no change. Neither is handed the bounds of a request, so neither can use the
count a send has: each counts the answers that point elsewhere within one call, 5 at
most (`MOVES_LIMIT`, the same number), and starts at nothing for the next call
(`chains::a_download_pointed_elsewhere_more_often_than_the_limit_is_refused_and_each_download_counts_its_own`,
`chains::a_probe_pointed_elsewhere_more_often_than_the_limit_is_refused_and_each_probe_counts_its_own`).

**A refusal is a fault in fixed words, `rejected`, and is noted nowhere.** It quotes
nothing of an address. Neither is made for a run, so there is no record to note it
in: a download is the installer's, which answers with why a file was refused, and a
probe is a command's. A refusal before anything was asked of any address states that
nothing was sent; one that follows an answer that pointed elsewhere states no stage,
since the address that answered was asked. A signal that is set is looked at before
each address, and a fault of the client behind comes back as it is
(`chains::a_cancelled_download_asks_nothing_and_a_fault_of_the_downloader_is_handed_on`,
`chains::a_probe_that_brings_a_credential_is_refused_and_a_cancelled_one_asks_nothing`).
One that is set while the first address answers that it moved ends the call as
`cancelled`, with no stage, and nothing is asked of the second address
(`chains::a_download_cancelled_between_two_addresses_asks_nothing_of_the_second`,
`chains::a_probe_cancelled_between_two_addresses_asks_nothing_of_the_second`).

**A download is held to a list of origins it is handed:** the origins a catalogue's
entry names for its files. `CheckedDownloader::new(downloader, origins)` takes 16 at
most (`DOWNLOAD_ORIGINS_LIMIT`; an entry names 8 at most), and one handed none reaches
none (`chains::a_checked_download_holds_a_bounded_number_of_origins_and_prints_how_many`).

- **A download that is redirected off those origins is refused,** by host, by scheme
  and by port, and nothing is asked of the address it was pointed to. The request
  that was made carried the address and nothing else: no header, and a
  `DownloadRequest` has no place for a body
  (`chains::a_download_redirected_off_its_catalogues_origins_is_refused_and_its_request_carried_nothing_but_the_address`).
- **A first address off them is refused too,** and nothing is asked
  (`chains::a_download_whose_first_address_is_off_its_origins_asks_nothing`).
- **A download moved to one of them is followed,** asking for as many bytes as the
  first request, and the sink is handed the file; a sink's no ends it as it ends a
  bare download
  (`chains::a_download_moved_to_one_of_its_origins_is_followed_and_handed_whole`).
- **A credential the request brings goes to the origin it is bound to and no other:**
  to a second address at that origin, and not to another origin of the list
  (`chains::a_downloads_credential_goes_to_the_origin_it_is_bound_to_and_no_other`).
  It is the first request's credential that is looked at for each address, not that
  of the request before: a download moved from its origin to another and back
  carries it at the first address, not at the second, and again at the third
  (`chains::a_downloads_credential_is_carried_again_at_its_origin_after_an_address_at_another`).
  The checked downloader holds no credential itself.
- **Bytes and then another address are not followed,** even to an origin of the list:
  the fault is `malformed` (`chains::bytes_and_then_another_address_are_not_followed`).

**How it fits `Entry::may_download_from`** (ADR 0017). The installer asks that
question itself of each address a download is moved to, and counts five moves for one
file; that is what holds a download today, against the fake, and it stays. The
checked downloader is what the real client of 1C is wrapped in before the installer
is handed it, made with `Entry::origins`, which is the list `may_download_from`
answers from: the two ask the same question of the same list. Behind a checked
downloader the installer is never told that a file moved, so its own check has
nothing to refuse, and a download off the entry's origins comes to it as a fault,
`rejected`, which it keeps as a failed download (`FileRefused::Download`) and not as
`Elsewhere` or `TooManyMoves`. The second hold is deliberate: the installer's stops
an installer that is handed a bare client, and the wrapper stops a client from being
asked at all. No test joins an `Entry` to a checked downloader yet, since nothing
makes one: `docs/carry-over.md`, the row on the real network client.

**A probe is for one recipient, under a policy.** `CheckedProber::new(prober, policy,
recipient)`, and `with_credential` for each credential it holds, which is refused for
an origin the recipient does not reach and for a second one of the same origin
(`chains::a_checked_probe_holds_one_credential_for_each_origin_it_reaches_and_prints_none`).
It asks, in this order:

1. **The policy approves probing the recipient** (`DispatchPolicy::may_probe`), which
   is apart from any class of data. So a probe is refused by default: under the
   policy an executor has until it is given one, and under a config that approves the
   recipient for all four classes and says nothing of probing. Approved for probing
   and for no class, it is sent
   (`chains::a_probe_is_refused_by_default_and_sent_when_approved_and_carries_no_user_data`).
   The recipient is the approval's only when its origin, its account and its
   upstreams are the same
   (`chains::a_probe_is_approved_for_the_recipient_as_it_was_approved_and_for_no_other`).
2. **The request brings no credential.** One that does is refused, as a send is.
3. **The address is at the recipient's own origin or one of its upstreams**
   (`Recipient::reaches`): the first address, and every address an answer points to.
   A redirect off the recipient, by host, by scheme or by port, is refused and
   nothing is asked of it
   (`chains::a_probes_redirect_off_the_recipient_is_refused_and_nothing_is_asked_of_it`).

Then the credential held for the origin of that address is attached, or none: an
upstream is handed the recipient's own never
(`chains::a_probes_redirect_within_the_recipient_is_followed_with_the_credential_of_each_origin_or_none`).
**A probe carries no user data** because a `ProbeRequest` has no place for any (ADR
0012): what the fake was handed is the address and the credential of its origin, and
the test holds the whole request to that.

The fake that probes plays a script as the sending fake does, keeps each request with
its address and the headers it would have carried, follows nothing, and prints no
credential
(`sending::the_probing_fake_plays_its_script_and_keeps_each_request_with_its_address_and_headers`,
`sending::the_probing_fake_past_the_end_of_its_script_fails_and_does_not_panic`,
`sending::what_is_printed_of_the_probing_fake_and_of_a_probe_holds_no_credential`).

Limits of it, each stated:

- **The rest of an address is not looked at.** A probe's and a download's address is
  made by trusted code from a config or a catalogue; what its path or its query holds
  is not searched for a path or a folder kept out, as a send's is.
- **Nothing gives a checked prober its policy, its recipient or its credentials yet.**
  The command that probes is 1C's (`docs/carry-over.md`), and takes all three from
  the snapshot of the config a call is made under.
- **A probe and a download are in no run's record.**

### The run's recipients

Task E4. The code is one question of the policy (`DispatchPolicy::why_not` and
`NotApproved`, in `policy.rs`) and the lines of `checked.rs` that ask it. Nothing else
is new: a send is noted where it is made, at the checked sender, since ADR 0015
(`RunFacts::sending`, before anything is handed over, and `RunFacts::went` for one
that is refused), and `run/3` has the list (ADR 0013, "Where data went"). The tests
are in `crates/whizbang-core/tests/it/egress/recipients.rs`, named below as
`recipients::…`: each runs an action on an executor with this record's check, a policy
and the test kit's sending fake, and reads the run's record from the store.

`SPEC.md` section 10: "Every run records where data went."

- **A run that sent a prompt and a derived image to one recipient lists it once, with
  both classes.** Three sends to one origin are one entry: the origin, the account
  cut, `derived_images` and `prompts` in the record's order, `sent`; and nothing of an
  address but its origin is in the store
  (`recipients::a_run_that_sent_a_prompt_and_a_derived_image_to_one_recipient_lists_it_once_with_both_classes`).
- **An upstream is listed when it was reached, and only then.** A request that went on
  to an approved upstream lists the recipient's own origin and the upstream, in that
  order; a second upstream that was approved and not reached is nowhere in the record
  (`recipients::a_run_whose_request_went_on_to_an_approved_upstream_lists_it_as_reached_and_one_that_was_not_reached_is_not_listed`).
- **A request that failed at its second hop lists the first, which had the data.**
  When the second hop's fault says that nothing left, the first is listed alone. When
  it states no such thing, the data may have left for the second, and it is listed
  after the first (ADR 0015: only a fault that says nothing left takes a note back)
  (`recipients::a_request_that_failed_at_its_second_hop_lists_the_first_which_had_the_data`).
- **A run whose only request was refused lists the refusal, and nothing as sent:**
  one entry, `refused`, and the refusal among the run's refusals
  (`recipients::a_run_whose_only_request_was_refused_lists_the_refusal_and_nothing_as_sent`).
- **A local run's list is empty,** and nothing of the record is lost
  (`recipients::a_local_runs_list_is_empty`).
- **An account in the list is shown cut, and a credential is nowhere in the record.**
  The credential went with the request, as the fake shows; the store holds neither
  the key, whole or by its start, nor the name of its header, nor the account past
  its first four characters
  (`recipients::an_account_in_the_list_is_shown_cut_and_a_marker_credential_is_nowhere_in_the_record`).

**Why a recipient is not approved is said in one of three sentences.** The executor's
fourth question (ADR 0015) knew only that the policy said no, and said of every such
request that its recipient "is not approved for the classes of data the request
holds", which is false of a backend whose account or upstreams are not the
approval's, and sent a person to the wrong line of a config. `DispatchPolicy::why_not`
answers which part differs, as a kind that holds no value, and nothing when the
recipient may receive, which is exactly when `may_receive` says yes:

| `NotApproved` | When | The sentence |
|---|---|---|
| `Classes` | The policy names the recipient whole, and not for every class the request holds; or no approval names its origin | "the recipient is not approved for the classes of data the request holds" |
| `Account` | An approval names its origin, and none names it with its account | "the recipient's origin is named by an approval, but not with the account the backend declares" |
| `Upstreams` | An approval names its origin and its account, with other upstreams | "the recipient's origin and account are named by an approval, but not with the upstreams the backend declares" |

The two sentences say that an approval names the origin, not that the origin is
approved: the approval may be for probing alone, which approves no data. An origin
approved under several accounts is compared with all of them: one with the
recipient's account makes it the upstreams, in whichever order they were approved. An
account is compared whole, never by what is shown of it. No sentence holds an
account, an origin or a class: the refusal's note names the origin and the classes, as
every refusal of a send does, and the entry in where data went shows the account cut
(`recipients::a_recipient_that_is_not_the_one_approved_is_refused_in_words_that_say_which_part_differs`,
`recipients::the_policy_says_why_a_recipient_may_not_receive_and_nothing_when_it_may`).

**What readers show.** `runs show` prints the list as the record holds it: `egress:`
and how many, then a line for each entry with the origin, the cut account or `none`,
the classes and `sent` or `refused`; `egress: none` for an empty list, and `egress:
unknown` for a record that lost it. That was built with the record (`run/3`), and is
held by the binary's tests against records written by hand
(`runs::runs_show_prints_the_settings_the_paid_requests_and_where_data_went`,
`runs::runs_show_says_unknown_for_every_group_a_record_has_lost_and_never_none`).
Nothing of it is changed here. The first test above holds the bytes of the list as an
executor writes it to the shape those tests read.

Limits of it, each stated:

- **A run that a process left after it had sent data** is recovered as interrupted,
  with `egress` named in `outcome.lost` and an empty list: where its data went was
  noted in the memory of the process that died, and recovery reconstructs nothing.
  The recovery that makes the record is task I4b's (ADR 0015, "The interrupted
  record, and what readers say of it"), and with it the clause is shown, through an
  executor and the sending fake
  (`recipients::a_run_that_a_process_left_after_it_had_sent_data_is_recovered_as_interrupted_with_egress_named_as_lost_and_an_empty_list`).
  So a list that is empty says "nowhere" only where `outcome.lost` does not name it.
- **The list says where data went, to four characters of an account** (ADR 0013): two
  accounts at one origin that start with the same four characters are one entry.
- **An unnamed origin is said as the classes are.** A backend whose origin no
  approval names is told that its recipient is not approved for the classes, which is
  true, and not that the origin is unknown.
- **The sentences tell a backend's declaration from a config's approval apart, and so
  say that an approval of the origin exists.** They go to the run's record and to the
  fault the action is answered with, on the machine that holds the config.

### Nothing but pixels

Task E6. The code is `crates/whizbang-core/src/egress/pixels.rs`, the look at each
part in `hops.rs`, and one accessor in `checked.rs` (`Hop::pieces`, which shows a check
the pieces a request holds; the executor's questions and their order are as they
were). The tests are in `crates/whizbang-core/tests/it/egress/pixels.rs`, named below
as `pixels::…`, and three inside the crate, as `egress::pixels::tests::…`.

`SPEC.md` section 10: "Before sending, image metadata is removed." Nothing is removed
here, because nothing that could hold it is sent.

**An image leaves as a picture, written by one encoder.** `Piece::picture(class,
picture)` makes a piece from a `Picture` (ADR 0006), which is canonical pixels and has
no place for metadata, a file name or a path. The bytes are a PNG of three chunks and
no other: `IHDR`, one `IDAT`, `IEND`; eight bits a sample, red, green, blue and
alpha, not interlaced; every row after the byte that says it is not filtered; the
rows stored, not compressed, in blocks of 65,535 bytes. It is written in the core
with no library, and there is one such file for a picture
(`pixels::the_encoders_output_for_a_picture_holds_only_the_chunks_the_test_lists`,
which also reads the pixels back with a decoder that is not whizbang's;
`egress::pixels::tests::the_checksums_are_the_ones_png_and_zlib_name`).

**An image imported with metadata is sent without it.** A PNG with a colour profile, a
text chunk and an EXIF block, each holding a marker, is imported; an action that is
given it asks a backend, which sends the picture; no marker is in any byte the sending
fake was handed, and the pixels that left are the image's canonical pixels
(`pixels::an_image_imported_with_an_exif_block_a_text_chunk_and_a_colour_profile_is_sent_and_no_marker_is_in_any_byte_the_fake_was_handed`).

**The check looks at every part of a request, at every address,** after its rule for
an answer that pointed elsewhere, and refuses in fixed words that show nothing of what
it found (`pixels::every_part_of_a_request_is_looked_at`):

1. **A part that is, byte for byte, a file the encoder writes is pixels,** and is not
   read as words: pixels spell anything by chance
   (`pixels::a_picture_as_the_encoder_writes_it_is_sent_whatever_its_pixels_spell`).
   The reader takes the size from where the encoder writes it, holds the length that
   size gives against the part's length before it reads on, and copies nothing.
2. **Any other part that starts as an image file does is refused:** a PNG, a JPEG, a
   GIF, a TIFF or a WebP. So a PNG with one chunk more, one byte more or fewer, an
   interlaced header, a filtered row, a block that is not the encoder's or a wrong
   checksum is not sent, in a part of any class
   (`pixels::an_image_file_that_the_encoder_did_not_write_is_refused`).
3. **A part, or the rest of an address, that holds a folder the check was told to keep
   out is refused.** `SendCheck::keeping_out(folder)` is told the root of the
   executor's store, as it is written; at most 8 (`KEPT_OUT_LIMIT`), and a path of
   nothing, or of nothing but slashes, is refused: every address holds a slash. What
   is printed of a check is how many it keeps out
   (`pixels::a_check_keeps_out_a_bounded_number_of_folders_and_prints_none`).
4. **A part that holds an absolute path is refused:** one of `/Users/`, `/home/`,
   `/tmp/`, `/var/folders/`, `/private/` and `file://`, or a drive, which is one
   letter, a colon and a backslash where the letter does not end a longer word or
   number
   (`pixels::a_body_that_holds_an_absolute_path_or_the_stores_root_is_refused_by_the_sending_interface`,
   `pixels::words_that_hold_no_path_are_sent_as_they_are`).

Limits of it, each pinned or stated:

- **It errs toward refusing.** The rule goes by how a path starts. An address in a
  prompt whose path starts as a home folder does, and a one-letter label before an
  escaped line break, are refused as paths
  (`pixels::known_words_that_only_look_like_a_path_are_refused_too`). A piece is the
  words as they are, not as a wire format escapes them.
- **A file name alone is not found,** nor a relative path, nor a path from a home
  written as `~`, nor an absolute path that starts any other way: under a folder that
  is not in the list and was not given to `keeping_out`, on a drive written with a
  forward slash, on a share, in another case, or in an encoded form (percent-encoded,
  JSON's `\/`, UTF-16). That no file name leaves is held by what a picture is, not by
  a search.
- **A folder is found only as it was written to the check.** Another spelling of the
  same folder (through a link, with another case, percent-encoded as an address writes a space, or
  with the slashes escaped as JSON may) is not. The list of path starts is
  not applied to an address, whose path is the recipient's: only the folders kept out
  are.
- **Another encoder's file that is the same bytes passes.** For pixels that do not
  compress, an encoder that stores them writes the very file this one does; it holds
  nothing but those pixels, and is sent. The test uses a picture of one colour, which
  the other encoder compresses.
- **An image inside other bytes, or cut across parts, is not seen:** an image in
  base64 inside words, after any byte in a part, in a container the list does not
  name (a BMP), or cut so that no part starts as an image file does. Adapters are
  trusted code (`SPEC.md` section 10); the type they are given for an image is
  `Piece::picture`.
- **The file is not compressed,** so it is a little larger than four bytes a pixel.
  Nothing real is sent in 1B. A compressing encoder before a real send is on
  `docs/carry-over.md`; it changes one function and the reader beside it.
- **A picture whose rows do not fit one chunk has no file** (`NotAllowed::TooLong`).
  No picture within the limit of a capability's picture (50,000,000 pixels) is one, so
  no test reaches the refusal through a picture; the sizes are tested without pixels
  (`egress::pixels::tests::a_size_of_no_pixels_and_a_size_past_one_chunk_have_no_file`,
  `egress::pixels::tests::pixels_that_are_not_of_the_size_have_no_file`).

### Limits, each pinned or stated

- **A refused backend was still called, and is not now.** The plan refuses a request
  at the gate's place, before its backend is called and before the meter's place sees
  it. That place was task I3's, and until it was built a pin here held that a refused
  backend was called once and could only not send. It is built (ADR 0015, "A wrapper
  around each request"): the backend's own count of requests is none and no other
  backend is asked in its place
  (`gate::a_backend_that_is_refused_at_the_gates_place_is_not_called`), and a fake in
  the meter's place saw nothing
  (`around::with_the_default_policy_a_backend_that_names_a_recipient_is_refused_at_the_gates_place_and_one_that_names_none_is_sent`).
  One thing the plan's task asks is still not shown: that a refused request has no
  dispatch record, which no request has until the meter (task S5;
  `docs/carry-over.md`). Shown since, with the meter:
  `spend::meter::a_request_the_gate_refused_has_no_dispatch_record`.
- **The gate's place knows a prompt and no other class.** What a request holds
  before any piece is made is less than what its backend then hands over: a picture
  has no class at a handle. So a recipient approved for prompts and not for source
  images is refused a source image by the checked sender, after its backend was
  called, as the test of it here has it; the gate's place refuses first what it can
  know (ADR 0015, "What a request is said to hold, before any piece is made").
- **A class is what whoever built the piece stated** (ADR 0012). Nothing here knows a
  source image from a derived one.
- **Nothing here reads a config or gives an executor its policy.** That is tasks I6
  and C5. Task W1 gave the binary's executor the policy of its config, and no sender:
  none is in the build (ADR 0015, "The binary wired").
- **`may_probe` is asked by the checked probe, and nothing in the build makes one
  yet** ("A download and a probe").

### For later tasks

- **Tasks C4 and C5.** A backend's declared recipient is made from the approval
  (`Approval::recipient`), not put together a second time from a backend's address:
  another account or another list of upstreams is another recipient, approved for
  nothing. A refusal of `from_approvals` is a config that gives no snapshot, not a
  policy of what could be taken.
- **Task I3.** The gate's place asks the same policy the same question,
  `may_receive(recipient, classes)`, with the classes a capability's request holds
  before any piece is made; a request it refuses does not reach the checked sender.
- **Task I6, and the task that gives the binary a sender** (checkpoint 1C,
  `docs/carry-over.md`; it was task W1's here, which had no sender to put a check
  over). The check an executor is given is
  `SendCheck::new().keeping_out(root)`, with the root of its store. Give it the root
  as an absolute path, and its canonical spelling too when that differs. A relative
  root such as `.` refuses any part that holds a dot. A root that holds a space or a
  backslash can never be found in an address. Slashes at the end of the root are left
  off, so the folder is found where it is written bare, and a root that is nothing
  but slashes is refused (`NotAllowed::Empty`), not kept
  (`pixels::a_body_that_holds_an_absolute_path_or_the_stores_root_is_refused_by_the_sending_interface`).
  The list of path starts is not applied to an address: a home path in a query is
  sent.
- **A real sender (1C).** It puts the pieces of a request into a wire format itself.
  A picture piece is already a whole PNG file.
- **A real download client and a real prober (1C).** Each is wrapped before anything
  is handed it: `CheckedDownloader::new(client, entry.origins())` for each entry, and
  `CheckedProber::new(client, policy, recipient)` for each recipient probed.

## Consequences

- An executor built with `Ways::through(sender).checked_by(SendCheck).under(policy)`
  sends to a recipient the config approves for what the request holds, at its own
  origin and its upstreams, through five answers that point elsewhere at most,
  and nothing else.
- Once 1C wraps its download client and its prober, neither is trusted: a model's file
  comes only from the origins its entry names, and a probe goes only to a recipient
  the config approves for probing. Until then the installer's own check holds a
  download, and nothing probes.
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
- **A mark on a piece that says it is pixels,** set by `Piece::picture` alone, in
  place of reading the bytes again. Turned down for now: it changes ADR 0012's type,
  and bytes that are read again need no trust in a mark. It costs one pass over each
  picture at each address.
- **The PNG encoder of the `image` library, which the core already holds.** Turned
  down for now: its bytes depend on the versions of its compressor, which the core
  does not pin (`docs/carry-over.md`, the row on one encoder in the core), and a
  reader could not say "exactly what the encoder writes" of them without inflating.
- **A general rule for an absolute path** (a slash after a space or a quote). Turned
  down: it refuses the path of every address and every pointer in a tool's schema.
- **A policy of the approvals that could be taken, with the rest left out.** Turned
  down: a config that is half applied is not the config a person wrote.
- **The check asks the policy and the recipient again.** Turned down: the executor
  asks them first, of every address, so a second asking could never refuse and no test
  could hold it.
- **The check goes by the moves in a row.** Turned down: a request answered meanwhile
  sets them back.
- **The limit on the moves of the whole run** (`Hop::total_moves`), which needs
  nothing of the executor. Turned down: it is a budget for a run, and a judge whose
  every turn is pointed elsewhere once would be refused at its sixth turn.
- **The checked sender follows an answer itself, and counts as it goes.** Turned down
  by ADR 0012 and ADR 0015: a hop that is not a new request is not asked the
  executor's questions where a test can see it.
- **A checked download and a checked probe that follow nothing,** and hand the
  address back as the bare interface does. Turned down: with no bounds to count in,
  a count of moves would have to be kept between calls, for a chain nothing marks
  the start or the end of. The questions are asked of each address inside the one
  call, where the fake behind shows what was asked of whom.
