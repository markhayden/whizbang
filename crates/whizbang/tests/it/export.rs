//! `export::…`: masks out of a store and into another (plan m3, task F1, item 7), by the
//! real binary: an artifact's canonical file by `store export <ID> --to`, which `store
//! import` brings into another store with the same identity; a mask as a grey PNG by
//! `store export --mask`, edited and brought back onto its image by `store import --mask
//! --onto`; and where an export may write. Pictures of 64 by 48.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::artifact::{self, ArtifactId, ArtifactType, Mask};
use whizbang_core::identity::DecodeLimits;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::scratch::Scratch;

use crate::wired::{Ran, whizbang};

/// A store with a picture of 64 by 48 and a mask on it, a block from 8, 8 to 24, 20: the
/// store's path, the image and the mask.
fn with_mask(dir: &Scratch, name: &str) -> (PathBuf, String, String) {
    let root = dir.path().join(name);
    drop(Store::open(&root, Arc::new(RealFs)).unwrap());
    let pictures = dir.path().join(format!("{name}-pictures"));
    std::fs::create_dir_all(&pictures).unwrap();
    let image = crate::anthropic::import_picture(&root, &pictures, "base", 40);
    let mask = crate::fal::put_mask(&root, ArtifactId(image.parse().unwrap()));
    (root, image, mask.0.to_string())
}

fn ok(ran: Ran) -> String {
    assert_eq!(ran.code, Some(0), "{}", ran.err);
    ran.out
}

fn refused(ran: &Ran, said: &str) {
    assert_eq!(ran.code, Some(1), "{}", ran.out);
    assert!(ran.err.contains(said), "{}", ran.err);
}

fn mask_in(root: &Path, id: &str) -> Mask {
    let store = Store::open(root, Arc::new(RealFs)).unwrap();
    let read = artifact::get(
        &store,
        ArtifactType::Mask,
        ArtifactId(id.parse().unwrap()),
        &DecodeLimits::default(),
    );
    read.unwrap().into_mask().unwrap()
}

/// The identity a line `imported <kind> <id>` names.
fn imported(out: &str, kind: &str) -> String {
    let line = out.lines().last().unwrap();
    line.strip_prefix(&format!("imported {kind} "))
        .unwrap_or_else(|| panic!("{out}"))
        .to_owned()
}

#[test]
fn a_mask_exported_imports_into_another_store_with_the_same_identity() {
    let dir = Scratch::new("export-mask");
    let (here, image, mask) = with_mask(&dir, "here");
    let there = dir.path().join("there");
    let files = dir.path().join("files");
    std::fs::create_dir(&files).unwrap();
    let file = |name: &str| files.join(name);
    let to = |p: &Path| p.to_str().unwrap().to_owned();

    // Its canonical file, and the image's, into a store of their own: the same
    // identities, and the mask reads back whole.
    let said = ok(whizbang(
        &["store", "export", &mask, "--to", &to(&file("m.mask"))],
        &here,
    ));
    assert!(
        said.starts_with(&format!("exported mask {mask} as its canonical file to ")),
        "{said}"
    );
    ok(whizbang(
        &["store", "export", &image, "--to", &to(&file("i.image"))],
        &here,
    ));
    let made = ok(whizbang(
        &["store", "import", &to(&file("i.image")), "--create"],
        &there,
    ));
    assert_eq!(imported(&made, "image"), image);
    let made = ok(whizbang(&["store", "import", &to(&file("m.mask"))], &there));
    assert_eq!(imported(&made, "mask"), mask);
    assert_eq!(mask_in(&there, &mask), mask_in(&here, &mask));

    // As a grey picture: the core 255, nothing else 0. Edited, by a pixel made 255 and
    // one made 128, and brought back onto the image in the other store: a mask on that
    // image, its core and blend those of the picture.
    ok(whizbang(
        &[
            "store",
            "export",
            "--mask",
            &mask,
            "--out",
            &to(&file("m.png")),
        ],
        &here,
    ));
    let mut grey = image::open(file("m.png")).unwrap().into_luma8();
    assert_eq!(grey.dimensions(), (64, 48));
    assert_eq!(grey.get_pixel(10, 10).0, [255]);
    assert_eq!(grey.get_pixel(0, 0).0, [0]);
    grey.put_pixel(40, 40, image::Luma([255]));
    grey.put_pixel(41, 40, image::Luma([128]));
    grey.save(file("edited.png")).unwrap();
    let made = ok(whizbang(
        &[
            "store",
            "import",
            "--mask",
            &to(&file("edited.png")),
            "--onto",
            &image,
        ],
        &there,
    ));
    let edited = mask_in(&there, &imported(&made, "mask"));
    let original = mask_in(&here, &mask);
    assert_eq!(edited.image(), original.image());
    let at = |x: usize, y: usize| y * 64 + x;
    for (index, (core, was)) in edited.core().iter().zip(original.core()).enumerate() {
        let expected = if index == at(40, 40) { 255 } else { *was };
        assert_eq!(*core, expected, "{index}");
    }
    assert_eq!(edited.blend()[at(41, 40)], 128);
    assert_eq!(edited.blend().iter().filter(|b| **b != 0).count(), 1);
    assert!(edited.protect().iter().all(|p| *p == 0));
    let origin = edited.origin().steps();
    assert_eq!(origin[0].action, "import-mask");

    // A picture of colour, or of another size, is no mask, and nothing is committed.
    image::RgbImage::from_pixel(64, 48, image::Rgb([200, 10, 10]))
        .save(file("red.png"))
        .unwrap();
    let red = [
        "store",
        "import",
        "--mask",
        &to(&file("red.png")),
        "--onto",
        &image,
    ];
    refused(&whizbang(&red, &there), "is not grey and opaque");
    image::GrayImage::new(48, 64)
        .save(file("turned.png"))
        .unwrap();
    let turned = [
        "store",
        "import",
        "--mask",
        &to(&file("turned.png")),
        "--onto",
        &image,
    ];
    refused(
        &whizbang(&turned, &there),
        "a mask is its image's size exactly",
    );
}

