//! Redaction: secrets are taken out of everything before it is written (`SPEC.md`
//! section 3, "What every run records"; ADR 0005, "Redaction"; ADR 0014, which changes
//! what is found).
//!
//! The pass began as squint's (`crates/squint-core/src/artifacts.rs`, `redact`): a secret
//! is replaced where it stands as it is, and where it stands as a JSON string would
//! escape it. Here it works on bytes, whatever their encoding. It looks for each secret
//! as it is, in UTF-16 and inside base64, and it reads the text as percent-encoding and
//! as the escapes of a JSON string and looks again in what that reading gives. Every
//! place is found over what was given before anything is replaced, and each stretch is
//! replaced once. The search follows the text a fixed number of times, whatever the
//! number of secrets.

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

/// One form of one secret: its length in bytes; whether the character before it, or
/// after it, in base64 holds bits of the secret too; and whether it is UTF-16.
#[derive(Clone, Copy)]
struct Form {
    len: usize,
    before: bool,
    after: bool,
    wide: bool,
}

/// Every form of every secret, as one machine that follows a text once and says where
/// each form ends (the construction of Aho and Corasick). What following costs does not
/// depend on how many forms there are.
struct Search {
    /// From the start, by byte: the state to go to, or 0 to stay.
    start: Vec<u32>,
    /// From any other state, by `state << 8 | byte`.
    edges: HashMap<u64, u32>,
    /// Where to go on from when no edge leaves a state: the state of the longest
    /// proper end of what was read that some form begins with.
    fail: Vec<u32>,
    /// The form that ends exactly at a state, as its place in `forms` and one, or 0.
    ends: Vec<u32>,
    /// The nearest state along `fail` at which a form ends, or 0.
    shorter: Vec<u32>,
    forms: Vec<Form>,
    longest: usize,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'-' | b'_')
}

/// The forms a secret is searched for in, as bytes: as it is; in UTF-16 of either byte
/// order; and inside base64, in the standard alphabet and the one for URLs, for each of
/// the three places in a group of three bytes it can begin at.
fn forms_of(secret: &str) -> Vec<(Vec<u8>, Form)> {
    let form = |bytes: Vec<u8>, before: bool, after: bool, wide: bool| {
        let len = bytes.len();
        (
            bytes,
            Form {
                len,
                before,
                after,
                wide,
            },
        )
    };
    let mut forms = vec![form(secret.as_bytes().to_vec(), false, false, false)];
    let units = || secret.encode_utf16();
    forms.push(form(
        units().flat_map(u16::to_le_bytes).collect(),
        false,
        false,
        true,
    ));
    forms.push(form(
        units().flat_map(u16::to_be_bytes).collect(),
        false,
        false,
        true,
    ));
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
        forms.push(form(standard, begins != 0, bits % 6 != 0, false));
        forms.push(form(url, begins != 0, bits % 6 != 0, false));
    }
    forms
}

impl Search {
    fn new(secrets: &BTreeSet<String>) -> Search {
        let mut search = Search {
            start: vec![0; 256],
            edges: HashMap::new(),
            fail: vec![0],
            ends: vec![0],
            shorter: vec![0],
            forms: Vec::new(),
            longest: 1,
        };
        // The byte that leads to each state, the state it leads from, and how many
        // bytes from the start the state is.
        let mut led: Vec<(u32, u8, u32)> = vec![(0, 0, 0)];
        for (bytes, form) in secrets.iter().flat_map(|secret| forms_of(secret)) {
            let mut state = 0u32;
            for byte in &bytes {
                let next = search.edge(state, *byte).unwrap_or_else(|| {
                    let made = led.len() as u32;
                    led.push((state, *byte, led[state as usize].2 + 1));
                    search.fail.push(0);
                    search.ends.push(0);
                    search.shorter.push(0);
                    if state == 0 {
                        search.start[usize::from(*byte)] = made;
                    } else {
                        search
                            .edges
                            .insert(u64::from(state) << 8 | u64::from(*byte), made);
                    }
                    made
                });
                state = next;
            }
            search.longest = search.longest.max(form.len);
            match search.ends[state as usize] {
                0 => {
                    search.forms.push(form);
                    search.ends[state as usize] = search.forms.len() as u32;
                }
                // The same bytes twice: one form, with what either says of its edges.
                held => {
                    let held = &mut search.forms[held as usize - 1];
                    held.before |= form.before;
                    held.after |= form.after;
                    held.wide &= form.wide;
                }
            }
        }
        // Nearer states first, so that a state's `fail` is known before its children's.
        let mut order: Vec<u32> = (1..led.len() as u32).collect();
        order.sort_by_key(|state| led[*state as usize].2);
        for state in order {
            let (from, byte, _) = led[state as usize];
            let fail = if from == 0 {
                0
            } else {
                search.step(search.fail[from as usize], byte)
            };
            search.fail[state as usize] = fail;
            search.shorter[state as usize] = if search.ends[fail as usize] != 0 {
                fail
            } else {
                search.shorter[fail as usize]
            };
        }
        search
    }

