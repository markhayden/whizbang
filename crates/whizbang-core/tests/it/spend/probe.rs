//! A probe of the reading's rules (ADR 0016, "What the ledger can have written", "What
//! a reservation counts for"): sets of names made from a seed, each read by the core
//! ([`Names`]) and by the test kit's model, which was written apart from it, and the
//! two compared field by field.
//!
//! The sets are of one reservation. Each begins as books a ledger could have written:
//! an admission, dispatches that fit, settlements under, at and over their bounds, a
//! closing, corrections. Then other hands change it up to three times: a name is taken
//! away, damaged, made, moved, doubled or restated, a closing says the other thing, a
//! correction, a closing or a dispatch comes with a place of its own. So the sets lie
//! on both sides of every rule of the recogniser, close to it.
//!
//! It is bounded: a fixed number of sets from a fixed seed, with no loop over a place
//! or an amount. What it cannot compare is held by named tests in `reading.rs`: a
//! seal, which the model makes another way; and whether a content can be read and
//! says what its name says, which the model is handed.

use std::collections::BTreeSet;

use whizbang_core::contract::RunId;
use whizbang_core::spend::{Counted, Names, Reading, Sent};
use whizbang_testkit::spend as model;
use whizbang_testkit::spend::{Name, Run, Stage};

/// The seed the sets are made from.
const SEED: u64 = 0x1B00_2026_1115;

/// How many sets are read.
const SETS: u64 = 6000;

/// The largest amount a name states: one under the largest number.
const MOST: u64 = u64::MAX - 1;

const ROOT: Run = Run(1);
const CHILD: Run = Run(2);

/// Numbers from a seed, the same on every machine (SplitMix64).
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }

    fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[self.below(of.len() as u64) as usize]
    }

    fn one_in(&mut self, of: u64) -> bool {
        self.below(of) == 0
    }
}

/// One set: the names, and those whose content is damaged.
#[derive(Default)]
struct Set {
    names: BTreeSet<Name>,
    damaged: BTreeSet<Name>,
}

/// Books a ledger could have written for one reservation, from what `seeded` chooses.
fn written(seeded: &mut Seeded) -> BTreeSet<Name> {
    let amount = seeded.pick(&[0, 1, 5, 10, 10, 10, 20]);
    let epoch = seeded.pick(&[0, 0, 1]);
    let mut names = BTreeSet::from([
        Name::Reservation(ROOT),
        Name::Held {
            epoch,
            run: ROOT,
            micros: amount,
        },
    ]);
    // What is known against the reservation, the next place, and each request that
    // is not settled: its place and its bound.
    let mut known = 0u64;
    let mut place = 1u64;
    let mut open: Vec<(u64, u64)> = Vec::new();
    for _ in 0..seeded.below(7) {
        let settle = !open.is_empty() && seeded.one_in(2);
        if settle {
            let (at, bound) = open.remove(seeded.below(open.len() as u64) as usize);
            let stage = seeded.pick(&Stage::ALL);
            let charge = match stage {
                Stage::NotSent => 0,
                Stage::Unresolved => bound,
                Stage::Answered => seeded.pick(&[0, bound / 2, bound, bound + 1, bound + 4, 12]),
            };
            names.insert(Name::Settling {
                run: ROOT,
                place: at,
            });
            names.insert(Name::Settled {
                run: ROOT,
                place: at,
                stage,
                micros: charge,
            });
            known = known - bound + charge;
            if charge > bound {
                names.insert(Name::Place { run: ROOT, place });
                place += 1;
            }
            continue;
        }
        let bound = seeded.pick(&[0, 1, 2, 3, 5]);
        if bound > amount.saturating_sub(known) {
            continue;
        }
        names.insert(Name::Place { run: ROOT, place });
        names.insert(Name::Dispatched {
            run: ROOT,
            place,
            by: seeded.pick(&[ROOT, CHILD]),
            micros: bound,
        });
        open.push((place, bound));
        known += bound;
        place += 1;
    }
    if seeded.one_in(2) {
        let released = open.is_empty();
        names.insert(Name::Closed {
            run: ROOT,
            released,
        });
        names.insert(Name::Place { run: ROOT, place });
        place += 1;
        for _ in 0..seeded.pick(&[0, 0, 1, 1, 2]) {
            if released {
                break;
            }
            names.insert(Name::Adjusted {
                run: ROOT,
                place,
                turn: 1 + seeded.below(9),
                seal: seeded.next(),
                micros: seeded.pick(&[0, 3, 12, 50]),
            });
            names.insert(Name::Place { run: ROOT, place });
            place += 1;
        }
    }
    names
}

