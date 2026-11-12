//! Artifacts in the store: created, stored, read back, and the same.

use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, ArtifactError, ArtifactId, ArtifactType, Step};
use whizbang_core::identity::Digest;
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{self, Kind, Reader, Store};
use whizbang_testkit::scratch::Scratch;

use super::support::*;

fn open(dir: &Scratch, name: &str) -> Store {
    Store::open(&dir.path().join(name), Arc::new(RealFs)).unwrap()
}

fn store_all(store: &Store, artifacts: &[Artifact]) {
    let mut publication = store.begin().unwrap();
    for artifact in artifacts {
        assert_eq!(
            artifact::put(&mut publication, artifact).unwrap(),
            artifact.id()
        );
    }
    publication.publish().unwrap();
}

#[test]
fn each_type_is_created_stored_read_back_and_has_the_same_identity() {
    let dir = Scratch::new("artifacts");
    let store = open(&dir, "store");
    let artifacts = one_of_each();
    store_all(&store, &artifacts);
    drop(store);

    // Read by another process's view of the store: a reader, after the writer is gone.
    let reader = Reader::open(&dir.path().join("store"), Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    assert_eq!(generation.entries().len(), 6);
    for artifact in &artifacts {
        let (kind, id) = (artifact.artifact_type(), artifact.id());
        assert!(artifact::holds(&generation, kind, id));
        let name = artifact::name(kind, id);
        assert_eq!(name, format!("artifacts/{}/{}", kind.name(), id.0.hex()));
        // What the store holds is the canonical file, byte for byte.
        let entry = generation.get(&name).unwrap();
        assert_eq!(reader.read(entry).unwrap(), artifact.to_bytes());
        let read = artifact::get(&reader, kind, id, &limits())
            .unwrap_or_else(|error| panic!("{kind}: {error}"));
        assert_eq!(read.id(), id);
        assert_eq!(read, *artifact);
        assert_eq!(read.origin(), artifact.origin());
    }
}

#[test]
fn the_same_mask_written_in_two_stores_has_the_same_identity() {
    let dir = Scratch::new("artifacts");
    let mask = full_mask(on("photo"));

    // Two stores with different histories: one is new, the other has published before
    // and holds other things.
    let first = open(&dir, "first");
    store_all(&first, &[mask.clone().into()]);
    let second = open(&dir, "second");
    for round in 0..3 {
        let mut publication = second.begin().unwrap();
        publication
            .put(&format!("something/{round}"), format!("{round}").as_bytes())
            .unwrap();
        publication.publish().unwrap();
    }
    // The second store is given the mask as a file from the first: an export and an
    // import.
    let exported = first
        .read(
            first
                .snapshot()
                .unwrap()
                .get(&artifact::name(ArtifactType::Mask, mask.id()))
                .unwrap(),
        )
        .unwrap();
    let imported = Artifact::from_bytes(&exported, &limits()).unwrap();
    store_all(&second, &[imported]);

    let from_first = artifact::get(&first, ArtifactType::Mask, mask.id(), &limits()).unwrap();
    let from_second = artifact::get(&second, ArtifactType::Mask, mask.id(), &limits()).unwrap();
    assert_eq!(from_first.id(), mask.id());
    assert_eq!(from_second.id(), mask.id());
    // Nothing was lost on the way: the masks are equal in every part, and so are the
    // bytes the two stores hold.
    assert_eq!(from_first, from_second);
    assert_eq!(from_second.into_mask().unwrap(), mask);
    assert_eq!(first.snapshot().unwrap().number(), 1);
    assert_eq!(second.snapshot().unwrap().number(), 4);

    // The same mask made again in the second store, by a run that tells its origin
    // another way, is still the same mask.
    let mut remade = full_mask(on("photo"));
    remade.record(Step::new("later", "1")).unwrap();
    assert_eq!(remade.id(), mask.id());
}

#[test]
fn a_rendering_in_the_store_is_refused_where_a_mask_is_required() {
    let dir = Scratch::new("artifacts");
    let store = open(&dir, "store");
    let mask = rect_mask(on("photo"));
    let rendering = rendering_of(&mask);
    store_all(&store, &[mask.clone().into(), rendering.clone().into()]);

    // Asked for as a mask, by the rendering's identity.
    let refusal = artifact::get(&store, ArtifactType::Mask, rendering.id(), &limits());
    assert!(
        matches!(
            &refusal,
            Err(ArtifactError::WrongType { expected: ArtifactType::Mask, found }) if found == "an image"
        ),
        "{refusal:?}"
    );
    // Taken as what it is, it still does not become a mask.
    let taken = artifact::get(&store, ArtifactType::Image, rendering.id(), &limits()).unwrap();
    assert!(matches!(
        taken.into_mask(),
        Err(ArtifactError::WrongType { .. })
    ));
    // The mask is there as a mask, and as nothing else.
    let taken = artifact::get(&store, ArtifactType::Mask, mask.id(), &limits()).unwrap();
    assert_eq!(taken.into_mask().unwrap(), mask);
    assert!(matches!(
        artifact::get(&store, ArtifactType::Image, mask.id(), &limits()),
        Err(ArtifactError::WrongType { .. })
    ));
    // What the store does not hold is missing, not mistaken.
    let absent = ArtifactId(Digest::of_bytes(b"nothing"));
    assert!(matches!(
        artifact::get(&store, ArtifactType::Mask, absent, &limits()),
        Err(ArtifactError::Missing { artifact: ArtifactType::Mask, id }) if id == absent
    ));
}

#[test]
fn what_is_stored_under_an_artifacts_name_must_be_that_artifact() {
    let dir = Scratch::new("artifacts");
    let store = open(&dir, "store");
    let mask = rect_mask(on("photo"));
    let other = rect_mask(on("another photo"));
    let rendering = rendering_of(&mask);
    let mut publication = store.begin().unwrap();
    // Another mask, a rendering's file, and a picture, each under a mask's name.
    let under = |n: u8| ArtifactId(Digest::of_bytes(&[n]));
    let name = |n: u8| artifact::name(ArtifactType::Mask, under(n));
    publication.put(&name(1), &other.to_bytes()).unwrap();
    publication.put(&name(2), &rendering.to_bytes()).unwrap();
    publication.put(&name(3), rendering.file()).unwrap();
    publication.publish().unwrap();

    assert!(matches!(
        artifact::get(&store, ArtifactType::Mask, under(1), &limits()),
        Err(ArtifactError::Format(_))
    ));
    for n in [2, 3] {
        assert!(matches!(
            artifact::get(&store, ArtifactType::Mask, under(n), &limits()),
            Err(ArtifactError::WrongType { .. })
        ));
    }
}

#[test]
fn an_artifact_is_stored_as_an_output_and_a_diagnostic_of_its_name_is_not_taken_for_it() {
    let dir = Scratch::new("artifacts");
    let root = dir.path().join("store");
    let store = open(&dir, "store");
    let artifact = one_of_each().remove(0);
    let (kind, id) = (artifact.artifact_type(), artifact.id());
    store_all(&store, std::slice::from_ref(&artifact));
    let generation = store.snapshot().unwrap();
    let entry = generation.get(&artifact::name(kind, id)).unwrap();
    assert_eq!(
        entry.kind,
        Kind::Output,
        "an artifact is a committed output"
    );
    assert_eq!(
        artifact::get(&store, kind, id, &limits()).unwrap(),
        artifact
    );
    drop(store);

    // The same reference, said to be a diagnostic: the same bytes under the same name.
    // It is read as a diagnostic is, and it is not an artifact to anyone who asks for one.
    let file = root.join("generations/1.json");
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("\"kind\": \"output\"").count(), 1);
    std::fs::write(
        &file,
        text.replace("\"kind\": \"output\"", "\"kind\": \"diagnostic\""),
    )
    .unwrap();
    let reader = Reader::open(&root, Arc::new(RealFs)).unwrap();
    let writer = Store::open(&root, Arc::new(RealFs)).unwrap();
    let generation = reader.snapshot().unwrap();
    let entry = generation.get(&artifact::name(kind, id)).unwrap();
    assert_eq!(reader.read(entry).unwrap(), artifact.to_bytes());
    let by_reader = artifact::get(&reader, kind, id, &limits());
    let by_writer = artifact::get(&writer, kind, id, &limits());
    for result in [by_reader, by_writer] {
        match result {
            Err(ArtifactError::Store(store::Error::NotAnOutput { digest })) => {
                assert_eq!(digest, entry.digest);
            }
            other => panic!("{other:?}"),
        }
    }
}
