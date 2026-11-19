//! The filler of fal (plan m3, task F1; `SPEC.md` section 5, "fill, paid service"): Bria
//! Eraser, one `POST` to `https://fal.run/fal-ai/bria/eraser`, read synchronously. A
//! table of it is the kind `service` under `[backends.fill]`, with `provider = "fal"`,
//! `model = "bria-eraser"` and a `key_env`; it takes no setting and names no `url`.
//!
//! **What is sent** ([`wire`]): the picture and its support, each as the PNG the core's
//! one encoder for sending writes (`Piece::picture`), in a data address of the fields
//! `image_url` and `mask_url`, and `sync_mode = true`, so the answer carries the filled
//! picture as a data address and nothing is fetched from another host of fal's. The
//! picture is of the class `source_images` and the support, a binary mask, white where
//! the model is to paint, of `derived_images`: no other class is sent. A picture larger
//! than [`LONG_SIDE`] is scaled inside the backend to fit it, the support with it, and
//! the answer is scaled back to the request's size.
//!
//! **What is read:** an answer of 200 whose `image.url` is a data address of a picture of
//! the size sent. An answer that holds any address of a host (any text with `://` in it)
//! is a fault, and nothing is downloaded: the backend has no way to fetch.
//!
//! **The key** goes in the header `Authorization`, as `Key` and the key, the form fal's
//! reference gives, bound to the origin `https://fal.run` (ADR 0012); the backend is never
//! handed it.
//!
//! **What it costs** ([`spend`]): a price for each call, fixed, so the bound of a request
//! is that price and a call is settled at it.

pub mod spend;
pub(crate) mod wire;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use whizbang_core::capability::{
    Bounds, Charge, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Price,
};
use whizbang_core::config::{KindRefused, Table};
use whizbang_core::contract::{Made, ModelIdentity, Secrets};
use whizbang_core::egress::{
    Address, Credential, MOVES_LIMIT, NotAllowed, Outgoing, Recipient, Sender,
};

use crate::service::{
    BUSY, CREDENTIAL, MOVED_TOO_OFTEN, OTHER_SUCCESS, RATE_LIMITED, REFUSED, STATUS,
};

/// The provider's name, as a table gives it.
pub const PROVIDER: &str = "fal";

/// The one model this build has of it.
pub const MODEL: &str = "bria-eraser";

/// The adapter's name: the backend a model identity names.
pub const ADAPTER: &str = "fal";

/// The version of the adapter.
pub const VERSION: &str = "1";

/// The address the kind sends to: the synchronous run of the model, read from fal's
/// reference of its interface on 2026-11-18 (`https://fal.run/{owner}/{app}`).
pub const ADDRESS: &str = "https://fal.run/fal-ai/bria/eraser";

/// The header the key is carried in, as `Key` and the key.
pub const KEY_HEADER: &str = "authorization";

/// The longest side a picture is sent at. Fal's page of the model states none (read
/// 2026-11-18); 2,048 is the side rascal's hosted example sent.
pub const LONG_SIDE: u32 = 2048;

/// The preparation of what is sent and its version: fitted to [`LONG_SIDE`], the
/// picture as the stored PNG and the support as a binary mask of the same.
pub const PREPARATION: &str = "fit1-png-binary1";

/// The fields of a request, each a constant: the type of its body.
pub const FIELDS: &[(&str, &str)] = &[("content-type", "application/json")];

const NOT_FILL: &str = "a service backend of fal is a filler, and is offered under backends.fill";
const A_URL: &str = "a service backend names no url: its provider's address is the kind's own";
const NO_KEY: &str = "a service backend names its key with key_env, and no token_env";
const NOT_MODEL: &str = "a service backend of fal names the model bria-eraser";
const NO_SETTINGS: &str = "a service backend of fal takes no setting";
const NOT_DECLARED: &str = "a service backend cannot declare that model";
/// What a fault says of a request that could not be written.
pub const NOT_WRITTEN: &str = "the request could not be written to be sent";

/// The version a backend's model identity names: the adapter's, and the preparation of
/// what it sends with the side it fits to, so a run's record names both.
pub fn identity_version() -> String {
    format!("{VERSION}+{PREPARATION}-{LONG_SIDE}")
}

