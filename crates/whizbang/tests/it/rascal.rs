//! `whizbang rascal grow`: the first command that runs an action. It goes through the
//! executor's one entry point, takes a request id through the executor, prints nothing
//! it was given unescaped, and exits with the codes its help states.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use whizbang::door::{self, GrowAsked};
use whizbang_core::artifact::{self, Artifact, ArtifactId, ArtifactType, Mask, Source, UseCase};
use whizbang_core::artifact::{Origin, Step};
use whizbang_core::contract::{
    Action, ActionDescriptor, Context, Engine, Failure, PreparedCall, Produced, Secrets,
};
use whizbang_core::identity::{Binding, DecodeLimits, Digest, PixelDigest, Space};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_rascal::grow::Grow;
use whizbang_testkit::scratch::Scratch;

/// Runs `whizbang` and gives its exit code, what it printed and what it complained of.
fn whizbang(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_whizbang"))
        .args(args)
        .output()
        .expect("the binary runs");
    (
        out.status.code().expect("the binary exits"),
        String::from_utf8(out.stdout).expect("output is UTF-8"),
        String::from_utf8(out.stderr).expect("errors are UTF-8"),
    )
}

const WIDTH: usize = 24;
const HEIGHT: usize = 16;

/// A mask of a 24 by 16 image: a block of core in columns 8 to 11 and rows 5 to 8, and
/// column 14 protected.
pub(super) fn fixture() -> Mask {
    let core = (0..WIDTH * HEIGHT)
        .map(|at| {
            let (x, y) = (at % WIDTH, at / WIDTH);
            if (8..12).contains(&x) && (5..9).contains(&y) {
                255
            } else {
                0
            }
        })
        .collect();
    let protect: Vec<u8> = (0..WIDTH * HEIGHT)
        .map(|at| if at % WIDTH == 14 { 255 } else { 0 })
        .collect();
    let binding = Binding::root(
        PixelDigest(Digest::of_bytes(b"photo")),
        Space::flat(WIDTH as u32, HEIGHT as u32).unwrap(),
    );
    let origin = Origin::new(Step::new("fixture", "1")).unwrap();
    let mut mask =
        Mask::from_core(binding, UseCase::Remove, core, Source::default(), origin).unwrap();
    mask.protect_union(&protect).unwrap();
    mask
}

/// A folder with the fixture's canonical file in it, and a store the file was brought
/// into with `whizbang store import`. The store's path, and the mask's identity as the
/// import printed it.
fn imported(dir: &Scratch) -> (PathBuf, String) {
    let file = dir.path().join("fixture.mask");
    std::fs::write(&file, fixture().to_bytes()).unwrap();
    let store = dir.path().join("store");
    let (code, out, err) = whizbang(&[
        "store",
        "import",
        file.to_str().unwrap(),
        "--store",
        store.to_str().unwrap(),
        "--create",
    ]);
    assert_eq!(code, 0, "{err}");
    let id = out.lines().last().unwrap().strip_prefix("imported mask ");
    let id = id.expect("the identity").to_owned();
    assert_eq!(id, fixture().id().0.to_string());
    (store, id)
}

fn grow(store: &Path, args: &[&str]) -> (i32, String, String) {
    let mut all = vec!["rascal", "grow", "--store", store.to_str().unwrap()];
    all.extend(args);
    whizbang(&all)
}

fn mask_in(store: &Path, id: &str) -> Mask {
    let reader = Reader::open(store, Arc::new(RealFs)).unwrap();
    let id = ArtifactId(id.parse().expect("an identity"));
    artifact::get(&reader, ArtifactType::Mask, id, &DecodeLimits::default())
        .unwrap()
        .into_mask()
        .unwrap()
}

/// The run and the mask a successful `grow` printed.
fn printed(out: &str) -> (String, String) {
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    let run = lines[0].strip_prefix("run ").expect("the run");
    let run = run.split(' ').next().unwrap().to_owned();
    (
        run,
        lines[1].strip_prefix("mask ").expect("the mask").to_owned(),
    )
}

// --- the command ------------------------------------------------------------------------------

