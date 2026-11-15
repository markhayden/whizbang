//! The gate: denied by default (ADR 0019; plan 1B, task E2). The policy a config's
//! `egress` table makes, and the check behind a send for a first address, each through
//! an executor a test builds with them. Nothing is sent anywhere: the sender behind is
//! the test kit's fake.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Stage,
};
use whizbang_core::config::Config;
use whizbang_core::contract::record::{Handed, Status};
use whizbang_core::contract::{
    Call, Capabilities, Capability, Executor, ModelIdentity, Produced, RefusalKind, Requirement,
    Secrets,
};
use whizbang_core::egress::{
    APPROVED_LIMIT, Classes, Credential, DataClass, Denied, DispatchPolicy, Hop, HopCheck,
    NotAllowed, Outgoing, Piece, Recipient, SendCheck, Ways,
};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::capabilities::{FakeFiller, FakeJudge, FakeSegmenter, Paint, roster};
use whizbang_testkit::sending::FakeSender;
use whizbang_testkit::threads;

use super::support::{
    ACCOUNT, ACCOUNT_CUT, API, ELSEWHERE, UPSTREAM, World, a_prompt, account, action, address,
    approving, config, engine, entry, fill_request, holds, made_up_key, moved, ok, origin, policy,
    policy_of, prompts, recipient, run_one, stored,
};

/// What the check answers once an answer has pointed its backend elsewhere.
const MOVED: &str = "an answer pointed this backend elsewhere in this run, and the address \
                     of no request after that is approved";

const NOT_APPROVED: &str =
    "the recipient is not approved for the classes of data the request holds";

fn source() -> Classes {
    Classes::of(&[DataClass::SourceImages])
}

fn a_source_image() -> Vec<Piece> {
    vec![Piece::of(DataClass::SourceImages, vec![7; 24])]
}

/// The ways out an executor is wired with: the fake sender, the strand's check, and
/// the policy this `egress` table makes.
fn ways(sender: &Arc<FakeSender>, table: &str) -> Ways {
    Ways::through(sender.clone())
        .checked_by(Arc::new(SendCheck::new()))
        .under(policy(table))
}

/// A filler that names this recipient and sends these pieces to this address for
/// every request, through what its bounds hand it.
fn remote(to_whom: Recipient, to: &str, pieces: Vec<Piece>) -> Arc<FakeFiller> {
    let filler = FakeFiller::new(Paint::Inverse).sending_to(to_whom);
    Arc::new(filler.posting(address(to), pieces))
}

fn refused_as(fault: &Fault) -> (FaultKind, Stage, &str) {
    (fault.kind(), fault.stage(), fault.sentence())
}

fn approval<'a>(config: &'a Config, name: &str) -> &'a whizbang_core::config::Approval {
    let found = config.recipients().find(|(named, _)| *named == name);
    found.map(|(_, approval)| approval).unwrap()
}

// --- the policy a config's egress table makes -------------------------------------------------

#[test]
fn a_config_that_names_no_recipient_makes_the_policy_that_approves_nothing() {
    for text in ["", "[egress]\n", "[egress.recipients]\n"] {
        let made = policy(text);
        assert_eq!(made, DispatchPolicy::none(), "{text:?}");
        assert!(made.is_empty());
    }
    assert_eq!(policy_of(&Config::empty()), DispatchPolicy::none());
    // An iterator of nothing is the same.
    assert_eq!(
        DispatchPolicy::from_approvals(std::iter::empty()),
        Ok(DispatchPolicy::none())
    );
}

