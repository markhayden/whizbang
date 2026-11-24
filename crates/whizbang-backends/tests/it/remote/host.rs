//! The fake host (plan m6, task R1, item 4): the pack's own front, run as its command
//! line runs it (`python -m whizbang_pack.front`), serving HTTPS on `127.0.0.1:0` with a
//! certificate of the test PKI the net client's tests use, with the pack's fake models
//! behind it. Its worker's side is `fake_host.py` beside this file, `--worker
//! fake_host:side`: the pack's own `LocalSide`, one for each fake fault a model's name
//! asks for, with limits the fast tier can wait for.
//!
//! Each model a test asks for is installed in the host's models directory as `whizbang
//! models get` installs one, with a record that names the fake model as its adapter; the
//! kind `remote` is told the same records ([`RemoteKind::hosting`]).
//!
//! The front is ended by closing its input, which stops it as a `SIGTERM` does and kills
//! every worker it started; then it is waited for, and killed past [`END_WITHIN`].

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use whizbang_backends::models::{FileName, InstallationRecord, InstalledFile, ModelName, Revision};
use whizbang_backends::net::testing::Pki;
use whizbang_backends::remote::RemoteKind;
use whizbang_core::contract::Secrets;
use whizbang_core::identity::Digest;
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::worker as kit;

/// The front's token, made up, of at least 32 characters.
pub const TOKEN: &str = "remote-test-token-0123456789abcdefghijk";
/// The variable the front reads its token from.
pub const TOKEN_VARIABLE: &str = "WHIZBANG_FRONT_TOKEN";
/// The token the fake fault `error-output` writes, which the front is told to take out
/// of every line it hands on (`--redact-env`).
pub const FAKES_TOKEN: &str = "made-up-token-5b8e1d0c4a2f6e93";
const FAKES_TOKEN_VARIABLE: &str = "WHIZBANG_FAKES_TOKEN";

/// How long the front has to say where it listens, and to end once its input closed.
const START_WITHIN: Duration = Duration::from_secs(10);
const END_WITHIN: Duration = Duration::from_secs(5);

/// The program the interpreter is started with, as the front's own `BOOT`: the folders
/// of its first argument on the path, then the module of its second run as a program.
const BOOT: &str = "import os, sys, runpy\n\
sys.path[:0] = sys.argv.pop(1).split(os.pathsep)\n\
name = sys.argv.pop(1)\n\
runpy.run_module(name, run_name='__main__', alter_sys=True)\n";

/// A model's name for a fault of the pack's fakes and a fake model: `<fault>--<model>`,
/// the fault `none` for the fakes with no fault.
pub fn named(fault: Option<&str>, model: &str) -> String {
    format!("{}--{model}", fault.unwrap_or("none"))
}

/// The record of an installation of one model, its adapter the fake model it names, of
/// one file of made-up bytes.
pub fn record(model: &str) -> (InstallationRecord, Vec<u8>) {
    record_with(model, format!("weights of {model}").as_bytes())
}

/// The same, its one file of these bytes.
pub fn record_with(model: &str, bytes: &[u8]) -> (InstallationRecord, Vec<u8>) {
    let none = Secrets::none();
    let adapter = model.split_once("--").map_or(model, |(_, fake)| fake);
    let bytes = bytes.to_vec();
    let file = InstalledFile::new(
        FileName::new("weights.bin", &none).unwrap(),
        bytes.len() as u64,
        Digest::of_bytes(&bytes),
    )
    .unwrap();
    let record = InstallationRecord::new(
        ModelName::new(model, &none).unwrap(),
        Revision::new("1", &none).unwrap(),
        ModelName::new(adapter, &none).unwrap(),
        Revision::new("fake-1", &none).unwrap(),
        vec![file],
        &none,
    )
    .unwrap();
    (record, bytes)
}

/// The front, its models and the root its certificate is of.
pub struct Host {
    child: Option<Child>,
    input: Option<ChildStdin>,
    errors: Option<JoinHandle<()>>,
    /// Told `None` once the front's error output has ended: once it has ended.
    told: std::sync::Mutex<Option<mpsc::Receiver<Option<u16>>>>,
    pub port: u16,
    pub pki: Arc<Pki>,
    pub records: BTreeMap<String, InstallationRecord>,
    models: Scratch,
    _tls: Scratch,
}

impl Host {
    /// A front with these models installed, each a name of [`named`].
    pub fn serving(models: &[String]) -> Host {
        Host::with_limit(models, 600)
    }

