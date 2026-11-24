//! A request's wait, its deadline and its cancel (ADR 0046, "The kind `remote`"): the
//! wait for the answer is the request's own and not the client's thirty seconds; a
//! request given up on is told to stop through a checked control send of its own, which
//! resolves no name and is let go of at its own deadline.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::json;
use whizbang_backends::net::testing::Pki;
use whizbang_backends::pack::proto::{self, Answer, Message, Open};
use whizbang_core::capability::{FaultKind, Stage};
use whizbang_core::contract::Cancellation;

use super::scripted::{
    Step, chunk, chunked, frames, hello, outcome, record, scripted, verified_head,
};
use super::wire::{charged_at_its_bound, failed};
use super::{Asking, Host, Stepping, named, phrase, request, scripted_digest};

/// How long a cancel has in these tests, in place of five seconds.
const CANCEL_WITHIN: Duration = Duration::from_millis(300);

/// The answer of a cancel: 204, as the front's.
fn no_content() -> Step {
    Step::Answer(Box::new(|_| {
        b"HTTP/1.1 204 No Content\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".to_vec()
    }))
}

/// A host that answers with its head and its worker's hello, and then says nothing
/// until the client closes the connection.
fn hello_then_silence() -> Step {
    Step::AnswerAndHold(Box::new(|seen| {
        let mut bytes = verified_head(&scripted_digest());
        bytes.extend(chunk(&hello(seen)));
        bytes
    }))
}

/// The two frames of a cancel's body: whizbang's hello and the `cancel` of the call.
fn cancel_frames(body: &[u8]) -> (Message, Message) {
    let mut input = body;
    let mut open = Open::new();
    let hello = proto::read(&mut input, &mut open).unwrap().unwrap();
    let cancel = proto::read(&mut input, &mut open).unwrap().unwrap();
    assert!(input.is_empty(), "nothing after the cancel");
    (hello, cancel)
}

#[test]
fn a_cold_start_a_silent_load_and_a_silent_call_past_30_seconds_complete_inside_the_requests_deadline()
 {
    let pki = Arc::new(Pki::loopback());
    let gap = Duration::from_millis(150);
    let result = |seen: &super::scripted::Seen| {
        let _ = seen;
        frames(&[Message::Result {
            id: "r2".to_owned(),
            device: "cuda".to_owned(),
            threads: 3,
            diagnostics: String::new(),
            answer: Answer::Segment(Vec::new()),
        }])
    };
    let front = scripted(
        &pki,
        vec![Step::Paced(vec![
            // The cold start: nothing but the head until the worker's hello.
            (
                Duration::ZERO,
                Box::new(|_| verified_head(&scripted_digest())),
            ),
            (gap, Box::new(|seen| chunk(&hello(seen)))),
            // A silent load.
            (
                gap,
                Box::new(|_| {
                    chunk(&frames(&[Message::Result {
                        id: "r1".to_owned(),
                        device: "cuda".to_owned(),
                        threads: 3,
                        diagnostics: String::new(),
                        answer: Answer::Load,
                    }]))
                }),
            ),
            // A silent call, then the result, the record and the end.
            (
                gap,
                Box::new(move |seen| {
                    let record =
                        record(&outcome("complete", None, "answered", json!(null), 0), &[]);
                    let mut bytes = chunk(&result(seen));
                    bytes.extend(chunked(&[&record]));
                    bytes
                }),
            ),
        ])],
    );
    // A clock that moves two seconds at each read: each silence is well past the
    // client's thirty seconds between two reads, and all of them inside the request's
    // own hour.
    let clock = Stepping::by(2_000);
    let mut asking = Asking::scripted(&front, &pki);
    asking.clock = clock.clone();
    asking.host_limit = Duration::from_secs(3_600);
    let answered = asking.ask(&named(None, "fake-ok"), &request(phrase(), 1));
    let answered = answered.unwrap_or_else(|failed| panic!("{failed:?}"));
    assert_eq!(answered.called.device, "cuda");
    assert!(
        clock.moved() > Duration::from_secs(90),
        "{:?}",
        clock.moved()
    );
}

