# 0019. The gate: a config's approvals as the executor's policy, and the check behind a send

Status: accepted. Names ADR 0011 (an `Approval`) and ADR 0012 (a recipient). Changes
ADR 0015 ("A policy before a send, and a checked send": a check is shown the pieces a
request holds, and a third count, of the moves of one request, which a request's
bounds carry)
Date: 2026-11-14
Amended: 2026-11-14, with task E6. Added: "Nothing but pixels", and its rows in the
table of what was read.
Amended: 2026-11-14, with task E3, its first pull request. Added: "Every hop". Changed:
the check's rule for an answer that points elsewhere, which was that nothing is
followed, and the two tests that held it.
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
| `same_origin` (`crates/squint-providers/src/http.rs`): a redirect was followed only to the scheme, host and port of the first address | Rewritten by ADR 0012 (`Origin`) and ADR 0015 (`Recipient::reaches`): to the recipient's own origin, or an upstream its approval names. Nothing follows a redirect by itself: it comes back to the backend as an address |
| `MAX_REDIRECTS`, 5 for one request | Kept as `MOVES_LIMIT`, 5 for one request of a backend |
| `scan`, which searched a recorded request for a marker and walked JSON so that image bytes were left out | Rewritten as the check's look at each part of a request before it is handed over: a part that is the encoder's file is pixels and is not read as words, and every other part is |
| `FORBIDDEN_MARKERS`: how a path starts (a person's folders, the temporary folders, a private folder, `C:` and a backslash, an address of a file) | Rewritten: the same starts, and a drive of any one letter |
| `FORBIDDEN_MARKERS`: `fixtures/` | Dropped: squint's own folder of recordings |
| `FORBIDDEN_MARKERS`: the names of metadata blocks (`Exif`, `eXIf`, the XMP and panorama marks) | Dropped as marks to search for. An image leaves as the encoder writes its pixels, which has no block to hold them, and an image file in any other form is refused whole |

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
- **The probe and the download are not checked here.** They are this task's second
  pull request.

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
   nothing is refused. What is printed of a check is how many it keeps out
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
- **A file name alone is not found,** nor a relative path, nor an absolute path under
  a folder that is not in the list and was not given to `keeping_out`. That no file
  name leaves is held by what a picture is, not by a search.
- **A folder is found only as it was written to the check.** Another spelling of the
  same folder (through a link, with another case) is not. The list of path starts is
  not applied to an address, whose path is the recipient's: only the folders kept out
  are.
- **Another encoder's file that is the same bytes passes.** For pixels that do not
  compress, an encoder that stores them writes the very file this one does; it holds
  nothing but those pixels, and is sent. The test uses a picture of one colour, which
  the other encoder compresses.
- **An image inside other bytes is not seen:** an image in base64 inside words, or in
  a container the list does not name. Adapters are trusted code (`SPEC.md` section
  10); the type they are given for an image is `Piece::picture`.
- **The file is not compressed,** so it is a little larger than four bytes a pixel.
  Nothing real is sent in 1B. A compressing encoder before a real send is on
  `docs/carry-over.md`; it changes one function and the reader beside it.
- **A picture whose rows do not fit one chunk has no file** (`NotAllowed::TooLong`).
  No picture within the limit of a capability's picture (50,000,000 pixels) is one, so
  no test reaches the refusal through a picture; the sizes are tested without pixels
  (`egress::pixels::tests::a_size_of_no_pixels_and_a_size_past_one_chunk_have_no_file`,
  `egress::pixels::tests::pixels_that_are_not_of_the_size_have_no_file`).

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
- **Tasks I6 and W1.** The check an executor is given is
  `SendCheck::new().keeping_out(root)`, with the root of its store.
- **A real sender (1C).** It puts the pieces of a request into a wire format itself.
  A picture piece is already a whole PNG file.

## Consequences

- An executor built with `Ways::through(sender).checked_by(SendCheck).under(policy)`
  sends to a recipient the config approves for what the request holds, at its own
  origin and its upstreams, through five answers that point elsewhere at most,
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