#[test]
fn the_policy_a_configs_egress_table_makes_approves_each_recipient_for_its_classes_and_no_other() {
    let read = config(
        "[egress.recipients.fills]\n\
         origin = \"https://api.example.com\"\n\
         account = \"acct_4f8a1c9e77\"\n\
         upstreams = [\"https://uploads.example.net\"]\n\
         classes = [\"prompts\", \"source_images\"]\n\
         probe = true\n\
         [egress.recipients.named-for-nothing]\n\
         origin = \"https://nothing.example.com\"\n\
         classes = []\n\
         [egress.recipients.probed-only]\n\
         origin = \"https://probed.example.com\"\n\
         classes = []\n\
         probe = true\n\
         [egress.recipients.judge]\n\
         origin = \"https://judge.example.com:8443\"\n\
         classes = [\"evidence\", \"derived_images\", \"prompts\", \"source_images\"]\n",
    );
    let made = policy_of(&read);
    let fills = approval(&read, "fills").recipient();
    assert_eq!(fills, &recipient());
    let both = prompts().with(DataClass::SourceImages);
    for approved in [Classes::none(), prompts(), source(), both] {
        assert!(made.may_receive(fills, approved), "{approved:?}");
    }
    for class in [DataClass::DerivedImages, DataClass::Evidence] {
        assert!(!made.may_receive(fills, Classes::of(&[class])));
        assert!(!made.may_receive(fills, both.with(class)));
    }
    assert!(!made.may_receive(fills, Classes::all()));
    assert!(made.may_probe(fills));

    // Named with no class and no probing: approved for nothing, not even a request
    // that holds no data.
    let nothing = approval(&read, "named-for-nothing").recipient();
    assert!(!made.may_receive(nothing, Classes::none()));
    assert!(!made.may_receive(nothing, prompts()));
    assert!(!made.may_probe(nothing));

    // Probing is approved apart from any class of data.
    let probed = approval(&read, "probed-only").recipient();
    assert!(made.may_probe(probed));
    assert!(!made.may_receive(probed, Classes::none()));
    assert!(!made.may_receive(probed, prompts()));

    let judge = approval(&read, "judge").recipient();
    assert!(made.may_receive(judge, Classes::all()));
    assert!(!made.may_probe(judge));

    // The policy is the one built by hand from the same approvals, and no more: a
    // recipient the file named for nothing has no entry in it.
    let by_hand = DispatchPolicy::none()
        .approving(fills.clone(), both)
        .and_then(|policy| policy.approving_probes_of(fills.clone()))
        .and_then(|policy| policy.approving(judge.clone(), Classes::all()))
        .and_then(|policy| policy.approving_probes_of(probed.clone()))
        .unwrap();
    assert_eq!(made, by_hand);
    // A recipient the file does not name is approved for nothing.
    let stranger = Recipient::new(origin(ELSEWHERE));
    assert!(!made.may_receive(&stranger, Classes::none()) && !made.may_probe(&stranger));
}

#[test]
fn two_approvals_of_one_origin_under_two_accounts_are_two_recipients_and_a_backend_with_no_account_gets_only_the_approval_with_none()
 {
    let two = "[egress.recipients.team]\n\
               origin = \"https://api.example.com\"\n\
               account = \"team-1234\"\n\
               classes = [\"prompts\"]\n\
               [egress.recipients.lab]\n\
               origin = \"https://api.example.com\"\n\
               account = \"lab-5678\"\n\
               classes = [\"source_images\"]\n\
               probe = true\n";
    let at = |name: Option<&str>| {
        let bare = Recipient::new(origin(API));
        name.map_or(bare.clone(), |name| bare.with_account(account(name)))
    };
    let made = policy(two);
    // Each account has its own approval, and not the other's.
    assert!(made.may_receive(&at(Some("team-1234")), prompts()));
    assert!(!made.may_receive(&at(Some("team-1234")), source()));
    assert!(!made.may_probe(&at(Some("team-1234"))));
    assert!(made.may_receive(&at(Some("lab-5678")), source()));
    assert!(!made.may_receive(&at(Some("lab-5678")), prompts()));
    assert!(made.may_probe(&at(Some("lab-5678"))));
    // A backend that declares no account, at the same origin, gets neither: only an
    // approval with no account is its own, and this file has none. An account is
    // compared whole, so one that shares what is shown of it gets nothing either.
    for other in [at(None), at(Some("team-9999")), at(Some("team"))] {
        assert!(!made.may_receive(&other, Classes::none()), "{other:?}");
        assert!(!made.may_receive(&other, prompts()), "{other:?}");
        assert!(!made.may_probe(&other), "{other:?}");
    }

    // With an approval that names no account beside them, that one is what a backend
    // with no account gets, and a backend with an account does not get it.
    let three = format!(
        "{two}[egress.recipients.open]\norigin = \"https://api.example.com\"\nclasses = \
         [\"evidence\"]\n"
    );
    let made = policy(&three);
    let evidence = Classes::of(&[DataClass::Evidence]);
    assert!(made.may_receive(&at(None), evidence));
    assert!(!made.may_receive(&at(None), prompts()));
    assert!(!made.may_receive(&at(None), source()));
    assert!(!made.may_probe(&at(None)));
    assert!(!made.may_receive(&at(Some("team-1234")), evidence));
    assert!(!made.may_receive(&at(Some("lab-5678")), evidence));
    assert!(made.may_receive(&at(Some("team-1234")), prompts()));
}

