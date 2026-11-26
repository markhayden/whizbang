//! `pairs_page::…`: what the pairs page (`whizbang lab pairs`) gives an inspecting picker,
//! and what its keys do (plan m4, task A1, item 9; plan mE, task P0, items 1 and 2).

use crate::cli::{chunks, pixels_of};
use crate::lab::{FILLED_WITH_AND_SOURCE, ThreeImages};

/// Each chunk of a PNG, as its type and the length of its data.
fn lengths(png: &[u8]) -> Vec<(String, u32)> {
    let names = chunks(png);
    let mut at = 8;
    (names.into_iter())
        .map(|name| {
            let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
            at += 12 + length as usize;
            (name, length)
        })
        .collect()
}

#[test]
fn a_pictures_bytes_tell_only_its_dimensions() {
    // A reference arm hands back the source, and a compressing encoder gave that arm's
    // file the source's size to the byte; a chunk of padding then hid the size, but its own
    // length told what the file had lacked (plan mE, task P0, item 1). Now every picture
    // is written uncompressed: its size and each chunk's length follow from its width and
    // height alone, so any two pictures of one size are files of one shape.
    use whizbang::pairs_page::{stored_png, stored_png_size};
    use whizbang_core::identity::encode::{Channels, png_of};
    let lab = ThreeImages::new("pairs-sizes", FILLED_WITH_AND_SOURCE);
    let over = lab.over(0);
    lab.run(&over, None, &|| false).unwrap();
    let out = lab.corpus.join("pairs");
    lab.pairs(&over, &out).unwrap();
    let assignment: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("assignment.json")).unwrap()).unwrap();
    let pairs = assignment["pair"].as_array().unwrap();
    assert_eq!(pairs.len(), 3);
    let read = |name: &str| std::fs::read(out.join("pictures").join(name)).ok();
    let compressed = |file: &[u8]| {
        let pixels = pixels_of(file);
        png_of(
            pixels.rgba(),
            pixels.width(),
            pixels.height(),
            Channels::Rgba,
        )
        .unwrap()
    };
    let (mut told, mut focused) = (0, 0);
    for n in 1..=pairs.len() {
        for kind in ["", "-focus"] {
            let files: Vec<Vec<u8>> = ["left", "right", "source"]
                .into_iter()
                .filter_map(|side| read(&format!("{n:03}-{side}{kind}.png")))
                .collect();
            if kind == "-focus" && files.is_empty() {
                continue;
            }
            focused += usize::from(kind == "-focus");
            assert_eq!(files.len(), 3, "pair {n}{kind}");
            for file in &files {
                let pixels = pixels_of(file);
                let (width, height) = (pixels.width(), pixels.height());
                // The file is what its dimensions give, to the byte, and holds no chunk
                // but the picture's own: the encoder's own file of an empty picture of
                // that size has the same chunks of the same lengths.
                assert_eq!(file.len() as u64, stored_png_size(width, height));
                let blank = vec![0; pixels.rgba().len()];
                let blank = stored_png(&blank, width, height).unwrap();
                assert_eq!(lengths(file), lengths(&blank), "pair {n}{kind}");
                let names: Vec<String> = lengths(file).into_iter().map(|(n, _)| n).collect();
                assert_eq!(names, ["IHDR", "IDAT", "IEND"]);
            }
            // A compressing encoder would have told the reference arm by its size.
            let before: Vec<usize> = files.iter().map(|file| compressed(file).len()).collect();
            if before[0] != before[1] {
                told += 1;
                assert!(
                    before[0] == before[2] || before[1] == before[2],
                    "{before:?}"
                );
            }
        }
    }
    assert!(
        told > 0,
        "the pairs of this round would have told their arms by size"
    );
    assert!(focused > 0, "a pair of this round has a focus");
    // Past one stored block of deflate, as a real picture is: two pictures of one size
    // and different pixels are files of one length and one shape, which a reader decodes.
    let (width, height) = (300, 120);
    let noise: Vec<u8> = (0..width * height * 4)
        .map(|i| (i * 7919 % 251) as u8)
        .collect();
    let flat = vec![200; noise.len()];
    let (a, b) = (
        stored_png(&noise, width, height).unwrap(),
        stored_png(&flat, width, height).unwrap(),
    );
    assert_eq!(a.len() as u64, stored_png_size(width, height));
    assert_eq!(lengths(&a), lengths(&b));
    assert_eq!(pixels_of(&a).rgba(), noise.as_slice());
    assert_eq!(pixels_of(&b).rgba(), flat.as_slice());
}

