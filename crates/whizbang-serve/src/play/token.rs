//! The launch token (ADR 0041, "The token"): 32 bytes from the system's randomness, made
//! when the host starts, handed to the page once, and compared in constant time. Its
//! `Debug` and `Display` print nothing of it, so no log, event, record or error that
//! formats one can hold it.

use std::fmt;
use std::io;

/// How many bytes a token is.
pub const TOKEN_BYTES: usize = 32;

/// A launch token. Only [`Token::hex`] gives its text, for the page's address and for the
/// owner-only file of a running host.
#[derive(Clone)]
pub struct Token([u8; TOKEN_BYTES]);

impl Token {
    /// A new token, from the system's randomness; or why none could be read.
    pub fn new() -> io::Result<Token> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::getrandom(&mut bytes)
            .map_err(|_| io::Error::other("the system's randomness could not be read"))?;
        Ok(Token(bytes))
    }

    /// The token of these bytes: a test's, and the one a running host's file holds.
    pub fn from_bytes(bytes: [u8; TOKEN_BYTES]) -> Token {
        Token(bytes)
    }

    /// The token as 64 lower-case hex digits, as the page sends it back.
    pub fn hex(&self) -> String {
        hex(&self.0)
    }

    /// The token these 64 hex digits are, or none.
    pub fn parse(text: &str) -> Option<Token> {
        let text = text.as_bytes();
        if text.len() != TOKEN_BYTES * 2 {
            return None;
        }
        let mut bytes = [0u8; TOKEN_BYTES];
        for (at, pair) in text.chunks(2).enumerate() {
            let high = digit(pair[0])?;
            let low = digit(pair[1])?;
            bytes[at] = high << 4 | low;
        }
        Some(Token(bytes))
    }

    /// Whether `presented` is this token's text. The time it takes does not depend on
    /// where the two first differ, nor on what was presented beyond its length.
    pub fn matches(&self, presented: &[u8]) -> bool {
        let expected = self.hex();
        let expected = expected.as_bytes();
        let mut differ = u8::from(presented.len() != expected.len());
        for (at, byte) in expected.iter().enumerate() {
            let other = presented.get(at).copied().unwrap_or(0);
            differ |= byte ^ other;
        }
        differ == 0
    }
}

/// A short id of random bytes, as hex: a preparation's handle.
pub(crate) fn random_hex(bytes: usize) -> io::Result<String> {
    let mut made = vec![0u8; bytes];
    getrandom::getrandom(&mut made)
        .map_err(|_| io::Error::other("the system's randomness could not be read"))?;
    Ok(hex(&made))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    out
}

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a launch token")
    }
}
