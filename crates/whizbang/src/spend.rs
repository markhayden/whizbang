//! `whizbang spend status|reset`: a store's allowance, what is counted against it, and
//! the owner's reset (plan 1B, task S9; ADR 0016, "The strand cut down").
//!
//! Spend control is a guard and a record (`SPEC.md` section 6). `status` is the
//! record: it reads the books, which are the names a store holds under `spend/`, and
//! says what they come to. It only reads, so it answers while another process writes
//! the store. `reset` is the owner's one way to set an allowance and to clear the
//! books: it opens the store to write through the one entry point
//! (`wiring::opening`), so the store is recovered first, and makes the reset's three
//! names in one publication (`whizbang_core::spend::reset`). It is a command and
//! nothing else: no action resets an allowance.
//!
//! Amounts are printed in US dollars. Everything read from the store is printed
//! escaped and cut: a run's id, a name, a text that is no name. Nothing printed is a
//! path, but for the store's own where a store could not be opened, which is the
//! path the caller gave and is escaped where a complaint is printed.

use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use clap::Subcommand;
use whizbang::wiring::{self, Make};
use whizbang_core::contract::{Clock, Recovery, RunId, SystemClock};
use whizbang_core::spend::fold::{Folded, Standing, State};
use whizbang_core::spend::{Amount, Counted, EpochWord, Name, Reading, Record, Sent, Sum};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Generation, Reader};

use crate::store::Where;

/// How many texts of no form `status` quotes before it only counts them.
const QUOTED: usize = 20;

#[derive(Subcommand)]
pub enum Command {
    /// Show what the store's books say: the allowance of the newest epoch, what is
    /// counted against it, what is left, and each reservation that still counts.
    /// Reads only: it works while another process writes.
    ///
    /// A run's end closes its reservation, and one that released what it did not
    /// spend is not listed. A reservation shown as "open" belongs to a run that is
    /// still executing, or to a process that has died: nothing closes that one, it
    /// counts in full, and `spend reset` clears it. A damaged one is shown as
    /// "frozen", and `spend reset` clears that too.
    Status {
        #[command(flatten)]
        at: Where,
        /// Show one run instead: what was reserved for it, what its requests were
        /// charged, and what they still hold.
        #[arg(long, value_name = "RUN")]
        run: Option<String>,
    },
    /// Set the store's allowance and close the books: nothing reserved, left open
    /// or frozen before the reset counts against the new allowance. The records
    /// stay. A run that is executing is not stopped: it goes on drawing on what it
    /// reserved.
    Reset {
        #[command(flatten)]
        at: Where,
        /// The new allowance, in US dollars: digits, with at most six after a point,
        /// as in 20 or 12.50.
        #[arg(long, value_name = "USD")]
        allowance: String,
    },
}

/// Runs a spend command: what to print on success, or the complaint.
pub fn run(command: &Command) -> Result<String, String> {
    run_with(command, wiring::recovery())
}

/// The same, with a store that is opened to write recovered by this.
pub fn run_with(command: &Command, recovery: Recovery) -> Result<String, String> {
    match command {
        Command::Status { at, run } => status(at.root(), run.as_deref()),
        Command::Reset { at, allowance } => reset(at.root(), allowance, recovery),
    }
}

/// A text that was read from a store, or given on the command line, as a message
/// shows it: its first 64 characters, escaped.
fn shown(text: &str) -> String {
    let cut: String = text.chars().take(64).collect();
    let more = if text.chars().count() > 64 { "..." } else { "" };
    format!("{}{more}", cut.escape_debug())
}

/// Millionths of a US dollar as money: a dollar sign, the dollars, and the cents,
/// with the digits after them only where they are not nothing. `whizbang runs show`
/// prints a record's amounts with it too, so the two commands say one amount one way.
pub(crate) fn usd(micros: u128) -> String {
    let mut part = format!("{:06}", micros % 1_000_000);
    while part.len() > 2 && part.ends_with('0') {
        part.pop();
    }
    format!("${}.{part}", micros / 1_000_000)
}

/// A sum as money. One that stopped at the largest number is said so: what it
/// stands for may be more.
fn sum(sum: Sum) -> String {
    match sum.stopped() {
        true => "more than can be counted".to_owned(),
        false => usd(sum.micros()),
    }
}

fn amount(amount: Amount) -> String {
    usd(u128::from(amount.micros()))
}

