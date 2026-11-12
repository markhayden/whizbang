//! Redaction: secrets are taken out of every text before it is written (`SPEC.md`
//! section 3, "What every run records").
//!
//! The pass is squint's (`crates/squint-core/src/artifacts.rs`, `redact`): every secret
//! is replaced where it stands as it is, and where it stands as a JSON string would
//! escape it, so a secret quoted inside a JSON body is found too.

/// What stands in a stored text where a secret was.
pub const REDACTED: &str = "[redacted]";

/// The secrets of one executor: the values that must never be written. They are held to
/// be compared against, never printed: the type shows how many it holds and nothing else.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secrets(Vec<String>);

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secrets({})", self.0.len())
    }
}

impl Secrets {
    /// No secret: nothing is replaced.
    pub fn none() -> Secrets {
        Secrets::default()
    }

    /// These values. An empty one is left out: it would match everywhere. They are kept
    /// longest first, so that a secret that holds another is replaced whole.
    pub fn new(values: impl IntoIterator<Item = String>) -> Secrets {
        let mut values: Vec<String> = values.into_iter().filter(|v| !v.is_empty()).collect();
        values.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        values.dedup();
        Secrets(values)
    }

    /// The text with every secret replaced by [`REDACTED`], in its own form and in the
    /// form a JSON string gives it.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for secret in &self.0 {
            out = out.replace(secret.as_str(), REDACTED);
            let escaped = serde_json::to_string(secret).unwrap_or_default();
            let escaped = escaped.trim_matches('"');
            if escaped != secret && !escaped.is_empty() {
                out = out.replace(escaped, REDACTED);
            }
        }
        out
    }

    /// Bytes to be stored as a diagnostic: redacted when they are text, and as they are
    /// when they are not UTF-8, as an image is.
    pub fn redact_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        match std::str::from_utf8(bytes) {
            Ok(text) => self.redact(text).into_bytes(),
            Err(_) => bytes.to_vec(),
        }
    }
}
