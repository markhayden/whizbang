//! The generation in buckets (plan m6, task S1, item 1; ADR 0047): the names of runs, of
//! request ids, of the ledger and of artifacts live in 256 buckets, which a publication
//! writes in one pack, a file of the store's content; the generation file holds the few
//! fixed names and a digest for each bucket, and so does not grow with the store.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Digest, buckets};
use whizbang_core::store::{Error, GENERATION_FORMAT, Kind, Reader, Store, WHOLE_FORMAT};
use whizbang_testkit::fs_shim::{Lost, ShimFs};
use whizbang_testkit::scratch::Scratch;

use super::{for_every_loss, losses};

/// The committed store of format 4, written by the build before buckets (plan m6, task
/// S1): an imported picture, three runs, each with its request id's two records, a mask
/// it committed and held, and a paid attempt's names in the ledger, and the ledger's
/// epoch.
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/store/format-4")
}

/// A copy of the committed store, to be opened and changed.
fn copied(dir: &Scratch) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            match entry.file_type().unwrap().is_dir() {
                true => copy(&entry.path(), &target),
                false => drop(std::fs::copy(entry.path(), &target).unwrap()),
            }
        }
    }
    let root = dir.path().join("store");
    copy(&fixture(), &root);
    // Git keeps no empty folder, and a store has its staging.
    std::fs::create_dir_all(root.join("staging")).unwrap();
    for folder in [
        root.as_path(),
        &root.join("objects"),
        &root.join("generations"),
    ] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    root
}

/// Every name the fixture holds outside runs, and every run with its names, read through
/// a reader with each reference's content.
fn everything(root: &Path) -> (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>) {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    for entry in generation.references() {
        reader.read(entry).unwrap();
    }
    let names = generation.entries().keys().cloned().collect();
    let runs = (generation.runs().iter())
        .map(|(id, run)| (id.clone(), run.entries().keys().cloned().collect()))
        .collect();
    (names, runs)
}

/// The format a generation file says it is.
fn format_of(root: &Path, number: u64) -> u32 {
    let file = root.join("generations").join(format!("{number}.json"));
    let text: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    text["format"].as_u64().unwrap() as u32
}

/// The names the fixture holds, as the build before buckets wrote them.
fn the_fixtures_names() -> (BTreeSet<String>, BTreeSet<String>) {
    let (names, runs) = everything(&fixture());
    (names, runs.into_keys().collect())
}

#[test]
fn a_store_of_the_old_format_reads_and_its_first_publication_writes_the_new() {
    let dir = Scratch::new("buckets-old");
    let root = copied(&dir);
    assert_eq!(format_of(&root, 8), WHOLE_FORMAT);
    let (names, runs) = everything(&root);
    // What the fixture holds: request ids, the ledger and artifacts, as the build
    // before buckets wrote them, read as they are.
    for at in 0..3 {
        let run = format!("1795275290505-{at}");
        assert!(runs[&run].contains("record.json"), "{run}");
        for name in [
            format!("requests/fixture-request-{at}/admitted"),
            format!("requests/fixture-request-{at}/ended"),
            format!("spend/reservations/{run}"),
            format!("spend/closed/{run}/released"),
        ] {
            assert!(names.contains(&name), "{name}");
        }
    }
    assert!(
        names
            .iter()
            .any(|name| name.starts_with("imported/artifacts/image/"))
    );
    assert_eq!(names.len(), 38, "{names:?}");

    // A writer opens it as it is, and its first publication writes the new format: the
    // generation file of format 5 and one pack of every bucket that holds anything.
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put_in(
            "1795275290505-9",
            "record.json",
            Kind::Diagnostic,
            b"one more",
        )
        .unwrap();
    assert_eq!(publication.publish().unwrap().generation, 9);
    assert_eq!(format_of(&root, 9), GENERATION_FORMAT);
    let generation = store.snapshot().unwrap();
    let packs: BTreeSet<Digest> = generation.bucket_digests().collect();
    assert_eq!(packs.len(), 1, "one pack, written once");
    let (after, runs_after) = everything(&root);
    assert_eq!(after, names);
    assert_eq!(runs_after.len(), runs.len() + 1);
    // The next publication writes the new format again, and only what it changed.
    let mut publication = store.begin().unwrap();
    publication
        .put_in(
            "1795275290505-10",
            "record.json",
            Kind::Diagnostic,
            b"and one more",
        )
        .unwrap();
    assert_eq!(publication.publish().unwrap().generation, 10);
    let generation = store.snapshot().unwrap();
    assert_eq!(generation.bucket_digests().count(), 2);
    drop(store);
    assert_eq!(everything(&root).0, names);
}

