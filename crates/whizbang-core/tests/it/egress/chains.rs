//! A download and a probe (ADR 0019; plan 1B, task E3, the second of its two pull
//! requests): a model's download is held to the origins it was handed, a probe to a
//! recipient the policy approves for probing, and each address either is pointed to is
//! checked as its first is. Against the test kit's fakes: nothing is fetched and
//! nothing is probed.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use whizbang_core::capability::{Fault, FaultKind, Stage};
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::{
    Address, CheckedDownloader, CheckedProber, Credential, DOWNLOAD_ORIGINS_LIMIT, DispatchPolicy,
    DownloadRequest, Downloaded, Downloader, MOVES_LIMIT, NotAllowed, Origin, ProbeRequest, Probed,
    Prober, Recipient,
};
use whizbang_testkit::downloads::{Ended, FakeDownloader};
use whizbang_testkit::sending::FakeProber;

use super::support::{
    ACCOUNT, API, ELSEWHERE, UPSTREAM, account, address, approving, made_up_key, origin, policy,
    recipient,
};

const WEIGHTS: &str = "https://weights.example.org";
const MIRROR: &str = "https://mirror.example.net";

/// What a checked download answers of an origin it was not handed.
const OFF: &str = "the origin is not one this download may reach";
const DOWNLOAD_MOVED: &str = "the download was pointed elsewhere more than 5 times, and is \
                              not followed further";
const NOT_PROBED: &str = "the recipient is not approved for probing, so nothing is asked of it";
const NOT_REACHED: &str = "the origin is neither the recipient's own nor one of its upstreams";
const PROBE_MOVED: &str = "the probe was pointed elsewhere more than 5 times, and is not \
                           followed further";

fn refused_as(fault: &Fault) -> (FaultKind, Stage, &str) {
    (fault.kind(), fault.stage(), fault.sentence())
}

/// A file of a few kilobytes, no two bytes in a row the same.
fn file() -> Vec<u8> {
    (0..2500u32).map(|at| (at % 251) as u8).collect()
}

/// The origins a catalogue's entry names for its files.
fn catalogue() -> Vec<Origin> {
    vec![origin(WEIGHTS), origin(MIRROR)]
}

fn checked(fake: &Arc<FakeDownloader>) -> CheckedDownloader {
    CheckedDownloader::new(fake.clone(), &catalogue()).unwrap()
}

fn asking(from: &Address) -> DownloadRequest {
    DownloadRequest::new(from.clone(), 4096).unwrap()
}

/// Downloads through the checked downloader, and answers how it ended with what the
/// sink was handed.
fn download(
    through: &CheckedDownloader,
    request: &DownloadRequest,
) -> (Result<Downloaded, Fault>, Vec<u8>) {
    let mut kept = Vec::new();
    let ended = through.download(request, &Cancellation::new(), &mut |piece| {
        kept.extend_from_slice(piece);
        true
    });
    (ended, kept)
}

fn credential(at: &str, tail: &str) -> Credential {
    Credential::new(
        origin(at),
        "x-api-key",
        &made_up_key(tail),
        &Secrets::none(),
    )
    .unwrap()
}

// --- a download --------------------------------------------------------------------------------

#[test]
fn a_download_redirected_off_its_catalogues_origins_is_refused_and_its_request_carried_nothing_but_the_address()
 {
    let first = address("https://weights.example.org/m/graph.onnx");
    // Off the origins by host, by scheme and by port.
    for elsewhere in [
        "https://elsewhere.example.org/g?sig=abc",
        "http://mirror.example.net/g",
        "https://mirror.example.net:8443/g",
    ] {
        let elsewhere = address(elsewhere);
        let fake = Arc::new(
            FakeDownloader::new()
                .moved(&first, elsewhere.clone())
                .serving(&elsewhere, file()),
        );
        let (ended, kept) = download(&checked(&fake), &asking(&first));
        let fault = ended.unwrap_err();
        // The first address was asked, so the fault does not say that nothing was
        // sent.
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::Unresolved, OFF)
        );
        // Nothing was asked of the address it was pointed to, and no byte came.
        assert_eq!(fake.asked(&elsewhere), 0);
        assert!(kept.is_empty());
        // The one request that was made carried the address and nothing else: no
        // header, and a download has no place for a body.
        let requests = fake.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].from(), &first);
        assert!(requests[0].headers().is_empty());
        assert_eq!(
            (requests[0].handed(), requests[0].ended()),
            (0, Ended::Moved)
        );
        // The refusal quotes nothing of either address.
        let shown = format!("{fault:?}");
        assert!(
            !shown.contains("example") && !shown.contains("sig"),
            "{shown}"
        );
    }
}

