//! `conformance::…`: the exchange of fal's filler under `tests/fixtures/fal-bria-eraser/`,
//! served by a fake on loopback. Until the recorded exchange of plan m3, task F1, item 8
//! comes, the one there is written from fal's published schema of the model's input and
//! output (read 2026-11-18): each picture of its request is `<picture WxH>` in place of
//! its bytes, and its key `<redacted>`.

use std::sync::Arc;

use serde_json::Value;
use whizbang_core::capability::Support;
use whizbang_core::identity::{DecodeLimits, identify};

use super::super::LONG_SIDE;
use super::super::wire::unbase64;
use super::{ask, body_of, carried, eraser, request};
use crate::net::testing::Pki;
use crate::service::testing::{answer, exchanges, named_size, serving};

const SET: &str = "fal-bria-eraser";

/// The one exchange of the set.
fn exchange() -> Value {
    let exchanges = exchanges(SET);
    assert_eq!(exchanges.len(), 1);
    exchanges.into_iter().next().unwrap().1
}

/// The request's body with the bytes of each picture's data address replaced by
/// `<picture WxH>`, its size, as the recording writes it.
fn as_recorded(body: &Value) -> Value {
    let mut shown = body.clone();
    for field in ["image_url", "mask_url"] {
        let pixels = carried(body, field);
        shown[field] = Value::String(format!(
            "data:image/png;base64,<picture {}x{}>",
            pixels.width(),
            pixels.height()
        ));
    }
    shown
}

#[test]
fn a_request_is_written_as_fals_reference_describes_it() {
    // One `POST` to the model's path, its body JSON of exactly the three fields, the
    // picture and the mask each a PNG of the request's size in a data address, the
    // mask white where the support paints and black elsewhere; and no key, which the
    // executor adds (`crates/whizbang/tests/it/fal.rs`).
    let exchange = exchange();
    let recorded = &exchange["request"];
    let (w, h) = named_size(recorded["body"]["image_url"].as_str().unwrap()).unwrap();
    let pki = Arc::new(Pki::loopback());
    let reply = serde_json::to_vec(&exchange["response"]["body"]).unwrap();
    let (listener, log) = serving(&pki, vec![answer("200 OK", "", &reply)]);
    let asked = request(w, h);
    ask(&pki, eraser(listener.port(), LONG_SIDE), &asked).unwrap();
    assert_eq!(listener.finish(), 1);
    let head = log.lock().unwrap()[0].head.clone();
    let path = recorded["path"].as_str().unwrap();
    assert!(
        head.starts_with(&format!("POST {path} HTTP/1.1\r\n")),
        "{head}"
    );
    assert!(
        head.contains("content-type: application/json\r\n"),
        "{head}"
    );
    assert!(
        !head.to_ascii_lowercase().contains("authorization"),
        "{head}"
    );
    let body = body_of(&log);
    assert_eq!(as_recorded(&body), recorded["body"]);
    assert_eq!(body.as_object().unwrap().len(), 3);
    assert_eq!(body["sync_mode"], Value::Bool(true));
    assert_eq!(carried(&body, "image_url"), *asked.image().pixels());
    let mask = carried(&body, "mask_url");
    for (pixel, sample) in mask.rgba().chunks(4).zip(asked.support().plane()) {
        let v = if *sample == Support::PAINT { 255 } else { 0 };
        assert_eq!(pixel, [v, v, v, 255]);
    }
}

#[test]
fn the_recorded_exchange_is_read_back_to_its_pixels() {
    // The answer's picture, decoded from its data address, is the fill: every pixel's
    // red, green and blue, at the request's size; no charge is said, so the call is
    // settled at its bound.
    let exchange = exchange();
    let (w, h) = named_size(exchange["request"]["body"]["image_url"].as_str().unwrap()).unwrap();
    let response = &exchange["response"];
    assert_eq!(response["status"], 200);
    let address = response["body"]["image"]["url"].as_str().unwrap();
    let data = address.split_once(";base64,").unwrap().1;
    let expected = identify(&unbase64(data).unwrap(), &DecodeLimits::default())
        .unwrap()
        .pixels;
    let pki = Arc::new(Pki::loopback());
    let reply = serde_json::to_vec(&response["body"]).unwrap();
    let (listener, _) = serving(&pki, vec![answer("200 OK", "", &reply)]);
    let painted = ask(&pki, eraser(listener.port(), LONG_SIDE), &request(w, h)).unwrap();
    assert_eq!(listener.finish(), 1);
    assert_eq!((painted.width(), painted.height()), (w, h));
    let rgb: Vec<u8> = expected
        .rgba()
        .chunks(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    assert_eq!(painted.rgb(), rgb.as_slice());
    assert_eq!(painted.charge(), None);
}
