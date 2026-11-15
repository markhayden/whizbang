//! A policy before a send, and a checked send (ADR 0015; plan 1B, task I5): what an
//! executor asks before data is handed to a sender, and the sender a backend is
//! handed. Nothing is sent anywhere: the sender behind it is the test kit's fake.

use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillHandle, FillRequest, Filler, Painted, Stage,
};
use whizbang_core::contract::record::{Handed, RunRecord, Status};
use whizbang_core::contract::{
    Answer, Call, Cancellation, Capabilities, Capability, Executor, ModelIdentity, Produced,
    RefusalKind, Request, Requirement, Secrets,
};
use whizbang_core::egress::{
    APPROVED_LIMIT, Account, Address, CREDENTIALS_LIMIT, Classes, Credential, DataClass, Denied,
    DispatchPolicy, HopCheck, NoHop, NotAllowed, Origin, Outgoing, Piece, Recipient, Reply, Sender,
    Ways,
};
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};
use whizbang_testkit::sending::{FakeCheck, FakeSender};

use super::asked::{action, engine, fill_request, stored};
use super::commit::World;
use super::redact::made_up_key;

const API: &str = "https://api.example.com";
const UPSTREAM: &str = "https://uploads.example.net";
const ELSEWHERE: &str = "https://elsewhere.example.org";

fn origin(address: &str) -> Origin {
    Origin::of(address, &Secrets::none()).unwrap()
}

fn address(text: &str) -> Address {
    Address::of(text, &Secrets::none()).unwrap()
}

fn account(text: &str) -> Account {
    Account::new(text, &Secrets::none()).unwrap()
}

/// The recipient at the API's origin, under an account, which may forward to the
/// upstream.
fn recipient() -> Recipient {
    Recipient::new(origin(API))
        .with_account(account("acct_4f8a1c9e77"))
        .forwarding_to(vec![origin(UPSTREAM)])
        .unwrap()
}

fn prompts() -> Classes {
    Classes::of(&[DataClass::Prompts])
}

fn a_prompt() -> Vec<Piece> {
    vec![Piece::of(DataClass::Prompts, b"a red door".to_vec())]
}

fn approving(classes: Classes) -> DispatchPolicy {
    DispatchPolicy::none()
        .approving(recipient(), classes)
        .unwrap()
}

fn ok() -> Result<Reply, Fault> {
    Ok(Reply::answered(200, b"{}".to_vec()).unwrap())
}

fn moved(to: &str) -> Result<Reply, Fault> {
    Ok(Reply::moved(address(to)))
}

/// A filler that names the recipient and sends these pieces to this address for
/// every request, through what its bounds hand it.
fn remote(to: &str, pieces: Vec<Piece>) -> Arc<FakeFiller> {
    let filler = FakeFiller::new(Paint::Inverse).sending_to(recipient());
    Arc::new(filler.posting(address(to), pieces))
}

/// What one run that asked its filler once came to: its record, the record's bytes,
/// and the fault the action was answered with, when it was.
struct Ran {
    record: RunRecord,
    bytes: Vec<u8>,
    fault: Option<Fault>,
    /// Every byte of every file under the store, after the run.
    store: Vec<u8>,
}

/// Every byte of every file under a folder, one file after another.
fn every_byte_under(folder: &std::path::Path, into: &mut Vec<u8>) {
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            every_byte_under(&path, into);
        } else {
            into.extend(std::fs::read(&path).unwrap());
            into.push(b'\n');
        }
    }
}

impl Ran {
    /// Where the run's data went: each origin, cut account, classes and outcome.
    fn went(&self) -> Vec<(String, Option<String>, Classes, Handed)> {
        let entry = |went: &whizbang_core::contract::record::Went| {
            (
                went.origin().to_owned(),
                went.account().map(str::to_owned),
                went.classes(),
                went.outcome(),
            )
        };
        self.record.egress().iter().map(entry).collect()
    }

    /// The run's refusals of a request: each kind and sentence.
    fn refused(&self) -> Vec<(RefusalKind, String)> {
        let refusals = &self.record.outcome().refusals;
        refusals
            .iter()
            .map(|refused| {
                assert_eq!(refused.action(), None);
                (refused.kind(), refused.why().to_owned())
            })
            .collect()
    }
}

/// Runs one action that asks this filler once and completes whatever it is answered,
/// on an executor with these ways out and these secrets.
fn run_one(filler: &Arc<FakeFiller>, ways: Ways, secrets: Secrets) -> Ran {
    let world = World::new();
    let answered = Arc::new(Mutex::new(None));
    let asks = {
        let answered = answered.clone();
        let needs = vec![Requirement::new(Capability::Fill)];
        action("asks", needs, move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            *answered.lock().unwrap() = fill.fill(&fill_request()).err();
            Ok(Produced::nothing())
        })
    };
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks],
    )
    .with_secrets(secrets)
    .with_ways(ways);
    let Ok(Answer::Ran(finished)) = executor.request(&Request::new(Call::new("asks"))) else {
        panic!("the request did not run");
    };
    let (record, names) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    let fault = answered.lock().unwrap().take();
    let mut store = Vec::new();
    every_byte_under(&world.root(), &mut store);
    Ran {
        record,
        bytes: names["record"].clone(),
        fault,
        store,
    }
}