#[test]
fn a_download_moved_to_one_of_its_origins_is_followed_and_handed_whole() {
    let first = address("https://weights.example.org/m/graph.onnx");
    let mirror = address("https://mirror.example.net/g?sig=abc");
    let fake = Arc::new(
        FakeDownloader::new()
            .moved(&first, mirror.clone())
            .serving(&mirror, file()),
    );
    let (ended, kept) = download(&checked(&fake), &asking(&first));
    assert_eq!(ended.unwrap(), Downloaded::Bytes(2500));
    assert_eq!(kept, file());
    // The second request asked for as much as the first, at the address it was
    // pointed to.
    let requests = fake.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        (requests[1].from(), requests[1].most_bytes()),
        (&mirror, 4096)
    );
    // A sink's no is the sink's: the download ends there, answered.
    let through = checked(&fake);
    let mut pieces = 0;
    let ended = through.download(&asking(&first), &Cancellation::new(), &mut |_| {
        pieces += 1;
        false
    });
    assert_eq!((ended.unwrap(), pieces), (Downloaded::Bytes(1024), 1));
}

#[test]
fn a_download_whose_first_address_is_off_its_origins_asks_nothing() {
    let elsewhere = address("https://elsewhere.example.org/g");
    let fake = Arc::new(FakeDownloader::new().serving(&elsewhere, file()));
    let (ended, kept) = download(&checked(&fake), &asking(&elsewhere));
    assert_eq!(
        refused_as(&ended.unwrap_err()),
        (FaultKind::Rejected, Stage::NotSent, OFF)
    );
    assert!(fake.requests().is_empty() && kept.is_empty());
    // A download handed no origin reaches none.
    let first = address("https://weights.example.org/m/graph.onnx");
    let fake = Arc::new(FakeDownloader::new().serving(&first, file()));
    let none = CheckedDownloader::new(fake.clone(), &[]).unwrap();
    let (ended, _) = download(&none, &asking(&first));
    assert_eq!(
        refused_as(&ended.unwrap_err()),
        (FaultKind::Rejected, Stage::NotSent, OFF)
    );
    assert!(fake.requests().is_empty());
}

/// A fake that serves the file after this many moves, each within the mirror.
fn chain(moves: u32) -> (Arc<FakeDownloader>, Address) {
    let first = address("https://weights.example.org/m/graph.onnx");
    let mut fake = FakeDownloader::new();
    let mut from = first.clone();
    for hop in 0..moves {
        let to = address(&format!("https://mirror.example.net/hop/{hop}"));
        fake = fake.moved(&from, to.clone());
        from = to;
    }
    (Arc::new(fake.serving(&from, file())), first)
}

#[test]
fn a_download_pointed_elsewhere_more_often_than_the_limit_is_refused_and_each_download_counts_its_own()
 {
    assert_eq!(MOVES_LIMIT, 5);
    let (fake, first) = chain(MOVES_LIMIT);
    let through = checked(&fake);
    // Twice through one checked downloader: the count is one download's, and starts
    // at nothing for the next.
    for round in 1..=2 {
        let (ended, kept) = download(&through, &asking(&first));
        assert_eq!(ended.unwrap(), Downloaded::Bytes(2500));
        assert_eq!(kept, file());
        assert_eq!(fake.requests().len(), round * 6);
    }
    let (fake, first) = chain(MOVES_LIMIT + 1);
    let (ended, kept) = download(&checked(&fake), &asking(&first));
    let fault = ended.unwrap_err();
    assert_eq!(
        (fault.kind(), fault.sentence()),
        (FaultKind::Rejected, DOWNLOAD_MOVED)
    );
    // The sixth move was answered, and the address it named was not asked.
    assert_eq!(fake.requests().len(), 6);
    assert_eq!(fake.asked(&address("https://mirror.example.net/hop/5")), 0);
    assert!(kept.is_empty());
}

#[test]
fn a_downloads_credential_goes_to_the_origin_it_is_bound_to_and_no_other() {
    let first = address("https://weights.example.org/m/graph.onnx");
    let again = address("https://weights.example.org/v2/graph.onnx");
    let mirror = address("https://mirror.example.net/g");
    let fake = Arc::new(
        FakeDownloader::new()
            .moved(&first, again.clone())
            .moved(&again, mirror.clone())
            .serving(&mirror, file()),
    );
    let request = asking(&first)
        .with_credential(credential(WEIGHTS, "w"))
        .unwrap();
    let (ended, _) = download(&checked(&fake), &request);
    assert_eq!(ended.unwrap(), Downloaded::Bytes(2500));
    let key = vec![("x-api-key".to_owned(), made_up_key("w"))];
    let headers: Vec<_> = fake
        .requests()
        .iter()
        .map(|asked| asked.headers())
        .collect();
    // To its own origin twice, and not to the mirror.
    assert_eq!(headers, [key.clone(), key, Vec::new()]);
}

