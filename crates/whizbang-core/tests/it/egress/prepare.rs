//! `prepare::…`: a picture prepared for a model (plan 2A, task S1, item 4), and what
//! the check behind a send knows of it; with the review's follow-ups of the check
//! (a request's own fields, a folder in another case, an address that holds a
//! secret).

use std::sync::{Arc, Mutex};

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Picture, Stage,
};
use whizbang_core::contract::{ModelIdentity, Secrets};
use whizbang_core::egress::{
    DataClass, JPEG_QUALITY, MOVED_SECRET, Outgoing, Piece, SendCheck, Ways, fitted, is_plain_jpeg,
};
use whizbang_core::identity::CanonicalPixels;
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::sending::FakeSender;

use super::support::{API, address, approving, made_up_key, moved, ok, policy, recipient, run_one};

const EVERY_CLASS: [&str; 4] = ["source_images", "derived_images", "prompts", "evidence"];

const NOT_PIXELS: &str = "a part of the request is an image file that whizbang's encoder \
                          did not write, and an image leaves only as its pixels";

const KEPT_OUT: &str = "the request holds the path of a folder that is kept out of what \
                        leaves the machine";

fn ways(sender: &Arc<FakeSender>, check: SendCheck) -> Ways {
    Ways::through(sender.clone())
        .checked_by(Arc::new(check))
        .under(policy(&approving(&EVERY_CLASS)))
}

fn posting(pieces: Vec<Piece>) -> Arc<FakeFiller> {
    let filler = FakeFiller::new(Paint::Inverse).sending_to(recipient());
    Arc::new(filler.posting(address(&format!("{API}/v1/fill")), pieces))
}

fn picture_of(width: u32, height: u32, rgba: Vec<u8>) -> Picture {
    Picture::new(CanonicalPixels::from_rgba(width, height, rgba).unwrap()).unwrap()
}

/// A picture with something in it, every pixel opaque, or every fourth one half
/// clear.
pub(super) fn patterned(width: u32, height: u32, clear: bool) -> Picture {
    let pixels = width as usize * height as usize;
    let rgba = (0..pixels)
        .flat_map(|at| {
            let alpha = if clear && at % 4 == 0 { 128 } else { 255 };
            [(at * 7) as u8, (at / 3) as u8, (at % 251) as u8, alpha]
        })
        .collect();
    picture_of(width, height, rgba)
}

/// Whether a piece of these bytes is sent through the check, and the fault when not.
fn sent(bytes: Vec<u8>) -> (Option<Fault>, u64) {
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let pieces = vec![Piece::of(DataClass::SourceImages, bytes)];
    let ran = run_one(
        posting(pieces),
        ways(&sender, SendCheck::new()),
        Secrets::none(),
    );
    (ran.fault.clone(), sender.requests())
}

/// The JPEG with this segment put in after its start.
pub(super) fn with_segment(jpeg: &[u8], marker: u8, body: &[u8]) -> Vec<u8> {
    let mut segment = vec![0xff, marker];
    segment.extend_from_slice(&u16::try_from(body.len() + 2).unwrap().to_be_bytes());
    segment.extend_from_slice(body);
    let mut file = jpeg.to_vec();
    file.splice(2..2, segment);
    file
}

