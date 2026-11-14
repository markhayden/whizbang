//! The fake of the download interface (ADR 0012, `Downloader`; the test kit's
//! `downloads`): bytes from memory, a few kilobytes, handed over in pieces. Nothing is
//! fetched from anywhere.

use std::sync::{Arc, Mutex};

use whizbang_core::capability::{Fault, FaultKind};
use whizbang_core::contract::{Cancellation, Secrets};
use whizbang_core::egress::{Address, Credential, DownloadRequest, Downloaded, Downloader};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::{Ended, FakeDownloader};
use whizbang_testkit::threads;

const KEY: &str = "sk-marker-7f3a9c";

fn address(text: &str) -> Address {
    Address::of(text, &Secrets::none()).unwrap()
}

/// 2,500 bytes that are different at every place a piece could start.
fn content() -> Vec<u8> {
    (0..2500u32).map(|at| (at % 251) as u8).collect()
}

fn request(from: &Address, most: u64) -> DownloadRequest {
    DownloadRequest::new(from.clone(), most).unwrap()
}

/// Downloads into memory with a sink that takes everything.
fn all(
    fake: &FakeDownloader,
    request: &DownloadRequest,
) -> (Result<Downloaded, Fault>, Vec<u8>, Vec<usize>) {
    let mut got = Vec::new();
    let mut pieces = Vec::new();
    let ended = fake.download(request, &Cancellation::new(), &mut |piece| {
        got.extend_from_slice(piece);
        pieces.push(piece.len());
        true
    });
    (ended, got, pieces)
}

#[test]
fn the_fake_serves_a_file_from_memory_in_pieces_in_order() {
    let from = address("https://weights.example.org/files/graph.onnx");
    let fake = FakeDownloader::new().serving(&from, content());
    let (ended, got, pieces) = all(&fake, &request(&from, 4096));
    assert_eq!(ended.unwrap(), Downloaded::Bytes(2500));
    assert_eq!(got, content());
    // Pieces of 1,024 bytes, and what is left.
    assert_eq!(pieces, [1024, 1024, 452]);
    // In pieces of another size, when told.
    let fake = FakeDownloader::new()
        .in_pieces_of(1000)
        .serving(&from, content());
    let (ended, got, pieces) = all(&fake, &request(&from, 4096));
    assert_eq!(ended.unwrap(), Downloaded::Bytes(2500));
    assert_eq!(got, content());
    assert_eq!(pieces, [1000, 1000, 500]);
    // A piece is one byte at least, whatever it is told.
    let fake = FakeDownloader::new()
        .in_pieces_of(0)
        .serving(&from, b"abc".to_vec());
    let (ended, got, pieces) = all(&fake, &request(&from, 4096));
    assert_eq!(
        (ended.unwrap(), got),
        (Downloaded::Bytes(3), b"abc".to_vec())
    );
    assert_eq!(pieces, [1, 1, 1]);
}

#[test]
fn told_twice_of_one_address_the_fake_does_what_it_was_told_last() {
    let from = address("https://weights.example.org/f");
    let fake = FakeDownloader::new()
        .serving(&from, b"first".to_vec())
        .serving(&from, b"second".to_vec());
    let (ended, got, _) = all(&fake, &request(&from, 4096));
    assert_eq!(
        (ended.unwrap(), got),
        (Downloaded::Bytes(6), b"second".to_vec())
    );
    assert!(format!("{fake:?}").contains("1 addresses"));
}

#[test]
fn the_fake_hands_over_no_more_than_the_most_it_was_asked_for() {
    let from = address("https://weights.example.org/f");
    let fake = FakeDownloader::new().serving(&from, content());
    let (ended, got, pieces) = all(&fake, &request(&from, 1500));
    assert_eq!(ended.unwrap(), Downloaded::Bytes(1500));
    assert_eq!(got, content()[..1500]);
    assert_eq!(pieces, [1024, 476]);
    assert_eq!(fake.handed(&from), 1500);
    // Asked for exactly what there is, and for one byte.
    let (ended, got, _) = all(&fake, &request(&from, 2500));
    assert_eq!((ended.unwrap(), got.len()), (Downloaded::Bytes(2500), 2500));
    let (ended, got, _) = all(&fake, &request(&from, 1));
    assert_eq!((ended.unwrap(), got), (Downloaded::Bytes(1), vec![0]));
    // And for one byte fewer than there is.
    let (ended, got, _) = all(&fake, &request(&from, 2499));
    assert_eq!((ended.unwrap(), got.len()), (Downloaded::Bytes(2499), 2499));
}

#[test]
fn a_fake_told_to_serve_too_many_hands_over_everything_whatever_it_was_asked_for() {
    let from = address("https://weights.example.org/f");
    let fake = FakeDownloader::new().serving_too_many(&from, content());
    let (ended, got, pieces) = all(&fake, &request(&from, 1500));
    assert_eq!(ended.unwrap(), Downloaded::Bytes(2500));
    assert_eq!(got, content());
    assert_eq!(pieces, [1024, 1024, 452]);
}

