//! Fixed vectors. Each expected value below was worked out apart from the code under
//! test, by a separate statement of ADR 0002's encoding written from the record's text: the
//! identities of the six types, and the bytes of three files. A change to any of them is a
//! change to the record, and a new version of the kind.

use whizbang_core::artifact::{
    Artifact, Image, ImageRole, Line, Location, Mask, Origin, Shape, Source, Step, UseCase,
};
use whizbang_core::identity::{
    Binding, Destination, Digest, Direction, Mapping, Point, Projection, Space, Topology,
};

use super::support::*;

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// A PNG of 2 by 1 pixels, red then blue, written by hand: 8-bit RGB, one row.
const TWO_PIXELS: &str = "89504e470d0a1a0a0000000d49484452000000020000000108020000007b40e8dd\
    0000000d4944415478da63f8cf0004ff01070001ff3d7d8c490000000049454e44ae426082";

fn find_box() -> Location {
    Location::new(
        on("photo"),
        Shape::Box {
            left: 1.0,
            top: 0.5,
            right: 6.25,
            bottom: 5.0,
        },
        made_by("find"),
    )
    .unwrap()
}

fn two_pixels(role: ImageRole) -> Image {
    Image::new(
        unhex(TWO_PIXELS),
        Projection::Flat,
        Topology::Bounded,
        role,
        Origin::new(
            Step::new("import", "1")
                .argument("from", "a test")
                .input("file", Digest::of_bytes(&unhex(TWO_PIXELS))),
        )
        .unwrap(),
        &limits(),
    )
    .unwrap()
}

#[test]
fn the_identity_of_each_type_is_the_one_the_record_gives() {
    let binding = on("photo");
    assert_eq!(
        binding.id().to_string(),
        "sha256:42b6562c18faefba91e7fe6bb53296e56be8f33df0e435e32160296f84fb638b"
    );
    let line = line(binding.clone());
    let composite = Binding::new(
        picture("composite"),
        space(),
        picture("photo"),
        space(),
        Direction::SourceToDestination,
        Mapping::Identity,
    )
    .unwrap();
    assert_eq!(
        composite.destination(),
        Destination::Image(picture("photo"))
    );
    let full = full_mask(binding.clone());
    let cases: Vec<(&str, Artifact, &str)> = vec![
        (
            "a box",
            find_box().into(),
            "78e39f0209000dd7fc09ef67a3e0de77c0e3fa8854e41b31a1d3d843a587487b",
        ),
        (
            "a line",
            line.clone().into(),
            "e9b0aebddbb5826b0318481570a48bac922e8376bebae28de0db3349951caeab",
        ),
        (
            "a series",
            sigma(binding.clone(), 0.75).into(),
            "14bd9eedbeffd2243421d3f82f129a34b5e2459cb7d3e21847520d9760440d90",
        ),
        (
            "a band profile",
            band(binding.clone(), line.id(), 1.5).into(),
            "7b6eb32361aeab129b8f2a5b05892b8355fa833cbe3f23006be95e5c612c2a4c",
        ),
        (
            "a plan",
            plan().into(),
            "a3b1b55315e10fdf61754b27e003227d6871e3374d082712060f2de7245b6440",
        ),
        (
            "a mask of a core alone",
            rect_mask(binding).into(),
            "8a87ba2e2b2cafa64e8074079f899eac38c73adc560a992e3c88b56e934af793",
        ),
        (
            "a mask with everything",
            full.clone().into(),
            "d15d31c3875e9dab35f4d7b776e6359fb6fc78c573e42cb3dc428b0b5f9d53eb",
        ),
        (
            // A derivation is made only of two real pictures (`rebind.rs`), and these
            // images are names. The rebound mask is the mask made by hand on the
            // derivation's binding (ADR 0004), and `rebind.rs` holds a rebind to that.
            "that mask, as a rebind onto a composite leaves it",
            full_mask_moved(&full, composite, line.id()).into(),
            "facab84ab7790740f0c0705525a93d9bc0ef5e3168881c688de69fe4fd14b7b0",
        ),
        (
            "a picture",
            two_pixels(ImageRole::Picture).into(),
            "7e7bd6d5ff2e1b323ce445cd05abaece92191fb8fa1efc47ac9eb384e72ccfe2",
        ),
        (
            "a rendering",
            two_pixels(ImageRole::Rendering).into(),
            "02e950709dad347d2ef642a4cc15dd55f3818105ce5d0b9680a8e28440140b60",
        ),
    ];
    for (what, artifact, expected) in cases {
        assert_eq!(artifact.id().0.hex(), expected, "{what}");
    }
}