/// The buckets a pack holds, by index, from its file.
fn pack_buckets(root: &Path, pack: &Digest) -> BTreeSet<u8> {
    let bytes = std::fs::read(pack_file(root, &pack.hex())).unwrap();
    let pack: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (pack["buckets"].as_object().unwrap().keys())
        .map(|at| at.parse().unwrap())
        .collect()
}

/// The pack each bucket of the current generation is in, from the generation file.
fn packs_by_bucket(root: &Path) -> Vec<String> {
    let reader = Reader::open(root, Arc::new(RealFs)).unwrap();
    let number = reader.snapshot().unwrap().number();
    let file = root.join("generations").join(format!("{number}.json"));
    let text: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    (text["buckets"].as_str().unwrap().as_bytes().chunks(64))
        .map(|digest| String::from_utf8_lossy(digest).into_owned())
        .collect()
}

/// Two run ids of different buckets.
fn two_runs_of_two_buckets() -> (String, String) {
    let first = "run-0".to_owned();
    let second = (1..)
        .map(|n| format!("run-{n}"))
        .find(|run| buckets::of_run(run) != buckets::of_run(&first))
        .unwrap();
    (first, second)
}

#[test]
fn a_publication_writes_only_the_buckets_it_changed() {
    let dir = Scratch::new("buckets-changed");
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let (one, two) = two_runs_of_two_buckets();
    let mut publication = store.begin().unwrap();
    publication
        .put_in(&one, "record.json", Kind::Diagnostic, b"one")
        .unwrap();
    publication
        .create("requests/a/admitted", Kind::Diagnostic, b"a")
        .unwrap();
    publication.publish().unwrap();
    let before = packs_by_bucket(&root);

    let mut publication = store.begin().unwrap();
    publication
        .put_in(&two, "record.json", Kind::Diagnostic, b"two")
        .unwrap();
    publication.publish().unwrap();
    let after = packs_by_bucket(&root);
    // Only the bucket of the second run is in another pack now, and that pack holds it
    // alone; every other bucket is named as before.
    let at = buckets::of_run(&two) as usize;
    for (index, (was, is)) in before.iter().zip(&after).enumerate() {
        match index == at {
            true => assert_ne!(was, is),
            false => assert_eq!(was, is, "bucket {index}"),
        }
    }
    let pack = Digest::from_hex(&after[at]).unwrap();
    assert_eq!(pack_buckets(&root, &pack), BTreeSet::from([at as u8]));
    // A publication of fixed names alone writes no pack.
    let objects = std::fs::read_dir(root.join("objects")).unwrap().count();
    let packs = packs_on_disk(&root).len();
    let mut publication = store.begin().unwrap();
    publication.put("fixed", b"fixed").unwrap();
    publication.publish().unwrap();
    assert_eq!(packs_by_bucket(&root), after);
    let now = std::fs::read_dir(root.join("objects")).unwrap().count();
    assert_eq!(now, objects + 1, "the content of the fixed name");
    assert_eq!(packs_on_disk(&root).len(), packs, "and no pack");
}

