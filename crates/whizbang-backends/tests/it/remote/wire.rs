//! What crosses the wire between the kind and a front: the pack's own (the fake host),
//! or a scripted one for the answers the pack's front never gives.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use whizbang_backends::net::testing::Pki;
use whizbang_backends::pack::proto::{Answer, Message, Scalar};
use whizbang_backends::pack::{Ended, Failed};
use whizbang_backends::remote::Answered;
use whizbang_core::capability::{FaultKind, Picture, Query, SegmentRequest, Stage};
use whizbang_core::contract::Secrets;
use whizbang_core::egress::SendCheck;
use whizbang_core::identity::CanonicalPixels;

use super::host::{FAKES_TOKEN, TOKEN};
use super::scripted::{
    self, Step, chunk, chunked, frames, hello, identity, keep_alive, outcome, record, scripted,
    unverified, verified, verified_head,
};
use super::{Asking, Host, Stepping, named, phrase, request, scripted_digest};

/// The failure, or a panic naming what was answered.
pub(super) fn failed(answered: Result<Answered, Box<Failed>>) -> Box<Failed> {
    match answered {
        Ok(answered) => panic!("answered: {:?}", answered.called),
        Err(failed) => failed,
    }
}

/// Charged at its bound: the request left, so its fault is unresolved.
pub(super) fn charged_at_its_bound(failed: &Failed) {
    assert_eq!(failed.fault.stage(), Stage::Unresolved, "{failed:?}");
}

/// A load's result, as the front hands it on.
fn loaded() -> Vec<u8> {
    frames(&[Message::Result {
        id: "r1".to_owned(),
        device: "cpu".to_owned(),
        threads: 3,
        diagnostics: String::new(),
        answer: Answer::Load,
    }])
}

// --- the answer's identity ----------------------------------------------------------------------

#[test]
fn an_answer_with_no_malformed_or_another_identity_is_refused_and_not_kept() {
    let pki = Arc::new(Pki::loopback());
    // Each answer is the whole of a good one but for its digest.
    let good = |digest: Option<&'static str>| -> Step {
        Step::Answer(Box::new(move |seen| {
            let mut head = match digest {
                Some(digest) => verified_head(digest),
                None => scripted::head(),
            };
            let body = [
                hello(seen),
                loaded(),
                record(&outcome("complete", None, "answered", json!(null), 0), &[]),
            ]
            .concat();
            head.extend(chunked(&[&body]));
            head
        }))
    };
    let other = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let front = scripted(
        &pki,
        vec![good(None), good(Some("sha256:not-hex")), good(Some(other))],
    );
    let asking = Asking::scripted(&front, &pki);
    let model = named(None, "fake-ok");
    for (case, kind) in [
        ("none", FaultKind::Malformed),
        ("malformed", FaultKind::Malformed),
        ("another", FaultKind::Rejected),
    ] {
        let failed = failed(asking.ask(&model, &request(phrase(), 1)));
        assert_eq!(failed.fault.kind(), kind, "{case}: {failed:?}");
        assert!(failed.fault.sentence().contains("refused whole"), "{case}");
        charged_at_its_bound(&failed);
        // Nothing of it is kept: no answer, no error output, no sentence of the worker.
        assert!(
            failed.error_output.is_empty() && failed.said.is_none(),
            "{case}"
        );
    }
    assert_eq!(front.listener.finish(), 3);
}

#[test]
fn an_answer_with_frames_and_no_verified_identity_is_refused_whole() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|seen| {
            let failure = record(
                &outcome("failed", Some("unavailable"), "not_sent", json!(null), 0),
                &[],
            );
            unverified(&[hello(seen), failure].concat())
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::Malformed, "{failed:?}");
    assert!(
        failed
            .fault
            .sentence()
            .contains("a frame before its identity record")
    );
    charged_at_its_bound(&failed);
}

// --- an answer kept alive -----------------------------------------------------------------------

