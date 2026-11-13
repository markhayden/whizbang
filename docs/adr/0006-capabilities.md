# 0006. Capabilities: an interface for each, declared features, and resolution by choice, then default

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, after the review of the first part, before it merged. Changed: what
a fault's cleaning guarantees and what it does not find, with a rule for adapters; a
handle is bound to its run's cancellation and deadline, and a backend is handed them as
`Bounds`; a point is in a pixel; an instance's box holds its plane; what rascal's `alpha`,
`tinted` and `binary` renderings marked, and that a backend cannot rebuild them. Added:
who retries; how requests will be accounted for. And after its second review: a
deadline is only ever narrowed; white space is trimmed before a fault's text is cleaned.
Amended: 2026-11-12, with the second pull request of task A8, which this record said
would add to it. Added: "Judge"; "How a call names its choice"; "The fakes"; the judge's
trait and handle among what every capability shares; two consequences and three
alternatives. Changed: the sentences that said the handles in an action's context were
still empty, and that prepare handed resolution no choice.
Amended: 2026-11-12, with the third and last pull request of task A8. Added: "The
composite"; "Who vouches for a derivation"; three consequences and four alternatives.
Changed: the sentences that left the alpha of a filled picture, and the keeping of
pixels outside the support, to this part. And from the review of the second part:
tool definitions count toward a conversation's bound; a request id is trimmed before it
is cleaned; `Part`, `StopReason` and `Usage` can gain members; what a conversation's
types do not check; what an opaque part's adapter mark is and is not; which controls
are absent; that a need needed only without an argument cannot be chosen by it.
Amended: 2026-11-12, with ADR 0007, from the review of the last part. Corrected: "a
derivation cannot be stated of two unrelated pictures of one size" holds only while the
mask weighs some pixel nothing; a mask whose support is the whole image vouches for any
picture of its size, and a test pins it. Added: the check takes the digest a picture
already holds (`Derivation::checked_of`).
Amended: 2026-11-13, with the third pull request of task A7 (ADR 0008). Changed: a
context makes its own handles from backends, and the executor hands each run its
handles; the fakes can hold a request open and see their bounds, and the fake
segmenter can fail.
Spec: section 3, "Crates and their edges", "Identities" and "Storage"; section 4,
"Backends"

## Context

`SPEC.md` section 4 names three capabilities, segment, fill and judge, and gives each its
own request and response, defined in the core, with no shared execution shape. It says what
every backend shares: an adapter declares the features it supports and an action declares
what it needs; resolution is deterministic, an explicit per-call choice and then the
config default; a backend lacking a required feature is an error; a model is never
substituted silently. And it leaves to this record, at checkpoint 1A: the support plane's
polarity and range, how a mask's core, blend and protect planes become it, and the colour
and alpha rules of the pixels a filler returns.

What existed before this record: `Backends` (ADR 0004), one question that prepare asks,
"which model would answer this capability with these features", with no place for a call's
choice; and three handles in an action's context that were empty and could not be made.

What was read, at the commits `docs/sources.toml` pins:

- Rascal's segmenter adapters (`tools/python/src/rascal_tools/models/adapters.py`). One
  interface for five models: `find` takes the image's pixels, a list of text queries and
  settings, and returns instances, each the query it answers, a score, a box and a plane
  the size of the image. The settings are a threshold, the most instances, a padding, and
  two that only some adapters read. Every adapter takes text alone.
- Rascal's filler port (`crates/rascal-mask/src/filler/mod.rs`) and its renderings
  (`crates/rascal-core/src/rendering.rs`). A filler is sent an image and, by a profile its
  config names, one of four pictures of the mask: `grey`, 8 bits, `round(255 w)` of the
  effective weight `w`, 255 to paint; `binary`, 255 where `w` is at least a threshold
  the config gives; `alpha`, the image made transparent in the unprotected core, and
  nowhere in the blend; `tinted`, the image tinted in the unprotected core alone. It
  answers with a PNG of exactly the dimensions sent, and anything else is an error. A
  call is recorded with how far it got, since that decides whether it may have been
  charged.
- Rascal's paste (`crates/rascal-ops/src/paste.rs`), for what is done with a fill's
  alpha: "a base's alpha is kept; a fill's alpha is ignored and the summary says so".
- Squint's provider interface (`crates/squint-core/src/provider.rs`): one round trip to
  a model. A request is a system prompt, a history of messages, tool definitions, a
  limit on output tokens and the prepared bytes of every image by an id. A message is a
  role and a list of content blocks: text, an image by id, a tool call, a tool result,
  and a block only its adapter reads, kept as a JSON value and echoed. A response is the
  texts and the tool calls as two lists, the content again in the provider's order, a
  stop reason, usage or none, and the provider's request id. Its errors are classes of
  one error type (`crates/squint-core/src/lib.rs`).

No capability runs in checkpoint 1A. Every backend is a fake from the test kit.

**This record was merged in three parts, with task A8's code.** The first decided what
every capability shares, faults, declarations, resolution, fill and segment. The second
added the judge's interface, how a call names its choice, and the fakes. The third added
the composite of a fill, and who vouches for the derivation it states.

## Decision

The code is in `crates/whizbang-core/src/capability/`; the tests that hold it to this
record are in `crates/whizbang-core/tests/it/capability/`.

### What every capability shares

- **A trait for each capability,** in the core: `Segmenter`, `Filler` and `Judge`. Each
  has two methods. `declaration` says which model answers and which features it supports. The
  other takes one request and the run's `Bounds`, and returns the response or a fault.
  It blocks until it has its answer. The core holds no implementation.