#[test]
fn a_backend_that_declares_other_upstreams_than_its_approval_is_approved_for_nothing() {
    let made = policy(&approving(&["prompts"]));
    assert!(made.may_receive(&recipient(), prompts()));
    let bare = Recipient::new(origin(API)).with_account(account(ACCOUNT));
    let one_more = bare
        .clone()
        .forwarding_to(vec![origin(UPSTREAM), origin(ELSEWHERE)])
        .unwrap();
    let another = bare.clone().forwarding_to(vec![origin(ELSEWHERE)]).unwrap();
    // One upstream more, another upstream, and none at all: each is another recipient
    // than the one the file approved.
    for other in [one_more, another, bare] {
        assert!(!made.may_receive(&other, prompts()), "{other:?}");
        assert!(!made.may_receive(&other, Classes::none()), "{other:?}");
    }
}

#[test]
fn approvals_that_name_one_recipient_twice_or_more_than_a_policy_holds_make_no_policy() {
    // A config names a recipient once, so the same approvals given twice name each
    // twice: no policy is made of them, and nothing is approved by half of them.
    let read = config(&approving(&["prompts"]));
    let once = || read.recipients().map(|(_, approval)| approval);
    assert!(DispatchPolicy::from_approvals(once()).is_ok());
    assert_eq!(
        DispatchPolicy::from_approvals(once().chain(once())),
        Err(NotAllowed::Twice)
    );
    // One origin and one account with other upstreams, which two files can say and
    // one cannot, is the same recipient named twice: which approval held would be a
    // guess.
    let other = config(
        "[egress.recipients.again]\norigin = \"https://api.example.com\"\naccount = \
         \"acct_4f8a1c9e77\"\nclasses = [\"evidence\"]\n",
    );
    let again = || other.recipients().map(|(_, approval)| approval);
    assert_eq!(
        DispatchPolicy::from_approvals(once().chain(again())),
        Err(NotAllowed::Twice)
    );
    // Named for nothing, a recipient is still named: a second naming is refused.
    let for_nothing = config(
        "[egress.recipients.nothing]\norigin = \"https://api.example.com\"\naccount = \
         \"acct_4f8a1c9e77\"\nclasses = []\n",
    );
    let nothing = || for_nothing.recipients().map(|(_, approval)| approval);
    assert_eq!(
        DispatchPolicy::from_approvals(nothing().chain(once())),
        Err(NotAllowed::Twice)
    );
    // The same origin under no account is another recipient.
    let open = config(
        "[egress.recipients.open]\norigin = \"https://api.example.com\"\nclasses = \
         [\"evidence\"]\n",
    );
    let unnamed = || open.recipients().map(|(_, approval)| approval);
    assert!(DispatchPolicy::from_approvals(once().chain(unnamed())).is_ok());

    // More approvals than a policy names recipients: three files of 32, 32 and 1.
    let numbered = |from: usize, to: usize| {
        let tables: Vec<String> = (from..to)
            .map(|at| {
                format!(
                    "[egress.recipients.r{at}]\norigin = \"https://r{at}.example.com\"\nclasses \
                     = [\"prompts\"]\n"
                )
            })
            .collect();
        config(&tables.concat())
    };
    let (first, second, third) = (numbered(0, 32), numbered(32, 64), numbered(64, 65));
    let of = |config: &'_ Config| -> Vec<whizbang_core::config::Approval> {
        let approvals = config.recipients().map(|(_, approval)| approval.clone());
        approvals.collect()
    };
    let mut all = of(&first);
    all.extend(of(&second));
    assert_eq!(all.len(), APPROVED_LIMIT);
    let full = DispatchPolicy::from_approvals(&all).unwrap();
    assert!(full.may_receive(
        &Recipient::new(origin("https://r63.example.com")),
        prompts()
    ));
    all.extend(of(&third));
    assert_eq!(
        DispatchPolicy::from_approvals(&all),
        Err(NotAllowed::TooMany)
    );
    // An approval for nothing is one of them: the list is bounded where it comes in,
    // by how many approvals there are and not by how many approve something.
    let idle =
        config("[egress.recipients.idle]\norigin = \"https://idle.example.com\"\nclasses = []\n");
    all.truncate(APPROVED_LIMIT);
    all.extend(of(&idle));
    assert_eq!(
        DispatchPolicy::from_approvals(&all),
        Err(NotAllowed::TooMany)
    );
    // And 64 of which one approves nothing are taken.
    all.remove(0);
    assert!(DispatchPolicy::from_approvals(&all).is_ok());
}

