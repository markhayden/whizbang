//! Runs and the two kinds of reference (plan 1A, task A3): what a run references, what
//! deleting it removes, and a diagnostic that is never an output.

use std::path::Path;
use std::sync::Arc;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Digest, Error, Kind, Reader, Store};
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::{B, C, after_restart, contents, for_every_loss, root};

pub(super) const MASK: &[u8] = b"a mask two runs share";
pub(super) const EXCHANGE: &[u8] = b"the exchange of a turn that failed";
pub(super) const ONLY_ONE: &[u8] = b"what only run one made";
pub(super) const LEDGER: &[u8] = b"a spend record";

/// A store with a record outside any run and three runs: `one` and `two` share the mask,
/// `one` has an output of its own, and `failed` holds diagnostics only, one of them the
/// very bytes `two` holds as an output.
pub(super) fn with_runs(root: &Path) {
    let store = Store::open(root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("spend/1", LEDGER).unwrap();
    publication
        .put_in("one", "mask", Kind::Output, MASK)
        .unwrap();
    publication
        .put_in("one", "render", Kind::Output, ONLY_ONE)
        .unwrap();
    publication
        .put_in("two", "mask", Kind::Output, MASK)
        .unwrap();
    publication
        .put_in("two", "exchange", Kind::Output, EXCHANGE)
        .unwrap();
    publication
        .put_in("failed", "exchange", Kind::Diagnostic, EXCHANGE)
        .unwrap();
    publication
        .put_in("failed", "journal", Kind::Diagnostic, B)
        .unwrap();
    assert_eq!(publication.publish().unwrap().generation, 1);
}

pub(super) fn object(root: &Path, bytes: &[u8]) -> std::path::PathBuf {
    root.join("objects").join(Digest::of_bytes(bytes).hex())
}

/// The ids of the runs a reader sees.
pub(super) fn runs(root: &Path) -> Vec<String> {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    reader.snapshot().unwrap().runs().keys().cloned().collect()
}

#[test]
fn a_run_holds_its_references_under_names_of_its_own() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(runs(&root), ["failed", "one", "two"]);
    let names = |run: &str| -> Vec<String> {
        let run = generation.run(run).unwrap();
        run.entries().keys().cloned().collect()
    };
    assert_eq!(names("one"), ["mask", "render"]);
    assert_eq!(names("two"), ["exchange", "mask"]);
    assert_eq!(names("failed"), ["exchange", "journal"]);
    assert!(generation.run("three").is_none());
    // The same name in two runs is two references; here to the same bytes, stored once.
    let one = generation.run("one").unwrap().get("mask").unwrap();
    let two = generation.run("two").unwrap().get("mask").unwrap();
    assert_eq!(one, two);
    assert_eq!(reader.read_output(one).unwrap(), MASK);
    assert_eq!(std::fs::read_dir(root.join("objects")).unwrap().count(), 5);
    // Outside any run.
    assert_eq!(generation.entries().keys().collect::<Vec<_>>(), ["spend/1"]);
    assert_eq!(generation.get("spend/1").unwrap().kind, Kind::Output);
}

#[test]
fn the_same_bytes_are_a_diagnostic_to_one_run_and_an_output_to_another() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    // After reopening, through a writer and through a reader.
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    for generation in [store.snapshot().unwrap(), reader.snapshot().unwrap()] {
        let diagnostic = generation.run("failed").unwrap().get("exchange").unwrap();
        let output = generation.run("two").unwrap().get("exchange").unwrap();
        assert_eq!(diagnostic.digest, output.digest, "one stored file");
        assert_eq!(
            (diagnostic.kind, output.kind),
            (Kind::Diagnostic, Kind::Output)
        );

        // Each is treated as it says. A diagnostic is read, and is not an output.
        assert_eq!(store.read(diagnostic).unwrap(), EXCHANGE);
        assert_eq!(reader.read(diagnostic).unwrap(), EXCHANGE);
        for result in [
            store.read_output(diagnostic),
            reader.read_output(diagnostic),
        ] {
            match result {
                Err(Error::NotAnOutput { digest }) => {
                    assert_eq!(digest, Digest::of_bytes(EXCHANGE))
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(store.read_output(output).unwrap(), EXCHANGE);
        assert_eq!(reader.read_output(output).unwrap(), EXCHANGE);
    }
    assert!(object(&root, EXCHANGE).exists());
}

#[test]
fn a_generation_file_says_the_kind_of_each_reference_and_the_run_it_belongs_to() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("a", B).unwrap();
    publication
        .put_in("r", "journal", Kind::Diagnostic, C)
        .unwrap();
    publication.publish().unwrap();
    let (b, c) = (Digest::of_bytes(B).hex(), Digest::of_bytes(C).hex());
    let expected = format!(
        r#"{{
  "format": 2,
  "number": 1,
  "entries": {{
    "a": {{
      "digest": "{b}",
      "bytes": {},
      "kind": "output"
    }}
  }},
  "runs": {{
    "r": {{
      "entries": {{
        "journal": {{
          "digest": "{c}",
          "bytes": {},
          "kind": "diagnostic"
        }}
      }}
    }}
  }}
}}
"#,
        B.len(),
        C.len()
    );
    assert_eq!(
        std::fs::read_to_string(root.join("generations/1.json")).unwrap(),
        expected
    );
}