/// A name other hands make: of any form a reservation's names have, at a place and
/// of an amount near those the ledger used, or far from them.
fn made(seeded: &mut Seeded) -> Name {
    let run = ROOT;
    let far = seeded.one_in(24);
    let place = match far {
        true => seeded.pick(&[u64::MAX, u64::MAX - 1, 1 << 40]),
        false => 1 + seeded.below(8),
    };
    let micros = match far {
        true => seeded.pick(&[MOST, MOST - 1, 1 << 62]),
        false => seeded.pick(&[0, 1, 2, 3, 5, 10, 12, 50]),
    };
    match seeded.below(9) {
        0 => Name::Place { run, place },
        1 => Name::Dispatched {
            run,
            place,
            by: seeded.pick(&[ROOT, CHILD]),
            micros,
        },
        2 => Name::Settling { run, place },
        3 => Name::Settled {
            run,
            place,
            stage: seeded.pick(&Stage::ALL),
            micros,
        },
        4 => Name::Closed {
            run,
            released: seeded.one_in(2),
        },
        5 => Name::Held {
            epoch: seeded.below(3),
            run,
            micros,
        },
        6 => Name::Adjusted {
            run,
            place,
            turn: 1 + seeded.below(9),
            seal: seeded.next(),
            micros,
        },
        7 => Name::Stray { run, count: 1 },
        _ => Name::Reservation(run),
    }
}

/// The lowest place that no guard among these names takes.
fn free_place(names: &BTreeSet<Name>) -> u64 {
    let mut free = 1;
    for name in names {
        if matches!(name, Name::Place { place, .. } if *place == free) {
            free += 1;
        }
    }
    free
}

/// This name at another place, when it is at one or names one.
fn moved(name: &Name, to: u64) -> Option<Name> {
    let mut name = name.clone();
    match &mut name {
        Name::Place { place, .. }
        | Name::Dispatched { place, .. }
        | Name::Settling { place, .. }
        | Name::Settled { place, .. }
        | Name::Adjusted { place, .. } => *place = to,
        _ => return None,
    }
    Some(name)
}

/// This name stating another amount, when it states one.
fn restated(name: &Name, to: u64) -> Option<Name> {
    let mut name = name.clone();
    match &mut name {
        Name::Held { micros, .. }
        | Name::Dispatched { micros, .. }
        | Name::Settled { micros, .. }
        | Name::Adjusted { micros, .. } => *micros = to,
        _ => return None,
    }
    Some(name)
}

/// A second name where the ledger writes one: another dispatch or settlement at the
/// same place, or another correction of it.
fn doubled(name: &Name, seeded: &mut Seeded) -> Option<Name> {
    let mut name = name.clone();
    match &mut name {
        Name::Dispatched { by, micros, .. } => {
            *by = seeded.pick(&[ROOT, CHILD]);
            *micros = micros.saturating_add(1).min(MOST);
        }
        Name::Settled { stage, micros, .. } => {
            *stage = seeded.pick(&Stage::ALL);
            *micros = micros.saturating_add(seeded.below(2)).min(MOST);
        }
        Name::Adjusted { turn, .. } => *turn += 1,
        _ => return None,
    }
    Some(name)
}