fn counted(counted: Counted) -> String {
    match counted {
        Counted::Exactly(exactly) => sum(exactly),
        Counted::AtLeast(least) => format!("unknown, at least {}", sum(least)),
    }
}

/// The allowance a caller typed, in US dollars: digits, and at most six more after
/// a point. Anything else is refused, and so is the largest number there is, which
/// is no amount.
fn allowance(text: &str) -> Result<Amount, String> {
    let refuse = |why: &str| {
        Err(format!(
            "{} is no allowance: {why}. Give US dollars as digits, with at most six \
             after a point, as in 20 or 12.50",
            shown(text)
        ))
    };
    let (whole, part) = match text.split_once('.') {
        Some((whole, part)) => (whole, Some(part)),
        None => (text, None),
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || part.is_some_and(|part| !digits(part)) {
        return refuse("it is not a number of dollars");
    }
    let part = part.unwrap_or("0");
    if part.len() > 6 {
        return refuse("a millionth of a dollar is the least that is counted");
    }
    let micros = whole.parse::<u64>().ok().and_then(|whole| {
        let part = format!("{part:0<6}").parse::<u64>().ok()?;
        whole.checked_mul(1_000_000)?.checked_add(part)
    });
    let Some(micros) = micros else {
        return refuse("it is more than can be counted");
    };
    match Amount::new(micros) {
        Some(amount) => Ok(amount),
        None => refuse("it is the largest number there is, which is no amount"),
    }
}

fn reset(root: &Path, asked: &str, recovery: Recovery) -> Result<String, String> {
    let allowance = allowance(asked)?;
    // The store must be there, and is recovered before anything is made in it.
    let opening = wiring::opening(root, Make::Nothing, recovery)
        .map_err(|error| wiring::not_opened(&error))?;
    let store = opening.writer().map_err(|why| why.to_string())?;
    let now = SystemClock.now_ms();
    let opened =
        whizbang_core::spend::reset(&store, allowance, now).map_err(|why| why.to_string())?;
    Ok(format!(
        "allowance reset: {}\nepoch: {}\ngeneration: {}\n",
        amount(opened.allowance),
        opened.epoch,
        opened.generation
    ))
}

/// The books of a store, read: the fold, and what only the names themselves say.
struct Books {
    folded: Folded,
    /// The newest epoch's resets that can be read, each with when it was made.
    resets: Vec<(Amount, u64)>,
    /// Whether a name of the newest epoch's reset or guard is damaged, of no form or
    /// missing: the epoch then allows nothing.
    whole: bool,
    /// Every text under `spend/` of no form that belongs to no reservation, as it is
    /// refused: escaped and cut.
    formless: Vec<String>,
}

fn books(root: &Path) -> Result<Books, String> {
    let reader = Reader::open(root, Arc::new(RealFs)).map_err(|e| e.to_string())?;
    let generation = reader.snapshot().map_err(|e| e.to_string())?;
    let reading = Reading::of(&generation, |entry| reader.read(entry));
    let formless: Vec<String> = reading
        .formless
        .iter()
        .map(|one| one.why.to_string())
        .collect();
    let folded = Folded::of(reading.clone());
    let newest = folded.epoch();
    let (mut resets, mut guarded, mut whole) = (Vec::new(), false, true);
    for (name, damaged) in &reading.epochs {
        match name {
            Name::Reset { epoch, .. } if epoch.get() == newest => {
                whole &= !damaged;
                if let Some(Record::Reset {
                    allowance, at_ms, ..
                }) = stored(&reader, &generation, name)
                {
                    resets.push((allowance, at_ms));
                }
            }
            Name::Epoch(epoch) if epoch.get() == newest => {
                guarded = true;
                whole &= !damaged;
            }
            _ => {}
        }
    }
    let of_newest = |word: &EpochWord| matches!(word, EpochWord::Reset | EpochWord::Guard);
    let garbled = reading.formless.iter().any(
        |one| matches!(&one.epoch, Some((word, epoch)) if of_newest(word) && *epoch == newest),
    );
    Ok(Books {
        folded,
        resets,
        whole: whole && guarded && !garbled,
        formless,
    })
}

/// The record under a name of the ledger, where it can be read and says what its
/// name says.
fn stored(reader: &Reader, generation: &Generation, name: &Name) -> Option<Record> {
    let entry = generation.get(&name.to_string())?;
    let bytes = reader.read(entry).ok()?;
    Record::read(name, &bytes).ok()
}

fn state(state: State) -> &'static str {
    match state {
        State::Open => "open",
        State::Released => "released",
        State::Interrupted => "interrupted",
        State::Frozen => "frozen",
        State::Adjusted => "corrected",
    }
}

