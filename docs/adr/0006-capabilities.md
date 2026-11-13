# 0006. Capabilities: an interface for each, declared features, and resolution by choice, then default

Status: accepted
Date: 2026-11-12
Amended: 2026-11-12, with the second pull request of task A8, which this record said
would add to it. Added: "Judge"; "How a call names its choice"; "The fakes"; the judge's
trait and handle among what every capability shares; two consequences and three
alternatives. Changed: the sentences that said the handles in an action's context were
still empty, and that prepare handed resolution no choice.
Spec: section 3, "Crates and their edges"; section 4, "Backends"

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
  effective weight `w`, 255 to paint; `binary`, 255 where `w` is at least a threshold;
  `alpha`, the image made transparent where it is to be painted; `tinted`, the image
  tinted there. It answers with a PNG of exactly the dimensions sent, and anything else
  is an error.
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

**This record is merged in parts, with task A8's code.** The first part decided what
every capability shares, faults, declarations, resolution, fill and segment. The second
added the judge's interface, how a call names its choice, and the fakes. The composite
of a fill, and who vouches for the derivation it states, come with the last pull request
of the task, which adds its sections here with a dated note.

## Decision

The code is in `crates/whizbang-core/src/capability/`; the tests that hold it to this
record are in `crates/whizbang-core/tests/it/capability/`.

### What every capability shares

- **A trait for each capability,** in the core: `Segmenter`, `Filler` and `Judge`. Each
  has two methods. `declaration` says which model answers and which features it supports. The
  other takes one request and a cancellation signal, and returns the response or a fault.
  It blocks until it has its answer. A backend is expected to stop early when the signal
  is set; it never retries. The core holds no implementation.
- **A handle for each,** which is what an action asks through: `SegmentHandle`,
  `FillHandle`, `JudgeHandle`. A handle wraps a backend and does three things the backend is not trusted
  to do. Once the run is cancelled it sends nothing, and answers with the fault
  `cancelled`. It sends a request only to a backend that declares every feature the
  request needs, and otherwise answers `unsupported` without sending. And it holds the
  answer to the request: an answer that does not fit is the fault `malformed`. The
  handles in an action's context (`Capabilities`, ADR 0004) are these. The executor
  hands an action none of them yet: the first action that asks a capability through it
  brings that, with what the run record keeps of each request (`docs/carry-over.md`).
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

**A fault holds no path and no address, by how it is made.** A fault is written into a
run record. Its sentence is cleaned when the fault is made, whatever an adapter passed in:

- the first 240 characters are taken (`FAULT_LIMIT`);
- each is kept when it is an ASCII letter, a digit, a space, or one of
  `. , ; : ' ( ) % + - _`;
- every other character becomes `?`;
- a sentence of no characters becomes `no reason was given`.

