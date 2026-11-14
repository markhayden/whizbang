//! Redaction: secrets are taken out of everything before it is written (`SPEC.md`
//! section 3, "What every run records"; ADR 0005, "Redaction"; ADR 0014, which changes
//! what is found).
//!
//! The pass began as squint's (`crates/squint-core/src/artifacts.rs`, `redact`): a secret
//! is replaced where it stands as it is, and where it stands as a JSON string would
//! escape it. Here it works on bytes, whatever their encoding. It looks for each secret
//! as it is, as JSON and percent-encoding write it, in UTF-16 and inside base64; and it
//! reads the text as percent-encoding, as the escapes of a JSON string, and with the
//! line breaks inside base64 passed over, and looks again in what each reading gives.
//! Every place is found over what was given before anything is replaced, and each
//! stretch is replaced once. The search follows the text a fixed number of times,
//! whatever the number of secrets and however many end at one place.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

/// What stands in what is stored where a secret was.
pub const REDACTED: &str = "[redacted]";

/// The shortest secret, in bytes. A shorter value is not a key: it would match inside
/// ordinary words, and inside the escapes of a JSON string.
pub const SECRET_MIN_BYTES: usize = 8;

/// Why a value cannot be a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRefused {
    /// The length of the value that was refused, without the white space at its ends, in
    /// bytes. The value itself is not kept.
    pub bytes: usize,
}

impl std::fmt::Display for SecretRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "a secret of {} bytes is too short to redact: a secret is at least \
             {SECRET_MIN_BYTES} bytes",
            self.bytes
        )
    }
}

impl std::error::Error for SecretRefused {}

/// The secrets of one executor: the values that must never be written. They are held to
/// be compared against, never printed: the type shows how many it holds and nothing else.
#[derive(Clone, Default)]
pub struct Secrets {
    values: BTreeSet<String>,
    /// What finds them. None when there is no secret.
    search: Option<Arc<Search>>,
}

impl PartialEq for Secrets {
    fn eq(&self, other: &Secrets) -> bool {
        self.values == other.values
    }
}

impl Eq for Secrets {}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secrets({})", self.values.len())
    }
}

/// One byte of a text as it is read, with the place in the bytes as they were given
/// that it came from: where that place starts, and where it ends.
type Unit = (u8, usize, usize);

/// What kind of form a form is. A wide form is UTF-16, of one byte order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    Base64,
    Wide { big: bool },
}

/// One form of one secret: its length in bytes; whether the character before it, or
/// after it, in base64 holds bits of the secret too; its kind; and which secret it is
/// of, by the secret's place among the secrets.
#[derive(Clone, Copy)]
struct Form {
    len: u32,
    before: bool,
    after: bool,
    kind: Kind,
    secret: u32,
}

impl Form {
    /// One form for two that are the same bytes. Two of one kind and one secret are
    /// one, with what either says of its edges. Otherwise the form is found wherever
    /// either would be: as base64 when either is, and never as a wide form, which can
    /// be left out for its twin.
    fn merged(self, other: Form) -> Form {
        let same = self.kind == other.kind && self.secret == other.secret;
        let base64 = self.kind == Kind::Base64 || other.kind == Kind::Base64;
        Form {
            before: self.before || other.before,
            after: self.after || other.after,
            kind: match (same, base64) {
                (true, _) => self.kind,
                (false, true) => Kind::Base64,
                (false, false) => Kind::Plain,
            },
            ..self
        }
    }
}

/// What ends at a state of the machine, put together when the machine is made from the
/// form that ends there and from every shorter form that ends there with it. One
/// stretch is noted for a place in a text, the longest, however many forms end there.
#[derive(Clone, Copy, Default)]
struct Mark {
    /// The length of the longest form that is not wide, or 0.
    len: u32,
    /// The length of the longest base64 form, or 0.
    base64: u32,
    /// The longest wide form, as its place in `wides` and one, or 0.
    wide: u32,
    flags: u8,
}

/// The longest form's character before it holds bits of its secret.
const BEFORE: u8 = 1;
/// The character after holds bits of the secret of some form that ends here.
const AFTER: u8 = 2;
/// The same two, of the base64 forms alone.
const BASE64_BEFORE: u8 = 4;
const BASE64_AFTER: u8 = 8;