/// A downloader that hands over a piece and then says the file moved: what none does.
struct BytesThenMoved(AtomicU32);

impl Downloader for BytesThenMoved {
    fn download(
        &self,
        _: &DownloadRequest,
        _: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        self.0.fetch_add(1, Ordering::SeqCst);
        sink(b"half a file");
        Ok(Downloaded::Moved(address("https://mirror.example.net/g")))
    }
}

#[test]
fn bytes_and_then_another_address_are_not_followed() {
    let inner = Arc::new(BytesThenMoved(AtomicU32::new(0)));
    let through = CheckedDownloader::new(inner.clone(), &catalogue()).unwrap();
    let first = address("https://weights.example.org/m/graph.onnx");
    let (ended, kept) = download(&through, &asking(&first));
    let fault = ended.unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Malformed);
    assert_eq!(
        fault.sentence(),
        "the download handed over bytes and then pointed elsewhere, and is not followed"
    );
    // The mirror is one of the origins, and still nothing was asked of it.
    assert_eq!(inner.0.load(Ordering::SeqCst), 1);
    assert_eq!(kept, b"half a file");
}

#[test]
fn a_cancelled_download_asks_nothing_and_a_fault_of_the_downloader_is_handed_on() {
    let first = address("https://weights.example.org/m/graph.onnx");
    let fake = Arc::new(FakeDownloader::new().serving(&first, file()));
    let through = checked(&fake);
    let cancelled = Cancellation::new();
    cancelled.cancel();
    let fault = through
        .download(&asking(&first), &cancelled, &mut |_| true)
        .unwrap_err();
    assert_eq!(
        refused_as(&fault),
        (
            FaultKind::Cancelled,
            Stage::NotSent,
            "the download was cancelled"
        )
    );
    assert!(fake.requests().is_empty());
    // A fault of the downloader behind it comes back as it is.
    let gone = Fault::new(FaultKind::Unavailable, "status 503").answered(None);
    let fake = Arc::new(FakeDownloader::new().failing(&first, gone.clone()));
    let (ended, _) = download(&checked(&fake), &asking(&first));
    assert_eq!(ended.unwrap_err(), gone);
}

#[test]
fn a_checked_download_holds_a_bounded_number_of_origins_and_prints_how_many() {
    let fake = Arc::new(FakeDownloader::new());
    let origins = |count: usize| -> Vec<Origin> {
        (0..count)
            .map(|at| origin(&format!("https://mirror-{at}.example.net")))
            .collect()
    };
    assert_eq!(DOWNLOAD_ORIGINS_LIMIT, 16);
    let full = CheckedDownloader::new(fake.clone(), &origins(DOWNLOAD_ORIGINS_LIMIT)).unwrap();
    assert_eq!(format!("{full:?}"), "CheckedDownloader(16 origins)");
    let over = CheckedDownloader::new(fake, &origins(DOWNLOAD_ORIGINS_LIMIT + 1));
    assert_eq!(over.err(), Some(NotAllowed::TooMany));
    // It is held as the interface is.
    let _: Arc<dyn Downloader> = Arc::new(full);
}

// --- a probe -----------------------------------------------------------------------------------

/// The `egress` table that approves [`recipient`] for probing and for no class.
fn probing() -> String {
    format!("{}probe = true\n", approving(&[]))
}

fn prober(fake: &Arc<FakeProber>, policy: DispatchPolicy, recipient: Recipient) -> CheckedProber {
    CheckedProber::new(fake.clone(), policy, recipient)
}

fn reached() -> Result<Probed, Fault> {
    Ok(Probed::Reached { status: 204 })
}

fn pointed(to: &str) -> Result<Probed, Fault> {
    Ok(Probed::Moved(address(to)))
}

fn probe(through: &CheckedProber, to: &str) -> Result<Probed, Fault> {
    through.probe(&ProbeRequest::new(address(to)), &Cancellation::new())
}