#[test]
fn a_cold_host_keeps_its_answer_alive_past_the_clients_limit_between_two_reads() {
    // The pack's own front, its side as slow as a card that is starting: 700 ms before
    // it checks the files and starts its worker, while the front sends a keep-alive after
    // every 20 ms of nothing. The client's limit between two reads, the request's own
    // wait, is 250 ms: the head and the keep-alives hold the answer open past it.
    let model = named(None, "fake-ok");
    let slow = Duration::from_millis(700);
    let host = Host::cold(
        std::slice::from_ref(&model),
        slow,
        Duration::from_millis(20),
    );
    let mut asking = Asking::of(&host);
    asking.host_limit = Duration::from_millis(250);
    let began = Instant::now();
    let answered = asking.ask(&model, &request(phrase(), 1));
    let answered = answered.unwrap_or_else(|failed| panic!("{failed:?}"));
    assert!(began.elapsed() > slow, "{:?}", began.elapsed());
    assert_eq!(answered.digest, asking.digest_of(&model));
    assert_eq!(answered.called.device, "cpu");
}

#[test]
fn keep_alives_anywhere_before_the_outcome_record_are_passed_over() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|seen| {
            let error = Message::Error {
                id: "r2".to_owned(),
                fault: FaultKind::Rejected,
                sentence: "the adapter refused the request".to_owned(),
            };
            let done = record(
                &outcome("failed", Some("rejected"), "answered", json!(null), 0),
                &[],
            );
            let mut bytes = scripted::head();
            for piece in [
                keep_alive(),
                keep_alive(),
                identity(&scripted_digest()),
                keep_alive(),
                hello(seen),
                keep_alive(),
                loaded(),
                frames(&[error]),
                keep_alive(),
            ] {
                bytes.extend(chunk(&piece));
            }
            bytes.extend(chunked(&[&done]));
            bytes
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::Rejected, "{failed:?}");
    assert_eq!(
        failed.said.as_deref(),
        Some("the adapter refused the request")
    );
    charged_at_its_bound(&failed);
}

#[test]
fn an_identity_out_of_place_or_keep_alives_past_their_most_are_refused() {
    let pki = Arc::new(Pki::loopback());
    let answer = |pieces: fn(&super::scripted::Seen) -> Vec<Vec<u8>>| -> Step {
        Step::Answer(Box::new(move |seen| {
            let done = record(&outcome("complete", None, "answered", json!(null), 0), &[]);
            let mut bytes = scripted::head();
            for piece in pieces(seen) {
                bytes.extend(chunk(&piece));
            }
            bytes.extend(chunked(&[&done]));
            bytes
        }))
    };
    let front = scripted(
        &pki,
        vec![
            // Twice.
            answer(|_| vec![identity(&scripted_digest()), identity(&scripted_digest())]),
            // After a frame.
            answer(|seen| {
                vec![
                    identity(&scripted_digest()),
                    hello(seen),
                    identity(&scripted_digest()),
                ]
            }),
            // One keep-alive more than an answer holds.
            answer(|_| vec![keep_alive().repeat(4_097)]),
            // A field beside the two an identity record holds.
            answer(|_| {
                let document =
                    json!({"format": "pack-host/1", "files": scripted_digest(), "more": 1});
                vec![record(&document, &[])]
            }),
        ],
    );
    let asking = Asking::scripted(&front, &pki);
    for (case, words) in [
        ("twice", "no place"),
        ("after a frame", "no place"),
        ("kept alive", "more keep-alives"),
        ("a field more", "refused whole"),
    ] {
        let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
        assert_eq!(
            failed.fault.kind(),
            FaultKind::Malformed,
            "{case}: {failed:?}"
        );
        assert!(
            failed.fault.sentence().contains(words),
            "{case}: {failed:?}"
        );
        charged_at_its_bound(&failed);
    }
}

#[test]
fn as_many_keep_alives_as_an_answer_holds_are_passed_over() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|_| {
            let failure = record(
                &outcome("failed", Some("unavailable"), "not_sent", json!(null), 0),
                &[],
            );
            let mut bytes = scripted::head();
            bytes.extend(chunked(&[&keep_alive().repeat(4_096), &failure]));
            bytes
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    // The record's own failure, not a refusal of the answer.
    assert_eq!(failed.fault.kind(), FaultKind::Unavailable, "{failed:?}");
    assert!(
        failed.fault.sentence().contains("ended the request"),
        "{failed:?}"
    );
}