fn holds(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

// --- the policy ------------------------------------------------------------------------------

#[test]
fn the_default_policy_approves_nothing() {
    let policy = DispatchPolicy::none();
    assert!(policy.is_empty());
    assert_eq!(policy, DispatchPolicy::default());
    for classes in [Classes::none(), prompts(), Classes::all()] {
        assert!(!policy.may_receive(&recipient(), classes));
    }
    assert!(!policy.may_probe(&recipient()));
    assert_eq!(Ways::none().policy(), &policy);
}

#[test]
fn a_policy_approves_a_recipient_for_the_classes_it_was_given_and_no_other() {
    let policy = approving(prompts());
    assert!(!policy.is_empty());
    assert!(policy.may_receive(&recipient(), prompts()));
    // A request with no data in it is still a request to the recipient.
    assert!(policy.may_receive(&recipient(), Classes::none()));
    for class in [
        DataClass::SourceImages,
        DataClass::DerivedImages,
        DataClass::Evidence,
    ] {
        assert!(!policy.may_receive(&recipient(), Classes::of(&[class])));
        assert!(!policy.may_receive(&recipient(), prompts().with(class)));
    }
    assert!(!policy.may_receive(&recipient(), Classes::all()));
    // Approved again, it may receive what it was approved for before too.
    let more = policy
        .approving(recipient(), Classes::of(&[DataClass::Evidence]))
        .unwrap();
    assert!(more.may_receive(&recipient(), prompts().with(DataClass::Evidence)));
    assert!(!more.may_receive(&recipient(), Classes::all()));
    // No class is no approval.
    assert_eq!(
        DispatchPolicy::none()
            .approving(recipient(), Classes::none())
            .map(|_| ()),
        Err(NotAllowed::Empty)
    );
}

#[test]
fn a_recipient_is_the_one_approved_only_with_its_origin_its_account_and_its_upstreams() {
    let policy = approving(Classes::all());
    let bare = Recipient::new(origin(API));
    let other_account = bare
        .clone()
        .with_account(account("acct_other"))
        .forwarding_to(vec![origin(UPSTREAM)])
        .unwrap();
    let no_upstream = bare.clone().with_account(account("acct_4f8a1c9e77"));
    let one_more = no_upstream
        .clone()
        .forwarding_to(vec![origin(UPSTREAM), origin(ELSEWHERE)])
        .unwrap();
    let elsewhere = Recipient::new(origin(ELSEWHERE));
    for other in [bare, other_account, no_upstream, one_more, elsewhere] {
        assert!(!policy.may_receive(&other, prompts()), "{other:?}");
        assert!(!policy.may_receive(&other, Classes::none()), "{other:?}");
    }
}

#[test]
fn probing_is_approved_apart_from_any_class_of_data() {
    let receives = approving(Classes::all());
    assert!(!receives.may_probe(&recipient()));
    let probed = DispatchPolicy::none()
        .approving_probes_of(recipient())
        .unwrap();
    assert!(probed.may_probe(&recipient()));
    assert!(!probed.is_empty());
    // It may be probed, and may receive nothing: not even a request with no data.
    assert!(!probed.may_receive(&recipient(), prompts()));
    assert!(!probed.may_receive(&recipient(), Classes::none()));
    assert!(!probed.may_probe(&Recipient::new(origin(API))));
    let both = probed.approving(recipient(), prompts()).unwrap();
    assert!(both.may_probe(&recipient()) && both.may_receive(&recipient(), prompts()));
}

#[test]
fn a_policy_names_a_bounded_number_of_recipients() {
    let numbered = |at: usize| Recipient::new(origin(&format!("https://r{at}.example.com")));
    let mut policy = DispatchPolicy::none();
    for at in 0..APPROVED_LIMIT {
        policy = policy.approving(numbered(at), prompts()).unwrap();
    }
    let one_more = numbered(APPROVED_LIMIT);
    assert_eq!(
        policy
            .clone()
            .approving(one_more.clone(), prompts())
            .map(|_| ()),
        Err(NotAllowed::TooMany)
    );
    assert_eq!(
        policy.clone().approving_probes_of(one_more).map(|_| ()),
        Err(NotAllowed::TooMany)
    );
    // One that is named already takes no new place.
    let policy = policy.approving(numbered(0), Classes::all()).unwrap();
    let policy = policy.approving_probes_of(numbered(1)).unwrap();
    assert!(policy.may_receive(&numbered(0), Classes::all()));
    assert!(policy.may_probe(&numbered(1)));
    assert!(policy.may_receive(&numbered(APPROVED_LIMIT - 1), prompts()));
}

#[test]
fn the_policys_debug_shows_recipients_cut_and_no_credential() {
    let policy = approving(prompts())
        .approving_probes_of(recipient())
        .unwrap();
    assert_eq!(
        format!("{policy:?}"),
        "DispatchPolicy[Recipient(https://api.example.com as acct...) may receive \
         Classes(prompts) and may be probed]"
    );
    assert_eq!(format!("{:?}", DispatchPolicy::none()), "DispatchPolicy[]");
    // What an executor holds beside the policy prints no credential either: not the
    // ways out, not the check's view of an address, not the fakes.
    let key = made_up_key("I5");
    let credential = Credential::new(origin(API), "authorization", &key, &Secrets::none()).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let check = Arc::new(FakeCheck::approving());
    let ways = Ways::through(sender.clone())
        .checked_by(check.clone())
        .under(policy.clone())
        .with_credential(&recipient(), credential)
        .unwrap();
    let printed = format!("{ways:?}");
    assert_eq!(
        printed,
        format!("Ways {{ sender: true, policy: {policy:?}, credentials: 1 }}")
    );
    let ran = run_one(
        &remote(&format!("{API}/v1/fill?sig=abc"), a_prompt()),
        ways,
        Secrets::none(),
    );
    assert_eq!(ran.fault, None);
    // The credential went to the sender, and to nothing that is printed or written.
    assert_eq!(
        sender.hops()[0].headers(),
        [("authorization".to_owned(), key.clone())]
    );
    let all = format!(
        "{printed} {sender:?} {:?} {check:?} {:?}",
        sender.hops(),
        check.looked()
    );
    assert!(!all.contains(&key) && !all.contains("4f8a1c9e77") && !all.contains("sig=abc"));
    assert!(!holds(&ran.bytes, &key) && !holds(&ran.bytes, "4f8a1c9e77"));
}

// --- the checked send ------------------------------------------------------------------------

#[test]
fn with_the_default_policy_a_backend_that_names_a_recipient_hands_nothing_over_and_one_that_names_none_is_asked()
 {
    // A sender and a check that would let everything through: the policy alone, which
    // was told nothing, is what stops the send. It stops it at the gate's place,
    // before the backend is called (ADR 0015, "A wrapper around each request"): the
    // request held no prompt, and the piece its backend would have made was never
    // made, so the refusal names no class.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let check = Arc::new(FakeCheck::approving());
    let ways = Ways::through(sender.clone()).checked_by(check.clone());
    let named = remote(&format!("{API}/v1/fill"), a_prompt());
    let ran = run_one(&named, ways.clone(), Secrets::none());
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage(), fault.charge()),
        (FaultKind::Rejected, Stage::NotSent, None)
    );
    assert_eq!(
        fault.sentence(),
        "the recipient is not approved for the classes of data the request holds"
    );
    assert_eq!(sender.requests(), 0, "nothing was handed to the sender");
    assert_eq!(named.requests(), 0, "the backend was not called");
    assert_eq!(
        check.looked(),
        [],
        "the check is asked only after the policy"
    );
    assert_eq!(
        ran.refused(),
        [(
            RefusalKind::RequestNotApproved,
            "nothing was sent to https://api.example.com: the recipient is not approved for \
             the classes of data the request holds (the classes of data the request holds: \
             none known before a piece of it is made)"
                .to_owned()
        )]
    );
    assert_eq!(
        ran.went(),
        [(
            API.to_owned(),
            Some("acct...".to_owned()),
            Classes::none(),
            Handed::Refused
        )]
    );

    // A backend that names no recipient is asked, as before, and answers: nothing of
    // it is refused and nothing of it is listed.
    let local = Arc::new(FakeFiller::new(Paint::Inverse));
    let ran = run_one(&local, ways, Secrets::none());
    assert_eq!((ran.fault.clone(), local.requests()), (None, 1));
    assert_eq!((ran.refused(), ran.went()), (vec![], vec![]));
    assert_eq!(sender.requests(), 0);
}

