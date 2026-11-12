//! Redaction: secrets are taken out of everything before it is written (`SPEC.md`
//! section 3, "What every run records"; ADR 0005, "Redaction").
//!
//! The pass began as squint's (`crates/squint-core/src/artifacts.rs`, `redact`): a secret
//! is replaced where it stands as it is, and where it stands as a JSON string would
//! escape it. Here it works on bytes, whatever their encoding, finds every form of every
//! secret over what it was given before it replaces anything, and replaces each stretch
//! once.

use std::collections::BTreeSet;

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
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secrets {
    /// How many secrets there are.
    count: usize,
    /// Every form of every secret, as bytes: what is searched for.
    forms: Vec<Vec<u8>>,
    /// For each byte, the forms that start with it, by their place in `forms`.
    starting: Vec<Vec<usize>>,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secrets({})", self.count)
    }
}

/// The forms a secret is searched for in: as it is; as a JSON string escapes it; as a
/// JSON string escapes it with each `/` written `\/`; and percent-encoded, as it would
/// stand in a URL, with the hex digits in either case.
fn forms_of(secret: &str) -> BTreeSet<Vec<u8>> {
    let mut forms = BTreeSet::new();
    forms.insert(secret.as_bytes().to_vec());
    // Writing a string as JSON cannot fail.
    let quoted = serde_json::to_string(secret).unwrap_or_default();
    // Exactly one quote from each end: a quote that is the secret's own is escaped, and
    // is part of the form.
    let escaped = quoted
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_default();
    if !escaped.is_empty() {
        forms.insert(escaped.as_bytes().to_vec());
        forms.insert(escaped.replace('/', "\\/").into_bytes());
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
        forms.insert(encoded);
    }
    forms
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
        let forms: Vec<Vec<u8>> = secrets
            .iter()
            .flat_map(|secret| forms_of(secret))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if forms.is_empty() {
            return Ok(Secrets::none());
        }
        let mut starting = vec![Vec::new(); 256];
        for (index, form) in forms.iter().enumerate() {
            if let Some(first) = form.first() {
                starting[usize::from(*first)].push(index);
            }
        }
        Ok(Secrets {
            count: secrets.len(),
            forms,
            starting,
        })
    }

    /// The bytes with every stretch that holds a secret, in any of its forms, replaced by
    /// [`REDACTED`]. The encoding does not matter: bytes that are not UTF-8, as a body cut
    /// in the middle of a character is not, are searched like any others.
    ///
    /// Every form of every secret is found over the bytes as they were given, before
    /// anything is replaced; stretches that overlap are one stretch; each is replaced
    /// once. So two secrets that overlap leave no tail of either, and a secret that is
    /// part of the marker does not rewrite the marker. It is one pass over the bytes: at
    /// each place, only the forms that start with the byte there are compared.
    pub fn redact_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        if self.forms.is_empty() {
            return bytes.to_vec();
        }
        let mut out = Vec::with_capacity(bytes.len());
        // Where the stretch being replaced ends, when the place is inside one.
        let mut covered_to = 0;
        let mut copied_to = 0;
        for at in 0..bytes.len() {
            let mut end = 0;
            for index in &self.starting[usize::from(bytes[at])] {
                let form = &self.forms[*index];
                if bytes[at..].starts_with(form) {
                    end = end.max(at + form.len());
                }
            }
            if end == 0 {
                continue;
            }
            if at >= covered_to {
                // A new stretch: what lay between the last one and here is kept.
                out.extend_from_slice(&bytes[copied_to..at]);
                out.extend_from_slice(REDACTED.as_bytes());
            }
            covered_to = covered_to.max(end);
            copied_to = covered_to;
        }
        out.extend_from_slice(&bytes[copied_to..]);
        out
    }

    /// The text with every secret replaced, as [`Secrets::redact_bytes`] replaces it.
    pub fn redact(&self, text: &str) -> String {
        // A form is UTF-8, so in a text it can only be found between two characters, and
        // what is left is a text.
        String::from_utf8_lossy(&self.redact_bytes(text.as_bytes())).into_owned()
    }
}