#[test]
fn a_host_silent_past_the_requests_deadline_is_timed_out_and_charged_at_its_bound() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(&pki, vec![hello_then_silence(), no_content()]);
    // The request's own wait, which the kind sets from its host's limit, here short.
    let mut asking = Asking::scripted(&front, &pki);
    asking.host_limit = Duration::from_millis(300);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::TimedOut, "{failed:?}");
    charged_at_its_bound(&failed);
    // Its host was told to stop.
    let seen = front.seen();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(
        seen[1].line.starts_with("POST /pack/1/cancel "),
        "{:?}",
        seen[1].line
    );
}

#[test]
fn a_cancel_sends_the_cancel_frame_and_closes_the_stream() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(&pki, vec![hello_then_silence(), no_content()]);
    let asking = Asking::scripted(&front, &pki);
    let cancellation = Cancellation::new();
    let failed = whizbang_testkit::threads::scope(|scope| {
        let signal = cancellation.clone();
        let seen = front.seen.clone();
        let watcher = scope.spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while seen.lock().unwrap().is_empty() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(5));
            }
            signal.cancel();
        });
        let answered = asking.ask_under(
            &named(None, "fake-ok"),
            &request(phrase(), 1),
            cancellation.clone(),
            None,
        );
        watcher.join().unwrap();
        failed(answered)
    });
    assert_eq!(failed.fault.kind(), FaultKind::Cancelled, "{failed:?}");
    charged_at_its_bound(&failed);
    let seen = front.seen();
    assert_eq!(seen.len(), 2, "{seen:?}");
    let (request, cancel) = (&seen[0], &seen[1]);
    assert!(cancel.line.starts_with("POST /pack/1/cancel "));
    // The cancel names the request's dispatch, carries the same token, and no user data:
    // whizbang's hello and the `cancel` of the call.
    assert_eq!(
        cancel.field("whizbang-dispatch"),
        request.field("whizbang-dispatch")
    );
    assert_eq!(
        cancel.field("authorization"),
        request.field("authorization")
    );
    let (hello, frame) = cancel_frames(&cancel.body);
    assert!(matches!(hello, Message::Hello { id, .. } if id == request.conversation));
    assert_eq!(
        frame,
        Message::Cancel {
            id: "r2".to_owned()
        }
    );
    // Both connections ended: the stream was closed, and the cancel answered.
    assert_eq!(front.listener.finish(), 2);
}

#[test]
fn a_cancel_is_sent_after_the_runs_signal_is_set_and_after_its_deadline_has_passed() {
    let model = named(None, "fake-wait");
    let host = Host::serving(std::slice::from_ref(&model));
    let asking = Asking::of(&host);
    // At its deadline: the fake waits until it is cancelled, and the front's limit of a
    // call is longer than the deadline.
    let deadline = Instant::now() + Duration::from_millis(300);
    let timed_out = failed(asking.ask_under(
        &model,
        &request(phrase(), 1),
        Cancellation::new(),
        Some(deadline),
    ));
    assert_eq!(timed_out.fault.kind(), FaultKind::TimedOut, "{timed_out:?}");
    charged_at_its_bound(&timed_out);
    // At its signal, set once the host's side began the request.
    let cancellation = Cancellation::new();
    let cancelled = whizbang_testkit::threads::scope(|scope| {
        let signal = cancellation.clone();
        let host = &host;
        let watcher = scope.spawn(move || {
            assert!(host.waits_for(|host| host.seen().len() == 2));
            signal.cancel();
        });
        let answered = asking.ask_under(&model, &request(phrase(), 1), cancellation.clone(), None);
        watcher.join().unwrap();
        failed(answered)
    });
    assert_eq!(
        cancelled.fault.kind(),
        FaultKind::Cancelled,
        "{cancelled:?}"
    );
    charged_at_its_bound(&cancelled);
    // The front's side was handed a cancel of each request, by its dispatch id.
    let seen: Vec<String> = host.seen().into_keys().collect();
    assert!(host.waits_for(|host| host.cancelled().len() == 2));
    assert_eq!(host.cancelled(), seen);
}

#[test]
fn a_host_that_never_answers_the_cancel_is_let_go_of_at_its_own_deadline() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(&pki, vec![hello_then_silence(), Step::Mute]);
    let mut asking = Asking::scripted(&front, &pki);
    asking.cancel_within = Some(CANCEL_WITHIN);
    let deadline = Instant::now() + Duration::from_millis(200);
    let started = Instant::now();
    let failed = failed(asking.ask_under(
        &named(None, "fake-ok"),
        &request(phrase(), 1),
        Cancellation::new(),
        Some(deadline),
    ));
    assert_eq!(failed.fault.kind(), FaultKind::TimedOut, "{failed:?}");
    charged_at_its_bound(&failed);
    // The request's 200 ms, the cancel's 300, and no more than a moment beside them.
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(front.seen().len(), 2, "the cancel was sent");
}