#[test]
fn the_bytes_of_a_file_are_the_ones_the_record_gives() {
    // A location, whole: the kind, the binding inside it, the box, the origin.
    let expected = "7768697a62616e673a6c6f636174696f6e2f3100617768697a62616e673a62696e64696e\
        672f31006455c64d0fcd6f9d5f7c828093857e3fdfda68478bb4e9bd24d481ef391c7804e875000000\
        0000000008750000000000000006750000000000000000750000000000000000750000000000000000\
        7500000000000000087500000000000000067500000000000000007500000000000000007500000000\
        00000000750000000000000000750000000000000000663ff0000000000000663fe000000000000066\
        40190000000000006640140000000000006f7500000000000000017500000000000000016200000000\
        0000000466696e6462000000000000000131750000000000000000750000000000000000";
    let expected: String = expected.split_whitespace().collect();
    let bytes = find_box().to_bytes();
    assert_eq!(bytes, unhex(&expected));
    assert_eq!(Location::from_bytes(&bytes, &limits()).unwrap(), find_box());

    // A mask, with its planes as runs, and an image, with its picture held whole: each by
    // its length and the digest of its bytes.
    let mask = rect_mask(on("photo"));
    let bytes = mask.to_bytes();
    assert_eq!(bytes.len(), 369);
    assert_eq!(
        Digest::of_bytes(&bytes).hex(),
        "43b29fa404cdc95fed9c6ec04d98a4b94c3a3573810163886bbfbf6979215856"
    );
    assert_eq!(Mask::from_bytes(&bytes, &limits()).unwrap(), mask);
    let bytes = two_pixels(ImageRole::Picture).to_bytes();
    assert_eq!(bytes.len(), 310);
    assert_eq!(
        Digest::of_bytes(&bytes).hex(),
        "4d3f29e48c3c8e0a1105f74b6a585134e26087794b591da87bc1e9717afd939e"
    );
}

#[test]
fn every_shape_has_the_number_the_record_gives() {
    // One location of each kind of shape, 0 to 6, and both ways to state a line. A kind
    // given another number, or two that swap theirs, changes one of these.
    let photo = on("photo");
    let panorama = Binding::root(picture("panorama"), Space::panorama(400, 200).unwrap());
    let path = vec![Point::new(1.0, 1.0), Point::new(7.5, 5.0)];
    let cases: Vec<(&str, Binding, Shape, &str)> = vec![
        (
            "0, a box",
            photo.clone(),
            Shape::Box {
                left: 1.0,
                top: 0.5,
                right: 6.25,
                bottom: 5.0,
            },
            "78e39f0209000dd7fc09ef67a3e0de77c0e3fa8854e41b31a1d3d843a587487b",
        ),
        (
            "1, a polygon",
            photo.clone(),
            Shape::Polygon {
                points: vec![
                    Point::new(1.0, 1.0),
                    Point::new(6.0, 1.5),
                    Point::new(3.0, 5.0),
                ],
            },
            "b779213bb9eb0048c949a4777f817f07fe18fe98d080d46106a78b810dafd9bb",
        ),
        (
            "2, a view",
            panorama,
            Shape::View {
                yaw: -30.0,
                pitch: 12.5,
                fov: 60.0,
            },
            "d238069c651186c9e5af95974238cc350bb801517f9a79da0c5377f172aa3a66",
        ),
        (
            "3, the whole image",
            photo.clone(),
            Shape::WholeImage,
            "a22f8cddab19df3bd8a46e5ca6ce036ae3bf93fc26c64a5d475624fb351ae1a9",
        ),
        (
            "4, absent",
            photo.clone(),
            Shape::Absent,
            "27c43afd2890f2d4c4a75492623c3d0f6eadf4d0a690951bc7aee910e425a230",
        ),
        (
            "5, a line that is a column",
            photo.clone(),
            Shape::Line {
                line: Line::Column(4.0),
                width: 3.0,
            },
            "e9b0aebddbb5826b0318481570a48bac922e8376bebae28de0db3349951caeab",
        ),
        (
            "5, a line that is a path",
            photo.clone(),
            Shape::Line {
                line: Line::Path(path.clone()),
                width: 2.0,
            },
            "19f6da89972444f7c47765e3c5acc8dd32dfe469a5522cd0c51d625246f781e4",
        ),
        (
            "6, a stroke",
            photo,
            Shape::Stroke { path, radius: 0.5 },
            "b428e68b604231b2cbb44e5b2c1a63d383115f0ce05905fb290e3d9a7162a990",
        ),
    ];
    for (what, binding, shape, expected) in cases {
        let location = Location::new(binding, shape, made_by("find")).unwrap();
        assert_eq!(location.id().0.hex(), expected, "{what}");
        let read = Location::from_bytes(&location.to_bytes(), &limits()).unwrap();
        assert_eq!(read, location, "{what}");
    }
}