#[test]
fn the_client_gives_up_at_its_whole_answer_bound_while_keep_alives_keep_coming() {
    let pki = Arc::new(Pki::loopback());
    let mut pieces: Vec<(Duration, scripted::Piece)> = vec![(
        Duration::ZERO,
        Box::new(|_| {
            let mut bytes = scripted::head();
            bytes.extend(chunk(&keep_alive()));
            bytes
        }),
    )];
    // Fewer than an answer holds, so only the bound of the whole can end it.
    for _ in 0..3_000 {
        pieces.push((Duration::from_millis(1), Box::new(|_| chunk(&keep_alive()))));
    }
    let answered = Step::Answer(Box::new(|_| {
        b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_vec()
    }));
    let front = scripted(&pki, vec![Step::Paced(pieces), answered]);
    // A clock that moves a quarter of a second at each read: no wait between two reads
    // comes near the request's own 2 seconds, and the whole passes 2 seconds and the
    // grace of 30 after some 130 reads.
    let clock = Stepping::by(250);
    let mut asking = Asking::scripted(&front, &pki);
    asking.clock = clock.clone();
    asking.host_limit = Duration::from_secs(2);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::TimedOut, "{failed:?}");
    assert!(
        failed.fault.sentence().contains("deadline passed"),
        "{failed:?}"
    );
    charged_at_its_bound(&failed);
    assert!(
        clock.moved() > Duration::from_secs(32),
        "{:?}",
        clock.moved()
    );
}

// --- what the fake host answers -----------------------------------------------------------------

#[test]
fn the_fake_host_answers_a_phrase_with_the_left_half() {
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let answered = Asking::of(&host).ask(&model, &request(phrase(), 1));
    let answered = answered.unwrap_or_else(|failed| panic!("{failed:?}"));
    let Answer::Segment(found) = &answered.called.answer else {
        panic!("{:?}", answered.called.answer);
    };
    assert_eq!(found.len(), 1);
    assert_eq!(answered.called.device, "cpu");
    assert_eq!(answered.dispatch.len(), 32);
    assert_eq!(answered.digest, Asking::of(&host).digest_of(&model));
}

#[test]
fn a_load_frame_carries_the_settings_of_the_call() {
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let mut asking = Asking::of(&host);
    asking.settings = BTreeMap::from([
        ("threshold".to_owned(), Scalar::Text("0.4".to_owned())),
        ("device".to_owned(), Scalar::Text("cpu".to_owned())),
    ]);
    let answered = asking.ask(&model, &request(phrase(), 1)).unwrap();
    let seen = host.seen();
    let load = &seen[&answered.dispatch];
    assert_eq!(load["model"], model.as_str());
    assert_eq!(
        load["settings"],
        json!({"threshold": "0.4", "device": "cpu"})
    );
}

#[test]
fn a_malformed_and_an_oversized_answer_caught_by_the_front_cross_as_malformed_and_too_large() {
    let bad = named(None, "fake-bad");
    let big = named(None, "fake-big");
    let host = Host::serving(&[bad.clone(), big.clone()]);
    let asking = Asking::of(&host);
    for (model, kind) in [(&bad, FaultKind::Malformed), (&big, FaultKind::TooLarge)] {
        let failed = failed(asking.ask(model, &request(phrase(), 1)));
        assert_eq!(failed.fault.kind(), kind, "{model}: {failed:?}");
        // The front ended the worker that sent it.
        assert!(failed.ended.is_some(), "{failed:?}");
        charged_at_its_bound(&failed);
    }
}

