//! JSON read so that a number keeps the text it was written with (docs/baseline/transcript.md,
//! "Numbers keep their text").
//!
//! `serde_json` keeps a number's text only under its feature `arbitrary_precision`, and a
//! feature one crate turns on reaches every crate of a build: the tests would then build a
//! `serde_json` that the binary does not ship (docs/adr/0023-one-serde-json.md). So this
//! reader is the comparison's own. It reads what `serde_json` reads and refuses what it
//! refuses, to the same depth, and a text is decoded by `serde_json` itself. A number is
//! kept as written, but for how its exponent is marked: `E` is written `e`, and a `+` is
//! written where the exponent has no sign, as `serde_json` did under that feature.

use std::collections::BTreeMap;

/// A JSON value whose numbers are their text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(String),
    Text(String),
    List(Vec<Json>),
    /// Keys in order, as `serde_json` keeps them; of a key given twice, the last value.
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.get(key),
            _ => None,
        }
    }

    pub(crate) fn as_list(&self) -> Option<&Vec<Json>> {
        match self {
            Json::List(items) => Some(items),
            _ => None,
        }
    }

    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Json::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// How deep lists and objects may nest: `serde_json`'s own limit, so that what one reads the
/// other does.
const DEPTH: usize = 128;

/// The value the text holds, or none when the text is not one JSON value.
pub(crate) fn parse(text: &str) -> Option<Json> {
    let mut reader = Reader {
        text,
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(DEPTH)?;
    reader.space();
    (reader.at == reader.bytes.len()).then_some(value)
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.at += 1;
        }
        found
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &[u8], value: Json) -> Option<Json> {
        let found = self.bytes[self.at..].starts_with(word);
        if found {
            self.at += word.len();
        }
        found.then_some(value)
    }

    /// One value; `depth` is how many more levels of nesting may be entered, as
    /// `serde_json` counts them: one is refused when it would leave none.
    fn value(&mut self, depth: usize) -> Option<Json> {
        self.space();
        match self.peek()? {
            b'n' => self.word(b"null", Json::Null),
            b't' => self.word(b"true", Json::Bool(true)),
            b'f' => self.word(b"false", Json::Bool(false)),
            b'"' => self.text().map(Json::Text),
            b'[' => {
                let depth = depth.checked_sub(1).filter(|left| *left > 0)?;
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.eat(b']') {
                    return Some(Json::List(items));
                }
                loop {
                    items.push(self.value(depth)?);
                    self.space();
                    if self.eat(b']') {
                        return Some(Json::List(items));
                    }
                    if !self.eat(b',') {
                        return None;
                    }
                }
            }
            b'{' => {
                let depth = depth.checked_sub(1).filter(|left| *left > 0)?;
                self.at += 1;
                let mut fields = BTreeMap::new();
                self.space();
                if self.eat(b'}') {
                    return Some(Json::Object(fields));
                }
                loop {
                    self.space();
                    if self.peek() != Some(b'"') {
                        return None;
                    }
                    let key = self.text()?;
                    self.space();
                    if !self.eat(b':') {
                        return None;
                    }
                    let value = self.value(depth)?;
                    fields.insert(key, value);
                    self.space();
                    if self.eat(b'}') {
                        return Some(Json::Object(fields));
                    }
                    if !self.eat(b',') {
                        return None;
                    }
                }
            }
            _ => self.number().map(Json::Number),
        }
    }

    /// A text, from its opening quote to the quote that closes it, decoded by `serde_json`,
    /// which refuses a bad escape, a lone surrogate and a control character.
    fn text(&mut self) -> Option<String> {
        let start = self.at;
        if !self.eat(b'"') {
            return None;
        }
        loop {
            match self.peek()? {
                b'"' => break,
                b'\\' => self.at += 2,
                _ => self.at += 1,
            }
        }
        self.at += 1;
        // Both ends are at a quote, which is one byte, so the slice falls between characters.
        serde_json::from_str(self.text.get(start..self.at)?).ok()
    }

    /// A number as JSON's grammar has it, kept as it was written but for its exponent's mark.
    fn number(&mut self) -> Option<String> {
        let mut kept = String::new();
        if self.eat(b'-') {
            kept.push('-');
        }
        match self.peek()? {
            b'0' => {
                self.at += 1;
                kept.push('0');
            }
            b'1'..=b'9' => self.digits(&mut kept),
            _ => return None,
        }
        if self.eat(b'.') {
            kept.push('.');
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return None;
            }
            self.digits(&mut kept);
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            kept.push('e');
            match self.peek() {
                Some(b'-') => {
                    self.at += 1;
                    kept.push('-');
                }
                Some(b'+') => {
                    self.at += 1;
                    kept.push('+');
                }
                _ => kept.push('+'),
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return None;
            }
            self.digits(&mut kept);
        }
        Some(kept)
    }

    fn digits(&mut self, kept: &mut String) {
        while let Some(digit @ b'0'..=b'9') = self.peek() {
            self.at += 1;
            kept.push(char::from(digit));
        }
    }
}