#[test]
fn grow_on_an_imported_fixture_commits_a_mask_whose_origin_names_its_action_arguments_and_input() {
    let dir = Scratch::new("rascal");
    let (store, input) = imported(&dir);
    let (code, out, err) = grow(&store, &["--mask", &input, "--px", "8", "--down-px", "4"]);
    assert_eq!((code, err.as_str()), (0, ""), "{out}");
    let (run, made) = printed(&out);
    assert_eq!(out, format!("run {run} completed\nmask {made}\n"));
    assert_ne!(made, input);

    let mask = mask_in(&store, &made);
    assert_eq!(
        mask.origin().steps(),
        [
            Step::new("fixture", "1"),
            Step::new("grow", "1.0.0")
                .argument("px", "8")
                .argument("down_px", "4")
                .input("mask", input.parse().unwrap()),
        ]
    );
    // Grown by 8 from columns 8 to 11: columns 0 to 19 in the block's rows, and never
    // the protected column.
    let row = |y: usize| -> Vec<bool> {
        (0..WIDTH)
            .map(|x| mask.core()[y * WIDTH + x] != 0)
            .collect()
    };
    assert_eq!(
        row(6),
        (0..WIDTH).map(|x| x < 20 && x != 14).collect::<Vec<_>>()
    );
    assert_eq!(mask.protect(), fixture().protect());

    // The run is in the store, completed, with the mask as its output.
    let (code, shown, err) = whizbang(&["runs", "show", &run, "--store", store.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    for line in [
        format!("run: {run}"),
        "status: completed".to_owned(),
        "action: grow 1.0.0".to_owned(),
    ] {
        assert!(
            shown.lines().any(|l| l == line),
            "{line:?} is not in:\n{shown}"
        );
    }
    assert!(shown.contains(&made), "{shown}");

    // The same inputs again: another run, and the same artifact identity.
    let (code, out, err) = grow(&store, &["--mask", &input, "--px", "8", "--down-px", "4"]);
    assert_eq!((code, err.as_str()), (0, ""));
    let (again, same) = printed(&out);
    assert_ne!(again, run);
    assert_eq!(same, made);
    // Written another way, the same numbers are the same call.
    let (_, out, _) = grow(&store, &["--mask", &input, "--px=8.0", "--down-px", "4e0"]);
    assert_eq!(printed(&out).1, made);
    // Without `--down-px`, and with another radius: other masks.
    let (code, out, _) = grow(&store, &["--mask", &input, "--px", "8"]);
    assert_eq!(code, 0);
    assert_ne!(printed(&out).1, made);
    let plain = mask_in(&store, &printed(&out).1);
    let step = &plain.origin().steps()[1];
    assert_eq!(step.arguments, [("px".to_owned(), "8".to_owned())]);
}

#[test]
fn a_request_id_makes_a_repeat_the_same_run_and_another_call_under_it_a_conflict() {
    let dir = Scratch::new("rascal");
    let (store, input) = imported(&dir);
    let asked = ["--mask", &input, "--px", "2.5", "--request-id", "job-7"];
    let (code, out, err) = grow(&store, &asked);
    assert_eq!((code, err.as_str()), (0, ""));
    let (run, made) = printed(&out);
    assert_eq!(out, format!("run {run} completed\nmask {made}\n"));

    // Sent again: the run it made then, and nothing runs.
    let (code, out, err) = grow(&store, &asked);
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(
        out,
        format!(
            "run {run} (the request was sent before, and nothing ran again) completed\nmask {made}\n"
        )
    );
    let (_, listed, _) = whizbang(&["runs", "list", "--store", store.to_str().unwrap()]);
    assert_eq!(listed.lines().next(), Some("runs: 1"));

    // Another call under the same id is refused, and makes no run.
    let (code, out, err) = grow(
        &store,
        &["--mask", &input, "--px", "3", "--request-id", "job-7"],
    );
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(
        err,
        format!(
            "whizbang: the request id job-7 was used for another request, which made the run \
             {run}: an id is one request's, and this one was not run\n"
        )
    );
    let (_, listed, _) = whizbang(&["runs", "list", "--store", store.to_str().unwrap()]);
    assert_eq!(listed.lines().next(), Some("runs: 1"));
}

#[test]
fn what_grow_cannot_take_is_refused_in_a_sentence_with_exit_1_and_no_run() {
    let dir = Scratch::new("rascal");
    let (store, input) = imported(&dir);
    let other = Digest::of_bytes(b"no such mask").to_string();
    for (args, said) in [
        (
            vec!["--mask", &input, "--px", "-1"],
            "not a call of grow: the argument px: -1 is outside 0 to 10000",
        ),
        (
            vec!["--mask", &input, "--px", "10000.5"],
            "the argument px: 10000.5 is outside 0 to 10000",
        ),
        (
            vec!["--mask", &input, "--px", "1e999"],
            "the argument px: inf is not a finite number",
        ),
        (
            vec!["--mask", &input, "--px", "NaN"],
            "the argument px: NaN is not a finite number",
        ),
        (
            vec!["--mask", &input, "--px=-inf"],
            "the argument px: -inf is not a finite number",
        ),
        (
            vec!["--mask", &input, "--px", "8", "--down-px", "-0.5"],
            "the argument down_px: -0.5 is outside 0 to 10000",
        ),
        (
            vec!["--mask", &input, "--px", "8", "--down-px", "nan"],
            "the argument down_px: NaN is not a finite number",
        ),
        (
            vec!["--mask", &input, "--px", "8", "--down-px", "1e9"],
            "the argument down_px: 1000000000 is outside 0 to 10000",
        ),
        (
            vec!["--mask", &other, "--px", "8"],
            "grow: the input mask: the store holds no mask sha256:",
        ),
        (
            vec!["--mask", "the-mask", "--px", "8"],
            "--mask is no artifact identity",
        ),
        (
            vec!["--mask", "sha256:ABC", "--px", "8"],
            "--mask is no artifact identity",
        ),
        (
            vec!["--mask", &input, "--px", "8", "--request-id", "two words"],
            "is no request id",
        ),
        (
            vec!["--mask", &input, "--px", "8", "--request-id", ""],
            "is no request id",
        ),
    ] {
        let (code, out, err) = grow(&store, &args);
        assert_eq!((code, out.as_str()), (1, ""), "{args:?}: {err}");
        assert!(
            err.starts_with("whizbang: ") && err.ends_with('\n'),
            "{args:?}: {err}"
        );
        assert!(err.contains(said), "{args:?}: {err}");
        assert_eq!(err.lines().count(), 1, "{args:?}: {err}");
    }
    // What was given as a mask is not said back: it could be anything.
    let (_, _, err) = grow(&store, &["--mask", "the-mask", "--px", "8"]);
    assert!(!err.contains("the-mask"), "{err}");

    // Not a number, a missing argument and no store are mistakes of usage: exit 2.
    for args in [
        vec!["--mask", &input, "--px", "wide"],
        vec!["--mask", &input],
        vec!["--px", "8"],
        vec!["--mask", &input, "--px", "8", "--method", "fast"],
    ] {
        let (code, out, _) = grow(&store, &args);
        assert_eq!((code, out.as_str()), (2, ""), "{args:?}");
    }
    let (code, _, err) = whizbang(&["rascal", "grow", "--mask", &input, "--px", "8"]);
    assert_eq!(code, 2);
    assert!(err.contains("--store"), "{err}");

    // A store that is not there is not made, and something that is not a mask is no mask.
    let nowhere = dir.path().join("nowhere");
    let (code, _, err) = grow(&nowhere, &["--mask", &input, "--px", "8"]);
    assert_eq!(code, 1, "{err}");
    assert!(!nowhere.exists());
    let line = dir.path().join("fixture.series");
    let series = whizbang_core::artifact::Series::new(
        fixture().binding().clone(),
        "sigma_px",
        "px",
        1,
        vec![0.5; HEIGHT],
        Origin::new(Step::new("feather", "1")).unwrap(),
    )
    .unwrap();
    std::fs::write(&line, Artifact::from(series.clone()).to_bytes()).unwrap();
    let (code, _, err) = whizbang(&[
        "store",
        "import",
        line.to_str().unwrap(),
        "--store",
        store.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = grow(&store, &["--mask", &series.id().0.to_string(), "--px", "8"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("a mask is required, and this is a series"),
        "{err}"
    );

    // None of it made a run.
    let (_, listed, _) = whizbang(&["runs", "list", "--store", store.to_str().unwrap()]);
    assert_eq!(listed, "runs: 0\n");
}

#[test]
fn nothing_grow_was_given_reaches_the_terminal_as_it_was() {
    let dir = Scratch::new("rascal");
    let (store, input) = imported(&dir);
    // An escape sequence, a line break and a very long text, each as a request id.
    let long = "x".repeat(5000);
    for id in ["\u{1b}[31mred", "one\ntwo", long.as_str()] {
        let (code, out, err) = grow(&store, &["--mask", &input, "--px", "8", "--request-id", id]);
        assert_eq!((code, out.as_str()), (1, ""));
        assert!(
            !err.contains('\u{1b}') && err.lines().count() == 1,
            "{err:?}"
        );
        assert!(err.len() < 400, "{} bytes", err.len());
    }
    // What the executor refused it quotes escaped, once: the door adds no second escape.
    let (_, _, err) = grow(
        &store,
        &["--mask", &input, "--px", "8", "--request-id", "one\ntwo"],
    );
    assert!(
        err.contains("one\\ntwo") && !err.contains("one\\\\ntwo"),
        "{err:?}"
    );
    // The same for what is given as a mask, which is not said back at all.
    let (code, _, err) = grow(&store, &["--mask", "\u{1b}[2J", "--px", "8"]);
    assert_eq!(code, 1);
    assert!(!err.contains('\u{1b}'), "{err:?}");
}

/// Every path under a folder, files and folders, relative to it.
fn tree(folder: &Path) -> Vec<String> {
    fn walk(base: &Path, at: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            found.push(
                path.strip_prefix(base)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
            if path.is_dir() {
                walk(base, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(folder, folder, &mut found);
    found.sort();
    found
}

#[test]
fn a_request_id_of_one_dot_or_two_is_a_name_in_the_store_and_never_a_path() {
    // `.` and `..` are request ids: the alphabet allows them. An id is a name inside
    // the generation's file, so neither moves anything on the disk: after a run under
    // each, the folder around the store holds what it held, and the store has gained
    // only files under the folders it already had, none of them named for an id.
    let dir = Scratch::new("rascal");
    let (store, input) = imported(&dir);
    let around = |dir: &Scratch| -> Vec<String> {
        let all = tree(dir.path());
        all.into_iter()
            .filter(|path| !path.starts_with("store/"))
            .collect()
    };
    let folders = |store: &Path| -> Vec<String> {
        let all = tree(store);
        all.into_iter()
            .filter(|path| store.join(path).is_dir())
            .collect()
    };
    let (before, inside) = (around(&dir), folders(&store));
    assert_eq!(before, ["fixture.mask", "store"]);
    for id in [".", "..", "a..b", "..."] {
        let (code, out, err) = grow(&store, &["--mask", &input, "--px", "2", "--request-id", id]);
        assert_eq!((code, err.as_str()), (0, ""), "{id}: {out}");
        let (run, _) = printed(&out);
        let (code, shown, _) =
            whizbang(&["runs", "show", &run, "--store", store.to_str().unwrap()]);
        assert_eq!(code, 0);
        assert!(
            shown.lines().any(|line| line == format!("request: {id}")),
            "{shown}"
        );
    }
    assert_eq!(around(&dir), before);
    assert_eq!(folders(&store), inside);
    for path in tree(&store) {
        let name = path.rsplit('/').next().unwrap();
        assert!(!name.starts_with('.') && !name.contains(".."), "{path}");
    }
}

#[test]
fn the_help_of_grow_states_its_arguments_and_its_exit_codes() {
    let (code, help, _) = whizbang(&["rascal", "grow", "--help"]);
    assert_eq!(code, 0);
    for said in [
        "--mask <MASK>",
        "--px <PX>",
        "--down-px <DOWN_PX>",
        "--store <DIR>",
        "--request-id <ID>",
        "from 0 to 10000",
        "Exit codes:",
        "0  the run completed",
        "1  nothing ran, or the run did not complete",
        "2  the command line was not understood",
        "3  what was to be printed could not be written",
        "101  the command itself failed",
    ] {
        assert!(help.contains(said), "{said:?} is not in:\n{help}");
    }
}

// --- the door, with an executor the test built ------------------------------------------------

/// An action under `grow`'s name and descriptor that ends as the test says.
struct Ends(Grow, Result<(), Failure>);

impl Action for Ends {
    fn descriptor(&self) -> &ActionDescriptor {
        self.0.descriptor()
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        match &self.1 {
            Ok(()) => {
                context.cancellation().cancel();
                self.0.run(call, context)
            }
            Err(failure) => Err(failure.clone()),
        }
    }
}

/// A store with the fixture in it, and the binary's own executor over it with this
/// action registered in place of the shipped `grow`.
fn runner(dir: &Scratch, action: Arc<dyn Action>, secrets: Secrets) -> (Engine, String) {
    let store = Arc::new(Store::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap());
    let mut publication = store.begin().unwrap();
    let id = artifact::put(&mut publication, &Artifact::from(fixture())).unwrap();
    publication.publish().unwrap();
    let mut registry = whizbang_core::contract::Registry::new();
    registry.register(action).unwrap();
    let engine = whizbang::wiring::runner(registry, store).with_secrets(secrets);
    (engine, id.0.to_string())
}

fn asked<'a>(mask: &'a str, request_id: Option<&'a str>) -> GrowAsked<'a> {
    GrowAsked {
        mask,
        px: 8.0,
        down_px: None,
        request_id,
    }
}

/// A made-up key, long enough to be a secret, assembled here so that no scan of this
/// file finds one.
fn made_up_key() -> String {
    ["made", "up", "key", "0123456789"].join("-")
}

#[test]
fn a_request_id_that_holds_a_secret_is_refused_without_being_shown() {
    let dir = Scratch::new("rascal");
    let key = made_up_key();
    let secrets = Secrets::new([key.clone()]).unwrap();
    let (engine, mask) = runner(&dir, Arc::new(Grow::new()), secrets);
    for id in [key.clone(), format!("job-{key}"), format!("{key} and more")] {
        let complaint = door::grow(&engine, &asked(&mask, Some(&id))).unwrap_err();
        assert!(
            !complaint.contains(&key) && !complaint.contains("0123456789"),
            "{complaint}"
        );
        assert!(
            complaint.contains("holds something that must not be written"),
            "{complaint}"
        );
    }
    // Nothing was admitted under any of them.
    let reader = Reader::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    assert!(reader.snapshot().unwrap().runs().is_empty());
    // An id with no secret in it is taken.
    let out = door::grow(&engine, &asked(&mask, Some("job-1"))).unwrap();
    assert!(out.contains(" completed\nmask sha256:"), "{out}");
}

#[test]
fn a_run_that_does_not_complete_is_a_complaint_that_names_the_run_escaped_and_redacted() {
    let key = made_up_key();
    let secrets = || Secrets::new([key.clone()]).unwrap();
    // An error whose message holds a secret, an escape sequence and a line break.
    let dir = Scratch::new("rascal");
    let message = format!("it broke: {key}\u{1b}[31m\nand more");
    let failing = Ends(Grow::new(), Err(Failure::error("pixels", &message)));
    let (engine, mask) = runner(&dir, Arc::new(failing), secrets());
    let complaint = door::grow(&engine, &asked(&mask, None)).unwrap_err();
    let reader = Reader::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let run = reader
        .snapshot()
        .unwrap()
        .runs()
        .keys()
        .next()
        .unwrap()
        .clone();
    assert_eq!(
        complaint,
        format!(
            "run {run} ended in an error (pixels): it broke: [redacted]\\u{{1b}}[31m\\nand more; \
             nothing was committed"
        )
    );

    // Invalid, and cancelled.
    let dir = Scratch::new("rascal");
    let invalid = Ends(Grow::new(), Err(Failure::invalid("the core is empty")));
    let (engine, mask) = runner(&dir, Arc::new(invalid), secrets());
    let complaint = door::grow(&engine, &asked(&mask, None)).unwrap_err();
    assert!(
        complaint.ends_with(
            " found what it was given invalid: the core is empty; nothing was committed"
        ),
        "{complaint}"
    );
    let dir = Scratch::new("rascal");
    let (engine, mask) = runner(&dir, Arc::new(Ends(Grow::new(), Ok(()))), secrets());
    let complaint = door::grow(&engine, &asked(&mask, Some("job-2"))).unwrap_err();
    assert!(
        complaint.ends_with(" was cancelled; nothing was committed"),
        "{complaint}"
    );
    // The same request again is the same run, and still a complaint.
    let again = door::grow(&engine, &asked(&mask, Some("job-2"))).unwrap_err();
    assert_eq!(
        again,
        complaint.replace(
            " was cancelled",
            " (the request was sent before, and nothing ran again) was cancelled"
        )
    );

    // A very long message is cut where the executor cuts it, and no further text follows.
    let dir = Scratch::new("rascal");
    let long = Ends(
        Grow::new(),
        Err(Failure::error("pixels", &"y".repeat(100_000))),
    );
    let (engine, mask) = runner(&dir, Arc::new(long), secrets());
    let complaint = door::grow(&engine, &asked(&mask, None)).unwrap_err();
    assert!(complaint.len() < 4400, "{} bytes", complaint.len());
}

// --- a panic ----------------------------------------------------------------------------------

/// An action under `grow`'s name that panics with a secret in what it says.
struct Panics(Grow, String);

impl Action for Panics {
    fn descriptor(&self) -> &ActionDescriptor {
        self.0.descriptor()
    }

    fn run(&self, _: &PreparedCall, _: &Context) -> Result<Produced, Failure> {
        panic!("{}", self.1);
    }
}

/// Run only by the test below, in a process of its own: it installs the door's panic
/// hook, as `main` does first of all, and runs an action that panics.
#[test]
#[ignore = "run by a_panic_leaves_nothing_it_said_and_no_place_in_the_source_on_stderr"]
fn child_an_action_panics_under_the_doors_hook() {
    door::quiet_panics();
    let dir = Scratch::new("rascal");
    let key = made_up_key();
    let secrets = Secrets::new([key.clone()]).unwrap();
    let panics = Panics(Grow::new(), format!("the key is {key}"));
    let (engine, mask) = runner(&dir, Arc::new(panics), secrets);
    let complaint = door::grow(&engine, &asked(&mask, None)).unwrap_err();
    eprintln!("whizbang: {complaint}");
}

#[test]
fn a_panic_leaves_nothing_it_said_and_no_place_in_the_source_on_stderr() {
    // The test binary runs itself, for the one test above, with nothing captured: what
    // comes back on stderr is what the door's hook and the door printed.
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "rascal::child_an_action_panics_under_the_doors_hook",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("the test binary runs");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "{err}");
    let key = made_up_key();
    assert!(!err.contains(&key) && !err.contains("0123456789"), "{err}");
    assert!(
        !err.contains("panicked at") && !err.contains(".rs"),
        "{err}"
    );
    let lines: Vec<&str> = err
        .lines()
        .filter(|l| l.starts_with("whizbang: "))
        .collect();
    assert_eq!(lines.len(), 2, "{err}");
    assert_eq!(lines[0], format!("whizbang: {}", door::PANIC_LINE));
    assert!(
        lines[1].contains(" ended in an error (panic): ") && lines[1].contains("[redacted]"),
        "{err}"
    );
    // And `main` installs that hook before anything else: the binary's source says so.
    let main = include_str!("../../src/main.rs");
    let body = main.split("fn main() -> ExitCode {").nth(1).expect("main");
    assert!(
        body.trim_start().starts_with("door::quiet_panics();"),
        "{body}"
    );
}
