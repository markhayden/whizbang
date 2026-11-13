//! Each of these makes one thread, and joins it, in a way `clippy.toml` refuses. The
//! file is linted, never run.

#[test]
fn a_thread_made_by_spawn() {
    std::thread::spawn(|| ()).join().unwrap();
}

#[test]
fn a_thread_made_in_a_scope() {
    std::thread::scope(|scope| {
        scope.spawn(|| ());
    });
}

#[test]
fn a_thread_made_by_a_builder() {
    let made = std::thread::Builder::new().spawn(|| ()).unwrap();
    made.join().unwrap();
}
