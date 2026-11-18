//! `prepare::…`: the picture each judge of a service sends (plan m2c, task P1, item 3),
//! through the binary's wiring and a server on loopback: fitted to its model's long edge
//! by the one encoder (`Piece::prepared`), and the run's record naming that preparation
//! in the version of the model that answered (carry-over row on what an adapter said of
//! itself).

use whizbang_core::contract::{Call, Executor, Value};
use whizbang_core::egress::fitted;

use crate::anthropic::{ANTHROPIC, GOOGLE, Judged, OPENAI, record_of, said_by, serving};

/// The base64 of RFC 4648, read back.
fn unbase64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("not base64"),
    };
    let bytes: Vec<u8> = text.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::new();
    for group in bytes.chunks(4) {
        let all = group.iter().enumerate().fold(0u32, |all, (at, &c)| {
            all | u32::from(value(c)) << (18 - 6 * at)
        });
        out.extend_from_slice(&all.to_be_bytes()[1..group.len()]);
    }
    out
}

/// A picture wider than every model's long edge, brought into the world's store by the
/// real binary: its identity.
fn wide(judged: &Judged) -> String {
    let (width, height) = (2700u32, 30u32);
    let rgb: Vec<u8> = (0..width * height)
        .flat_map(|at| [(at % 241) as u8, (at / 97) as u8, 60])
        .collect();
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let file = scratch.join("wide.png");
    image::RgbImage::from_raw(width, height, rgb)
        .unwrap()
        .save_with_format(&file, image::ImageFormat::Png)
        .unwrap();
    let imported = crate::wired::whizbang(
        &["store", "import", "--picture", file.to_str().unwrap()],
        &judged.world.root,
    );
    assert_eq!(imported.code, Some(0), "{}", imported.err);
    let last = imported.out.lines().last().unwrap();
    last.strip_prefix("imported image ").unwrap().to_owned()
}

#[test]
fn each_models_picture_is_fitted_to_its_long_edge_and_the_run_names_the_preparation() {
    for (provider, edge) in [(ANTHROPIC, 2576), (GOOGLE, 1536), (OPENAI, 2048)] {
        let judged = Judged::of(&format!("prepare-{}", provider.name), provider);
        let answer = said_by(provider, "pass\nThe two agree.", (900, 0, 20));
        let (listener, log) = serving(&judged.pki, vec![answer]);
        let origin = format!("https://127.0.0.1:{}", listener.port());
        let over = judged.over_with(
            &judged.address(listener.port()),
            Some(&origin),
            &["prompts", "source_images"],
            "",
        );
        let picture = wide(&judged);
        let executor = judged.runner(&over);
        let call = Call::new("check")
            .input(
                "images",
                picture
                    .parse::<whizbang_core::identity::Digest>()
                    .map(whizbang_core::artifact::ArtifactId)
                    .unwrap(),
            )
            .argument("tools", Value::text(""));
        let ran = executor.submit(&call).unwrap();
        assert_eq!(ran.outcome().name(), "completed", "{}", provider.name);
        drop(executor);
        assert_eq!(listener.finish(), 1);
        // The one picture sent is a JPEG fitted to the model's long edge: its base64 is
        // the one text of the body that starts as a JPEG's does.
        let body = String::from_utf8(log.lock().unwrap()[0].1.clone()).unwrap();
        let (_, from) = body.split_once("/9j/").unwrap();
        let data = format!("/9j/{}", from.split('"').next().unwrap());
        assert!(!from.contains("/9j/"), "{}: one picture", provider.name);
        let sent = image::load_from_memory(&unbase64(&data)).unwrap();
        assert_eq!(
            (sent.width(), sent.height()),
            fitted(2700, 30, edge),
            "{}",
            provider.name
        );
        // The run names the model that answered, and the preparation in its version.
        let record = record_of(&judged.world.root, ran.run().as_str());
        let model = &record["models"][0];
        assert_eq!(model["model"], provider.model);
        assert_eq!(
            model["version"],
            format!("1+fit1-jpeg90-{edge}"),
            "{}",
            provider.name
        );
    }
}