#[test]
fn a_recipient_written_another_way_is_the_same_recipient_named_twice_and_the_one_approved() {
    // One recipient written two ways: the host in capitals, the scheme's own port
    // written, and the upstreams in the other order.
    let plain = config(
        "[egress.recipients.plain]\n\
         origin = \"https://api.example.com\"\n\
         account = \"acct_4f8a1c9e77\"\n\
         upstreams = [\"https://uploads.example.net\", \"https://elsewhere.example.org\"]\n\
         classes = [\"prompts\"]\n",
    );
    let other = config(
        "[egress.recipients.other]\n\
         origin = \"HTTPS://API.Example.COM:443\"\n\
         account = \"acct_4f8a1c9e77\"\n\
         upstreams = [\"https://ELSEWHERE.example.org:443\", \"https://uploads.example.net\"]\n\
         classes = [\"evidence\"]\n",
    );
    let (first, second) = (approval(&plain, "plain"), approval(&other, "other"));
    assert_eq!(first.recipient(), second.recipient());
    // Across two files it is named twice, in either order.
    assert_eq!(
        DispatchPolicy::from_approvals([first, second]),
        Err(NotAllowed::Twice)
    );
    assert_eq!(
        DispatchPolicy::from_approvals([second, first]),
        Err(NotAllowed::Twice)
    );
    assert_eq!(
        NotAllowed::Twice.to_string(),
        "an origin is named twice, or is the recipient's own, or a recipient is approved twice"
    );
    // And a backend that declares it the other way is the recipient the file approved.
    let made = policy_of(&plain);
    let declared = Recipient::new(origin("HTTPS://API.EXAMPLE.COM:443/"))
        .with_account(account(ACCOUNT))
        .forwarding_to(vec![
            origin("https://Elsewhere.Example.org"),
            origin("https://uploads.example.net:443"),
        ])
        .unwrap();
    assert!(made.may_receive(&declared, prompts()));
    assert!(policy_of(&other).may_receive(&declared, Classes::of(&[DataClass::Evidence])));
    assert!(!made.may_receive(&declared, Classes::of(&[DataClass::Evidence])));
}

// --- the check behind a send, for a first address ---------------------------------------------

#[test]
fn with_the_empty_policy_every_request_to_a_backend_with_a_recipient_is_refused_and_the_sending_fake_is_handed_none()
 {
    // The policy of a config that names no recipient, behind the strand's check and a
    // sender that would take everything: whatever a request holds and whichever of
    // the recipient's origins it is for, nothing is handed over.
    let pieces: [Vec<Piece>; 5] = [
        Vec::new(),
        a_prompt(),
        a_source_image(),
        vec![Piece::unclassed(b"words".to_vec())],
        vec![
            Piece::of(DataClass::DerivedImages, vec![1; 8]),
            Piece::of(DataClass::Evidence, b"seen".to_vec()),
        ],
    ];
    for to in [format!("{API}/v1/fill"), format!("{UPSTREAM}/put")] {
        for pieces in &pieces {
            let sender = Arc::new(FakeSender::scripted(vec![ok()]));
            let filler = remote(recipient(), &to, pieces.clone());
            let ran = run_one(filler.clone(), ways(&sender, ""), Secrets::none());
            let fault = ran.fault.clone().unwrap();
            assert_eq!(
                refused_as(&fault),
                (FaultKind::Rejected, Stage::NotSent, NOT_APPROVED),
                "{to} {pieces:?}"
            );
            assert_eq!(fault.charge(), None);
            assert_eq!(sender.requests(), 0, "{to} {pieces:?}");
            assert_eq!(ran.refused().len(), 1);
            assert_eq!(ran.refused()[0].0, RefusalKind::RequestNotApproved);
            let listed = ran.went();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].3, Handed::Refused);
        }
    }
}

