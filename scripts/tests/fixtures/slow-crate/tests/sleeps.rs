//! Sleeps for `SLOW_CRATE_SLEEP_SECS` seconds. The one place a test may sleep: it is the
//! thing the timeout is checked against.

#[test]
fn sleeps() {
    let secs = std::env::var("SLOW_CRATE_SLEEP_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(31);
    std::thread::sleep(std::time::Duration::from_secs(secs));
}