/// A store of `paid` runs as the engine API leaves them: each with its record, journal
/// and output, an upload marked as imported, its request id's two records and a paid
/// attempt's names in the ledger. All but the last are made in one publication, so the
/// measurement is of the store at that size; the generation file of the last one, the
/// publication of one more run, is returned with its number.
fn paid_runs(root: &Path, paid: usize) -> (u64, u64) {
    let store = Store::open(root, Arc::new(RealFs)).unwrap();
    let made = |publication: &mut whizbang_core::store::Publication<'_>, at: usize| {
        let run = format!("1795275290505-{at}");
        publication.create_run(&run).unwrap();
        for (name, kind) in [
            ("record.json", Kind::Diagnostic),
            ("journal.jsonl", Kind::Diagnostic),
            ("outputs/mask", Kind::Output),
        ] {
            // One content for every name: what grows is the names, and a content
            // written once is synced once.
            publication.put_in(&run, name, kind, b"content").unwrap();
        }
        let upload = format!("artifacts/image/{}", Digest::of_bytes(run.as_bytes()).hex());
        publication.put(&upload, b"content").unwrap();
        publication.mark_imported(&upload).unwrap();
        for name in [
            format!("requests/app-{at:06}/admitted"),
            format!("requests/app-{at:06}/ended"),
            format!("spend/reservations/{run}"),
            format!("spend/held/1/{run}/180000"),
            format!("spend/turn/1/{}", at + 1),
            format!("spend/place/{run}/1"),
            format!("spend/dispatched/{run}/1/segment/180000"),
            format!("spend/settling/{run}/1"),
            format!("spend/settled/{run}/1/answered/120000"),
            format!("spend/closed/{run}/released"),
        ] {
            publication
                .create(&name, Kind::Diagnostic, b"content")
                .unwrap();
        }
    };
    let mut publication = store.begin().unwrap();
    for at in 0..paid {
        made(&mut publication, at);
    }
    publication.publish().unwrap();
    let mut publication = store.begin().unwrap();
    made(&mut publication, paid);
    let published = publication.publish().unwrap().generation;
    let file = root.join("generations").join(format!("{published}.json"));
    (published, std::fs::metadata(file).unwrap().len())
}

#[test]
fn the_generation_file_is_the_same_size_at_100_and_at_2000_paid_runs() {
    let dir = Scratch::new("buckets-flat");
    let (at_100, small) = paid_runs(&dir.path().join("small"), 100);
    let (at_2000, large) = paid_runs(&dir.path().join("large"), 2_000);
    // The same number, so the same digits: the file holds no other thing that grows.
    assert_eq!((at_100, at_2000), (2, 2));
    assert_eq!(small, large);
    // About 18 KB: 256 digests and the rest of the file.
    assert!(small < 20_000, "{small}");
}

/// The file of a pack, by its digest: beside the generation files (ADR 0047).
fn pack_file(root: &Path, digest: &str) -> PathBuf {
    root.join("generations").join(format!("{digest}.pack"))
}

/// The packs on disk, by their digests, with their sizes.
fn packs_on_disk(root: &Path) -> BTreeMap<String, u64> {
    std::fs::read_dir(root.join("generations"))
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let digest = name.strip_suffix(".pack")?.to_owned();
            Some((digest, entry.metadata().unwrap().len()))
        })
        .collect()
}

/// The bytes of the buckets a generation names, each as its pack holds it.
fn bucket_bytes(root: &Path) -> u64 {
    let by_bucket = packs_by_bucket(root);
    let mut bytes = 0;
    for (at, digest) in by_bucket.iter().enumerate() {
        if digest.bytes().all(|digit| digit == b'0') {
            continue;
        }
        let text = std::fs::read(pack_file(root, digest)).unwrap();
        let pack: serde_json::Value = serde_json::from_slice(&text).unwrap();
        let bucket = &pack["buckets"][at.to_string()];
        bytes += serde_json::to_vec_pretty(bucket).unwrap().len() as u64;
    }
    bytes
}

