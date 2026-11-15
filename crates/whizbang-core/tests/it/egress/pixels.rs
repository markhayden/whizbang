//! Nothing but pixels (ADR 0019; plan 1B, task E6): what leaves for a recipient holds
//! no image metadata, no file name and no path. Each test goes through an executor
//! built with the strand's check and the test kit's fake sender; nothing is sent
//! anywhere.

use std::path::Path;
use std::sync::{Arc, Mutex};

use whizbang_core::artifact::{self, Artifact, ArtifactType, ImageRole};
use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillRequest, Filler, Painted, Picture, Stage, Support,
};
use whizbang_core::contract::record::{Handed, Status};
use whizbang_core::contract::{
    ActionDescriptor, Call, Capabilities, Capability, Executor, InputPort, Kind, ModelIdentity,
    Produced, RefusalKind, Requirement, Secrets,
};
use whizbang_core::egress::{Classes, DataClass, NotAllowed, Outgoing, Piece, SendCheck, Ways};
use whizbang_core::identity::{CanonicalPixels, DecodeLimits};
use whizbang_testkit::capabilities::{FakeFiller, Paint, roster};
use whizbang_testkit::sending::FakeSender;

use super::support::{
    ACCOUNT_CUT, API, World, a_prompt, address, approving, engine, entry, holds, ok, policy,
    recipient, run_in, run_one, scripted, stored,
};
use crate::artifact::support::image_of;
use crate::identity::support::{self as raw, Curve, Grid, RawPng, WIDE_COLORANTS};

const EVERY_CLASS: [&str; 4] = ["source_images", "derived_images", "prompts", "evidence"];

/// What the check answers of a part that holds a path.
const PATH: &str = "a part of the request holds an absolute path, and no path leaves the \
                    machine";

/// What it answers of a request that holds a folder it was told to keep out.
const KEPT_OUT: &str = "the request holds the path of a folder that is kept out of what \
                        leaves the machine";

/// What it answers of a part that is an image file the encoder did not write.
const NOT_PIXELS: &str = "a part of the request is an image file that whizbang's encoder \
                          did not write, and an image leaves only as its pixels";

/// The ways out of an executor that approves the recipient for every class, with
/// this check.
fn ways(sender: &Arc<FakeSender>, check: SendCheck) -> Ways {
    Ways::through(sender.clone())
        .checked_by(Arc::new(check))
        .under(policy(&approving(&EVERY_CLASS)))
}

/// A filler that names the recipient and sends these pieces to the API.
fn posting(pieces: Vec<Piece>) -> Arc<FakeFiller> {
    let filler = FakeFiller::new(Paint::Inverse).sending_to(recipient());
    Arc::new(filler.posting(address(&format!("{API}/v1/fill")), pieces))
}

fn words(class: DataClass, text: &str) -> Piece {
    Piece::of(class, text.as_bytes().to_vec())
}

fn picture_of(width: u32, height: u32, rgba: Vec<u8>) -> Picture {
    Picture::new(CanonicalPixels::from_rgba(width, height, rgba).unwrap()).unwrap()
}

/// A picture of this size, every byte made from where it is.
fn patterned(width: u32, height: u32) -> Picture {
    let bytes = width as usize * height as usize * 4;
    let rgba = (0..bytes).map(|at| (at * 7 + at / 251) as u8).collect();
    picture_of(width, height, rgba)
}

/// The chunks of a PNG, each by its name with its data, read strictly: the signature,
/// then chunks whose checksums are right, the last of them the end, and nothing after.
fn chunks(file: &[u8]) -> Vec<(String, Vec<u8>)> {
    assert_eq!(&file[..8], b"\x89PNG\r\n\x1a\n");
    let mut rest = &file[8..];
    let mut read = Vec::new();
    while !rest.is_empty() {
        let length = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        let (name, data) = (&rest[4..8], &rest[8..8 + length]);
        let sum = u32::from_be_bytes(rest[8 + length..12 + length].try_into().unwrap());
        assert_eq!(sum, raw::crc32(&rest[4..8 + length]), "a chunk's checksum");
        read.push((String::from_utf8(name.to_vec()).unwrap(), data.to_vec()));
        rest = &rest[12 + length..];
    }
    read
}

