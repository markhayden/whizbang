//! `remote::…`: the kind `remote` through the binary's wiring (ADR 0046, "The kind
//! `remote`"; plan m6, task R1): a config that names it under `[backends.segment]`, its
//! token in the env file, the action `segment` as the build ships it, under the gate,
//! the ledger and the run's record, against the fake host (the pack's own front on
//! loopback with the pack's fake models, `crates/whizbang-backends/tests/it/remote/`).
//! Every token here is made up, and nothing leaves the machine.

// The fake host of the backends' tests, shared whole: these tests use some of it.
#[allow(dead_code)]
#[path = "../../../whizbang-backends/tests/it/remote/host.rs"]
pub(crate) mod host;

use std::path::Path;
use std::sync::Arc;

use whizbang::backend_status::{self, Asked};
use whizbang::wiring::{self, Over};
use whizbang_backends::net::testing::prober_trusting;
use whizbang_backends::remote::{
    BAD_HOST_LIMIT, NOT_HTTPS, NOT_SEGMENT, PRICE_PAIR, RemoteKind, files_digest, price_of,
};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::config::Kinds;
use whizbang_core::contract::{
    Answer, Call, Cancellation, Engine, Executor, Finished, Request, Value,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;

use crate::anthropic::{import_picture, record_of};
use crate::wired::{World, has_line, whizbang};
use host::{Host, TOKEN, named};

/// The variable the env file sets the front's token in.
const TOKEN_VARIABLE: &str = "SEGMENT_TOKEN";

/// A world whose config names a remote segmenter at the fake host, with an allowance
/// to spend and a picture.
struct Remoted {
    world: World,
    host: Host,
    image: ArtifactId,
}

impl Remoted {
    fn new(label: &str, models: &[&str]) -> Remoted {
        Remoted::hosted(label, models, Host::serving)
    }

    /// The same, the host started by `start`.
    fn hosted(label: &str, models: &[&str], start: impl Fn(&[String]) -> Host) -> Remoted {
        let world = World::new(label);
        let reset = whizbang(&["spend", "reset", "--allowance", "1"], &world.root);
        assert_eq!(reset.code, Some(0), "{}", reset.err);
        let pictures = world.beside("pictures");
        std::fs::create_dir_all(&pictures).unwrap();
        let image: Digest = import_picture(&world.root, &pictures, "base", 200)
            .parse()
            .unwrap();
        let models: Vec<String> = models.iter().map(|model| named(None, model)).collect();
        Remoted {
            world,
            host: start(&models),
            image: ArtifactId(image),
        }
    }

    /// The config's table of the segmenter: its model, its url and these settings.
    fn table(&self, model: &str, url: &str, settings: &str) -> String {
        format!(
            "[backends.segment]\nkind = \"remote\"\nurl = \"{url}\"\ntoken_env = \"{TOKEN_VARIABLE}\"\n\
             model = \"{}\"\n\n[backends.segment.settings]\n{settings}\n",
            named(None, model)
        )
    }

    /// The config, written: the table, a cap, and, when `approved`, the host's origin
    /// approved for what a segment sends.
    fn over(&self, table: &str, approved: bool) -> Over {
        self.over_with(table, approved, self.host.kind())
    }

    /// The same, the kind `remote` over these models in place of the host's.
    fn over_with(&self, table: &str, approved: bool, kind: RemoteKind) -> Over {
        let mut config = format!("{table}\n[spend]\ncall_cap_usd = 0.50\n");
        if approved {
            config.push_str(&format!(
                "\n[egress.recipients.gpu]\norigin = \"{}\"\nclasses = [\"source_images\", \"prompts\"]\nprobe = true\n",
                self.host.base()
            ));
        }
        std::fs::write(self.world.config_path(), config).unwrap();
        std::fs::write(self.world.env_path(), format!("{TOKEN_VARIABLE}={TOKEN}\n")).unwrap();
        let mut kinds = Kinds::new();
        kinds.register(Arc::new(kind)).unwrap();
        Over::nothing()
            .config(Some(&self.world.config_path()))
            .env_file(Some(&self.world.env_path()))
            .kinds(kinds)
            .trusting(self.host.pki.clone())
    }

    /// The config of the plan's example price, approved.
    fn priced(&self, model: &str) -> Over {
        let table = self.table(
            model,
            &self.host.base(),
            "price_microusd_per_second = 300\nhost_limit_s = 600",
        );
        self.over(&table, true)
    }

    /// The same, the kind `remote` over these models.
    fn priced_with(&self, model: &str, kind: RemoteKind) -> Over {
        let table = self.table(
            model,
            &self.host.base(),
            "price_microusd_per_second = 300\nhost_limit_s = 600",
        );
        self.over_with(&table, true, kind)
    }

    fn runner(&self, over: &Over) -> Engine {
        let store = Arc::new(Store::open(&self.world.root, Arc::new(RealFs)).unwrap());
        wiring::runner(wiring::registry().unwrap(), store, over).expect("the executor builds")
    }

    /// One `segment` of the picture for a phrase.
    fn segment(&self, executor: &Engine) -> Finished {
        let call = Call::new("segment")
            .input("image", self.image)
            .argument("query", Value::text("a red thing"));
        executor.submit(&call).unwrap()
    }

    /// The digest of the files of this fake model's installation.
    fn digest(&self, model: &str) -> String {
        let record = &self.host.records[&named(None, model)];
        files_digest(
            record
                .files()
                .iter()
                .map(|file| (file.name().as_str(), file.sha256().to_string())),
        )
    }

    /// What `whizbang runs show --diagnostic diagnostics/segment.txt` prints of a run.
    fn said(&self, run: &str) -> String {
        let said = whizbang(
            &[
                "runs",
                "show",
                run,
                "--diagnostic",
                "diagnostics/segment.txt",
            ],
            &self.world.root,
        );
        assert_eq!(said.code, Some(0), "{}", said.err);
        said.out
    }
}

/// Whether any file under a folder holds these bytes.
fn holds(dir: &Path, needle: &[u8]) -> bool {
    std::fs::read_dir(dir).unwrap().any(|entry| {
        let path = entry.unwrap().path();
        if path.is_dir() {
            holds(&path, needle)
        } else {
            let bytes = std::fs::read(&path).unwrap();
            bytes.windows(needle.len()).any(|at| at == needle)
        }
    })
}

#[test]
fn a_call_is_sent_through_the_gate_with_its_token_bound_to_its_origin() {
    let remoted = Remoted::new("remote-gate", &["fake-ok"]);
    // Not approved: the gate refuses the call, and the host sees nothing.
    let table = remoted.table(
        "fake-ok",
        &remoted.host.base(),
        "price_microusd_per_second = 300\nhost_limit_s = 600",
    );
    let refused = remoted.segment(&remoted.runner(&remoted.over(&table, false)));
    assert_eq!(refused.outcome().name(), "error", "{:?}", refused.outcome());
    assert!(remoted.host.seen().is_empty());
    // Approved: sent, its token carried to the host's origin alone (the front answers
    // nothing without it), and listed where the run's data went with the classes a
    // segment sends.
    let ran = remoted.segment(&remoted.runner(&remoted.priced("fake-ok")));
    assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
    assert_eq!(remoted.host.seen().len(), 1);
    let record = record_of(&remoted.world.root, ran.run().as_str());
    let egress = record["egress"].as_array().unwrap();
    assert_eq!(egress.len(), 1, "{egress:?}");
    assert_eq!(egress[0]["origin"], remoted.host.base());
    assert_eq!(
        egress[0]["classes"],
        serde_json::json!(["source_images", "prompts"])
    );
    assert_eq!(egress[0]["outcome"], "sent");
    // The store holds no byte of the token.
    assert!(!holds(&remoted.world.root, TOKEN.as_bytes()));
}

#[test]
fn the_examples_bound_is_360000_microusd_and_an_unknown_end_settles_at_it() {
    // 300 millionths a second, for 600 seconds an attempt, over 2 attempts.
    let price = price_of(300, 600).unwrap();
    assert_eq!(price.quote().bound_micros(), Some(360_000));
    let remoted = Remoted::new("remote-bound", &["fake-ok", "fake-exit"]);
    // An answer is settled at the bound too: the host's bill is not known.
    let over = remoted.priced("fake-ok");
    let ran = remoted.segment(&remoted.runner(&over));
    assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
    let record = record_of(&remoted.world.root, ran.run().as_str());
    let paid = &record["spend"]["requests"];
    assert_eq!(paid[0]["reserved_micros"], 360_000, "{paid}");
    assert_eq!(paid[0]["settled_micros"], 360_000, "{paid}");
    // A worker that ends inside the call: an unknown end, settled at its bound.
    let ran = remoted.segment(&remoted.runner(&remoted.priced("fake-exit")));
    assert_eq!(ran.outcome().name(), "error", "{:?}", ran.outcome());
    let record = record_of(&remoted.world.root, ran.run().as_str());
    let paid = &record["spend"]["requests"];
    assert_eq!(paid[0]["reserved_micros"], 360_000, "{paid}");
    assert_eq!(paid[0]["settled_micros"], 360_000, "{paid}");
    assert_eq!(paid[0]["stage"], "unresolved", "{paid}");
}

#[test]
fn a_remote_backend_without_both_price_settings_has_no_bound_and_is_refused() {
    let remoted = Remoted::new("remote-unpriced", &["fake-ok"]);
    // Neither: the backend is made, with no price, and its call is refused before
    // anything is sent.
    let table = remoted.table("fake-ok", &remoted.host.base(), "");
    let over = remoted.over(&table, true);
    let call = Call::new("segment")
        .input("image", remoted.image)
        .argument("query", Value::text("a red thing"));
    let refused = remoted.runner(&over).submit(&call).unwrap_err();
    assert!(format!("{refused:?}").contains("NoBound"), "{refused:?}");
    assert!(remoted.host.seen().is_empty());
    // One without the other: refused when the config is read, naming them.
    for one in ["price_microusd_per_second = 300", "host_limit_s = 600"] {
        let table = remoted.table("fake-ok", &remoted.host.base(), one);
        let text = backend_status::status(
            &remoted.over(&table, true),
            Asked::default(),
            prober_trusting(&remoted.host.pki),
        )
        .unwrap();
        assert!(text.contains(PRICE_PAIR), "{text}");
    }
}

#[test]
fn a_zero_or_overflowing_host_limit_is_refused_when_the_config_is_read() {
    let remoted = Remoted::new("remote-limit", &["fake-ok"]);
    for settings in [
        "price_microusd_per_second = 300\nhost_limit_s = 0",
        "price_microusd_per_second = 300\nhost_limit_s = 9223372036854775807",
        "price_microusd_per_second = 9007199254740993\nhost_limit_s = 600000000000",
    ] {
        let table = remoted.table("fake-ok", &remoted.host.base(), settings);
        let over = remoted.over(&table, true);
        let text =
            backend_status::status(&over, Asked::default(), prober_trusting(&remoted.host.pki))
                .unwrap();
        assert!(
            text.contains("not ready: its kind does not take it"),
            "{settings}: {text}"
        );
        assert!(
            text.contains(BAD_HOST_LIMIT) || text.contains(whizbang_backends::remote::BAD_PRICE),
            "{settings}: {text}"
        );
    }
    // A zero price is refused too, and a plain http address, and a remote fill.
    let zero = remoted.table(
        "fake-ok",
        &remoted.host.base(),
        "price_microusd_per_second = 0\nhost_limit_s = 600",
    );
    let plain = remoted.table(
        "fake-ok",
        &remoted.host.base().replace("https", "http"),
        "price_microusd_per_second = 300\nhost_limit_s = 600",
    );
    let fill = remoted
        .table(
            "fake-ok",
            &remoted.host.base(),
            "price_microusd_per_second = 300\nhost_limit_s = 600",
        )
        .replace("backends.segment", "backends.fill");
    for (table, why) in [
        (zero, whizbang_backends::remote::BAD_PRICE),
        (plain, NOT_HTTPS),
        (fill, NOT_SEGMENT),
    ] {
        let text = backend_status::status(
            &remoted.over(&table, true),
            Asked::default(),
            prober_trusting(&remoted.host.pki),
        )
        .unwrap();
        assert!(text.contains(why), "{why}: {text}");
    }
}

#[test]
fn the_run_records_the_identity_asked_the_one_reported_and_the_device() {
    let remoted = Remoted::new("remote-identity", &["fake-mps"]);
    let ran = remoted.segment(&remoted.runner(&remoted.priced("fake-mps")));
    assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
    let digest = remoted.digest("fake-mps");
    // The identity asked: the model of the kind at the digest of the catalogue's files.
    let record = record_of(&remoted.world.root, ran.run().as_str());
    let model = &record["models"][0];
    assert_eq!(model["backend"], "remote", "{model}");
    assert_eq!(model["model"], named(None, "fake-mps"), "{model}");
    let version = whizbang_backends::remote::version_of(&digest);
    assert_eq!(model["version"], version.as_str(), "{model}");
    // The device the worker named, and no cache.
    assert_eq!(model["device"], "mps", "{model}");
    assert_eq!(model["cached"], false, "{model}");
    // The one reported, beside the response in the run's diagnostics.
    let said = remoted.said(ran.run().as_str());
    assert!(said.contains(&format!("files {digest}")), "{said}");
}

/// The card the worker named on the `load` that put the model on it (plan m7, task B1,
/// item 6; ADR 0048): the fake host's model on a card, whose worker's own code names it
/// as on a GPU host, is answered on `cuda`, and the run's record, `run/7`, names the card
/// beside the device. A model on no card names none, and its record is as before.
#[test]
fn the_run_records_the_card_the_fake_host_named() {
    let remoted = Remoted::new("remote-card", &["fake-cuda", "fake-mps"]);
    let ran = remoted.segment(&remoted.runner(&remoted.priced("fake-cuda")));
    assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
    let record = record_of(&remoted.world.root, ran.run().as_str());
    assert_eq!(record["record"], "run/7", "{record}");
    let model = &record["models"][0];
    assert_eq!(model["device"], "cuda", "{model}");
    assert_eq!(model["card"], "NVIDIA Fake A10G", "{model}");
    assert_eq!(model["cached"], false, "{model}");
    // `runs show` reads it back.
    let shown = whizbang(&["runs", "show", ran.run().as_str()], &remoted.world.root);
    assert_eq!(shown.code, Some(0), "{}", shown.err);
    // A model on no card: no card, and the record is of the version before.
    let ran = remoted.segment(&remoted.runner(&remoted.priced("fake-mps")));
    assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
    let record = record_of(&remoted.world.root, ran.run().as_str());
    assert_eq!(record["record"], "run/4", "{record}");
    assert!(record["models"][0].get("card").is_none(), "{record}");
}

#[test]
fn the_dispatch_id_is_in_the_runs_record_and_a_cancel_names_it() {
    let remoted = Remoted::new("remote-dispatch", &["fake-ok"]);
    let ran = remoted.segment(&remoted.runner(&remoted.priced("fake-ok")));
    assert_eq!(ran.outcome().name(), "completed", "{:?}", ran.outcome());
    let seen: Vec<String> = remoted.host.seen().into_keys().collect();
    assert_eq!(seen.len(), 1);
    // The run keeps the dispatch id its request was sent under; the front's side
    // began the request of that id. A cancel names it the same way, in the same field
    // (`remote::cancel::a_cancel_sends_the_cancel_frame_and_closes_the_stream`, in the
    // backends' tests).
    let said = remoted.said(ran.run().as_str());
    assert!(said.contains(&format!("dispatch {}", seen[0])), "{said}");
}

#[test]
fn status_with_probe_reads_the_hosts_document_and_names_the_model_it_holds() {
    let remoted = Remoted::new("remote-status", &["fake-ok"]);
    let over = remoted.priced("fake-ok");
    let asked = Asked { probe: true };
    let text = backend_status::status(&over, asked, prober_trusting(&remoted.host.pki)).unwrap();
    let identity = remoted.host.records[&named(None, "fake-ok")].identity();
    has_line(
        &text,
        &format!(
            "probe segment default: reached: the host holds {} by the catalogue's hashes, installation {identity}, not yet checked by the front",
            named(None, "fake-ok")
        ),
    );
    // A host whose files are not those the kind holds the model to does not hold it.
    let elsewhere = host::record_with(&named(None, "fake-ok"), b"other weights").0;
    let kind = RemoteKind::hosting(&[(elsewhere, host::features())]);
    let over = remoted.priced_with("fake-ok", kind);
    let text = backend_status::status(&over, asked, prober_trusting(&remoted.host.pki)).unwrap();
    has_line(
        &text,
        &format!(
            "probe segment default: reached: the host does not hold {} by the catalogue's hashes",
            named(None, "fake-ok")
        ),
    );
}

/// One `segment` of the picture under this request's deadline or its signal: the run.
fn segment_under(executor: &Engine, request: Request) -> Finished {
    match executor.request(&request) {
        Ok(Answer::Ran(finished)) => finished,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_timeout_and_a_cancel_send_the_hosts_cancel_through_the_checked_sender_and_name_the_dispatch() {
    // The host's side waits 30 seconds for a call, so each request ends on whizbang's side.
    let remoted = Remoted::hosted("remote-cancel", &["fake-wait"], |models| {
        Host::with_call_limit(models, 30)
    });
    let executor = remoted.runner(&remoted.priced("fake-wait"));
    let call = || {
        Call::new("segment")
            .input("image", remoted.image)
            .argument("query", Value::text("a red thing"))
    };
    // At its deadline: the fake waits until it is cancelled, and the host's limit of a
    // call is longer than the deadline.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let timed_out = segment_under(&executor, Request::new(call()).until(deadline));
    assert_ne!(timed_out.outcome().name(), "completed");
    // At the run's signal, set once the host's side began the request.
    let signal = Cancellation::new();
    let cancelled = whizbang_testkit::threads::scope(|scope| {
        let host = &remoted.host;
        let cancel = signal.clone();
        let before = host.seen().len();
        let watcher = scope.spawn(move || {
            let began = host.waits_for(|host| host.seen().len() > before);
            cancel.cancel();
            began
        });
        let ran = segment_under(&executor, Request::new(call()).cancelled_by(signal.clone()));
        assert!(watcher.join().unwrap(), "the host's side began no request");
        ran
    });
    assert_ne!(cancelled.outcome().name(), "completed");
    drop(executor);
    // The host's side was handed a cancel of each, sent through the production checked
    // sender, by its dispatch id; and each run names its dispatch, and lists the cancel
    // where its data went, with no class.
    let seen: Vec<String> = remoted.host.seen().into_keys().collect();
    assert!(remoted.host.waits_for(|host| host.cancelled().len() == 2));
    assert_eq!(remoted.host.cancelled(), seen);
    for ran in [&timed_out, &cancelled] {
        let text = remoted.said(ran.run().as_str());
        assert!(text.starts_with("failed "), "{text}");
        let named = seen
            .iter()
            .any(|dispatch| text.contains(&format!("dispatch {dispatch}")));
        assert!(named, "{text}");
        let record = record_of(&remoted.world.root, ran.run().as_str());
        let paid = &record["spend"]["requests"];
        assert_eq!(paid[0]["settled_micros"], 360_000, "{paid}");
    }
}