/// One change by other hands. Some are of one name: it is taken away, damaged or
/// made. The others keep the count of the places right, so that what tells is the
/// one rule they break: a closing that says the other thing, a correction or a
/// closing with a place of its own, a name moved to another place, a second name
/// where the ledger writes one, an amount restated, a dispatch of any bound at the
/// next place.
fn change(set: &mut Set, seeded: &mut Seeded) {
    let all: Vec<Name> = set.names.iter().cloned().collect();
    let one = all.get(seeded.below(all.len() as u64) as usize);
    let place = free_place(&set.names);
    fn replace(set: &mut Set, old: &Name, new: Option<Name>) {
        if let Some(new) = new {
            set.names.remove(old);
            set.damaged.remove(old);
            set.names.insert(new);
        }
    }
    match (seeded.below(11), one) {
        (0, Some(lost)) => {
            set.names.remove(lost);
            set.damaged.remove(lost);
        }
        // A text of no form has no content that can disagree with it.
        (1, Some(hurt)) if !matches!(hurt, Name::Stray { .. }) => {
            set.damaged.insert(hurt.clone());
        }
        (2 | 3, _) => {
            set.names.insert(made(seeded));
        }
        (4, _) => {
            let closing = all.iter().find(|name| matches!(name, Name::Closed { .. }));
            if let Some(old @ Name::Closed { run, released }) = closing {
                let other = Name::Closed {
                    run: *run,
                    released: !released,
                };
                replace(set, old, Some(other));
            }
        }
        (5, _) => {
            set.names.insert(Name::Place { run: ROOT, place });
            set.names.insert(Name::Adjusted {
                run: ROOT,
                place: seeded.pick(&[place, place, place.saturating_sub(1).max(1)]),
                turn: 1 + seeded.below(9),
                seal: seeded.next(),
                micros: seeded.pick(&[0, 3, 12, 50]),
            });
        }
        (6, _) => {
            set.names.insert(Name::Place { run: ROOT, place });
            set.names.insert(Name::Closed {
                run: ROOT,
                released: seeded.one_in(2),
            });
        }
        (7, Some(old)) => replace(set, old, moved(old, 1 + seeded.below(8))),
        (8, Some(old)) => {
            if let Some(second) = doubled(old, seeded) {
                set.names.insert(second);
            }
        }
        (9, Some(old)) => {
            let to = seeded.pick(&[0, 1, 2, 3, 4, 5, 9, 10, 11, 12]);
            replace(set, old, restated(old, to));
        }
        // A dispatch with the guard of its place, of any bound: whether it fit what
        // was left when it took its place is the replay's to say.
        (10, _) => {
            set.names.insert(Name::Place { run: ROOT, place });
            set.names.insert(Name::Dispatched {
                run: ROOT,
                place,
                by: seeded.pick(&[ROOT, CHILD]),
                micros: seeded.pick(&[0, 1, 2, 3, 4, 5, 6, 8, 10]),
            });
        }
        _ => {}
    }
}

/// The set of this number: books a ledger could have written, then changed by other
/// hands up to three times.
fn set(number: u64) -> Set {
    let mut seeded = Seeded(SEED ^ number.wrapping_mul(0xA24B_AED4_963E_E407));
    let mut set = Set {
        names: written(&mut seeded),
        damaged: BTreeSet::new(),
    };
    for _ in 0..seeded.pick(&[0, 0, 1, 1, 1, 2, 2, 3]) {
        change(&mut set, &mut seeded);
    }
    set
}

fn id(run: Run) -> RunId {
    RunId::parse(&run.to_string()).unwrap()
}

/// A name of the model as a store holds it. A correction's seal is of 32 digits in
/// the core; and the model's name of no form is a text that begins as a guard of
/// its reservation's does and is none.
fn text(name: &Name) -> String {
    match name {
        Name::Adjusted {
            run,
            place,
            turn,
            seal,
            micros,
        } => format!("spend/adjusted/{run}/{place}/{turn}/{seal:032x}/{micros}"),
        Name::Stray { run, count } => format!("spend/place/{run}/0{count}"),
        other => other.to_string(),
    }
}

/// The core's reading of the root's reservation in this set.
fn read(set: &Set) -> Option<Names> {
    let texts: Vec<(String, bool)> = set
        .names
        .iter()
        .map(|name| (text(name), set.damaged.contains(name)))
        .collect();
    let named = texts.iter().map(|(text, hurt)| (text.as_str(), *hurt));
    let mut reading = Reading::of_names(named);
    assert!(reading.epochs.is_empty() && reading.formless.is_empty());
    reading.reservations.remove(&id(ROOT))
}

/// What the core read, in the model's type: every field a reservation's names give
/// by themselves. What a reservation counts for and its state are across
/// reservations and epochs, and are the fold's.
fn standing(names: &Names, of: &model::Standing) -> model::Standing {
    model::Standing {
        epoch: names.epoch(),
        epochs: names.epochs(),
        amount: names.amount().micros(),
        charged: names.charged().micros(),
        settled: names.settled().micros(),
        outstanding: names.outstanding().micros(),
        unresolved: names.unresolved().micros(),
        unsettled: names.unsettled(),
        unexplained: names.unexplained(),
        corrections: names.corrections_most().micros(),
        reachable: names.reachable(),
        names: names.count(),
        closed: names.closed(),
        counts: of.counts,
        unknown: of.unknown,
        state: of.state,
    }
}