#[test]
fn every_use_of_a_mask_has_the_number_the_record_gives() {
    // The same planes on the same image, made for each of the five uses, 0 to 4.
    let cases = [
        (
            UseCase::Seam,
            "7c824f5454c366dca4333737ad57c49ff00fecef723ba6b05a6b31f781286cb1",
        ),
        (
            UseCase::Remove,
            "3440f92363df609296e5f8782c7ba3b2911424eec6dc7caef3542a61a804b282",
        ),
        (
            UseCase::Repair,
            "5318d8a92818329cabab65b7da2c1b95ee8be410b883402ce174e8f2a1d0476a",
        ),
        (
            UseCase::Protect,
            "cde86e1bacd601e40ddfeb0cebad93cd77e70001f6dcc08016300b4d721af0fb",
        ),
        (
            UseCase::Custom,
            "8a87ba2e2b2cafa64e8074079f899eac38c73adc560a992e3c88b56e934af793",
        ),
    ];
    for (use_case, expected) in cases {
        let mask = Mask::from_core(
            on("photo"),
            use_case,
            rect(2, 1, 5, 4),
            Source::default(),
            made_by("rect"),
        )
        .unwrap();
        assert_eq!(mask.id().0.hex(), expected, "{use_case:?}");
        let read = Mask::from_bytes(&mask.to_bytes(), &limits()).unwrap();
        assert_eq!(read.use_case(), use_case);
    }
}

#[test]
fn a_run_longer_than_65535_samples_is_one_run() {
    // A flat plane of 300 by 300: 90,000 samples of one value. A run's length is four
    // bytes, so each plane is one run, and the whole file is 340 bytes.
    let large = Binding::root(picture("large"), Space::flat(300, 300).unwrap());
    let mask = Mask::from_core(
        large,
        UseCase::Custom,
        vec![255; 90_000],
        Source::default(),
        made_by("flood"),
    )
    .unwrap();
    assert_eq!(
        mask.id().0.hex(),
        "cec516fa99ee7ff0c8f1e35690689fa1528bbed78e7602f155c166eacb556d65"
    );
    let bytes = mask.to_bytes();
    assert_eq!(bytes.len(), 340);
    assert_eq!(
        Digest::of_bytes(&bytes).hex(),
        "7f1d6e454db01a71604867c6cace09911677437e9fd567571341d119386e4717"
    );
    // The core: `p`, 90,000 samples, one run, of 255, 90,000 long (0x00015f90).
    let plane = [
        &b"p"[..],
        &90_000u64.to_be_bytes(),
        &1u64.to_be_bytes(),
        &[255, 0x00, 0x01, 0x5f, 0x90],
    ]
    .concat();
    assert!(bytes.windows(plane.len()).any(|window| window == plane));
    assert_eq!(Mask::from_bytes(&bytes, &limits()).unwrap(), mask);
}