#[test]
fn a_worker_that_ended_is_told_by_its_outcome_record_with_its_status_stage_and_error_output_and_no_secret()
 {
    let model = named(Some("error-output"), "fake-exit");
    let host = Host::serving(std::slice::from_ref(&model));
    let mut asking = Asking::of(&host);
    // The run knows the front's token as a secret, as an executor knows each credential.
    asking.secrets = Secrets::new([TOKEN.to_owned()]).unwrap();
    let failed = failed(asking.ask(&model, &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::Unavailable, "{failed:?}");
    assert_eq!(failed.ended, Some(Ended::Exited(4)), "{failed:?}");
    charged_at_its_bound(&failed);
    // The lines the worker wrote in the call cross; the made-up token the front was told
    // to take out is in none of them, and the fault quotes none.
    assert!(!failed.error_output.is_empty(), "{failed:?}");
    let all = failed.error_output.join("\n");
    assert!(all.contains("fake-error-output-marker-7f3a9c"), "{all}");
    assert!(!all.contains(FAKES_TOKEN) && !all.contains(TOKEN));
    for secret in ["fake-error-output-marker", FAKES_TOKEN, TOKEN] {
        assert!(!failed.fault.sentence().contains(secret));
    }
}

#[test]
fn a_worker_exited_killed_and_signalled_each_crosses_as_itself() {
    let pki = Arc::new(Pki::loopback());
    let ended = |ended: serde_json::Value| -> Step {
        Step::Answer(Box::new(move |_| {
            let document = outcome(
                "failed",
                Some("unavailable"),
                "unresolved",
                ended.clone(),
                0,
            );
            unverified(&record(&document, &[]))
        }))
    };
    let front = scripted(
        &pki,
        vec![
            ended(json!({"exited": 7})),
            ended(json!("killed")),
            ended(json!("signalled")),
        ],
    );
    let asking = Asking::scripted(&front, &pki);
    for expected in [Ended::Exited(7), Ended::Killed, Ended::Signalled] {
        let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
        assert_eq!(failed.ended, Some(expected), "{failed:?}");
        charged_at_its_bound(&failed);
    }
}

#[test]
fn a_halt_a_timeout_and_a_cancel_with_the_same_status_and_stage_are_told_apart_by_their_kind() {
    let pki = Arc::new(Pki::loopback());
    let of = |kind: &'static str| -> Step {
        Step::Answer(Box::new(move |_| {
            let document = outcome("failed", Some(kind), "unresolved", json!("killed"), 0);
            unverified(&record(&document, &[]))
        }))
    };
    let front = scripted(
        &pki,
        vec![of("unavailable"), of("timed_out"), of("cancelled")],
    );
    let asking = Asking::scripted(&front, &pki);
    for kind in [
        FaultKind::Unavailable,
        FaultKind::TimedOut,
        FaultKind::Cancelled,
    ] {
        let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
        assert_eq!(failed.fault.kind(), kind, "{failed:?}");
        assert_eq!(failed.ended, Some(Ended::Killed));
        charged_at_its_bound(&failed);
    }
}

#[test]
fn a_worker_error_that_leaves_the_worker_alive_hands_back_its_error_output() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|seen| {
            let error = Message::Error {
                id: "r2".to_owned(),
                fault: FaultKind::Rejected,
                sentence: "the adapter refused the request".to_owned(),
            };
            let body = [
                hello(seen),
                loaded(),
                frames(&[error]),
                record(
                    &outcome("failed", Some("rejected"), "answered", json!(null), 1),
                    &[b"a line the worker wrote".to_vec()],
                ),
            ]
            .concat();
            verified(&scripted_digest(), &body)
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::Rejected, "{failed:?}");
    assert_eq!(failed.ended, None, "the worker lives on");
    assert_eq!(failed.error_output, ["a line the worker wrote"]);
    assert_eq!(
        failed.said.as_deref(),
        Some("the adapter refused the request")
    );
    charged_at_its_bound(&failed);
}

/// An answer of a failure before the files were verified whose chunk says more than is
/// sent: the front holds the rest back, so a reader that read past what it checked would
/// wait for it.
fn unverified_open(bytes: &[u8]) -> Vec<u8> {
    let mut answer = scripted::head();
    answer.extend(format!("{:x}\r\n", bytes.len() + 1_000_000).into_bytes());
    answer.extend(bytes);
    answer
}