- **`Bounds`** is what a backend is told of the run it answers for: the run's
  cancellation signal, and its deadline when it has one. Only a handle makes one. Its
  fields are private and it can gain more, so what a later task needs a backend to
  know of its run is added to it and no trait changes. A backend is expected to stop
  early when the signal is set, and to give up at the deadline with the fault
  `timed_out`. Nothing cuts off a backend that does neither: enforcing a deadline is
  the worker boundary's, at checkpoint 1C (`docs/carry-over.md`).
- **A handle for each,** which is what an action asks through: `SegmentHandle`,
  `FillHandle`, `JudgeHandle`. A handle wraps a backend and **is bound to one run**: it is made with
  that run's cancellation signal, and with its deadline when it has one. **A deadline is
  only ever narrowed:** the executor sets it, and an action, which holds the handle and
  can clone it, may only shorten it; setting a later time than the one already set
  changes nothing. The method an
  action calls takes the request and nothing else, so an action cannot ask on behalf
  of a run that was cancelled by handing over some other signal. An action's context
  makes its own handles, with its own signal and its own deadline, from the backends
  it is given (`Capabilities`, which holds backends and no handle; `Context::new`): so
  a context never holds a handle that is bound to another run's signal (ADR 0008,
  "Cancellation"). A handle made by other code, as a test makes one, is bound to the
  signal it was made with, and tells its deadline by the machine's clock; the handles
  an executor's context makes tell it by the executor's clock (ADR 0008, "A deadline in
  a run"). A handle does four things the backend is not
  trusted to do. Once the run is cancelled it sends nothing, and answers with the
  fault `cancelled`. Once the deadline has passed it sends nothing, and answers
  `timed_out`. It sends a request only to a backend that declares every feature the
  request needs, and otherwise answers `unsupported` without sending. And it holds the
  answer to the request: an answer that does not fit is the fault `malformed`. The
  handles in an action's context (`Handles`, ADR 0004) are these. The executor hands
  each run a handle on every backend it was given (`Engine::with_capabilities`, ADR
  0008), whatever the run's call resolved: choosing among backends by the call's
  choice, and what the run record keeps of each request, come with the first action
  that asks a capability (`docs/carry-over.md`).
- **Nobody retries in checkpoint 1A.** A backend sends one request and returns; a
  handle sends it once. Whether to ask again is for whoever owns the loop the request
  is part of, which for the judge is the judge loop of milestone 2, and it decides from
  a fault's kind and its wait. Squint's retry rule read an HTTP status that a fault
  does not carry (`docs/carry-over.md`).
- **Accounting for requests** (task A7: a parent is terminal only when every request a
  capability was sent is accounted for) will be a decorator over the backend, inside
  the handle: the handle already stands between every request and its backend. It
  needs no change to a trait.
- **Checked types.** Every type a request or a response is made of has private fields
  and is built through a constructor that checks it. None is deserialised. So a value of
  the type is a value that passed the check: there is no support plane out of its range,
  no fill of four channels, no instance with a score that is not a number. What does not
  pass is refused with an `Unfit`, which is a type (empty, too large, wrong length, wrong
  size, out of range, not allowed) and a sentence made of fixed words and numbers. It
  quotes nothing a backend sent.
- **Bounds.** An image, a plane and a fill hold at most 50,000,000 pixels
  (`PIXEL_LIMIT`): ADR 0001's default limit on a decoded image. A constructor is handed
  bytes that are already in memory, so the limit is checked on the stated width and
  height before a byte is looked at, and the count of bytes is then held to the size:
  refusing costs nothing. Whoever reads a response off a wire or out of a process
  bounds what it reads; that is the backend's, at checkpoint 1C. The other bounds are
  stated with each capability.

An image is sent as a `Picture`: the canonical pixels of ADR 0001, four bytes a pixel,
red, green, blue and straight alpha, sRGB, row by row from the top left. It carries its
canonical pixel digest, worked out once. What `{:?}` prints of a picture is its size and
its digest, never its pixels.

Not decided here: what a response reports of the device that answered and of a cache,
which the run record's `models` group needs and which comes with the first request sent
through the executor (`docs/carry-over.md`); how a blocking request is scheduled, which
ADR 0004 left open for `submit` as well; spend; and the wire.

### Faults

A request that fails ends in a `Fault`: a kind, one sentence, and a wait in milliseconds
when the backend said when to ask again.

| Kind | Name | Meaning |
|---|---|---|
| Cancelled | `cancelled` | The run was cancelled. Nothing more is sent |
| Unsupported | `unsupported` | The request needs a feature the backend does not declare. It was not sent |
| Credential | `credential` | The backend refused the credential. Never retried |
| Rejected | `rejected` | The backend refused the request as it is: the same request would be refused again |
| Unavailable | `unavailable` | The backend could not answer now: overloaded, or not reached. The one kind where the same request may be answered later |
| Timed out | `timed_out` | No answer came within the time allowed |
| Malformed | `malformed` | An answer came and cannot be used |
| Too large | `too_large` | The request or the answer is over a bound of the backend or of whizbang |

The kinds are read from squint's error classes (`crates/squint-core/src/lib.rs`:
`credential`, `provider`, `provider_rejected`, `image_limit`, `cancelled`), which told a
caller the same things: whether to ask again, and whose fault it was.

**A fault's sentence is cleaned when the fault is made,** whatever an adapter passed in.
A fault is written into a run record.

- the first 240 characters are taken (`FAULT_LIMIT`), so the cut falls between
  characters;
- each is kept when it is an ASCII letter, a digit, a space, or one of
  `. , ; : ' ( ) % + - _`;
- every other character becomes `?`;
- white space at the ends of the text is dropped before any of that, so a sentence of no
  characters, or of white space alone of any kind, becomes `no reason was given`.

