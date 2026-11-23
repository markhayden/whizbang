//! Pictures in and out (ADR 0045, "Plain routes"): `POST /v1/uploads`, a picture or a
//! mask imported, its body read on an upload handler at no less than the rate after the
//! headers' time and its size checked from its header before anything is decoded; and
//! `GET /v1/artifacts/<identity>`, an artifact's bytes.

use std::sync::Arc;

use super::Shared;
use super::guard::{Asked, Guarded, deadlines};
use crate::escaped;
use crate::http::{self, Response, Unread};
use crate::play::guard::dimensions;

pub(crate) fn upload(shared: &Arc<Shared>, guarded: Guarded) {
    let Guarded {
        mut conn,
        since,
        head,
        rest,
        asked,
        ..
    } = guarded;
    let Asked::Upload { mask_of, _place } = asked else {
        return;
    };
    let times = shared.settings.times;
    let until = since
        + times.head
        + std::time::Duration::from_secs_f64(head.length as f64 / times.rate.max(1) as f64);
    let body = match http::read_body_until(&mut conn, head.length, rest, until) {
        Ok(body) => body,
        Err(Unread::Answer(response)) => return response.send(&mut conn, &deadlines(shared)),
        Err(Unread::Close) => return,
    };
    let declared = head.header("content-type").map(|value| {
        let kind = value.split(';').next().unwrap_or_default();
        kind.trim().to_ascii_lowercase()
    });
    let response = taken(shared, &body, declared.as_deref(), mask_of.as_deref());
    drop(_place);
    response.send(&mut conn, &deadlines(shared));
}

/// What an upload's body comes to.
fn taken(shared: &Shared, body: &[u8], declared: Option<&str>, mask_of: Option<&str>) -> Response {
    // Its bytes are of the type it was sent as: a PNG's signature, or a JPEG's start.
    let png = body.starts_with(b"\x89PNG\r\n\x1a\n");
    let jpeg = body.starts_with(&[0xff, 0xd8, 0xff]);
    let of_its_type = match declared {
        Some("image/png") => png,
        Some("image/jpeg") => jpeg && mask_of.is_none(),
        _ => false,
    };
    if !of_its_type {
        return Response::refused(415, "the body is not of the type it was sent as");
    }
    // Its size, from its header alone, before anything is decoded.
    let Some((width, height)) = dimensions(body) else {
        return Response::refused(415, "the body is not a PNG or JPEG");
    };
    let pixels = u64::from(width) * u64::from(height);
    let most = shared.settings.pixel_limit;
    if pixels > most {
        return Response::refused(
            413,
            &format!("the picture is {width}x{height}, {pixels} pixels, over the limit of {most}"),
        );
    }
    let made = match mask_of {
        None => shared.hosted.upload(body, most),
        Some(image) => match shared.hosted.image_size(image) {
            None => return Response::refused(404, "the store holds no image of that identity"),
            Some(size) if size != (width, height) => {
                return Response::refused(
                    400,
                    &format!(
                        "the mask is {width}x{height} and its image {}x{}: a mask is its image's size exactly",
                        size.0, size.1
                    ),
                );
            }
            Some(_) => shared.hosted.mask(body, image, most),
        },
    };
    match made {
        Ok(made) => Response::json(201, &made),
        Err(refused) => Response::refused(refused.status, &escaped(&refused.why)),
    }
}

pub(crate) fn download(shared: &Arc<Shared>, guarded: Guarded) {
    let Guarded {
        mut conn, asked, ..
    } = guarded;
    let Asked::Artifact(id) = asked else {
        return;
    };
    let identity = id.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    });
    let response = match identity {
        false => Response::refused(404, "no artifact has that identity"),
        true => match shared.hosted.artifact(&id) {
            Ok((content_type, bytes)) => Response::bytes(200, content_type, bytes),
            Err(refused) => Response::refused(refused.status, &escaped(&refused.why)),
        },
    };
    response.send(&mut conn, &deadlines(shared));
}