/// Every form of every secret, as one machine that follows a text once and says what
/// ends at each place (the construction of Aho and Corasick). What following costs
/// does not depend on how many forms there are, nor on how many end at one place.
struct Search {
    /// From the start, by byte: the state to go to, or 0 to stay.
    start: Vec<u32>,
    /// The edges that leave each state, sorted: those of state `s` are at
    /// `first[s]..first[s + 1]` of `bytes` and `to`.
    first: Vec<u32>,
    bytes: Vec<u8>,
    to: Vec<u32>,
    /// Where to go on from when no edge leaves a state: the state of the longest
    /// proper end of what was read that some form begins with.
    fail: Vec<u32>,
    marks: Vec<Mark>,
    wides: Vec<Form>,
    longest: usize,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'-' | b'_')
}

/// The forms a secret is searched for in, as bytes: as it is; as a JSON string escapes
/// it, and the same with each `/` written `\/`; percent-encoded, with every byte that
/// is not a letter, a digit, `-`, `.`, `_` or `~` encoded, in either case of hex; in
/// UTF-16 of either byte order; and inside base64, in the standard alphabet and the
/// one for URLs, for each of the three places in a group of three bytes it can begin
/// at.
fn forms_of(secret: &str, id: u32) -> Vec<(Vec<u8>, Form)> {
    let form = |bytes: Vec<u8>, before: bool, after: bool, kind: Kind| {
        let len = bytes.len() as u32;
        let form = Form {
            len,
            before,
            after,
            kind,
            secret: id,
        };
        (bytes, form)
    };
    let plain = |bytes: Vec<u8>| form(bytes, false, false, Kind::Plain);
    let mut forms = vec![plain(secret.as_bytes().to_vec())];
    // Writing a string as JSON cannot fail. Exactly one quote is taken from each end:
    // a quote that is the secret's own is escaped, and is part of the form.
    let quoted = serde_json::to_string(secret).unwrap_or_default();
    let escaped = quoted
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_default();
    if !escaped.is_empty() {
        forms.push(plain(escaped.as_bytes().to_vec()));
        forms.push(plain(escaped.replace('/', "\\/").into_bytes()));
    }
    for upper in [true, false] {
        let mut encoded = Vec::new();
        for byte in secret.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                encoded.push(byte);
            } else if upper {
                encoded.extend(format!("%{byte:02X}").into_bytes());
            } else {
                encoded.extend(format!("%{byte:02x}").into_bytes());
            }
        }
        forms.push(plain(encoded));
    }
    let units = || secret.encode_utf16();
    for big in [false, true] {
        let bytes = if big {
            units().flat_map(u16::to_be_bytes).collect()
        } else {
            units().flat_map(u16::to_le_bytes).collect()
        };
        forms.push(form(bytes, false, false, Kind::Wide { big }));
    }
    for begins in 0..3 {
        // The characters every bit of which is the secret's. One before them holds
        // bits of the secret when it does not begin a group, and one after them when
        // its bits do not end a character.
        let mut data = vec![0u8; begins];
        data.extend_from_slice(secret.as_bytes());
        let bits = data.len() * 8;
        data.extend_from_slice(&[0, 0]);
        let sextet = |index: usize| {
            let (byte, shift) = (index * 6 / 8, index * 6 % 8);
            let pair = u16::from(data[byte]) << 8 | u16::from(data[byte + 1]);
            usize::from(pair >> (10 - shift) & 63)
        };
        let whole: Vec<usize> = ([0, 2, 3][begins]..bits / 6).map(sextet).collect();
        let standard: Vec<u8> = whole.iter().map(|six| BASE64[*six]).collect();
        let url = standard
            .iter()
            .map(|byte| match byte {
                b'+' => b'-',
                b'/' => b'_',
                other => *other,
            })
            .collect();
        forms.push(form(standard, begins != 0, bits % 6 != 0, Kind::Base64));
        forms.push(form(url, begins != 0, bits % 6 != 0, Kind::Base64));
    }
    forms
}