#[test]
fn a_later_publication_adds_to_a_run_and_replaces_a_name_of_it() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put_in("one", "render", Kind::Output, C)
        .unwrap();
    publication
        .put_in("one", "journal", Kind::Diagnostic, B)
        .unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    let one = generation.run("one").unwrap();
    assert_eq!(
        one.entries().keys().collect::<Vec<_>>(),
        ["journal", "mask", "render"]
    );
    assert_eq!(store.read(one.get("render").unwrap()).unwrap(), C);
    assert_eq!(generation.run("two").unwrap().entries().len(), 2);
}

#[test]
fn a_publication_that_makes_a_run_is_refused_when_the_store_holds_that_run() {
    // Adding names to a run is one thing, and `put_in` does it whether the run is there
    // or not. Making a run is another: the publication says so, and the store refuses
    // it when the run exists, looking inside the publishing step. Nothing is merged and
    // nothing is published.
    let dir = Scratch::new("store");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let before = store.snapshot().unwrap();
    let mut publication = store.begin().unwrap();
    publication.create_run("one").unwrap();
    publication
        .put_in("one", "record", Kind::Diagnostic, b"another run's record")
        .unwrap();
    publication.put("spend/2", LEDGER).unwrap();
    let refused = publication.publish().unwrap_err();
    assert!(
        matches!(&refused, Error::RunExists { run } if run == "one"),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "the store already holds a run \"one\": a new run is not merged into it"
    );
    assert_eq!(
        refused.without_paths(),
        "the store already holds a run of that id"
    );
    assert_eq!(store.snapshot().unwrap(), before);

    // Two publications that each make the same new run, both begun before either is
    // published: the first is taken, the second is refused, and the run is the first's.
    let mut first = store.begin().unwrap();
    let mut second = store.begin().unwrap();
    first.create_run("new").unwrap();
    first
        .put_in("new", "record", Kind::Diagnostic, b"first")
        .unwrap();
    second.create_run("new").unwrap();
    second
        .put_in("new", "record", Kind::Diagnostic, b"second")
        .unwrap();
    second
        .put_in("new", "more", Kind::Diagnostic, b"second")
        .unwrap();
    first.publish().unwrap();
    assert!(matches!(second.publish(), Err(Error::RunExists { .. })));
    let generation = store.snapshot().unwrap();
    let run = generation.run("new").unwrap();
    assert_eq!(run.entries().len(), 1);
    assert_eq!(store.read(run.get("record").unwrap()).unwrap(), b"first");
    // A run made with no name in it exists, and its id is held to the store's rule.
    let mut publication = store.begin().unwrap();
    publication.create_run("empty").unwrap();
    assert!(matches!(
        publication.create_run(""),
        Err(Error::InvalidRun { .. })
    ));
    publication.publish().unwrap();
    assert!(
        store
            .snapshot()
            .unwrap()
            .run("empty")
            .unwrap()
            .entries()
            .is_empty()
    );
    // A run of an id the store does not hold is made as freely after that.
    let mut publication = store.begin().unwrap();
    publication.create_run("fresh").unwrap();
    publication.publish().unwrap();
}