#[test]
fn known_until_the_gates_place_is_built_a_backend_that_is_refused_was_still_called() {
    // The plan refuses a request at the gate's place, before its backend is called
    // (task I3). That place is not built: the backend is called once, and what it
    // cannot do is send. Task I3 turns this pin round.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let filler = remote(recipient(), &format!("{API}/v1/fill"), a_prompt());
    let ran = run_one(filler.clone(), ways(&sender, ""), Secrets::none());
    assert!(ran.fault.is_some());
    assert_eq!((filler.requests(), sender.requests()), (1, 0));
}

#[test]
fn a_recipient_approved_for_prompts_and_not_for_source_images_is_refused_a_source_image_and_sent_a_prompt_alone()
 {
    let table = approving(&["prompts"]);
    let to = format!("{API}/v1/fill");

    // A source image, alone and beside a prompt: refused, and nothing is handed over.
    let mut with_a_prompt = a_source_image();
    with_a_prompt.extend(a_prompt());
    for pieces in [a_source_image(), with_a_prompt] {
        let needs = Outgoing::new(address(&to), pieces.clone()).unwrap().needs();
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(
            remote(recipient(), &to, pieces),
            ways(&sender, &table),
            Secrets::none(),
        );
        let fault = ran.fault.clone().unwrap();
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::NotSent, NOT_APPROVED)
        );
        assert_eq!(sender.requests(), 0);
        assert_eq!(
            ran.went(),
            [entry(API, Some(ACCOUNT_CUT), needs, Handed::Refused)]
        );
    }

    // A prompt alone: sent, with the credential the executor holds for that origin.
    let key = made_up_key("E2");
    let credential = Credential::new(origin(API), "authorization", &key, &Secrets::none()).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let ways = ways(&sender, &table)
        .with_credential(&recipient(), credential)
        .unwrap();
    let ran = run_one(
        remote(recipient(), &to, a_prompt()),
        ways,
        Secrets::new([key.clone()]).unwrap(),
    );
    assert_eq!(ran.fault, None);
    let hops = sender.hops();
    assert_eq!(hops.len(), 1);
    assert_eq!(hops[0].to(), &address(&to));
    assert_eq!(hops[0].outgoing().pieces(), a_prompt());
    assert_eq!(
        hops[0].headers(),
        [("authorization".to_owned(), key.clone())]
    );
    assert_eq!(ran.refused(), []);
    assert_eq!(
        ran.went(),
        [entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent)]
    );
    // The key is in no byte the store holds, and the account is nowhere whole.
    assert!(!holds(&ran.store, &key) && !holds(&ran.store, ACCOUNT));
}

