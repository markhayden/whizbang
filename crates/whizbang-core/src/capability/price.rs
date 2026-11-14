//! What a backend says of cost (`SPEC.md` section 6, "Spend"; ADR 0010).
//!
//! A [`Quote`] is what one request may cost, in millionths of a US dollar (ADR 0007's
//! unit), and is one of three kinds: free, an enforceable bound, or an estimate only.
//! A [`Price`] is what a backend states before any request exists, and gives the quote
//! of one request under it. A price by time is a bound only under a limit its host
//! enforces, or with a fixed maximum charge: a timeout of whizbang's own is not a bound.
//!
//! Nothing here reserves, records or refuses a request. A quote reaches admission as
//! the answer of [`Backends::bound`](crate::contract::Backends::bound), through
//! [`Quote::most`]: an estimate only is the amount that is no bound.
//!
//! Every number becomes a whole count of the unit where it enters, rounded up, and
//! everything after that is worked out in whole numbers. None of these types holds a
//! text, and none is deserialised.

use super::Unfit;
use crate::contract::Estimate;

/// What a number of millionths may be, as a refusal says it.
const AMOUNT_RULE: &str =
    "a finite number of millionths of a US dollar, from 0 to under 2 to the power 64";

/// 2 to the power 64, which a 64-bit float holds exactly: the least number that is not
/// an amount. The largest float under it is 2,048 less, so no float that passes becomes
/// the largest whole number, which is ADR 0007's "no bound".
const TWO_TO_THE_64: f64 = 18_446_744_073_709_551_616.0;

/// The whole count of millionths that a number comes to: rounded up, never down.
/// Refused when it is not finite, is less than nothing, or reaches 2 to the power 64.
fn whole(what: &'static str, number: f64) -> Result<u64, Unfit> {
    let fits = (0.0..TWO_TO_THE_64).contains(&number);
    if !fits {
        // A number that is not one is in no range, so it is refused here too.
        return Err(Unfit::OutOfRange {
            what,
            at: None,
            found: number.to_string(),
            allowed: AMOUNT_RULE,
        });
    }
    // Rounding up a number in that range gives a whole number in it, which the
    // conversion holds exactly.
    Ok(number.ceil() as u64)
}

/// Which of the three a quote is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuoteKind {
    /// The backend said that a request costs nothing.
    Free,
    /// A request is never charged more than the amount, and something other than
    /// whizbang's own patience holds that.
    Bound,
    /// About the amount is expected, and nothing holds a request to it.
    EstimateOnly,
}

/// What one request to a backend may cost: free, an enforceable bound, or an estimate
/// only, in millionths of a US dollar. What `{:?}` prints is the kind and the amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Quote {
    kind: QuoteKind,
    micros: u64,
}

impl Quote {
    /// The backend said that a request costs nothing.
    pub fn free() -> Quote {
        Quote {
            kind: QuoteKind::Free,
            micros: 0,
        }
    }

    /// A request is never charged more than this many millionths of a US dollar. A
    /// fraction of a millionth is rounded up. Refused: a number that is not finite, is
    /// less than nothing, or is 2 to the power 64 or more.
    pub fn bound(micros: f64) -> Result<Quote, Unfit> {
        Ok(Quote {
            kind: QuoteKind::Bound,
            micros: whole("a quote", micros)?,
        })
    }

    /// About this many millionths of a US dollar are expected, and nothing holds a
    /// request to it. Rounded and refused as [`Quote::bound`] is.
    pub fn estimate_only(micros: f64) -> Result<Quote, Unfit> {
        Ok(Quote {
            kind: QuoteKind::EstimateOnly,
            micros: whole("a quote", micros)?,
        })
    }

    pub fn kind(&self) -> QuoteKind {
        self.kind
    }

    /// The amount, in millionths of a US dollar: the bound, or the estimate; nothing
    /// for a free quote. Whether it binds is the kind's to say.
    pub fn micros(&self) -> u64 {
        self.micros
    }

    /// The most the request may cost, as [`Backends::bound`] answers it: nothing when
    /// free, the amount when it is a bound, and for an estimate only the amount that is
    /// no bound ([`Estimate::unbounded`]), whatever the estimate was.
    ///
    /// [`Backends::bound`]: crate::contract::Backends::bound
    pub fn most(&self) -> Estimate {
        match self.kind {
            QuoteKind::Free => Estimate::of_micros(0),
            QuoteKind::Bound => Estimate::of_micros(self.micros),
            QuoteKind::EstimateOnly => Estimate::unbounded(),
        }
    }
}

/// How long a request priced by time may run, in milliseconds, and who enforces that.
/// Only a limit the backend's host enforces bounds what the time costs: when whizbang
/// stops waiting, nothing shows that the host stopped billing (`SPEC.md` section 6).
///
/// Who enforces a limit is the statement of whoever configures the backend. It cannot
/// be checked here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeLimit {
    enforced_by_host: bool,
    ms: u64,
}

impl TimeLimit {
    /// A limit the backend's host itself enforces: it stops the work, and the billing,
    /// after this many milliseconds whatever whizbang does. A limit of no time is
    /// refused.
    pub fn of_host(ms: u64) -> Result<TimeLimit, Unfit> {
        TimeLimit::new(true, ms)
    }

    /// A timeout of whizbang's own: it stops waiting after this many milliseconds, and
    /// the host may go on. A limit of no time is refused.
    pub fn own_timeout(ms: u64) -> Result<TimeLimit, Unfit> {
        TimeLimit::new(false, ms)
    }