#[test]
fn the_cancel_resolves_no_name_and_ends_in_its_deadline_under_a_resolver_that_stalls() {
    let pki = Arc::new(Pki::for_name(7, "front.test"));
    let front = scripted(&pki, vec![hello_then_silence(), no_content()]);
    let port = front.listener.port();
    // The first look resolves the name to loopback; every one after it stalls.
    let looks = Arc::new(AtomicUsize::new(0));
    let (_hold, stalled) = mpsc::channel::<()>();
    let stalled = Arc::new(std::sync::Mutex::new(stalled));
    let counted = looks.clone();
    let resolve: whizbang_backends::net::testing::Resolve = Arc::new(move |_, _| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(vec![SocketAddr::from(([127, 0, 0, 1], port))]);
        }
        let _ = stalled
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10));
        Err(std::io::Error::other("stalled"))
    });
    let mut records = std::collections::BTreeMap::new();
    let model = named(None, "fake-ok");
    records.insert(model.clone(), super::host::record(&model).0);
    let mut asking = Asking::at(&format!("https://front.test:{port}"), pki.clone(), records);
    asking.resolve = Some(resolve);
    asking.cancel_within = Some(CANCEL_WITHIN);
    let started = Instant::now();
    let deadline = started + Duration::from_millis(200);
    let failed = failed(asking.ask_under(
        &model,
        &request(phrase(), 1),
        Cancellation::new(),
        Some(deadline),
    ));
    assert_eq!(failed.fault.kind(), FaultKind::TimedOut, "{failed:?}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // One look, the request's; the cancel connected to the address it reached.
    assert_eq!(looks.load(Ordering::SeqCst), 1);
    let seen = front.seen();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(
        seen[1].head.contains("Host: front.test:"),
        "{}",
        seen[1].head
    );
}

#[test]
fn the_cancel_ends_in_its_deadline_on_a_stalled_connect_handshake_or_answer() {
    let model = named(None, "fake-ok");
    // A connect that stalls for as long as it is let, the second time it is asked.
    let pki = Arc::new(Pki::loopback());
    let front = scripted(&pki, vec![hello_then_silence()]);
    let dials = Arc::new(AtomicUsize::new(0));
    let counted = dials.clone();
    let dial: whizbang_backends::net::testing::Dial = Arc::new(move |address, limit| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            return std::net::TcpStream::connect_timeout(address, limit);
        }
        let (_hold, never) = mpsc::channel::<()>();
        let _ = never.recv_timeout(limit);
        Err(std::io::Error::from(std::io::ErrorKind::TimedOut))
    });
    let mut asking = Asking::scripted(&front, &pki);
    asking.dial = Some(dial);
    asking.cancel_within = Some(CANCEL_WITHIN);
    let started = Instant::now();
    let deadline = started + Duration::from_millis(200);
    let failed_connect = failed(asking.ask_under(
        &model,
        &request(phrase(), 1),
        Cancellation::new(),
        Some(deadline),
    ));
    assert_eq!(failed_connect.fault.kind(), FaultKind::TimedOut);
    assert_eq!(dials.load(Ordering::SeqCst), 2);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );

    // A handshake the host never answers, and an answer it never gives.
    for cancel in [Step::Silent, Step::Mute] {
        let front = scripted(&pki, vec![hello_then_silence(), cancel]);
        let mut asking = Asking::scripted(&front, &pki);
        asking.cancel_within = Some(CANCEL_WITHIN);
        let started = Instant::now();
        let deadline = started + Duration::from_millis(200);
        let failed = failed(asking.ask_under(
            &model,
            &request(phrase(), 1),
            Cancellation::new(),
            Some(deadline),
        ));
        assert_eq!(failed.fault.kind(), FaultKind::TimedOut, "{failed:?}");
        assert_eq!(failed.fault.stage(), Stage::Unresolved);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(front.listener.accepted(), 2);
    }
}