#[test]
fn a_part_with_no_stated_class_is_refused_unless_every_class_is_approved() {
    let unclassed = || vec![Piece::unclassed(b"words".to_vec())];
    let to = format!("{API}/v1/fill");
    let names = ["source_images", "derived_images", "prompts", "evidence"];
    // Approved for any three of the four, the recipient is refused the part.
    for left_out in names {
        let three: Vec<&str> = names.into_iter().filter(|name| *name != left_out).collect();
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(
            remote(recipient(), &to, unclassed()),
            ways(&sender, &approving(&three)),
            Secrets::none(),
        );
        let fault = ran.fault.clone().unwrap();
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::NotSent, NOT_APPROVED),
            "without {left_out}"
        );
        assert_eq!(sender.requests(), 0, "without {left_out}");
        assert_eq!(
            ran.went(),
            [entry(
                API,
                Some(ACCOUNT_CUT),
                Classes::all(),
                Handed::Refused
            )]
        );
    }
    // Approved for all four, it is sent, beside a part that states its class.
    let mut pieces = unclassed();
    pieces.extend(a_prompt());
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let ran = run_one(
        remote(recipient(), &to, pieces.clone()),
        ways(&sender, &approving(&names)),
        Secrets::none(),
    );
    assert_eq!(ran.fault, None);
    assert_eq!(sender.hops()[0].outgoing().pieces(), pieces);
    assert_eq!(
        ran.went(),
        [entry(API, Some(ACCOUNT_CUT), Classes::all(), Handed::Sent)]
    );
}

#[test]
fn under_the_strands_check_a_request_is_still_held_to_what_its_backend_declared_and_reaches() {
    // The check is asked last and widens nothing: a backend that declares no
    // recipient, one that declares another than the file approves, and an address
    // outside the recipient are each refused before it, under a policy that approves
    // the recipient for everything.
    let table = approving(&["source_images", "derived_images", "prompts", "evidence"]);
    let to = format!("{API}/v1/fill");
    let local = Arc::new(FakeFiller::new(Paint::Inverse).posting(address(&to), a_prompt()));
    let other_account = Recipient::new(origin(API))
        .with_account(account("acct_other"))
        .forwarding_to(vec![origin(UPSTREAM)])
        .unwrap();
    let cases: [(Arc<FakeFiller>, &str, Option<&str>); 3] = [
        (
            local,
            "the backend names no recipient, so nothing it is asked leaves the machine",
            None,
        ),
        (
            remote(other_account, &to, a_prompt()),
            NOT_APPROVED,
            Some("acct..."),
        ),
        (
            remote(recipient(), &format!("{ELSEWHERE}/v1/fill"), a_prompt()),
            "the origin is neither the recipient's own nor one of its upstreams",
            Some(ACCOUNT_CUT),
        ),
    ];
    for (filler, why, shown) in cases {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(filler, ways(&sender, &table), Secrets::none());
        let fault = ran.fault.clone().unwrap();
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::NotSent, why)
        );
        assert_eq!(sender.requests(), 0, "{why}");
        let listed = ran.went();
        assert_eq!(listed.len(), 1, "{why}");
        assert_eq!(
            (listed[0].1.as_deref(), listed[0].3),
            (shown, Handed::Refused)
        );
    }
}

#[test]
fn a_refusal_answers_the_action_with_a_fault_and_no_other_backend_is_asked_in_its_place() {
    // The executor holds a remote filler whose recipient the file does not approve
    // for a source image, a local segmenter, and a remote judge whose recipient the
    // file approves for everything. The action is handed all three and asks the
    // filler: it is answered with the fault, and nothing asks the segmenter, the
    // judge or the sender in the filler's place.
    let judges = Recipient::new(origin("https://judge.example.com"));
    let table = format!(
        "{}[egress.recipients.judge]\norigin = \"https://judge.example.com\"\nclasses = \
         [\"source_images\", \"derived_images\", \"prompts\", \"evidence\"]\n",
        approving(&["prompts"])
    );
    let sender = Arc::new(FakeSender::scripted(vec![ok(), ok()]));
    let filler = remote(recipient(), &format!("{API}/v1/fill"), a_source_image());
    let segmenter = Arc::new(FakeSegmenter::new());
    let judge = Arc::new(FakeJudge::scripted(Vec::new()).sending_to(judges));
    let world = World::new();
    let answered = Arc::new(Mutex::new(None));
    let asks = {
        let answered = answered.clone();
        let needs = vec![
            Requirement::new(Capability::Fill),
            Requirement::new(Capability::Segment),
            Requirement::new(Capability::Judge),
        ];
        action("asks", needs, move |_, context| {
            let handles = context.capabilities();
            assert!(handles.segment().is_some() && handles.judge().is_some());
            let fault = handles.fill().unwrap().fill(&fill_request()).unwrap_err();
            *answered.lock().unwrap() = Some(fault);
            Ok(Produced::nothing())
        })
    };
    let held = Capabilities::none()
        .with_fill(filler.clone())
        .with_segment(segmenter.clone())
        .with_judge(judge.clone());
    let offered = roster(
        Some(segmenter.as_ref()),
        Some(filler.as_ref()),
        Some(judge.as_ref()),
    );
    let executor = engine(&world, offered, held, vec![asks]).with_ways(ways(&sender, &table));
    let finished = executor.submit(&Call::new("asks")).unwrap();
    let fault = answered.lock().unwrap().take().unwrap();
    assert_eq!(
        refused_as(&fault),
        (FaultKind::Rejected, Stage::NotSent, NOT_APPROVED)
    );
    assert_eq!(fault.charge(), None, "a refused request states no charge");
    assert_eq!((segmenter.requests(), judge.requests()), (0, 0));
    assert_eq!(sender.requests(), 0);
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    // The record names one request, the filler's, and no model but its own.
    assert_eq!(record.egress().len(), 1);
    assert_eq!(record.outcome().refusals.len(), 1);
}