    fn edge(&self, state: u32, byte: u8) -> Option<u32> {
        if state == 0 {
            Some(self.start[usize::from(byte)]).filter(|next| *next != 0)
        } else {
            self.edges
                .get(&(u64::from(state) << 8 | u64::from(byte)))
                .copied()
        }
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
    /// were given that a form covers.
    fn follow(
        &self,
        reading: impl Iterator<Item = Unit>,
        bytes: &[u8],
        found: &mut Vec<(usize, usize)>,
        wide: &mut Vec<(usize, usize)>,
    ) {
        // Where each of the last `longest` units began.
        let mut began = vec![0usize; self.longest];
        let mut state = 0;
        for (count, (byte, start, end)) in reading.enumerate() {
            began[count % self.longest] = start;
            state = self.step(state, byte);
            let mut at = if self.ends[state as usize] != 0 {
                state
            } else {
                self.shorter[state as usize]
            };
            while at != 0 {
                let form = self.forms[self.ends[at as usize] as usize - 1];
                let mut from = began[(count + 1 - form.len) % self.longest];
                let mut to = end;
                if form.before && from > 0 && is_base64(bytes[from - 1]) {
                    from -= 1;
                }
                if form.after && to < bytes.len() && is_base64(bytes[to]) {
                    to += 1;
                }
                if form.wide {
                    wide.push((from, to));
                } else {
                    found.push((from, to));
                }
                at = self.shorter[at as usize];
            }
        }
    }
}

/// A text read as percent-encoding and as the escapes of a JSON string: `%` and two
/// hex digits are one byte; a backslash and `"`, `\`, `/`, `b`, `f`, `n`, `r` or `t`
/// are one character; `\u` and four hex digits are one character, or half of one with
/// the `\u` that follows. With `plus`, a `+` is a space. Anything else is as it is,
/// and so is what only looks like one of these.
struct Read<I> {
    inner: I,
    plus: bool,
    /// Up to twelve units ahead: the longest thing read as one is two `\u` escapes.
    ahead: VecDeque<Unit>,
    /// The rest of a character of several bytes, each from the same place.
    rest: VecDeque<Unit>,
}

impl<I: Iterator<Item = Unit>> Read<I> {
    fn new(inner: I, plus: bool) -> Read<I> {
        Read {
            inner,
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
            b'%' => {
                let value = self.hex(1)? * 16 + self.hex(2)?;
                Some((3, Err(u8::try_from(value).ok()?)))
            }
            b'+' if self.plus => Some((1, Err(b' '))),
            b'\\' => match self.byte(1)? {
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
    /// in the middle of a character is not, are searched like any others.
    ///
    /// The bytes are followed as they are; then, where they hold a `%` or a backslash,
    /// as percent-encoding and JSON escapes read them, and as that reading read once
    /// more; then, where they hold a `+`, with each `+` a space. Every stretch found by
    /// any reading is found over the bytes as they were given, before anything is
    /// replaced; stretches that overlap are one stretch; each is replaced once. So two
    /// secrets that overlap leave no tail of either, and a secret that is part of the
    /// marker does not rewrite the marker.
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
        let (mut found, mut wide) = (Vec::new(), Vec::new());
        search.follow(plain(), bytes, &mut found, &mut wide);
        if bytes.contains(&b'%') || bytes.contains(&b'\\') {
            search.follow(Read::new(plain(), false), bytes, &mut found, &mut wide);
            let twice = Read::new(Read::new(plain(), false), false);
            search.follow(twice, bytes, &mut found, &mut wide);
        }
        if bytes.contains(&b'+') {
            search.follow(Read::new(plain(), true), bytes, &mut found, &mut wide);
        }
        // A secret of ASCII letters in UTF-16 of one byte order is also, one byte to
        // the side, the same secret in the other order, with a zero byte of the letter
        // beside it. Of two such stretches the one that begins at an even place is the
        // secret, since UTF-16 is in pairs of bytes; the other would take a byte of a
        // neighbour with it.
        let begins: BTreeSet<usize> = wide.iter().map(|stretch| stretch.0).collect();
        found.extend(wide.iter().filter(|(from, _)| {
            from % 2 == 0 || !(begins.contains(&(from - 1)) || begins.contains(&(from + 1)))
        }));
        if found.is_empty() {
            return bytes.to_vec();
        }
        found.sort_unstable();
        let mut out = Vec::with_capacity(bytes.len());
        let mut copied_to = 0;
        // The stretch being put together, until one begins at or past its end.
        let mut stretch = found[0];
        for (from, to) in found.into_iter().skip(1).chain([(usize::MAX, usize::MAX)]) {
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
    pub fn redact(&self, text: &str) -> String {
        // A form found in a text is nearly always between two characters. Where one is
        // not, as a secret's UTF-16 bytes can stand inside a longer character, what is
        // left of that character is no text and is written as the mark for that.
        String::from_utf8_lossy(&self.redact_bytes(text.as_bytes())).into_owned()
    }
}