#[test]
fn a_backend_that_names_no_recipient_and_sends_all_the_same_hands_nothing_over() {
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(approving(Classes::all()));
    let local = FakeFiller::new(Paint::Inverse).posting(address(&format!("{API}/v1")), a_prompt());
    let ran = run_one(&Arc::new(local), ways, Secrets::none());
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    assert_eq!(sender.requests(), 0);
    assert_eq!(
        ran.went(),
        [(API.to_owned(), None, prompts(), Handed::Refused)]
    );
    assert_eq!(ran.refused().len(), 1);
    assert!(
        ran.refused()[0]
            .1
            .contains("the backend names no recipient")
    );
}

#[test]
fn a_sender_with_the_default_check_hands_nothing_to_any_origin() {
    // The policy approves the recipient for everything; the check an executor has
    // until it is given one approves no address, the recipient's own included.
    for to in [format!("{API}/v1/fill"), format!("{UPSTREAM}/put/1")] {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ways = Ways::through(sender.clone()).under(approving(Classes::all()));
        let ran = run_one(&remote(&to, a_prompt()), ways, Secrets::none());
        let fault = ran.fault.clone().unwrap();
        assert_eq!(
            (fault.kind(), fault.stage()),
            (FaultKind::Rejected, Stage::NotSent)
        );
        assert_eq!(
            fault.sentence(),
            "no check approves an address: this executor sends nothing"
        );
        assert_eq!(sender.requests(), 0, "{to}");
        assert_eq!(ran.went().len(), 1);
        assert_eq!(ran.went()[0].3, Handed::Refused);
        assert_eq!(ran.refused()[0].0, RefusalKind::RequestNotApproved);
    }
    // The default check, asked by itself, says no in fixed words.
    let denied = Denied::because("  ");
    assert_eq!(denied.why(), "no reason was given");
    let _: &dyn HopCheck = &NoHop;
}

#[test]
fn a_check_that_approves_sees_each_origin_before_the_sending_fake_does() {
    // The first address answers by pointing to the upstream, which answers. The check
    // notes, each time it is asked, how many requests the sender had been handed.
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{UPSTREAM}/put/1?sig=abc")),
        ok(),
    ]));
    let check = Arc::new(FakeCheck::approving());
    let handed_when_asked = Arc::new(Mutex::new(Vec::new()));
    check.when_asked({
        let (sender, handed) = (sender.clone(), handed_when_asked.clone());
        move || handed.lock().unwrap().push(sender.requests())
    });
    let ways = Ways::through(sender.clone())
        .checked_by(check.clone())
        .under(approving(prompts()));
    let ran = run_one(
        &remote(&format!("{API}/v1/fill"), a_prompt()),
        ways,
        Secrets::none(),
    );
    assert_eq!(ran.fault, None);

    let looked = check.looked();
    let seen: Vec<_> = looked.iter().map(|l| (l.origin.clone(), l.moves)).collect();
    assert_eq!(seen, [(origin(API), 0), (origin(UPSTREAM), 1)]);
    assert!(
        looked
            .iter()
            .all(|l| l.recipient == recipient() && l.needs == prompts())
    );
    // Each origin was shown to the check before the sender was handed its request.
    assert_eq!(*handed_when_asked.lock().unwrap(), [0, 1]);
    let hops = sender.hops();
    let reached: Vec<_> = hops.iter().map(|hop| hop.to().origin().clone()).collect();
    assert_eq!(reached, [origin(API), origin(UPSTREAM)]);
    // What was handed over is in the run's record, each origin once, and nothing is
    // refused.
    assert_eq!(
        ran.went(),
        [
            (
                API.to_owned(),
                Some("acct...".to_owned()),
                prompts(),
                Handed::Sent
            ),
            (
                UPSTREAM.to_owned(),
                Some("acct...".to_owned()),
                prompts(),
                Handed::Sent
            ),
        ]
    );
    assert_eq!(ran.refused(), []);
}