#[test]
fn a_run_whose_action_catches_the_refusal_and_completes_has_it_in_its_record_with_the_recipient_and_the_reason()
 {
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let mut pieces = a_source_image();
    pieces.extend(a_prompt());
    let filler = remote(recipient(), &format!("{API}/v1/fill?sig=abc123"), pieces);
    let ran = run_one(
        filler,
        ways(&sender, &approving(&["prompts"])),
        Secrets::none(),
    );
    assert_eq!(ran.record.outcome().status, Status::Completed);
    assert!(ran.fault.is_some());
    assert_eq!(
        ran.refused(),
        [(
            RefusalKind::RequestNotApproved,
            "nothing was sent to https://api.example.com: the recipient is not approved for \
             the classes of data the request holds (the classes of data the request holds: \
             source_images, prompts)"
                .to_owned()
        )]
    );
    // The recipient: its origin and its account, cut; the classes; and that nothing
    // was sent.
    assert_eq!(
        ran.went(),
        [entry(
            API,
            Some(ACCOUNT_CUT),
            source().with(DataClass::Prompts),
            Handed::Refused
        )]
    );
    // Nothing of the address but its origin, and nothing of the account past its
    // cut, is in the record or anywhere in the store.
    for bytes in [&ran.bytes, &ran.store] {
        assert!(!holds(bytes, "sig=abc123") && !holds(bytes, "/v1/fill"));
        assert!(!holds(bytes, ACCOUNT));
    }
}

#[test]
fn the_check_approves_a_first_address_and_no_address_an_answer_pointed_to() {
    // The recipient is approved for a prompt, and its upstream is one it reaches. The
    // first address is sent to; it answers with another address, at the upstream and
    // then at its own origin, and the request the backend then makes of that is
    // refused by the check, in fixed words. Following an answer is task E3's.
    for next in [
        format!("{UPSTREAM}/put?sig=abc123"),
        format!("{API}/v2/fill?sig=abc123"),
    ] {
        let sender = Arc::new(FakeSender::scripted(vec![moved(&next), ok()]));
        let filler = remote(recipient(), &format!("{API}/v1/fill"), a_prompt());
        let ran = run_one(
            filler,
            ways(&sender, &approving(&["prompts"])),
            Secrets::none(),
        );
        let fault = ran.fault.clone().unwrap();
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::NotSent, MOVED)
        );
        let hops = sender.hops();
        assert_eq!(
            hops.len(),
            1,
            "the address the answer pointed to was handed nothing"
        );
        assert_eq!(hops[0].to().origin(), &origin(API));
        let there = address(&next).origin().to_string();
        let mut listed = vec![entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent)];
        listed.push(entry(&there, Some(ACCOUNT_CUT), prompts(), Handed::Refused));
        assert_eq!(ran.went(), listed);
        assert_eq!(
            ran.refused(),
            [(
                RefusalKind::RequestNotApproved,
                format!(
                    "nothing was sent to {there}: {MOVED} (the classes of data the request \
                     holds: prompts)"
                )
            )]
        );
        assert!(!holds(&ran.store, "sig=abc123"));
    }

    // An upstream is approved as a first address: an upload address comes in an
    // answer, and the request to it is one no answer pointed to.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let filler = remote(recipient(), &format!("{UPSTREAM}/put"), a_prompt());
    let ran = run_one(
        filler,
        ways(&sender, &approving(&["prompts"])),
        Secrets::none(),
    );
    assert_eq!((ran.fault.clone(), sender.requests()), (None, 1));
    assert_eq!(
        ran.went(),
        [entry(UPSTREAM, Some(ACCOUNT_CUT), prompts(), Handed::Sent)]
    );
}