fn names(file: &[u8]) -> Vec<String> {
    chunks(file).into_iter().map(|(name, _)| name).collect()
}

/// The pixels a decoder that is not whizbang's reads from a file: red, green, blue
/// and alpha, with the size.
fn decoded(file: &[u8]) -> (u32, u32, Vec<u8>) {
    let read = image::load_from_memory_with_format(file, image::ImageFormat::Png).unwrap();
    let read = read.into_rgba8();
    (read.width(), read.height(), read.into_raw())
}

/// A file of the encoder's with one byte set, and the checksums a reader would catch
/// the change by written again: the header's, the image data's, and, when `rows`
/// says so, the one of the rows inside the image data.
fn set(file: &[u8], at: usize, to: u8, rows: bool) -> Vec<u8> {
    let mut file = file.to_vec();
    file[at] = to;
    let end = file.len();
    let header = raw::crc32(&file[12..29]);
    file[29..33].copy_from_slice(&header.to_be_bytes());
    if rows {
        // The rows are what the blocks hold: each block after its five bytes.
        let (mut low, mut high) = (1u32, 0u32);
        for block in file[43..end - 20].chunks(65_535 + 5) {
            for byte in &block[5..] {
                low = (low + u32::from(*byte)) % 65_521;
                high = (high + low) % 65_521;
            }
        }
        file[end - 20..end - 16].copy_from_slice(&((high << 16) | low).to_be_bytes());
    }
    let data = raw::crc32(&file[37..end - 16]);
    file[end - 16..end - 12].copy_from_slice(&data.to_be_bytes());
    file
}

fn refused_as(fault: &Fault) -> (FaultKind, Stage, &str) {
    (fault.kind(), fault.stage(), fault.sentence())
}

// --- an image leaves as its pixels ------------------------------------------------------------

/// A filler that names the recipient and, for every request, sends the request's
/// image as the encoder writes it, and its words beside it.
struct Shows {
    declaration: Declaration,
    sent: Mutex<Option<Result<(), Fault>>>,
}

impl Filler for Shows {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let image = Piece::picture(DataClass::SourceImages, request.image()).unwrap();
        let prompt = words(DataClass::Prompts, "a red door");
        let to = address(&format!("{API}/v1/fill"));
        let outgoing = Outgoing::new(to, vec![image, prompt]).unwrap();
        let sent = bounds.sender().send(&outgoing, bounds).map(|_| ());
        *self.sent.lock().unwrap() = Some(sent);
        Err(Fault::new(FaultKind::Unsupported, "the test's filler paints nothing").unresolved())
    }
}

