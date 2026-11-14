//! The executor as the binary builds it, with the testkit's `echo` added by the test.

use std::sync::Arc;

use whizbang_core::artifact::{self, Artifact, Image, ImageRole, Origin, Step};
use whizbang_core::contract::{Call, CallError, Executor, Readiness, Refusal, Value};
use whizbang_core::identity::{DecodeLimits, Projection, Topology};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::scratch::Scratch;

fn reader(dir: &Scratch) -> Arc<Reader> {
    let root = dir.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).expect("a store"));
    Arc::new(Reader::open(&root, Arc::new(RealFs)).expect("a reader"))
}

/// A picture of one pixel: the smallest PNG file there is, written out.
fn a_picture() -> Image {
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let origin = Origin::new(Step::new("fixture", "1")).expect("an origin");
    let limits = DecodeLimits::default();
    let (flat, bounded) = (Projection::Flat, Topology::Bounded);
    Image::new(
        PNG.to_vec(),
        flat,
        bounded,
        ImageRole::Picture,
        origin,
        &limits,
    )
    .expect("an image")
}

#[test]
fn the_shipping_executor_knows_no_echo() {
    let dir = Scratch::new("executor");
    let registry = whizbang::wiring::registry().expect("the registry builds");
    let executor = whizbang::wiring::executor(registry, reader(&dir));
    let call = Call::new("echo").argument("text", Value::text("hello"));
    assert_eq!(
        executor.prepare(&call),
        Err(CallError::Refused(Refusal::UnknownAction {
            name: "echo".to_owned()
        }))
    );
}

#[test]
fn the_shipping_executor_describes_check_and_cannot_run_it_for_want_of_a_judge() {
    // `check` and `view` are registered where the binary builds its registry. The
    // binary's executor holds no backend, so `check`, which needs a judge, is
    // prepared as not ready, and says what is missing; `view` needs none.
    let dir = Scratch::new("executor");
    let registry = whizbang::wiring::registry().expect("the registry builds");
    let names: Vec<&str> = registry.descriptors().map(|d| d.name()).collect();
    assert_eq!(names, ["check", "grow", "view"]);
    let root = dir.path().join("store");
    let image = {
        let store = Store::open(&root, Arc::new(RealFs)).expect("a store");
        let image = a_picture();
        let mut publication = store.begin().expect("a publication");
        artifact::put(&mut publication, &Artifact::Image(image.clone())).expect("put");
        publication.publish().expect("published");
        image.id()
    };
    let reader = Arc::new(Reader::open(&root, Arc::new(RealFs)).expect("a reader"));
    let executor = whizbang::wiring::executor(registry, reader);
    let prepared = executor
        .prepare(&Call::new("check").input("images", image))
        .expect("check is prepared");
    let Readiness::NotReady(missing) = prepared.readiness() else {
        panic!("check is ready with no judge: {:?}", prepared.readiness());
    };
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].capability().name(), "judge");
    let view = Call::new("view")
        .input("image", image)
        .argument("left", Value::Number(0.0))
        .argument("top", Value::Number(0.0))
        .argument("right", Value::Number(1.0))
        .argument("bottom", Value::Number(1.0));
    let prepared = executor.prepare(&view).expect("view is prepared");
    assert_eq!(prepared.readiness(), &Readiness::Ready);
}

#[test]
fn a_test_prepares_echo_on_the_executor_the_binary_builds() {
    let dir = Scratch::new("executor");
    let mut registry = whizbang::wiring::registry().expect("the registry builds");
    registry
        .register(Arc::new(Echo::new()))
        .expect("echo is registered");
    let executor = whizbang::wiring::executor(registry, reader(&dir));
    let call = Call::new("echo").argument("text", Value::text("hello"));
    let prepared = executor.prepare(&call).expect("echo is prepared");
    // It needs no capability, so it is ready though no backend is configured.
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    assert_eq!(
        prepared.arguments(),
        [("text".to_owned(), Value::text("hello"))]
    );
    // There is no config yet, and the binary says so with the digest of no bytes.
    assert_eq!(
        prepared.config(),
        whizbang_core::identity::Digest::of_bytes(b"")
    );
    assert_eq!(
        prepared.config().to_string(),
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    // This executor was built over a read of the store: it runs nothing.
    assert_eq!(
        executor.submit(&call),
        Err(CallError::Refused(Refusal::ReadOnly))
    );
    assert!(matches!(
        executor.submit(&Call::new("echo")),
        Err(CallError::Refused(Refusal::Invalid { .. }))
    ));
}
