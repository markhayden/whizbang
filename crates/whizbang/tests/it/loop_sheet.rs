//! `loop_sheet::…`: `whizbang eval loop sheet` (plan e1, task L1, item 6), the blind
//! contact sheet of the loop's fills, over a run of the loop made with its test kit
//! ([`crate::loop_score::scripted`]).

use whizbang::loop_sheet::{self, Asked, OnSheet};
use whizbang_testkit::scratch::Scratch;

use crate::checked::{passes, remains};
use crate::loop_score::{ITEM, JUDGE, scripted, set_of};

#[test]
fn refuses_a_folder_inside_a_work_tree() {
    let dir = Scratch::new("sheet-in-a-tree");
    let tree = dir.path().join("repo");
    std::fs::create_dir_all(tree.join(".git")).unwrap();
    let (manifest, folder) = set_of(dir.path(), "a mug");
    let store = dir.path().join("store");
    for out in [tree.clone(), tree.join("sheet"), tree.join("deep/er/sheet")] {
        let asked = Asked {
            manifest: &manifest,
            set: &folder,
            root: &store,
            judge: JUDGE,
            out: &out,
        };
        let refused = loop_sheet::sheet(&asked).unwrap_err();
        assert!(refused.contains("inside a git work tree"), "{refused}");
    }
    // Nothing was written, nor a folder made.
    let left: Vec<_> = std::fs::read_dir(&tree).unwrap().collect();
    assert_eq!(left.len(), 1);
}

#[test]
fn the_page_shows_no_verdict_coverage_or_attempt() {
    let scripted = scripted("sheet-blind", vec![remains(), passes()]);
    let out = scripted.dir.path().join("sheet");
    let asked = Asked {
        manifest: &scripted.manifest,
        set: &scripted.folder,
        root: &scripted.world.root,
        judge: JUDGE,
        out: &out,
    };
    let written = loop_sheet::sheet(&asked).unwrap();
    // Both fills are on it, each beside the picture the loop was given.
    assert_eq!(written.pictures, 2);
    let mut files: Vec<String> = std::fs::read_dir(out.join("pictures"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "001-edited.png",
            "001-source.png",
            "002-edited.png",
            "002-source.png"
        ]
    );
    let page = std::fs::read_to_string(out.join("index.html")).unwrap();
    for word in [
        "verdict",
        "coverage",
        "attempt",
        "judge",
        JUDGE,
        "RQ1",
        "unsure",
        "retry",
        "kept",
        ITEM,
        scripted.run.as_str(),
        "MARMALADE",
    ] {
        assert!(
            !page.to_lowercase().contains(&word.to_lowercase()),
            "{word}"
        );
    }
    assert!(page.contains(crate::checked::PHRASE));
    // The page lets nothing in but its own script and style, by digest.
    assert!(page.contains("default-src &#39;none&#39;"));
    assert!(page.contains(&whizbang::pairs_page::escape(&loop_sheet::policy())));
    // The assignment beside it is what maps a picture to its run and attempt.
    let assignment: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("assignment.json")).unwrap())
            .unwrap();
    let pictures = assignment["pictures"].as_array().unwrap();
    let mut attempts: Vec<i64> = pictures
        .iter()
        .map(|p| p["attempt"].as_i64().unwrap())
        .collect();
    attempts.sort_unstable();
    assert_eq!(attempts, [1, 2]);
    for p in pictures {
        assert_eq!(p["run"], scripted.run.as_str());
        let key = whizbang::loop_score::key(
            assignment["seed"].as_u64().unwrap(),
            &scripted.run,
            p["attempt"].as_u64().unwrap() as u32,
        );
        assert_eq!(p["key"], key.as_str());
        assert!(page.contains(&key));
    }
    // The seed is in the assignment alone; the page names the sheet by its id.
    let seed = assignment["seed"].as_u64().unwrap();
    assert!(!page.contains(&seed.to_string()));
    assert!(page.contains(&whizbang::loop_score::sheet_id(seed)));
    {}
}