#[test]
fn an_image_imported_with_an_exif_block_a_text_chunk_and_a_colour_profile_is_sent_and_no_marker_is_in_any_byte_the_fake_was_handed()
 {
    // A PNG written by hand, with a colour profile, a text chunk and an EXIF block,
    // each holding a marker.
    let markers = ["MARKER-PROFILE-7Q", "MARKER-TEXT-7Q", "MARKER-EXIF-7Q"];
    let profile = raw::matrix_profile(&WIDE_COLORANTS, &Curve::SrgbParametric);
    let mut named = markers[0].as_bytes().to_vec();
    named.extend_from_slice(&[0, 0]);
    named.extend_from_slice(&raw::zlib_stored(&profile));
    let mut text = b"Comment\0".to_vec();
    text.extend_from_slice(markers[1].as_bytes());
    let mut exif = raw::exif_orientation(1);
    exif.extend_from_slice(markers[2].as_bytes());
    let grid = Grid::new(raw::WIDTH, raw::HEIGHT, raw::picture());
    let file = RawPng::rgb(&grid)
        .before(b"iCCP", named)
        .before(b"tEXt", text)
        .after(b"eXIf", exif)
        .bytes();
    assert_eq!(
        names(&file),
        ["IHDR", "iCCP", "tEXt", "IDAT", "eXIf", "IEND"]
    );
    for marker in markers {
        assert!(holds(&file, marker), "the file holds {marker}");
    }

    // It is imported, and an action that is given it asks a filler to fill it in.
    let world = World::new();
    let image = image_of(file, ImageRole::Picture);
    let limits = DecodeLimits::default();
    let pixels = image.pixels(&limits).unwrap();
    {
        let store = world.store();
        let mut publication = store.begin().unwrap();
        artifact::put(&mut publication, &Artifact::from(image.clone())).unwrap();
        publication.publish().unwrap();
    }
    let draft = ActionDescriptor::draft("fills", "1.0.0", Kind::Primitive, "A test action.")
        .needs(Requirement::new(Capability::Fill))
        .input(InputPort::required(
            "image",
            ArtifactType::Image,
            "The image.",
        ));
    let fills = scripted(draft.build().unwrap(), |call, context| {
        let limits = DecodeLimits::default();
        let id = call.inputs()[0].id();
        let Ok(Artifact::Image(image)) =
            artifact::get(context.store(), ArtifactType::Image, id, &limits)
        else {
            panic!("the input is an image");
        };
        let picture = Picture::new(image.pixels(&limits).unwrap()).unwrap();
        let (width, height) = (picture.width(), picture.height());
        let plane = vec![255; (width * height) as usize];
        let support = Support::from_plane(width, height, plane).unwrap();
        let request = FillRequest::new(picture, support, None).unwrap();
        let fill = context.capabilities().fill().unwrap();
        fill.fill(&request).unwrap_err();
        Ok(Produced::nothing())
    });
    let model = ModelIdentity::new("fake", "fake-filler", "1");
    let filler = Arc::new(Shows {
        declaration: Declaration::new(model, &[])
            .unwrap()
            .sending_to(recipient()),
        sent: Mutex::new(None),
    });
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let check = SendCheck::new().keeping_out(&world.root()).unwrap();
    let executor = engine(
        &world,
        roster(None, Some(filler.as_ref()), None),
        Capabilities::none().with_fill(filler.clone()),
        vec![fills],
    )
    .with_ways(ways(&sender, check));
    let finished = executor
        .submit(&Call::new("fills").input("image", image.id()))
        .unwrap();
    let (record, _) = stored(&world, finished.run());
    assert_eq!(record.outcome().status, Status::Completed);
    assert_eq!(filler.sent.lock().unwrap().take(), Some(Ok(())));

    // What the fake was handed: the image's pixels as the encoder writes them, and the
    // prompt. No marker is in any byte of it.
    let hops = sender.hops();
    assert_eq!(hops.len(), 1);
    let pieces = hops[0].outgoing().pieces();
    assert_eq!(pieces.len(), 2);
    let mut handed = hops[0].to().rest().as_bytes().to_vec();
    for piece in pieces {
        handed.extend_from_slice(piece.bytes());
    }
    for marker in markers {
        assert!(!holds(&handed, marker), "{marker} was handed over");
    }
    for word in ["Comment", "icc", "eXIf", "tEXt", "iCCP", "Exif"] {
        assert!(!holds(&handed, word), "{word} was handed over");
    }
    assert_eq!(pieces[0].class(), Some(DataClass::SourceImages));
    assert_eq!(names(pieces[0].bytes()), ["IHDR", "IDAT", "IEND"]);
    let size = (pixels.width(), pixels.height());
    assert_eq!(size, (raw::WIDTH, raw::HEIGHT));
    assert_eq!(
        decoded(pieces[0].bytes()),
        (size.0, size.1, pixels.rgba().to_vec()),
        "the pixels that left are the image's canonical pixels"
    );
    assert_eq!(pieces[1].bytes(), b"a red door");
}