/// A table of the kind under `[backends.fill]`, read: the address it sends to, or the
/// table's fault.
fn read(
    table: &Table<'_>,
    address: Option<&str>,
    secrets: &Secrets,
) -> Result<Address, KindRefused> {
    if table.capability() != whizbang_core::contract::Capability::Fill {
        return Err(KindRefused::Table(NOT_FILL));
    }
    let backend = table.backend();
    if backend.address().is_some() {
        return Err(KindRefused::Table(A_URL));
    }
    if table.key().is_none() || table.token().is_some() {
        return Err(KindRefused::Table(NO_KEY));
    }
    if backend.model() != Some(MODEL) {
        return Err(KindRefused::Table(NOT_MODEL));
    }
    if backend.settings().next().is_some() {
        return Err(KindRefused::Table(NO_SETTINGS));
    }
    Address::of(address.unwrap_or(ADDRESS), secrets).map_err(KindRefused::Credential)
}

/// The backend a table of the kind makes, sending to `address` in place of fal's own
/// when a test's build gives one.
pub(crate) fn make(table: &Table<'_>, address: Option<&str>) -> Result<Made, KindRefused> {
    let address = read(table, address, table.secrets())?;
    let identity = ModelIdentity::new(ADAPTER, MODEL, &identity_version());
    let price = Price::at_most(spend::bound_micros(spend::PRICE))
        .map_err(|_| KindRefused::Table(NOT_DECLARED))?;
    let declaration = Declaration::new(identity, &[])
        .map_err(|_| KindRefused::Table(NOT_DECLARED))?
        .sending_to(Recipient::new(address.origin().clone()))
        .priced(price);
    Ok(Made::Fill(Arc::new(Eraser {
        declaration,
        address,
        long_side: LONG_SIDE,
    })))
}

/// The key, bound to the origin of the address [`make`] sends to, in the header
/// `Authorization` as `Key` and the key. It is in no address.
pub(crate) fn credential(
    table: &Table<'_>,
    address: Option<&str>,
) -> Result<Option<Credential>, KindRefused> {
    let address = read(table, address, table.secrets())?;
    let key = table.key().ok_or(KindRefused::Table(NO_KEY))?;
    Credential::new(
        address.origin().clone(),
        KEY_HEADER,
        &format!("Key {}", key.expose()),
        table.secrets(),
    )
    .map(Some)
    .map_err(KindRefused::Credential)
}

/// The filler: Bria Eraser at one address.
pub(crate) struct Eraser {
    pub(crate) declaration: Declaration,
    pub(crate) address: Address,
    /// The longest side sent.
    pub(crate) long_side: u32,
}

impl Eraser {
    /// One request, sent through `sender`: written, sent, followed where the answer
    /// points (each address asked of the gate again, [`MOVES_LIMIT`] times at most), and
    /// its answer read.
    pub(crate) fn fill_through(
        &self,
        request: &FillRequest,
        sender: &dyn Sender,
        bounds: &Bounds,
    ) -> Result<Painted, Fault> {
        let written = wire::request(request, self.long_side)?;
        let mut to = self.address.clone();
        for _ in 0..=MOVES_LIMIT {
            let outgoing = Outgoing::new(to.clone(), written.pieces.clone())
                .and_then(|outgoing| outgoing.with_fields(FIELDS))
                .map_err(|why| match why {
                    NotAllowed::TooMany => Fault::new(FaultKind::TooLarge, NOT_WRITTEN).not_sent(),
                    _ => Fault::new(FaultKind::Rejected, NOT_WRITTEN).not_sent(),
                })?;
            let reply = sender.send(&outgoing, bounds)?;
            if let Some(next) = reply.moved_to() {
                to = next.clone();
                continue;
            }
            // An error status is charged nothing: fal charges no request it refused or
            // failed. An answer of 200 states no usage: it costs the price, its bound,
            // which the meter charges when no charge is said.
            let nothing = Charge::whole(0).ok();
            let answered = |kind, why| Err(Fault::new(kind, why).answered(nothing));
            return match reply.status() {
                Some(200) => wire::painted(reply.body(), request, &written),
                Some(400 | 404 | 413 | 422) => answered(FaultKind::Rejected, REFUSED),
                Some(401 | 403) => answered(FaultKind::Credential, CREDENTIAL),
                Some(429) => answered(FaultKind::Unavailable, RATE_LIMITED),
                Some(500 | 502 | 503 | 504 | 529) => answered(FaultKind::Unavailable, BUSY),
                Some(201..=299) => {
                    Err(Fault::new(FaultKind::Malformed, OTHER_SUCCESS).answered(None))
                }
                _ => answered(FaultKind::Malformed, STATUS),
            };
        }
        Err(Fault::new(FaultKind::Rejected, MOVED_TOO_OFTEN).unresolved())
    }
}

impl Filler for Eraser {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        self.fill_through(request, bounds.sender(), bounds)
    }
}
