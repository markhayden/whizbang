//! `whizbang rascal select --run`, `--pick` and `--at` (plan 1D, task Q1): one instance kept
//! out of the several a `segment` run answered, so that a cut-out is three commands:
//! segment, select, render. The run is made through the binary's wiring and door over the
//! fake segmenter of `cli.rs`; `select` is the real binary.

use whizbang::door::SegmentAsked;
use whizbang_testkit::scratch::Scratch;

use crate::cli::{segment, with_picture};
use crate::wired::whizbang;

/// A segment run of a point on the fixture: its id and its three masks, in the order it
/// printed them (around the disc, its upper half, and the square).
fn segmented(dir: &Scratch, root: &std::path::Path, image: &str) -> (String, Vec<String>) {
    let asked = SegmentAsked {
        image,
        kind: "point",
        query: "420 200",
        max_instances: 8,
        backend: None,
        request_id: None,
    };
    let out = segment(dir, root, &asked).unwrap();
    let run = out.lines().next().unwrap().strip_prefix("run ").unwrap();
    let run = run.strip_suffix(" completed").unwrap().to_owned();
    let masks = out
        .lines()
        .filter_map(|line| line.strip_prefix("mask "))
        .map(|rest| rest.split(' ').next().unwrap().to_owned())
        .collect();
    (run, masks)
}

/// The run of a `select`, and the masks it took as inputs, by `runs show`.
fn selected(root: &std::path::Path, args: &[&str]) -> (String, Vec<String>) {
    let mut all = vec!["rascal", "select"];
    all.extend_from_slice(args);
    let ran = whizbang(&all, root);
    assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
    let run = ran
        .out
        .lines()
        .next()
        .unwrap()
        .strip_prefix("run ")
        .unwrap();
    let run = run.strip_suffix(" completed").unwrap().to_owned();
    assert!(ran.out.lines().any(|line| line.starts_with("mask sha256:")));
    let json = whizbang(&["runs", "show", &run, "--json"], root);
    assert_eq!(json.code, Some(0), "{}", json.err);
    let record: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    let inputs = record["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| input["artifact"].as_str().unwrap().to_owned())
        .collect();
    (run, inputs)
}

#[test]
fn the_run_and_pick_keep_the_nth_mask_of_a_segment_run() {
    let dir = Scratch::new("select-pick");
    let (root, image) = with_picture(&dir);
    let (run, masks) = segmented(&dir, &root, &image);
    assert_eq!(masks.len(), 3);
    // The second, by its place in what the run printed.
    let (_, inputs) = selected(&root, &["--run", &run, "--pick", "2"]);
    assert_eq!(inputs, [masks[1].clone()]);
    // Two picks keep both, in the order given.
    let (_, inputs) = selected(&root, &["--run", &run, "--pick", "3", "--pick", "1"]);
    assert_eq!(inputs, [masks[2].clone(), masks[0].clone()]);
    // No pick keeps every mask of the run.
    let (_, inputs) = selected(&root, &["--run", &run]);
    assert_eq!(inputs, masks);
    // A pick needs a run, and a run takes no --mask beside it.
    for args in [
        &[
            "rascal",
            "select",
            "--mask",
            masks[0].as_str(),
            "--pick",
            "1",
        ][..],
        &[
            "rascal",
            "select",
            "--run",
            run.as_str(),
            "--mask",
            masks[0].as_str(),
        ][..],
        &["rascal", "select"][..],
        &["rascal", "select", "--run", run.as_str(), "--pick", "0"][..],
    ] {
        let ran = whizbang(args, &root);
        assert_eq!(ran.code, Some(2), "{args:?}: {}", ran.err);
    }
    // A run that is not in the store, or not of segment, is refused before anything runs.
    let (select_run, _) = selected(&root, &["--run", &run, "--pick", "1"]);
    let before = whizbang(&["runs", "list"], &root).out;
    let ran = whizbang(&["rascal", "select", "--run", "1-1"], &root);
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err.contains("--run names no run in the store"),
        "{}",
        ran.err
    );
    let ran = whizbang(&["rascal", "select", "--run", &select_run], &root);
    assert_eq!(ran.code, Some(1));
    assert!(ran.err.contains("a run of another action"), "{}", ran.err);
    assert_eq!(whizbang(&["runs", "list"], &root).out, before);
}

/// The mask a `select` printed.
fn printed_mask(root: &std::path::Path, args: &[&str]) -> String {
    let mut all = vec!["rascal", "select"];
    all.extend_from_slice(args);
    let ran = whizbang(&all, root);
    assert_eq!(ran.code, Some(0), "{args:?}: {}", ran.err);
    let mask = ran.out.lines().find_map(|line| line.strip_prefix("mask "));
    mask.unwrap().to_owned()
}

#[test]
fn at_keeps_the_masks_whose_core_holds_the_pixel() {
    let dir = Scratch::new("select-at");
    let (root, image) = with_picture(&dir);
    let (run, masks) = segmented(&dir, &root, &image);
    // A mask's identity is of what it holds: the union of the masks kept by --at is the
    // union of the same masks named. The disc's lower half is in the first box alone;
    // its upper half in the first two; the square in the third alone.
    let kept = |at: &str| printed_mask(&root, &["--run", &run, "--at", at]);
    let named = |of: &[&String]| {
        let mut args = Vec::new();
        for mask in of {
            args.extend(["--mask", mask.as_str()]);
        }
        printed_mask(&root, &args)
    };
    assert_eq!(kept("420 250"), named(&[&masks[0]]));
    assert_eq!(kept("420 150"), named(&[&masks[0], &masks[1]]));
    assert_ne!(kept("420 250"), named(&[&masks[2]]));
    // With --mask as with --run.
    let square = printed_mask(
        &root,
        &[
            "--mask",
            &masks[0],
            "--mask",
            &masks[2],
            "--at",
            "100.5 300",
        ],
    );
    assert_eq!(square, named(&[&masks[2]]));
    // A place no mask holds, or one outside the image, is refused, naming the count.
    for at in ["5 5", "640 10"] {
        let ran = whizbang(&["rascal", "select", "--run", &run, "--at", at], &root);
        assert_eq!(ran.code, Some(1), "{at}");
        assert!(ran.err.contains("of the 3 masks"), "{at}: {}", ran.err);
    }
}

#[test]
fn a_pick_past_the_runs_masks_is_refused_naming_the_count() {
    let dir = Scratch::new("select-past");
    let (root, image) = with_picture(&dir);
    let (run, _) = segmented(&dir, &root, &image);
    let before = whizbang(&["runs", "list"], &root).out;
    let ran = whizbang(&["rascal", "select", "--run", &run, "--pick", "4"], &root);
    assert_eq!(ran.code, Some(1), "{}", ran.err);
    assert!(
        ran.err
            .contains("--pick 4 is past the run's masks: it has 3 (from 1 to 3)"),
        "{}",
        ran.err
    );
    // Nothing ran.
    assert_eq!(whizbang(&["runs", "list"], &root).out, before);
}
