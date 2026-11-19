//! The tests of fal's filler (plan m3, task F1), each against a server on loopback with a
//! certificate of a root made in the test (`crate::service::testing`). Nothing here
//! contacts a host off the machine. The gate and the executor's credential are the
//! binary's wiring's, and are tested there (`crates/whizbang/tests/it/fal.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod conformance;

use std::sync::Arc;

use serde_json::Value;
use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillHandle, FillRequest, Filler, Painted, Picture,
    Stage, Support,
};
use whizbang_core::contract::{Cancellation, ModelIdentity, Secrets};
use whizbang_core::egress::{Address, Sender};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits, identify};

use super::wire::{AN_ADDRESS, unbase64};
use super::{ADAPTER, Eraser, LONG_SIDE, MODEL, identity_version};
use crate::net::testing::{Pki, sender_trusting};
use crate::service::testing::{Log, answer, serving};
use crate::service::{BUSY, CREDENTIAL, RATE_LIMITED, REFUSED, STATUS};

/// The filler at a port of loopback, sending pictures at this longest side.
pub(super) fn eraser(port: u16, long_side: u32) -> Eraser {
    let identity = ModelIdentity::new(ADAPTER, MODEL, &identity_version());
    Eraser {
        declaration: Declaration::new(identity, &[]).unwrap(),
        address: Address::of(
            &format!("https://127.0.0.1:{port}/fal-ai/bria/eraser"),
            &Secrets::none(),
        )
        .unwrap(),
        long_side,
    }
}

/// A filler that asks the backend through the test's sender.
struct Through {
    eraser: Eraser,
    sender: Arc<dyn Sender>,
}

impl Filler for Through {
    fn declaration(&self) -> &Declaration {
        &self.eraser.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        self.eraser.fill_through(request, &*self.sender, bounds)
    }
}

/// Asks the backend once, through a handle.
pub(super) fn ask(pki: &Pki, eraser: Eraser, request: &FillRequest) -> Result<Painted, Fault> {
    let filler = Arc::new(Through {
        eraser,
        sender: sender_trusting(pki),
    });
    FillHandle::new(filler, Cancellation::new()).fill(request)
}

/// A picture of this size with something in it, every pixel opaque.
pub(super) fn picture(width: u32, height: u32) -> Picture {
    let rgba = (0..width * height)
        .flat_map(|at| [(at * 5) as u8, (at / 7) as u8, (at % 199) as u8, 255])
        .collect();
    Picture::new(CanonicalPixels::from_rgba(width, height, rgba).unwrap()).unwrap()
}

/// A request to paint the left half of the top rows of a picture of this size.
pub(super) fn request(width: u32, height: u32) -> FillRequest {
    let plane = (0..height)
        .flat_map(|y| (0..width).map(move |x| if x < width / 2 && y < 8 { 255 } else { 0 }))
        .collect();
    let support = Support::from_plane(width, height, plane).unwrap();
    FillRequest::new(picture(width, height), support, None).unwrap()
}

/// A PNG of these pixels, four bytes a pixel.
pub(super) fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use image::ImageEncoder;
    let mut file = Vec::new();
    image::codecs::png::PngEncoder::new(&mut file)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    file
}

/// The body of an answer of the model's form, its picture this PNG.
pub(super) fn answer_body(png: &[u8]) -> Value {
    let mut data = Vec::new();
    crate::net::http::base64_onto(png, &mut data);
    serde_json::json!({
        "image": {
            "url": format!("data:image/png;base64,{}", String::from_utf8(data).unwrap()),
            "content_type": "image/png", "file_name": "eraser.png"
        }
    })
}

/// An answer of 200 of the model's form, its picture this PNG.
pub(super) fn answered(png: &[u8]) -> Vec<u8> {
    answer("200 OK", "", answer_body(png).to_string().as_bytes())
}

/// The bytes of the picture a request's field carried in its data address.
pub(super) fn carried(body: &Value, field: &str) -> CanonicalPixels {
    let address = body[field].as_str().unwrap();
    let data = address.strip_prefix("data:image/png;base64,").unwrap();
    identify(&unbase64(data).unwrap(), &DecodeLimits::default())
        .unwrap()
        .pixels
}

/// What a server saw of its one request, its body read as JSON.
pub(super) fn body_of(log: &Log) -> Value {
    serde_json::from_slice(&log.lock().unwrap()[0].body).unwrap()
}

#[test]
fn a_reply_holding_a_url_is_a_fault_and_nothing_is_fetched() {
    // The model's answer as fal gives it without `sync_mode`, a link to its media host,
    // and one with a data address beside a link elsewhere in it: each is a malformed
    // answer, charged its bound, and the server is asked once each, nothing more.
    let pki = Arc::new(Pki::loopback());
    let link = serde_json::json!({
        "image": { "url": "https://v3.media.example.test/files/eraser.png" }
    });
    let mut beside = answer_body(&png(64, 64, &[9; 64 * 64 * 4]));
    beside["seed"] = serde_json::json!("see https://media.example.test/x");
    let bodies = [link, beside];
    let answers = bodies
        .iter()
        .map(|body| answer("200 OK", "", body.to_string().as_bytes()))
        .collect();
    let (listener, _) = serving(&pki, answers);
    for _ in &bodies {
        let fault = ask(&pki, eraser(listener.port(), LONG_SIDE), &request(64, 64)).unwrap_err();
        assert_eq!(
            (
                fault.kind(),
                fault.stage(),
                fault.sentence(),
                fault.charge()
            ),
            (FaultKind::Malformed, Stage::Answered, AN_ADDRESS, None)
        );
    }
    assert_eq!(listener.finish(), bodies.len());
}