#[test]
fn the_encoders_output_for_a_picture_holds_only_the_chunks_the_test_lists() {
    // One pixel; a few; rows that fill one stored block exactly (255 rows of 257
    // bytes are 65,535); one byte more than a block; and several blocks.
    for (width, height) in [(1, 1), (6, 4), (64, 255), (16384, 1), (64, 256), (300, 200)] {
        let picture = patterned(width, height);
        let piece = Piece::picture(DataClass::DerivedImages, &picture).unwrap();
        assert_eq!(piece.class(), Some(DataClass::DerivedImages));
        let file = piece.bytes();
        let read = chunks(file);
        let listed: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(listed, ["IHDR", "IDAT", "IEND"], "{width} by {height}");
        // The header: the size, eight bits a sample, colour with alpha, the one
        // compression, the one filtering, not interlaced.
        let mut header = width.to_be_bytes().to_vec();
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert_eq!(read[0].1, header);
        assert_eq!(read[2].1, b"");
        // The image data is the rows, stored: each after a byte of nothing, in
        // blocks of at most 65,535 bytes, the last of them marked.
        let mut rows = Vec::new();
        for row in picture.rgba().chunks(width as usize * 4) {
            rows.push(0);
            rows.extend_from_slice(row);
        }
        assert_eq!(read[1].1, raw::zlib_stored(&rows), "{width} by {height}");
        // And a decoder that is not whizbang's reads the same pixels from it.
        assert_eq!(
            decoded(file),
            (width, height, picture.rgba().to_vec()),
            "{width} by {height}"
        );
        // The same picture is the same bytes.
        assert_eq!(
            Piece::picture(DataClass::DerivedImages, &picture).unwrap(),
            piece
        );
    }
}

#[test]
fn a_picture_as_the_encoder_writes_it_is_sent_whatever_its_pixels_spell() {
    // Pixels are not read as words: a picture whose bytes spell a path, and the
    // folder that is kept out, leaves as any picture does.
    let kept = format!("/{}/whizbang/store", "srv");
    let mut spelled = format!("/{}/someone/cat.png {kept} C:{}cat ", "Users", '\\').into_bytes();
    spelled.resize(spelled.len().div_ceil(4) * 4, b' ');
    let width = (spelled.len() / 4) as u32;
    let picture = picture_of(width, 1, spelled.clone());
    let piece = Piece::picture(DataClass::SourceImages, &picture).unwrap();
    assert!(
        holds(piece.bytes(), &kept),
        "the pixels are stored as they are"
    );
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let check = SendCheck::new().keeping_out(Path::new(&kept)).unwrap();
    let ran = run_one(
        posting(vec![piece.clone()]),
        ways(&sender, check),
        Secrets::none(),
    );
    assert_eq!(ran.fault, None);
    assert_eq!(sender.hops()[0].outgoing().pieces(), [piece]);
    // A picture of several blocks, whose rows do not start where its blocks do, and
    // one whose rows fill a block exactly.
    for (width, height) in [(300, 200), (64, 255), (64, 256)] {
        let piece = Piece::picture(DataClass::DerivedImages, &patterned(width, height)).unwrap();
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(
            posting(vec![piece.clone()]),
            ways(&sender, SendCheck::new()),
            Secrets::none(),
        );
        assert_eq!(ran.fault, None, "{width} by {height}");
        assert_eq!(sender.hops()[0].outgoing().pieces(), [piece]);
    }
    assert_eq!(
        ran.went(),
        [entry(
            API,
            Some(ACCOUNT_CUT),
            Classes::of(&[DataClass::SourceImages]),
            Handed::Sent
        )]
    );
}