#[test]
fn a_sinks_no_ends_the_download_with_that_piece_counted_and_nothing_after_it() {
    let from = address("https://weights.example.org/f");
    let fake = FakeDownloader::new().serving(&from, content());
    let mut seen = Vec::new();
    let ended = fake.download(&request(&from, 4096), &Cancellation::new(), &mut |piece| {
        seen.push(piece.len());
        seen.len() < 2
    });
    // The second piece was handed over, and is counted; the third never was.
    assert_eq!(ended.unwrap(), Downloaded::Bytes(2048));
    assert_eq!(seen, [1024, 1024]);
    assert_eq!(fake.handed(&from), 2048);
    assert_eq!(fake.requests()[0].ended(), Ended::Stopped);
}

#[test]
fn the_fake_answers_a_file_that_moved_with_its_address_and_asks_nothing_of_it() {
    let from = address("https://weights.example.org/f");
    let to = address("https://mirror.example.net/f?sig=abc");
    let fake = FakeDownloader::new()
        .moved(&from, to.clone())
        .serving(&to, content());
    let (ended, got, _) = all(&fake, &request(&from, 4096));
    assert_eq!(ended.unwrap(), Downloaded::Moved(to.clone()));
    assert!(got.is_empty());
    // One request, to the first address; the fake followed nothing.
    assert_eq!(fake.requests().len(), 1);
    assert_eq!((fake.asked(&from), fake.asked(&to)), (1, 0));
    assert_eq!(fake.requests()[0].ended(), Ended::Moved);
    // Whoever drives the hops asks the second address itself.
    let (ended, got, _) = all(&fake, &request(&to, 4096));
    assert_eq!(ended.unwrap(), Downloaded::Bytes(2500));
    assert_eq!(got, content());
    assert_eq!((fake.asked(&from), fake.asked(&to)), (1, 1));
}

#[test]
fn the_fake_fails_at_once_or_after_a_stated_count_of_bytes() {
    let from = address("https://weights.example.org/f");
    let fault = Fault::new(FaultKind::Unavailable, "the scripted host is down");
    let fake = FakeDownloader::new().failing(&from, fault.clone());
    let (ended, got, _) = all(&fake, &request(&from, 4096));
    assert_eq!(ended.unwrap_err(), fault);
    assert!(got.is_empty());
    assert_eq!(fake.requests()[0].ended(), Ended::Failed);
    // After 1,500 bytes: exactly those were handed over, and then the fault.
    let fake = FakeDownloader::new().failing_after(&from, content(), 1500, fault.clone());
    let (ended, got, pieces) = all(&fake, &request(&from, 4096));
    assert_eq!(ended.unwrap_err(), fault);
    assert_eq!(got, content()[..1500]);
    assert_eq!(pieces, [1024, 476]);
    assert_eq!(fake.handed(&from), 1500);
    // A request for less than that is answered: it never came to the fault.
    let (ended, got, _) = all(&fake, &request(&from, 1000));
    assert_eq!((ended.unwrap(), got.len()), (Downloaded::Bytes(1000), 1000));
    // And so is one that ends exactly there: the fault is for a request that would
    // have gone on.
    let (ended, got, _) = all(&fake, &request(&from, 1500));
    assert_eq!((ended.unwrap(), got.len()), (Downloaded::Bytes(1500), 1500));
    let fake = FakeDownloader::new().failing_after(&from, content(), 2500, fault);
    let (ended, got, _) = all(&fake, &request(&from, 4096));
    assert_eq!((ended.unwrap(), got.len()), (Downloaded::Bytes(2500), 2500));
}

#[test]
fn the_fake_fails_a_request_for_an_address_it_serves_nothing_at_and_does_not_panic() {
    let fake = FakeDownloader::new();
    let from = address("https://weights.example.org/nothing");
    let (ended, got, _) = all(&fake, &request(&from, 10));
    let fault = ended.unwrap_err();
    assert_eq!(fault.kind(), FaultKind::Unavailable);
    assert!(got.is_empty());
    assert_eq!(fake.asked(&from), 1);
    // Two addresses of one origin are two addresses.
    let other = address("https://weights.example.org/f");
    let fake = FakeDownloader::new().serving(&other, content());
    assert!(all(&fake, &request(&from, 10)).0.is_err());
}

