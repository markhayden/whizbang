//! A run's place for facts (ADR 0013, "A run's place for facts"): a setting, a paid
//! request, a refusal of a request and where data went are noted from outside the
//! executor while a run executes, and written in its record when it ends.

use std::sync::{Arc, Mutex};

use whizbang_core::capability::Price;
use whizbang_core::contract::record::{
    EGRESS_LIMIT, Handed, PAID_LIMIT, Paid, REFUSALS_LIMIT, SETTINGS_LIMIT, Status,
};
use whizbang_core::contract::{
    Answer, Call, Cancellation, Capabilities, Capability, Context, Executor, Failure, Produced,
    RECORD_MAX_BYTES, RefusalKind, Request, Requirement, RunFacts, Secrets, Value,
};
use whizbang_core::egress::{Account, Classes, DataClass, Origin};
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};

use super::asked::{action, engine, fill_request, stored};
use super::commit::World;
use super::redact::made_up_key;

fn origin(address: &str) -> Origin {
    Origin::of(address, &Secrets::none()).unwrap()
}

fn account(text: &str) -> Account {
    Account::new(text, &Secrets::none()).unwrap()
}

fn prompts() -> Classes {
    Classes::of(&[DataClass::Prompts])
}

/// A filler that states a bound of 500 and reports a charge of 120 with each answer.
fn filler() -> Arc<FakeFiller> {
    let price = Price::at_most(500.0).unwrap();
    Arc::new(FakeFiller::new(Paint::Inverse).priced(price).charging(120))
}

/// One fact of each kind, noted as another module would note it. Each is taken.
fn note_one_of_each(context: &Context) {
    let facts = context.facts();
    assert!(facts.setting("backends.fill.model", &Value::text("bria-erase")));
    assert!(facts.paid(Paid::answered(0, 500, 120).unwrap()));
    let to = origin("https://api.example.com/v1");
    let who = account("acct_4f8a1c9e77");
    assert!(facts.went(&to, Some(&who), prompts(), Handed::Sent));
    assert!(facts.refused(
        RefusalKind::RequestNotApproved,
        "the recipient is not approved for source images"
    ));
}

#[test]
fn a_fact_of_each_kind_noted_inside_a_run_is_in_its_record_however_the_run_ended() {
    for how in ["completes", "fails", "is cancelled", "panics"] {
        let world = World::new();
        let filler = filler();
        let signal = Cancellation::new();
        let notes = {
            let signal = signal.clone();
            let needs = vec![Requirement::new(Capability::Fill)];
            action("notes", needs, move |_, context| {
                let fill = context.capabilities().fill().unwrap();
                fill.fill(&fill_request()).unwrap();
                note_one_of_each(context);
                match how {
                    "fails" => Err(Failure::error("made", "the test failed it")),
                    "is cancelled" => {
                        signal.cancel();
                        Ok(Produced::nothing())
                    }
                    "panics" => panic!("the test panicked it"),
                    _ => Ok(Produced::nothing()),
                }
            })
        };
        let executor = engine(
            &world,
            roster(None, Some(filler.as_ref()), None),
            Capabilities::none().with_fill(filler.clone()),
            vec![notes],
        );
        let request = Request::new(Call::new("notes")).cancelled_by(signal);
        let Ok(Answer::Ran(finished)) = executor.request(&request) else {
            panic!("{how}: the request did not run");
        };
        let (record, _) = stored(&world, finished.run());
        let status = match how {
            "completes" => Status::Completed,
            "is cancelled" => Status::Cancelled,
            _ => Status::Error,
        };
        assert_eq!(record.outcome().status, status, "{how}");
        let settings = &record.asked().settings;
        assert_eq!(settings.len(), 1, "{how}");
        assert_eq!(settings[0].name(), "backends.fill.model", "{how}");
        let paid = &record.spend().requests;
        assert_eq!(paid.len(), 1, "{how}");
        assert_eq!(
            (
                paid[0].request(),
                paid[0].reserved_micros(),
                paid[0].settled_micros()
            ),
            (0, 500, 120),
            "{how}"
        );
        let went = record.egress();
        assert_eq!(went.len(), 1, "{how}");
        assert_eq!(
            (
                went[0].origin(),
                went[0].account(),
                went[0].classes(),
                went[0].outcome()
            ),
            (
                "https://api.example.com",
                Some("acct..."),
                prompts(),
                Handed::Sent
            ),
            "{how}"
        );
        let refusals = &record.outcome().refusals;
        assert_eq!(refusals.len(), 1, "{how}");
        assert_eq!(
            (refusals[0].action(), refusals[0].kind()),
            (None, RefusalKind::RequestNotApproved),
            "{how}"
        );
        assert!(record.outcome().lost.is_empty(), "{how}");
    }
}