#[test]
fn an_image_file_that_the_encoder_did_not_write_is_refused() {
    let picture = patterned(6, 4);
    let ours = Piece::picture(DataClass::SourceImages, &picture).unwrap();
    let ours = ours.bytes().to_vec();
    // A picture of one colour as another encoder writes it: compressed, where the
    // encoder here stores. (Pixels that do not compress, some other encoders store
    // too, and their file is then the very bytes the encoder here writes.)
    let flat = picture_of(32, 32, vec![90; 32 * 32 * 4]);
    let stored = Piece::picture(DataClass::SourceImages, &flat).unwrap();
    let mut compressed = Vec::new();
    image::RgbaImage::from_raw(32, 32, flat.rgba().to_vec())
        .unwrap()
        .write_to(
            &mut std::io::Cursor::new(&mut compressed),
            image::ImageFormat::Png,
        )
        .unwrap();
    assert_eq!(decoded(&compressed), decoded(stored.bytes()));
    assert_eq!(names(&compressed), ["IHDR", "IDAT", "IEND"]);
    assert!(compressed.len() < stored.bytes().len() / 4);
    // The encoder's file with a text chunk put in after its header.
    let mut text = b"Comment\0MARKER-TEXT-7Q".to_vec();
    let mut chunk = (text.len() as u32).to_be_bytes().to_vec();
    let mut named = b"tEXt".to_vec();
    named.append(&mut text);
    chunk.extend_from_slice(&named);
    chunk.extend_from_slice(&raw::crc32(&named).to_be_bytes());
    let mut with_text = ours.clone();
    with_text.splice(33..33, chunk);
    assert_eq!(names(&with_text), ["IHDR", "tEXt", "IDAT", "IEND"]);
    // The encoder's file with one byte more, one byte fewer, and one byte changed in
    // each of its parts: the header, a block's five bytes, a row's filter, a pixel,
    // the stream's checksum, the chunk's checksum, and the end.
    let mut longer = ours.clone();
    longer.push(0);
    let shorter = ours[..ours.len() - 1].to_vec();
    let changed = |at: usize| {
        let mut file = ours.clone();
        file[at] ^= 1;
        file
    };
    let end = ours.len();
    let mut cases: Vec<(String, Vec<u8>)> = vec![
        ("another encoder's".to_owned(), compressed),
        ("with a text chunk".to_owned(), with_text),
        ("one byte more".to_owned(), longer),
        ("one byte fewer".to_owned(), shorter),
    ];
    // 8 the header's length, 12 its name, 24 its depth, 29 its checksum, 33 the
    // data's length, 37 its name, 41 and 42 the stream's start, 43 the block's
    // mark, 44 and 46 its length twice, 48 the first row's filter, 49 a pixel.
    for at in [
        8, 12, 24, 25, 26, 27, 28, 29, 33, 37, 41, 42, 43, 44, 46, 48, 49,
    ] {
        cases.push((format!("byte {at} changed"), changed(at)));
    }
    for back in [1, 5, 9, 13, 17, 21] {
        cases.push((
            format!("byte {back} from the end changed"),
            changed(end - back),
        ));
    }
    // One byte changed and every checksum that would show it written again, so that
    // the file is one a reader of PNG may take, and is still not the encoder's: a
    // header that says the rows are interlaced; a block that is not marked as the
    // last; a block's length that is not said twice; a row that says it is
    // filtered; and, with the rows' own checksum left as it was, a pixel.
    assert_eq!(
        set(&ours, 28, 0, true),
        ours,
        "written again, it is the same file"
    );
    cases.push(("interlaced".to_owned(), set(&ours, 28, 1, false)));
    cases.push(("no last block".to_owned(), set(&ours, 43, 0, false)));
    cases.push((
        "a length not said twice".to_owned(),
        set(&ours, 46, 0, false),
    ));
    cases.push(("a filtered row".to_owned(), set(&ours, 48, 1, true)));
    let pixel = ours[49] ^ 1;
    cases.push((
        "a pixel past the rows' checksum".to_owned(),
        set(&ours, 49, pixel, false),
    ));
    // A filtered row in the second block of a larger picture: row 55 of 300 by 200
    // starts 520 bytes into it.
    let larger = Piece::picture(DataClass::SourceImages, &patterned(300, 200)).unwrap();
    let larger = larger.bytes().to_vec();
    let row_55 = 43 + (65_535 + 5) + 5 + (55 * 1201 - 65_535);
    assert_eq!(larger[row_55], 0);
    assert_eq!(set(&larger, row_55, 0, true), larger);
    cases.push((
        "a filtered row in a later block".to_owned(),
        set(&larger, row_55, 1, true),
    ));
    // And files of other kinds, by how each starts.
    let starts: [&[u8]; 6] = [
        b"\xff\xd8\xff\xe0\x00\x10JFIF\x00",
        b"GIF89a\x01\x00\x01\x00",
        b"GIF87a\x01\x00\x01\x00",
        b"II*\x00\x08\x00\x00\x00",
        b"MM\x00*\x00\x00\x00\x08",
        b"RIFF\x24\x00\x00\x00WEBPVP8 ",
    ];
    for start in starts {
        let mut file = start.to_vec();
        file.extend_from_slice(&[7; 40]);
        cases.push((format!("a file that starts {:?}", &start[..4]), file));
    }
    for (what, file) in cases {
        for class in [DataClass::SourceImages, DataClass::Prompts] {
            let sender = Arc::new(FakeSender::scripted(vec![ok()]));
            let pieces = vec![Piece::of(class, file.clone())];
            let ran = run_one(
                posting(pieces),
                ways(&sender, SendCheck::new()),
                Secrets::none(),
            );
            let fault = ran
                .fault
                .clone()
                .unwrap_or_else(|| panic!("{what} was sent"));
            assert_eq!(
                refused_as(&fault),
                (FaultKind::Rejected, Stage::NotSent, NOT_PIXELS),
                "{what}"
            );
            assert_eq!(sender.requests(), 0, "{what}");
            assert!(!holds(&ran.store, "MARKER-TEXT-7Q"));
        }
    }
    // The encoder's own file, untouched, is sent.
    let sender = Arc::new(FakeSender::scripted(vec![ok()]));
    let pieces = vec![Piece::of(DataClass::SourceImages, ours)];
    let ran = run_one(
        posting(pieces),
        ways(&sender, SendCheck::new()),
        Secrets::none(),
    );
    assert_eq!((ran.fault.clone(), sender.requests()), (None, 1));
}

