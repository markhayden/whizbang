//! The run's recipients (ADR 0019; plan 1B, task E4): each send is noted where it is
//! made, at the checked sender, and a run's `egress` list is those notes: the origin
//! that was reached and the classes of data, once for each origin, and a send that was
//! refused as refused. Against the test kit's sending fake: nothing is sent anywhere.

use std::sync::Arc;

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Stage,
};
use whizbang_core::contract::record::Handed;
use whizbang_core::contract::{ModelIdentity, RefusalKind, Secrets};
use whizbang_core::egress::{
    Classes, Credential, DataClass, DispatchPolicy, NotApproved, Outgoing, Piece, Recipient,
    SendCheck, Ways,
};
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::sending::FakeSender;

use super::support::{
    ACCOUNT, ACCOUNT_CUT, API, ELSEWHERE, UPSTREAM, a_prompt, account, address, approving, entry,
    holds, made_up_key, moved, ok, origin, policy, prompts, recipient, run_one,
};

const CLASSES: &str = "the recipient is not approved for the classes of data the request holds";
const ACCOUNT_DIFFERS: &str = "the recipient's origin is approved, but not with the account \
                               the backend declares";
const UPSTREAMS_DIFFER: &str = "the recipient's origin and account are approved, but not \
                                with the upstreams the backend declares";

fn ways(sender: &Arc<FakeSender>, policy: DispatchPolicy) -> Ways {
    Ways::through(sender.clone())
        .checked_by(Arc::new(SendCheck::new()))
        .under(policy)
}

/// A filler that names this recipient and sends a prompt to this address for every
/// request, going on to each address an answer points to.
fn remote(to_whom: Recipient, to: &str) -> Arc<FakeFiller> {
    let filler = FakeFiller::new(Paint::Inverse).sending_to(to_whom);
    Arc::new(filler.posting(address(to), a_prompt()))
}

fn a_derived_image() -> Vec<Piece> {
    vec![Piece::of(DataClass::DerivedImages, vec![9; 24])]
}

/// A filler that names the recipient, sends each of these in turn through what its
/// bounds hand it, and then paints.
struct Sends {
    declaration: Declaration,
    each: Vec<(String, Vec<Piece>)>,
}

impl Filler for Sends {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        for (to, pieces) in &self.each {
            let outgoing = Outgoing::new(address(to), pieces.clone()).unwrap();
            bounds.sender().send(&outgoing, bounds)?;
        }
        let (width, height) = (request.image().width(), request.image().height());
        let rgb = vec![0; width as usize * height as usize * 3];
        Ok(Painted::from_rgb(width, height, rgb)?)
    }
}

fn sends(each: &[(&str, Vec<Piece>)]) -> Arc<Sends> {
    let model = ModelIdentity::new("fake", "fake-filler", "1");
    Arc::new(Sends {
        declaration: Declaration::new(model, &[])
            .unwrap()
            .sending_to(recipient()),
        each: each
            .iter()
            .map(|(to, pieces)| ((*to).to_owned(), pieces.clone()))
            .collect(),
    })
}

// --- what a run lists ---------------------------------------------------------------------------

#[test]
fn a_run_that_sent_a_prompt_and_a_derived_image_to_one_recipient_lists_it_once_with_both_classes() {
    let sender = Arc::new(FakeSender::scripted(vec![ok(), ok(), ok()]));
    let filler = sends(&[
        (&format!("{API}/v1/words"), a_prompt()),
        (&format!("{API}/v1/pictures?sig=abc123"), a_derived_image()),
        (&format!("{API}/v1/words"), a_prompt()),
    ]);
    let approved = policy(&approving(&["prompts", "derived_images"]));
    let ran = run_one(filler, ways(&sender, approved), Secrets::none());
    assert_eq!(ran.fault, None);
    assert_eq!(sender.requests(), 3);
    // Three sends to one origin: one entry, with the classes of all of them.
    let both = prompts().with(DataClass::DerivedImages);
    assert_eq!(
        ran.went(),
        [entry(API, Some(ACCOUNT_CUT), both, Handed::Sent)]
    );
    assert_eq!(ran.refused(), []);
    // As the record writes it, which is what `runs show` reads: the classes in the
    // record's own order, and nothing of an address but its origin.
    let written = "\"egress\":[{\"origin\":\"https://api.example.com\",\"account\":\"acct...\",\
                   \"classes\":[\"derived_images\",\"prompts\"],\"outcome\":\"sent\"}]";
    assert!(holds(&ran.bytes, written));
    assert!(!holds(&ran.store, "sig=abc123") && !holds(&ran.store, "/v1/words"));
    assert!(ran.record.outcome().lost.is_empty());
}