**What that guarantees.** A sentence has no `/` and no `\`; no `@`, `=` or `&`; no line
break, double quote or angle bracket. So it holds no path with its separators, no
address in its URL form, and no second line that could pass for another header or
another record. A fault's `{:?}` prints the kind, the cleaned sentence and the wait, and
nothing else: a fault has no field that holds a request, a body, a header or an address,
and no source error behind it.

**What it does not find.** Cleaning replaces characters; it does not know what a text
means. Each of these goes through unchanged, and a test pins that
(`fault::what_cleaning_does_not_find_is_known`):

- a host name, such as `build-host-7.corp.internal`;
- an IP address and its port, such as `10.0.3.17:8443`;
- a header's name and value on one line, such as `Authorization: Bearer` and a token;
- a token made of letters, digits, `-`, `_` and `.`, which is what most keys are.

**Redaction does not close that.** The executor takes the secrets it was told of out of
every text it writes or reports, an error's message among them (ADR 0005, "Redaction").
It finds a secret by its value, and only one it was told of. A token that a provider
sent back in a body, or that an adapter read from a header, is a value the executor was
never told, and neither cleaning nor redaction finds it.

**So the rule for an adapter is part of the guarantee:** an adapter builds a fault's
sentence from its own words and from typed fields (a status, a count, a size), and never
copies a provider's body, a header, or an address into it. Nothing here can check that
of an adapter that does not exist; the first real adapter is held to it with a test
(`docs/carry-over.md`).

### Declarations and features

A backend's `Declaration` is the model that answers (`ModelIdentity`: backend, model,
version) and a set of feature names. A feature is a name as an action's name is (ADR
0003), declared once; it is the same text a descriptor's need spells.

A request needs a feature when it holds what the feature is for:

| Capability | Feature | A request needs it when |
|---|---|---|
| segment | `phrase` | it holds a request by a phrase |
| segment | `point` | it holds a request by a point |
| segment | `box` | it holds a request by a box |
| fill | `prompt` | it has a prompt |
| judge | `image` | its conversation holds an image |
| judge | `tool` | it defines a tool |

A backend may declare other names; nothing in the core needs them of a request.

A handle checks this for every request, whatever prepare said: an action can build a
request that its descriptor's needs did not announce, and the request is still not sent
to a backend that does not declare what it holds.

### Resolution

A `Roster` holds the backends on offer: for each capability, declarations by name, and at
most one default. A name keeps the rule of an action's name and names one backend of a
capability; the same name under another capability is another entry. A default names a
backend that is offered. The config will fill a roster at checkpoint 1B; until then a
test does.

`Roster::resolve(capability, choice, features)` answers which backend answers a
capability for a call:

1. **The call's choice, when it made one.** The backend offered under that name for that
   capability. A name that is not offered is the error `UnknownChoice`; the default is
   not used in its place.
2. **Otherwise the default** of the capability. A capability with no default is the error
   `NoDefault`, though one backend, or several, be offered: being the only one offered
   does not make a backend the default, or adding a second would change which model
   answers a call that named none.
3. **Then the features.** The backend picked in step 1 or 2 declares every feature the
   call needs, or the error is `MissingFeature`, naming the first feature it lacks in the
   order they were needed. No other backend is looked for.

It reads the roster and its three arguments and nothing else, so it is deterministic, and
there is no path through it on which a backend other than the chosen one, or the default
when none was chosen, answers. That is how "a model is never substituted silently" is
held.

An error says which case it is as a type and as a sentence. A choice is what a caller
typed: it is shown escaped, and its first 64 characters at most, with its length when it
was longer.

`Backends::resolve`, the question prepare asks (ADR 0004), takes the choice as well, and
a roster is the implementation: it answers with the model's identity, or with the error's
sentence as the reason nothing answers.

### How a call names its choice

By an argument, and the descriptor says which. A need (ADR 0003) may name the argument
that chooses its backend: `Requirement::chosen_by`, printed in a description as
`chosen_by`, the argument's name or null. The argument is declared, takes a text, and is
one a call may leave without a value: not required, and with no default. It is a text
and not a choice among names the descriptor lists, because what is on offer is the
machine's state and a descriptor holds none.

Prepare (ADR 0004, step 6) reads the choice of each need the call has: the text the call
gave for that argument, or none when it gave none or the need names no argument. It
hands the choice to `Backends::resolve` with the capability and the features. A
prepared need says the choice it was resolved with (`Need::choice`).

- A call that names a backend nothing offers is prepared and **not ready**, with the
  reason. The default is not used in its place. A test holds this, the inverse of the
  pin it replaced:
  `prepare::a_filler_named_in_a_call_is_prepared_against_that_filler`.
- A call that names none gets the default.
- A need that names no argument is always answered by the default.
- **Naming the default and naming nothing are two preparations of one model.** A call
  that names the default backend and a call that leaves the argument out resolve to the
  same model and have different fingerprints, because their arguments differ. That is
  intended: a fingerprint is of what was asked and what was seen, and task A6's
  admission compares a call with itself.
- Two needs may name one choosing argument: one name then chooses a backend for each.
- A need that holds only when an argument is left out cannot be chosen by that
  argument; such a descriptor is refused.

The fingerprint `prepared/1` keeps its kind and its encoding. The choice is an argument,
which was hashed already, and the model it resolves to is the need's, which was hashed
already; so two calls that differ in their choice, and one call prepared against two
rosters that resolve its choice differently, have different fingerprints, with no new
field.

The description's version goes from 3 to 4: every need gained a field (ADR 0003, "What
changes the version").

### Fill

**The request** is an image, one support plane, and an optional prompt of 1 to 4,096
bytes (`PROMPT_LIMIT`). The image is a `Picture`: it is sent with its alpha, and the
colour under a transparent pixel is sent as stored, since it is part of the picture
(ADR 0001).

**The support plane** (`Support`) is the area to paint:

- **Layout.** One byte a pixel, row by row from the top left, the size of the image.
- **Polarity.** 255 is "paint here". 0 is "keep this pixel: whatever is painted here is
  thrown away".
- **Range.** A byte is 0 or 255. Any other value, 1 to 254, is refused, naming the first
  such byte and where it is.
- **Also refused:** a plane of another count of bytes than its width times its height; a
  width or a height of 0; more than 50,000,000 pixels.

**How a mask's planes become it** (`Support::of`). A pixel's byte is 255 when the mask's
effective weight there is over 0, and 0 otherwise, by ADR 0002's one rule for the weight:
0 where protected, else 1 in the core, else the blend over 255. Pixel by pixel:

| Protect | Core | Blend | Support |
|---|---|---|---|
| not 0 | any | any | 0 |
| 0 | not 0 | any | 255 |
| 0 | 0 | 1 to 255 | 255 |
| 0 | 0 | 0 | 0 |

So the support is everything a composite will read from the fill, the faint edge of the
blend included, and nothing else; and no protected pixel is ever in it.

A request whose support plane is of another size than its image is refused, and so is one
whose support has nothing to paint: there is nothing to ask.

**The pixels returned** (`Painted`) are colour, and nothing else:

- **Layout.** Three bytes a pixel, red, green and blue, eight bits a channel, row by row
  from the top left.
- **Dimensions.** Those of the request's image, exactly. The handle refuses any other as
  `malformed`; nothing is resized.
- **Colour.** sRGB, as canonical pixels are (ADR 0001). A backend whose model works in
  another space converts before it answers.
- **Alpha.** There is none. A filler paints colour. A backend whose model returns an
  alpha channel drops it before it answers; a response of four bytes a pixel is refused
  by its length, and so is one of one byte a pixel. The alpha of the picture that comes
  of a fill is the image's own ("The composite").
- **Also refused:** a width or a height of 0; more than 50,000,000 pixels.

A filler is not trusted to leave pixels alone (`SPEC.md` section 4). Nothing in this
interface asks it to: what it returns outside the support is never read. The composite
holds that.

A prompt is sent only to a filler that declares `prompt`. It is not dropped for one that
does not: the request is refused as `unsupported`.

### The composite

`whizbang_rascal::composite::composite(image, fill, mask)` pastes a fill onto an image
under a mask, and is the one place a filler's pixels enter a picture. It is a function
of its three arguments and nothing else. The tests that hold it to this section are in
`crates/whizbang-rascal/tests/it/composite.rs`.

**What it takes.** The image as a `Picture`; the fill as a `Painted`; a mask. Refused,
each as a type with a sentence:

- a mask that does not belong to the image, by canonical pixel digest (ADR 0002), or
  whose space is not the image's size (`WrongImage`);
- a fill of other dimensions than the image (`FillSize`). Nothing is resized.

It takes no support plane. The support is the mask's (`Support::of`, above), and the
composite reads the three planes it was made from, which the mask's type keeps valid. A
support plane with a value outside its range cannot be made, so none reaches a request
or a composite.

**What each pixel becomes.** The result starts as a copy of the image's bytes. Then for
each pixel, by the mask's planes there:

| Protect | Core | Blend | Red, green and blue | Alpha |
|---|---|---|---|---|
| not 0 | any | any | the image's | the image's |
| 0 | not 0 | any | the fill's | the image's |
| 0 | 0 | `b`, 1 to 255 | the mix below, for each channel | the image's |
| 0 | 0 | 0 | the image's | the image's |

The first and the last row are every pixel whose effective weight is 0: every protected
pixel, and every pixel outside the support. No arithmetic is done on them. Their four
bytes are the image's, so they are exactly the original, whatever the filler painted
there.

**The mix.** For one channel, with `o` the image's byte, `f` the fill's byte and `b` the
blend's byte, all whole numbers from 0 to 255:

```
out = ((255 - b) * o + b * f + 127) div 255
```

where `div` is division of whole numbers that drops the remainder. The sum is at most
65,152, so it fits in 32 bits without wrapping, and `out` is from 0 to 255.

- This is the nearest whole number to `(1 - w) o + w f` with `w = b / 255`, the mask's
  rule for a weight (ADR 0002).
- **There is no tie, so there is no rounding rule to choose.** The exact mix is `N / 255`
  for the whole number `N = (255 - b) o + b f`. A value half way between two whole
  numbers, `k + 1/2`, would need `2 N = 255 (2 k + 1)`: an even number equal to an odd
  one. So the nearest whole number is always one number, and adding 127 before the
  division finds it: a remainder of 128 or more goes up, one of 127 or less goes down.
- A blend of 255 gives the fill, as the core does; a blend of 0 would give the image,
  and such a pixel is not touched at all.
- **Colour space.** The bytes mixed are the stored ones: sRGB values of eight bits, as
  canonical pixels hold them (ADR 0001) and as a fill returns them. They are not turned
  into linear light first.
- **Alpha.** Straight, as canonical pixels are, and not multiplied in: a colour is mixed
  the same way whatever the alpha of its pixel.

It gives, for every `o`, `f` and `b`, the byte rascal's `feather` paste gave
(`crates/rascal-ops/src/paste.rs`: per channel in 32-bit floating point,
`(1 - w) base + w fill`, rounded and clamped); a test goes through all 16,711,680 cases
with a blend of 1 to 255.

**One half.** A blend is a count of 255ths, so no blend is exactly one half. The two
nearest are 128 and 127. Under either, an image's 100 and a fill's 200 give 150; an
image's 0 and a fill's 255 give 128 at 128 and 127 at 127; an image's 10 and a fill's 21
give 16 at 128 and 15 at 127.

**A transparent pixel of the image** is treated as any other pixel, and its alpha is
never changed. In the core, the colour under it becomes the fill's, and it stays as
transparent as it was; under a blend, the colour under it is mixed as stored; outside the
support it is untouched, the hidden colour included. A fill has no alpha to give, and a
composite never makes a pixel more or less transparent: an image with a transparent
background keeps its outline through a fill.

**What it returns.** The picture, as canonical pixels; how many pixels were read from
the fill; and the derivation of the picture from the image, below. The composite does
not encode a file and makes no artifact: the action that calls it does.

**Protected pixels never change** (`SPEC.md` section 3) is held twice. The composite
copies every pixel whose weight is 0. Then the core compares the result with the image
under the mask, when it makes the derivation; were a pixel of weight 0 to differ, there
would be no derivation and the composite would fail with `NotPreserved`. That second
check is rascal's preservation check (`crates/rascal-mask/src/heal.rs`,
`preservation_check`), which ended every heal; here it cannot fail unless this code is
wrong, and no test reaches it through the composite.

### Who vouches for a derivation

ADR 0002 left a boundary open, and ADR 0004 handed this task two questions about it.

**A derivation is made only by looking at the pictures.** `Derivation::checked(parent,
derived, mask)` is the one way to make one. It is given the image, the picture said to
be made from it, and a mask that belongs to the image, and it checks that:

- the mask belongs to `parent`, and both pictures are the size of the mask's space;
- the two are not the same picture, which would derive nothing;
- every pixel the mask weighs 0, outside its core and its blend or under its protect, is
  the same four bytes in both. Any that differ are counted, and the refusal says how
  many and where the first is.

Where the mask's weight is over 0 the pictures may differ in any way. The derivation it
returns holds the binding that places `derived` onto `parent` by the identity in the
mask's space, as before; it adds nothing to that binding and has no identity of its own.

`Derivation::stated`, which took a binding at its word, is no longer open: nothing
outside its module can call it. So the rebind's old boundary, "it trusts that the
statement is true" (ADR 0002), is closed where it could be: a derivation cannot be
stated of two unrelated pictures of one size, unless the mask it is checked against
weighs every pixel. The check compares the pictures only where the mask weighs nothing,
so a mask whose support is the whole image, with nothing protected, leaves no pixel to
compare and vouches for any two pictures of its size
(`rebind::known_a_mask_that_weighs_every_pixel_vouches_for_any_picture_of_its_size`).
That is the maker's statement again: whoever made that mask said every pixel may
change. The composite hands the check the picture it was given, whose digest is not
worked out a second time (`Derivation::checked_of`). The test that pinned the old boundary,
`rebind::a_derivation_is_trusted_to_be_true`, is replaced by its refusal:
`rebind::a_derivation_is_made_only_of_two_pictures_that_differ_inside_a_mask_alone`.

- **The composite states the derivation of the picture it made,** by that check, against
  the mask it was given. When the picture is the image unchanged it states none: a mask
  on the image is already a mask on the picture.
- **A derivation is about two pictures, not about one mask.** Any mask on `parent`, in
  the space of the mask it was checked with, may be rebound through it: a protect mask
  used on the result of a fill made through another mask, which is the ordinary case.
- **It is a comparison of the pictures after all.** ADR 0002 turned down a rebind that
  compares the two pictures outside the mask being rebound, because that refuses the
  ordinary case just named and decodes two pictures at every rebind. This check does
  neither: it is made once, where a composite is made and both pictures are in memory,
  against the mask the composite was made through.

**The trust boundary for prepare is the maker's statement.** Of the two decisions ADR
0004 offered, this record takes the first. Prepare does not ask for a record that the
executor or the composite wrote; it accepts a mask on the image its binding's source
names, and a mask is the statement of whoever made it.

- A mask made by hand, with `Mask::from_core` or as a file brought in by `whizbang store
  import`, on a binding that places one image onto another by the identity, is accepted
  on the first image. The old pin, rewritten with `Mask::from_core`, holds this, and
  shows that no derivation of the two pictures can be made:
  `prepare::a_mask_made_by_hand_on_an_image_placed_onto_another_is_taken_at_its_makers_word`.
- The other decision was turned down because it would guard nothing. A rebound mask and
  the mask made by hand on the same binding are one artifact with one identity (ADR
  0004), so a record kept beside an artifact would be a second statement about it that
  the artifact does not hold, that does not travel with its file, and that a store
  which imported the mask would not have.
- What is guaranteed does not rest on where a mask came from: a mask is used only on the
  image its binding's source names, and whatever mask a composite is given, on whatever
  image, every pixel it weighs 0 comes out as it went in. A mask made by hand on the
  wrong image protects the wrong places; it cannot make a protected pixel of that image
  change.

### Segment

**The request** is one image, a batch of 1 to 64 requests (`QUERY_LIMIT`), and the most
instances wanted for each, from 1 to 64 (`INSTANCE_LIMIT`). A request of the batch is one
of:

| Kind | Holds | Rule |
|---|---|---|
| Phrase | a text | 1 to 512 bytes (`PHRASE_LIMIT`) |
| Point | `x`, `y` | Finite; in a pixel of the image: `x` from 0 to under the width, `y` from 0 to under the height |
| Box | left, top, right, bottom | Finite; inside the image; has area |

Coordinates are in pixels of the image, as ADR 0001's run. A box is a region, and
"inside the image" includes the image's edges for it, as in ADR 0002: its right may be
the width and its bottom the height. A point picks a pixel, so one on the right or the
bottom edge, which is in no pixel, is refused. One request to a backend is one image, so whatever a
backend computes of the image it computes once for the batch (`SPEC.md` section 4).

**The response** is, for each request of the batch and in the batch's order, a list of
instances, in the order the backend gives them. An `Instance` is:

- **a plane,** the size of the image, one byte a pixel, row by row from the top left:
  255 on the thing and 0 off it. Any other byte is refused, and so is a plane with
  nothing on it: a thing that covers no pixel is not an instance;
- **a box,** left, top, right and bottom in pixels of the image: finite, inside the
  image, with area. **The box holds the plane:** every pixel that is on lies among the
  whole pixels the box touches, from the floor of its left and top to the ceiling of
  its right and bottom. A box that leaves part of its thing outside is refused; a box
  larger than its thing, as rascal's padded boxes were, is not;
- **a score:** a number from 0 to 1. One that is not a number is refused.

The handle refuses, as `malformed`, an answer that does not answer each request once, one
that holds more instances for a request than were asked for, and an instance whose plane
is of another size than the image. So what a response that is accepted can hold is
bounded by the request: 64 requests, 64 instances each, a byte a pixel. That bound is
checked after the backend built the response. It does not stop a backend from building
more before the handle sees it; bounding what is read off a wire or out of a process,
before anything is built from it, is the backend's, at checkpoint 1C
(`docs/carry-over.md`).

Not in the request: rascal's threshold, padding, outline rule and least cover. They are
settings of an adapter, not of a call, and come with the adapters and the config. Nor an
order among instances: rascal sorted every adapter's answer, best score first; here the
order is the backend's, and an action that needs one sorts. Both are on
`docs/carry-over.md`.

### Judge

The code is `capability/judge.rs`. The shapes are squint's (`provider.rs`), with what
follows changed.

**The request** (`JudgeRequest`) is a system prompt, a conversation, tool definitions
and controls.

**A conversation is ordered, mixed content.** It is a list of messages; a message is a
role and a list of parts; the order of each list is the order the model will read it in,
and nothing reorders, merges or splits a part. A part is one of five:

| Part | Holds | May be said by |
|---|---|---|
| Text | a text of 1 to 1,048,576 bytes | user, assistant, system |
| Image | a `Picture` | user |
| Tool call | an id, the tool's name, and its arguments as a JSON value | assistant |
| Tool result | the id of the call it answers, a text of at most 1,048,576 bytes, whether it is an error, and up to 16 pictures; a text or a picture at least | user |
| Opaque | the adapter that made it, and its body | assistant |

A message is refused when it is empty, when it holds more than 256 parts, or when it
holds a part its role cannot say. A system message is an instruction of the harness
inside the conversation, as squint's was; it holds text only.

- **A tool call's id** is 1 to 128 printable ASCII characters with no space, and **the
  name it calls** 1 to 64. The name is what the model wrote. It need not be a tool the
  request defined: a call of a tool that does not exist is kept, so that the harness can
  answer it with an error. The arguments are any JSON value, of at most 1,048,576 bytes
  when written.
- **A tool definition** is a name, which keeps the rule of an action's name; a
  description of 1 to 4,096 bytes; and a schema, a JSON object of at most 1,048,576
  bytes when written. A request defines at most 128, no two of one name.
- **The controls** are the most output tokens, 1 or more. A choice of tool, a
  temperature, a seed, stop sequences and hints for a provider's cache are absent, as
  they were in squint's interface; each comes with the adapter that needs it.
- **Bounds on the whole.** At most 1,024 messages, and at most 67,108,864 bytes of text
  in all: the system prompt, every text, every tool result's text, every opaque body,
  every tool call's arguments as written, and each tool definition's name, description
  and schema as written. Pictures are not counted in that; each is
  held to `PIXEL_LIMIT`, and a conversation shares the pixels of a picture it holds more
  than once.

What `{:?}` prints of a request, a message, a turn or a part is its shape: the kind of
each part and its size, a picture's size and digest, a tool call's id and name. Never a
text, a body or an argument: a conversation is a prompt and a model's answers.

**A provider-opaque part is kept whole.** An `Opaque` is the bytes a provider sent for
its own later reading (thinking, a signature, a reasoning item), as text, of 1 to
1,048,576 bytes, marked with the adapter that made it. Whizbang never parses it, orders
its keys, prints its numbers again or trims it: the body that comes out is the body that
went in, byte for byte. Squint kept such a block as a JSON value, which is not that: a
JSON library is free to order keys and print numbers its own way.

What "whole" covers, and what it does not:

- **A part over 1,048,576 bytes is refused, never cut.** An adapter that is sent one
  cannot keep it, and fails the turn with the fault `too_large`. A part cut short
  would be sent back to the provider as if it were whole.
- **A body is UTF-8 text.** Every provider's part known to this record arrives inside a
  JSON document, which is UTF-8. A part of other bytes cannot be held as it is; an
  adapter that meets one encodes it, and decodes it before it sends it back.
- **It is whole from the adapter on.** Whether the adapter kept the provider's bytes
  when it cut the part out of a response is the adapter's, and is in its own tests: one
  that parses a response and writes the part again has already lost it.

- The adapter's name is the backend's name: the first part of the model identity a
  backend declares. It keeps the rule of an action's name, so a backend that makes
  opaque parts has such a name.
- The handle sends an opaque part only to the adapter it is marked with. A conversation
  that holds a part marked with another, anywhere in it, is not sent: the fault is
  `unsupported`.
- **The mark is a tag the handle checks when it sends, not proof of where a part came
  from.** `Opaque::new` is open to any code and takes the tag it is given. The tag is a
  backend's name, so two models of one backend take each other's parts, and all three
  fakes are `fake`. What it stops is a conversation carried from one provider to
  another by mistake.
- A turn that holds a part marked with another adapter than the backend that answered
  is `malformed`.

**The response** is a `Turn`, or a fault. A turn is:

- **content,** the parts the model wrote, in the order it wrote them: text, tool calls
  and opaque parts, at most 256, holding at most 67,108,864 bytes of text in all, as a
  conversation may: a turn is said back as a message of it. Several calls in one turn are several parts.
  `Turn::message` gives the same parts, whole and in order, as the assistant's message
  for the conversation that goes on. Squint's response held the texts and the calls as
  two lists beside the content; here they are read from the one list;
- **a stop reason:** the turn ended, it stopped for its tool calls to be answered, it was
  cut off at the most output tokens, the model declined, or another reason in the
  provider's word;
- **usage,** or none: input and output tokens, and input tokens read from and written to
  the provider's cache. None means the backend reported none, which is not the same as
  having used nothing; what a turn with no usage costs is the ledger's to say;
- **the provider's id for the exchange,** or none.

A turn is refused when it holds what only a user says; when two of its tool calls have
one id, since they could not be answered apart; when it stopped for a tool and calls
none; and when it holds nothing at all, unless it was cut off, declined, or stopped for
another reason. A turn that ended with nothing in it gives the harness nothing to
continue from. A turn that was cut off keeps what came before the cut.

**A turn is a backend's answer, and is not trusted.** Every bound above is checked when
the turn is made, each refusal is an `Unfit` with its type, and the handle makes a
refusal of what a backend handed it the fault `malformed`; nothing panics on a turn of
any shape. What is not covered: the bounds are checked on values that already exist, so
an adapter that reads a body of any size and parses it has spent that memory before a
turn is refused. Bounding what is read is the adapter's, at checkpoint 1C, on the row
that holds the same for segment and fill (`docs/carry-over.md`).

**What the types do not check is the shape of a conversation.** They hold each part and
each bound. They do not hold: the order of roles; that a tool result answers a call
that was made; that two calls in one message have two ids, which a turn does check;
that a stop reason agrees with the content, beyond the two rules above; that another
stop reason has any words. A conversation that is wrong in one of these ways is made
and is sent. Keeping the shape is the harness's, which builds the conversation, and
refusing a wrong one is the provider's. A test pins three of them
(`judge::what_a_conversation_does_not_check_is_known`).

`Part`, `StopReason` and `Usage` can each gain a member without breaking code that
reads them: the two enums are not matched exhaustively outside the core, and a usage is
made through `Usage::new` and read through its methods.

A stop reason in the provider's word and the provider's id are a provider's text, and
end up in a record. White space at the ends of an id is dropped first. Each is cleaned as a fault's sentence is, and cut at 64 and at 128
characters.

**Typed errors** are faults, the same eight kinds for every capability. A backend sends
one request and returns: it never retries and never changes the conversation. Retrying,
repairing a turn, and what a refusal or a cut-off turn means for a verdict are the
harness's (squint's judge loop, milestone 2).

**An opaque part and redaction.** A conversation in memory holds every part whole, and
that is what is sent. What is written of a run passes through redaction like every
other text (ADR 0005): a secret the executor was told of is taken out of an opaque
part's body too, and a part so changed is no longer the part the provider sent. A
secret is not expected inside a provider's part, and one inside an encoding of it is
not found (ADR 0005, "What redaction does not find"). What the `judge` group of the run
record keeps of a part, and whether it marks one that redaction changed, is decided with
that group (`docs/carry-over.md`).

Not carried over from squint's provider interface, each on `docs/carry-over.md`: a
backend's prices, the preparation of an image for a provider and its version, and who
receives a request.

### The fakes

`whizbang-testkit` has one for each capability (`capabilities.rs`). None runs a model or
sends anything, and each counts the requests it was sent, answered or not.

- `FakeFiller` paints as it was built to: the whole image repainted, every channel of
  every pixel 255 less what it was, so that no pixel is left as it was; one colour
  everywhere; one colour in the support and the image elsewhere; an answer of another
  size; or a fault.
- `FakeSegmenter` answers a phrase with the boxes it was told to find for it, a point
  with the pixel under it, and a box with the whole pixels it touches.
- `FakeJudge` plays a scripted transcript, a turn or a fault for each request in order,
  and keeps every request it was sent. It does not look at what it is asked: a request
  its script's writer did not expect is answered with the next entry like any other,
  and a test that cares what was asked reads what the fake kept. Past the end of its
  script it answers with the fault `malformed`; it never panics.

Each fake may be shared across threads, and counts every request then too. The judge
takes a request and plays its next entry in one step, so of many simultaneous requests
each gets one entry, and the request kept in place `n` is the one answered with entry
`n`. Nothing in checkpoint 1A sends two requests to a capability at once: task A6's two
simultaneous requests are calls to the executor.

Each fake can be told to stop at a named barrier of the test kit (`stopping_at`): it
then holds every request it is sent open until a test lets it go, having counted it,
and notes what the `Bounds` it was handed say of the run's cancellation when the
request arrives and when it goes on, and the deadline (`observed`). A request whose run
was cancelled meanwhile is answered with the fault `cancelled`. A judge that stops
keeps the request before it stops and plays an entry after, so a request cancelled
while it is held plays none. And the fake segmenter can be told to answer every request
with a fault (`failing`).

A request the handle refused, because the run was cancelled or the fake does not
declare a feature, never reaches the fake and is not counted. Prepare is shown to send
none of them a request (`prepare::prepare_sends_no_capability_request`): the roster it
asks is built from what the same fakes declare.

## Consequences

- An action, a fake and a real backend meet at types that cannot hold a malformed plane
  or fill, so a tool does not check what it is handed, and a backend cannot hand over
  what was not checked.
- A fault can be written to a run record after redaction, and what that leaves in it is
  as good as the adapter's keeping of the rule above.
- A fault does not say whether a request that failed may have been charged: `unavailable`
  covers both a backend that was never reached and one that was sent the request and
  did not answer. The ledger cannot settle from a fault as it is (`docs/carry-over.md`);
  a fault's fields are private, so it can gain one for that.
- The support plane is the same for every filler. Rascal's four renderings of a mask for
  a filler are not carried over as a filler's setting, and a backend cannot rebuild
  them from what it is sent: the support merges the core and the blend, where `alpha`
  and `tinted` marked the core alone, `grey` carried the weight, and `binary` cut at a
  threshold. A backend whose model wants the image made transparent or tinted where it
  is to be painted can make that picture over the whole support, and no other
  (`docs/carry-over.md`).
- A filler that needs a soft edge does not get one. The composite blends by the mask's
  weight whatever the filler painted, so the edge is soft in the result.
- `Backends::resolve` changed: every implementor takes a choice. Prepare's fingerprint
  did not change.
- A call can name a backend only where its action's descriptor names the argument for
  it. An action whose need names none always gets the default.
- A fill cannot change a pixel outside its mask's support or under its protect, whatever
  the filler returned, and a picture cannot be called derived from an image it does not
  match there.
- Rebinding a mask needs the derivation, and a derivation exists only where a composite
  was just made. An action that fills and returns its mask on the result rebinds it then
  (ADR 0003, `rebound_onto`). Nothing can rebind a mask onto a stored composite later;
  no action of checkpoint 1A needs to, and the first that does is given a way
  (`docs/carry-over.md`).
- The composite pastes by the mask's weight and in no other way. Rascal's `hard` and
  `poisson` pastes, and a fill pasted back through a crop or a wrapped cut, are not
  here (`docs/carry-over.md`).
- A judge adapter must keep the exact text of each part it will be handed back. An
  adapter that parses a response and writes the part again does not keep it whole.
- Reversing this costs every backend, every fake and every action that asks a
  capability. It is cheap now: nothing but fakes implements these.

## Alternatives

- **A support plane of weights,** 0 to 255 as `round(255 w)`, which is rascal's `grey`
  rendering. Turned down: every byte would be a legal value, so nothing sent by mistake
  could be refused; a filler that painted in proportion would hand back, at the edge,
  pixels that are already part original, which the composite would then mix with the
  original again; and "the area to paint" (`SPEC.md` section 4) is an area.
- **A support plane cut at a threshold,** rascal's `binary`. Turned down: a pixel whose
  weight is under the threshold and over 0 would be read from the fill by the composite
  without having been painted.
- **A plane of floating-point weights.** Turned down: four or eight bytes a pixel for
  what a mask holds in one, and values that are not numbers to refuse.
- **Take four channels from a filler and ignore the alpha,** as rascal's paste did,
  saying so in its summary. Turned down: a response that is not what was asked for is
  refused here, not repaired; an adapter that knows its model returns alpha drops it,
  where the knowledge is.
- **Resize a fill of the wrong size.** Turned down, as rascal turned it down: "there is
  no resize and no fallback".
- **Fall back to the default when the chosen backend lacks a feature,** or to any backend
  that has it. Turned down by `SPEC.md` section 4: a model is never substituted silently.
- **Make the only backend offered the default.** Turned down: which model answers would
  then change when a second is installed.
- **Features as a closed list in the core.** Turned down: a descriptor's features are
  names (ADR 0003), and a backend of the Python pack may declare one the core has never
  heard of. The core names only those its own requests need.
- **One request and response shape for all three,** with the capability as a field.
  Turned down by `SPEC.md` section 4.
- **Mix in linear light.** Turned down: rascal's paste mixed the stored values, its
  results were judged on that, and the blend zone is a few pixels of feather; a change
  of the mix is a change to measure, not one to make in passing.
- **Mix in floating point, as rascal did.** Turned down: the same bytes come of whole
  numbers, and a sum of whole numbers needs no statement of an order of operations or
  of a rounding mode to be the same on every machine.
- **Let the fill decide the alpha where it painted,** making a filled pixel opaque.
  Turned down: a fill has no alpha, an image with a transparent background would grow
  an opaque patch, and rascal kept the base's alpha.
- **Have the composite take a support plane and weights instead of a mask.** Turned
  down: the planes would have to be checked against each other again, which the mask's
  type has done, and the derivation is checked against a mask.
- **Keep an opaque part as a JSON value,** as squint did. Turned down: the bytes that
  come out depend on a JSON library's ordering of keys and printing of numbers, and a
  provider's part may be signed over its bytes.
- **Tool arguments as text,** whole like an opaque part. Turned down: the harness reads
  them to run the tool, and the model does not get them back checked against a
  signature.
- **A choice of backend as a field of a call,** beside its arguments. Turned down: an
  action can need two capabilities, a call would need a choice for each, and the
  arguments already say it (`filler`, `model`), as rascal's tasks did.
- **Asynchronous traits.** Turned down for now: the core has no runtime, `submit` blocks
  (ADR 0004), and work that blocks is to run on an executor of its own (`SPEC.md`
  section 3). A request that blocks and watches a cancellation signal can be run from
  either kind of caller.
- **A fault that keeps the adapter's own error as its source.** Turned down: a source is
  printed by whatever prints a chain of causes, and an HTTP client's error holds an
  address.