impl Search {
    fn new(secrets: &BTreeSet<String>) -> Search {
        // The states as a tree, first: an edge by the state it leaves and its byte;
        // for each state, the state and the byte that lead to it and how far it is
        // from the start; and the form that ends exactly there.
        let mut edges: HashMap<u64, u32> = HashMap::new();
        let mut led: Vec<(u32, u8, u32)> = vec![(0, 0, 0)];
        let mut own: Vec<Option<Form>> = vec![None];
        let mut longest = 1;
        for (id, secret) in secrets.iter().enumerate() {
            for (bytes, form) in forms_of(secret, id as u32) {
                let mut state = 0u32;
                for byte in &bytes {
                    let key = u64::from(state) << 8 | u64::from(*byte);
                    state = *edges.entry(key).or_insert_with(|| {
                        led.push((state, *byte, led[state as usize].2 + 1));
                        own.push(None);
                        led.len() as u32 - 1
                    });
                }
                longest = longest.max(bytes.len());
                let held = &mut own[state as usize];
                *held = Some(held.map_or(form, |held| held.merged(form)));
            }
        }
        // Then the edges in one sorted run, which takes a third of the room of a map.
        let states = led.len();
        let mut sorted: Vec<(u64, u32)> = edges.into_iter().collect();
        sorted.sort_unstable();
        let mut first = vec![0u32; states + 1];
        let mut start = vec![0u32; 256];
        for (key, to) in &sorted {
            first[(key >> 8) as usize + 1] += 1;
            if key >> 8 == 0 {
                start[(key & 0xff) as usize] = *to;
            }
        }
        for state in 0..states {
            first[state + 1] += first[state];
        }
        let mut search = Search {
            start,
            first,
            bytes: sorted.iter().map(|(key, _)| (key & 0xff) as u8).collect(),
            to: sorted.iter().map(|(_, to)| *to).collect(),
            fail: vec![0; states],
            marks: vec![Mark::default(); states],
            wides: Vec::new(),
            longest,
        };
        // Nearer states first, so that a state's `fail`, and what ends there, are
        // known before its children's.
        let mut order: Vec<u32> = (1..states as u32).collect();
        order.sort_by_key(|state| led[*state as usize].2);
        for state in order {
            let (from, byte, _) = led[state as usize];
            let fail = if from == 0 {
                0
            } else {
                search.step(search.fail[from as usize], byte)
            };
            search.fail[state as usize] = fail;
            // What ends at the state `fail` ends here too, and is shorter than a form
            // of this state's own.
            let mut mark = search.marks[fail as usize];
            match own[state as usize] {
                None => {}
                Some(form) if matches!(form.kind, Kind::Wide { .. }) => {
                    search.wides.push(form);
                    mark.wide = search.wides.len() as u32;
                }
                Some(form) => {
                    mark.len = form.len;
                    mark.flags &= !BEFORE;
                    mark.flags |= if form.before { BEFORE } else { 0 };
                    mark.flags |= if form.after { AFTER } else { 0 };
                    if form.kind == Kind::Base64 {
                        mark.base64 = form.len;
                        mark.flags &= !BASE64_BEFORE;
                        mark.flags |= if form.before { BASE64_BEFORE } else { 0 };
                        mark.flags |= if form.after { BASE64_AFTER } else { 0 };
                    }
                }
            }
            search.marks[state as usize] = mark;
        }
        search
    }

    fn edge(&self, state: u32, byte: u8) -> Option<u32> {
        if state == 0 {
            return Some(self.start[usize::from(byte)]).filter(|next| *next != 0);
        }
        let from = self.first[state as usize] as usize;
        let to = self.first[state as usize + 1] as usize;
        let at = self.bytes[from..to].iter().position(|held| *held == byte)?;
        Some(self.to[from + at])
    }

    /// The state after one more byte.
    fn step(&self, mut state: u32, byte: u8) -> u32 {
        loop {
            if let Some(next) = self.edge(state, byte) {
                return next;
            }
            if state == 0 {
                return 0;
            }
            state = self.fail[state as usize];
        }
    }

