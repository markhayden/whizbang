//! Who receives data, and where a request goes (`SPEC.md` section 10, "Egress"; ADR
//! 0012): an origin, an account, a recipient, an address and a credential.
//!
//! Each constructor that takes a text looks at the whole text for a secret first, and
//! refuses one that holds a secret without showing it. No refusal quotes the text.

use std::fmt;

use super::NotAllowed;
use crate::contract::Secrets;

/// The most bytes of an address.
pub const ADDRESS_MAX_BYTES: usize = 2048;

/// The most characters of an account.
pub const ACCOUNT_LIMIT: usize = 128;

/// The most upstreams a recipient may forward to.
pub const UPSTREAM_LIMIT: usize = 16;

/// The most characters of a host's name, and of one label of it.
const HOST_LIMIT: usize = 253;
const LABEL_LIMIT: usize = 63;

/// How many characters of an account are shown.
const ACCOUNT_SHOWN: usize = 4;

/// The most characters of a header's name, and bytes of a credential's value.
const HEADER_LIMIT: usize = 64;
const CREDENTIAL_MAX_BYTES: usize = 4096;

/// Whether a text holds a secret the executor was told of, in any form redaction
/// finds.
fn holds_secret(text: &str, secrets: &Secrets) -> bool {
    secrets.redact(text) != text
}

/// An address, taken apart: its origin, and the rest of it as it was given.
fn parse(text: &str, secrets: &Secrets, query: bool) -> Result<(Origin, String), NotAllowed> {
    // The whole text, before anything is cut from it or judged of it.
    if holds_secret(text, secrets) {
        return Err(NotAllowed::Secret);
    }
    if text.len() > ADDRESS_MAX_BYTES {
        return Err(NotAllowed::TooLong);
    }
    if !text.bytes().all(|b| b.is_ascii_graphic() && b != b'\\') {
        return Err(NotAllowed::Character);
    }
    // One rule that cannot be parsed around: no `@` anywhere, and no fragment.
    if text.contains('@') {
        return Err(NotAllowed::UserInfo);
    }
    if text.contains('#') {
        return Err(NotAllowed::Fragment);
    }
    if !query && text.contains('?') {
        return Err(NotAllowed::Query);
    }
    let (scheme, after) = text.split_once("://").ok_or(NotAllowed::Scheme)?;
    let (scheme, implied) = match scheme.to_ascii_lowercase().as_str() {
        "http" => ("http", 80),
        "https" => ("https", 443),
        _ => return Err(NotAllowed::Scheme),
    };
    // The host and the port end at the first `/`, and nowhere else: a query follows a
    // path, and one written straight after the host is no part of a host or a port.
    let end = after.find('/').unwrap_or(after.len());
    let (authority, rest) = after.split_at(end);
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    let host = host.to_ascii_lowercase();
    let label = |label: &str| {
        (1..=LABEL_LIMIT).contains(&label.len())
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    if host.len() > HOST_LIMIT || !host.split('.').all(label) || !plainly_numbered(&host) {
        return Err(NotAllowed::Host);
    }
    let port = match port {
        None => implied,
        Some(digits) => {
            let plain = !digits.is_empty()
                && digits.len() <= 5
                && digits.bytes().all(|b| b.is_ascii_digit())
                && !digits.starts_with('0');
            let port = if plain {
                digits.parse::<u16>().ok()
            } else {
                None
            };
            port.ok_or(NotAllowed::Port)?
        }
    };
    let origin = Origin { scheme, host, port };
    // The host was lowered, so the origin is not the text that was looked at: a
    // secret that the lowering made is in what would be printed and compared. It is
    // looked for again, in the origin as it is written.
    if holds_secret(&origin.to_string(), secrets) {
        return Err(NotAllowed::Secret);
    }
    Ok((origin, rest.to_owned()))
}

/// Whether a host that ends in a number is one machine written the one way. A client
/// reads a host whose last label is a number as an address in numbers, in whatever of
/// many spellings it is given: `127.1`, `0x7f.1`, `2130706433` and `017700000001` are
/// one machine, and `00000127.0.0.1` is another than it looks. So a host whose last
/// label is a number (digits alone, or `0x` and hex digits or none) is taken only when
/// the whole host is four decimal parts of 0 to 255, none with a leading zero. A host
/// whose last label is no number is a name, and is as it is.
fn plainly_numbered(host: &str) -> bool {
    let last = host.rsplit('.').next().unwrap_or(host);
    let hex = last.strip_prefix("0x");
    let number = last.bytes().all(|b| b.is_ascii_digit())
        || hex.is_some_and(|digits| digits.bytes().all(|b| b.is_ascii_hexdigit()));
    if !number {
        return true;
    }
    let part = |part: &str| {
        let plain = part == "0" || !part.starts_with('0');
        plain && part.len() <= 3 && part.parse::<u8>().is_ok()
    };
    host.split('.').count() == 4 && host.split('.').all(part)
}

/// A scheme, a host and a port: what two addresses share when a request to one may
/// carry what a request to the other carried. What is printed of one is
/// `scheme://host`, with the port when it is not the scheme's own.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    scheme: &'static str,
    host: String,
    port: u16,
}