#[test]
fn a_run_whose_request_went_on_to_an_approved_upstream_lists_it_as_reached_and_one_that_was_not_reached_is_not_listed()
 {
    // A recipient that may forward to two origins, approved with both.
    let other = "https://cdn.example.net";
    let forwards = Recipient::new(origin(API))
        .with_account(account(ACCOUNT))
        .forwarding_to(vec![origin(UPSTREAM), origin(other)])
        .unwrap();
    let approved = DispatchPolicy::none()
        .approving(forwards.clone(), prompts())
        .unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{UPSTREAM}/put")),
        ok(),
    ]));
    let ran = run_one(
        remote(forwards, &format!("{API}/v1/fill")),
        ways(&sender, approved),
        Secrets::none(),
    );
    assert_eq!(ran.fault, None);
    assert_eq!(
        ran.went(),
        [
            entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent),
            entry(UPSTREAM, Some(ACCOUNT_CUT), prompts(), Handed::Sent),
        ]
    );
    // The upstream that was approved and not reached is nowhere in the record.
    assert!(!holds(&ran.bytes, "cdn.example.net"));
}

#[test]
fn a_request_that_failed_at_its_second_hop_lists_the_first_which_had_the_data() {
    let second = |fault: Fault| {
        let sender = Arc::new(FakeSender::scripted(vec![
            moved(&format!("{UPSTREAM}/put")),
            Err(fault),
        ]));
        let approved = policy(&approving(&["prompts"]));
        let ran = run_one(
            remote(recipient(), &format!("{API}/v1/fill")),
            ways(&sender, approved),
            Secrets::none(),
        );
        // Both were handed the prompt.
        assert_eq!(sender.requests(), 2);
        ran
    };
    let first = entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent);
    // The second hop says nothing left for it: the first is listed, and it alone.
    let nothing_left = Fault::new(FaultKind::Unavailable, "no route").not_sent();
    let ran = second(nothing_left.clone());
    assert_eq!(ran.fault, Some(nothing_left));
    assert_eq!(ran.went(), std::slice::from_ref(&first));
    assert_eq!(ran.refused(), []);
    // The second hop failed with no answer that settles it: the data may have left
    // for it, so it is listed too, after the first.
    let ran = second(Fault::new(FaultKind::TimedOut, "no answer").unresolved());
    assert_eq!(
        ran.fault.as_ref().map(Fault::stage),
        Some(Stage::Unresolved)
    );
    assert_eq!(
        ran.went(),
        [
            first,
            entry(UPSTREAM, Some(ACCOUNT_CUT), prompts(), Handed::Sent)
        ]
    );
}

#[test]
fn a_run_whose_only_request_was_refused_lists_the_refusal_and_nothing_as_sent() {
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    // Approved for source images, and asked to take a prompt.
    let approved = policy(&approving(&["source_images"]));
    let ran = run_one(
        remote(recipient(), &format!("{API}/v1/fill")),
        ways(&sender, approved),
        Secrets::none(),
    );
    assert_eq!(sender.requests(), 0);
    assert_eq!(
        ran.went(),
        [entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Refused)]
    );
    assert_eq!(
        ran.refused(),
        [(
            RefusalKind::RequestNotApproved,
            format!(
                "nothing was sent to {API}: {CLASSES} (the classes of data the request holds: \
                 prompts)"
            )
        )]
    );
    assert!(holds(&ran.bytes, "\"outcome\":\"refused\""));
    assert!(!holds(&ran.bytes, "\"outcome\":\"sent\""));
}