    /// Follows one reading of the bytes, and notes each stretch of the bytes as they
    /// were given that a form covers: for a place in the reading, the longest form
    /// that ends there, and the longest wide one apart. With `base64_only`, base64
    /// forms alone are looked for.
    ///
    /// A base64 character at either edge that holds bits of the secret is the unit of
    /// the reading before the form, or after it, and its whole stretch is taken: so
    /// an edge written as an escape is taken whole, and not its last byte.
    ///
    /// `most` is the most units the reading can give: the length of the bytes it is a
    /// reading of, since no reading gives more units than it was given bytes.
    fn follow(
        &self,
        reading: impl Iterator<Item = Unit>,
        most: usize,
        base64_only: bool,
        found: &mut Found,
    ) {
        // The byte and the start of each of the last `longest` units and one more; or
        // of every unit, where the text is shorter than the longest form. So a short
        // text costs its own length, and not the longest secret's.
        let room = self.longest.min(most) + 1;
        let mut last = vec![(0u8, 0usize); room];
        let mut state = 0;
        // The stretch noted at the unit before, when the unit after it may be its.
        let mut open: Option<usize> = None;
        for (count, (byte, start, end)) in reading.enumerate() {
            if let Some(noted) = open.take()
                && is_base64(byte)
            {
                found.stretches[noted].1 = end;
            }
            last[count % room] = (byte, start);
            state = self.step(state, byte);
            let mark = self.marks[state as usize];
            let (len, before, after) = if base64_only {
                (mark.base64, BASE64_BEFORE, BASE64_AFTER)
            } else {
                (mark.len, BEFORE, AFTER)
            };
            let len = len as usize;
            if len != 0 {
                let mut from = last[(count + 1 - len) % room].1;
                if mark.flags & before != 0 && count >= len {
                    let (edge, began) = last[(count - len) % room];
                    if is_base64(edge) {
                        from = began;
                    }
                }
                if mark.flags & after != 0 {
                    open = Some(found.stretches.len());
                }
                found.stretches.push((from, end));
            }
            if mark.wide != 0 && !base64_only {
                let form = self.wides[mark.wide as usize - 1];
                let from = last[(count + 1 - form.len as usize) % room].1;
                let big = form.kind == Kind::Wide { big: true };
                found.wide.insert((from, end, form.secret, big));
            }
        }
    }
}

/// What the readings of one text found: stretches of the bytes as they were given;
/// and, apart, those of wide forms, each with its secret and its byte order.
#[derive(Default)]
struct Found {
    stretches: Vec<(usize, usize)>,
    wide: BTreeSet<(usize, usize, u32, bool)>,
}

/// A text read as percent-encoding, as the escapes of a JSON string, or as both. With
/// `percent`, `%` and two hex digits are one byte. With `escapes`, a backslash and
/// `"`, `\`, `/`, `b`, `f`, `n`, `r` or `t` are one character, and `\u` and four hex
/// digits are one character, or half of one with the `\u` that follows. With `plus`,
/// a `+` is a space. Anything else is as it is, and so is what only looks like one of
/// these.
struct Read<I> {
    inner: I,
    percent: bool,
    escapes: bool,
    plus: bool,
    /// Up to twelve units ahead: the longest thing read as one is two `\u` escapes.
    ahead: VecDeque<Unit>,
    /// The rest of a character of several bytes, each from the same place.
    rest: VecDeque<Unit>,
}

impl<I: Iterator<Item = Unit>> Read<I> {
    fn new(inner: I, percent: bool, escapes: bool, plus: bool) -> Read<I> {
        Read {
            inner,
            percent,
            escapes,
            plus,
            ahead: VecDeque::new(),
            rest: VecDeque::new(),
        }
    }

    fn byte(&self, index: usize) -> Option<u8> {
        self.ahead.get(index).map(|unit| unit.0)
    }

    fn hex(&self, index: usize) -> Option<u32> {
        char::from(self.byte(index)?).to_digit(16)
    }

    /// The sixteen bits of a `\u` escape that begins at this unit.
    fn escape(&self, index: usize) -> Option<u32> {
        if self.byte(index) != Some(b'\\') || self.byte(index + 1) != Some(b'u') {
            return None;
        }
        let digit = |place: usize| self.hex(index + 2 + place);
        Some(digit(0)? << 12 | digit(1)? << 8 | digit(2)? << 4 | digit(3)?)
    }