#[test]
fn a_refusal_of_a_request_is_in_the_runs_refusals_within_their_bound() {
    // The action catches each refusal and completes: 300 of them, of which a record
    // lists 256 and counts the rest, as it does for refused calls (ADR 0008).
    let world = World::new();
    let notes = action("notes", Vec::new(), |_, context| {
        for at in 0..300 {
            let why = format!("request {at} has no bound");
            assert!(context.facts().refused(RefusalKind::RequestNoBound, &why));
        }
        // A kind that is of a call is not one a request is refused with.
        assert!(!context.facts().refused(RefusalKind::NotReady, "not ready"));
        Ok(Produced::nothing())
    });
    let executor = engine(
        &world,
        roster(None, None, None),
        Capabilities::none(),
        vec![notes],
    );
    let finished = executor.submit(&Call::new("notes")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), REFUSALS_LIMIT);
    assert_eq!(
        record.outcome().refusals_omitted,
        300 - REFUSALS_LIMIT as u64
    );
    assert_eq!(refusals[0].why(), "request 0 has no bound");
    assert_eq!(refusals[255].why(), "request 255 has no bound");
    assert!(
        refusals
            .iter()
            .all(|r| r.kind() == RefusalKind::RequestNoBound)
    );
}

#[test]
fn an_account_and_a_settings_value_are_redacted_when_they_are_noted() {
    let world = World::new();
    let key = made_up_key("Z");
    let seen: Arc<Mutex<Vec<bool>>> = Arc::default();
    let notes = {
        let (key, seen) = (key.clone(), seen.clone());
        action("notes", Vec::new(), move |_, context| {
            let facts = context.facts();
            let to = origin("https://api.example.com");
            let holding = account(&format!("acct_{key}"));
            let taken = [
                facts.setting("backends.fill.note", &Value::text(&format!("see {key}"))),
                // A name is written as it is: one that holds the key is not taken.
                facts.setting(&format!("backends.{key}"), &Value::Integer(1)),
                facts.went(&to, Some(&holding), prompts(), Handed::Sent),
                facts.refused(
                    RefusalKind::RequestNoBound,
                    &format!("the key {key} was used"),
                ),
            ];
            seen.lock().unwrap().extend(taken);
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        &world,
        roster(None, None, None),
        Capabilities::none(),
        vec![notes],
    )
    .with_secrets(Secrets::new([key.clone()]).unwrap());
    let finished = executor.submit(&Call::new("notes")).unwrap();
    assert_eq!(*seen.lock().unwrap(), [true, false, true, true]);
    let (record, names) = stored(&world, finished.run());
    let written = String::from_utf8(names["record"].clone()).unwrap();
    assert!(
        !written.contains(&key) && !written.contains(&key[..12]),
        "{written}"
    );
    assert!(written.contains(r#"{"name":"backends.fill.note","value":{"text":"see [redacted]"}}"#));
    assert_eq!(record.asked().settings.len(), 1);
    // The account held the key: the entry says where the data went and names none.
    assert_eq!(record.egress()[0].account(), None);
    assert_eq!(
        record.outcome().refusals[0].why(),
        "the key [redacted] was used"
    );
}

#[test]
fn a_note_made_after_the_runs_end_began_is_dropped_and_changes_no_record() {
    let world = World::new();
    let kept: Arc<Mutex<Option<RunFacts>>> = Arc::default();
    let notes = {
        let kept = kept.clone();
        action("notes", Vec::new(), move |_, context| {
            assert!(context.facts().setting("during", &Value::Integer(1)));
            *kept.lock().unwrap() = Some(context.facts().clone());
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        &world,
        roster(None, None, None),
        Capabilities::none(),
        vec![notes],
    );
    let finished = executor.submit(&Call::new("notes")).unwrap();
    let (_, before) = stored(&world, finished.run());
    // The run is over, and something still holds its place for facts.
    let late = kept.lock().unwrap().clone().unwrap();
    let to = origin("https://api.example.com");
    assert!(!late.setting("after", &Value::Integer(2)));
    assert!(!late.paid(Paid::not_sent(0, 1).unwrap()));
    assert!(!late.went(&to, None, prompts(), Handed::Sent));
    assert!(!late.refused(RefusalKind::RequestNoBound, "too late"));
    let (record, after) = stored(&world, finished.run());
    assert_eq!(before, after);
    let names: Vec<&str> = record.asked().settings.iter().map(|s| s.name()).collect();
    assert_eq!(names, ["during"]);
    assert!(record.outcome().refusals.is_empty() && record.egress().is_empty());
}

/// An action that sends `requests` fills and notes as many facts as a record lists of
/// each kind, and one more of each, which is not taken.
fn fills_every_list(requests: usize) -> Arc<dyn whizbang_core::contract::Action> {
    action(
        "fills",
        vec![Requirement::new(Capability::Fill)],
        move |_, context| {
            let (facts, fill) = (context.facts(), context.capabilities().fill().unwrap());
            let text = Value::text(&"\u{1}".repeat(256));
            for at in 0..SETTINGS_LIMIT {
                let name = format!("{at:03}{}", "n".repeat(125));
                assert!(facts.setting(&name, &text), "{at}");
            }
            assert!(!facts.setting("one-more", &text));
            for at in 0..requests {
                fill.fill(&fill_request()).unwrap();
                assert!(
                    facts.paid(Paid::unresolved(at, u64::MAX - 1).unwrap()),
                    "{at}"
                );
            }
            let label = "a".repeat(63);
            let host = |at: usize| format!("{label}.{label}.{label}.{at:03}{}", "b".repeat(58));
            for at in 0..EGRESS_LIMIT {
                let to = origin(&format!("https://{}:65535", host(at)));
                assert!(
                    facts.went(&to, None, Classes::all(), Handed::Refused),
                    "{at}"
                );
            }
            let more = origin(&format!("https://{}:65535", host(EGRESS_LIMIT)));
            assert!(!facts.went(&more, None, Classes::all(), Handed::Refused));
            Ok(Produced::nothing())
        },
    )
}

#[test]
fn a_run_that_fills_every_new_list_to_its_bound_has_a_record_under_the_limit() {
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![fills_every_list(PAID_LIMIT)],
    );
    let finished = executor.submit(&Call::new("fills")).unwrap();
    // `stored` reads the record back, holds it to the schema and to the real limit.
    let (record, names) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(record.asked().settings.len(), SETTINGS_LIMIT);
    assert_eq!(record.spend().requests.len(), PAID_LIMIT);
    assert_eq!(record.models().len(), PAID_LIMIT);
    assert_eq!(record.egress().len(), EGRESS_LIMIT);
    let bytes = names["record"].len() as u64;
    assert!((500_000..900_000).contains(&bytes), "{bytes}");
    assert!(bytes < RECORD_MAX_BYTES / 4);
}

#[test]
fn with_the_limit_set_low_a_run_whose_facts_take_its_record_over_ends_as_an_error_that_lists_none()
{
    // The limit is lowered through the executor's seam for tests. A record that would
    // be over it is not written as it is: the run ends as an error of the type
    // `record`, with a record that leaves out the requests and what was noted, says
    // how many there were of each, and is read back.
    let world = World::new();
    let filler = Arc::new(FakeFiller::new(Paint::Inverse));
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![fills_every_list(2)],
    )
    .with_record_limit(20_000);
    let finished = executor.submit(&Call::new("fills")).unwrap();
    let (record, names) = stored(&world, finished.run());
    assert!(names["record"].len() < 20_000, "{}", names["record"].len());
    assert_eq!(record.outcome().status, Status::Error);
    let error = record.outcome().error.as_ref().unwrap();
    assert_eq!(error.kind, "record");
    assert!(
        error.message.starts_with(
            "the run sent 2 capability requests, and what was noted of it is 96 settings, 2 \
             paid requests and 128 entries of where data went"
        ),
        "{}",
        error.message
    );
    assert!(record.asked().settings.is_empty() && record.egress().is_empty());
    assert!(record.spend().requests.is_empty() && record.models().is_empty());
}

#[test]
fn paid_requests_settled_out_of_order_are_all_listed_in_the_order_they_were_sent() {
    // Requests are settled as their answers come, which need not be the order they
    // were sent in. Whatever order they are noted in, none is lost: the record lists
    // each, in the order of the requests. One noted twice is noted once.
    let world = World::new();
    let filler = filler();
    let notes = action(
        "notes",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            let (facts, fill) = (context.facts(), context.capabilities().fill().unwrap());
            for _ in 0..3 {
                fill.fill(&fill_request()).unwrap();
            }
            assert!(facts.paid(Paid::answered(2, 500, 300).unwrap()));
            assert!(facts.paid(Paid::answered(0, 500, 100).unwrap()));
            assert!(facts.paid(Paid::unresolved(1, 500).unwrap()));
            assert!(!facts.paid(Paid::answered(1, 500, 1).unwrap()));
            Ok(Produced::nothing())
        },
    );
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![notes],
    );
    let finished = executor.submit(&Call::new("notes")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    let listed: Vec<(u64, u64)> = record
        .spend()
        .requests
        .iter()
        .map(|paid| (paid.request(), paid.settled_micros()))
        .collect();
    assert_eq!(listed, [(0, 100), (1, 500), (2, 300)]);
}

#[test]
fn a_paid_request_noted_for_a_request_the_run_did_not_send_ends_the_run_as_an_error_that_says_so() {
    // A record lists a paid request only for a request it lists. One that was noted
    // for a request the run never sent cannot be written, and is not dropped in
    // silence, which would be a record that says less was spent: the run ends as an
    // error that says so, whatever its action handed back.
    let world = World::new();
    let filler = filler();
    let notes = action(
        "notes",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            let (facts, fill) = (context.facts(), context.capabilities().fill().unwrap());
            fill.fill(&fill_request()).unwrap();
            assert!(facts.paid(Paid::answered(0, 500, 120).unwrap()));
            // The run sent one request, which is request 0: there is no request 1,
            // the first place past those it sent.
            assert!(facts.paid(Paid::answered(1, 500, 400).unwrap()));
            Ok(Produced::nothing())
        },
    );
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![notes],
    );
    let finished = executor.submit(&Call::new("notes")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Error);
    let error = record.outcome().error.as_ref().unwrap();
    assert_eq!(error.kind, "record");
    assert_eq!(
        error.message,
        "what a request cost was noted for request 1, and the run sent 1 capability \
         requests: a record lists what a request cost only for a request it lists, so 1 \
         of the 2 that were noted is not in this record"
    );
    // What could be written is written: the request the run did send.
    let listed: Vec<u64> = record
        .spend()
        .requests
        .iter()
        .map(|p| p.request())
        .collect();
    assert_eq!(listed, [0]);
    assert_eq!(record.models().len(), 1);
}
