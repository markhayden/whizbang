//! `pairs_page::…`: what the pairs page (`whizbang lab pairs`) gives an inspecting picker,
//! and what its keys do (plan m4, task A1, item 9).

use crate::cli::{chunks, pixels_of};
use crate::lab::{FILLED_WITH_AND_SOURCE, ThreeImages};

#[test]
fn a_pictures_size_on_disk_does_not_tell_its_arm() {
    // A reference arm hands back the source, and a picture is written from its pixels by
    // one encoder: so that arm's file had the source's size to the byte, and the other
    // arm's had another. Now the three pictures of a pair are one size on disk, and so
    // are the three of its focus: a size says nothing of a side.
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
    // What the encoder alone gives a picture: its size before this change.
    let plain = |file: &[u8]| {
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
            // One size on disk, each a whole PNG that holds its pixels and, beside them,
            // one chunk of padding a reader passes over.
            let sizes: Vec<usize> = files.iter().map(Vec::len).collect();
            assert!(sizes.iter().all(|size| *size == sizes[0]), "{sizes:?}");
            for file in &files {
                let found = chunks(file);
                assert_eq!(found.first().map(String::as_str), Some("IHDR"));
                assert_eq!(found.last().map(String::as_str), Some("IEND"));
                assert_eq!(found.iter().filter(|c| *c == "paDd").count(), 1);
                assert!(
                    found
                        .iter()
                        .all(|c| ["IHDR", "IDAT", "paDd", "IEND"].contains(&c.as_str())),
                    "{found:?}"
                );
                // The picture is the picture: the padding changes no pixel.
                assert_eq!(pixels_of(file).rgba(), pixels_of(&plain(file)).rgba());
            }
            // Without the padding the sizes differed, and the side whose size was the
            // source's was the reference arm's.
            let before: Vec<usize> = files.iter().map(|file| plain(file).len()).collect();
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
    // page, where there is a `node`: every hosted runner has one, and the owner's Mac.
    // No JavaScript engine is among the pinned tools, so a machine without one holds the
    // page to its text alone, and says so.
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
    let ran = match ran {
        Ok(ran) => ran,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("no `node` here: the page's script was held to its text, and not run");
            return;
        }
        Err(error) => panic!("node: {error}"),
    };
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
