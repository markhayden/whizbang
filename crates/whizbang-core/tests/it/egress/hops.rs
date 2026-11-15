//! Every hop (ADR 0019; plan 1B, task E3, the first of its two pull requests): what a
//! backend is asked to reach after its first address, a redirect, an upload address
//! and a returned download address, is checked as the first address is; a credential
//! goes only to the origin it is bound to; and one request is pointed elsewhere a
//! bounded number of times. Against the test kit's sending fake: nothing is sent
//! anywhere.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use whizbang_core::capability::{
    Bounds, Controls, Declaration, Fault, FaultKind, FillRequest, Filler, Judge, JudgeRequest,
    Message, Painted, Part, Query, Role, SegmentRequest, SegmentResponse, Segmenter, Stage, Turn,
    feature,
};
use whizbang_core::contract::record::{Handed, Status};
use whizbang_core::contract::{
    Call, Capabilities, Capability, Executor, ModelIdentity, Produced, RefusalKind, Requirement,
    Secrets,
};
use whizbang_core::egress::{Credential, MOVES_LIMIT, Outgoing, Reply, SendCheck, Ways};
use whizbang_testkit::capabilities::{FAKE_MOVES_LIMIT, FakeFiller, Paint, roster};
use whizbang_testkit::sending::FakeSender;
use whizbang_testkit::threads;

use super::support::{
    ACCOUNT_CUT, API, ELSEWHERE, UPSTREAM, World, a_prompt, action, address, approving, engine,
    entry, fill_request, holds, made_up_key, moved, ok, origin, policy, prompts, recipient,
    run_one, stored,
};

/// What the check answers of a request that was pointed elsewhere too often.
const MOVED: &str = "the request was pointed elsewhere more than 5 times, and is not \
                     followed further";

const NOT_REACHED: &str = "the origin is neither the recipient's own nor one of its upstreams";

/// The ways out of an executor whose config approves the recipient for prompts.
fn ways(sender: &Arc<FakeSender>) -> Ways {
    Ways::through(sender.clone())
        .checked_by(Arc::new(SendCheck::new()))
        .under(policy(&approving(&["prompts"])))
}

/// A filler that names the recipient, sends a prompt to this address for every
/// request, and goes on to each address an answer points to.
fn remote(to: &str) -> Arc<FakeFiller> {
    let filler = FakeFiller::new(Paint::Inverse).sending_to(recipient());
    Arc::new(filler.posting(address(to), a_prompt()))
}

fn refused_as(fault: &Fault) -> (FaultKind, Stage, &str) {
    (fault.kind(), fault.stage(), fault.sentence())
}

fn a_prompt_to(to: &str) -> Outgoing {
    Outgoing::new(address(to), a_prompt()).unwrap()
}

/// A filler that names the recipient and does what a test tells it with the bounds
/// of each request, and then answers with a fault.
struct Custom {
    declaration: Declaration,
    with: Box<dyn Fn(&Bounds) + Send + Sync>,
}

fn custom(with: impl Fn(&Bounds) + Send + Sync + 'static) -> Arc<Custom> {
    let model = ModelIdentity::new("fake", "fake-filler", "1");
    Arc::new(Custom {
        declaration: Declaration::new(model, &[])
            .unwrap()
            .sending_to(recipient()),
        with: Box::new(with),
    })
}

impl Filler for Custom {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, _: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        (self.with)(bounds);
        Err(Fault::new(FaultKind::Unsupported, "the test's filler paints nothing").not_sent())
    }
}

// --- a redirect ---------------------------------------------------------------------------------

#[test]
fn a_redirect_to_an_origin_that_is_not_the_recipients_or_an_upstream_is_refused_and_nothing_is_sent_to_it()
 {
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{ELSEWHERE}/v1/fill?sig=abc123")),
        ok(),
    ]));
    let ran = run_one(
        remote(&format!("{API}/v1/fill")),
        ways(&sender),
        Secrets::none(),
    );
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        refused_as(&fault),
        (FaultKind::Rejected, Stage::NotSent, NOT_REACHED)
    );
    // The first address was handed the request; the address it pointed to, nothing.
    let hops = sender.hops();
    assert_eq!(hops.len(), 1);
    assert_eq!(hops[0].to().origin(), &origin(API));
    assert_eq!(
        ran.went(),
        [
            entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent),
            entry(ELSEWHERE, Some(ACCOUNT_CUT), prompts(), Handed::Refused),
        ]
    );
    assert_eq!(
        ran.refused(),
        [(
            RefusalKind::RequestNotApproved,
            format!(
                "nothing was sent to {ELSEWHERE}: {NOT_REACHED} (the classes of data the \
                 request holds: prompts)"
            )
        )]
    );
    assert!(!holds(&ran.store, "sig=abc123"));
}