#[test]
fn pairs_drawn_from_runs_are_pictures_whose_bytes_tell_only_their_dimensions() {
    // `lab pairs --from-runs` (plan mE, task M2) writes through the same function as a
    // round of the lab: a run of `remove_prompt` against the picture it was given, the
    // reference that hands the source back, so its side is the source to the pixel.
    use whizbang::pairs_page::stored_png_size;
    let recorded = crate::edits_gate::recorded_of("pairs-from-runs-sizes", false);
    let assignment = recorded.pairs(&recorded.remove_cases, None, "pairs");
    let pictures = assignment.with_file_name("pictures");
    let mut names: Vec<String> = (std::fs::read_dir(&pictures).unwrap())
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(names.contains(&"001-left.png".to_owned()), "{names:?}");
    for name in &names {
        let file = std::fs::read(pictures.join(name)).unwrap();
        let pixels = pixels_of(&file);
        assert_eq!(
            file.len() as u64,
            stored_png_size(pixels.width(), pixels.height()),
            "{name}"
        );
        let found: Vec<String> = lengths(&file).into_iter().map(|(n, _)| n).collect();
        assert_eq!(found, ["IHDR", "IDAT", "IEND"], "{name}");
    }
    // The three whole pictures share their dimensions, so they are files of one length.
    let sizes: Vec<usize> = ["left", "right", "source"]
        .iter()
        .map(|side| {
            std::fs::read(pictures.join(format!("001-{side}.png")))
                .unwrap()
                .len()
        })
        .collect();
    assert!(sizes.iter().all(|s| *s == sizes[0]), "{sizes:?}");
}

/// The page's own script, as the page written for these pair ids holds it, and its data.
fn script_and_data(ids: &[&str]) -> (String, String) {
    let pairs: Vec<whizbang::pairs_page::OnPage> = (ids.iter().enumerate())
        .map(|(n, id)| whizbang::pairs_page::OnPage {
            id: (*id).to_owned(),
            file: format!("{:03}", n + 1),
            focus: false,
            note: String::new(),
        })
        .collect();
    let page = whizbang::pairs_page::page("lab", "", "", "sha256:abc", &pairs);
    let between = |open: &str| {
        let start = page.find(open).unwrap() + open.len();
        let end = start + page[start..].find("</script>").unwrap();
        page[start..end].to_owned()
    };
    (between("<script>"), between("id=\"data\">"))
}

/// A page to run the script on with no browser: only what the script touches, and a way to
/// press keys and read back the picks it would save and the pair it shows. Printed as one
/// line of JSON for each step.
const HARNESS: &str = r#"
"use strict";
const fs = require("fs");
const [code, data] = [fs.readFileSync(process.argv[2], "utf8"), fs.readFileSync(process.argv[3], "utf8")];
const handlers = {};
let lastBlob = "";
function element() {
  const classes = new Set();
  return {
    src: "", className: "", textContent: "", disabled: false, href: "", title: "", type: "",
    style: {}, dataset: {}, files: [], firstChild: { textContent: "" }, children: [],
    scrollLeft: 0, scrollTop: 0, scrollWidth: 0, scrollHeight: 0, clientWidth: 0, clientHeight: 0,
    classList: { toggle(name, on) { if (on) { classes.add(name); } else { classes.delete(name); } } },
    addEventListener(type, handler) { this.handlers = this.handlers || {}; this.handlers[type] = handler; },
    click() {},
    appendChild(child) { this.children.push(child); },
    querySelectorAll() { return this.children; },
  };
}
const elements = {};
const buttons = ["left", "right", "same", "neither"].map((pick) => { const b = element(); b.dataset.pick = pick; return b; });
global.document = {
  getElementById(id) {
    if (!elements[id]) { elements[id] = element(); if (id === "data") { elements[id].textContent = data; } }
    return elements[id];
  },
  querySelectorAll(what) { return what === ".picks button" ? buttons : []; },
  createElement() { return element(); },
  addEventListener(type, handler) { handlers[type] = handler; },
};
global.window = { addEventListener() {} };
global.Blob = class { constructor(parts) { this.text = parts.join(""); } };
global.URL = { createObjectURL(blob) { lastBlob = blob.text; return "blob:x"; }, revokeObjectURL() {} };
global.Image = class {};
global.FileReader = class { readAsText(file) { this.result = file.text; this.onload(); } };
(0, eval)(code);
function state(step) {
  const saved = JSON.parse(lastBlob);
  const picks = {};
  for (const p of saved.pick) { picks[p.pair] = p.pick; }
  console.log(JSON.stringify({ step, where: elements.where.textContent, picks }));
}
state("opened");
for (const step of process.argv.slice(4)) {
  if (step === "Load") {
    // A saved picks file of this page, opened on it: b was picked right.
    const text = JSON.stringify({ picks_version: 1, assignment_digest: "sha256:abc", pick: [{ pair: "b", pick: "right" }] });
    elements.load.handlers.change({ target: { files: [{ text }] } });
  } else {
    const [key, held] = step.split("+");
    handlers.keydown({ key: key === "Space" ? " " : key, preventDefault() {}, repeat: held === "held" });
  }
  state(step);
}
"#;