impl Origin {
    /// The origin of an address. The address is at most [`ADDRESS_MAX_BYTES`] of
    /// printable ASCII with no space and no backslash; its scheme is `http` or
    /// `https`; its host is a name of letters, digits and hyphens in labels of 1 to
    /// 63, 253 characters at most, kept in lower case; its port is 1 to 65,535,
    /// written plainly, or the scheme's own when none is written. A path may follow
    /// and is no part of the origin. A host whose last label is a number is taken
    /// only as four decimal parts of 0 to 255 written plainly. Refused: an `@`
    /// anywhere, a query, a fragment, and a text that holds a secret, which is looked
    /// for first in the text as given and again in the origin as it is written.
    pub fn of(address: &str, secrets: &Secrets) -> Result<Origin, NotAllowed> {
        parse(address, secrets, false).map(|(origin, _)| origin)
    }

    /// `http` or `https`.
    pub fn scheme(&self) -> &str {
        self.scheme
    }

    /// The host, in lower case.
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    fn implied_port(&self) -> u16 {
        if self.scheme == "http" { 80 } else { 443 }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme, self.host)?;
        if self.port != self.implied_port() {
            write!(f, ":{}", self.port)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Origin({self})")
    }
}

/// Where one request goes: an origin, and the rest of the address, its path and its
/// query, as it was given. What `{:?}` prints is the origin and how long the rest is,
/// never the rest: a signed address keeps its signature there.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Address {
    origin: Origin,
    rest: String,
}

impl Address {
    /// The address, by the rules of [`Origin::of`], but that it may have a query after
    /// its path: one straight after the host is refused. The rest is kept as it was
    /// given and is not normalised: only origins are compared.
    pub fn of(text: &str, secrets: &Secrets) -> Result<Address, NotAllowed> {
        parse(text, secrets, true).map(|(origin, rest)| Address { origin, rest })
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The path and the query, as they were given: for whoever makes the request.
    pub fn rest(&self) -> &str {
        &self.rest
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Address({}, {} more bytes)",
            self.origin,
            self.rest.len()
        )
    }
}

/// The account used with an endpoint. It is compared whole and shown cut: what `{:?}`
/// prints, and what a record holds, is [`Account::shown`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Account {
    name: String,
}

impl Account {
    /// An account: 1 to [`ACCOUNT_LIMIT`] printable ASCII characters with no space, no
    /// `"` and no `\`. A text that holds a secret is refused first, whole.
    pub fn new(text: &str, secrets: &Secrets) -> Result<Account, NotAllowed> {
        if holds_secret(text, secrets) {
            return Err(NotAllowed::Secret);
        }
        let plain = |b: u8| b.is_ascii_graphic() && b != b'"' && b != b'\\';
        if text.is_empty() || text.len() > ACCOUNT_LIMIT || !text.bytes().all(plain) {
            return Err(NotAllowed::Name);
        }
        Ok(Account {
            name: text.to_owned(),
        })
    }

    /// The account as it is shown: its first four characters, and `...` when there are
    /// more.
    pub fn shown(&self) -> String {
        match self.name.get(..ACCOUNT_SHOWN) {
            Some(start) if self.name.len() > ACCOUNT_SHOWN => format!("{start}..."),
            _ => self.name.clone(),
        }
    }
}

impl Account {
    /// The account as it is shown ([`Account::shown`]), for something that is written
    /// and kept: none when the whole account holds a secret, which may have come to be
    /// known after the account was made. A cut account is too short to hold a secret
    /// itself, so it is the whole that is looked at.
    pub fn shown_without(&self, secrets: &Secrets) -> Option<String> {
        (!holds_secret(&self.name, secrets)).then(|| self.shown())
    }
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Account({})", self.shown())
    }
}

