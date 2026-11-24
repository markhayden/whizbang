//! `remote::…` of the core: the checked sender of frames and its control send (ADR 0046,
//! "The kind `remote`"; plan m6, task R1, item 2), through an executor's run, with a
//! sender of the test's behind it.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Stage,
};
use whizbang_core::contract::record::Handed;
use whizbang_core::contract::{ModelIdentity, Secrets};
use whizbang_core::egress::{
    CheckedFrames, Classes, Control, DataClass, Dispatched, FrameBody, Frames, Head, Outgoing,
    Piece, Reply, SendCheck, Sender, Sent, Take, Ways,
};

use super::support::{
    ACCOUNT_CUT, API, ELSEWHERE, address, approving, entry, policy, recipient, run_one,
};

/// A sender of frames of the test's: each request of frames reached the address it
/// names, at a peer of loopback, and is answered with nothing; each control send is
/// counted, with the credential it carried.
#[derive(Default)]
struct Behind {
    controls: Mutex<Vec<(String, Option<String>)>>,
}

impl Sender for Behind {
    fn send(&self, _: &Outgoing, _: &Bounds) -> Result<Reply, Fault> {
        Err(Fault::new(FaultKind::Rejected, "no pieces here").not_sent())
    }

    fn stream(&self, frames: &CheckedFrames, _: &Bounds, take: &mut Take<'_>) -> Sent {
        let peer = SocketAddr::from(([127, 0, 0, 1], 4443));
        let ended = take(&Head::new(200, Vec::new()), &mut std::io::empty());
        Sent {
            dispatched: Some(Dispatched::reached(frames.to().origin().clone(), peer)),
            ended,
        }
    }

    fn control(&self, _: &Dispatched, control: &Control, _: &Bounds) -> Result<u16, Fault> {
        let credential = control.credential().map(|c| c.header().to_owned());
        self.controls
            .lock()
            .unwrap()
            .push((control.to().origin().to_string(), credential));
        Ok(204)
    }
}

/// A body of one prompt.
struct Words;

impl FrameBody for Words {
    fn parts(&self) -> Vec<Piece> {
        vec![Piece::of(DataClass::Prompts, b"a red thing".to_vec())]
    }

    fn write(&self) -> Result<Vec<u8>, Fault> {
        Ok(b"a red thing".to_vec())
    }
}

/// What the filler of the test came to: each control send's end.
type Ends = Arc<Mutex<Vec<Result<u16, Fault>>>>;

/// A filler that sends one request of frames, then three control sends: one that
/// follows it, one that follows a request no checked sender dispatched, and one to
/// another origin.
struct Follows {
    declaration: Declaration,
    ends: Ends,
}

impl Filler for Follows {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let frames = Frames::new(
            address(&format!("{API}/pack/1/request")),
            Arc::new(Words),
            std::time::Duration::from_secs(1),
        );
        let sent = bounds
            .sender()
            .send_frames(&frames, bounds, &mut |_, _| Ok(()));
        let dispatched = sent.dispatched.expect("the request was dispatched");
        let cancel = |to: &str| Control::new(address(to), b"cancel".to_vec()).unwrap();
        let cleanup = bounds.for_cleanup(std::time::Duration::from_secs(5));
        let made_up = Dispatched::reached(
            dispatched.origin().clone(),
            SocketAddr::from(([127, 0, 0, 1], 4443)),
        );
        let mut ends = self.ends.lock().unwrap();
        let api = |path: &str| format!("{API}{path}");
        ends.push(
            bounds
                .sender()
                .control(&dispatched, &cancel(&api("/pack/1/cancel")), &cleanup),
        );
        ends.push(
            bounds
                .sender()
                .control(&made_up, &cancel(&api("/pack/1/cancel")), &cleanup),
        );
        ends.push(bounds.sender().control(
            &dispatched,
            &cancel(&format!("{ELSEWHERE}/pack/1/cancel")),
            &cleanup,
        ));
        // A control whose body holds a path is shown to the check, and refused.
        let spelt =
            Control::new(address(&api("/pack/1/cancel")), b"x /tmp/a-file x".to_vec()).unwrap();
        ends.push(bounds.sender().control(&dispatched, &spelt, &cleanup));
        // Under the bounds of cleaning up, nothing of the run's data is sent: neither a
        // request of frames nor a send.
        let again = bounds
            .sender()
            .send_frames(&frames, &cleanup, &mut |_, _| Ok(()));
        ends.push(again.ended.map(|()| 0));
        let outgoing = Outgoing::new(
            address(&api("/v1/words")),
            vec![Piece::of(DataClass::Prompts, b"words".to_vec())],
        )
        .unwrap();
        ends.push(bounds.sender().send(&outgoing, &cleanup).map(|_| 0));
        // A cleanup's time past what the clock holds is no time, and never no deadline.
        assert!(
            bounds
                .for_cleanup(std::time::Duration::MAX)
                .deadline()
                .is_some()
        );
        assert!(cleanup.is_cleanup() && !bounds.is_cleanup());
        let (width, height) = (request.image().width(), request.image().height());
        Ok(Painted::from_rgb(
            width,
            height,
            vec![0; width as usize * height as usize * 3],
        )?)
    }
}

#[test]
fn the_control_send_is_refused_for_a_request_not_dispatched_or_another_origin() {
    let behind = Arc::new(Behind::default());
    let ends: Ends = Arc::default();
    let filler = Arc::new(Follows {
        declaration: Declaration::new(ModelIdentity::new("remote", "sam3", "1"), &[])
            .unwrap()
            .sending_to(recipient()),
        ends: ends.clone(),
    });
    let ways = Ways::through(behind.clone())
        .checked_by(Arc::new(SendCheck::new()))
        .under(policy(&approving(&["prompts"])));
    let ways = ways
        .with_credential(
            &recipient(),
            whizbang_core::egress::Credential::new(
                super::support::origin(API),
                "Authorization",
                "Bearer made-up-token-of-the-test",
                &Secrets::none(),
            )
            .unwrap(),
        )
        .unwrap();
    let ran = run_one(filler, ways, Secrets::none());
    assert_eq!(ran.fault, None);
    let ends = ends.lock().unwrap();
    // The one that follows the request is sent, with the request's credential.
    assert_eq!(ends[0].as_ref().ok(), Some(&204));
    assert_eq!(
        *behind.controls.lock().unwrap(),
        [(API.to_owned(), Some("Authorization".to_owned()))]
    );
    // The others are refused before anything leaves: a request no checked sender
    // dispatched, another origin, a body that holds a path, and the run's data under the
    // bounds of cleaning up.
    assert_eq!(ends.len(), 6);
    for refused in &ends[1..] {
        let fault = refused.as_ref().unwrap_err();
        assert_eq!(fault.kind(), FaultKind::Rejected, "{fault:?}");
        assert_eq!(fault.stage(), Stage::NotSent);
    }
    // The request and its cancel are where the run's data went, the cancel with no
    // class; the refusals are the run's.
    let went = ran.went();
    assert!(
        went.contains(&entry(
            API,
            Some(ACCOUNT_CUT),
            Classes::none().with(DataClass::Prompts),
            Handed::Sent
        )),
        "{went:?}"
    );
    assert_eq!(ran.refused().len(), 5, "{:?}", ran.refused());
}