#[test]
fn a_fault_holds_nothing_of_the_body_the_headers_or_the_address() {
    let pki = Arc::new(Pki::loopback());
    let marker = "MARKER-7Q2";
    let host = "api.example-host.test";
    // A made-up token, put together here so that no scan of this file finds one.
    let token = &["fal", "made", "up", "token", "0123456789abcdef"].join("-");
    let body = format!(
        "{{\"detail\":\"{marker} at https://{host}/v1 with {token}\",\"error_type\":\"{marker}\"}}"
    );
    let fields = format!("X-Fal-Error-Type: {marker}\r\nX-Token: {token}\r\n");
    let cases: Vec<(&str, FaultKind, &str)> = vec![
        ("400 Bad Request", FaultKind::Rejected, REFUSED),
        ("401 Unauthorized", FaultKind::Credential, CREDENTIAL),
        ("403 Forbidden", FaultKind::Credential, CREDENTIAL),
        ("413 Too Large", FaultKind::Rejected, REFUSED),
        ("422 Unprocessable", FaultKind::Rejected, REFUSED),
        (
            "429 Too Many Requests",
            FaultKind::Unavailable,
            RATE_LIMITED,
        ),
        ("503 Unavailable", FaultKind::Unavailable, BUSY),
        ("504 Timeout", FaultKind::Unavailable, BUSY),
        ("418 Teapot", FaultKind::Malformed, STATUS),
        ("200 OK", FaultKind::Malformed, AN_ADDRESS),
    ];
    let answers = cases
        .iter()
        .map(|(status, _, _)| answer(status, &fields, body.as_bytes()))
        .collect();
    let (listener, _) = serving(&pki, answers);
    let port = listener.port();
    for (status, kind, why) in &cases {
        let fault = ask(&pki, eraser(port, LONG_SIDE), &request(16, 16)).unwrap_err();
        assert_eq!(
            (fault.kind(), fault.stage(), fault.sentence()),
            (*kind, Stage::Answered, *why),
            "{status}"
        );
        let printed = format!("{fault} {fault:?}");
        for held in [marker, host, token.as_str(), "127.0.0.1", &port.to_string()] {
            assert!(!printed.contains(held), "{status}: {printed}");
        }
    }
    assert_eq!(listener.finish(), cases.len());
}

#[test]
fn a_picture_larger_than_the_models_side_is_scaled_inside_and_returned_at_its_size() {
    // A picture of 64 by 48 to a filler whose side is 32: it is sent at 32 by 24, the
    // mask with it, still black and white, and the answer of 32 by 24 comes back at 64
    // by 48. An answer of another size than was sent is malformed.
    let pki = Arc::new(Pki::loopback());
    let small = png(32, 24, &[200; 32 * 24 * 4]);
    let (listener, log) = serving(
        &pki,
        vec![answered(&small), answered(&png(64, 48, &[1; 64 * 48 * 4]))],
    );
    let painted = ask(&pki, eraser(listener.port(), 32), &request(64, 48)).unwrap();
    assert_eq!((painted.width(), painted.height()), (64, 48));
    assert!(painted.rgb().iter().all(|v| *v == 200));
    let body = body_of(&log);
    let (sent, mask) = (carried(&body, "image_url"), carried(&body, "mask_url"));
    assert_eq!((sent.width(), sent.height()), (32, 24));
    assert_eq!((mask.width(), mask.height()), (32, 24));
    assert!(mask.rgba().iter().all(|v| *v == 0 || *v == 255));
    // The top left quarter of the rows painted, and none of the bottom.
    assert_eq!(&mask.rgba()[..4], &[255, 255, 255, 255]);
    assert_eq!(&mask.rgba()[mask.rgba().len() - 4..], &[0, 0, 0, 255]);
    let fault = ask(&pki, eraser(listener.port(), 32), &request(64, 48)).unwrap_err();
    assert_eq!(fault.sentence(), super::wire::WRONG_SIZE);
    assert_eq!(listener.finish(), 2);
}

#[test]
fn base64_is_read_strictly() {
    assert_eq!(unbase64("aGk=").unwrap(), b"hi");
    assert_eq!(unbase64("aGVsbG8h").unwrap(), b"hello!");
    assert_eq!(unbase64("").unwrap(), b"");
    for bad in ["aGk", "aG=k", "a=Gk", "aGk=aGk=", "aGk*", "aG==a"] {
        assert_eq!(unbase64(bad), None, "{bad}");
    }
}