#[test]
fn a_name_that_is_made_is_never_given_other_content() {
    // A record that must outlive every run is published under a name outside runs, and
    // made there: a publication that makes a name the store already holds is refused,
    // whole, and the name keeps what it had.
    let dir = Scratch::new("store-names");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .create("requests/a", Kind::Diagnostic, b"first")
        .unwrap();
    publication.publish().unwrap();
    let before = store.snapshot().unwrap();
    let entry = before.get("requests/a").unwrap().clone();
    assert_eq!(entry.kind, Kind::Diagnostic);
    assert_eq!(store.read(&entry).unwrap(), b"first");
    assert!(matches!(
        store.read_output(&entry),
        Err(Error::NotAnOutput { .. })
    ));

    // Made again, with a run beside it: nothing of the publication is published.
    let mut publication = store.begin().unwrap();
    publication
        .create("requests/a", Kind::Diagnostic, b"second")
        .unwrap();
    publication.create_run("beside").unwrap();
    let refused = publication.publish().unwrap_err();
    assert!(
        matches!(&refused, Error::NameExists { name } if name == "requests/a"),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "the store already holds the name \"requests/a\": it is made once and keeps its content"
    );
    assert_eq!(
        refused.without_paths(),
        "the store already holds a name that is made once"
    );
    assert_eq!(store.snapshot().unwrap(), before);

    // The same content again is refused as well: the name was made, by someone.
    let mut publication = store.begin().unwrap();
    publication
        .create("requests/a", Kind::Diagnostic, b"first")
        .unwrap();
    assert!(matches!(
        publication.publish(),
        Err(Error::NameExists { .. })
    ));

    // Two publications that each make one name, both begun before either is published:
    // the first is taken, and the second is refused whatever it saw when it began.
    let mut first = store.begin().unwrap();
    let mut second = store.begin().unwrap();
    first.create("requests/b", Kind::Diagnostic, b"1").unwrap();
    second.create("requests/b", Kind::Diagnostic, b"2").unwrap();
    first.publish().unwrap();
    assert!(matches!(second.publish(), Err(Error::NameExists { .. })));
    let generation = store.snapshot().unwrap();
    assert_eq!(
        store.read(generation.get("requests/b").unwrap()).unwrap(),
        b"1"
    );

    // One publication cannot make a name and also give it content by `put`, in either
    // order: the name would hold whichever came last.
    let mut publication = store.begin().unwrap();
    publication
        .create("requests/c", Kind::Diagnostic, b"1")
        .unwrap();
    assert!(matches!(
        publication.put("requests/c", b"2"),
        Err(Error::NameExists { .. })
    ));
    assert!(matches!(
        publication.create("requests/c", Kind::Diagnostic, b"3"),
        Err(Error::NameExists { .. })
    ));
    publication.put("free", b"4").unwrap();
    assert!(matches!(
        publication.create("free", Kind::Diagnostic, b"5"),
        Err(Error::NameExists { .. })
    ));
}

#[test]
fn known_a_name_that_was_made_can_still_be_replaced_by_put() {
    // The limit of "made once", pinned: `create` refuses a name that exists, and `put`
    // does not ask. A later publication that puts the name replaces what it held. No
    // code of whizbang puts a name under `requests/` or `spend/`; nothing in the store
    // stops other code.
    let dir = Scratch::new("store-names");
    let root = root(&dir);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .create("requests/a", Kind::Diagnostic, b"first")
        .unwrap();
    publication.publish().unwrap();
    let mut publication = store.begin().unwrap();
    publication.put("requests/a", b"other").unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    assert_eq!(
        store.read(generation.get("requests/a").unwrap()).unwrap(),
        b"other"
    );
}