    /// How many units are read as one, and the character or the byte they are.
    fn one(&self, byte: u8) -> Option<(usize, Result<char, u8>)> {
        match byte {
            b'%' if self.percent => {
                let value = self.hex(1)? * 16 + self.hex(2)?;
                Some((3, Err(u8::try_from(value).ok()?)))
            }
            b'+' if self.plus => Some((1, Err(b' '))),
            b'\\' if self.escapes => match self.byte(1)? {
                same @ (b'"' | b'\\' | b'/') => Some((2, Err(same))),
                b'b' => Some((2, Err(0x08))),
                b'f' => Some((2, Err(0x0c))),
                b'n' => Some((2, Err(b'\n'))),
                b'r' => Some((2, Err(b'\r'))),
                b't' => Some((2, Err(b'\t'))),
                b'u' => match self.escape(0)? {
                    high @ 0xd800..=0xdbff => match self.escape(6)? {
                        low @ 0xdc00..=0xdfff => {
                            let point = 0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00);
                            Some((12, Ok(char::from_u32(point)?)))
                        }
                        _ => None,
                    },
                    unit => Some((6, Ok(char::from_u32(unit)?))),
                },
                _ => None,
            },
            _ => None,
        }
    }
}

impl<I: Iterator<Item = Unit>> Iterator for Read<I> {
    type Item = Unit;

    fn next(&mut self) -> Option<Unit> {
        if let Some(unit) = self.rest.pop_front() {
            return Some(unit);
        }
        if self.ahead.is_empty() {
            self.ahead.extend(self.inner.next());
        }
        let (byte, start, end) = *self.ahead.front()?;
        if !matches!(byte, b'%' | b'+' | b'\\') {
            self.ahead.pop_front();
            return Some((byte, start, end));
        }
        while self.ahead.len() < 12 {
            match self.inner.next() {
                Some(unit) => self.ahead.push_back(unit),
                None => break,
            }
        }
        let Some((taken, read)) = self.one(byte) else {
            self.ahead.pop_front();
            return Some((byte, start, end));
        };
        let end = self.ahead.get(taken - 1).map_or(end, |unit| unit.2);
        self.ahead.drain(..taken);
        match read {
            Err(byte) => Some((byte, start, end)),
            Ok(character) => {
                let mut buffer = [0u8; 4];
                let encoded = character.encode_utf8(&mut buffer).as_bytes();
                self.rest
                    .extend(encoded[1..].iter().map(|byte| (*byte, start, end)));
                Some((encoded[0], start, end))
            }
        }
    }
}

/// A text with one line break passed over: a line feed, a carriage return, or the two
/// together. It is how a long body in base64 is wrapped. What follows the break is
/// given in its place, so a second break running is given, and a blank line still parts
/// two texts. Only base64 forms are looked for in this reading, so a form goes on
/// across a break only between two base64 characters.
struct Unwrapped<I>(I);

impl<I: Iterator<Item = Unit>> Iterator for Unwrapped<I> {
    type Item = Unit;

    fn next(&mut self) -> Option<Unit> {
        let mut unit = self.0.next()?;
        let first = unit.0;
        if first == b'\r' || first == b'\n' {
            // At the end of the text nothing follows, and the break is the last unit.
            unit = self.0.next().unwrap_or(unit);
            if first == b'\r' && unit.0 == b'\n' {
                unit = self.0.next().unwrap_or(unit);
            }
        }
        Some(unit)
    }
}

impl Secrets {
    /// No secret: nothing is replaced.
    pub fn none() -> Secrets {
        Secrets::default()
    }

