//! Egress (`SPEC.md` section 10, "Egress"; ADR 0012): where data may go.
//!
//! This is the ways out, and no more: who a recipient is, the four classes of data, and
//! the three interfaces through which anything leaves the machine, for data, for a
//! model's file and for a probe. **Nothing here refuses a request for where it goes.**
//! The checks that deny by default are built on these types by the egress strand of
//! checkpoint 1B (`docs/plans/`).
//!
//! Every type has private fields and is made through a constructor that checks it. None
//! is deserialised. A constructor that takes a text from outside the build is told the
//! executor's secrets, and looks at the whole text for one before it parses, cuts or
//! bounds it: a text that holds a secret is refused whole and nothing of it is shown.

mod classes;
mod recipient;
mod ways;

pub use classes::{Classes, DataClass};
pub use recipient::{
    ACCOUNT_LIMIT, ADDRESS_MAX_BYTES, Account, Address, Credential, Origin, Recipient,
    UPSTREAM_LIMIT,
};
pub use ways::{
    DownloadRequest, Downloaded, Downloader, Outgoing, PIECES_LIMIT, Piece, ProbeRequest, Probed,
    Prober, REPLY_MAX_BYTES, Reply, Sender,
};

/// Why something that names a way out could not be made. Each is a kind and a sentence
/// of fixed words: the text that was refused is never quoted, whether or not it held a
/// secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotAllowed {
    /// The text holds a secret the constructor was told of. It is refused whole,
    /// whatever else is wrong with it.
    Secret,
    /// The text, or a body, is over its bound.
    TooLong,
    /// The text holds a character an address does not: one outside printable ASCII,
    /// a space or a backslash.
    Character,
    /// The address does not start with `http://` or `https://`.
    Scheme,
    /// The address holds an `@`, which is how a user and a password are written.
    UserInfo,
    /// The address has a query, where none is allowed.
    Query,
    /// The address has a fragment.
    Fragment,
    /// The host is not a name of letters, digits, hyphens and dots.
    Host,
    /// The port is not a number from 1 to 65,535 written plainly.
    Port,
    /// A name, an account's or a header's, is not one.
    Name,
    /// More of something than may be held.
    TooMany,
    /// An origin is named twice, or is the recipient's own.
    Twice,
    /// A credential is bound to another origin than the request's address.
    Elsewhere,
    /// Nothing where something is required.
    Empty,
}

impl std::fmt::Display for NotAllowed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NotAllowed::Secret => "the text holds a secret, and is not shown",
            NotAllowed::TooLong => "the text or the body is over its bound",
            NotAllowed::Character => {
                "the text holds a character that is not printable ASCII, or a space or a backslash"
            }
            NotAllowed::Scheme => {
                "the address does not start with http or https, a colon and two slashes"
            }
            NotAllowed::UserInfo => {
                "the address holds an at sign, as a user and a password are written"
            }
            NotAllowed::Query => "the address has a query, and may have none",
            NotAllowed::Fragment => "the address has a fragment, and may have none",
            NotAllowed::Host => {
                "the host is not a name of letters, digits and hyphens in labels of 1 to 63, \
                 253 characters at most"
            }
            NotAllowed::Port => "the port is not a number from 1 to 65535 written plainly",
            NotAllowed::Name => {
                "the name is not 1 to its limit of printable ASCII characters with no space, \
                 no double quote and no backslash"
            }
            NotAllowed::TooMany => "there are more than may be held",
            NotAllowed::Twice => "an origin is named twice, or is the recipient's own",
            NotAllowed::Elsewhere => {
                "the credential is bound to another origin than the one the request goes to"
            }
            NotAllowed::Empty => "nothing is there, where something is required",
        })
    }
}

impl std::error::Error for NotAllowed {}