#[test]
fn a_check_that_refuses_hands_nothing_over_and_its_sentence_is_the_refusal() {
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let check = Arc::new(FakeCheck::refusing("the test's check says no"));
    let ways = Ways::through(sender.clone())
        .checked_by(check.clone())
        .under(approving(prompts()));
    let ran = run_one(
        &remote(&format!("{API}/v1/fill"), a_prompt()),
        ways,
        Secrets::none(),
    );
    assert_eq!(
        ran.fault.clone().unwrap().sentence(),
        "the test's check says no"
    );
    assert_eq!((sender.requests(), check.looked().len()), (0, 1));
    assert!(ran.refused()[0].1.contains("the test's check says no"));
    assert_eq!(ran.went()[0].3, Handed::Refused);
}

#[test]
fn an_origin_that_is_not_the_recipients_or_an_upstream_is_refused_before_the_check_is_asked() {
    // The first address is the recipient's, and answers by pointing somewhere that is
    // neither it nor its upstream. Nothing is handed to that origin, and the check,
    // which would approve it, is not asked about it.
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{ELSEWHERE}/take")),
        ok(),
    ]));
    let check = Arc::new(FakeCheck::approving());
    let ways = Ways::through(sender.clone())
        .checked_by(check.clone())
        .under(approving(prompts()));
    let ran = run_one(
        &remote(&format!("{API}/v1/fill"), a_prompt()),
        ways,
        Secrets::none(),
    );
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    assert_eq!(
        fault.sentence(),
        "the origin is neither the recipient's own nor one of its upstreams"
    );
    assert_eq!(sender.requests(), 1);
    assert_eq!(sender.hops()[0].to().origin(), &origin(API));
    assert_eq!(check.looked().len(), 1);
    // The first hop had the data, and is listed as sent; the second as refused.
    assert_eq!(
        ran.went(),
        [
            (
                API.to_owned(),
                Some("acct...".to_owned()),
                prompts(),
                Handed::Sent
            ),
            (
                ELSEWHERE.to_owned(),
                Some("acct...".to_owned()),
                prompts(),
                Handed::Refused
            ),
        ]
    );
    assert_eq!(ran.refused().len(), 1);
}

#[test]
fn a_piece_with_no_class_needs_every_class_approved() {
    // A piece's class is what whoever built the piece stated (ADR 0012). The sender
    // asks the policy for the classes the pieces state, and for all four when one
    // states none.
    let unclassed = || vec![a_prompt().remove(0), Piece::unclassed(vec![1, 2, 3])];
    for (approved, pieces, sent) in [
        (prompts(), a_prompt(), true),
        (prompts(), unclassed(), false),
        (Classes::all(), unclassed(), true),
        (Classes::of(&[DataClass::SourceImages]), a_prompt(), false),
    ] {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ways = Ways::through(sender.clone())
            .checked_by(Arc::new(FakeCheck::approving()))
            .under(approving(approved));
        let needs = Outgoing::new(address(API), pieces.clone()).unwrap().needs();
        let ran = run_one(&remote(&format!("{API}/v1"), pieces), ways, Secrets::none());
        assert_eq!(ran.fault.is_none(), sent, "{approved:?} {needs:?}");
        assert_eq!(sender.requests(), u64::from(sent));
        let outcome = if sent { Handed::Sent } else { Handed::Refused };
        assert_eq!(
            ran.went(),
            [(API.to_owned(), Some("acct...".to_owned()), needs, outcome)]
        );
    }
}

#[test]
fn a_credential_is_attached_by_the_executor_for_its_own_origin_and_a_backend_brings_none() {
    let key = made_up_key("I5a");
    let secrets = Secrets::new([key.clone()]).unwrap();
    let credential = || Credential::new(origin(API), "authorization", &key, &secrets).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{UPSTREAM}/put/1")),
        ok(),
    ]));
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(approving(prompts()))
        .with_credential(&recipient(), credential())
        .unwrap();
    let ran = run_one(
        &remote(&format!("{API}/v1/fill"), a_prompt()),
        ways.clone(),
        secrets.clone(),
    );
    assert_eq!(ran.fault, None);
    // The recipient's own origin was handed the credential; the upstream was not.
    let hops = sender.hops();
    assert_eq!(
        hops[0].headers(),
        [("authorization".to_owned(), key.clone())]
    );
    assert_eq!(hops[1].headers(), []);
    assert!(!holds(&ran.bytes, &key));

    // A backend of another recipient at the same origin is handed none of it.
    let other = Recipient::new(origin(API));
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let policy = DispatchPolicy::none()
        .approving(other.clone(), prompts())
        .unwrap();
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(policy)
        .with_credential(&recipient(), credential())
        .unwrap();
    let filler = FakeFiller::new(Paint::Inverse).sending_to(other);
    let filler = Arc::new(filler.posting(address(&format!("{API}/v1")), a_prompt()));
    let ran = run_one(&filler, ways, secrets.clone());
    assert_eq!((ran.fault.clone(), sender.requests()), (None, 1));
    assert_eq!(sender.hops()[0].headers(), []);

    // What a handle hands a backend takes no request that brings a credential.
    struct Brings(
        Credential,
        Mutex<Option<Fault>>,
        whizbang_core::capability::Declaration,
    );
    impl whizbang_core::capability::Filler for Brings {
        fn declaration(&self) -> &whizbang_core::capability::Declaration {
            &self.2
        }
        fn fill(
            &self,
            _: &whizbang_core::capability::FillRequest,
            bounds: &whizbang_core::capability::Bounds,
        ) -> Result<whizbang_core::capability::Painted, Fault> {
            let outgoing = Outgoing::new(address(&format!("{API}/v1")), a_prompt()).unwrap();
            let outgoing = outgoing.with_credential(self.0.clone()).unwrap();
            let fault = bounds.sender().send(&outgoing, bounds).unwrap_err();
            *self.1.lock().unwrap() = Some(fault.clone());
            Err(fault)
        }
    }
    let declaration = FakeFiller::new(Paint::Inverse).sending_to(recipient());
    let declaration = whizbang_core::capability::Filler::declaration(&declaration).clone();
    let brings = Arc::new(Brings(credential(), Mutex::new(None), declaration));
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(approving(prompts()));
    let world = World::new();
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            drop(context.capabilities().fill().unwrap().fill(&fill_request()));
            Ok(Produced::nothing())
        },
    );
    let executor = engine(
        &world,
        roster(None, Some(brings.as_ref()), None),
        Capabilities::none().with_fill(brings.clone()),
        vec![asks],
    )
    .with_ways(ways);
    executor.submit(&Call::new("asks")).unwrap();
    let fault = brings.1.lock().unwrap().take().unwrap();
    assert_eq!(
        (fault.stage(), fault.sentence()),
        (
            Stage::NotSent,
            "the request came with a credential, and a backend holds none"
        )
    );
    assert_eq!(sender.requests(), 0);
}