#[test]
fn a_backspace_takes_back_the_last_pick() {
    let (code, data) = script_and_data(&["a", "b", "c"]);
    // What the page's text says, wherever the test runs: the key is said to take a pick
    // back, and the key's handler is the one function that does.
    let page = whizbang::pairs_page::page("lab", "", "", "sha256:abc", &[]);
    assert!(
        page.contains("<kbd>backspace</kbd> take back the last pick"),
        "the keys the page lists"
    );
    assert!(
        code.contains(r#"if (e.key === "Backspace") { e.preventDefault(); if (!e.repeat) { takeBack(); } return; }"#),
        "{code}"
    );
    // And what the script does, run as it is under `node` with only what it touches of a
    // page. Node is among the pinned tools `just check` needs (`scripts/tool-versions.txt`),
    // so a machine without one fails here (plan mE, task P0, item 2).
    let dir = whizbang_testkit::scratch::Scratch::new("pairs-backspace");
    let file = |name: &str, text: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    };
    let keys = [
        // Two picks, each moving on to the next open pair.
        "ArrowLeft",
        "ArrowRight",
        // The last pick is taken back, and the page returns to its pair; then the one
        // before it.
        "Backspace",
        "Backspace",
        // With none left to take back, the key goes to the pair before, as it did.
        "Backspace",
        // A pick made over another: taken back, the earlier pick is there again.
        "ArrowUp",
        "Space",
        "Space",
        "ArrowDown",
        "Backspace",
        // A key held down repeats: the repeats take nothing back.
        "Backspace+held",
        // A saved picks file is opened: its picks, and those it leaves, are no longer
        // this page's to take back, so the key goes to the pair before and removes none.
        "Load",
        "Backspace",
    ];
    let ran = std::process::Command::new("node")
        .arg(file("harness.js", HARNESS))
        .arg(file("page.js", &code))
        .arg(file("data.json", &data))
        .args(keys)
        .output();
    let ran = ran.unwrap_or_else(|error| {
        panic!(
            "node, which runs the page's script, cannot be started ({error}); it is a pinned \
             tool: `just setup` names the command that installs it"
        )
    });
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let steps: Vec<serde_json::Value> = (String::from_utf8(ran.stdout).unwrap().lines())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let json = |text: &str| -> serde_json::Value { serde_json::from_str(text).unwrap() };
    let want = [
        ("opened", "Pair 1 of 3: a", "{}"),
        ("ArrowLeft", "Pair 2 of 3: b", r#"{"a":"left"}"#),
        (
            "ArrowRight",
            "Pair 3 of 3: c",
            r#"{"a":"left","b":"right"}"#,
        ),
        ("Backspace", "Pair 2 of 3: b", r#"{"a":"left"}"#),
        ("Backspace", "Pair 1 of 3: a", "{}"),
        ("Backspace", "Pair 3 of 3: c", "{}"),
        ("ArrowUp", "Pair 1 of 3: a", r#"{"c":"same"}"#),
        ("Space", "Pair 2 of 3: b", r#"{"c":"same"}"#),
        ("Space", "Pair 3 of 3: c", r#"{"c":"same"}"#),
        ("ArrowDown", "Pair 1 of 3: a", r#"{"c":"neither"}"#),
        ("Backspace", "Pair 3 of 3: c", r#"{"c":"same"}"#),
        ("Backspace+held", "Pair 3 of 3: c", r#"{"c":"same"}"#),
        ("Load", "Pair 1 of 3: a", r#"{"b":"right","c":"same"}"#),
        ("Backspace", "Pair 3 of 3: c", r#"{"b":"right","c":"same"}"#),
    ];
    assert_eq!(steps.len(), want.len(), "{steps:?}");
    for (step, (key, where_, picks)) in steps.iter().zip(want) {
        assert_eq!(step["step"], key, "{step}");
        assert_eq!(step["where"], where_, "after {key}: {step}");
        assert_eq!(step["picks"], json(picks), "after {key}: {step}");
    }
}
