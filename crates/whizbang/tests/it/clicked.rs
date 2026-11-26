//! `clicked::…`: `whizbang eval clicked` (plan mE, task C1, item 1), through the binary's
//! wiring over a kind of the test's own whose segmenter is the test kit's fake, which
//! answers every point with a box inside the truth mask: the click's mask is
//! under-covered, and the same mask shaped as `remove` shapes a location's is not.

use std::path::Path;
use std::sync::Arc;

use whizbang::clicked::{self, Decision, Ran};
use whizbang::loop_score::{self, Manifest};
use whizbang::pack::OneWorker;
use whizbang::wiring::{self, Over};
use whizbang_core::capability::{Declaration, Price, feature};
use whizbang_core::config::{Kind, KindRefused, Table};
use whizbang_core::contract::{Made, ModelIdentity};
use whizbang_core::egress::Recipient;
use whizbang_core::identity::Digest;
use whizbang_core::spend::Amount;
use whizbang_core::store::Store;
use whizbang_testkit::capabilities::FakeSegmenter;
use whizbang_testkit::scratch::Scratch;

use crate::checked::Unsynced;
use crate::cli::CONFIG;
use crate::loop_score::{TRUTH, png};

/// What the fake finds at every point: a box 2 pixels inside the truth on each side.
const INSIDE: [u32; 4] = [10, 10, 40, 38];

/// A kind named `finding`, as `cli.rs`'s, whose fake finds [`INSIDE`] at every point.
struct Inside;

impl Kind for Inside {
    fn name(&self) -> &str {
        "finding"
    }

    fn make(&self, table: &Table<'_>) -> Result<Made, KindRefused> {
        let model = ModelIdentity::new("finding", "m", "1");
        let features = [feature::PHRASE, feature::POINT, feature::BOX];
        let address = table.backend().address().expect("a url");
        let declaration = Declaration::new(model, &features)
            .unwrap()
            .sending_to(Recipient::new(address.origin().clone()))
            .priced(Price::at_most(1000.0).unwrap());
        let fake = FakeSegmenter::new()
            .declared_as(declaration)
            .finding_at_a_place(INSIDE);
        Ok(Made::Segment(Arc::new(fake)))
    }
}

/// A manifest of `count` entries in two groups, each the same picture and truth, and the
/// folder they are read from.
fn set_of(dir: &Path, count: usize) -> (Manifest, std::path::PathBuf) {
    let folder = dir.join("pictures");
    let (image, truth) = (png(None), png(Some(TRUTH)));
    let pin = |bytes: &[u8]| format!("sha256:{}", Digest::of_bytes(bytes).hex());
    let mut text = format!(
        "kind = \"segment-01\"\nset = \"segmentation/segment-01\"\n\
         images_repo = \"github.com/example/pictures\"\nimages_rev = \"{}\"\n",
        "a".repeat(40)
    );
    for at in 0..count {
        let id = format!("it-{at:02}");
        let items = folder.join("segmentation/segment-01/items").join(&id);
        std::fs::create_dir_all(&items).unwrap();
        std::fs::write(items.join("image.png"), &image).unwrap();
        std::fs::write(items.join("truth.png"), &truth).unwrap();
        let group = if at == 0 { "part" } else { "whole" };
        text.push_str(&format!(
            "\n[[item]]\nid = \"{id}\"\ngroup = \"{group}\"\nphrase = \"box\"\n\
             image = \"items/{id}/image.png\"\nimage_sha256 = \"{}\"\n\
             truth = \"items/{id}/truth.png\"\ntruth_sha256 = \"{}\"\n",
            pin(&image),
            pin(&truth)
        ));
    }
    (Manifest::parse(&text).unwrap(), folder)
}

#[test]
fn the_shaped_click_covers_what_the_click_alone_leaves_and_the_rule_says_build() {
    let dir = Scratch::new("clicked");
    // A store with an allowance the fake's stated price is reserved from.
    let root = dir.path().join("store");
    let store = Store::open(&root, Arc::new(Unsynced)).unwrap();
    whizbang_core::spend::reset(&store, Amount::new(1_000_000).unwrap(), 1).unwrap();
    drop(store);
    let (manifest, folder) = set_of(dir.path(), 3);
    let recovery = wiring::recovery();
    let prepared = clicked::prepare(&manifest, &folder, &root, &recovery).unwrap();
    // The click is inside the truth: its deepest pixel, first in reading order.
    assert!(prepared.iter().all(|p| p.click == (23, 23)));

    let config = dir.path().join("whizbang.toml");
    std::fs::write(&config, CONFIG).unwrap();
    let mut kinds = wiring::kinds(&OneWorker::installed());
    kinds.register(Arc::new(Inside)).unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    // The real disk without the syncs, as the loop's tests run.
    let store = Arc::new(Store::open(&root, Arc::new(Unsynced)).unwrap());
    let executor = wiring::runner(wiring::registry().unwrap(), store, &over).unwrap();
    let ran = clicked::run(&executor, &prepared, None).unwrap();
    // Asked again, the store answers each request with the run it made.
    let again = clicked::run(&executor, &prepared, None).unwrap();
    drop(executor);
    assert_eq!(ran, again);
    assert!(
        ran.iter().all(|r| matches!(r, Ran::Masks { .. })),
        "{ran:?}"
    );

    let tally = clicked::score(&root, &prepared, &ran).unwrap();
    let all = &tally.all;
    assert_eq!((all.pictures, all.measured, all.nothing_found), (3, 3, 0));
    // The box inside covers (30 * 28) / (34 * 32) of the truth; grown, closed and
    // feathered, all of it.
    let inside = (30.0 * 28.0) / (34.0 * 32.0);
    assert!((all.coverage_sum_unshaped / 3.0 - inside).abs() < 1e-9);
    assert!(all.coverage_sum_shaped / 3.0 >= loop_score::UNDER);
    assert_eq!((all.under_unshaped, all.under_shaped), (3, 0));
    assert_eq!(all.decision(), Decision::Build);
    assert_eq!(tally.group["part"].pictures, 1);
    assert_eq!(tally.group["whole"].under_unshaped, 2);
    let text = tally.text();
    assert!(
        text.contains("under-covered (coverage below 0.95) unshaped: 3\n"),
        "{text}"
    );
    assert!(
        text.ends_with("pictures than the unshaped one): build\n"),
        "{text}"
    );
    // Counts only: no item, no phrase.
    assert!(!text.contains("it-0") && !text.contains("box"), "{text}");
}