#[test]
fn a_thousand_publications_leave_only_the_current_and_held_generations_buckets() {
    let dir = Scratch::new("buckets-thousand");
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(whizbang_testkit::stored::Unsynced)).unwrap();
    let mut held = None;
    let mut held_packs = BTreeSet::new();
    for number in 1..=1_000u64 {
        let mut publication = store.begin().unwrap();
        // A run a publication, and every tenth a name of a run before it changed.
        publication
            .put_in(
                &format!("run-{number}"),
                "out",
                Kind::Output,
                &number.to_be_bytes(),
            )
            .unwrap();
        if number % 10 == 0 {
            let earlier = format!("run-{}", number / 2);
            publication
                .put_in(&earlier, "later", Kind::Diagnostic, &number.to_le_bytes())
                .unwrap();
        }
        assert_eq!(publication.publish().unwrap().generation, number);
        if number == 400 {
            let holding = store.hold().unwrap();
            held_packs = holding
                .generation()
                .bucket_digests()
                .map(|d| d.hex())
                .collect();
            held = Some(holding);
        }
        // Every hundredth: the packs on disk are those of the current generation and
        // of the held one, and no others.
        if number % 100 == 0 {
            let current: BTreeSet<String> = (store.snapshot().unwrap().bucket_digests())
                .map(|d| d.hex())
                .collect();
            let on_disk: BTreeSet<String> = packs_on_disk(&root).into_keys().collect();
            let mut expected = current.clone();
            if held.is_some() {
                expected.extend(held_packs.iter().cloned());
            }
            assert_eq!(on_disk, expected, "after {number}");
        }
    }
    let kept: u64 = packs_on_disk(&root).values().sum();
    // The held generation is read whole, 600 publications on.
    let holding = held.take().unwrap();
    for entry in holding.generation().references() {
        store.read(entry).unwrap();
    }
    drop(holding);
    // Let go: the next publication removes what only it named, and no prune is needed.
    let mut publication = store.begin().unwrap();
    publication
        .put_in("run-last", "out", Kind::Output, b"last")
        .unwrap();
    publication.publish().unwrap();
    let current: BTreeSet<String> = (store.snapshot().unwrap().bucket_digests())
        .map(|d| d.hex())
        .collect();
    let on_disk = packs_on_disk(&root);
    assert_eq!(on_disk.keys().cloned().collect::<BTreeSet<_>>(), current);
    // What the packs keep is at most twice the buckets the generation names, and a pack
    // of their worth: a pack is carried over once fewer than half of its buckets are
    // named (ADR 0047).
    let left: u64 = on_disk.values().sum();
    let live = bucket_bytes(&root);
    eprintln!(
        "1,001 publications: {kept} bytes of packs with a generation held, {left} after it \
         was let go, for {live} bytes of buckets"
    );
    assert!(left <= 2 * live + 64 * 1024, "{left} bytes kept for {live}");
    assert_eq!(store.prune().unwrap().buckets_removed, 0);
}

#[test]
fn a_build_before_buckets_refuses_the_new_format_by_its_version() {
    let dir = Scratch::new("buckets-version");
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put_in("r", "record.json", Kind::Diagnostic, b"r")
        .unwrap();
    publication.publish().unwrap();
    drop(store);
    // A build before buckets reads the format first, by itself, and refuses any other
    // than 4 as another format, naming it, before it reads the file's shape
    // (`Files::generation` at 9cbff430): so the file says 5 first.
    let file = root.join("generations/1.json");
    #[derive(serde::Deserialize)]
    struct Format {
        format: u32,
    }
    let Format { format } = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(format, 5);
    assert_ne!(format, WHOLE_FORMAT, "a build before buckets reads 4 alone");
    // And this build refuses a format after its own the same way, naming it.
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    std::fs::write(&file, text.replacen("\"format\": 5", "\"format\": 6", 1)).unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let refused = reader.snapshot().unwrap_err();
    assert!(
        matches!(
            &refused,
            Error::OtherFormat {
                format: 6,
                reads: 5,
                ..
            }
        ),
        "{refused:?}"
    );
    assert!(
        refused
            .to_string()
            .ends_with("of another format: 6, and this build reads formats 4 and 5"),
        "{refused}"
    );
}