#[test]
fn every_string_is_escaped() {
    let hostile = "</script><img src=x onerror=alert(1)> & \"quoted\" 'single'";
    let page = loop_sheet::page(
        &format!("summary {hostile}"),
        "0123456789abcdef",
        &[OnSheet {
            file: "001".to_owned(),
            key: "0123456789abcdef".to_owned(),
            phrase: hostile.to_owned(),
        }],
    );
    assert!(!page.contains(hostile));
    assert!(!page.contains("<img src=x"));
    // In the text: as HTML escapes; in the data: as JSON escapes of `<`, `>` and `&`.
    assert!(page.contains("summary &lt;/script&gt;&lt;img src=x onerror=alert(1)&gt; &amp; &quot;quoted&quot; &#39;single&#39;"));
    assert!(page.contains("\\u003c/script\\u003e\\u003cimg src=x onerror=alert(1)\\u003e \\u0026"));
    // One script of data and one of the page's own, and no other.
    assert_eq!(page.matches("<script").count(), 2);
    assert_eq!(page.matches("</script>").count(), 2);
    // The order is drawn from the seed, the same each time, and every place once.
    let order = loop_sheet::order(40, 20_261_121);
    assert_eq!(order, loop_sheet::order(40, 20_261_121));
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..40).collect::<Vec<_>>());
    assert_ne!(order, sorted);
}

fn sheet_of(
    scripted: &crate::loop_score::Scripted,
    out: &std::path::Path,
) -> Result<loop_sheet::Written, String> {
    loop_sheet::sheet(&Asked {
        manifest: &scripted.manifest,
        set: &scripted.folder,
        root: &scripted.world.root,
        judge: JUDGE,
        out,
    })
}

fn assignment_of(out: &std::path::Path) -> whizbang::loop_score::Assignment {
    serde_json::from_slice(&std::fs::read(out.join("assignment.json")).unwrap()).unwrap()
}

#[test]
fn two_sheets_of_one_store_are_drawn_in_different_orders() {
    let scripted = scripted("sheet-two", vec![remains(), passes()]);
    let (one, two) = (
        scripted.dir.path().join("one"),
        scripted.dir.path().join("two"),
    );
    sheet_of(&scripted, &one).unwrap();
    sheet_of(&scripted, &two).unwrap();
    let (a, b) = (assignment_of(&one), assignment_of(&two));
    // Each sheet's seed is its own, drawn when it was written, so its order and its keys
    // cannot be worked out from another's, nor from anything committed.
    assert_ne!(a.seed, b.seed);
    assert_ne!(loop_sheet::order(20, a.seed), loop_sheet::order(20, b.seed));
    assert_ne!(a.pictures[0].key, b.pictures[0].key);
    let page = std::fs::read_to_string(one.join("index.html")).unwrap();
    for p in &b.pictures {
        assert!(!page.contains(&p.key));
    }
}

#[test]
fn refuses_a_folder_that_is_not_empty() {
    let scripted = scripted("sheet-full", vec![passes()]);
    let out = scripted.dir.path().join("sheet");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("notes.txt"), "kept").unwrap();
    let refused = sheet_of(&scripted, &out).unwrap_err();
    assert!(refused.contains("not empty"), "{refused}");
    let left: Vec<_> = std::fs::read_dir(&out).unwrap().collect();
    assert_eq!(left.len(), 1);
    // An empty folder is taken, and a second sheet into it, now full, is refused.
    let empty = scripted.dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    sheet_of(&scripted, &empty).unwrap();
    assert!(
        sheet_of(&scripted, &empty)
            .unwrap_err()
            .contains("not empty")
    );
}

#[test]
fn a_path_through_dot_dot_into_a_work_tree_is_refused() {
    let dir = Scratch::new("sheet-dot-dot");
    let tree = dir.path().join("repo");
    std::fs::create_dir_all(tree.join(".git")).unwrap();
    // A part that is not there, then `..`, then the work tree: the folder is in the tree.
    let through = dir.path().join("gone/../repo/sheet");
    assert!(whizbang::edits::inside_a_work_tree(&through));
    assert!(whizbang::edits::inside_a_work_tree(
        &dir.path().join("repo/a/../b")
    ));
    assert!(!whizbang::edits::inside_a_work_tree(
        &dir.path().join("repo/../elsewhere")
    ));
    let (manifest, folder) = set_of(dir.path(), "a mug");
    let store = dir.path().join("store");
    let refused = loop_sheet::sheet(&Asked {
        manifest: &manifest,
        set: &folder,
        root: &store,
        judge: JUDGE,
        out: &through,
    })
    .unwrap_err();
    assert!(refused.contains("inside a git work tree"), "{refused}");
    assert!(!tree.join("sheet").exists());
    // `eval edits draw` shares the check.
    let ran = crate::wired::whizbang_storeless(&[
        "eval",
        "edits",
        "draw",
        "--pictures",
        folder.to_str().unwrap(),
        "--out",
        through.to_str().unwrap(),
    ]);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(ran.err.contains("inside a git work tree"), "{}", ran.err);
}
