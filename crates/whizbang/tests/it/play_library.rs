//! `play_library::…`: Home's library (plan m7, task U1, item 3): every picture the store
//! holds as an input of a run or an import, whatever door gave it, by pages, with each
//! picture's card, through the host's `GET /api/library` and `GET /api/library/{id}`.

use serde_json::{Value as Json, json};

use crate::play::{Kitted, fresh_id};

fn ran(kitted: &Kitted, action: &str, arguments: &Json) -> String {
    let prepared = kitted.prepare(action, arguments);
    assert_eq!(prepared["ready"], true, "{prepared}");
    let started = kitted.begin(action, arguments, &prepared["handle"], &fresh_id());
    assert_eq!(started.status, 201, "{}", started.text());
    let run = started.json()["run"].as_str().unwrap().to_owned();
    kitted.until_ended(&run);
    run
}

fn listed(kitted: &Kitted, query: &str) -> Json {
    let answer = kitted.api("GET", &format!("/api/library?{query}"), None);
    assert_eq!(answer.status, 200, "{}", answer.text());
    answer.json()
}

fn ids(page: &Json) -> Vec<String> {
    (page["pictures"].as_array().unwrap().iter())
        .map(|one| one["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_library_lists_inputs_of_runs_and_imports_by_pages_with_each_cards_runs() {
    let kitted = Kitted::new("library-doors");
    // A picture given a name in the playground, and an input of two runs.
    let named = kitted.picture(80);
    let first = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": named, "prompt": "the hose"}),
    );
    // What a run made, taken by another run: no person named it, no import marked it.
    let record = kitted
        .api("GET", &format!("/api/runs/{first}/record"), None)
        .json();
    let made = (record["outputs"].as_array().unwrap().iter())
        .find(|output| output["type"] == "image")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = ran(
        &kitted,
        "remove_prompt",
        &json!({"image": made, "prompt": "the reel"}),
    );
    ran(
        &kitted,
        "remove_prompt",
        &json!({"image": named, "prompt": "the cat"}),
    );
    let all = listed(&kitted, "");
    assert_eq!(all["counts"]["all"], 2, "{all}");
    assert_eq!(all["counts"]["named"], 1, "{all}");
    assert_eq!(all["counts"]["inputs"], 2, "{all}");
    assert_eq!(all["counts"]["imported"], 1, "{all}");
    let one = |id: &str| {
        (all["pictures"].as_array().unwrap().iter())
            .find(|one| one["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} is not listed: {all}"))
    };
    assert_eq!(one(&named)["name"], "porch.png");
    assert_eq!(one(&named)["runs"], 2);
    assert_eq!(one(&made)["name"], Json::Null);
    assert_eq!(one(&made)["imported"], false);
    assert_eq!(one(&made)["runs"], 1);
    // Each chip keeps its own; a page of one, then the next after it.
    assert_eq!(ids(&listed(&kitted, "show=named")), vec![named.clone()]);
    assert_eq!(ids(&listed(&kitted, "show=imported")), vec![named.clone()]);
    let page = listed(&kitted, "limit=1");
    assert_eq!(page["more"], true, "{page}");
    let after = ids(&page)[0].clone();
    let next = listed(&kitted, &format!("limit=1&after={after}"));
    assert_eq!(next["more"], false, "{next}");
    assert_ne!(ids(&next)[0], after);
    // A card: its size, and the runs that took it, newest first.
    let card = kitted.api("GET", &format!("/api/library/{made}"), None);
    assert_eq!(card.status, 200, "{}", card.text());
    let card = card.json();
    assert_eq!(
        (card["width"].clone(), card["height"].clone()),
        (json!(64), json!(48)),
        "{card}"
    );
    assert_eq!(card["runs_list"][0]["run"], second.as_str(), "{card}");
    assert!(card["first_ms"].as_u64().is_some(), "{card}");
    let bad = kitted.api("GET", "/api/library?show=other", None);
    assert_eq!(bad.status, 400, "{}", bad.text());
}