#[test]
fn a_probe_is_refused_by_default_and_sent_when_approved_and_carries_no_user_data() {
    let health = "https://api.example.com/health";
    // By default: an executor's policy until it is given one, and the policy of a
    // config that approves the recipient for every class of data and not for probing.
    let all = approving(&["prompts", "source_images", "derived_images", "evidence"]);
    for unapproved in [DispatchPolicy::none(), policy(&all)] {
        let fake = Arc::new(FakeProber::scripted(vec![reached()]));
        let fault = probe(&prober(&fake, unapproved, recipient()), health).unwrap_err();
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::NotSent, NOT_PROBED)
        );
        assert_eq!((fake.requests(), fake.remaining()), (0, 1));
    }
    // Approved for probing, and for no class of data: sent.
    let fake = Arc::new(FakeProber::scripted(vec![reached()]));
    let through = prober(&fake, policy(&probing()), recipient())
        .with_credential(credential(API, "a"))
        .unwrap();
    assert_eq!(
        probe(&through, health).unwrap(),
        Probed::Reached { status: 204 }
    );
    // What was handed over is the address and the credential of its origin, which is
    // all a probe has a place for.
    let probes = fake.probes();
    assert_eq!(probes.len(), 1);
    let whole = ProbeRequest::new(address(health))
        .with_credential(credential(API, "a"))
        .unwrap();
    assert_eq!(probes[0].request(), &whole);
    assert_eq!(
        probes[0].headers(),
        [("x-api-key".to_owned(), made_up_key("a"))]
    );
}

#[test]
fn a_probe_is_approved_for_the_recipient_as_it_was_approved_and_for_no_other() {
    let health = "https://api.example.com/health";
    let approved = policy(&probing());
    // The same origin under another account, under none, and with no upstream.
    let others = [
        Recipient::new(origin(API))
            .with_account(account("acct_other"))
            .forwarding_to(vec![origin(UPSTREAM)])
            .unwrap(),
        Recipient::new(origin(API))
            .forwarding_to(vec![origin(UPSTREAM)])
            .unwrap(),
        Recipient::new(origin(API)).with_account(account(ACCOUNT)),
    ];
    for other in others {
        let fake = Arc::new(FakeProber::scripted(vec![reached()]));
        let fault = probe(&prober(&fake, approved.clone(), other), health).unwrap_err();
        assert_eq!(fault.sentence(), NOT_PROBED);
        assert_eq!(fake.requests(), 0);
    }
    // And a first address that is not the recipient's.
    let fake = Arc::new(FakeProber::scripted(vec![reached()]));
    let through = prober(&fake, approved, recipient());
    let fault = probe(&through, "https://elsewhere.example.org/health").unwrap_err();
    assert_eq!(
        refused_as(&fault),
        (FaultKind::Rejected, Stage::NotSent, NOT_REACHED)
    );
    assert_eq!(fake.requests(), 0);
}

#[test]
fn a_probes_redirect_off_the_recipient_is_refused_and_nothing_is_asked_of_it() {
    let health = "https://api.example.com/health";
    for elsewhere in [
        "https://elsewhere.example.org/health?from=api",
        "http://api.example.com/health",
        "https://api.example.com:8443/health",
    ] {
        let fake = Arc::new(FakeProber::scripted(vec![pointed(elsewhere), reached()]));
        let through = prober(&fake, policy(&probing()), recipient())
            .with_credential(credential(API, "a"))
            .unwrap();
        let fault = probe(&through, health).unwrap_err();
        // The recipient was asked, so the fault does not say that nothing was sent.
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::Unresolved, NOT_REACHED)
        );
        // One probe was made, of the recipient; the answer after it was never played.
        let probes = fake.probes();
        assert_eq!((probes.len(), fake.remaining()), (1, 1));
        assert_eq!(probes[0].to(), &address(health));
        let shown = format!("{fault:?}");
        assert!(
            !shown.contains("example") && !shown.contains("from"),
            "{shown}"
        );
    }
    assert_eq!(ELSEWHERE, "https://elsewhere.example.org");
}

#[test]
fn a_probes_redirect_within_the_recipient_is_followed_with_the_credential_of_each_origin_or_none() {
    let fake = Arc::new(FakeProber::scripted(vec![
        pointed("https://api.example.com/v2/health"),
        pointed("https://uploads.example.net/health"),
        pointed("https://api.example.com/v3/health"),
        reached(),
    ]));
    let through = prober(&fake, policy(&probing()), recipient())
        .with_credential(credential(API, "a"))
        .unwrap();
    assert_eq!(
        probe(&through, "https://api.example.com/health").unwrap(),
        Probed::Reached { status: 204 }
    );
    let key = vec![("x-api-key".to_owned(), made_up_key("a"))];
    let headers: Vec<_> = fake.probes().iter().map(|probe| probe.headers()).collect();
    // The recipient's own goes to its own origin, and the upstream is handed none.
    assert_eq!(headers, [key.clone(), key.clone(), Vec::new(), key]);
    // An upstream is handed its own, when one was given, and that one goes nowhere
    // else.
    let fake = Arc::new(FakeProber::scripted(vec![
        pointed("https://uploads.example.net/health"),
        reached(),
    ]));
    let through = prober(&fake, policy(&probing()), recipient())
        .with_credential(credential(UPSTREAM, "u"))
        .unwrap();
    probe(&through, "https://api.example.com/health").unwrap();
    let headers: Vec<_> = fake.probes().iter().map(|probe| probe.headers()).collect();
    assert_eq!(
        headers,
        [Vec::new(), vec![("x-api-key".to_owned(), made_up_key("u"))]]
    );
}

