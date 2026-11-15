//! The owner's reset (ADR 0016, "Epochs" and "The strand cut down"; plan 1B, task
//! S9): the one thing that sets an allowance, and the one way to clear the books.
//!
//! A reset is one publication of three names: the reset of the next epoch, which
//! states the allowance; its guard, without which a reset allows nothing; and the
//! next turn of the epoch it closes, so that an admission that judged the books
//! before it is refused by the store and judged again. Nothing is removed and
//! nothing is rewritten: the records before it stay, and what they hold in the
//! epochs before it counts in none after.
//!
//! **It closes the books.** A reservation counts only in the epochs its holds name
//! ([`Standing::counts_in`](super::fold::Standing::counts_in)), so nothing made
//! before a reset counts in the epoch it opens: not an open reservation, one a dead
//! process left, a frozen one, or one with no hold. A hold that other hands made
//! naming the epoch a reset opens counts there, and the reset after that passes it.
//!
//! **It stops nothing that runs.** A run admitted before it goes on drawing on its
//! own reservation, in the epoch it closed.

use std::fmt;
use std::num::NonZeroU64;

use crate::store::{self, Kind, Store};

use super::fold::fold;
use super::name::Amount;
use super::record::{Record, TurnBy};

/// What a reset made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opened {
    /// The epoch it opened.
    pub epoch: u64,
    /// What that epoch allows.
    pub allowance: Amount,
    /// The store's generation that holds it.
    pub generation: u64,
}

/// Why an allowance was not reset. Nothing was published. Its words hold no path.
#[derive(Debug)]
pub enum NotReset {
    /// The newest epoch is the largest a name can say, so no name can say the next.
    /// Only other hands open that epoch (ADR 0016, "Epochs").
    NoEpochAfter { epoch: u64 },
    /// The store refused the publication, or could not be read.
    Store(store::Error),
}

impl fmt::Display for NotReset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotReset::NoEpochAfter { epoch } => write!(
                f,
                "the allowance was not reset: the store's newest epoch is {epoch}, the \
                 largest number a name can say, so no reset can follow it. Whizbang \
                 opens no such epoch; nothing gets a store out of it but a new store"
            ),
            NotReset::Store(store::Error::GenerationTooLarge { bytes, limit }) => write!(
                f,
                "the allowance was not reset: a reset adds three names, and the store \
                 holds too many for one generation already (its file would be {bytes} \
                 bytes, and one is at most {limit}); nothing was published. Deleting \
                 runs and pruning makes room where runs' names filled it"
            ),
            NotReset::Store(error) => {
                write!(f, "the allowance was not reset: {}", error.without_paths())
            }
        }
    }
}

impl std::error::Error for NotReset {}

impl From<store::Error> for NotReset {
    fn from(error: store::Error) -> NotReset {
        NotReset::Store(error)
    }
}

/// Resets the store's allowance: opens the epoch after the newest, allowing this
/// much, in one publication. `at_ms` is when, for the record.
pub fn reset(store: &Store, allowance: Amount, at_ms: u64) -> Result<Opened, NotReset> {
    let generation = store.snapshot()?;
    let folded = fold(&generation, |entry| store.read(entry));
    let closes = folded.epoch();
    let next = closes.checked_add(1).and_then(NonZeroU64::new);
    let Some(epoch) = next else {
        return Err(NotReset::NoEpochAfter { epoch: closes });
    };
    // The lowest turn that is free is never nothing: turns are numbers from 1.
    let turn = NonZeroU64::new(folded.next_turn(closes)).unwrap_or(NonZeroU64::MIN);
    let records = [
        Record::Reset {
            epoch,
            allowance,
            at_ms,
        },
        Record::Epoch { epoch },
        Record::Turn {
            epoch: closes,
            turn,
            by: TurnBy::Reset,
            run: None,
        },
    ];
    let mut publication = store.begin()?;
    for record in &records {
        let name = record.name().to_string();
        publication.create(&name, Kind::Diagnostic, &record.to_bytes())?;
    }
    let published = publication.publish()?;
    Ok(Opened {
        epoch: epoch.get(),
        allowance,
        generation: published.generation,
    })
}