// --- no path leaves ---------------------------------------------------------------------------

/// Absolute paths, each put together here so that no file of the repository holds one
/// whole.
fn absolute_paths() -> Vec<String> {
    let back = '\\';
    vec![
        format!("/{}/someone/pictures/cat.png", "Users"),
        format!("/{}/someone/cat.png", "home"),
        format!("/{}/whizbang-1/cat.png", "tmp"),
        format!("/{}/folders/ab/cdef/T/cat.png", "var"),
        format!("/{}/var/run/cat.png", "private"),
        format!("{}://{}/cat.png", "file", "/srv"),
        format!("C:{back}pictures{back}cat.png"),
        format!("d:{back}cat.png"),
    ]
}

#[test]
fn a_body_that_holds_an_absolute_path_or_the_stores_root_is_refused_by_the_sending_interface() {
    // An absolute path, alone and inside a sentence, in a part of each class and in
    // one that states none.
    for path in absolute_paths() {
        for text in [path.clone(), format!("Look at \"{path}\", please.")] {
            let stated = [
                Some(DataClass::Prompts),
                Some(DataClass::Evidence),
                Some(DataClass::SourceImages),
                None,
            ];
            for class in stated {
                let piece = match class {
                    Some(class) => words(class, &text),
                    None => Piece::unclassed(text.clone().into_bytes()),
                };
                let sender = Arc::new(FakeSender::scripted(vec![ok()]));
                let ran = run_one(
                    posting(vec![piece]),
                    ways(&sender, SendCheck::new()),
                    Secrets::none(),
                );
                let fault = ran
                    .fault
                    .clone()
                    .unwrap_or_else(|| panic!("{text} was sent"));
                assert_eq!(
                    refused_as(&fault),
                    (FaultKind::Rejected, Stage::NotSent, PATH),
                    "{text}"
                );
                assert_eq!(sender.requests(), 0, "{text}");
                // The refusal is in the record, and shows nothing of the path.
                assert_eq!(ran.refused().len(), 1);
                assert_eq!(ran.refused()[0].0, RefusalKind::RequestNotApproved);
                assert!(ran.refused()[0].1.contains(PATH));
                assert!(!holds(&ran.store, "cat.png"), "{text}");
            }
        }
    }

    // The store's root, which the check was told to keep out: in a part, and in the
    // address.
    let world = World::new();
    let root = world.root();
    let root_text = root.to_str().unwrap().to_owned();
    let check = SendCheck::new().keeping_out(&root).unwrap();
    let sender = Arc::new(FakeSender::scripted(vec![ok(), ok()]));
    let in_a_part = posting(vec![
        words(DataClass::Prompts, "a red door"),
        words(
            DataClass::Evidence,
            &format!("it was read from {root_text}/objects"),
        ),
    ]);
    let ran = run_in(
        &world,
        in_a_part,
        ways(&sender, check.clone()),
        Secrets::none(),
    );
    let fault = ran.fault.clone().unwrap();
    assert_eq!(
        refused_as(&fault),
        (FaultKind::Rejected, Stage::NotSent, KEPT_OUT)
    );
    assert_eq!(sender.requests(), 0);
    assert!(ran.refused()[0].1.contains(KEPT_OUT));
    assert!(!holds(&ran.bytes, &root_text));

    // A folder the check was told to keep out is found where no other rule would
    // find it, in a part and in an address.
    let kept = format!("/{}/whizbang/store", "srv");
    let check = SendCheck::new().keeping_out(Path::new(&kept)).unwrap();
    let in_a_part = posting(vec![words(
        DataClass::Prompts,
        &format!("in {kept}, it says"),
    )]);
    let filler = FakeFiller::new(Paint::Inverse).sending_to(recipient());
    let to = address(&format!("{API}/v1/fill?from={kept}/a"));
    let in_the_address = Arc::new(filler.posting(to, a_prompt()));
    let fillers: [Arc<FakeFiller>; 2] = [in_a_part, in_the_address];
    for filler in fillers {
        let ran = run_one(filler, ways(&sender, check.clone()), Secrets::none());
        let fault = ran.fault.clone().unwrap();
        assert_eq!(
            refused_as(&fault),
            (FaultKind::Rejected, Stage::NotSent, KEPT_OUT)
        );
        assert_eq!(sender.requests(), 0);
        assert!(!holds(&ran.store, &kept));
    }
    // The same parts with a check that was told of no folder: sent.
    let ran = run_one(
        posting(vec![words(
            DataClass::Prompts,
            &format!("in {kept}, it says"),
        )]),
        ways(&sender, SendCheck::new()),
        Secrets::none(),
    );
    assert_eq!((ran.fault.clone(), sender.requests()), (None, 1));
}

