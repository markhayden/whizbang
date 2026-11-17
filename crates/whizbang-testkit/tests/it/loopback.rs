//! The listener on loopback: it serves the connections it was told to, counts them, and
//! is stopped and joined on every path, a blocked accept included.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use whizbang_testkit::loopback::{CONNECTIONS_LIMIT, Listener, WAIT};
use whizbang_testkit::threads;

const PATIENCE: Duration = Duration::from_secs(5);

fn connect(listener: &Listener) -> TcpStream {
    let stream = TcpStream::connect_timeout(&listener.address(), PATIENCE).unwrap();
    stream.set_read_timeout(Some(PATIENCE)).unwrap();
    stream
}

#[test]
fn the_listener_serves_its_connections_on_loopback_and_counts_them() {
    let listener = Listener::serve(2, |number, mut stream| {
        stream
            .write_all(&[b'a' + u8::try_from(number).unwrap()])
            .unwrap();
    });
    assert!(listener.address().ip().is_loopback());
    for expected in *b"ab" {
        let mut answer = Vec::new();
        connect(&listener).read_to_end(&mut answer).unwrap();
        assert_eq!(answer, [expected]);
    }
    assert_eq!(listener.finish(), 2);
}

#[test]
fn a_listener_that_was_asked_nothing_stops_at_once_and_counts_no_connection() {
    // Its accept is blocked; the stop's own connection wakes it and is not counted.
    let before = threads::live();
    let listener = Listener::serve(CONNECTIONS_LIMIT, |_, _| panic!("nothing is served"));
    assert_eq!(listener.finish(), 0);
    assert!(threads::live() <= before);
}

#[test]
fn a_listener_dropped_in_a_panic_is_stopped_and_joined() {
    let before = threads::live();
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _listener = Listener::serve(1, |_, _| {});
        panic!("the test failed while its listener waited");
    }));
    assert!(unwound.is_err());
    assert!(threads::live() <= before);
}

#[test]
fn a_panic_of_what_serves_is_raised_by_finish() {
    let listener = Listener::serve(1, |_, _| panic!("the served connection was wrong"));
    served(&listener);
    let raised = catch_unwind(AssertUnwindSafe(|| listener.finish()));
    assert!(raised.is_err());
}

#[test]
fn a_served_connection_waits_for_a_read_or_a_write_no_longer_than_the_listeners_wait() {
    let listener = Listener::serve(1, |_, stream| {
        assert_eq!(stream.read_timeout().unwrap(), Some(WAIT));
        assert_eq!(stream.write_timeout().unwrap(), Some(WAIT));
    });
    served(&listener);
    assert_eq!(listener.finish(), 1);
}

/// Connects and waits until the listener has served the connection and closed it: a
/// connection still waiting to be accepted when the listener is stopped is not served.
fn served(listener: &Listener) {
    let _ = connect(listener).read_to_end(&mut Vec::new());
}

#[test]
#[should_panic(expected = "a listener serves 1 to 32 connections")]
fn a_listener_of_more_connections_than_its_limit_is_not_made() {
    let _ = Listener::serve(CONNECTIONS_LIMIT + 1, |_, _| {});
}
