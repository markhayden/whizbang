//! `record::…`: what the run of a check of source-fidelity records (plan 2A, task S2,
//! item 2), through the binary's wiring and a judge on loopback: where its data went,
//! and no byte of the key anywhere under the store.

use crate::anthropic::{
    CHECK_CLASSES, GOOGLE, Judged, OPENAI, checked, record_of, submission, submission_by,
};

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
fn a_run_on_each_new_judge_holds_no_byte_of_its_key() {
    // As above, for a judge of Google and one of OpenAI: a reason that says the key back;
    // nothing printed, recorded or kept under the store holds it, and the provider was
    // sent it in its header alone. A fault of either holds nothing of what the provider
    // answered (`google::` and `openai::a_fault_holds_nothing_of_the_body_the_headers_or_the_address`).
    for provider in [GOOGLE, OPENAI] {
        let judged = Judged::of(&format!("record-key-{}", provider.name), provider);
        let said = format!("The key is {}.", judged.key);
        let (printed, log) = checked(&judged, vec![submission_by(provider, "held", &said)]);
        let printed = printed.unwrap();
        assert!(!printed.contains(&judged.key), "{printed}");
        assert!(printed.contains("verdict: pass"), "{printed}");
        let (head, body) = log.lock().unwrap()[0].clone();
        assert_eq!(head.matches(&judged.key).count(), 1, "{head}");
        assert!(!String::from_utf8(body).unwrap().contains(&judged.key));
        let record = record_of(&judged.world.root, &run_of(&printed));
        assert!(!record.to_string().contains(&judged.key));
        assert!(!judged.store_holds_the_key(), "{}", provider.name);
    }
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

#[test]
fn a_run_without_the_env_file_holds_no_byte_of_a_key_pasted_into_the_config() {
    // A config whose judge holds a key of capitals and digits where its variable's
    // name belongs, and no env file: a segment run and an operation's run over it
    // complete, and no file under the store holds the key.
    use crate::cli::{CONFIG, pasted_judge, pasted_key, segment_under, with_picture};
    use whizbang::door::SegmentAsked;
    use whizbang_testkit::scratch::Scratch;
    let dir = Scratch::new("record-pasted");
    let (root, image) = with_picture(&dir);
    let text = format!("{CONFIG}{}", pasted_judge());
    let asked = SegmentAsked {
        image: &image,
        kind: "phrase",
        query: "circle",
        max_instances: 8,
        backend: None,
        request_id: None,
    };
    let found = segment_under(&dir, &root, &asked, &text).unwrap();
    assert!(!found.contains(&pasted_key()), "{found}");
    let mask = found
        .lines()
        .find_map(|line| line.strip_prefix("mask "))
        .map(|rest| rest.split(' ').next().unwrap().to_owned())
        .unwrap();
    let config = dir.path().join("whizbang.toml");
    let grown = crate::wired::whizbang(
        &[
            "rascal",
            "grow",
            "--mask",
            &mask,
            "--px",
            "2",
            "--config",
            config.to_str().unwrap(),
        ],
        &root,
    );
    assert_eq!(grown.code, Some(0), "{}", grown.err);
    let key = pasted_key();
    let mut seen = 0;
    let mut folders = vec![root.clone()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                seen += 1;
                let found = bytes.windows(key.len()).any(|at| at == key.as_bytes());
                assert!(!found, "a file of the store holds the key");
            }
        }
    }
    assert!(seen > 10, "the store holds next to nothing");
}

#[test]
fn each_judge_turn_records_its_counted_input_beside_its_usage() {
    // The exchange the run keeps of each turn holds the input the adapter counted
    // before the request was sent, beside the usage the provider reported.
    let judged = Judged::new("record-counted");
    let (printed, log) = checked(&judged, vec![submission("held", "Nothing moved.")]);
    let printed = printed.unwrap();
    let run = run_of(&printed);
    let record = record_of(&judged.world.root, &run);
    let turns = record["judge"].as_array().unwrap();
    let turn = turns.iter().find(|turn| turn["judge"] == "turn").unwrap();
    let name = turn["exchange"].as_str().unwrap();
    let id = whizbang_core::contract::RunId::parse(&run).unwrap();
    let name = name.strip_prefix("diagnostics/").unwrap_or(name);
    let bytes = whizbang::door::diagnostic(&judged.world.root, &id, name).unwrap();
    let exchange: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let answered = &exchange["answer"]["turn"];
    let counted = answered["counted_input_tokens"].as_u64().unwrap();
    assert!(answered["usage"].is_object(), "{exchange}");
    // The count is the adapter's: at least the request's own bytes, and no more than
    // the input ceiling it was held to.
    let sent = log.lock().unwrap()[0].1.len() as u64;
    assert!(
        counted >= sent / 2 && counted <= 40_000,
        "{counted} for {sent} bytes"
    );
}