#[test]
fn the_long_error_output_fixture_crosses_whole_and_a_line_or_count_over_its_bound_is_refused_before_it_is_read()
 {
    let pki = Arc::new(Pki::loopback());
    let lines: Vec<Vec<u8>> = (0..256u32)
        .map(|at| vec![b'a' + (at % 26) as u8; 4096])
        .collect();
    let whole = Step::Answer(Box::new(move |_| {
        let document = outcome(
            "failed",
            Some("unavailable"),
            "unresolved",
            json!(null),
            256,
        );
        unverified(&record(&document, &lines))
    }));
    // A count over 256, and a line of 4,097 bytes: each refused at its length, the bytes
    // it states never sent.
    let count = Step::AnswerAndHold(Box::new(|_| {
        let document = outcome(
            "failed",
            Some("unavailable"),
            "unresolved",
            json!(null),
            257,
        );
        unverified_open(&record(&document, &[]))
    }));
    let long = Step::AnswerAndHold(Box::new(|_| {
        let document = outcome("failed", Some("unavailable"), "unresolved", json!(null), 1);
        let mut bytes = record(&document, &[]);
        bytes.extend(4097u32.to_be_bytes());
        unverified_open(&bytes)
    }));
    let front = scripted(&pki, vec![whole, count, long]);
    let asking = Asking::scripted(&front, &pki);
    let model = named(None, "fake-ok");
    let crossed = failed(asking.ask(&model, &request(phrase(), 1)));
    assert_eq!(crossed.error_output.len(), 256);
    assert!(crossed.error_output.iter().all(|line| line.len() == 4096));
    for _ in 0..2 {
        let refused = failed(asking.ask(&model, &request(phrase(), 1)));
        assert!(
            refused.fault.sentence().contains("truncated"),
            "{refused:?}"
        );
        assert!(refused.error_output.is_empty());
        charged_at_its_bound(&refused);
    }
}

#[test]
fn a_failed_check_of_the_files_and_a_busy_refusal_cross_as_failure_only_answers_with_their_kind_and_are_charged_at_the_bound()
 {
    // A file of the installation changed on the host: the front refuses the load.
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let identity = host.records[&model].identity().hex();
    let file = host
        .models_dir()
        .join("models")
        .join(&model)
        .join("installations")
        .join(identity)
        .join("files")
        .join("weights.bin");
    std::fs::write(&file, b"not the weights the record names").unwrap();
    let failed_check = failed(Asking::of(&host).ask(&model, &request(phrase(), 1)));
    assert_eq!(
        failed_check.fault.kind(),
        FaultKind::Rejected,
        "{failed_check:?}"
    );
    charged_at_its_bound(&failed_check);
    assert_eq!(
        host.seen().len(),
        1,
        "the side began it, and checked its files"
    );

    // A front whose queue is full refuses at its admission.
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|_| {
            let mut document = outcome("failed", Some("unavailable"), "not_sent", json!(null), 0);
            document["admission"] = json!("busy");
            unverified(&record(&document, &[]))
        }))],
    );
    let busy = failed(Asking::scripted(&front, &pki).ask(&model, &request(phrase(), 1)));
    assert_eq!(busy.fault.kind(), FaultKind::Unavailable, "{busy:?}");
    assert!(busy.fault.sentence().contains("busy"), "{busy:?}");
    charged_at_its_bound(&busy);
}