    fn new(enforced_by_host: bool, ms: u64) -> Result<TimeLimit, Unfit> {
        if ms == 0 {
            return Err(Unfit::Empty {
                what: "a limit on a request's time",
            });
        }
        Ok(TimeLimit {
            enforced_by_host,
            ms,
        })
    }

    /// Whether the backend's host enforces the limit.
    pub fn is_enforced_by_host(&self) -> bool {
        self.enforced_by_host
    }

    /// The limit, in milliseconds.
    pub fn ms(&self) -> u64 {
        self.ms
    }
}

/// What a backend states of its price, apart from a fixed maximum charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Stated {
    Free,
    AtMost {
        micros: u64,
    },
    About {
        micros: u64,
    },
    ByTime {
        micros_per_second: u64,
        limit: TimeLimit,
        /// The price of the whole limit, worked out when the price was stated.
        micros: u64,
    },
}

/// A backend's statement of its price, made before any request exists: free, a fixed
/// maximum charge for a request, an estimate, or a price by time under a limit. What
/// `{:?}` prints is the statement and its numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Price {
    stated: Stated,
    /// A fixed maximum charge for one request, stated beside the price.
    most: Option<u64>,
}

impl Price {
    fn of(stated: Stated) -> Price {
        Price { stated, most: None }
    }

    /// The backend says that a request costs nothing.
    pub fn free() -> Price {
        Price::of(Stated::Free)
    }

    /// A fixed maximum charge: one request is never charged more than this many
    /// millionths of a US dollar. Rounded and refused as [`Quote::bound`] is.
    pub fn at_most(micros: f64) -> Result<Price, Unfit> {
        let micros = whole("a price", micros)?;
        Ok(Price::of(Stated::AtMost { micros }))
    }

    /// An estimate: one request is expected to cost about this many millionths of a US
    /// dollar, and nothing holds it to that. Rounded and refused as [`Quote::bound`]
    /// is.
    pub fn about(micros: f64) -> Result<Price, Unfit> {
        let micros = whole("a price", micros)?;
        Ok(Price::of(Stated::About { micros }))
    }

    /// A price by time: this many millionths of a US dollar a second, for a request
    /// that runs no longer than the limit. The price a second is rounded up to a whole
    /// millionth here, and the price of the whole limit is
    /// `price × milliseconds / 1000`, in whole numbers, rounded up. Refused: a price
    /// that [`Quote::bound`] would refuse, and a price that over the whole limit
    /// reaches the largest amount, which is no amount.
    pub fn by_time(micros_per_second: f64, limit: TimeLimit) -> Result<Price, Unfit> {
        let micros_per_second = whole("a price by time", micros_per_second)?;
        let product = u128::from(micros_per_second) * u128::from(limit.ms);
        let micros = u64::try_from(product.div_ceil(1000)).unwrap_or(u64::MAX);
        if micros == u64::MAX {
            return Err(Unfit::OutOfRange {
                what: "a price by time, over its limit,",
                at: None,
                found: format!("{micros_per_second} a second for {} milliseconds", limit.ms),
                allowed: AMOUNT_RULE,
            });
        }
        Ok(Price::of(Stated::ByTime {
            micros_per_second,
            limit,
            micros,
        }))
    }

    /// The same price, with a fixed maximum charge for one request: whatever else is
    /// stated, a request is never charged more than this many millionths of a US
    /// dollar. Stated twice, the lower charge is the charge. Rounded and refused as
    /// [`Quote::bound`] is.
    pub fn charged_at_most(mut self, micros: f64) -> Result<Price, Unfit> {
        let micros = whole("a maximum charge", micros)?;
        self.most = Some(self.most.map_or(micros, |most| most.min(micros)));
        Ok(self)
    }

    /// The price a second, in millionths of a US dollar, when the price is by time.
    pub fn micros_per_second(&self) -> Option<u64> {
        match self.stated {
            Stated::ByTime {
                micros_per_second, ..
            } => Some(micros_per_second),
            _ => None,
        }
    }

    /// What one request may cost under this price.
    ///
    /// - Free: free, whatever maximum charge is stated beside it.
    /// - A fixed maximum charge: a bound of it.
    /// - An estimate: an estimate only.
    /// - By time: the price of the whole limit; a bound when the host enforces the
    ///   limit, and an estimate only when the limit is a timeout of whizbang's own.
    ///
    /// A maximum charge stated beside the price makes the quote a bound: of that
    /// charge, or of the smaller of it and the bound the price had already.
    pub fn quote(&self) -> Quote {
        let quote = |kind, micros| Quote { kind, micros };
        let stated = match self.stated {
            Stated::Free => return Quote::free(),
            Stated::AtMost { micros } => quote(QuoteKind::Bound, micros),
            Stated::About { micros } => quote(QuoteKind::EstimateOnly, micros),
            Stated::ByTime { limit, micros, .. } if limit.enforced_by_host => {
                quote(QuoteKind::Bound, micros)
            }
            Stated::ByTime { micros, .. } => quote(QuoteKind::EstimateOnly, micros),
        };
        match (self.most, stated.kind) {
            (None, _) => stated,
            (Some(most), QuoteKind::Bound) => quote(QuoteKind::Bound, most.min(stated.micros)),
            (Some(most), _) => quote(QuoteKind::Bound, most),
        }
    }
}