#[test]
fn a_redirect_to_an_approved_upstream_is_followed_without_the_credential() {
    // The executor holds a credential bound to the recipient's own origin and none
    // for the upstream. The first address is handed it; the upstream an answer
    // pointed to is handed the request and no credential.
    let key = made_up_key("E3");
    let credential = Credential::new(origin(API), "authorization", &key, &Secrets::none()).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{UPSTREAM}/put?sig=abc123")),
        ok(),
    ]));
    let ways = ways(&sender)
        .with_credential(&recipient(), credential)
        .unwrap();
    let ran = run_one(
        remote(&format!("{API}/v1/fill")),
        ways,
        Secrets::new([key.clone()]).unwrap(),
    );
    assert_eq!(ran.fault, None);
    let hops = sender.hops();
    assert_eq!(hops.len(), 2);
    assert_eq!(hops[0].to().origin(), &origin(API));
    assert_eq!(
        hops[0].headers(),
        [("authorization".to_owned(), key.clone())]
    );
    assert_eq!(
        hops[1].to(),
        &address(&format!("{UPSTREAM}/put?sig=abc123"))
    );
    assert_eq!(
        hops[1].headers(),
        [],
        "the upstream is handed no credential"
    );
    assert_eq!(hops[1].outgoing().credential(), None);
    assert_eq!(hops[1].outgoing().pieces(), a_prompt());
    // Both are listed as reached, and nothing as refused.
    assert_eq!(
        ran.went(),
        [
            entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent),
            entry(UPSTREAM, Some(ACCOUNT_CUT), prompts(), Handed::Sent),
        ]
    );
    assert_eq!(ran.refused(), []);
    assert!(!holds(&ran.store, &key) && !holds(&ran.store, "sig=abc123"));
}

#[test]
fn a_credential_bound_to_an_upstream_goes_to_that_upstream_and_never_to_the_recipients_own_origin()
{
    // The other way round: the only credential is bound to the upstream. The first
    // address goes without it, and a redirect back to the recipient's own origin
    // goes without it too.
    let key = made_up_key("E3-up");
    let credential =
        Credential::new(origin(UPSTREAM), "x-upload-key", &key, &Secrets::none()).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{UPSTREAM}/put")),
        moved(&format!("{API}/v1/done")),
        ok(),
    ]));
    let ways = ways(&sender)
        .with_credential(&recipient(), credential)
        .unwrap();
    let ran = run_one(
        remote(&format!("{API}/v1/fill")),
        ways,
        Secrets::new([key.clone()]).unwrap(),
    );
    assert_eq!(ran.fault, None);
    let headers: Vec<_> = sender.hops().iter().map(|hop| hop.headers()).collect();
    assert_eq!(
        headers,
        [
            vec![],
            vec![("x-upload-key".to_owned(), key.clone())],
            vec![]
        ]
    );
    assert!(!holds(&ran.store, &key));
}

// --- an upload address and a download address ---------------------------------------------------

#[test]
fn an_upload_address_and_a_download_address_outside_the_recipients_are_each_refused() {
    // The first request is answered, and its answer names an upload address and a
    // download address. Each is another request the backend makes: one inside the
    // recipient's origins is sent, one outside is refused, and nothing is handed to
    // it.
    let named = Reply::answered(
        200,
        format!("{{\"upload\":\"{ELSEWHERE}/put\",\"download\":\"{ELSEWHERE}/get\"}}").into_bytes(),
    )
    .unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![Ok(named), ok(), ok()]));
    let faults = Arc::new(Mutex::new(Vec::new()));
    let filler = custom({
        let faults = faults.clone();
        move |bounds| {
            let send = |to: String| bounds.sender().send(&a_prompt_to(&to), bounds).err();
            let asked = [
                send(format!("{API}/v1/fill")),
                send(format!("{ELSEWHERE}/put")),
                send(format!("{ELSEWHERE}/get")),
                send(format!("{UPSTREAM}/put")),
                send(format!("{UPSTREAM}/get")),
            ];
            faults.lock().unwrap().extend(asked);
        }
    });
    let ran = run_one(filler, ways(&sender), Secrets::none());
    let faults = faults.lock().unwrap().clone();
    assert_eq!(faults.len(), 5);
    assert_eq!(faults[0], None);
    for refused in &faults[1..3] {
        assert_eq!(
            refused_as(refused.as_ref().unwrap()),
            (FaultKind::Rejected, Stage::NotSent, NOT_REACHED)
        );
    }
    assert_eq!((&faults[3], &faults[4]), (&None, &None));
    let reached: Vec<String> = sender
        .hops()
        .iter()
        .map(|hop| hop.to().origin().to_string())
        .collect();
    assert_eq!(reached, [API, UPSTREAM, UPSTREAM]);
    assert_eq!(
        ran.went(),
        [
            entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent),
            entry(ELSEWHERE, Some(ACCOUNT_CUT), prompts(), Handed::Refused),
            entry(UPSTREAM, Some(ACCOUNT_CUT), prompts(), Handed::Sent),
        ]
    );
    assert_eq!(ran.refused().len(), 2);
}