    /// These values, each without the ASCII white space at its ends: a value read from a
    /// file or a variable often carries a line break there. Nothing else is taken off:
    /// quotes around a value are part of it. A value that is then empty is left out: it
    /// is what a variable that was never set expands to, and is no secret. Any other
    /// value shorter than [`SECRET_MIN_BYTES`] is refused, and then no secrets are
    /// made: a caller that meant to protect a value is told that it is not protected.
    pub fn new(values: impl IntoIterator<Item = String>) -> Result<Secrets, SecretRefused> {
        let mut secrets: BTreeSet<String> = BTreeSet::new();
        for value in values {
            let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
            if value.is_empty() {
                continue;
            }
            if value.len() < SECRET_MIN_BYTES {
                return Err(SecretRefused { bytes: value.len() });
            }
            secrets.insert(value.to_owned());
        }
        if secrets.is_empty() {
            return Ok(Secrets::none());
        }
        let search = Some(Arc::new(Search::new(&secrets)));
        Ok(Secrets {
            values: secrets,
            search,
        })
    }

    /// The bytes with every stretch that holds a secret, in any of its forms, replaced by
    /// [`REDACTED`]. The encoding does not matter: bytes that are not UTF-8, as a body cut
    /// in the middle of a character is not, are searched like any others. What comes
    /// back need not be UTF-8 though what was given was: a form can stand inside the
    /// bytes of other characters. A text goes through [`Secrets::redact`].
    ///
    /// The bytes are followed as they are. Where they hold a backslash, they are
    /// followed again as the escapes of a JSON string read them, and as that reading
    /// read once more, as escapes and as percent-encoding; where they hold a `%`, as
    /// percent-encoding reads them, and as that reading read once more, either way;
    /// where they hold both, with both read at once, once and twice;
    /// where they hold a `+`, as percent-encoding with each `+` a space; and where they
    /// hold a line break, plain or as an escape, with the line breaks inside a run of
    /// base64 passed over, for base64 alone. Every stretch found by any reading is
    /// found over the bytes as they were given, before anything is replaced; stretches
    /// that overlap are one stretch; each is replaced once. So two secrets that overlap
    /// leave no tail of either, and a secret that is part of the marker does not
    /// rewrite the marker.
    pub fn redact_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        let Some(search) = &self.search else {
            return bytes.to_vec();
        };
        let plain = || {
            bytes
                .iter()
                .enumerate()
                .map(|(at, byte)| (*byte, at, at + 1))
        };
        let escapes = || Read::new(plain(), false, true, false);
        let percent = || Read::new(plain(), true, false, false);
        let both = || Read::new(plain(), true, true, false);
        let holds = |byte: u8| bytes.contains(&byte);
        let (backslash, percents) = (holds(b'\\'), holds(b'%'));
        let mut found = Found::default();
        let most = bytes.len();
        search.follow(plain(), most, false, &mut found);
        // A reading of one kind can bring out the marks of the other: a `%5C` is a
        // backslash once read, and a `\u0025` a `%`. So the second reading of each
        // pair is of the other kind too.
        if backslash {
            search.follow(escapes(), most, false, &mut found);
            search.follow(
                Read::new(escapes(), false, true, false),
                most,
                false,
                &mut found,
            );
            search.follow(
                Read::new(escapes(), true, false, false),
                most,
                false,
                &mut found,
            );
        }
        if percents {
            search.follow(percent(), most, false, &mut found);
            search.follow(
                Read::new(percent(), true, false, false),
                most,
                false,
                &mut found,
            );
            search.follow(
                Read::new(percent(), false, true, false),
                most,
                false,
                &mut found,
            );
        }
        if backslash && percents {
            search.follow(both(), most, false, &mut found);
            search.follow(
                Read::new(both(), true, true, false),
                most,
                false,
                &mut found,
            );
        }
        if holds(b'+') {
            search.follow(
                Read::new(plain(), true, false, true),
                most,
                false,
                &mut found,
            );
        }
        if holds(b'\n') || holds(b'\r') {
            search.follow(Unwrapped(plain()), most, true, &mut found);
        }
        if backslash {
            search.follow(Unwrapped(escapes()), most, true, &mut found);
        }
        // A secret of ASCII letters in UTF-16 of one byte order is also, one byte to
        // the side, the same secret in the other order, with a zero byte of the letter
        // beside it. Of two such stretches, of one secret, the one that begins at an
        // even place is the secret, since UTF-16 is in pairs of bytes, and the other is
        // left out: it would take a byte of a neighbour with it. A stretch with no such
        // twin is kept wherever it begins, and so is one beside another secret's.
        let Found {
            mut stretches,
            wide,
        } = found;
        for (from, to, secret, big) in &wide {
            let twin = |from: usize, to: usize| wide.contains(&(from, to, *secret, !*big));
            if from % 2 == 0 || !(twin(from - 1, to - 1) || twin(from + 1, to + 1)) {
                stretches.push((*from, *to));
            }
        }
        if stretches.is_empty() {
            return bytes.to_vec();
        }
        stretches.sort_unstable();
        let mut out = Vec::with_capacity(bytes.len());
        let mut copied_to = 0;
        // The stretch being put together, until one begins at or past its end.
        let mut stretch = stretches[0];
        let end = [(usize::MAX, usize::MAX)];
        for (from, to) in stretches.into_iter().skip(1).chain(end) {
            if from < stretch.1 {
                stretch.1 = stretch.1.max(to);
                continue;
            }
            out.extend_from_slice(&bytes[copied_to..stretch.0]);
            out.extend_from_slice(REDACTED.as_bytes());
            copied_to = stretch.1;
            stretch = (from, to);
        }
        out.extend_from_slice(&bytes[copied_to..]);
        out
    }

    /// The text with every secret replaced, as [`Secrets::redact_bytes`] replaces it.
    /// What comes back is a text: where a form stood inside the bytes of other
    /// characters, as a secret's UTF-16 bytes can, what is left of those characters is
    /// no text, and is written as the mark for that.
    pub fn redact(&self, text: &str) -> String {
        String::from_utf8_lossy(&self.redact_bytes(text.as_bytes())).into_owned()
    }

    /// A body that is JSON, with every secret taken out and still JSON: each string, a
    /// key or a value, is decoded, searched as [`Secrets::redact`] searches a text, and
    /// written again as a JSON string when it held one; a string that held none is left
    /// byte for byte as it was, and so is everything between the strings. A number,
    /// `true`, `false` or `null` that holds a secret becomes the string `"[redacted]"`.
    ///
    /// A body that is not JSON is not refused: it is searched as bytes, as
    /// [`Secrets::redact_bytes`] searches.
    pub fn redact_json(&self, body: &[u8]) -> Vec<u8> {
        if self.search.is_none() {
            return body.to_vec();
        }
        if serde_json::from_slice::<serde::de::IgnoredAny>(body).is_err() {
            return self.redact_bytes(body);
        }
        let marker = format!("\"{REDACTED}\"");
        let between = |byte: &u8| b"{}[]:, \t\n\r\"".contains(byte);
        let mut out = Vec::with_capacity(body.len());
        let mut at = 0;
        while let Some(byte) = body.get(at) {
            if *byte != b'"' && between(byte) {
                out.push(*byte);
                at += 1;
                continue;
            }
            if *byte != b'"' {
                // A number, or one of the three words.
                let length = body[at..]
                    .iter()
                    .position(between)
                    .unwrap_or(body.len() - at);
                let token = &body[at..at + length];
                if self.redact_bytes(token) == token {
                    out.extend_from_slice(token);
                } else {
                    out.extend_from_slice(marker.as_bytes());
                }
                at += length;
                continue;
            }
            // A string, to the quote that ends it: a backslash takes the byte after it.
            let mut end = at + 1;
            while let Some(inside) = body.get(end) {
                end += if *inside == b'\\' { 2 } else { 1 };
                if *inside == b'"' {
                    break;
                }
            }
            let end = end.min(body.len());
            let written = &body[at..end];
            match serde_json::from_slice::<String>(written) {
                Ok(text) => {
                    let redacted = self.redact(&text);
                    if redacted == text {
                        out.extend_from_slice(written);
                    } else {
                        // Writing a string as JSON cannot fail.
                        let again = serde_json::to_string(&redacted).unwrap_or(marker.clone());
                        out.extend_from_slice(again.as_bytes());
                    }
                }
                // A string that is JSON and no text, as half of a pair of escapes is:
                // it is searched as it is written, and replaced whole if it holds one.
                Err(_) if self.redact_bytes(written) == written => out.extend_from_slice(written),
                Err(_) => out.extend_from_slice(marker.as_bytes()),
            }
            at = end;
        }
        out
    }
}