#[test]
fn a_request_id_of_a_dot_or_two_names_no_file_by_it() {
    // Runs under the request ids `.` and `..`, whose outputs are exported by identity:
    // each file is the path the command was given, and the folder around holds nothing
    // else; a path that names no file, `.` or `..` itself, is refused and writes nothing.
    let dir = Scratch::new("export-dots");
    let (root, _, mask) = with_mask(&dir, "store");
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let mut written = Vec::new();
    for (at, id) in [".", ".."].into_iter().enumerate() {
        let grown = ok(whizbang(
            &[
                "rascal",
                "grow",
                "--mask",
                &mask,
                "--px",
                &format!("{}", at + 1),
                "--request-id",
                id,
            ],
            &root,
        ));
        let made = (grown.lines())
            .find_map(|line| line.strip_prefix("mask "))
            .map(|rest| rest.split(' ').next().unwrap().to_owned())
            .unwrap_or_else(|| panic!("{grown}"));
        let name = format!("grown-{at}.mask");
        let to = out.join(&name);
        ok(whizbang(
            &["store", "export", &made, "--to", to.to_str().unwrap()],
            &root,
        ));
        written.push(name);
        for named in [".", ".."] {
            let to = out.join(named);
            let ran = whizbang(
                &["store", "export", &made, "--to", to.to_str().unwrap()],
                &root,
            );
            assert_eq!(ran.code, Some(1), "{named}: {}", ran.out);
        }
    }
    let mut held: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    held.sort();
    assert_eq!(held, written);
}

#[test]
fn an_export_by_identity_writes_no_file_through_a_link_inside_the_store_or_into_a_missing_folder() {
    let dir = Scratch::new("export-where");
    let (root, _, mask) = with_mask(&dir, "store");
    let export = |to: &Path| {
        let to = to.to_str().unwrap();
        whizbang(&["store", "export", &mask, "--to", to], &root)
    };
    refused(&export(&root.join("m.mask")), "is inside the store");
    refused(
        &export(&dir.path().join("missing").join("m.mask")),
        "cannot use the folder",
    );
    assert!(!dir.path().join("missing").exists());
    #[cfg(unix)]
    {
        let target = dir.path().join("target");
        std::fs::write(&target, b"kept").unwrap();
        let link = dir.path().join("link.mask");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        refused(&export(&link), "is a link");
        let overwrite = whizbang(
            &[
                "store",
                "export",
                &mask,
                "--to",
                link.to_str().unwrap(),
                "--overwrite",
            ],
            &root,
        );
        refused(&overwrite, "is a link");
        assert_eq!(std::fs::read(&target).unwrap(), b"kept");
    }
    // A file that is there is kept unless told.
    let there = dir.path().join("there.mask");
    std::fs::write(&there, b"kept").unwrap();
    refused(&export(&there), "is there already");
    assert_eq!(std::fs::read(&there).unwrap(), b"kept");
}