#[test]
fn a_local_runs_list_is_empty() {
    // A backend that names no recipient and sends nothing.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let approved = policy(&approving(&["prompts"]));
    let local = Arc::new(FakeFiller::new(Paint::Inverse));
    let ran = run_one(local, ways(&sender, approved), Secrets::none());
    assert_eq!(ran.fault, None);
    assert_eq!(ran.went(), []);
    assert_eq!(ran.refused(), []);
    assert!(holds(&ran.bytes, "\"egress\":[]"));
    assert!(ran.record.outcome().lost.is_empty());
    assert_eq!(sender.requests(), 0);
}

#[test]
fn an_account_in_the_list_is_shown_cut_and_a_marker_credential_is_nowhere_in_the_record() {
    let key = made_up_key("E4");
    let credential = Credential::new(origin(API), "authorization", &key, &Secrets::none()).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let approved = policy(&approving(&["prompts"]));
    let ways = ways(&sender, approved)
        .with_credential(&recipient(), credential)
        .unwrap();
    let ran = run_one(
        remote(recipient(), &format!("{API}/v1/fill")),
        ways,
        Secrets::new([key.clone()]).unwrap(),
    );
    assert_eq!(ran.fault, None);
    // The credential went with the request.
    assert_eq!(
        sender.hops()[0].headers(),
        [("authorization".to_owned(), key.clone())]
    );
    assert_eq!(
        ran.went(),
        [entry(API, Some(ACCOUNT_CUT), prompts(), Handed::Sent)]
    );
    // The account is cut wherever it is written, and the credential is in no file of
    // the store, whole or by its start.
    assert!(holds(&ran.bytes, "\"account\":\"acct...\""));
    assert!(!holds(&ran.store, ACCOUNT) && !holds(&ran.store, &ACCOUNT[..8]));
    assert!(!holds(&ran.store, &key) && !holds(&ran.store, &key[..14]));
    assert!(!holds(&ran.store, "authorization"));
}

// --- why a recipient is not approved ------------------------------------------------------------

#[test]
fn a_recipient_that_is_not_the_one_approved_is_refused_in_words_that_say_which_part_differs() {
    // The config approves the origin under one account, forwarding to one upstream.
    let approved = policy(&approving(&["prompts"]));
    let under = |text: &str| Recipient::new(origin(API)).with_account(account(text));
    let to_upstream = |recipient: Recipient| recipient.forwarding_to(vec![origin(UPSTREAM)]);
    let cases: [(Recipient, &str, Option<&str>); 4] = [
        // Another account, with the approved upstream.
        (
            to_upstream(under("acct_other_0001")).unwrap(),
            ACCOUNT_DIFFERS,
            Some("acct..."),
        ),
        // No account at all.
        (
            to_upstream(Recipient::new(origin(API))).unwrap(),
            ACCOUNT_DIFFERS,
            None,
        ),
        // The approved account, with no upstream, and with one more.
        (under(ACCOUNT), UPSTREAMS_DIFFER, Some(ACCOUNT_CUT)),
        (
            under(ACCOUNT)
                .forwarding_to(vec![origin(UPSTREAM), origin(ELSEWHERE)])
                .unwrap(),
            UPSTREAMS_DIFFER,
            Some(ACCOUNT_CUT),
        ),
    ];
    for (declared, why, cut) in cases {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(
            remote(declared.clone(), &format!("{API}/v1/fill")),
            ways(&sender, approved.clone()),
            Secrets::none(),
        );
        let fault = ran.fault.clone().unwrap();
        assert_eq!(
            (fault.kind(), fault.stage(), fault.sentence()),
            (FaultKind::Rejected, Stage::NotSent, why),
            "{declared:?}"
        );
        assert_eq!(sender.requests(), 0);
        assert_eq!(
            ran.went(),
            [entry(API, cut, prompts(), Handed::Refused)],
            "{declared:?}"
        );
        assert_eq!(
            ran.refused(),
            [(
                RefusalKind::RequestNotApproved,
                format!(
                    "nothing was sent to {API}: {why} (the classes of data the request \
                     holds: prompts)"
                )
            )]
        );
        // No value is in it: not an account, whole or cut, and not an upstream.
        for value in ["acct", "uploads.example.net", "elsewhere"] {
            assert!(!why.contains(value), "{why}");
        }
        assert!(!holds(&ran.store, "acct_other_0001") && !holds(&ran.store, ACCOUNT));
    }
}