#[test]
fn a_picture_is_fitted_and_sent_as_jpeg_and_the_check_knows_it() {
    // Fitted to its long edge, never made larger, and written as a JPEG at 90.
    let picture = patterned(300, 200, false);
    let piece = Piece::prepared(DataClass::SourceImages, &picture, 100).unwrap();
    assert!(piece.is_picture());
    assert_eq!(JPEG_QUALITY, 90);
    let jpeg = piece.bytes().to_vec();
    assert!(jpeg.starts_with(b"\xff\xd8") && jpeg.ends_with(b"\xff\xd9"));
    assert!(is_plain_jpeg(&jpeg));
    let read = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap();
    assert_eq!((read.width(), read.height()), fitted(300, 200, 100));
    assert_eq!(fitted(300, 200, 100), (100, 67));
    let small = Piece::prepared(DataClass::SourceImages, &patterned(30, 20, false), 100);
    let small = image::load_from_memory(small.unwrap().bytes()).unwrap();
    assert_eq!((small.width(), small.height()), (30, 20));
    // A picture that is not all opaque goes as the PNG the encoder writes of the
    // fitted pixels, which the check knows already.
    let clear = Piece::prepared(DataClass::SourceImages, &patterned(300, 200, true), 100);
    let clear = clear.unwrap();
    assert!(clear.bytes().starts_with(b"\x89PNG"));
    let read = image::load_from_memory(clear.bytes()).unwrap();
    assert_eq!((read.width(), read.height()), (100, 67));
    assert_eq!(sent(clear.bytes().to_vec()), (None, 1));

    // The check knows the JPEG: it is sent, as a piece of any class.
    assert_eq!(sent(jpeg.clone()), (None, 1));
    // And refuses the same JPEG with anything more in it: an EXIF block, a colour
    // profile, a comment, a thumbnail in its JFIF block, a second frame, a byte after
    // its end, and its end cut off.
    let mut thumbnail = jpeg.clone();
    assert_eq!(&thumbnail[2..4], b"\xff\xe0");
    let jfif = 4 + 2;
    thumbnail[jfif + 12] = 1;
    let mut after = jpeg.clone();
    after.push(0);
    let frame = jpeg
        .windows(2)
        .position(|pair| pair == b"\xff\xc0")
        .unwrap();
    let length = usize::from(u16::from_be_bytes([jpeg[frame + 2], jpeg[frame + 3]]));
    let mut two_frames = jpeg.clone();
    two_frames.splice(frame..frame, jpeg[frame..frame + 2 + length].to_vec());
    let cases = [
        (
            "an EXIF block",
            with_segment(&jpeg, 0xe1, b"Exif\0\0MARKER-EXIF"),
        ),
        (
            "a colour profile",
            with_segment(&jpeg, 0xe2, b"ICC_PROFILE\0\x01\x01"),
        ),
        ("a comment", with_segment(&jpeg, 0xfe, b"MARKER-COMMENT")),
        ("a thumbnail", thumbnail),
        ("a second frame", two_frames),
        ("a byte after its end", after),
        ("its end cut off", jpeg[..jpeg.len() - 2].to_vec()),
    ];
    for (what, file) in cases {
        assert!(!is_plain_jpeg(&file), "{what}");
        let (fault, requests) = sent(file);
        let fault = fault.unwrap_or_else(|| panic!("{what} was sent"));
        assert_eq!(
            (fault.kind(), fault.stage(), fault.sentence(), requests),
            (FaultKind::Rejected, Stage::NotSent, NOT_PIXELS, 0),
            "{what}"
        );
    }
}

/// A filler that sends one request with fields of its own.
struct Fields {
    declaration: Declaration,
    fields: &'static [(&'static str, &'static str)],
    sent: Mutex<Option<Result<(), Fault>>>,
}

impl Filler for Fields {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, _: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let words = Piece::of(DataClass::Prompts, b"a red door".to_vec());
        let to = address(&format!("{API}/v1/fill"));
        let outgoing = Outgoing::new(to, vec![words])
            .unwrap()
            .with_fields(self.fields)
            .unwrap();
        let sent = bounds.sender().send(&outgoing, bounds).map(|_| ());
        *self.sent.lock().unwrap() = Some(sent.clone());
        sent?;
        Err(Fault::new(FaultKind::Unsupported, "the test's filler paints nothing").unresolved())
    }
}