#[test]
fn the_fake_stops_at_a_barrier_after_a_stated_count_of_bytes_until_it_is_released() {
    let from = address("https://weights.example.org/f");
    let barriers = Barriers::new();
    let fake = Arc::new(FakeDownloader::new().stopping_after(
        &from,
        content(),
        1500,
        barriers.clone(),
        "mid-file",
    ));
    let got = Arc::new(Mutex::new(Vec::new()));
    let download = {
        let (fake, got, from) = (fake.clone(), got.clone(), from.clone());
        threads::spawn(move || {
            fake.download(&request(&from, 4096), &Cancellation::new(), &mut |piece| {
                got.lock().unwrap().extend_from_slice(piece);
                true
            })
        })
    };
    barriers.arrived("mid-file", 1);
    // Stopped: exactly 1,500 bytes were handed over, and the request is open.
    assert_eq!(*got.lock().unwrap(), content()[..1500]);
    assert_eq!(fake.handed(&from), 1500);
    assert_eq!(fake.requests()[0].ended(), Ended::Open);
    barriers.release("mid-file");
    assert_eq!(download.join().unwrap().unwrap(), Downloaded::Bytes(2500));
    assert_eq!(*got.lock().unwrap(), content());
    assert_eq!(fake.requests()[0].ended(), Ended::Whole);
}

#[test]
fn a_download_cancelled_while_the_fake_is_stopped_ends_as_cancelled_with_nothing_more_handed_over()
{
    let from = address("https://weights.example.org/f");
    let barriers = Barriers::new();
    let fake = Arc::new(FakeDownloader::new().stopping_after(
        &from,
        content(),
        1024,
        barriers.clone(),
        "mid-file",
    ));
    let cancellation = Cancellation::new();
    let download = {
        let (fake, from, cancellation) = (fake.clone(), from.clone(), cancellation.clone());
        threads::spawn(move || {
            let mut got = 0;
            let ended = fake.download(&request(&from, 4096), &cancellation, &mut |piece| {
                got += piece.len();
                true
            });
            (ended, got)
        })
    };
    barriers.arrived("mid-file", 1);
    cancellation.cancel();
    barriers.release("mid-file");
    let (ended, got) = download.join().unwrap();
    assert_eq!(ended.unwrap_err().kind(), FaultKind::Cancelled);
    assert_eq!(got, 1024);
    assert_eq!(fake.handed(&from), 1024);
    assert_eq!(fake.requests()[0].ended(), Ended::Failed);
    // Cancelled before it started: nothing is handed over at all.
    let fake = FakeDownloader::new().serving(&from, content());
    let mut got = 0;
    let ended = fake.download(&request(&from, 4096), &cancellation, &mut |piece| {
        got += piece.len();
        true
    });
    assert_eq!(ended.unwrap_err().kind(), FaultKind::Cancelled);
    assert_eq!(got, 0);
}

#[test]
fn the_fake_keeps_each_request_in_order_with_what_it_asked_for_and_the_headers_it_carried() {
    let first = address("https://weights.example.org/a");
    let second = address("https://weights.example.org/b");
    let fake = FakeDownloader::new()
        .serving(&first, content())
        .serving(&second, b"terms".to_vec());
    let credential = Credential::new(
        second.origin().clone(),
        "Authorization",
        KEY,
        &Secrets::none(),
    )
    .unwrap();
    let with_key = request(&second, 5).with_credential(credential).unwrap();
    for asked in [&request(&first, 2000), &with_key, &request(&first, 3000)] {
        assert!(all(&fake, asked).0.is_ok());
    }
    let kept = fake.requests();
    assert_eq!(kept.len(), 3);
    let seen: Vec<_> = kept
        .iter()
        .map(|asked| {
            (
                asked.from().clone(),
                asked.most_bytes(),
                asked.handed(),
                asked.ended(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (first.clone(), 2000, 2000, Ended::Whole),
            (second.clone(), 5, 5, Ended::Whole),
            (first.clone(), 3000, 2500, Ended::Whole),
        ]
    );
    assert_eq!(kept[0].headers(), Vec::<(String, String)>::new());
    assert_eq!(
        kept[1].headers(),
        [("Authorization".to_owned(), KEY.to_owned())]
    );
    assert_eq!((fake.asked(&first), fake.asked(&second)), (2, 1));
    assert_eq!((fake.handed(&first), fake.handed(&second)), (4500, 5));
    // What is printed of the fake and of a request holds no credential and no byte of
    // an address's rest.
    let printed = format!("{fake:?} {kept:?}");
    assert!(!printed.contains(KEY), "{printed}");
    assert!(!printed.contains("/b"), "{printed}");
    assert!(
        printed.contains("FakeDownloader(2 addresses, 3 requests)"),
        "{printed}"
    );
}

#[test]
fn the_fake_may_be_asked_by_two_threads_at_once_and_counts_both() {
    let from = address("https://weights.example.org/f");
    let fake = Arc::new(FakeDownloader::new().serving(&from, content()));
    // Two threads show that the fake is shared; each is joined.
    let downloads: Vec<_> = (0..2)
        .map(|_| {
            let (fake, from) = (fake.clone(), from.clone());
            threads::spawn(move || all(&fake, &request(&from, 4096)).1)
        })
        .collect();
    for download in downloads {
        assert_eq!(download.join().unwrap(), content());
    }
    assert_eq!((fake.asked(&from), fake.handed(&from)), (2, 5000));
}