// --- a chain --------------------------------------------------------------------------------------

/// A script of this many answers that point elsewhere, each to the other of the
/// recipient's two origins, and then an answer.
fn chain(moves: usize) -> Vec<Result<Reply, Fault>> {
    let mut script: Vec<_> = (0..moves)
        .map(|at| {
            let there = if at % 2 == 0 { UPSTREAM } else { API };
            moved(&format!("{there}/hop/{at}"))
        })
        .collect();
    script.push(ok());
    script
}

#[test]
fn a_chain_longer_than_the_limit_is_refused() {
    assert_eq!(MOVES_LIMIT, 5);
    assert!(FAKE_MOVES_LIMIT > MOVES_LIMIT as usize + 1);
    // Pointed elsewhere five times, the request is followed each time and answered.
    let sender = Arc::new(FakeSender::scripted(chain(5)));
    let ran = run_one(
        remote(&format!("{API}/v1/fill")),
        ways(&sender),
        Secrets::none(),
    );
    assert_eq!((ran.fault.clone(), sender.requests()), (None, 6));
    assert_eq!(ran.refused(), []);

    // Pointed elsewhere a sixth time, it is not followed: the address the sixth
    // answer named is handed nothing, and the refusal is the check's.
    let sender = Arc::new(FakeSender::scripted(chain(6)));
    let ran = run_one(
        remote(&format!("{API}/v1/fill")),
        ways(&sender),
        Secrets::none(),
    );
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        refused_as(&fault),
        (FaultKind::Rejected, Stage::NotSent, MOVED)
    );
    assert_eq!(sender.requests(), 6, "the first address and five more");
    assert_eq!(
        sender.remaining(),
        1,
        "the answer after the sixth was not asked for"
    );
    assert_eq!(
        ran.refused(),
        [(
            RefusalKind::RequestNotApproved,
            format!(
                "nothing was sent to {API}: {MOVED} (the classes of data the request holds: \
                 prompts)"
            )
        )]
    );
    // Once over the limit, every later address of that request is refused too: the
    // count does not go back.
    let later = Arc::new(Mutex::new(Vec::new()));
    let sender = Arc::new(FakeSender::scripted(chain(6)));
    let filler = custom({
        let later = later.clone();
        move |bounds| {
            let mut to = address(&format!("{API}/v1/fill"));
            for _ in 0..6 {
                let outgoing = Outgoing::new(to.clone(), a_prompt()).unwrap();
                let reply = bounds.sender().send(&outgoing, bounds).unwrap();
                to = reply.moved_to().unwrap().clone();
            }
            for to in [format!("{API}/hop/5"), format!("{API}/v1/fill")] {
                let fault = bounds.sender().send(&a_prompt_to(&to), bounds).unwrap_err();
                later.lock().unwrap().push(fault.sentence().to_owned());
            }
        }
    });
    run_one(filler, ways(&sender), Secrets::none());
    assert_eq!(*later.lock().unwrap(), [MOVED, MOVED]);
    assert_eq!(sender.requests(), 6);
}

#[test]
fn a_long_run_that_is_pointed_elsewhere_once_for_each_request_is_not_refused() {
    // Thirty requests of one backend in one run, each pointed elsewhere once: the
    // limit is of one request, so none is refused, though the moves of the run are
    // thirty.
    const TURNS: usize = 30;
    let script = (0..TURNS).flat_map(|at| [moved(&format!("{UPSTREAM}/put/{at}")), ok()]);
    let sender = Arc::new(FakeSender::scripted(script.collect()));
    let filler = remote(&format!("{API}/v1/fill"));
    let world = World::new();
    let faults = Arc::new(AtomicU32::new(0));
    let asks = {
        let faults = faults.clone();
        let needs = vec![Requirement::new(Capability::Fill)];
        action("asks", needs, move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            for _ in 0..TURNS {
                if fill.fill(&fill_request()).is_err() {
                    faults.fetch_add(1, Ordering::SeqCst);
                }
            }
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks],
    )
    .with_ways(ways(&sender));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(faults.load(Ordering::SeqCst), 0);
    assert_eq!(sender.requests(), 2 * TURNS as u64);
    assert_eq!(record.outcome().refusals.len(), 0);
}

