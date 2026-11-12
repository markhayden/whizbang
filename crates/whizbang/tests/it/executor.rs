//! The executor as the binary builds it, with the testkit's `echo` added by the test.

use std::sync::Arc;

use whizbang_core::contract::{Call, CallError, Executor, Readiness, Refusal, Value};
use whizbang_core::store::fs::RealFs;
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::echo::Echo;
use whizbang_testkit::scratch::Scratch;

fn reader(dir: &Scratch) -> Arc<Reader> {
    let root = dir.path().join("store");
    drop(Store::open(&root, Arc::new(RealFs)).expect("a store"));
    Arc::new(Reader::open(&root, Arc::new(RealFs)).expect("a reader"))
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
    // The entry point takes it as far as there is anything built to take it.
    assert_eq!(
        executor.submit(&call),
        Err(CallError::Refused(Refusal::NothingRunsYet))
    );
    assert!(matches!(
        executor.submit(&Call::new("echo")),
        Err(CallError::Refused(Refusal::Invalid { .. }))
    ));
}