#[test]
fn the_check_looks_at_a_requests_own_fields_and_finds_a_folder_kept_out_in_any_case_on_macos() {
    let identity = ModelIdentity::new("fields", "m", "1");
    let declaration = Declaration::new(identity, &[])
        .unwrap()
        .sending_to(recipient());
    let check = || {
        SendCheck::new()
            .keeping_out(std::path::Path::new("/Work/Store"))
            .unwrap()
    };
    // A field's value that names the folder kept out is refused, as words are.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let leaked: &'static str = Box::leak("see /Work/Store/runs".to_owned().into_boxed_str());
    let fields: &'static [(&'static str, &'static str)] = Box::leak(Box::new([("x-note", leaked)]));
    let filler = Arc::new(Fields {
        declaration: declaration.clone(),
        fields,
        sent: Mutex::new(None),
    });
    let ran = run_one(filler, ways(&sender, check()), Secrets::none());
    let fault = ran.fault.clone().unwrap();
    assert_eq!((fault.sentence(), sender.requests()), (KEPT_OUT, 0));
    // The folder in another case: found on macOS, whose file system does not tell
    // case apart; elsewhere the words are sent.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let words = vec![Piece::of(
        DataClass::Prompts,
        b"see /work/STORE/runs".to_vec(),
    )];
    let ran = run_one(posting(words), ways(&sender, check()), Secrets::none());
    if cfg!(target_os = "macos") {
        let fault = ran.fault.clone().unwrap();
        assert_eq!((fault.sentence(), sender.requests()), (KEPT_OUT, 0));
    } else {
        assert_eq!((ran.fault.clone(), sender.requests()), (None, 1));
    }
}

#[test]
fn a_store_under_a_folder_written_in_another_unicode_case_is_kept_out() {
    // The security review of S1's second pull request: APFS does not tell case apart in
    // letters beyond ASCII either, so a store under `/Work/Ölwerk` is the folder
    // `/work/ölwerk` there. On macOS the words are refused; elsewhere they are sent.
    let check = || {
        SendCheck::new()
            .keeping_out(std::path::Path::new("/Work/Ölwerk/Store"))
            .unwrap()
    };
    for written in ["see /work/ölwerk/store/runs", "see /WORK/ÖLWERK/STORE/runs"] {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let words = vec![Piece::of(DataClass::Prompts, written.as_bytes().to_vec())];
        let ran = run_one(posting(words), ways(&sender, check()), Secrets::none());
        if cfg!(target_os = "macos") {
            let fault = ran.fault.clone().unwrap();
            assert_eq!(
                (fault.sentence(), sender.requests()),
                (KEPT_OUT, 0),
                "{written}"
            );
        } else {
            assert_eq!(
                (ran.fault.clone(), sender.requests()),
                (None, 1),
                "{written}"
            );
        }
    }
    // A sigma lowers alike wherever it stands: the folder `/x/ΑΣ` is found in
    // `/x/ΑΣB/…`, where `str`'s lowering makes the folder's last letter a final sigma
    // and the text's not.
    let sigma = || {
        SendCheck::new()
            .keeping_out(std::path::Path::new("/x/ΑΣ"))
            .unwrap()
    };
    for written in ["see /x/ΑΣB/runs", "see /x/ας/runs", "see /x/ασ"] {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let words = vec![Piece::of(DataClass::Prompts, written.as_bytes().to_vec())];
        let ran = run_one(posting(words), ways(&sender, sigma()), Secrets::none());
        if cfg!(target_os = "macos") {
            let fault = ran.fault.clone().unwrap();
            assert_eq!(
                (fault.sentence(), sender.requests()),
                (KEPT_OUT, 0),
                "{written}"
            );
        }
    }
    // Another folder whose letters differ beyond case is sent, here and on macOS.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let words = vec![Piece::of(
        DataClass::Prompts,
        b"see /work/olwerk/store".to_vec(),
    )];
    let ran = run_one(posting(words), ways(&sender, check()), Secrets::none());
    assert_eq!((ran.fault.clone(), sender.requests()), (None, 1));
}

#[test]
fn an_answer_that_points_to_an_address_holding_a_secret_of_the_run_is_not_handed_to_the_backend() {
    let key = made_up_key("moved");
    let secrets = Secrets::new([key.clone()]).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![
        moved(&format!("{API}/v1/fill?sig={key}")),
        ok(),
    ]));
    let ran = run_one(
        posting(vec![Piece::of(DataClass::Prompts, b"a red door".to_vec())]),
        ways(&sender, SendCheck::new()),
        secrets,
    );
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        (fault.kind(), fault.stage(), fault.sentence()),
        (FaultKind::Malformed, Stage::Unresolved, MOVED_SECRET)
    );
    // The first address was handed the request; the one it pointed to, nothing.
    assert_eq!(sender.requests(), 1);
    assert!(!format!("{fault} {fault:?}").contains(&key));
}
