//! Which device answered a request, and whether a cache did (ADR 0006, "What a
//! response says of where it was answered"; plan 1C, task P7a). Every response of a
//! capability may say both; one that says neither is answered on a device nobody
//! named, from nothing anyone said was a cache. The run's record keeps what each
//! request's answer said (`run/4`, ADR 0025).

use super::Unfit;

/// The most characters of a device's name: the protocol's bound on the one a worker
/// states (ADR 0020).
pub const DEVICE_LIMIT: usize = 32;

/// The device a model's work ran on, as its runtime names it (`cpu`, `mps`,
/// `cuda:0`): a lower-case letter, then lower-case letters, digits, `.`, `_`, `-`
/// and `:`, [`DEVICE_LIMIT`] characters at most. The rule is the pack protocol's, so a
/// worker's device is one as it is, and a name of the rule holds no path, no space
/// and nothing a terminal obeys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Device(String);

impl Device {
    /// A device of this name, when the name is one.
    pub fn named(name: &str) -> Result<Device, Unfit> {
        if is_device(name) {
            return Ok(Device(name.to_owned()));
        }
        Err(Unfit::NotAllowed {
            what: "a device's name",
            why: "is not a lower-case letter, then lower-case letters, digits, ., _, - and :, \
                  32 characters at most",
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether this is a device's name ([`Device`]).
pub fn is_device(name: &str) -> bool {
    let mut chars = name.chars();
    let first = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest = |c: char| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-' | ':')
    };
    first && name.len() <= DEVICE_LIMIT && chars.all(rest)
}

/// What a response says of where it was answered: the device, when the backend
/// named one, and whether a cache answered, when it said. Neither is known by
/// default: a backend that says nothing is recorded as saying nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Served {
    device: Option<Device>,
    cached: Option<bool>,
}

impl Served {
    /// Nothing said: the device and the cache are not known.
    pub fn unknown() -> Served {
        Served::default()
    }

    /// The same, answered on this device.
    pub fn on(mut self, device: Device) -> Served {
        self.device = Some(device);
        self
    }

    /// The same, saying whether a cache answered: `false` for a backend that has none
    /// and says so.
    pub fn cached(mut self, cached: bool) -> Served {
        self.cached = Some(cached);
        self
    }

    /// The device that answered, when it was named.
    pub fn device(&self) -> Option<&Device> {
        self.device.as_ref()
    }

    /// Whether a cache answered, when it was said.
    pub fn from_cache(&self) -> Option<bool> {
        self.cached
    }
}

/// A response that may say where it was answered: what the handle keeps for the
/// run's record when it takes the answer.
pub(crate) trait Answer {
    fn served(&self) -> &Served;
}