// --- the count is one request's --------------------------------------------------------------------

#[test]
fn the_moves_of_one_request_are_its_own_and_a_clone_of_its_bounds_holds_the_same_count() {
    // One handle, two requests of one run, interleaved: the backend keeps a clone of
    // the first request's bounds and goes on using it during the second.
    //
    // The first request is pointed elsewhere five times. The second, through the
    // same handle, is then pointed elsewhere five times and followed each time: the
    // first's five are not counted against it. Then the first is pointed elsewhere a
    // sixth time, through the clone that was kept: its next address is refused, by
    // the clone and by nothing of the second, whose next address is still sent.
    let kept: Arc<Mutex<Option<Bounds>>> = Arc::default();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let script: Vec<_> = (0..11)
        .map(|at| moved(&format!("{API}/hop/{at}")))
        .chain([ok()])
        .collect();
    let sender = Arc::new(FakeSender::scripted(script));
    let filler = custom({
        let (kept, seen) = (kept.clone(), seen.clone());
        move |bounds| {
            let to = a_prompt_to(&format!("{API}/v1/fill"));
            let pointed = |bounds: &Bounds| {
                let reply = bounds.sender().send(&to, bounds).unwrap();
                assert!(reply.moved_to().is_some());
            };
            let mut kept = kept.lock().unwrap();
            let Some(first) = kept.as_ref() else {
                // The first request: pointed elsewhere five times, and kept.
                for _ in 0..5 {
                    pointed(bounds);
                }
                *kept = Some(bounds.clone());
                return;
            };
            // The second request: five times, each followed.
            for _ in 0..5 {
                pointed(bounds);
            }
            // The first, a sixth time, through the clone; and then its next address.
            pointed(first);
            let refused = first.sender().send(&to, first).unwrap_err();
            // The second request's next address is sent, and answered.
            let answered = bounds.sender().send(&to, bounds).unwrap();
            let mut seen = seen.lock().unwrap();
            seen.push(refused.sentence().to_owned());
            seen.push(format!("{:?}", answered.status()));
        }
    });
    let world = World::new();
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            let fill = context.capabilities().fill().unwrap();
            for _ in 0..2 {
                fill.fill(&fill_request()).unwrap_err();
            }
            Ok(Produced::nothing())
        },
    );
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks],
    )
    .with_ways(ways(&sender));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(*seen.lock().unwrap(), [MOVED, "Some(200)"]);
    assert_eq!(sender.requests(), 12);
    assert_eq!(record.outcome().refusals.len(), 1);
}

/// A backend of each capability, which names the recipient and for every request
/// sends a prompt once and counts a send that was refused.
struct Hopping {
    declaration: Declaration,
    refused: AtomicU32,
}

impl Hopping {
    fn send(&self, bounds: &Bounds) -> Fault {
        let to = a_prompt_to(&format!("{API}/v1/ask"));
        if bounds.sender().send(&to, bounds).is_err() {
            self.refused.fetch_add(1, Ordering::SeqCst);
        }
        Fault::new(FaultKind::Unsupported, "the test's backend answers nothing").unresolved()
    }
}

impl Filler for Hopping {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, _: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        Err(self.send(bounds))
    }
}

impl Segmenter for Hopping {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn segment(&self, _: &SegmentRequest, bounds: &Bounds) -> Result<SegmentResponse, Fault> {
        Err(self.send(bounds))
    }
}

impl Judge for Hopping {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn send(&self, _: &JudgeRequest, bounds: &Bounds) -> Result<Turn, Fault> {
        Err(Hopping::send(self, bounds))
    }
}