So a sentence has no `/` and no `\`, which a path needs; no `/`, `@`, `=` or `&`, which
an address with a credential or a query needs; no line break, double quote or angle
bracket. A fault's `{:?}` prints the kind, the cleaned sentence and the wait, and nothing
else: a fault has no field that holds a request, a body, a header or an address, and no
source error behind it.

What cleaning does not do is find a secret. A key passed in as letters and digits stays
letters and digits. Two things stand against that. An adapter writes a fault's sentence
from fixed words, a status and what the provider said was wrong, never from what it sent.
And the executor takes the secrets it was told of out of every text it writes or reports,
an error's message among them (ADR 0005, "Redaction").

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
  of a fill is the composite's to decide, in the last part of this record.
- **Also refused:** a width or a height of 0; more than 50,000,000 pixels.

A filler is not trusted to leave pixels alone (`SPEC.md` section 4). Nothing in this
interface asks it to: what it returns outside the support is never read. Holding that is
the composite's, in the last part of this record.

A prompt is sent only to a filler that declares `prompt`. It is not dropped for one that
does not: the request is refused as `unsupported`.

### Segment

**The request** is one image, a batch of 1 to 64 requests (`QUERY_LIMIT`), and the most
instances wanted for each, from 1 to 64 (`INSTANCE_LIMIT`). A request of the batch is one
of:

| Kind | Holds | Rule |
|---|---|---|
| Phrase | a text | 1 to 512 bytes (`PHRASE_LIMIT`) |
| Point | `x`, `y` | Finite; inside the image |
| Box | left, top, right, bottom | Finite; inside the image; has area |

Coordinates are in pixels of the image, as ADR 0001's run, and "inside the image"
includes its edges, as in ADR 0002. One request to a backend is one image, so whatever a
backend computes of the image it computes once for the batch (`SPEC.md` section 4).

**The response** is, for each request of the batch and in the batch's order, a list of
instances, in the order the backend gives them. An `Instance` is:

- **a plane,** the size of the image, one byte a pixel, row by row from the top left:
  255 on the thing and 0 off it. Any other byte is refused, and so is a plane with
  nothing on it: a thing that covers no pixel is not an instance;
- **a box,** left, top, right and bottom in pixels of the image: finite, inside the
  image, with area;
- **a score:** a number from 0 to 1. One that is not a number is refused.

The handle refuses, as `malformed`, an answer that does not answer each request once, one
that holds more instances for a request than were asked for, and an instance whose plane
is of another size than the image. So what a response can cost is bounded by the request:
64 requests, 64 instances each, a byte a pixel.

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
- **The controls** are the most output tokens, 1 or more. Others come with the adapters
  that need them.
- **Bounds on the whole.** At most 1,024 messages, and at most 67,108,864 bytes of text
  in all: the system prompt, every text, every tool result's text, every opaque body
  and every tool call's arguments as written. Pictures are not counted in that; each is
  held to `PIXEL_LIMIT`, and a conversation shares the pixels of a picture it holds more
  than once.

**A provider-opaque part is kept whole.** An `Opaque` is the bytes a provider sent for
its own later reading (thinking, a signature, a reasoning item), as text, of 1 to
1,048,576 bytes, marked with the adapter that made it. Whizbang never parses it, orders
its keys, prints its numbers again or trims it: the body that comes out is the body that
went in, byte for byte. Squint kept such a block as a JSON value, which is not that: a
JSON library is free to order keys and print numbers its own way.

- The adapter's name is the backend's name: the first part of the model identity a
  backend declares. It keeps the rule of an action's name, so a backend that makes
  opaque parts has such a name.
- The handle sends an opaque part only to the adapter that made it. A conversation that
  holds another adapter's part is not sent: the fault is `unsupported`.
- A turn that holds a part marked with another adapter than the backend that answered
  is `malformed`.

**The response** is a `Turn`, or a fault. A turn is:

- **content,** the parts the model wrote, in the order it wrote them: text, tool calls
  and opaque parts, at most 256. Several calls in one turn are several parts.
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

A stop reason in the provider's word and the provider's id are a provider's text, and
end up in a record. Each is cleaned as a fault's sentence is, and cut at 64 and at 128
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
  and keeps every request it was sent.

A request the handle refused, because the run was cancelled or the fake does not
declare a feature, never reaches the fake and is not counted. Prepare is shown to send
none of them a request (`prepare::prepare_sends_no_capability_request`): the roster it
asks is built from what the same fakes declare.

## Consequences

- An action, a fake and a real backend meet at types that cannot hold a malformed plane
  or fill, so a tool does not check what it is handed, and a backend cannot hand over
  what was not checked.
- A fault can be written to a run record as it is.
- The support plane is the same for every filler. Rascal's four renderings of a mask for
  a filler are not carried over as a filler's setting. A backend whose model wants the
  image made transparent or tinted where it is to be painted makes that picture itself,
  from the image and the plane.
- A filler that needs a soft edge does not get one. The composite blends by the mask's
  weight whatever the filler painted, so the edge is soft in the result.
- `Backends::resolve` changed: every implementor takes a choice. Prepare's fingerprint
  did not change.
- A call can name a backend only where its action's descriptor names the argument for
  it. An action whose need names none always gets the default.
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