#[test]
fn a_generation_or_a_pack_that_is_not_what_it_says_is_damage() {
    let dir = Scratch::new("buckets-damage");
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(RealFs)).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put_in("r", "record.json", Kind::Diagnostic, b"r")
        .unwrap();
    publication.publish().unwrap();
    drop(store);
    let file = root.join("generations/1.json");
    let text = std::fs::read_to_string(&file).unwrap();
    let pack = (packs_by_bucket(&root).into_iter())
        .find(|digest| !digest.bytes().all(|digit| digit == b'0'))
        .unwrap();
    let pack_path = pack_file(&root, &pack);
    let pack_text = std::fs::read_to_string(&pack_path).unwrap();
    let at = buckets::of_run("r");
    let other = at.wrapping_add(1);
    for (what, generation, packed) in [
        (
            "a bucket in a pack that is not there",
            text.replacen(&pack, &"1".repeat(64), 1),
            pack_text.clone(),
        ),
        (
            "fewer than 256 buckets",
            text.replacen(&"0".repeat(64), "", 1),
            pack_text.clone(),
        ),
        (
            "a fixed name that belongs in a bucket",
            text.replacen("\"entries\": {}", &format!(
                "\"entries\": {{\"requests/x/admitted\": {{\"digest\": \"{pack}\", \"bytes\": 1, \"kind\": \"diagnostic\"}}}}"
            ), 1),
            pack_text.clone(),
        ),
        (
            "a pack whose bytes are not its digest's",
            text.clone(),
            pack_text.replace("record.json", "record.jsom"),
        ),
    ] {
        std::fs::remove_file(&file).unwrap();
        std::fs::write(&file, &generation).unwrap();
        std::fs::remove_file(&pack_path).unwrap();
        std::fs::write(&pack_path, &packed).unwrap();
        let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
        let refused = reader.snapshot().map(|_| ());
        assert!(matches!(refused, Err(Error::Corrupt { .. })), "{what}: {refused:?}");
    }
    // A pack that holds the run in another bucket's place, named by its own digest.
    let moved = pack_text.replacen(&format!("\"{at}\""), &format!("\"{other}\""), 1);
    let moved_digest = Digest::of_bytes(moved.as_bytes()).hex();
    std::fs::write(pack_file(&root, &moved_digest), &moved).unwrap();
    std::fs::remove_file(&file).unwrap();
    std::fs::write(&file, text.replacen(&pack, &moved_digest, 1)).unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let refused = reader.snapshot().map(|_| ());
    assert!(matches!(refused, Err(Error::Corrupt { .. })), "{refused:?}");
}

#[test]
fn a_crash_at_any_event_of_the_first_bucket_publication_leaves_one_whole_generation() {
    let (names, runs) = the_fixtures_names();
    // The first publication onto the store of format 4: one run more, of content the
    // store holds already, so that what it writes is the pack, the generation file and
    // the pointer; then the removal of the older generation's file.
    let first = |root: &Path, fs: Arc<ShimFs>| {
        let store = Store::open(root, fs)?;
        let mut publication = store.begin()?;
        publication.put_in(
            "1795275290505-9",
            "record.json",
            Kind::Diagnostic,
            b"1795275290505-0",
        )?;
        publication.publish()
    };
    let count = {
        let dir = Scratch::new("buckets-crash");
        let root = copied(&dir);
        let fs = Arc::new(ShimFs::new());
        first(&root, fs.clone()).unwrap();
        fs.events().len()
    };
    // Every loss a crash can bring with no reader; with another process's reader holding
    // the old generation, the two extremes, every name lost and every name kept.
    let extremes = |case: &dyn Fn(Lost) -> (usize, usize)| {
        case(Lost::Unsynced);
        case(Lost::ContentOnly);
    };
    for held in [false, true] {
        for crash_at in 0..=count {
            let sweep = |case: &dyn Fn(Lost) -> (usize, usize)| match held {
                false => for_every_loss(case),
                true => extremes(case),
            };
            sweep(&|lost| {
                let case = format!("held {held}, {lost:?}, crash before event {crash_at}");
                let dir = Scratch::new("buckets-crash");
                let root = copied(&dir);
                // Another process's reader, which holds generation 8 throughout.
                let reader = held.then(|| Reader::open(&root, Arc::new(RealFs)).unwrap());
                let snapshot = reader.as_ref().map(|reader| reader.snapshot().unwrap());
                let fs = Arc::new(ShimFs::new().crash_before(crash_at, lost));
                let result = first(&root, fs.clone());
                fs.crash(lost);
                // The next writer recovers it: one whole generation, 8 or 9, with every
                // request id and every name of the ledger, each read whole.
                let next = Arc::new(ShimFs::new());
                let store = Store::open(&root, next).unwrap();
                let generation = store.snapshot().unwrap();
                for entry in generation.references() {
                    store.read(entry).unwrap();
                }
                let found: BTreeSet<String> = generation.entries().keys().cloned().collect();
                assert_eq!(found, names, "{case}");
                let found: BTreeSet<String> = generation.runs().keys().cloned().collect();
                match generation.number() {
                    8 => assert_eq!(found, runs, "{case}"),
                    9 => {
                        assert_eq!(found.len(), runs.len() + 1, "{case}");
                        assert_eq!(format_of(&root, 9), GENERATION_FORMAT, "{case}");
                    }
                    other => panic!("{case}: generation {other}"),
                }
                if result.is_ok() {
                    assert_eq!(generation.number(), 9, "{case}: success was reported");
                }
                // And it publishes on top of what it found.
                let mut publication = store.begin().unwrap();
                publication
                    .put_in("after", "out", Kind::Output, b"after")
                    .unwrap();
                publication.publish().unwrap();
                // What the held snapshot names is kept, and read whole.
                if let (Some(reader), Some(snapshot)) = (reader, snapshot) {
                    for entry in snapshot.references() {
                        reader.read(entry).unwrap();
                    }
                }
                losses(&fs)
            });
        }
    }
}