#[test]
fn a_reason_item_is_redacted_before_it_is_cut_or_escaped() {
    // A reason whose key stands across both cuts: the summary's, at 300 characters of
    // the reason, and the door's, at 5,000 of the item. The key is taken out first,
    // whole, so neither cut leaves a piece of it: not the record, the diagnostic of
    // the items, any file under the store, nor what is printed.
    let judged = Judged::new("record-reason-cut");
    let key = judged.key.clone();
    let filler = |chars: usize| vec!["ab"; chars / 3].join(" ");
    // The item's own words come first: 40 characters before the reason.
    let first = filler(294 - 2);
    let second = filler(5_000 - 40 - first.len() - key.len() - 2 - 4);
    let reason = format!("{first} {key} {second} {key} tail");
    let at = |of: &str, from: usize| of[..from].chars().count();
    let first_at = at(&reason, reason.find(&key).unwrap());
    assert!(first_at < 300 && first_at + key.len() > 300, "{first_at}");
    let item = format!("chk:ob:SF1@candidate: held (confident): {reason}");
    let second_at = at(&item, item.rfind(&key).unwrap());
    assert!(
        second_at < 5_000 && second_at + key.len() > 5_000,
        "{second_at}"
    );
    let (printed, _) = checked(&judged, vec![submission("held", &reason)]);
    let printed = printed.unwrap();
    // No piece of the key of eight characters or more, anywhere.
    let pieces: Vec<&str> = (0..=key.len() - 8)
        .map(|from| &key[from..from + 8])
        .collect();
    for piece in &pieces {
        assert!(!printed.contains(piece), "{piece} in {printed}");
    }
    let run = run_of(&printed);
    let record = record_of(&judged.world.root, &run).to_string();
    let id = whizbang_core::contract::RunId::parse(&run).unwrap();
    let items =
        whizbang::door::diagnostic(&judged.world.root, &id, whizbang_squint::use_case::REASONS)
            .unwrap();
    let items = String::from_utf8(items).unwrap();
    for piece in &pieces {
        assert!(!record.contains(piece), "{piece} in the record");
        assert!(!items.contains(piece), "{piece} in the items");
    }
    assert_eq!(
        items.matches("[redacted]").count(),
        4,
        "the items hold the marker"
    );
    assert!(!judged.store_holds_the_key());
}

#[test]
fn the_brief_and_each_bindings_role_and_label_are_in_the_run() {
    use crate::anthropic::{
        Judged, checked_job, checklist_of, import_picture, record_of, submission_of,
    };
    // Plan 2C, task A1, items 2 and 3: the job the run was given, its brief and each
    // image's role and label among them, is the record's argument; what each image was
    // bound to is the run's diagnostic of what it decided.
    let judged = Judged::new("record-bindings");
    let scratch = judged.world.beside("pictures");
    std::fs::create_dir_all(&scratch).unwrap();
    let candidate = import_picture(&judged.world.root, &scratch, "candidate", 200);
    let first = import_picture(&judged.world.root, &scratch, "first", 10);
    let second = import_picture(&judged.world.root, &scratch, "second", 90);
    let brief = "The bottle keeps its label: SQUINT in blue capitals.";
    let asked = whizbang::door::CheckAsked {
        use_case: "product-fidelity",
        candidate: &candidate,
        source: None,
        references: &[&first, &second],
        judge: None,
        brief: Some(brief),
        request_id: None,
        details: whizbang::door::Details::None,
        detail: None,
        projection: "full",
    };
    let (job, images) = whizbang::door::shorthand(&asked).unwrap();
    let ids = checklist_of(&job);
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let (printed, _) = checked_job(
        &judged,
        &job,
        &images,
        vec![submission_of(&ids, "held", "Same.")],
    );
    let printed = printed.unwrap();
    let run = run_of(&printed);
    let record = record_of(&judged.world.root, &run);
    let arguments = record["asked"]["arguments"].as_array().unwrap();
    let argument = |name: &str| {
        let found = arguments.iter().find(|argument| argument["name"] == name);
        found.unwrap()["value"]["text"].as_str().unwrap().to_owned()
    };
    assert_eq!(argument("use_case"), "product-fidelity");
    let held: serde_json::Value = serde_json::from_str(&argument("job")).unwrap();
    assert_eq!(held["contract"], "job/1");
    assert_eq!(held["brief"], brief);
    let bound: Vec<(&str, &str, &serde_json::Value)> = held["images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|image| {
            (
                image["id"].as_str().unwrap(),
                image["role"].as_str().unwrap(),
                &image["label"],
            )
        })
        .collect();
    assert_eq!(
        bound,
        [
            ("candidate", "candidate", &serde_json::Value::Null),
            (
                "reference-1",
                "reference",
                &serde_json::json!("reference 1")
            ),
            (
                "reference-2",
                "reference",
                &serde_json::json!("reference 2")
            ),
        ]
    );
    // The inputs, in the job's order.
    let inputs: Vec<&str> = record["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| input["artifact"].as_str().unwrap())
        .collect();
    assert_eq!(
        inputs,
        [candidate.as_str(), first.as_str(), second.as_str()]
    );
    let id = whizbang_core::contract::RunId::parse(&run).unwrap();
    let kept = whizbang::door::diagnostic(&judged.world.root, &id, whizbang_squint::check::JUDGED);
    let kept: serde_json::Value = serde_json::from_slice(&kept.unwrap()).unwrap();
    assert_eq!(kept["images"][2]["artifact"], second.as_str());
    assert_eq!(kept["images"][2]["label"], "reference 2");
    assert_eq!(kept["checklist"].as_array().unwrap().len(), ids.len());
}
