//! The fake that sends (ADR 0012): it answers each request through the sending
//! interface with the next reply of its script, a redirect, an upload address or a
//! returned download address among them, and keeps what it was asked to send to each.

use std::sync::Arc;

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillHandle, FillRequest, Filler, Painted, Picture,
    Stage, Support,
};
use whizbang_core::contract::{Cancellation, ModelIdentity, Secrets};
use whizbang_core::egress::{
    Address, Classes, Credential, DataClass, Outgoing, Piece, Reply, Sender,
};
use whizbang_core::identity::CanonicalPixels;
use whizbang_testkit::sending::FakeSender;

const KEY: &str = "sk-marker-7f3a9c";

fn address(text: &str) -> Address {
    Address::of(text, &Secrets::none()).unwrap()
}

fn said(words: &str) -> Reply {
    Reply::answered(200, words.as_bytes().to_vec()).unwrap()
}

/// A filler as a remote adapter would be written: it sends the prompt to its endpoint,
/// follows where it is pointed, uploads the picture to the address it is given, and
/// fetches the result from the address that comes back. Its credential goes only to
/// the origin it is bound to.
struct Remote {
    declaration: Declaration,
    sender: Arc<FakeSender>,
    credential: Credential,
}

impl Remote {
    fn send(&self, to: Address, pieces: Vec<Piece>, bounds: &Bounds) -> Result<Reply, Fault> {
        let mut outgoing = Outgoing::new(to.clone(), pieces).unwrap();
        if to.origin() == self.credential.origin() {
            outgoing = outgoing.with_credential(self.credential.clone()).unwrap();
        }
        self.sender.send(&outgoing, bounds)
    }

    /// The address a reply's body names after this word.
    fn named(reply: &Reply, word: &str) -> Address {
        let body = String::from_utf8(reply.body().to_vec()).unwrap();
        address(body.strip_prefix(word).unwrap().trim())
    }
}

impl Filler for Remote {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let prompt = request.prompt().unwrap_or_default().as_bytes().to_vec();
        let prompt = || vec![Piece::of(DataClass::Prompts, prompt.clone())];
        let first = address("https://api.example.com/v1/fill");
        let mut reply = self.send(first, prompt(), bounds)?;
        // Two redirects at most: a literal bound on the loop.
        for _ in 0..2 {
            let Some(next) = reply.moved_to().cloned() else {
                break;
            };
            reply = self.send(next, prompt(), bounds)?;
        }
        let upload = Remote::named(&reply, "upload");
        let pixels = Piece::of(DataClass::SourceImages, request.image().rgba().to_vec());
        let reply = self.send(upload, vec![pixels], bounds)?;
        let download = Remote::named(&reply, "download");
        let reply = self.send(download, Vec::new(), bounds)?;
        let (width, height) = (request.image().width(), request.image().height());
        Ok(Painted::from_rgb(width, height, reply.body().to_vec())?)
    }
}

fn remote(sender: &Arc<FakeSender>) -> Arc<Remote> {
    let origin = address("https://api.example.com").origin().clone();
    Arc::new(Remote {
        declaration: Declaration::new(ModelIdentity::new("fake", "remote", "1"), &["prompt"])
            .unwrap(),
        sender: sender.clone(),
        credential: Credential::new(origin, "x-api-key", KEY, &Secrets::none()).unwrap(),
    })
}

fn request() -> FillRequest {
    let rgba = (0..2 * 2 * 4).map(|at| at as u8).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(2, 2, rgba).unwrap()).unwrap();
    let support = Support::from_plane(2, 2, vec![255; 4]).unwrap();
    FillRequest::new(picture, support, Some("a red door")).unwrap()
}