#[test]
fn a_credential_is_held_for_an_origin_its_recipient_reaches_once_and_within_a_bound() {
    let none = Secrets::none();
    let of = |at: &str| Credential::new(origin(at), "x-key", "value", &none).unwrap();
    let ways = || Ways::none();
    assert!(ways().with_credential(&recipient(), of(API)).is_ok());
    assert!(ways().with_credential(&recipient(), of(UPSTREAM)).is_ok());
    assert_eq!(
        ways()
            .with_credential(&recipient(), of(ELSEWHERE))
            .map(|_| ()),
        Err(NotAllowed::Elsewhere)
    );
    let once = ways().with_credential(&recipient(), of(API)).unwrap();
    assert_eq!(
        once.clone()
            .with_credential(&recipient(), of(API))
            .map(|_| ()),
        Err(NotAllowed::Twice)
    );
    // The same origin under another recipient is another place.
    assert!(
        once.with_credential(&Recipient::new(origin(API)), of(API))
            .is_ok()
    );
    let mut full = ways();
    for at in 0..CREDENTIALS_LIMIT {
        let host = format!("https://r{at}.example.com");
        full = full
            .with_credential(&Recipient::new(origin(&host)), of(&host))
            .unwrap();
    }
    assert_eq!(
        full.with_credential(&recipient(), of(API)).map(|_| ()),
        Err(NotAllowed::TooMany)
    );
}

#[test]
fn a_send_that_says_nothing_left_is_not_listed_as_sent_and_any_other_fault_is() {
    for (stage, listed) in [
        (Stage::NotSent, false),
        (Stage::Unresolved, true),
        (Stage::Answered, true),
    ] {
        let fault = Fault::new(FaultKind::Unavailable, "the test failed it");
        let fault = match stage {
            Stage::NotSent => fault.not_sent(),
            Stage::Answered => fault.answered(None),
            Stage::Unresolved => fault.unresolved(),
        };
        let sender = Arc::new(FakeSender::scripted(vec![Err(fault)]));
        let ways = Ways::through(sender.clone())
            .checked_by(Arc::new(FakeCheck::approving()))
            .under(approving(prompts()));
        let ran = run_one(
            &remote(&format!("{API}/v1"), a_prompt()),
            ways,
            Secrets::none(),
        );
        assert_eq!(ran.fault.clone().unwrap().stage(), stage);
        assert_eq!(sender.requests(), 1);
        let sent = (
            API.to_owned(),
            Some("acct...".to_owned()),
            prompts(),
            Handed::Sent,
        );
        assert_eq!(
            ran.went(),
            if listed { vec![sent] } else { vec![] },
            "{stage:?}"
        );
        // A fault of the sender is no refusal.
        assert_eq!(ran.refused(), []);
    }
}

#[test]
fn an_executor_with_no_sender_sends_nothing_whatever_approves() {
    let check = Arc::new(FakeCheck::approving());
    let ways = Ways::none()
        .checked_by(check.clone())
        .under(approving(prompts()));
    let ran = run_one(
        &remote(&format!("{API}/v1"), a_prompt()),
        ways,
        Secrets::none(),
    );
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Unavailable, Stage::NotSent)
    );
    assert_eq!((ran.went(), ran.refused()), (vec![], vec![]));
    assert_eq!(check.looked().len(), 1);
}

#[test]
fn a_handle_that_no_executor_made_has_no_way_out() {
    let filler = remote(&format!("{API}/v1"), a_prompt());
    let handle = FillHandle::new(filler.clone(), Cancellation::new());
    let fault = handle.fill(&fill_request()).unwrap_err();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    assert_eq!(
        fault.sentence(),
        "the handle was made by no executor, and has no way to send"
    );
    assert_eq!(filler.requests(), 1);
}

#[test]
fn a_secret_the_executor_knows_is_in_no_refusal_of_a_send() {
    // An origin is made with the secrets known then. One the executor is told of
    // later is taken out of the refusal's sentence where it is noted.
    let host = "keyhost-0123456789abcdef.example.com";
    let secrets = Secrets::new([host.to_owned()]).unwrap();
    let to = format!("https://{host}/v1");
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    // The recipient is approved, so that the request passes the gate's place and its
    // backend is called: what is refused is the send, to an origin that is neither
    // the recipient's own nor one of its upstreams.
    let ran = run_one(
        &remote(&to, a_prompt()),
        Ways::through(sender.clone()).under(approving(Classes::all())),
        secrets,
    );
    assert_eq!(sender.requests(), 0);
    let refused = ran.refused();
    assert_eq!(refused.len(), 1);
    assert!(!refused[0].1.contains(host), "{}", refused[0].1);
    assert!(!ran.fault.clone().unwrap().sentence().contains("keyhost"));
    // Nor is it in the list of where data went, which writes an origin as it is: an
    // origin that holds a secret is not listed, and the refusal stands for it. No
    // file of the store holds the host.
    assert_eq!(ran.went(), []);
    assert!(!holds(&ran.store, "keyhost"));
    assert!(holds(&ran.store, "request_not_approved"));
}