#[test]
fn a_host_that_failed_its_start_or_was_cancelled_before_the_worker_saw_the_request_is_charged_at_its_bound()
 {
    // The front's worker says no hello: the request never reached it.
    let model = named(Some("no-hello"), "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let no_start = failed(Asking::of(&host).ask(&model, &request(phrase(), 1)));
    charged_at_its_bound(&no_start);
    // A record that says the request was cancelled before the worker saw it.
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|seen| {
            let document = outcome("failed", Some("cancelled"), "not_sent", json!(null), 0);
            let body = [hello(seen), record(&document, &[])].concat();
            verified(&scripted_digest(), &body)
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let cancelled = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(cancelled.fault.kind(), FaultKind::Cancelled);
    charged_at_its_bound(&cancelled);
}

// --- answers cut, over a bound, or pointed elsewhere --------------------------------------------

#[test]
fn a_stream_cut_before_its_result_is_truncated_and_charged_at_its_bound() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|seen| {
            // The head and the hello in one chunk, and the connection closed with no last
            // chunk.
            let mut bytes = verified_head(&scripted_digest());
            let hello = hello(seen);
            bytes.extend(format!("{:x}\r\n", hello.len()).into_bytes());
            bytes.extend(hello);
            bytes.extend(b"\r\n");
            bytes
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    charged_at_its_bound(&failed);
    assert!(
        matches!(
            failed.fault.kind(),
            FaultKind::Malformed | FaultKind::Unavailable
        ),
        "{failed:?}"
    );
}

#[test]
fn a_body_that_ends_without_its_outcome_record_is_truncated() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|seen| {
            let body = [hello(seen), loaded()].concat();
            verified(&scripted_digest(), &body)
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert!(failed.fault.sentence().contains("truncated"), "{failed:?}");
    charged_at_its_bound(&failed);
}

#[test]
fn an_answer_past_a_bound_of_pack_1_is_refused_before_it_is_held() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::AnswerAndHold(Box::new(|seen| {
            // The hello, then a frame whose header states 16 MiB, far past pack/1's 64
            // KiB; none of it is sent, so a reader that waited for it would wait.
            let mut body = hello(seen);
            body.extend((16u32 << 20).to_be_bytes());
            let mut bytes = verified_head(&scripted_digest());
            bytes.extend(format!("{:x}\r\n", body.len()).into_bytes());
            bytes.extend(body);
            bytes.extend(b"\r\n");
            bytes
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::TooLarge, "{failed:?}");
    charged_at_its_bound(&failed);
}

#[test]
fn a_redirect_from_the_host_is_refused_and_nothing_follows_it() {
    let pki = Arc::new(Pki::loopback());
    let front = scripted(
        &pki,
        vec![Step::Answer(Box::new(|_| {
            b"HTTP/1.1 307 Temporary Redirect\r\nLocation: https://127.0.0.1:9/pack/1/request\r\n\
              Content-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec()
        }))],
    );
    let asking = Asking::scripted(&front, &pki);
    let failed = failed(asking.ask(&named(None, "fake-ok"), &request(phrase(), 1)));
    assert_eq!(failed.fault.kind(), FaultKind::Rejected, "{failed:?}");
    assert!(failed.fault.sentence().contains("nothing follows it"));
    charged_at_its_bound(&failed);
    // One request, and nothing asked of the address it pointed to.
    assert_eq!(front.listener.finish(), 1);
}

#[test]
fn a_plain_http_address_is_refused_and_a_certificate_of_another_root_fails_the_call() {
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let plain = Asking::at(
        &host.base().replace("https://", "http://"),
        host.pki.clone(),
        host.records.clone(),
    );
    let refused = failed(plain.ask(&model, &request(phrase(), 1)));
    assert_eq!(refused.fault.kind(), FaultKind::Rejected, "{refused:?}");
    assert_eq!(refused.fault.stage(), Stage::NotSent, "nothing left");
    // A client that trusts another root: the handshake fails, and the front reads
    // nothing of the request.
    let other = Asking::at(
        &host.base(),
        Arc::new(Pki::made(41, [127, 0, 0, 1])),
        host.records.clone(),
    );
    let untrusted = failed(other.ask(&model, &request(phrase(), 1)));
    assert_eq!(untrusted.fault.kind(), FaultKind::Rejected, "{untrusted:?}");
    assert!(host.seen().is_empty(), "the front began no request");
}

// --- what the send check sees ------------------------------------------------------------------

/// A picture whose pixels hold the bytes of a path, and a request of it.
fn path_spelt() -> SegmentRequest {
    // A person's folder on macOS, put together here so that no scan of this file finds
    // a home path in it.
    let spelt = format!("/{}/someone/secret/", "Users").into_bytes();
    let rgba: Vec<u8> = (0..8 * 6 * 4).map(|at| spelt[at % spelt.len()]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(8, 6, rgba).unwrap()).unwrap();
    SegmentRequest::new(picture, phrase(), 1).unwrap()
}

#[test]
fn pixels_that_spell_a_path_are_sent_and_a_path_in_the_phrase_or_a_setting_is_refused_before_anything_leaves()
 {
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    // Pixels are pixels, whatever their bytes spell.
    let answered = Asking::of(&host).ask(&model, &path_spelt());
    assert!(answered.is_ok(), "{answered:?}");
    // A path in the phrase.
    let in_folder = format!("the file /{}/someone/notes.txt", "Users");
    let phrase = vec![Query::phrase(&in_folder).unwrap()];
    let in_phrase = failed(Asking::of(&host).ask(&model, &request(phrase, 1)));
    assert_eq!(in_phrase.fault.kind(), FaultKind::Rejected, "{in_phrase:?}");
    assert_eq!(in_phrase.fault.stage(), Stage::NotSent);
    // A path in a setting of the load.
    let mut asking = Asking::of(&host);
    asking.settings = BTreeMap::from([(
        "outline".to_owned(),
        Scalar::Text("/tmp/outline".to_owned()),
    )]);
    let in_setting = failed(asking.ask(&model, &request(super::phrase(), 1)));
    assert_eq!(in_setting.fault.stage(), Stage::NotSent, "{in_setting:?}");
    // The front saw the one request that was sent.
    assert_eq!(host.seen().len(), 1);
}

#[test]
fn the_stores_root_in_any_text_is_refused_before_anything_leaves() {
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let root = "store-root-of-the-test";
    let mut asking = Asking::of(&host);
    asking.check = SendCheck::new()
        .keeping_out(std::path::Path::new(root))
        .unwrap();
    for (phrase, settings) in [
        (format!("near {root} here"), BTreeMap::new()),
        (
            "a red thing".to_owned(),
            BTreeMap::from([("outline".to_owned(), Scalar::Text(format!("x{root}x")))]),
        ),
    ] {
        asking.settings = settings;
        let query = vec![Query::phrase(&phrase).unwrap()];
        let refused = failed(asking.ask(&model, &request(query, 1)));
        assert_eq!(refused.fault.kind(), FaultKind::Rejected, "{refused:?}");
        assert_eq!(refused.fault.stage(), Stage::NotSent);
    }
    assert!(host.seen().is_empty());
}

// --- dispatch ids --------------------------------------------------------------------------------

/// The plan's name for it. What it shows is narrower than its name: two backends, each
/// made as a process with its own store makes its own, asking the one fake host at once
/// from one test process, mint other dispatch ids and are both answered. The ids come
/// from the system's randomness and not from a clock or a store, so two OS processes
/// mint them as two backends of one process do; no second OS process is started here.
#[test]
fn two_processes_with_their_own_stores_at_one_clock_mint_other_dispatch_ids_and_both_run_against_the_fake_host()
 {
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    // Two backends, as two processes each make their own, ask at once: the front's side
    // takes one call at a time, so one waits its turn in the front's queue.
    let (one, two) = (Asking::of(&host), Asking::of(&host));
    let (first, second) = whizbang_testkit::threads::scope(|scope| {
        let one = scope.spawn(|| one.ask(&model, &request(phrase(), 1)));
        let two = scope.spawn(|| two.ask(&model, &request(phrase(), 1)));
        (one.join().unwrap(), two.join().unwrap())
    });
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_ne!(first.dispatch, second.dispatch);
    let seen = host.seen();
    assert!(seen.contains_key(&first.dispatch) && seen.contains_key(&second.dispatch));
}

#[test]
fn an_image_that_is_no_picture_is_refused_before_anything_leaves() {
    // A call made by hand whose raster says one size and holds the samples of another:
    // it is not a picture, so the check could not be shown it, and nothing is sent.
    let model = named(None, "fake-ok");
    let host = Host::serving(std::slice::from_ref(&model));
    let asking = Asking::of(&host);
    let through = asking.through(&model);
    let mut call = whizbang_backends::pack::proto::Ask::segment(
        &request(phrase(), 1),
        through.model.clone(),
        through.installation,
    );
    if let whizbang_backends::pack::proto::Ask::Segment { image, .. } = &mut call {
        image.samples.truncate(4);
    }
    let refused = failed(asking.ask_by_hand(
        &model,
        &request(phrase(), 1),
        whizbang_core::contract::Cancellation::new(),
        None,
        Some(call),
    ));
    assert_eq!(refused.fault.kind(), FaultKind::Rejected, "{refused:?}");
    assert_eq!(refused.fault.stage(), Stage::NotSent);
    assert!(
        refused.fault.sentence().contains("not a picture"),
        "{refused:?}"
    );
    assert!(host.seen().is_empty());
}