#[test]
fn a_pack_whose_sync_failed_is_synced_before_a_publication_names_it() {
    // The sync of a pack fails after its bytes were written whole: the publication is
    // refused. The same publication again finds the whole file under the pack's digest,
    // and syncs it before the generation that names it is written; so a crash that
    // keeps the names and loses unsynced content leaves a store that opens and reads.
    use whizbang_testkit::fs_shim::Event;
    let dir = Scratch::new("buckets-unsynced");
    let root = dir.path().join("store");
    let fs = Arc::new(ShimFs::new());
    let store = Store::open(&root, fs.clone()).unwrap();
    let is_pack_sync = |event: &Event| matches!(event, Event::SyncFile(path) if path.extension().is_some_and(|e| e == "pack"));
    fs.refuse_matching(0, is_pack_sync);
    let publish = || {
        let mut publication = store.begin().unwrap();
        publication
            .put_in("run-1", "record.json", Kind::Diagnostic, b"a record")
            .unwrap();
        publication.publish()
    };
    assert!(publish().is_err(), "the pack's sync was refused");
    assert_eq!(publish().unwrap().generation, 1);
    let synced = fs
        .events()
        .into_iter()
        .filter(|event| is_pack_sync(event))
        .count();
    // The refused call is not recorded: the one sync is the second publication's, of
    // the whole pack it found.
    assert_eq!(synced, 1, "the second publication synced the pack it found");
    drop(store);
    fs.crash(Lost::ContentOnly);
    let store = Store::open(&root, Arc::new(ShimFs::new())).unwrap();
    let generation = store.snapshot().unwrap();
    let record = generation.run("run-1").unwrap().get("record.json").unwrap();
    assert_eq!(store.read(record).unwrap(), b"a record");
}

/// The real file system, which syncs no file it did not write, and notes by file name
/// what it was asked to write, rename and remove among the packs.
#[derive(Default)]
struct SyncsNoFoundFile {
    seen: std::sync::Mutex<Vec<String>>,
}