#[test]
fn a_name_that_is_kept_holds_the_first_output_it_was_given() {
    // An artifact is stored under a name made of its identity, and its identity does
    // not cover its origin: two runs can make one artifact with two origins, which are
    // two files for one name. The name keeps the first. `keep` gives a name content
    // unless the store holds the name as an output already, and the store looks inside
    // its publishing step.
    let dir = Scratch::new("store-names");
    let root = root(&dir);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let read = |name: &str| {
        let generation = store.snapshot().unwrap();
        let entry = generation.get(name).unwrap().clone();
        (entry.kind, store.read(&entry).unwrap())
    };

    // A name the store does not hold is given the content, as an output.
    let mut publication = store.begin().unwrap();
    publication.keep("artifacts/mask/a", b"first").unwrap();
    publication.publish().unwrap();
    assert_eq!(read("artifacts/mask/a"), (Kind::Output, b"first".to_vec()));

    // Kept again with other content, with a run beside it: the name holds what it
    // held, and the rest of the publication is published.
    let mut publication = store.begin().unwrap();
    publication.keep("artifacts/mask/a", b"second").unwrap();
    publication.create_run("beside").unwrap();
    publication.publish().unwrap();
    assert_eq!(read("artifacts/mask/a"), (Kind::Output, b"first".to_vec()));
    assert!(store.snapshot().unwrap().run("beside").is_some());

    // Two publications that each keep one name, both begun before either is
    // published: the first is taken, and the second leaves it, whatever it saw when
    // it began. Both are published.
    let mut first = store.begin().unwrap();
    let mut second = store.begin().unwrap();
    first.keep("artifacts/mask/b", b"1").unwrap();
    second.keep("artifacts/mask/b", b"2").unwrap();
    second.put("beside-b", b"3").unwrap();
    first.publish().unwrap();
    second.publish().unwrap();
    assert_eq!(read("artifacts/mask/b"), (Kind::Output, b"1".to_vec()));
    assert_eq!(read("beside-b"), (Kind::Output, b"3".to_vec()));

    // A diagnostic reference under the name is not the output: it is replaced, as
    // `put` replaces it.
    let mut publication = store.begin().unwrap();
    publication
        .create("artifacts/mask/c", Kind::Diagnostic, b"evidence")
        .unwrap();
    publication.publish().unwrap();
    let mut publication = store.begin().unwrap();
    publication.keep("artifacts/mask/c", b"the mask").unwrap();
    publication.publish().unwrap();
    assert_eq!(
        read("artifacts/mask/c"),
        (Kind::Output, b"the mask".to_vec())
    );

    // One publication cannot keep a name and also make it or put it, in any order:
    // the name would hold whichever the store came to last.
    let mut publication = store.begin().unwrap();
    publication.keep("artifacts/mask/d", b"1").unwrap();
    assert!(matches!(
        publication.put("artifacts/mask/d", b"2"),
        Err(Error::NameExists { .. })
    ));
    assert!(matches!(
        publication.create("artifacts/mask/d", Kind::Diagnostic, b"3"),
        Err(Error::NameExists { .. })
    ));
    publication.put("free", b"4").unwrap();
    assert!(matches!(
        publication.keep("free", b"5"),
        Err(Error::NameExists { .. })
    ));
    publication.create("made", Kind::Diagnostic, b"6").unwrap();
    assert!(matches!(
        publication.keep("made", b"7"),
        Err(Error::NameExists { .. })
    ));
    // Kept twice in one publication, the first content is the one.
    publication.keep("artifacts/mask/d", b"8").unwrap();
    publication.publish().unwrap();
    assert_eq!(read("artifacts/mask/d"), (Kind::Output, b"1".to_vec()));
}

#[test]
fn a_crash_at_any_event_of_a_publication_that_only_keeps_a_name_leaves_it_whole_or_absent() {
    // A publication that keeps a name and does nothing else names content like any
    // other: the content is durable before the generation that names it is visible.
    const KEPT: &[u8] = b"an artifact that is kept";
    let keep = |root: &Path, fs: Arc<ShimFs>| {
        Store::open(root, fs).and_then(|store| {
            let mut publication = store.begin()?;
            publication.keep("artifacts/mask/k", KEPT)?;
            publication.publish()
        })
    };
    let count = {
        let dir = Scratch::new("store-names");
        let root = root(&dir);
        with_runs(&root);
        let fs = Arc::new(ShimFs::new());
        keep(&root, fs.clone()).unwrap();
        fs.events().len()
    };
    for crash_at in 0..=count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store-names");
            let root = root(&dir);
            with_runs(&root);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = keep(&root, fs.clone());
            fs.crash(lost);

            // Reading after the restart reads every name whole.
            let (number, held) = after_restart(&root);
            match number {
                1 => {
                    assert!(result.is_err(), "{case}: success was reported");
                    assert!(!held.contains_key("artifacts/mask/k"), "{case}");
                }
                2 => assert_eq!(held["artifacts/mask/k"], KEPT, "{case}"),
                other => panic!("{case}: generation {other}"),
            }
            if matches!(lost, Lost::Unsynced) && crash_at == count {
                assert_eq!(number, 2, "{case}");
            }
            fs.crash_choices()
        });
    }
}