#[test]
fn every_part_of_a_request_is_looked_at() {
    let path = format!("/{}/someone/cat.png", "home");
    let picture = Piece::picture(DataClass::DerivedImages, &patterned(6, 4)).unwrap();
    for at in 0..4 {
        let mut pieces = vec![
            words(DataClass::Prompts, "a red door"),
            picture.clone(),
            words(DataClass::Evidence, "it is red"),
            words(DataClass::Prompts, "and it is a door"),
        ];
        pieces[at] = words(DataClass::Evidence, &path);
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(
            posting(pieces),
            ways(&sender, SendCheck::new()),
            Secrets::none(),
        );
        let fault = ran
            .fault
            .clone()
            .unwrap_or_else(|| panic!("part {at} was sent"));
        assert_eq!(fault.sentence(), PATH, "part {at}");
        assert_eq!(sender.requests(), 0, "part {at}");
    }
}

#[test]
fn words_that_hold_no_path_are_sent_as_they_are() {
    let back = '\\';
    let texts = [
        "a red door".to_owned(),
        String::new(),
        "this and/or that, 1/2 of it, at 3 / 4".to_owned(),
        "see https://example.com/a/b?x=1 and http://example.com:8080/home.html".to_owned(),
        "the Users of a home, in a tmp of their own; private; var".to_owned(),
        "a file: one; C: a drive; c:/forward".to_owned(),
        // A letter, a colon and a backslash after a longer word is no drive.
        format!("Answer:{back}nYes, and so on:{back}"),
        "/relative-looking/but/not/a/known/root".to_owned(),
        "\u{89}PNG is how a PNG starts, in words".to_owned(),
        // A colon and a backslash after no letter, and after a letter that ends a
        // number, are no drive.
        format!("a dash - :{back} and step 1a:{back}n"),
        // Another container that starts as a WebP does.
        "RIFF....WAVEfmt is how a sound starts".to_owned(),
        // Bytes that are too short to be a file of the encoder's, and say a size
        // where it writes one.
        "sixteen letters.\0\0\0\u{1}\0\0\0\u{1}abc".to_owned(),
    ];
    for text in texts {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let pieces = vec![words(DataClass::Prompts, &text)];
        let ran = run_one(
            posting(pieces.clone()),
            ways(&sender, SendCheck::new()),
            Secrets::none(),
        );
        assert_eq!(ran.fault, None, "{text}");
        assert_eq!(sender.hops()[0].outgoing().pieces(), pieces, "{text}");
    }
}