impl SyncsNoFoundFile {
    fn note(&self, what: &str, paths: &[&Path]) {
        let names: Vec<String> = (paths.iter())
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        if names.iter().any(|name| name.contains(".pack")) {
            let line = format!("{what} {}", names.join(" "));
            self.seen.lock().unwrap().push(line);
        }
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

impl whizbang_core::store::fs::Fs for SyncsNoFoundFile {
    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        RealFs.create_dir(path)
    }
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.note("write", &[path]);
        RealFs.write_new(path, bytes)
    }
    fn sync_dir(&self, path: &Path) -> std::io::Result<()> {
        RealFs.sync_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.note("rename", &[from, to]);
        RealFs.rename(from, to)
    }
    fn remove_all(&self, path: &Path) -> std::io::Result<()> {
        self.note("remove", &[path]);
        RealFs.remove_all(path)
    }
    fn read_at_most(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
        RealFs.read_at_most(path, limit)
    }
    fn list(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        RealFs.list(path)
    }
    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        RealFs.exists(path)
    }
    fn hold_lock_file(
        &self,
        path: &Path,
        create: bool,
    ) -> std::io::Result<Option<Box<dyn whizbang_core::store::fs::LockFile>>> {
        RealFs.hold_lock_file(path, create)
    }
    fn share(&self, path: &Path) -> std::io::Result<Box<dyn whizbang_core::store::fs::Held>> {
        RealFs.share(path)
    }
    fn try_alone(
        &self,
        path: &Path,
    ) -> std::io::Result<Option<Box<dyn whizbang_core::store::fs::Held>>> {
        RealFs.try_alone(path)
    }
}

#[test]
fn on_a_file_system_that_syncs_no_found_file_a_whole_pack_is_written_again_beside_and_renamed_over()
{
    // A pack a held generation names is found whole by a later publication that writes
    // the same bucket again. The file system cannot sync a file it did not write: the
    // pack is written again beside it and renamed over it, so the held generation finds
    // it whole throughout, and nothing is left beside it.
    let dir = Scratch::new("buckets-no-sync");
    let root = dir.path().join("store");
    let fs = Arc::new(SyncsNoFoundFile::default());
    let store = Store::open(&root, fs.clone()).unwrap();
    let publish = |store: &Store| {
        let mut publication = store.begin().unwrap();
        publication
            .put_in("run-1", "record.json", Kind::Diagnostic, b"a record")
            .unwrap();
        publication.publish().unwrap()
    };
    publish(&store);
    let held = store.hold().unwrap();
    let pack = held.generation().bucket_digests().next().unwrap().hex();
    store.delete(&["run-1"]).unwrap();
    let before = fs.seen().len();
    publish(&store);
    // The pack was never removed, nor written in its place: it was there throughout,
    // and what took its place is the file written beside it, renamed over it.
    let replacing: Vec<String> = (fs.seen().into_iter().skip(before))
        .filter(|line| line.contains(&pack))
        .collect();
    assert_eq!(
        replacing,
        [
            format!("write {pack}.pack.new"),
            format!("rename {pack}.pack.new {pack}.pack"),
        ]
    );
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .bucket_digests()
            .next()
            .unwrap()
            .hex(),
        pack
    );
    for entry in held.generation().references() {
        store.read(entry).unwrap();
    }
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    reader.snapshot().unwrap();
    let beside: Vec<String> = std::fs::read_dir(root.join("generations"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".new"))
        .collect();
    assert!(beside.is_empty(), "{beside:?}");
}

#[test]
fn a_prune_removes_a_pack_written_beside_another_and_left_there_by_a_crash() {
    // A crash between the write of a pack beside the one it replaces and its rename
    // over it leaves the file beside: the next prune removes it, and the pack stays.
    let dir = Scratch::new("buckets-left-beside");
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(SyncsNoFoundFile::default())).unwrap();
    let mut publication = store.begin().unwrap();
    publication
        .put_in("run-1", "record.json", Kind::Diagnostic, b"a record")
        .unwrap();
    publication.publish().unwrap();
    let pack = store
        .snapshot()
        .unwrap()
        .bucket_digests()
        .next()
        .unwrap()
        .hex();
    let generations = root.join("generations");
    let beside = generations.join(format!("{pack}.pack.new"));
    std::fs::write(&beside, b"a pack the crash left half written").unwrap();
    let pruned = store.prune().unwrap();
    assert_eq!(pruned.buckets_removed, 1);
    assert!(!beside.exists());
    assert!(generations.join(format!("{pack}.pack")).exists());
    let generation = store.snapshot().unwrap();
    let record = generation.run("run-1").unwrap().get("record.json").unwrap();
    assert_eq!(store.read(record).unwrap(), b"a record");
}