#[test]
fn a_publication_that_adds_to_a_run_that_must_exist_is_refused_when_it_is_gone() {
    // A run is made in one publication and added to in another. The second says that
    // the run must be there: when it was deleted in between, nothing is published, and
    // the run is not made again from the names that were to be added.
    let dir = Scratch::new("store-names");
    let root = root(&dir);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let mut publication = store.begin().unwrap();
    publication.create_run("r").unwrap();
    publication
        .put_in("r", "admitted", Kind::Diagnostic, b"a")
        .unwrap();
    publication.publish().unwrap();

    // While it is there, names are added to it and the ones it had stay.
    let mut publication = store.begin().unwrap();
    publication.extend_run("r").unwrap();
    publication
        .put_in("r", "journal", Kind::Diagnostic, b"j")
        .unwrap();
    publication.publish().unwrap();
    let generation = store.snapshot().unwrap();
    assert_eq!(generation.run("r").unwrap().entries().len(), 2);

    let mut adding = store.begin().unwrap();
    adding.extend_run("r").unwrap();
    adding
        .put_in("r", "record", Kind::Diagnostic, b"x")
        .unwrap();
    adding
        .create("requests/a/ended", Kind::Diagnostic, b"e")
        .unwrap();
    store.delete(&["r"]).unwrap();
    let before = store.snapshot().unwrap();
    let refused = adding.publish().unwrap_err();
    assert!(
        matches!(&refused, Error::NoSuchRun { run } if run == "r"),
        "{refused:?}"
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(before.run("r").is_none());
    assert!(before.get("requests/a/ended").is_none());
    let mut publication = store.begin().unwrap();
    assert!(matches!(
        publication.extend_run(""),
        Err(Error::InvalidRun { .. })
    ));
}

#[test]
fn a_writer_knows_the_runs_it_made_since_it_opened_the_store() {
    // What tells a run that this process is executing from one that a process now gone
    // left: the writer made it, by a publication that succeeded, since it opened.
    let dir = Scratch::new("store-names");
    let root = root(&dir);
    {
        let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
        assert!(!store.made_here("r"));
        let mut publication = store.begin().unwrap();
        publication.create_run("r").unwrap();
        publication.publish().unwrap();
        assert!(store.made_here("r"));
        // A run that was only added to, and one whose publication was refused, were
        // not made here.
        let mut publication = store.begin().unwrap();
        publication
            .put_in("other", "x", Kind::Diagnostic, b"x")
            .unwrap();
        publication.publish().unwrap();
        assert!(!store.made_here("other"));
        let mut publication = store.begin().unwrap();
        publication.create_run("r").unwrap();
        publication.create_run("never").unwrap();
        publication.publish().unwrap_err();
        assert!(!store.made_here("never"));
        // A run that is added to is still open; closed, or deleted, it is forgotten,
        // so what the writer remembers does not grow with every run it ever made.
        let mut publication = store.begin().unwrap();
        publication.extend_run("r").unwrap();
        publication.publish().unwrap();
        assert!(store.made_here("r"));
        let mut publication = store.begin().unwrap();
        publication.close_run("r").unwrap();
        publication
            .put_in("r", "record", Kind::Diagnostic, b"x")
            .unwrap();
        publication.publish().unwrap();
        assert!(!store.made_here("r"));
        let mut publication = store.begin().unwrap();
        publication.create_run("gone").unwrap();
        publication.publish().unwrap();
        assert!(store.made_here("gone"));
        store.delete(&["gone"]).unwrap();
        assert!(!store.made_here("gone"));
        // Closing a run that is not there is refused, as adding to it is.
        let mut publication = store.begin().unwrap();
        publication.close_run("gone").unwrap();
        assert!(matches!(
            publication.publish(),
            Err(Error::NoSuchRun { .. })
        ));
    }
    // The next writer made none of them.
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    assert!(store.snapshot().unwrap().run("r").is_some());
    assert!(!store.made_here("r"));
}

#[test]
fn a_closed_run_that_was_kept_for_its_closer_stays_in_the_table_until_it_is_let_go() {
    // Whoever closes a run may still hold something of it when the closing becomes
    // visible. Asked to, the store keeps the run in its table, with what is attached,
    // until the closer lets it go; and lets go of nothing that was not closed that way.
    let dir = Scratch::new("store-names");
    let store = Store::open(&root(&dir), Arc::new(ShimFs::new())).unwrap();
    let made = |run: &str| {
        let mut publication = store.begin().unwrap();
        publication.create_run(run).unwrap();
        publication.attach(run, Arc::new("its own"));
        publication.publish().unwrap();
    };
    for run in ["kept", "plain", "open", "deleted", "refused"] {
        made(run);
    }

    // Closed and kept: still in the table once the closing is visible, and out of it
    // when it is let go, and not before.
    let mut publication = store.begin().unwrap();
    publication.close_run("kept").unwrap();
    publication.keep_until_let_go("kept");
    // Asked for a run the publication does not close: nothing comes of it.
    publication.keep_until_let_go("open");
    publication.publish().unwrap();
    assert!(store.made_here("kept"));
    assert!(store.attached("kept").unwrap().downcast::<&str>().is_ok());
    store.let_go("kept");
    assert!(!store.made_here("kept"));
    assert!(store.attached("kept").is_none());

    // Closed without being kept: gone when the closing is visible, as before.
    let mut publication = store.begin().unwrap();
    publication.close_run("plain").unwrap();
    publication.publish().unwrap();
    assert!(!store.made_here("plain"));

    // A run that is open is not let go, whoever asks; nor is one that is not there.
    store.let_go("open");
    store.let_go("never made");
    assert!(store.made_here("open"));

    // A closing that was to be kept and was never published closed nothing: the run
    // is open, and letting it go changes nothing.
    let mut publication = store.begin().unwrap();
    publication.close_run("refused").unwrap();
    publication.keep_until_let_go("refused");
    drop(publication);
    store.let_go("refused");
    assert!(store.made_here("refused"));

    // A run that is kept and then deleted is out of the table at the deletion.
    let mut publication = store.begin().unwrap();
    publication.close_run("deleted").unwrap();
    publication.keep_until_let_go("deleted");
    publication.publish().unwrap();
    assert!(store.made_here("deleted"));
    store.delete(&["deleted"]).unwrap();
    assert!(!store.made_here("deleted"));
}

#[test]
fn what_is_attached_to_a_run_is_had_while_the_writer_has_the_run_open_and_no_longer() {
    // The table of the runs a writer made and has open holds, for each, what its maker
    // attached: one table, so "made here" and "what is attached" never disagree.
    let dir = Scratch::new("store-names");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let word = |run: &str| {
        let attached = store.attached(run)?;
        Some(
            attached
                .downcast::<&str>()
                .map_or("something else", |word| *word),
        )
    };
    assert_eq!(word("r"), None);

    // Had at the moment the run becomes visible, and not before it is published.
    let mut publication = store.begin().unwrap();
    publication.create_run("r").unwrap();
    publication.attach("r", Arc::new("the run's own"));
    publication.attach("not made", Arc::new("nothing"));
    assert_eq!(word("r"), None);
    let seen = Arc::new(std::sync::Mutex::new(None));
    fs.pause_after_rename_onto("CURRENT", {
        let (store, seen) = (store.clone(), seen.clone());
        move || {
            let attached = store.attached("r").map(|a| *a.downcast::<&str>().unwrap());
            *seen.lock().unwrap() = Some(attached);
        }
    });
    publication.publish().unwrap();
    assert_eq!(*seen.lock().unwrap(), Some(Some("the run's own")));
    assert_eq!(word("r"), Some("the run's own"));
    assert_eq!(word("not made"), None);

    // A run made with nothing attached is made here all the same.
    let mut publication = store.begin().unwrap();
    publication.create_run("plain").unwrap();
    publication.publish().unwrap();
    assert_eq!(word("plain"), Some("something else"));
    assert!(store.made_here("plain"));

    // Still had at the moment the closing becomes visible, and gone once it has.
    let mut publication = store.begin().unwrap();
    publication.close_run("r").unwrap();
    fs.pause_after_rename_onto("CURRENT", {
        let (store, seen) = (store.clone(), seen.clone());
        move || {
            let attached = store.attached("r").map(|a| *a.downcast::<&str>().unwrap());
            *seen.lock().unwrap() = Some(attached);
        }
    });
    publication.publish().unwrap();
    assert_eq!(*seen.lock().unwrap(), Some(Some("the run's own")));
    assert_eq!(word("r"), None);
    assert!(!store.made_here("r"));

    // A publication that is refused attaches nothing.
    let mut publication = store.begin().unwrap();
    publication.create_run("plain").unwrap();
    publication.attach("plain", Arc::new("a second maker's"));
    assert!(publication.publish().is_err());
    assert_eq!(word("plain"), Some("something else"));
}

#[test]
fn a_run_is_known_as_made_here_from_the_moment_it_is_visible() {
    // Whoever sees a run in a snapshot and then asks whether this writer made it must
    // not be told no of a run this writer has just made. The look is made at the one
    // moment that matters: the pointer was renamed, so the run is visible, and the
    // publication has not returned.
    let dir = Scratch::new("store-made");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Arc::new(Store::open(&root, fs.clone()).unwrap());
    let seen = Arc::new(std::sync::Mutex::new(None));
    {
        let (store, seen) = (store.clone(), seen.clone());
        fs.pause_after_rename_onto("CURRENT", move || {
            let visible = store.snapshot().unwrap().run("r").is_some();
            *seen.lock().unwrap() = Some((visible, store.made_here("r")));
        });
    }
    let mut publication = store.begin().unwrap();
    publication.create_run("r").unwrap();
    publication.publish().unwrap();
    assert_eq!(*seen.lock().unwrap(), Some((true, true)));

    // A run whose publication did not become visible was not made here: the pointer's
    // rename is refused, and the run is forgotten again.
    fs.refuse_matching(0, |event| {
        matches!(event, whizbang_testkit::fs_shim::Event::Rename(_, to) if to.ends_with("CURRENT"))
    });
    let mut publication = store.begin().unwrap();
    publication.create_run("never").unwrap();
    publication.publish().unwrap_err();
    assert!(store.snapshot().unwrap().run("never").is_none());
    assert!(!store.made_here("never"));
    // And a run that is closed is still made here while its closing is not visible:
    // the generation's file is refused, after `close_run` was called.
    fs.refuse_matching(0, |event| {
        matches!(event, whizbang_testkit::fs_shim::Event::Write(path)
            if path.ends_with("generation.json"))
    });
    let mut publication = store.begin().unwrap();
    publication.close_run("r").unwrap();
    publication
        .put_in("r", "record", Kind::Diagnostic, b"x")
        .unwrap();
    publication.publish().unwrap_err();
    assert!(store.made_here("r"));
}

#[test]
fn deleting_a_run_removes_its_references_and_no_others() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    assert_eq!(store.delete(&["one"]).unwrap().generation, 2);

    let generation = store.snapshot().unwrap();
    assert_eq!(runs(&root), ["failed", "two"]);
    // The mask that run two shares is still referenced, and read.
    let mask = generation.run("two").unwrap().get("mask").unwrap();
    assert_eq!(store.read_output(mask).unwrap(), MASK);
    assert!(
        !generation
            .runs()
            .values()
            .flat_map(|run| run.entries().values())
            .any(|entry| entry.digest == Digest::of_bytes(ONLY_ONE)),
        "what only the deleted run referenced is referenced no more"
    );
    // Records outside runs are not a run's to lose.
    assert_eq!(
        store.read(generation.get("spend/1").unwrap()).unwrap(),
        LEDGER
    );
}