#[test]
fn an_origin_that_holds_a_secret_cannot_be_listed_so_nothing_is_sent_to_it() {
    // Everything approves: the recipient is at the host, the policy approves it and
    // the check lets every address through. The destination is itself a secret the
    // executor knows, which a record cannot list; so nothing is handed over.
    let host = "keyhost-0123456789abcdef.example.com";
    let secrets = Secrets::new([host.to_owned()]).unwrap();
    let there = Recipient::new(origin(&format!("https://{host}")));
    let filler = FakeFiller::new(Paint::Inverse).sending_to(there.clone());
    let filler = Arc::new(filler.posting(address(&format!("https://{host}/v1")), a_prompt()));
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let policy = DispatchPolicy::none().approving(there, prompts()).unwrap();
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(policy);
    let ran = run_one(&filler, ways, secrets);
    assert_eq!(sender.requests(), 0, "nothing was handed over");
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    assert_eq!(ran.went(), []);
    let refused = ran.refused();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, RefusalKind::RequestNotApproved);
    assert!(!holds(&ran.store, "keyhost"));
}

#[test]
fn a_check_that_panics_hands_nothing_over_and_the_next_run_goes() {
    let world = World::new();
    let sender = Arc::new(FakeSender::scripted(vec![ok(), ok()]));
    let check = Arc::new(FakeCheck::approving());
    let ways = Ways::through(sender.clone())
        .checked_by(check.clone())
        .under(approving(prompts()));
    let filler = remote(&format!("{API}/v1"), a_prompt());
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap();
            Ok(Produced::nothing())
        },
    );
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![asks],
    )
    .with_ways(ways);
    check.when_asked(|| panic!("the check went wrong"));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Error);
    assert_eq!(sender.requests(), 0, "nothing was handed over");
    assert_eq!(record.egress().len(), 0);
    // The executor goes on: the next run sends, and ends as any other.
    check.when_asked(|| {});
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!((sender.requests(), record.egress().len()), (1, 1));
}

// --- what the record says of a send, and a sender that outlives its run ------------------------

/// A filler that does what a test tells it with the bounds of each request, and then
/// answers with a fault. It names the recipient.
struct Custom {
    declaration: Declaration,
    with: Box<dyn Fn(&Bounds) + Send + Sync>,
}

fn custom(with: impl Fn(&Bounds) + Send + Sync + 'static) -> Arc<Custom> {
    let model = ModelIdentity::new("fake", "fake-filler", "1");
    let declaration = Declaration::new(model, &[]).unwrap();
    Arc::new(Custom {
        declaration: declaration.sending_to(recipient()),
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

fn a_prompt_to(to: &str) -> Outgoing {
    Outgoing::new(address(to), a_prompt()).unwrap()
}

/// An executor with one action, `asks`, that asks this filler once and then does
/// what the test gives it, over these ways out.
fn asking(
    world: &World,
    filler: Arc<Custom>,
    ways: Ways,
    then: impl Fn(&whizbang_core::contract::Context) + Send + Sync + 'static,
) -> whizbang_core::contract::Engine {
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        move |_, context| {
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap_err();
            then(context);
            Ok(Produced::nothing())
        },
    );
    let held = Capabilities::none().with_fill(filler.clone());
    engine(
        world,
        roster(None, Some(filler.as_ref()), None),
        held,
        vec![asks],
    )
    .with_ways(ways)
}

fn open_ways(sender: Arc<dyn Sender>) -> Ways {
    Ways::through(sender)
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(approving(prompts()))
}

fn sent_to(record: &RunRecord) -> Vec<String> {
    let sent = record.egress().iter();
    sent.filter(|went| went.outcome() == Handed::Sent)
        .map(|went| went.origin().to_owned())
        .collect()
}

#[test]
fn a_send_whose_entry_cannot_be_listed_is_refused_and_nothing_is_handed_over() {
    // The list of where the run's data went is filled first, by the run's own notes.
    // A send that everything approves has then no entry to be listed in: it is
    // refused, and nothing reaches the sender behind.
    let world = World::new();
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let fault = Arc::new(Mutex::new(None));
    let filler = custom({
        let fault = fault.clone();
        move |bounds| {
            let sent = bounds
                .sender()
                .send(&a_prompt_to(&format!("{API}/v1")), bounds);
            *fault.lock().unwrap() = sent.err();
        }
    });
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            for at in 0..128 {
                let to = origin(&format!("https://host-{at}.example.org"));
                assert!(
                    context.facts().went(&to, None, prompts(), Handed::Sent),
                    "{at}"
                );
            }
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap_err();
            Ok(Produced::nothing())
        },
    );
    let held = Capabilities::none().with_fill(filler.clone());
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        held,
        vec![asks],
    )
    .with_ways(open_ways(sender.clone()));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(sender.requests(), 0, "nothing was handed over");
    let fault = fault.lock().unwrap().clone().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    assert_eq!(record.egress().len(), 128);
    assert!(!sent_to(&record).contains(&API.to_owned()));
    // The refusal is in the record.
    let refusals = &record.outcome().refusals;
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].kind(), RefusalKind::RequestNotApproved);
}

#[test]
fn refused_sends_cannot_use_up_the_room_of_a_send() {
    // 128 sends are refused, each for another origin. Half the list is all that
    // refusals may take: an approved send after them is handed over, and listed.
    let world = World::new();
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let answers = Arc::new(Mutex::new(Vec::new()));
    let filler = custom({
        let answers = answers.clone();
        move |bounds| {
            for at in 0..128 {
                let to = a_prompt_to(&format!("https://host-{at}.example.org/v1"));
                let fault = bounds.sender().send(&to, bounds).unwrap_err();
                assert_eq!(fault.stage(), Stage::NotSent, "{at}");
            }
            let sent = bounds
                .sender()
                .send(&a_prompt_to(&format!("{API}/v1")), bounds);
            answers.lock().unwrap().push(sent.is_ok());
        }
    });
    let executor = asking(&world, filler, open_ways(sender.clone()), |_| {});
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, names) = stored(&world, finished.run());
    assert_eq!(*answers.lock().unwrap(), [true]);
    assert_eq!(sender.requests(), 1);
    assert_eq!(sent_to(&record), [API.to_owned()]);
    let refused = record.egress().iter();
    assert_eq!(
        refused.filter(|w| w.outcome() == Handed::Refused).count(),
        64
    );
    // Every refused send is among the run's refusals, listed or counted.
    let refusals = &record.outcome().refusals;
    assert!(!refusals.is_empty());
    // The 64th refused send still had its entry; the 65th had none, and says so.
    let unlisted = "not listed in where the run's data went";
    assert!(
        !refusals[63].why().contains(unlisted),
        "{}",
        refusals[63].why()
    );
    assert!(
        refusals[64].why().contains(unlisted),
        "{}",
        refusals[64].why()
    );
    assert!(holds(&names["record"], "request_not_approved"));
}