/// Who receives data: an endpoint's origin, the account used with it, and the
/// upstreams it may forward to. Two recipients are equal when all three are. What
/// `{:?}` prints is the origin and the cut account, and nothing else.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Recipient {
    origin: Origin,
    account: Option<Account>,
    upstreams: Vec<Origin>,
}

impl Recipient {
    /// The recipient at this origin, with no account and no upstream.
    pub fn new(origin: Origin) -> Recipient {
        Recipient {
            origin,
            account: None,
            upstreams: Vec::new(),
        }
    }

    /// The recipient at the origin of this address ([`Origin::of`]).
    pub fn at(address: &str, secrets: &Secrets) -> Result<Recipient, NotAllowed> {
        Origin::of(address, secrets).map(Recipient::new)
    }

    /// The same recipient, under this account.
    pub fn with_account(mut self, account: Account) -> Recipient {
        self.account = Some(account);
        self
    }

    /// The same recipient, which may forward to these origins and no others: at most
    /// [`UPSTREAM_LIMIT`], each named once, and none the recipient's own.
    pub fn forwarding_to(mut self, upstreams: Vec<Origin>) -> Result<Recipient, NotAllowed> {
        if upstreams.len() > UPSTREAM_LIMIT {
            return Err(NotAllowed::TooMany);
        }
        for (at, upstream) in upstreams.iter().enumerate() {
            if *upstream == self.origin || upstreams[..at].contains(upstream) {
                return Err(NotAllowed::Twice);
            }
        }
        // Kept in one order, so that a recipient is the same recipient in whatever
        // order its upstreams were given.
        let mut upstreams = upstreams;
        upstreams.sort_by_key(Origin::to_string);
        self.upstreams = upstreams;
        Ok(self)
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }

    /// The origins it may forward to, in the order of their text.
    pub fn upstreams(&self) -> &[Origin] {
        &self.upstreams
    }

    /// Whether an origin is the recipient's own or one of its upstreams: what a hop is
    /// checked with.
    pub fn reaches(&self, origin: &Origin) -> bool {
        self.origin == *origin || self.upstreams.contains(origin)
    }
}

impl fmt::Debug for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Recipient({}", self.origin)?;
        if let Some(account) = &self.account {
            write!(f, " as {}", account.shown())?;
        }
        write!(f, ")")
    }
}

/// A credential: a header's name, its value, and the one origin it is bound to. A
/// request takes a credential only for the origin of its own address. What `{:?}`
/// prints is the header's name and the origin, never the value.
#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    origin: Origin,
    header: String,
    value: String,
}

impl Credential {
    /// A credential for this origin, carried in this header. The header's name is 1
    /// to 64 ASCII letters, digits, `-` and `_`, and holds no secret the constructor
    /// was told of; the value is 1 to 4,096 bytes of printable ASCII and spaces, and
    /// is the secret: it is not looked at for one, and is never printed.
    pub fn new(
        origin: Origin,
        header: &str,
        value: &str,
        secrets: &Secrets,
    ) -> Result<Credential, NotAllowed> {
        // The header's name is printed and the value never is. A key written where the
        // name belongs is refused whole, before the name is judged, and not shown.
        if holds_secret(header, secrets) {
            return Err(NotAllowed::Secret);
        }
        let named = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
        if header.is_empty() || header.len() > HEADER_LIMIT || !header.bytes().all(named) {
            return Err(NotAllowed::Name);
        }
        if value.is_empty() {
            return Err(NotAllowed::Empty);
        }
        if value.len() > CREDENTIAL_MAX_BYTES {
            return Err(NotAllowed::TooLong);
        }
        if !value.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
            return Err(NotAllowed::Character);
        }
        Ok(Credential {
            origin,
            header: header.to_owned(),
            value: value.to_owned(),
        })
    }

    /// The one origin the credential may go to.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The name of the header that carries it.
    pub fn header(&self) -> &str {
        &self.header
    }

    /// The value itself, for the one header that carries it.
    pub fn expose(&self) -> &str {
        &self.value
    }

    /// The credential, when it is bound to the origin of this address.
    pub(super) fn bound_to(self, address: &Address) -> Result<Credential, NotAllowed> {
        if self.origin == address.origin {
            Ok(self)
        } else {
            Err(NotAllowed::Elsewhere)
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Credential({} for {})", self.header, self.origin)
    }
}
