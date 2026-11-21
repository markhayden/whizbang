//! A run's journal held to a bound, its typed events, and its two formats (plan m5,
//! task K1, items 3 and 4): the first events kept and the rest counted, the same in
//! the store as in memory; a journal of the first format read as it always was.

use std::sync::Arc;

use whizbang_core::contract::{
    Answer, EVENT_MAX_BYTES, EVENTS_KEPT_LIMIT, Event, Executor, Journal, MESSAGES_KEPT_LIMIT,
    MESSAGES_KEPT_MAX_BYTES, Produced, RunId, Secrets, journal_bytes, read_journal,
};
use whizbang_core::contract::{JournalLine, record::JOURNAL_NAME};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::scratch::Scratch;

use super::watch::{asked, engine, work};

/// Runs `work` with this body to its end, and reads the journal its run stored.
fn stored(
    label: &str,
    body: impl Fn(&dyn whizbang_core::contract::Events) + Send + Sync + 'static,
) -> (Journal, Vec<u8>) {
    let dir = Scratch::new(label);
    let root = dir.path().join("store");
    let store = Arc::new(Store::open(&root, Arc::new(ShimFs::new())).unwrap());
    let executor = engine(
        store,
        vec![work(move |_, context| {
            body(context.events());
            Ok(Produced::nothing())
        })],
    );
    let Ok(Answer::Ran(finished)) = executor.request(&asked("order-journal")) else {
        panic!("the request did not run");
    };
    let run: &RunId = finished.run();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation
        .run(run.as_str())
        .unwrap()
        .get(JOURNAL_NAME)
        .unwrap();
    let bytes = reader.read(entry).unwrap();
    (Journal::read(&bytes).unwrap(), bytes)
}

#[test]
fn a_journal_of_the_older_format_is_read() {
    // As every run wrote it before typed events: a line for each event, in order from
    // 0, and nothing else. It reads as it did, and a journal of messages alone with
    // nothing left out is still written so, byte for byte.
    let older = concat!(
        r#"{"seq":0,"at_ms":1001,"message":"echo: hello"}"#,
        "\n",
        r#"{"seq":1,"at_ms":1002,"message":"echo: done"}"#,
        "\n"
    );
    let journal = Journal::read(older.as_bytes()).unwrap();
    let said: Vec<(u64, u64, &str, bool)> = (journal.lines().iter())
        .map(|l| (l.seq(), l.at_ms(), l.message(), l.event().is_some()))
        .collect();
    assert_eq!(
        said,
        [
            (0, 1001, "echo: hello", false),
            (1, 1002, "echo: done", false)
        ]
    );
    assert_eq!(
        journal.not_kept().messages() + journal.not_kept().events(),
        0
    );
    assert_eq!(journal.to_bytes(), older.as_bytes());
    assert_eq!(journal_bytes(journal.lines()), older.as_bytes());
    assert_eq!(read_journal(older.as_bytes()).unwrap(), journal.lines());
    // A typed line has no place in that format, and the newer one is read whole.
    let typed = JournalLine::typed(
        0,
        5,
        &Event::AttemptStarted {
            number: 1,
            changed: None,
        },
        &Secrets::none(),
    );
    let newer = journal_bytes(std::slice::from_ref(&typed));
    assert_eq!(
        String::from_utf8(newer.clone()).unwrap(),
        concat!(
            "{\"journal\":2}\n",
            r#"{"seq":0,"at_ms":5,"message":"attempt 1 started","event":{"kind":"attempt_started","number":1}}"#,
            "\n",
            r#"{"not_kept":{"messages":0,"events":0}}"#,
            "\n"
        )
    );
    assert_eq!(read_journal(&newer).unwrap(), std::slice::from_ref(&typed));
    let mut torn = typed_line_text(&typed);
    torn.push('\n');
    assert!(Journal::read(torn.as_bytes()).is_err());
}

fn typed_line_text(line: &JournalLine) -> String {
    let bytes = journal_bytes(std::slice::from_ref(line));
    let text = String::from_utf8(bytes).unwrap();
    text.lines().nth(1).unwrap().to_owned()
}

#[test]
fn past_its_bound_a_run_keeps_its_first_messages_and_counts_the_rest() {
    // Four more messages than a run keeps: the first are kept, the rest counted, and
    // the journal's last line says how many.
    let (journal, bytes) = stored("journal-count", |events| {
        for n in 0..MESSAGES_KEPT_LIMIT + 4 {
            events.progress(&format!("message {n}"));
        }
    });
    assert_eq!(journal.lines().len(), MESSAGES_KEPT_LIMIT);
    assert_eq!(journal.lines()[0].message(), "message 0");
    let last = journal.lines().last().unwrap();
    assert_eq!(
        last.message(),
        format!("message {}", MESSAGES_KEPT_LIMIT - 1)
    );
    assert_eq!(
        (journal.not_kept().messages(), journal.not_kept().events()),
        (4, 0)
    );
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(
        text.lines().last(),
        Some(r#"{"not_kept":{"messages":4,"events":0}}"#)
    );

    // And by their text: the message that would pass the bound is not kept, nor any
    // after it, a short one among them.
    let long = "x".repeat(4000);
    let (journal, _) = stored("journal-bytes", move |events| {
        for _ in 0..300 {
            events.progress(&long);
        }
        events.progress("short");
    });
    let kept = MESSAGES_KEPT_MAX_BYTES / 4000;
    assert_eq!(journal.lines().len(), kept);
    assert_eq!(journal.not_kept().messages(), (301 - kept) as u64);
}

#[test]
fn typed_events_past_their_bound_are_counted_and_the_journal_says_so() {
    // One more typed event than a run keeps, and each kept is at most its bound however
    // long what it says; progress messages are counted on their own.
    let (journal, _) = stored("journal-typed", |events| {
        events.progress("began");
        for n in 0..=EVENTS_KEPT_LIMIT {
            events.event(&Event::StepStarted {
                step: format!("s{n}"),
                action: "a".repeat(2000),
                attempt: Some(1),
            });
        }
        events.progress("ended");
    });
    assert_eq!(journal.lines().len(), EVENTS_KEPT_LIMIT + 2);
    assert_eq!(
        (journal.not_kept().messages(), journal.not_kept().events()),
        (0, 1)
    );
    let last = journal.lines().last().unwrap();
    assert_eq!(
        (last.message(), last.seq()),
        ("ended", EVENTS_KEPT_LIMIT as u64 + 2)
    );
    let line = &journal.lines()[1];
    let Some(Event::StepStarted { step, action, .. }) = line.event() else {
        panic!("{line:?}");
    };
    assert_eq!(step, "s0");
    assert!(action.len() < 2000);
    let written = serde_json::to_vec(&(line.message(), line.event().unwrap())).unwrap();
    assert!(written.len() <= EVENT_MAX_BYTES, "{}", written.len());
}