    /// The same, the front told this host limit of one attempt.
    pub fn with_limit(models: &[String], host_limit_s: u32) -> Host {
        let pki = Arc::new(Pki::loopback());
        let tls = Scratch::new("remote-tls");
        pki.write_pem(tls.path());
        let dir = Scratch::new("remote-models");
        let mut records = BTreeMap::new();
        for model in models {
            let (record, bytes) = record(model);
            install(dir.path(), &record, &bytes);
            records.insert(model.clone(), record);
        }
        let sources = format!(
            "{}:{}",
            kit::source().display(),
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../whizbang-backends/tests/it/remote"
            ))
            .display()
        );
        let mut command = Command::new(kit::python());
        command
            .env_clear()
            .env(TOKEN_VARIABLE, TOKEN)
            .env(FAKES_TOKEN_VARIABLE, FAKES_TOKEN)
            .env("PATH", "/usr/bin:/bin");
        for name in ["HOME", "TMPDIR", "LANG"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let host_limit = host_limit_s.to_string();
        let models_dir = dir.path().display().to_string();
        let cert = tls.path().join("cert.pem").display().to_string();
        let key = tls.path().join("key.pem").display().to_string();
        command
            .args(["-I", "-B", "-c", BOOT, &sources, "whizbang_pack.front"])
            .args(["--listen", "127.0.0.1:0", "--models-dir", &models_dir])
            .args(["--token-env", TOKEN_VARIABLE, "--worker", "fake_host:side"])
            .args(["--tls-cert", &cert, "--tls-key", &key])
            .args([
                "--host-limit-s",
                &host_limit,
                "--redact-env",
                FAKES_TOKEN_VARIABLE,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("the front starts");
        let input = child.stdin.take();
        let errors = child.stderr.take().expect("its error output is piped");
        let (tell, told) = mpsc::channel();
        let errors = whizbang_testkit::threads::spawn(move || {
            let mut said = false;
            for line in BufReader::new(errors).lines().map_while(Result::ok) {
                if !said && let Some(at) = line.strip_prefix("whizbang_pack.front: listening on ") {
                    let port = at.rsplit(':').next().and_then(|port| port.parse().ok());
                    let _ = tell.send(port);
                    said = true;
                }
            }
            let _ = tell.send(None);
        });
        let port: Option<u16> = told.recv_timeout(START_WITHIN).ok().flatten();
        let told = told;
        let mut host = Host {
            child: Some(child),
            input,
            errors: Some(errors),
            told: std::sync::Mutex::new(Some(told)),
            port: 0,
            pki,
            records,
            models: dir,
            _tls: tls,
        };
        match port {
            Some(port) => host.port = port,
            None => {
                host.end();
                panic!("the front did not say where it listens");
            }
        }
        host
    }

    /// The front's base address.
    pub fn base(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }

    /// The models directory.
    pub fn models_dir(&self) -> PathBuf {
        self.models.path().to_path_buf()
    }

    /// The dispatch ids whose request the host's side began, each with the `load` it
    /// carried (its model and settings, as JSON).
    pub fn seen(&self) -> BTreeMap<String, serde_json::Value> {
        self.noted("seen")
    }

    /// The dispatch ids whose request the host's side has ended.
    pub fn ended(&self) -> Vec<String> {
        self.noted("ended").into_keys().collect()
    }

    /// The dispatch ids the host's side was handed a cancel of.
    pub fn cancelled(&self) -> Vec<String> {
        self.noted("cancelled").into_keys().collect()
    }

    fn noted(&self, folder: &str) -> BTreeMap<String, serde_json::Value> {
        let Ok(listed) = std::fs::read_dir(self.models.path().join(folder)) else {
            return BTreeMap::new();
        };
        listed
            .take(256)
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let name = path.file_name()?.to_str()?.to_owned();
                if name.ends_with(".new") {
                    return None;
                }
                let document = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
                Some((name, document))
            })
            .collect()
    }

    /// Waits until `until` holds of the host, within [`START_WITHIN`]: whether it came.
    pub fn waits_for(&self, until: impl Fn(&Host) -> bool) -> bool {
        let end = Instant::now() + START_WITHIN;
        while Instant::now() < end {
            if until(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        until(self)
    }

    /// The kind `remote` over the host's models, each with the features `fake-ok` has.
    pub fn kind(&self) -> RemoteKind {
        let records: Vec<(InstallationRecord, Vec<String>)> = self
            .records
            .values()
            .map(|record| (record.clone(), features()))
            .collect();
        RemoteKind::hosting(&records)
    }

    /// Ends the front: its input closed, then the end of its error output waited for,
    /// which comes when it ends, and it is killed past [`END_WITHIN`]; then waited for,
    /// and the reader of its error output joined.
    fn end(&mut self) {
        drop(self.input.take());
        if let Some(mut child) = self.child.take() {
            let told = self
                .told
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let ended = told.is_some_and(|told| {
                let until = Instant::now() + END_WITHIN;
                loop {
                    let left = until.saturating_duration_since(Instant::now());
                    match told.recv_timeout(left) {
                        Ok(None) => break true,
                        Ok(Some(_)) => {}
                        Err(_) => break false,
                    }
                }
            });
            if !ended {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        if let Some(errors) = self.errors.take() {
            let _ = errors.join();
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.end();
    }
}

/// The features a fake model declares: a phrase, a point and a box.
pub fn features() -> Vec<String> {
    use whizbang_core::capability::feature;
    [feature::PHRASE, feature::POINT, feature::BOX]
        .map(str::to_owned)
        .to_vec()
}

/// An installation as `whizbang models get` leaves one: its folder named by its
/// identity, its record, and its files.
fn install(dir: &Path, record: &InstallationRecord, bytes: &[u8]) {
    let identity = record.identity().hex();
    let folder = dir
        .join("models")
        .join(record.model().as_str())
        .join("installations")
        .join(identity);
    std::fs::create_dir_all(folder.join("files")).unwrap();
    std::fs::write(folder.join("RECORD"), record.to_bytes()).unwrap();
    for file in record.files() {
        std::fs::write(folder.join("files").join(file.name().as_str()), bytes).unwrap();
    }
}