fn epochs(epochs: impl IntoIterator<Item = u64>) -> String {
    let epochs: Vec<String> = epochs.into_iter().map(|epoch| epoch.to_string()).collect();
    epochs.join(", ")
}

/// One reservation, on a line: its run, its state, what it counts for, and what
/// its names say of it.
fn line(run: &RunId, one: &Standing) -> String {
    let names = one.names();
    let mut text = format!(
        "  {}  {}  counts for {}  (reserved {}, charged {}, outstanding {})",
        shown(run.as_str()),
        state(one.state()),
        counted(one.counted()),
        amount(names.amount()),
        sum(names.charged()),
        sum(names.outstanding()),
    );
    if names.holds().len() > 1 {
        let _ = write!(text, "; its holds name epochs {}", epochs(names.epochs()));
    }
    let damaged = names.all().iter().filter(|(_, damaged)| *damaged);
    let damaged: Vec<String> = damaged.map(|(name, _)| shown(&name.to_string())).collect();
    if let Some(first) = damaged.first() {
        let _ = write!(
            text,
            "; {} of its records cannot be read or do not say what their names say, \
             the first under {first}",
            damaged.len()
        );
    }
    if let Some(first) = names.formless().first() {
        let _ = write!(
            text,
            "; {} texts of no form name it, the first {}",
            names.formless().len(),
            shown(first)
        );
    }
    text.push('\n');
    text
}

fn status(root: &Path, run: Option<&str>) -> Result<String, String> {
    let books = books(root)?;
    match run {
        Some(run) => of_run(&books.folded, run),
        None => Ok(of_store(&books)),
    }
}

fn of_store(books: &Books) -> String {
    let folded = &books.folded;
    let balance = folded.balance();
    let newest = balance.epoch;
    let mut text = String::new();
    let mut say = |line: String| {
        text.push_str(&line);
        text.push('\n');
    };
    say(format!("epoch: {newest}"));
    match newest {
        0 => say(
            "allowance: none. No reset has set one, so every paid call is refused: \
             whizbang spend reset --allowance <usd>"
                .to_owned(),
        ),
        _ => say(format!("allowance: {}", amount(balance.allowed))),
    }
    say(format!("counted against it: {}", counted(balance.counted)));
    say(format!("  charged: {}", sum(balance.charged)));
    say(format!("  held beyond that: {}", sum(balance.held)));
    say(format!("  unresolved: {}", sum(balance.unresolved)));
    say(format!("left: {}", amount(balance.left)));
    say(format!("debt: {}", sum(balance.debt)));
    let made = |(allowance, at_ms): &(Amount, u64)| {
        format!("{} at {}", amount(*allowance), crate::runs::utc(*at_ms))
    };
    let resets: Vec<String> = books.resets.iter().map(made).collect();
    match (newest, resets.is_empty()) {
        (0, _) => say("last reset: none".to_owned()),
        (_, true) => say(format!(
            "last reset: epoch {newest}, and no record of it can be read"
        )),
        (_, false) => say(format!(
            "last reset: epoch {newest}, to {}",
            resets.join(" and to ")
        )),
    }
    if newest > 0 && !books.whole {
        say(format!(
            "the reset of epoch {newest} is not whole (its record or its guard is \
             missing, cannot be read or is of no form): it allows nothing until the \
             next reset"
        ));
    }
    if balance.needs_owner {
        say(format!(
            "epoch {newest} admits nothing until a reset: its turns are not those its \
             publications took, so a name that stated an amount may be gone"
        ));
    }
    let earlier = folded.resets().keys().copied().chain([0]);
    let earlier: std::collections::BTreeSet<u64> = earlier
        .filter(|epoch| *epoch != newest && folded.needs_owner(*epoch))
        .collect();
    if !earlier.is_empty() {
        say(format!(
            "earlier epochs whose turns are not those their publications took: {}",
            epochs(earlier)
        ));
    }
    drop(say);
    // Each reservation that counts in the newest epoch and that no run's end
    // closed as released.
    let here =
        |(_, one): &(&RunId, &Standing)| one.counts_in(newest) && one.state() != State::Released;
    let counting: Vec<_> = folded.reservations().iter().filter(here).collect();
    let _ = writeln!(
        text,
        "reservations that count in epoch {newest} and were not released: {}",
        counting.len()
    );
    for (run, one) in &counting {
        text.push_str(&line(run, one));
    }
    // Frozen reservations that count in earlier epochs alone: a reset passed them.
    let passed = |(_, one): &(&RunId, &Standing)| {
        one.state() == State::Frozen && !one.counts_in(newest) && !one.names().holds().is_empty()
    };
    let passed: Vec<_> = folded.reservations().iter().filter(passed).collect();
    if !passed.is_empty() {
        let _ = writeln!(
            text,
            "frozen reservations that count in earlier epochs alone: {}",
            passed.len()
        );
        for (run, one) in &passed {
            let of = epochs(one.names().epochs());
            text.push_str(line(run, one).trim_end());
            let _ = writeln!(text, "; in epoch {of}");
        }
    }
    // What counts nowhere: a reservation with no hold, and a text nothing places.
    let nowhere = |(_, one): &(&RunId, &Standing)| one.names().holds().is_empty();
    let nowhere: Vec<_> = folded.reservations().iter().filter(nowhere).collect();
    if !nowhere.is_empty() {
        let _ = writeln!(
            text,
            "reservations that count against no allowance, because no hold names an \
             epoch for them: {}",
            nowhere.len()
        );
        for (run, one) in &nowhere {
            text.push_str(&line(run, one));
        }
    }
    if !books.formless.is_empty() {
        let _ = writeln!(
            text,
            "texts under spend/ that are no name of the ledger and belong to no \
             reservation: {}, of which {} are placed nowhere and change no balance",
            books.formless.len(),
            folded.unplaced()
        );
        for one in books.formless.iter().take(QUOTED) {
            let _ = writeln!(text, "  {one}");
        }
        if books.formless.len() > QUOTED {
            let _ = writeln!(text, "  and {} more", books.formless.len() - QUOTED);
        }
    }
    text
}