#[test]
fn known_words_that_only_look_like_a_path_are_refused_too() {
    // The rule is by how a path starts, and errs toward refusing: an address whose
    // path starts as a home folder does, and a one-letter label before an escaped
    // line break, are refused as paths.
    let back = '\\';
    let texts = [
        format!("see https://example.com/{}/page", "home"),
        format!("Q:{back}nA:{back}n"),
    ];
    for text in texts {
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(
            posting(vec![words(DataClass::Prompts, &text)]),
            ways(&sender, SendCheck::new()),
            Secrets::none(),
        );
        assert_eq!(ran.fault.clone().unwrap().sentence(), PATH, "{text}");
        assert_eq!(sender.requests(), 0);
    }
}

#[test]
fn a_check_keeps_out_a_bounded_number_of_folders_and_prints_none() {
    assert_eq!(
        SendCheck::new().keeping_out(Path::new("")).map(|_| ()),
        Err(NotAllowed::Empty)
    );
    let mut check = SendCheck::new();
    for at in 0..8 {
        let folder = format!("/{}/whizbang/store-{at}", "srv");
        check = check.keeping_out(Path::new(&folder)).unwrap();
    }
    let one_more = format!("/{}/whizbang/store-8", "srv");
    assert_eq!(
        check.clone().keeping_out(Path::new(&one_more)).map(|_| ()),
        Err(NotAllowed::TooMany)
    );
    let printed = format!("{check:?}");
    assert_eq!(printed, "SendCheck(8 folders kept out)");
    assert_eq!(
        format!("{:?}", SendCheck::new()),
        "SendCheck(0 folders kept out)"
    );
    // Each of the eight is kept out, the last as the first.
    for at in [0, 7] {
        let text = format!("in /{}/whizbang/store-{at}/x", "srv");
        let sender = Arc::new(FakeSender::scripted(vec![ok()]));
        let ran = run_one(
            posting(vec![words(DataClass::Prompts, &text)]),
            ways(&sender, check.clone()),
            Secrets::none(),
        );
        assert_eq!(ran.fault.clone().unwrap().sentence(), KEPT_OUT);
        assert_eq!(sender.requests(), 0);
    }
}
