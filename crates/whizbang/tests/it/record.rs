//! `record::…`: what the run of a check of source-fidelity records (plan 2A, task S2,
//! item 2), through the binary's wiring and a judge on loopback: where its data went,
//! and no byte of the key anywhere under the store.

use crate::anthropic::{CHECK_CLASSES, Judged, checked, record_of, submission};

/// The run's id, from what the door printed.
fn run_of(printed: &str) -> String {
    let line = printed.lines().next().unwrap();
    line.strip_prefix("run ")
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_owned()
}

#[test]
fn the_runs_record_diagnostics_and_faults_hold_no_byte_of_the_key() {
    // A judge whose reason says the key back, and then a provider that answers an
    // error whose body holds it: neither the record, its diagnostics (the exchanges),
    // the events, nor any fault holds it, nor any file under the store.
    let judged = Judged::new("record-key");
    let said = format!("The key is {}.", judged.key);
    let (printed, log) = checked(&judged, vec![submission("held", &said)]);
    let printed = printed.unwrap();
    assert!(!printed.contains(&judged.key), "{printed}");
    assert!(printed.contains("verdict: pass"), "{printed}");
    // It went to the provider in its header, and nowhere into the store.
    assert!(log.lock().unwrap()[0].0.contains(&judged.key));
    let run = run_of(&printed);
    let record = record_of(&judged.world.root, &run);
    assert!(!record.to_string().contains(&judged.key));
    let body = format!(
        "{{\"type\":\"error\",\"error\":{{\"type\":\"invalid_request_error\",\"message\":\"bad key {}\"}}}}",
        judged.key
    );
    let error = format!(
        "HTTP/1.1 400 Bad Request\r\nX-Echo: {}\r\nContent-Length: {}\r\n\r\n{body}",
        judged.key,
        body.len()
    );
    let (failed, _) = checked(&judged, vec![error.into_bytes()]);
    let complaint = failed.unwrap_err();
    assert!(complaint.contains("refused the request"), "{complaint}");
    assert!(!complaint.contains(&judged.key), "{complaint}");
    assert!(!judged.store_holds_the_key());
}

#[test]
fn the_run_records_the_recipient_and_the_classes_sent() {
    let judged = Judged::new("record-egress");
    let (printed, log) = checked(&judged, vec![submission("violation", "The square is new.")]);
    let printed = printed.unwrap();
    assert!(printed.contains("verdict: fail"), "{printed}");
    let port = {
        let head = log.lock().unwrap()[0].0.clone();
        let host = head
            .lines()
            .find(|line| line.starts_with("Host: "))
            .unwrap()
            .to_owned();
        host.rsplit(':').next().unwrap().to_owned()
    };
    let record = record_of(&judged.world.root, &run_of(&printed));
    let egress = record["egress"].as_array().unwrap();
    assert_eq!(egress.len(), 1, "{egress:?}");
    assert_eq!(egress[0]["origin"], format!("https://127.0.0.1:{port}"));
    assert_eq!(egress[0]["outcome"], "sent");
    // The words and the two pictures of the request: prompts and source images.
    let classes: Vec<&str> = egress[0]["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|class| class.as_str().unwrap())
        .collect();
    assert!(
        classes.contains(&"prompts") && classes.contains(&"source_images"),
        "{classes:?}"
    );
    assert!(classes.iter().all(|class| CHECK_CLASSES.contains(class)));
    // No header of the request is in the record.
    assert!(!record.to_string().contains("x-api-key"));
}