/// One run: the reservation made for it, when it is the root of a tree, and what
/// the requests it sent were charged and still hold, under whichever reservation.
fn of_run(folded: &Folded, given: &str) -> Result<String, String> {
    let run = RunId::parse(given)?;
    let mut text = format!("run: {}\n", shown(run.as_str()));
    let own = folded.reservation(&run);
    if let Some(one) = own {
        let names = one.names();
        let held = one.counts().less(names.charged());
        let _ = writeln!(text, "reservation: {}", state(one.state()));
        match names.holds().is_empty() {
            true => text.push_str("  reserved: unknown: no hold of it is in the books\n"),
            false => {
                let _ = writeln!(
                    text,
                    "  reserved: {}, in epoch {}",
                    amount(names.amount()),
                    epochs(names.epochs())
                );
            }
        }
        let _ = writeln!(text, "  counts for: {}", counted(one.counted()));
        let _ = writeln!(text, "  charged: {}", sum(names.charged()));
        let _ = writeln!(text, "  outstanding: {}", sum(names.outstanding()));
        let _ = writeln!(text, "  unresolved: {}", sum(names.unresolved()));
        let _ = writeln!(text, "  still held beyond what was charged: {}", sum(held));
    }
    // The reservations its requests were sent under: its own, or that of the run
    // at the root of its tree.
    let names_it =
        |(_, one): &(&RunId, &Standing)| one.names().at().iter().any(|at| at.by.contains(&run));
    let mut roots: Vec<&RunId> = folded
        .reservations()
        .iter()
        .filter(names_it)
        .map(|(root, _)| root)
        .collect();
    if own.is_some() && !roots.contains(&&run) {
        roots.push(&run);
    }
    if roots.is_empty() {
        text.push_str("the books hold no reservation of it and no request it sent\n");
        return Ok(text);
    }
    for root in roots {
        let under = match *root == run {
            true => "its own reservation".to_owned(),
            false => format!("the reservation of {}", shown(root.as_str())),
        };
        let said = match folded.sent_by(&run, root) {
            Sent::Known(spend) => format!(
                "charged {}, still held {}, unresolved {}",
                sum(spend.charged),
                sum(spend.outstanding),
                sum(spend.unresolved)
            ),
            Sent::Unknown { at_most } => format!(
                "unknown, at most {}: that reservation is frozen, so which run sent \
                 what cannot be read",
                sum(at_most)
            ),
            Sent::Unbounded => "unknown: that reservation is frozen and no one hold \
                                states its amount"
                .to_owned(),
        };
        let _ = writeln!(text, "its requests under {under}: {said}");
    }
    Ok(text)
}