fn sent(found: Sent) -> model::Sent {
    let plain = |one: whizbang_core::spend::RunSpend| model::RunSpend {
        charged: one.charged.micros(),
        outstanding: one.outstanding.micros(),
        unresolved: one.unresolved.micros(),
    };
    match found {
        Sent::Known(one) => model::Sent::Known(plain(one)),
        Sent::Unknown { at_most } => model::Sent::Unknown {
            at_most: at_most.micros(),
        },
        Sent::Unbounded => model::Sent::Unbounded,
    }
}

/// What the probe reached: sets the recogniser took and did not, and of each those
/// that hold a correction.
#[derive(Default, Debug, PartialEq, Eq)]
struct Reached {
    reachable: u64,
    frozen: u64,
    corrected: u64,
    corrected_frozen: u64,
    settled_over: u64,
    damaged: u64,
    no_one_hold: u64,
}

/// Compares the two readings of one set; what comes back is what differed.
fn compare(set: &Set, reached: &mut Reached) -> Result<(), String> {
    let folded = model::fold(&set.names, &set.damaged);
    let (wanted, found) = match (folded.reservations.get(&ROOT), read(set)) {
        (None, None) => return Ok(()),
        (Some(wanted), Some(found)) => (wanted, found),
        (wanted, found) => return Err(format!("the model has {wanted:?}, the core {found:?}")),
    };
    let as_model = standing(&found, wanted);
    if as_model != *wanted {
        return Err(format!(
            "the core reads {as_model:#?}, the model {wanted:#?}"
        ));
    }
    let same = found.most().micros() == wanted.most()
        && found.known().micros() == wanted.known()
        && found.damaged() == set.names.iter().any(|name| set.damaged.contains(name));
    if !same {
        return Err(format!(
            "the most: the core {}, the model {}; known: {}, {}",
            found.most(),
            wanted.most(),
            found.known(),
            wanted.known()
        ));
    }
    for by in [ROOT, CHILD, Run(3)] {
        let (one, other) = (found.sent_by(&id(by)), folded.sent_by(by, ROOT));
        if sent(one) != other {
            return Err(format!("{by} sent {one:?}, the model {other:?}"));
        }
    }
    // What a frozen reservation counts for, where no correction is there to stand:
    // the most its names can mean, and "unknown, at least" without exactly one hold.
    let corrected = set
        .names
        .iter()
        .any(|name| matches!(name, Name::Adjusted { .. }));
    let frozen = match found.frozen() {
        Some(Counted::Exactly(sum)) => Some(model::Counted::Exactly(sum.micros())),
        Some(Counted::AtLeast(sum)) => Some(model::Counted::AtLeast(sum.micros())),
        None => None,
    };
    if found.reachable() != frozen.is_none() {
        return Err(format!("reachable, and frozen at {frozen:?}"));
    }
    if !corrected && !found.reachable() && frozen != Some(wanted.counted()) {
        let counted = wanted.counted();
        return Err(format!("frozen at {frozen:?}, the model {counted:?}"));
    }
    let over = |name: &Name| match name {
        Name::Settled { place, micros, .. } => set.names.iter().any(|other| {
            matches!(other, Name::Dispatched { place: at, micros: bound, .. }
                if at == place && bound < micros)
        }),
        _ => false,
    };
    reached.reachable += u64::from(found.reachable());
    reached.frozen += u64::from(!found.reachable());
    reached.corrected += u64::from(corrected && found.reachable());
    reached.corrected_frozen += u64::from(corrected && !found.reachable());
    reached.settled_over += u64::from(found.reachable() && set.names.iter().any(over));
    reached.damaged += u64::from(found.damaged());
    reached.no_one_hold += u64::from(found.holds().len() != 1);
    Ok(())
}

#[test]
fn every_generated_set_of_names_reads_in_the_core_as_the_model_reads_it() {
    let mut reached = Reached::default();
    for number in 0..SETS {
        let set = set(number);
        if let Err(differed) = compare(&set, &mut reached) {
            let names: Vec<String> = set
                .names
                .iter()
                .map(|name| {
                    let hurt = set.damaged.contains(name);
                    format!("{}{}", text(name), if hurt { " (damaged)" } else { "" })
                })
                .collect();
            panic!("set {number}: {differed}\n{names:#?}");
        }
    }
    // The probe is held to what it reached: a change to the generator that left it
    // on one side of the recogniser would pass for nothing.
    assert_eq!(
        reached,
        Reached {
            reachable: 2632,
            frozen: 3368,
            corrected: 412,
            corrected_frozen: 1191,
            settled_over: 291,
            damaged: 626,
            no_one_hold: 257,
        }
    );
}