#[test]
fn the_sending_fake_records_each_hop_in_order_with_the_headers_it_would_have_carried() {
    let sender = Arc::new(FakeSender::scripted(vec![
        // A redirect within the endpoint's origin, then one off it.
        Ok(Reply::moved(address("https://api.example.com/v2/fill"))),
        Ok(Reply::moved(address(
            "https://eu.example.net/v2/fill?region=eu",
        ))),
        // An upload address, and a returned download address.
        Ok(said("upload https://uploads.example.net/put/1?sig=abc")),
        Ok(said("download https://cdn.example.org/out/1")),
        Ok(Reply::answered(200, vec![7; 2 * 2 * 3]).unwrap()),
    ]));
    let handle = FillHandle::new(remote(&sender), Cancellation::new());
    let painted = handle.fill(&request()).unwrap();
    assert_eq!(painted.rgb(), [7; 12]);

    let hops = sender.hops();
    let reached: Vec<String> = hops
        .iter()
        .map(|hop| format!("{}{}", hop.to().origin(), hop.to().rest()))
        .collect();
    assert_eq!(
        reached,
        [
            "https://api.example.com/v1/fill",
            "https://api.example.com/v2/fill",
            "https://eu.example.net/v2/fill?region=eu",
            "https://uploads.example.net/put/1?sig=abc",
            "https://cdn.example.org/out/1",
        ]
    );
    // The headers each would have carried: the credential to its own origin, twice,
    // and to no other.
    let key = vec![("x-api-key".to_owned(), KEY.to_owned())];
    let headers: Vec<Vec<(String, String)>> = hops.iter().map(|hop| hop.headers()).collect();
    assert_eq!(
        headers,
        [key.clone(), key, Vec::new(), Vec::new(), Vec::new()]
    );
    // And what it was asked to send to each.
    let prompts = Classes::of(&[DataClass::Prompts]);
    let pictures = Classes::of(&[DataClass::SourceImages]);
    let classes: Vec<Classes> = hops.iter().map(|hop| hop.outgoing().needs()).collect();
    assert_eq!(
        classes,
        [prompts, prompts, prompts, pictures, Classes::none()]
    );
    assert_eq!(hops[0].outgoing().pieces()[0].bytes(), b"a red door");
    assert_eq!(hops[2].outgoing().pieces()[0].bytes(), b"a red door");
    assert_eq!(
        hops[3].outgoing().pieces()[0].bytes(),
        request().image().rgba()
    );
    assert!(hops[4].outgoing().pieces().is_empty());
    assert_eq!((sender.requests(), sender.remaining()), (5, 0));
}

#[test]
fn the_sending_fake_plays_a_fault_where_its_script_has_one_and_keeps_the_hop() {
    let refused = Fault::new(FaultKind::Rejected, "status 403").answered(None);
    let sender = Arc::new(FakeSender::scripted(vec![
        Ok(Reply::moved(address("https://eu.example.net/v2/fill"))),
        Err(refused.clone()),
        Ok(said("never reached")),
    ]));
    let handle = FillHandle::new(remote(&sender), Cancellation::new());
    assert_eq!(handle.fill(&request()).unwrap_err(), refused);
    // The second hop was handed the prompt before it refused, and is kept.
    let hops = sender.hops();
    assert_eq!(hops.len(), 2);
    assert_eq!(hops[1].to(), &address("https://eu.example.net/v2/fill"));
    assert_eq!(hops[1].outgoing().pieces()[0].bytes(), b"a red door");
    assert!(hops[1].headers().is_empty());
    assert_eq!((sender.requests(), sender.remaining()), (2, 1));
}

#[test]
fn the_sending_fake_past_the_end_of_its_script_fails_and_does_not_panic() {
    let sender = Arc::new(FakeSender::scripted(Vec::new()));
    let handle = FillHandle::new(remote(&sender), Cancellation::new());
    let fault = handle.fill(&request()).unwrap_err();
    assert_eq!(
        (fault.kind(), fault.stage()),
        (FaultKind::Malformed, Stage::Unresolved)
    );
    assert_eq!(fault.sentence(), "the scripted hops have no reply left");
    // What it was handed is kept all the same.
    assert_eq!(sender.hops().len(), 1);
}

#[test]
fn the_sending_fake_sends_nothing_for_a_run_that_was_cancelled() {
    // The handle refuses before the backend is called, so the fake is handed nothing.
    let sender = Arc::new(FakeSender::scripted(vec![Ok(said("upload x"))]));
    let run = Cancellation::new();
    run.cancel();
    let fault = FillHandle::new(remote(&sender), run)
        .fill(&request())
        .unwrap_err();
    assert_eq!(fault.stage(), Stage::NotSent);
    assert!(sender.hops().is_empty());
    assert_eq!(sender.remaining(), 1);
}

#[test]
fn what_is_printed_of_the_sending_fake_and_of_a_hop_holds_no_credential() {
    let upload = said("upload https://uploads.example.net/put/1");
    let sender = Arc::new(FakeSender::scripted(vec![Ok(upload)]));
    let handle = FillHandle::new(remote(&sender), Cancellation::new());
    assert!(handle.fill(&request()).is_err());
    let shown = format!("{sender:?} {:?}", sender.hops());
    assert!(!shown.contains(KEY), "{shown}");
    assert!(
        shown.contains("x-api-key for https://api.example.com"),
        "{shown}"
    );
}
