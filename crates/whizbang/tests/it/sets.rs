//! `sets::…`: refusals that tell nothing of a held-out set (plan mE, task M1, item 7), over
//! a set made in the test whose item ids and job text would each give its construction away.

use whizbang::held_out;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::sets;

const ID: &str = "source-fidelity/secret-v1";

/// What no held-out refusal may hold: the folders, the item ids, a job's text and a TOML
/// parser's words.
fn told(said: &str, root: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    for word in [
        root.display().to_string(),
        "zebra-hidden".to_owned(),
        "okapi-hidden".to_owned(),
        "lion".to_owned(),
        "items/".to_owned(),
        "expected".to_owned(),
        "TOML".to_owned(),
        ".png".to_owned(),
    ] {
        if said.contains(&word) {
            found.push(word);
        }
    }
    found
}

#[test]
fn a_held_out_refusal_names_no_path_no_item_and_no_text() {
    let world = crate::wired::World::new("held-out");
    let dir = world.beside("owner");
    let set = crate::fetch::tiny_of(
        &dir,
        ID,
        "source-fidelity",
        &["candidate", "source"],
        &["zebra-hidden", "okapi-hidden"],
    );
    let store = world.beside("store");
    drop(Store::open(&store, std::sync::Arc::new(RealFs)).unwrap());
    let recovery = whizbang::wiring::recovery();
    let ids = [ID.to_owned()];
    // A set that reads: no refusal, and a refusal of something else passes as it was.
    assert_eq!(held_out::refusal(&set.root, ID), None);
    assert_eq!(
        held_out::shown(&set.root, &ids, "the allowance is spent"),
        "the allowance is spent"
    );
    // A refusal that names a path of the sets' folder, or an item, says only that it was.
    for leaking in [
        format!("{}: no such file", set.root.join(ID).display()),
        "item okapi-hidden image source is not in the store".to_owned(),
    ] {
        let said = held_out::shown(&set.root, &ids, &leaking);
        assert_eq!(said, format!("{ID}: {}", held_out::WITHHELD));
    }
    // A malformed job: the set, the item's place, the kind of problem.
    let job = set.root.join(ID).join("items/okapi-hidden/job.toml");
    let kept = std::fs::read_to_string(&job).unwrap();
    std::fs::write(&job, "[job\nbrief = \"the lion behind the fence\"\n").unwrap();
    let plain = sets::find(&set.root, ID).unwrap_err();
    assert!(
        !told(&plain, &set.root).is_empty(),
        "the plain refusal names nothing: {plain}"
    );
    let said = held_out::shown(&set.root, &ids, &plain);
    assert_eq!(
        said,
        format!("{ID}: item 2: its job cannot be read as one check takes")
    );
    assert_eq!(told(&said, &set.root), Vec::<String>::new());
    std::fs::write(&job, kept).unwrap();
    // A missing picture, and one whose digest is not its pin, fetched from the folder.
    let found = sets::find(&set.root, ID).unwrap();
    let picture = set.from.join(ID).join("items/zebra-hidden/source.png");
    let bytes = std::fs::read(&picture).unwrap();
    std::fs::remove_file(&picture).unwrap();
    let said = held_out::from_folder(&found, &set.from, &store, &recovery).unwrap_err();
    assert_eq!(said, format!("{ID}: item 1: a picture is missing"));
    assert_eq!(told(&said, &set.from), Vec::<String>::new());
    let mut other = bytes.clone();
    other[bytes.len() - 1] ^= 1;
    std::fs::write(&picture, other).unwrap();
    let said = held_out::from_folder(&found, &set.from, &store, &recovery).unwrap_err();
    assert_eq!(
        said,
        format!("{ID}: item 1: a picture is not the one its job pins")
    );
    assert_eq!(told(&said, &set.from), Vec::<String>::new());
    // Whole again, it imports the rest; the picture before the missing one is there.
    std::fs::write(&picture, bytes).unwrap();
    let said = held_out::from_folder(&found, &set.from, &store, &recovery).unwrap();
    assert_eq!(
        said,
        format!("{ID}: imported 3 pictures from the folder, 1 already there\n")
    );
    // Every set under the folder, as `eval sets check --held-out` names them.
    assert_eq!(held_out::ids_under(&set.root), ids);
}