#[test]
fn a_sender_kept_past_its_run_hands_nothing_over_after_the_run_or_in_a_later_one() {
    // A backend lives across runs and can keep the bounds of a request, which carry
    // the checked sender. That sender is good for its run only.
    let world = World::new();
    let sender = Arc::new(FakeSender::scripted(vec![ok(), ok(), ok()]));
    let kept: Arc<Mutex<Option<Bounds>>> = Arc::new(Mutex::new(None));
    let to = format!("{API}/v1");
    let filler = custom({
        let (kept, to) = (kept.clone(), to.clone());
        move |bounds| {
            let mut kept = kept.lock().unwrap();
            match kept.as_ref() {
                // The first run: the request is sent, and its bounds are kept.
                None => {
                    bounds.sender().send(&a_prompt_to(&to), bounds).unwrap();
                    *kept = Some(bounds.clone());
                }
                // A later run: the backend sends through what it kept.
                Some(old) => {
                    let fault = old.sender().send(&a_prompt_to(&to), old).unwrap_err();
                    assert_eq!(
                        (fault.kind(), fault.stage()),
                        (FaultKind::Rejected, Stage::NotSent)
                    );
                }
            }
        }
    });
    let executor = asking(&world, filler, open_ways(sender.clone()), |_| {});
    let first = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(sender.requests(), 1);
    let (record, names) = stored(&world, first.run());
    assert_eq!(sent_to(&record), [API.to_owned()]);

    // After the run has ended.
    let old = kept.lock().unwrap().clone().unwrap();
    let fault = old.sender().send(&a_prompt_to(&to), &old).unwrap_err();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Rejected, Stage::NotSent)
    );
    assert_eq!(
        fault.sentence(),
        "the run this sender was made for is over, and nothing is sent for it"
    );
    assert_eq!(
        sender.requests(),
        1,
        "nothing was handed over after the run"
    );

    // In a later run: nothing is handed over, the later run's record lists nothing,
    // and the first run's record is as it was.
    let second = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(
        sender.requests(),
        1,
        "nothing was handed over in a later run"
    );
    let (later, _) = stored(&world, second.run());
    assert_eq!(later.egress().len(), 0);
    let (_, again) = stored(&world, first.run());
    assert_eq!(names["record"], again["record"]);
}

#[test]
fn a_send_that_races_its_runs_end_is_in_the_record_or_was_refused() {
    // A thread the backend left behind sends through kept bounds, to one origin
    // after another, while the run ends. Whatever the order, each request the
    // sender behind was handed is in the record, and each that is not was refused.
    let upstreams: Vec<String> = (0..16)
        .map(|at| format!("https://up-{at}.example.net"))
        .collect();
    let there = Recipient::new(origin(API))
        .forwarding_to(upstreams.iter().map(|up| origin(up)).collect())
        .unwrap();
    let world = World::new();
    let sender = Arc::new(FakeSender::scripted((0..16).map(|_| ok()).collect()));
    let policy = DispatchPolicy::none()
        .approving(there.clone(), prompts())
        .unwrap();
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(policy);
    let (started, begun) = std::sync::mpsc::channel::<()>();
    let begun = Mutex::new(begun);
    let left = Arc::new(Mutex::new(None));
    let model = ModelIdentity::new("fake", "fake-filler", "1");
    let filler = Arc::new(Custom {
        declaration: Declaration::new(model, &[]).unwrap().sending_to(there),
        with: Box::new({
            let (left, upstreams) = (left.clone(), upstreams.clone());
            let started = Mutex::new(started);
            move |bounds| {
                let (bounds, upstreams) = (bounds.clone(), upstreams.clone());
                let started = started.lock().unwrap().clone();
                let thread = whizbang_testkit::threads::spawn(move || {
                    let mut refused = 0;
                    for (at, up) in upstreams.iter().enumerate() {
                        if at == 1 {
                            started.send(()).unwrap();
                        }
                        let to = a_prompt_to(&format!("{up}/put"));
                        if bounds.sender().send(&to, &bounds).is_err() {
                            refused += 1;
                        }
                    }
                    refused
                });
                *left.lock().unwrap() = Some(thread);
            }
        }),
    });
    // The action comes back once the thread has sent its first request, and the
    // run's end begins while the thread goes on.
    let executor = asking(&world, filler, ways, move |_| {
        begun.lock().unwrap().recv().unwrap();
    });
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let thread = left.lock().unwrap().take().unwrap();
    let refused: usize = thread.join().unwrap();
    let (record, _) = stored(&world, finished.run());
    let hops = sender.hops();
    let mut handed: Vec<String> = hops
        .iter()
        .map(|hop| hop.to().origin().to_string())
        .collect();
    handed.sort();
    let mut listed = sent_to(&record);
    listed.sort();
    assert_eq!(handed, listed);
    assert_eq!(handed.len() + refused, 16);
    assert!(!handed.is_empty());
}

