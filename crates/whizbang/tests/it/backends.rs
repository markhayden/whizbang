//! `whizbang backends status [--probe]` (plan 1C, task P7b): each backend the config
//! offers, with its kind, its model and whether it is ready; and a probe of each.
//!
//! The list and the local probe are shown through the binary's wiring, over the pack's
//! fakes installed in a models directory of the test's own (`pack.rs`, `Local`), whose
//! one worker is started through the test kit's guard. A remote backend is probed
//! through the checked prober with the test kit's fake behind it: nothing in these tests
//! reaches a host, and the binary has no prober that could.

use std::sync::Arc;

use whizbang::backend_status::{self, Asked, NO_PROBER, NoProber};
use whizbang::pack::NOT_IN_CATALOGUE;
use whizbang::wiring::{self, Over};
use whizbang_core::config::Kinds;
use whizbang_core::contract::Secrets;
use whizbang_core::egress::{Address, Probed, Prober};
use whizbang_testkit::kinds::FakeKind;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::sending::FakeProber;
use whizbang_testkit::worker as kit;

use crate::pack::Local;
use crate::wired::has_line;

/// `whizbang backends status` with these arguments, run by the binary.
fn binary(args: &[&str]) -> std::process::Output {
    whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"))
        .args(["backends", "status"])
        .args(args)
        .output()
        .expect("the binary runs")
}

/// A prober that must not be asked: its script is empty, so a probe it is handed is
/// counted, and answered with a fault.
fn unasked() -> Arc<FakeProber> {
    Arc::new(FakeProber::scripted(Vec::new()))
}

/// Two local backends, one whose model is installed and one whose model the catalogue
/// does not hold, and a remote one of a kind this build does not have.
const THREE: &str = "[backends.segment]\ndefault = \"ok\"\n\n\
     [backends.segment.ok]\nkind = \"pack\"\nmodel = \"fake-ok\"\n\n\
     [backends.segment.ok.settings]\nmax_lifetime_s = 120\n\n\
     [backends.segment.gone]\nkind = \"pack\"\nmodel = \"fake-gone\"\n\n\
     [backends.segment.gone.settings]\nmax_lifetime_s = 120\n\n\
     [backends.fill.remote]\nkind = \"elsewhere\"\nurl = \"https://fill.example.org/v1\"\n";

#[test]
fn status_lists_each_configured_backend_and_starts_no_worker() {
    let local = Local::new("backends-status");
    let over = local.over(THREE);
    let identity = local.identity(&over, "fake-ok");
    let prober = unasked();
    let text = backend_status::status(&over, Asked::default(), prober.clone()).unwrap();
    has_line(&text, "backends: 3");
    has_line(
        &text,
        "fill\tremote\telsewhere\tno model\tnot ready: its kind is not one this build has",
    );
    has_line(
        &text,
        &format!(
            "segment\tgone\tpack\tfake-gone\tnot ready: its kind does not take it: {NOT_IN_CATALOGUE}"
        ),
    );
    has_line(
        &text,
        &format!("segment\tok\tpack\tfake-ok\tready: installation {identity}\tdefault"),
    );
    assert_eq!(text.lines().count(), 4, "{text}");
    // Nothing was started and nothing was asked: no supervisor, no worker, no probe.
    assert!(local.worker.status().is_none());
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
    assert_eq!(prober.requests(), 0);

    // The binary says the same of the same config, with its own catalogue, which
    // holds none of the fakes, and its own worker, which it does not start.
    let config = local.config_path();
    let out = binary(&["--config", config.to_str().unwrap()]);
    let said = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    has_line(&said, "backends: 3");
    has_line(
        &said,
        &format!(
            "segment\tok\tpack\tfake-ok\tnot ready: its kind does not take it: {NOT_IN_CATALOGUE}\tdefault"
        ),
    );
    assert!(!said.contains("probe"), "{said}");
    // With no config named the empty config offers nothing.
    let out = binary(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "backends: none: no config was named, and the empty config offers none\n"
    );
}