#[test]
fn each_request_of_each_capability_is_handed_a_count_of_its_own() {
    // Seven requests through one handle, each pointed elsewhere once, for a filler,
    // a segmenter and a judge: with one count for a handle the seventh would be over
    // the limit before it was sent.
    const REQUESTS: usize = 7;
    let hopping = || {
        let model = ModelIdentity::new("fake", "fake-hopping", "1");
        Arc::new(Hopping {
            declaration: Declaration::new(model, &[feature::POINT])
                .unwrap()
                .sending_to(recipient()),
            refused: AtomicU32::new(0),
        })
    };
    let (filler, segmenter, judge) = (hopping(), hopping(), hopping());
    let script = (0..3 * REQUESTS).map(|at| moved(&format!("{API}/hop/{at}")));
    let sender = Arc::new(FakeSender::scripted(script.collect()));
    let world = World::new();
    let needs = vec![
        Requirement::new(Capability::Fill),
        Requirement::new(Capability::Segment),
        Requirement::new(Capability::Judge),
    ];
    let asks = action("asks", needs, |_, context| {
        let handles = context.capabilities();
        let picture = fill_request().image().clone();
        let point = vec![Query::point(1.0, 1.0).unwrap()];
        let segment = SegmentRequest::new(picture, point, 1).unwrap();
        let said = Message::new(Role::User, vec![Part::Text("is it red".to_owned())]).unwrap();
        let controls = Controls::new(64).unwrap();
        let judge = JudgeRequest::new("You judge.", vec![said], Vec::new(), controls).unwrap();
        for _ in 0..REQUESTS {
            handles.fill().unwrap().fill(&fill_request()).unwrap_err();
            handles.segment().unwrap().segment(&segment).unwrap_err();
            handles.judge().unwrap().send(&judge).unwrap_err();
        }
        Ok(Produced::nothing())
    });
    let held = Capabilities::none()
        .with_fill(filler.clone())
        .with_segment(segmenter.clone())
        .with_judge(judge.clone());
    let offered = roster(
        Some(segmenter.as_ref()),
        Some(filler.as_ref()),
        Some(judge.as_ref()),
    );
    let executor = engine(&world, offered, held, vec![asks]).with_ways(ways(&sender));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    for backend in [&filler, &segmenter, &judge] {
        assert_eq!(backend.refused.load(Ordering::SeqCst), 0);
    }
    assert_eq!(sender.requests(), 3 * REQUESTS as u64);
    assert_eq!(record.outcome().refusals.len(), 0);
}

/// Whose turn it is among requests that take turns: each waits for its own.
struct Turns {
    turn: Mutex<u32>,
    changed: Condvar,
}

impl Turns {
    /// Waits until it is the turn of `who` among `of`, does `work`, and passes the
    /// turn on. A wait that lasts a minute is a test that cannot pass.
    fn take<T>(&self, who: u32, of: u32, work: impl FnOnce() -> T) -> T {
        let limit = Instant::now() + Duration::from_secs(60);
        let mut turn = self.turn.lock().unwrap();
        while *turn % of != who {
            let left = limit.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "the turn of {who} never came");
            turn = self.changed.wait_timeout(turn, left).unwrap().0;
        }
        let done = work();
        *turn += 1;
        self.changed.notify_all();
        done
    }
}

#[test]
fn two_requests_through_one_handle_at_once_each_count_their_own_moves() {
    // Two threads ask one handle at once, and their sends take turns: one answer
    // that points elsewhere for the one, then one for the other, five times each,
    // and then an answer for each. Counted together they would be ten, and over the
    // limit; counted for each request they are five, and every one is followed.
    let turns = Arc::new(Turns {
        turn: Mutex::new(0),
        changed: Condvar::new(),
    });
    let arrived = Arc::new(AtomicU32::new(0));
    let refused = Arc::new(AtomicU32::new(0));
    let script: Vec<_> = (0..10)
        .map(|at| moved(&format!("{API}/hop/{at}")))
        .chain([ok(), ok()])
        .collect();
    let sender = Arc::new(FakeSender::scripted(script));
    let filler = custom({
        let (turns, arrived, refused) = (turns.clone(), arrived.clone(), refused.clone());
        move |bounds| {
            let who = arrived.fetch_add(1, Ordering::SeqCst);
            let to = a_prompt_to(&format!("{API}/v1/fill"));
            for _ in 0..6 {
                let sent = turns.take(who, 2, || bounds.sender().send(&to, bounds));
                if sent.is_err() {
                    refused.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    });
    let world = World::new();
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            let fill = context.capabilities().fill().unwrap();
            // Two threads show it: one request each.
            threads::scope(|scope| {
                let other = scope.spawn(|| fill.fill(&fill_request()).is_err());
                let here = fill.fill(&fill_request()).is_err();
                assert!(
                    here && other.join().unwrap(),
                    "the test's filler paints nothing"
                );
            });
            Ok(Produced::nothing())
        },
    );
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks],
    )
    .with_ways(ways(&sender));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(refused.load(Ordering::SeqCst), 0);
    assert_eq!(sender.requests(), 12);
    assert_eq!(record.outcome().refusals.len(), 0);
}