#[test]
fn a_refusal_between_two_hops_does_not_set_the_moves_back_and_the_total_never_goes_back() {
    let world = World::new();
    let up = format!("{UPSTREAM}/put");
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&up),
        ok(),
        moved(&up),
        ok(),
    ]));
    let check = Arc::new(FakeCheck::approving());
    let ways = Ways::through(sender.clone())
        .checked_by(check.clone())
        .under(approving(prompts()));
    let filler = custom({
        let up = up.clone();
        move |bounds| {
            let send = |to: &str| bounds.sender().send(&a_prompt_to(to), bounds);
            // An answer that points elsewhere; then a request that is refused, for
            // an origin the recipient does not reach; then the address pointed to.
            assert!(send(&format!("{API}/v1")).unwrap().moved_to().is_some());
            assert_eq!(
                send(&format!("{ELSEWHERE}/x")).unwrap_err().stage(),
                Stage::NotSent
            );
            assert!(send(&up).unwrap().moved_to().is_none());
            // The answer set the count in a row back; the total stays.
            assert!(send(&format!("{API}/v1")).unwrap().moved_to().is_some());
            assert!(send(&up).unwrap().moved_to().is_none());
        }
    });
    let executor = asking(&world, filler, ways, |_| {});
    executor.submit(&Call::new("asks")).unwrap();
    let looked = check.looked();
    let seen: Vec<_> = looked
        .iter()
        .map(|l| (l.origin.clone(), l.moves, l.total))
        .collect();
    assert_eq!(
        seen,
        [
            (origin(API), 0, 0),
            (origin(UPSTREAM), 1, 1),
            (origin(API), 0, 1),
            (origin(UPSTREAM), 1, 2),
        ]
    );
}

/// A sender that panics when it is handed a request, and counts each.
struct Panics(std::sync::atomic::AtomicU64);

impl Sender for Panics {
    fn send(&self, _: &Outgoing, _: &Bounds) -> Result<Reply, Fault> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        panic!("the sender went wrong");
    }
}

#[test]
fn a_sender_that_panics_is_listed_as_sent_its_run_ends_as_an_error_and_the_next_run_goes() {
    let world = World::new();
    let panics = Arc::new(Panics(std::sync::atomic::AtomicU64::new(0)));
    let filler = remote(&format!("{API}/v1"), a_prompt());
    let asks = action(
        "asks",
        vec![Requirement::new(Capability::Fill)],
        |_, context| {
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap();
            Ok(Produced::nothing())
        },
    );
    let build = |ways: Ways| {
        engine(
            &world,
            roster(None, Some(filler.as_ref()), None),
            Capabilities::none().with_fill(filler.clone()),
            vec![asks.clone()],
        )
        .with_ways(ways)
    };
    let finished = build(open_ways(panics.clone()))
        .submit(&Call::new("asks"))
        .unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Error);
    assert_eq!(panics.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    // What a sender that unwound did with the request is not known: it is listed.
    assert_eq!(sent_to(&record), [API.to_owned()]);
    // The next run, over the same store, sends and ends as any other.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let finished = build(open_ways(sender.clone()))
        .submit(&Call::new("asks"))
        .unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!((sender.requests(), sent_to(&record).len()), (1, 1));
}

#[test]
fn a_fault_that_states_no_stage_is_listed_as_sent_and_a_cancellation_only_when_it_says_nothing_left()
 {
    let plain = |kind| Fault::new(kind, "the test failed it");
    for (fault, listed) in [
        (plain(FaultKind::Unavailable), true),
        (plain(FaultKind::Cancelled), true),
        (plain(FaultKind::Cancelled).not_sent(), false),
    ] {
        let sender = Arc::new(FakeSender::scripted(vec![Err(fault.clone())]));
        let ran = run_one(
            &remote(&format!("{API}/v1"), a_prompt()),
            open_ways(sender.clone()),
            Secrets::none(),
        );
        assert_eq!(sender.requests(), 1);
        assert_eq!(ran.fault.clone().unwrap().kind(), fault.kind());
        let sent: Vec<_> = ran.went().into_iter().map(|went| went.3).collect();
        let expected = if listed { vec![Handed::Sent] } else { vec![] };
        assert_eq!(sent, expected, "{fault:?}");
        assert_eq!(ran.refused(), []);
    }
}

#[test]
fn a_note_that_is_taken_back_leaves_what_the_other_sends_to_the_origin_hold() {
    // Two requests to one origin: a prompt, which is answered, and evidence, of
    // which the sender says nothing left. The entry stays, with the prompt alone.
    let world = World::new();
    let nothing_left = Fault::new(FaultKind::Unavailable, "the test failed it").not_sent();
    let sender = Arc::new(FakeSender::scripted(vec![ok(), Err(nothing_left), ok()]));
    let both = Classes::of(&[DataClass::Prompts, DataClass::Evidence]);
    let ways = Ways::through(sender.clone())
        .checked_by(Arc::new(FakeCheck::approving()))
        .under(approving(both));
    let filler = custom(|bounds| {
        let to = address(&format!("{API}/v1"));
        let evidence = vec![Piece::of(DataClass::Evidence, b"a log".to_vec())];
        let evidence = Outgoing::new(to.clone(), evidence).unwrap();
        bounds
            .sender()
            .send(&a_prompt_to(&format!("{API}/v1")), bounds)
            .unwrap();
        bounds.sender().send(&evidence, bounds).unwrap_err();
        // And to the upstream, evidence alone, which is answered.
        let up = address(&format!("{UPSTREAM}/put"));
        let evidence = vec![Piece::of(DataClass::Evidence, b"a log".to_vec())];
        let evidence = Outgoing::new(up, evidence).unwrap();
        bounds.sender().send(&evidence, bounds).unwrap();
    });
    let executor = asking(&world, filler, ways, |_| {});
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(sender.requests(), 3);
    let went: Vec<_> = record.egress().iter().collect();
    let listed: Vec<_> = went.iter().map(|w| (w.origin(), w.classes())).collect();
    let evidence = Classes::of(&[DataClass::Evidence]);
    assert_eq!(listed, [(API, prompts()), (UPSTREAM, evidence)]);
}