#[test]
fn a_probe_pointed_elsewhere_more_often_than_the_limit_is_refused_and_each_probe_counts_its_own() {
    let hops = |moves: u32| -> Vec<Result<Probed, Fault>> {
        let moved = (0..moves).map(|hop| pointed(&format!("https://api.example.com/hop/{hop}")));
        moved.chain([reached()]).collect()
    };
    // Five moves, twice through one checked prober: each probe starts at nothing.
    let mut twice = hops(MOVES_LIMIT);
    twice.extend(hops(MOVES_LIMIT));
    let fake = Arc::new(FakeProber::scripted(twice));
    let through = prober(&fake, policy(&probing()), recipient());
    for round in 1..=2 {
        assert_eq!(
            probe(&through, "https://api.example.com/health").unwrap(),
            Probed::Reached { status: 204 }
        );
        assert_eq!(fake.requests(), round * 6);
    }
    let fake = Arc::new(FakeProber::scripted(hops(MOVES_LIMIT + 1)));
    let through = prober(&fake, policy(&probing()), recipient());
    let fault = probe(&through, "https://api.example.com/health").unwrap_err();
    assert_eq!(
        (fault.kind(), fault.sentence()),
        (FaultKind::Rejected, PROBE_MOVED)
    );
    // The sixth move was answered, and the address it named was not probed.
    assert_eq!((fake.requests(), fake.remaining()), (6, 1));
}

#[test]
fn a_probe_that_brings_a_credential_is_refused_and_a_cancelled_one_asks_nothing() {
    let health = address("https://api.example.com/health");
    let fake = Arc::new(FakeProber::scripted(vec![reached()]));
    let through = prober(&fake, policy(&probing()), recipient());
    let brought = ProbeRequest::new(health.clone())
        .with_credential(credential(API, "b"))
        .unwrap();
    let fault = through.probe(&brought, &Cancellation::new()).unwrap_err();
    assert_eq!(
        refused_as(&fault),
        (
            FaultKind::Rejected,
            Stage::NotSent,
            "the probe came with a credential, and whoever asks for a probe holds none"
        )
    );
    let cancelled = Cancellation::new();
    cancelled.cancel();
    let fault = through
        .probe(&ProbeRequest::new(health), &cancelled)
        .unwrap_err();
    assert_eq!(
        refused_as(&fault),
        (
            FaultKind::Cancelled,
            Stage::NotSent,
            "the probe was cancelled"
        )
    );
    assert_eq!(fake.requests(), 0);
    // A fault of the prober behind it comes back as it is.
    let gone = Fault::new(FaultKind::Unavailable, "no route").not_sent();
    let fake = Arc::new(FakeProber::scripted(vec![Err(gone.clone())]));
    let through = prober(&fake, policy(&probing()), recipient());
    assert_eq!(
        probe(&through, "https://api.example.com/health").unwrap_err(),
        gone
    );
}

#[test]
fn a_checked_probe_holds_one_credential_for_each_origin_it_reaches_and_prints_none() {
    let fake = Arc::new(FakeProber::scripted(Vec::new()));
    let through = prober(&fake, policy(&probing()), recipient());
    // One for an origin the recipient does not reach is refused, and a second for
    // one origin.
    let off = through.clone().with_credential(credential(ELSEWHERE, "e"));
    assert_eq!(off.err(), Some(NotAllowed::Elsewhere));
    let held = through
        .with_credential(credential(API, "a"))
        .unwrap()
        .with_credential(credential(UPSTREAM, "u"))
        .unwrap();
    let twice = held.clone().with_credential(credential(API, "c"));
    assert_eq!(twice.err(), Some(NotAllowed::Twice));
    let shown = format!("{held:?}");
    assert_eq!(
        shown,
        "CheckedProber(Recipient(https://api.example.com as acct...), 2 credentials)"
    );
    for tail in ["a", "u"] {
        assert!(!shown.contains(&made_up_key(tail)));
    }
    let _: Arc<dyn Prober> = Arc::new(held);
}
