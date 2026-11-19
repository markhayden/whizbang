//! `runner::…`: the shared harness (plan m3, task L1, item 0): cells of any action run,
//! recognised by their identity in a run's record, and forecast by their kind.

use std::sync::Arc;

use whizbang::harness::{self, Ended, Running, Work};
use whizbang::laboratory::LabCell;
use whizbang::wiring;
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::{Call, Request, Value};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_eval::lab::protocol;
use whizbang_testkit::scratch::Scratch;

use crate::lab::{backends, checked, corpus_of, matrix_text, protocol_text};
use crate::recipe::{CONFIG, over, record, with_small_picture};
use crate::wired::whizbang;

/// A cell of `segment`, an action that is neither `check` nor a recipe.
pub struct Finding {
    pub image: ArtifactId,
    pub scope: String,
}

impl Work for Finding {
    fn identity(&self) -> String {
        "0123abcd".to_owned()
    }
    fn request(&self, attempt: u32) -> Result<Option<Request>, String> {
        let call = Call::new("segment")
            .input("image", self.image)
            .argument("query", Value::text("collar"));
        let id = harness::request_id(&self.scope, &self.identity(), attempt)?;
        Ok(Some(Request::new(call).with_id(id)))
    }
    fn forecast(&self) -> Result<u64, String> {
        Ok(1000)
    }
    fn group(&self) -> String {
        "segment".to_owned()
    }
}

#[test]
fn a_cell_of_any_action_is_recognised_by_its_identity_in_a_runs_record() {
    let dir = Scratch::new("runner-any");
    let (root, image) = with_small_picture(&dir);
    let over = over(&dir, CONFIG);
    let image = ArtifactId(Digest::from_hex(image.strip_prefix("sha256:").unwrap()).unwrap());
    let cell = Finding {
        image,
        scope: "test-scope".to_owned(),
    };
    let run = || {
        let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
        let registry = wiring::registry_over(&over, false).unwrap();
        let executor = wiring::runner(registry, store, &over).unwrap();
        let running = Running {
            executor: &executor,
            root: &root,
            left_micros: 1_000_000,
            stop: &|| false,
        };
        harness::run(
            &[&cell],
            &harness::by_request("test-scope", &[&cell]),
            &running,
            1,
            "test",
        )
    };
    assert_eq!(run(), Ok(1));
    let found = harness::attempts(&root, &harness::by_request("test-scope", &[&cell])).unwrap();
    let attempts = &found["0123abcd"];
    assert_eq!(attempts.len(), 1);
    assert_eq!((attempts[0].0, attempts[0].2), (1, Ended::Completed));
    let held = record(&root, &attempts[0].1);
    assert_eq!(held.asked().action.as_deref(), Some("segment"));
    assert_eq!(
        held.identity().request.as_deref(),
        Some("cell.test-scope.0123abcd.1")
    );
    // Another scope's cells are not these.
    let other = harness::attempts(&root, &harness::by_request("other-scope", &[&cell])).unwrap();
    assert!(other.is_empty());
    // Run again, the completed cell is recognised and not run again.
    assert_eq!(run(), Ok(0));
    let listed = whizbang(&["runs", "list"], &root);
    assert!(listed.out.starts_with("runs: 1\n"), "{}", listed.out);
    drop(dir);
}

#[test]
fn a_fill_cells_forecast_is_its_price_per_call() {
    let price = whizbang_backends::fal::spend::bound_micros(whizbang_backends::fal::spend::PRICE);
    let price = price as u64;
    assert_eq!(price, 40_000);
    let protocol = protocol_text(["wide", "clean"], &["a"], 3);
    let arms = "[[arm]]\nname = \"wide\"\nrecipe = \"seam\"\nfiller = \"fal\"\n\n\
                [[arm]]\nname = \"clean\"\nreference = \"clean\"\n";
    let matrix = matrix_text("collar", "", arms)
        .replace("protect = \"a-protect\", ", "")
        .replace("texts = { prompt = \"collar\" }\n", "");
    assert!(!matrix.contains("texts"), "{matrix}");
    let (validated, cells) = checked(
        &matrix,
        &protocol,
        &corpus_of(b"a mask"),
        &backends("1", price),
    )
    .unwrap();
    // The seam fills once: its forecast is one call at the filler's price; the reference
    // arm's is nothing.
    let forecasts: Vec<(&str, u64)> = cells
        .iter()
        .map(|c| (c.arm.as_str(), c.forecast_micros))
        .collect();
    assert_eq!(forecasts, [("wide", price), ("clean", 0)]);
    // On the harness, a round whose forecast passes what is left runs nothing.
    let dir = Scratch::new("runner-fill");
    let (root, image) = with_small_picture(&dir);
    let over = over(&dir, CONFIG);
    let image = ArtifactId(Digest::from_hex(image.strip_prefix("sha256:").unwrap()).unwrap());
    let pictures = [("a".to_owned(), image), ("a-clean".to_owned(), image)].into();
    let scope = protocol::scope(&validated.protocol.name, &validated.protocol_digest);
    let labs: Vec<LabCell<'_>> = (cells.iter())
        .map(|cell| LabCell {
            cell,
            scope: &scope,
            pictures: &pictures,
        })
        .collect();
    let work: Vec<&dyn Work> = labs.iter().map(|c| c as &dyn Work).collect();
    let store = Arc::new(Store::open(&root, Arc::new(RealFs)).unwrap());
    let registry = wiring::registry_over(&over, false).unwrap();
    let executor = wiring::runner(registry, store, &over).unwrap();
    let running = Running {
        executor: &executor,
        root: &root,
        left_micros: price - 1,
        stop: &|| false,
    };
    let refused = harness::run(
        &work,
        &harness::by_request(&scope, &work),
        &running,
        1,
        "round",
    );
    assert_eq!(
        refused,
        Err(format!(
            "the round's forecast for its 1 runs still to run is {price} millionths of a \
             dollar, more than the {} the store's allowance has left; nothing was run",
            price - 1
        ))
    );
    drop(executor);
    let listed = whizbang(&["runs", "list"], &root);
    assert_eq!(listed.out, "runs: 0\n");
}