#[test]
fn a_record_outside_runs_outlives_the_deletion_of_every_run() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    store.delete(&["one", "two", "failed"]).unwrap();
    drop(store);
    assert!(runs(&root).is_empty());
    assert_eq!(
        after_restart(&root),
        (2, contents(&[("spend/1", LEDGER)])),
        "the spend and request-id records are kept this way"
    );
}

#[test]
fn deleting_a_run_the_store_does_not_hold_is_refused_and_publishes_nothing() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    with_runs(&root);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    // One that exists beside one that does not: neither goes.
    match store.delete(&["one", "three"]) {
        Err(Error::NoSuchRun { run }) => assert_eq!(run, "three"),
        other => panic!("{other:?}"),
    }
    assert_eq!(store.snapshot().unwrap().number(), 1);
    assert_eq!(runs(&root), ["failed", "one", "two"]);
    assert!(!root.join("generations/2.json").exists());
    assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);
}

#[test]
fn a_crash_at_any_event_of_a_deletion_leaves_the_run_whole_or_gone() {
    let count = {
        let dir = Scratch::new("store-runs");
        let root = root(&dir);
        with_runs(&root);
        let fs = Arc::new(ShimFs::new());
        Store::open(&root, fs.clone())
            .unwrap()
            .delete(&["one"])
            .unwrap();
        fs.events().len()
    };
    for crash_at in 0..=count {
        for_every_loss(|lost| {
            let case = format!("{lost:?}, crash before event {crash_at}");
            let dir = Scratch::new("store-runs");
            let root = root(&dir);
            with_runs(&root);
            let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
            let result = Store::open(&root, fs.clone()).and_then(|store| store.delete(&["one"]));
            fs.crash(lost);

            let (number, _) = after_restart(&root);
            match number {
                1 => {
                    assert!(result.is_err(), "{case}: success was reported");
                    assert_eq!(runs(&root), ["failed", "one", "two"], "{case}");
                    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
                    let generation = reader.snapshot().unwrap();
                    for entry in generation.run("one").unwrap().entries().values() {
                        reader.read(entry).unwrap();
                    }
                }
                2 => assert_eq!(runs(&root), ["failed", "two"], "{case}"),
                other => panic!("{case}: generation {other}"),
            }
            if matches!(lost, Lost::Unsynced) && crash_at == count {
                assert_eq!(number, 2, "{case}");
            }
            fs.crash_choices()
        });
    }
}