#[test]
fn status_with_probe_prints_the_hello_of_a_local_backends_worker_and_the_worker_is_gone_when_the_command_ends()
 {
    let local = Local::new("backends-probe");
    let over = local.over(THREE);
    let prober = unasked();
    let asked = Asked { probe: true };
    let text = backend_status::status(&over, asked, prober.clone()).unwrap();
    // One worker, for the command: each local backend, ready or not, is answered with
    // its hello, which names the protocol, the device the pack's core runs on and how
    // many threads it held. No model was loaded.
    let hello = text
        .lines()
        .find(|line| line.starts_with("probe segment ok: "))
        .unwrap_or_else(|| panic!("no probe of ok in:\n{text}"))
        .strip_prefix("probe segment ok: ")
        .unwrap()
        .to_owned();
    assert!(
        hello.starts_with("the pack's worker said hello: pack/1, device cpu, "),
        "{hello}"
    );
    assert!(hello.ends_with(" threads; it was stopped"), "{hello}");
    has_line(&text, &format!("probe segment gone: {hello}"));
    // A remote backend that is not ready is not probed, and the prober is not asked.
    has_line(&text, "probe fill remote: not probed: it is not ready");
    assert_eq!(prober.requests(), 0);
    // The worker it started was stopped and waited for before the command ended: no
    // child of this process runs the pack, and no supervisor was made, so no request
    // could have reached it.
    assert_eq!(kit::pack_children(), Vec::<u32>::new());
    assert!(local.worker.status().is_none());
    assert!(local.worker.journal().is_empty());
}

/// Two remote backends of the test kit's fake kind, at two recipients the config
/// approves: the first for probing and for nothing else, the second for prompts and
/// not for probing.
const REMOTE: &str = "[backends.fill.approved]\nkind = \"fake\"\nurl = \"https://fill.example.org/v1\"\n\n\
     [backends.fill.unapproved]\nkind = \"fake\"\nurl = \"https://other.example.net/v1\"\n\n\
     [egress.recipients.fill]\norigin = \"https://fill.example.org\"\nclasses = []\nprobe = true\n\n\
     [egress.recipients.other]\norigin = \"https://other.example.net\"\nclasses = [\"prompts\"]\n";

#[test]
fn status_with_probe_asks_a_prober_of_a_remote_backend_only_when_the_config_approves_probing_it() {
    let dir = Scratch::new("backends-remote");
    let config = dir.path().join("whizbang.toml");
    std::fs::write(&config, REMOTE).unwrap();
    let worker = whizbang::pack::OneWorker::installed();
    let mut kinds: Kinds = wiring::kinds(&worker);
    kinds.register(Arc::new(FakeKind::new("fake"))).unwrap();
    let over = Over::nothing().config(Some(&config)).kinds(kinds);
    // Listed, each ready and sending to its recipient; nothing is asked.
    let prober = Arc::new(FakeProber::scripted(vec![Ok(Probed::Reached {
        status: 204,
        body: Vec::new(),
    })]));
    let text = backend_status::status(&over, Asked::default(), prober.clone()).unwrap();
    has_line(
        &text,
        "fill\tapproved\tfake\tno model\tready: sends to https://fill.example.org",
    );
    has_line(
        &text,
        "fill\tunapproved\tfake\tno model\tready: sends to https://other.example.net",
    );
    assert_eq!(prober.requests(), 0);
    // Probed: the approved one is asked once, at its address, carrying no credential;
    // the other is refused before the prober, which is not asked about it.
    let asked = Asked { probe: true };
    let text = backend_status::status(&over, asked, prober.clone()).unwrap();
    has_line(
        &text,
        "probe fill approved: reached: it answered with status 204",
    );
    has_line(
        &text,
        "probe fill unapproved: not reached: the recipient is not approved for probing, so nothing is asked of it",
    );
    assert_eq!(prober.requests(), 1);
    let probes = prober.probes();
    let none = Secrets::none();
    let address = Address::of("https://fill.example.org/v1", &none).unwrap();
    assert_eq!(probes[0].to(), &address);
    assert!(probes[0].request().credential().is_none());
    // A prober with no network client contacts nothing, and says so; what the config
    // does not approve is refused before it.
    let nothing: Arc<dyn Prober> = Arc::new(NoProber);
    let text = backend_status::status(&over, asked, nothing).unwrap();
    has_line(
        &text,
        &format!("probe fill approved: not reached: {NO_PROBER}"),
    );
    has_line(
        &text,
        "probe fill unapproved: not reached: the recipient is not approved for probing, so nothing is asked of it",
    );
}