/// A filler that names the recipient and does what a test tells it with the bounds
/// of each request, and then answers with a fault.
struct Custom {
    declaration: Declaration,
    with: Box<dyn Fn(&Bounds) + Send + Sync>,
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

/// The strand's check, which stops the first request it has answered at a barrier,
/// after it answered and before the request is handed over.
struct Stopping {
    check: SendCheck,
    asked: AtomicU32,
    barriers: Barriers,
}

impl HopCheck for Stopping {
    fn look(&self, hop: &Hop<'_>) -> Result<(), Denied> {
        let answer = self.check.look(hop);
        if self.asked.fetch_add(1, Ordering::SeqCst) == 0 {
            self.barriers.stop_at("looked");
        }
        answer
    }
}

#[test]
fn a_request_that_was_answered_after_an_answer_pointed_elsewhere_does_not_make_the_next_address_a_first_one()
 {
    // Two requests through one handle at once. The first is looked at while no answer
    // has pointed elsewhere, and is held there. The second is sent and is pointed
    // elsewhere. Then the first is sent and answered, which sets the moves in a row
    // back to nothing. The address the second was pointed to is still not a first
    // address: the check goes by the moves of the whole run, which never go back.
    let barriers = Barriers::new();
    let check = Arc::new(Stopping {
        check: SendCheck::new(),
        asked: AtomicU32::new(0),
        barriers: barriers.clone(),
    });
    let next = format!("{API}/v2/fill");
    let sender = Arc::new(FakeSender::scripted(vec![moved(&next), ok(), ok()]));
    let ways = Ways::through(sender.clone())
        .checked_by(check)
        .under(policy(&approving(&["prompts"])));
    let refused = Arc::new(Mutex::new(None));
    let model = ModelIdentity::new("fake", "fake-filler", "1");
    let filler = Arc::new(Custom {
        declaration: Declaration::new(model, &[])
            .unwrap()
            .sending_to(recipient()),
        with: Box::new({
            let (refused, next) = (refused.clone(), next.clone());
            move |bounds| {
                let first = Outgoing::new(address(&format!("{API}/v1/fill")), a_prompt()).unwrap();
                // Two threads show the race: the one made here, and this one.
                threads::scope(|scope| {
                    let held = scope.spawn(|| bounds.sender().send(&first, bounds));
                    barriers.arrived("looked", 1);
                    let pointed = bounds.sender().send(&first, bounds);
                    // Let go before anything is asserted, so that a failure ends the
                    // test and does not leave the other thread at the barrier.
                    barriers.release("looked");
                    let answered = held.join().unwrap();
                    assert_eq!(pointed.unwrap().moved_to(), Some(&address(&next)));
                    assert_eq!(answered.unwrap().status(), Some(200));
                });
                let on = Outgoing::new(address(&next), a_prompt()).unwrap();
                *refused.lock().unwrap() = bounds.sender().send(&on, bounds).err();
            }
        }),
    });
    let ran = run_one(filler, ways, Secrets::none());
    assert!(ran.fault.is_some());
    let fault = refused.lock().unwrap().take().unwrap();
    assert_eq!(
        refused_as(&fault),
        (FaultKind::Rejected, Stage::NotSent, MOVED)
    );
    assert_eq!(sender.requests(), 2);
}

#[test]
fn what_is_printed_of_the_check_is_its_name_and_how_many_folders_it_keeps_out() {
    assert_eq!(
        format!("{:?}", SendCheck::new()),
        "SendCheck(0 folders kept out)"
    );
}