#[test]
fn a_run_id_is_not_empty_and_holds_no_control_character() {
    let dir = Scratch::new("store-runs");
    let root = root(&dir);
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let mut publication = store.begin().unwrap();
    let before = fs.events().len();
    for id in [
        "",
        "a\nb",
        "tab\there",
        "\u{1b}[31mred",
        "bell\u{7}",
        "nul\0",
    ] {
        match publication.put_in(id, "out", Kind::Output, MASK) {
            Err(Error::InvalidRun { run }) => assert_eq!(run, id),
            other => panic!("{id:?}: {other:?}"),
        }
    }
    assert_eq!(fs.events().len(), before, "nothing was staged for them");
    let message = publication
        .put_in("a\nb", "out", Kind::Output, MASK)
        .unwrap_err()
        .to_string();
    assert!(
        message.starts_with("\"a\\nb\" cannot be a run's id"),
        "{message}"
    );
    // Anything else is an id, as far as the store goes.
    for id in ["one", "run 2", "caf\u{e9}/7", "-", "."] {
        publication.put_in(id, "out", Kind::Output, MASK).unwrap();
    }
    publication.publish().unwrap();
    assert_eq!(runs(&root), ["-", ".", "caf\u{e9}/7", "one", "run 2"]);
}

#[test]
fn a_generation_file_of_another_format_is_refused_as_another_format_not_as_damage() {
    let dir = Scratch::new("store-runs");
    let root = super::with_old(&dir);
    // Generation 1 exactly as the first format wrote it: no kinds, no runs.
    let first_format = format!(
        r#"{{
  "format": 1,
  "number": 1,
  "entries": {{
    "a": {{
      "digest": "{}",
      "bytes": {}
    }}
  }}
}}
"#,
        Digest::of_bytes(super::A1).hex(),
        super::A1.len()
    );
    std::fs::write(root.join("generations/1.json"), first_format).unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let by_reader = reader.snapshot().map(|_| ());
    let by_writer = Store::open(&root, Arc::new(ShimFs::new())).map(|_| ());
    for result in [by_reader, by_writer] {
        let error = result.unwrap_err();
        match &error {
            Error::OtherFormat {
                path,
                format: 1,
                reads: 2,
            } => assert_eq!(*path, root.join("generations/1.json")),
            other => panic!("{other:?}"),
        }
        let message = error.to_string();
        assert!(
            message.ends_with("is of another format: 1, and this build reads format 2"),
            "{message}"
        );
        assert!(!message.contains("damaged"), "{message}");
    }
}