#[test]
fn the_policy_says_why_a_recipient_may_not_receive_and_nothing_when_it_may() {
    let approved = policy(&approving(&["prompts"]));
    let evidence = Classes::of(&[DataClass::Evidence]);
    // Whenever it says nothing, the recipient may receive, and the other way round.
    let asked = |policy: &DispatchPolicy, recipient: &Recipient, classes: Classes| {
        let why = policy.why_not(recipient, classes);
        assert_eq!(why.is_none(), policy.may_receive(recipient, classes));
        why
    };
    assert_eq!(asked(&approved, &recipient(), prompts()), None);
    assert_eq!(asked(&approved, &recipient(), Classes::none()), None);
    // The recipient as it was approved, and a class it was not approved for.
    assert_eq!(
        asked(&approved, &recipient(), evidence),
        Some(NotApproved::Classes)
    );
    // An origin no approval names is said as the classes are: it is approved for
    // none.
    let stranger = Recipient::new(origin(ELSEWHERE));
    assert_eq!(
        asked(&approved, &stranger, prompts()),
        Some(NotApproved::Classes)
    );
    assert_eq!(
        asked(&DispatchPolicy::none(), &recipient(), prompts()),
        Some(NotApproved::Classes)
    );
    // Another account: said as the account, whatever the upstreams and the classes.
    let other = Recipient::new(origin(API)).with_account(account("acct_other_0001"));
    assert_eq!(
        asked(&approved, &other, evidence),
        Some(NotApproved::Account)
    );
    // The same account and other upstreams: said as the upstreams.
    let bare = Recipient::new(origin(API)).with_account(account(ACCOUNT));
    assert_eq!(
        asked(&approved, &bare, prompts()),
        Some(NotApproved::Upstreams)
    );
    // An origin approved under two accounts: the one with the same account is the
    // one that is compared, in whichever order they were approved.
    let both = [bare.clone(), other.clone()];
    for order in [[0, 1], [1, 0]] {
        let two = order.iter().fold(DispatchPolicy::none(), |policy, at| {
            policy.approving(both[*at].clone(), prompts()).unwrap()
        });
        assert_eq!(
            asked(&two, &recipient(), prompts()),
            Some(NotApproved::Upstreams)
        );
        assert_eq!(asked(&two, &other, prompts()), None);
    }
    // A recipient that may only be probed is one the policy names: approved for no
    // class, and another account of its origin is said as the account.
    let probed = DispatchPolicy::none()
        .approving_probes_of(recipient())
        .unwrap();
    assert_eq!(
        asked(&probed, &recipient(), prompts()),
        Some(NotApproved::Classes)
    );
    assert_eq!(
        asked(&probed, &other, prompts()),
        Some(NotApproved::Account)
    );
    // Each is said in fixed words.
    assert_eq!(NotApproved::Classes.why(), CLASSES);
    assert_eq!(NotApproved::Account.why(), ACCOUNT_DIFFERS);
    assert_eq!(NotApproved::Upstreams.why(), UPSTREAMS_DIFFER);
}
