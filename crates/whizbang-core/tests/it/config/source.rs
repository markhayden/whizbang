//! The source over the config file and its env file (ADR 0011, "The source over both
//! files"; ADR 0015, "One snapshot of the config"; plan 1B, task C4). Every test here
//! builds an executor over the source, and the source over two files the test writes
//! into its own scratch folder. The kinds are the test kit's fake, and every key is
//! made up at run time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use whizbang_core::capability::{FillRequest, Picture, Support};
use whizbang_core::config::{
    ConfigRefused, EnvRefused, Files, KEY_SHOWN_LIMIT, KIND_SENTENCE_WITHHELD, Kinds, LoadRefused,
};
use whizbang_core::contract::record::RunRecord;
use whizbang_core::contract::{
    Action, ActionDescriptor, Answer, Argument, Call, CallError, Capability, ConfigSource, Context,
    Engine, Executor, Failure, Finished, Kind, ModelIdentity, NoBackends, Outcome, PreparedCall,
    Produced, Readiness, Refusal, Registry, Request, Requirement, SECRETS_KEPT_LIMIT, Secrets,
    SourceRefused, Stale, Value, ValueType,
};
use whizbang_core::identity::{CanonicalPixels, Digest};
use whizbang_core::store::{Reader, Store};
use whizbang_testkit::fs_shim::ShimFs;
use whizbang_testkit::kinds::FakeKind;
use whizbang_testkit::scratch::Scratch;

use super::made_up_key;

const API: &str = "https://api.example.com";

type Body = Box<dyn Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync>;

/// An action whose run is whatever the test says.
struct Scripted(ActionDescriptor, Body);

impl Action for Scripted {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.0
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        (self.1)(call, context)
    }
}

/// An action of this name with these needs. It takes an optional text, `backend`,
/// which a need may name as the argument that chooses its backend.
fn action(
    name: &str,
    needs: Vec<Requirement>,
    body: impl Fn(&PreparedCall, &Context) -> Result<Produced, Failure> + Send + Sync + 'static,
) -> Arc<dyn Action> {
    let mut draft = ActionDescriptor::draft(name, "1.0.0", Kind::Primitive, "A test action.")
        .argument(Argument::optional("backend", "A backend.", ValueType::Text));
    for need in needs {
        draft = draft.needs(need);
    }
    Arc::new(Scripted(draft.build().unwrap(), Box::new(body)))
}

/// A request a filler takes: a small picture and the part of it to fill.
fn fill_request() -> FillRequest {
    let rgba = (0..24u8).flat_map(|at| [at, at, at, 255]).collect();
    let picture = Picture::new(CanonicalPixels::from_rgba(6, 4, rgba).unwrap()).unwrap();
    let mut plane = vec![0; 24];
    plane[..6].fill(255);
    FillRequest::new(picture, Support::from_plane(6, 4, plane).unwrap(), None).unwrap()
}

/// `asks`: needs a filler, which its argument `backend` may choose, and asks it once.
fn asks() -> Arc<dyn Action> {
    let needs = vec![Requirement::new(Capability::Fill).chosen_by("backend")];
    action("asks", needs, |_, context| {
        let fill = context.capabilities().fill().unwrap();
        fill.fill(&fill_request()).unwrap();
        Ok(Produced::nothing())
    })
}

/// A scratch folder with a config, an env file and a store in it, the source over
/// the two files with one fake kind, and that kind.
struct World {
    dir: Scratch,
    fs: Arc<ShimFs>,
    kind: Arc<FakeKind>,
    files: Arc<Files>,
}

impl World {
    /// With this config, and this env file when one is given. A world with no env
    /// file has a source that names none.
    fn new(config: &str, env: Option<&str>) -> World {
        let dir = Scratch::new("config-source");
        let kind = Arc::new(FakeKind::new("fake"));
        let mut kinds = Kinds::new();
        kinds.register(kind.clone()).unwrap();
        let named = env.map(|_| dir.path().join("keys.env"));
        let files = Files::new(&dir.path().join("whizbang.toml"), named.as_deref(), kinds);
        let world = World {
            dir,
            fs: Arc::new(ShimFs::new()),
            kind,
            files: Arc::new(files),
        };
        world.config(config);
        if let Some(env) = env {
            world.env(env);
        }
        world
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("whizbang.toml")
    }

    fn env_path(&self) -> PathBuf {
        self.dir.path().join("keys.env")
    }

    /// Writes the config again.
    fn config(&self, text: &str) {
        std::fs::write(self.config_path(), text).unwrap();
    }

    /// Writes the env file again.
    fn env(&self, text: &str) {
        std::fs::write(self.env_path(), text).unwrap();
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("store")
    }

    /// An executor with these actions over the source, with no backend of its own.
    fn executor(&self, actions: Vec<Arc<dyn Action>>) -> Engine {
        let mut registry = Registry::new();
        for action in actions {
            registry.register(action).unwrap();
        }
        let store = Arc::new(Store::open(&self.root(), self.fs.clone()).unwrap());
        Engine::over(registry, Arc::new(NoBackends), store, Digest::of_bytes(b""))
            .with_config(self.files.clone())
    }

    /// How many runs the store holds.
    fn runs(&self) -> usize {
        let reader = Reader::open(&self.root(), self.fs.clone()).unwrap();
        reader.snapshot().unwrap().runs().len()
    }

    /// The record of a run that ended.
    fn record(&self, finished: &Finished) -> RunRecord {
        let reader = Reader::open(&self.root(), self.fs.clone()).unwrap();
        let generation = reader.snapshot().unwrap();
        let entries = generation.run(finished.run().as_str()).unwrap().entries();
        RunRecord::from_bytes(&reader.read(&entries["record"]).unwrap()).unwrap()
    }

    /// How many requests the backends made under this name were sent, over every
    /// reading of the files.
    fn requests(&self, name: &str) -> u64 {
        let made = self.kind.made();
        made.iter()
            .filter(|(_, of, _)| of == name)
            .map(|(_, _, fake)| fake.requests())
            .sum()
    }

    /// Everything a failure says, and whether it holds the scratch folder's path or
    /// the name of either file.
    fn names_a_path(&self, said: &str) -> bool {
        said.contains(&self.dir.path().display().to_string())
            || said.contains("whizbang.toml")
            || said.contains("keys.env")
    }
}

fn said(error: &CallError) -> String {
    format!("{error:?} {error}")
}

fn config_failure(why: &str) -> CallError {
    CallError::Config {
        why: why.to_owned(),
    }
}

/// The model a prepared call resolved for its one need, or why it is not ready.
fn resolved(prepared: &PreparedCall) -> Result<ModelIdentity, String> {
    match prepared.readiness() {
        Readiness::Ready => Ok(prepared.needs()[0].model().unwrap().clone()),
        Readiness::NotReady(missing) => Err(missing[0].why().to_owned()),
    }
}

fn choosing(name: &str) -> Call {
    Call::new("asks").argument("backend", Value::text(name))
}

const TWO_AND_A_DEFAULT: &str = r#"
[backends.fill]
default = "one"

[backends.fill.one]
kind = "fake"
model = "first"

[backends.fill.two]
kind = "fake"
model = "second"

[backends.fill.two.settings]
features = "prompt"
"#;

// --- the roster --------------------------------------------------------------------------------

#[test]
fn two_backends_of_one_capability_and_a_default_resolve_a_choice_and_the_default_over_the_files() {
    // No env file is named: the config names no variable.
    let world = World::new(TWO_AND_A_DEFAULT, None);
    let executor = world.executor(vec![asks()]);
    let model = |name: &str| ModelIdentity::new("fake", name, "1");

    // A call that names none is answered by the default, and one that names the
    // other by the other (ADR 0006, "Resolution").
    let unnamed = executor.prepare(&Call::new("asks")).unwrap();
    assert_eq!(resolved(&unnamed), Ok(model("first")));
    assert_eq!(
        unnamed.config(),
        world.files.load().unwrap().config().digest()
    );
    let named = executor.prepare(&choosing("two")).unwrap();
    assert_eq!(resolved(&named), Ok(model("second")));
    // A name the config does not offer is answered by nothing, the default least.
    let absent = executor.prepare(&choosing("three")).unwrap();
    assert_eq!(
        resolved(&absent),
        Err("no fill backend is named \"three\"".to_owned())
    );
    assert_eq!(world.requests("one") + world.requests("two"), 0);

    // Each run is handed the backend its call resolved, and no other.
    let finished = executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.requests("one"), world.requests("two")), (1, 0));
    let finished = executor.submit(&choosing("two")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.requests("one"), world.requests("two")), (1, 1));
    let record = world.record(&finished);
    assert_eq!(record.asked().config, Some(named.config()));
    let refused = executor.submit(&choosing("three")).unwrap_err();
    assert!(matches!(
        refused,
        CallError::Refused(Refusal::NotReady { .. })
    ));
    assert_eq!(world.runs(), 2);
    assert_eq!((world.requests("one"), world.requests("two")), (1, 1));
}

#[test]
fn a_backend_whose_kind_this_build_does_not_have_is_not_ready_with_the_reason_and_never_another() {
    // The default is of a kind the build has not. Another backend is ready, and
    // declares the same model: it answers neither the default nor the choice.
    let config = r#"
[backends.fill]
default = "gone"

[backends.fill.gone]
kind = "no-such-kind"
model = "same"

[backends.fill.there]
kind = "fake"
model = "same"
"#;
    let world = World::new(config, None);
    let executor = world.executor(vec![asks()]);
    let unnamed = executor.prepare(&Call::new("asks")).unwrap();
    assert_eq!(
        resolved(&unnamed),
        Err("the default fill backend is not ready: its kind is not one this build has".to_owned())
    );
    let named = executor.prepare(&choosing("gone")).unwrap();
    assert_eq!(
        resolved(&named),
        Err(
            "the fill backend the call chose is not ready: its kind is not one this build has"
                .to_owned()
        )
    );
    for call in [Call::new("asks"), choosing("gone")] {
        let refused = executor.submit(&call).unwrap_err();
        let CallError::Refused(Refusal::NotReady { missing, .. }) = refused else {
            panic!("not refused as not ready: {refused:?}");
        };
        assert!(
            missing[0]
                .why()
                .ends_with("its kind is not one this build has")
        );
    }
    assert_eq!((world.runs(), world.requests("there")), (0, 0));
    // The one that is ready answers the call that names it.
    let finished = executor.submit(&choosing("there")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.runs(), world.requests("there")), (1, 1));
}

// --- a config that changes -------------------------------------------------------------------

fn one_filler(model: &str) -> String {
    format!("[backends.fill]\nkind = \"fake\"\nmodel = \"{model}\"\n")
}

#[test]
fn a_backend_changed_in_the_file_between_prepare_and_admission_is_a_stale_config() {
    let world = World::new(&one_filler("before"), None);
    let executor = world.executor(vec![asks()]);
    let call = Call::new("asks");
    let prepared = executor.prepare(&call).unwrap();
    // The backend changes in the file. The same executor refuses what was prepared.
    world.config(&one_filler("after"));
    let refused = executor
        .request(&Request::new(call.clone()).expecting(&prepared))
        .unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::Stale {
            action: "asks".to_owned(),
            why: Stale::Config,
        })
    );
    assert_eq!((world.runs(), world.requests("default")), (0, 0));
    // Prepared again under the file as it is now, it is admitted.
    let again = executor.prepare(&call).unwrap();
    assert_ne!(again.config(), prepared.config());
    assert_eq!(
        resolved(&again),
        Ok(ModelIdentity::new("fake", "after", "1"))
    );
    let answer = executor.request(&Request::new(call).expecting(&again));
    assert!(matches!(answer, Ok(Answer::Ran(_))), "{answer:?}");
    assert_eq!((world.runs(), world.requests("default")), (1, 1));
}

#[test]
fn a_config_that_does_not_parse_fails_the_next_call_in_fixed_words_with_no_run_and_the_call_after_it_is_put_right_is_admitted()
 {
    let good = one_filler("lama");
    let world = World::new(&good, None);
    let executor = world.executor(vec![asks()]);
    let id = executor.request_id("request-1").unwrap();
    let request = Request::new(Call::new("asks")).with_id(id);

    // A key the reader does not know, longer than a refusal shows and with a
    // letter outside ASCII in it; and text that is not TOML at all.
    let key = format!("caf\u{e9}-{}", "k".repeat(80));
    let unknown = format!("{good}\"{key}\" = 1\n");
    let cases = [
        (
            unknown.as_str(),
            "a key of the config is not known, is missing, or has a value that does not belong \
             there; the key is not named here",
        ),
        (
            "[backends.fill\nkind = \"fake\"\n",
            "the config is not TOML",
        ),
    ];
    for (broken, why) in cases {
        world.config(broken);
        let failed = config_failure(why);
        assert_eq!(executor.prepare(&Call::new("asks")), Err(failed.clone()));
        assert_eq!(executor.submit(&Call::new("asks")), Err(failed.clone()));
        assert_eq!(executor.request(&request), Err(failed.clone()));
        assert_eq!(executor.start(&request), Err(failed.clone()));
        // A failure, and no refusal of the call: the call was not looked at.
        assert_eq!(failed.refusal(), None);
        assert_eq!(
            failed.to_string(),
            format!("the config was not used: {why}")
        );
        // No path, and nothing of the file: not the key, not a word of it.
        let all = said(&failed);
        assert!(!world.names_a_path(&all), "{all}");
        assert!(!all.contains("caf") && !all.contains("kkkk") && !all.contains("fake"));
        assert_eq!((world.runs(), world.requests("default")), (0, 0));
    }

    // The key is named by the reader's own refusal, escaped and cut, with no path:
    // what loads the config directly says it.
    world.config(&unknown);
    let Err(LoadRefused::Config(ConfigRefused::Key { key: shown, .. })) = world.files.load() else {
        panic!("the reader took the key");
    };
    // The key's own part is escaped and cut; the tables above it are the reader's.
    let escaped = format!("caf\\u{{e9}}-{}", "k".repeat(80));
    let cut: String = escaped.chars().take(KEY_SHOWN_LIMIT).collect();
    assert_eq!(shown, format!("backends.fill.{cut}..."));
    let refused = world.files.load().unwrap_err();
    assert!(!world.names_a_path(&format!("{refused:?} {refused}")));

    // Put right, the same request is admitted under its id, which was not taken.
    world.config(&good);
    let Ok(Answer::Ran(finished)) = executor.request(&request) else {
        panic!("the request did not run");
    };
    assert_eq!(finished.outcome().name(), "completed");
    assert_eq!((world.runs(), world.requests("default")), (1, 1));
}

#[test]
fn an_env_file_that_cannot_be_read_as_one_fails_the_next_call_and_put_right_it_is_admitted() {
    let key = made_up_key("-env");
    let config =
        format!("[backends.fill]\nkind = \"fake\"\nkey_env = \"FILL_KEY\"\nurl = \"{API}/v1\"\n");
    let env = format!("FILL_KEY={key}\n");
    let world = World::new(&config, Some(&env));
    let executor = world.executor(vec![asks()]);
    assert_eq!(
        executor
            .submit(&Call::new("asks"))
            .unwrap()
            .outcome()
            .name(),
        "completed"
    );
    // A key pasted on a line of its own; then the file gone.
    world.env(&format!("{key}\n"));
    let failed = executor.submit(&Call::new("asks")).unwrap_err();
    assert_eq!(
        failed,
        config_failure("a line of the env file is not an assignment; the line is not shown")
    );
    assert!(!said(&failed).contains(&key) && !world.names_a_path(&said(&failed)));
    std::fs::remove_file(world.env_path()).unwrap();
    let failed = executor.submit(&Call::new("asks")).unwrap_err();
    assert_eq!(
        failed,
        config_failure("the env file cannot be read: it is not there")
    );
    assert!(!world.names_a_path(&said(&failed)));
    assert_eq!(world.runs(), 1);
    world.env(&env);
    assert_eq!(
        executor
            .submit(&Call::new("asks"))
            .unwrap()
            .outcome()
            .name(),
        "completed"
    );
    assert_eq!(world.runs(), 2);
}

#[test]
fn a_source_that_names_no_env_file_refuses_a_config_that_names_a_variable() {
    let config =
        format!("[backends.fill]\nkind = \"fake\"\nkey_env = \"FILL_KEY\"\nurl = \"{API}/v1\"\n");
    let world = World::new(&config, None);
    let executor = world.executor(vec![asks()]);
    let failed = executor.submit(&Call::new("asks")).unwrap_err();
    assert_eq!(
        failed,
        config_failure(
            "the config names a variable a credential is in, and no env file was given: \
             without one nothing is known to keep out of what is written, so the config is \
             not used; name the env file"
        )
    );
    assert!(!said(&failed).contains("FILL_KEY"));
    assert_eq!(world.runs(), 0);
}

#[test]
fn every_refusal_of_either_file_is_said_in_fixed_words_that_quote_nothing_of_it() {
    // What each refusal of the two readers holds that came from a file, or from the
    // system: a key, a variable's name, the number of a line, a limit.
    let marked = "MARKED_9731";
    let io = std::io::ErrorKind::PermissionDenied;
    let key = || marked.to_owned();
    let cases = [
        LoadRefused::Config(ConfigRefused::Unreadable(std::io::ErrorKind::NotFound)),
        LoadRefused::Config(ConfigRefused::Unreadable(std::io::ErrorKind::Interrupted)),
        LoadRefused::Config(ConfigRefused::Unreadable(io)),
        LoadRefused::Config(ConfigRefused::NotAFile),
        LoadRefused::Config(ConfigRefused::TooLarge { limit: 9731 }),
        LoadRefused::Config(ConfigRefused::NotText { line: 9731 }),
        LoadRefused::Config(ConfigRefused::HoldsSecret),
        LoadRefused::Config(ConfigRefused::NotToml { line: 9731 }),
        LoadRefused::Config(ConfigRefused::Twice {
            key: key(),
            line: 9731,
        }),
        LoadRefused::Config(ConfigRefused::Key {
            key: key(),
            why: key(),
        }),
        LoadRefused::Env(EnvRefused::Unreadable(std::io::ErrorKind::NotFound)),
        LoadRefused::Env(EnvRefused::Unreadable(std::io::ErrorKind::Interrupted)),
        LoadRefused::Env(EnvRefused::Unreadable(io)),
        LoadRefused::Env(EnvRefused::NotAFile),
        LoadRefused::Env(EnvRefused::TooLarge { limit: 9731 }),
        LoadRefused::Env(EnvRefused::NotText { line: 9731 }),
        LoadRefused::Env(EnvRefused::NotAssignment { line: 9731 }),
        LoadRefused::Env(EnvRefused::Twice { line: 9731 }),
        LoadRefused::Env(EnvRefused::Value { line: 9731 }),
        LoadRefused::Env(EnvRefused::TooMany { limit: 9731 }),
        LoadRefused::Env(EnvRefused::TooShort { variable: key() }),
        LoadRefused::Env(EnvRefused::NameIsAValue),
        LoadRefused::Env(EnvRefused::Unprotected),
        LoadRefused::NoEnvFile,
    ];
    let mut sentences = Vec::new();
    for refused in &cases {
        let why = SourceRefused::from(refused).why();
        assert!(!why.contains(marked) && !why.contains("9731"), "{why}");
        // It names the file it is about, as "the config" or "the env file".
        let names = match refused {
            LoadRefused::Config(_) => "the config",
            LoadRefused::Env(_) | LoadRefused::NoEnvFile => "the env file",
        };
        assert!(why.contains(names), "{why}");
        assert!(why.len() <= 256 && !why.contains('/') && !why.contains('`'));
        sentences.push(why);
    }
    // Each kind of refusal has its own sentence.
    sentences.sort_unstable();
    sentences.dedup();
    assert_eq!(sentences.len(), cases.len());
}

#[test]
fn a_run_that_was_admitted_goes_on_under_its_snapshot_though_the_file_changed_while_it_ran() {
    let world = Arc::new(World::new(&one_filler("one"), None));
    let changed = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let outer = {
        let (world, changed, seen) = (world.clone(), changed.clone(), seen.clone());
        let needs = vec![Requirement::new(Capability::Fill)];
        action("outer", needs, move |_, context| {
            // The config changes while the run executes, and then stops parsing.
            if !changed.swap(true, Ordering::SeqCst) {
                world.config(&one_filler("two"));
            }
            let fill = context.capabilities().fill().unwrap();
            fill.fill(&fill_request()).unwrap();
            // What the run calls is prepared and admitted under the run's snapshot.
            let prepared = context.executor().prepare(&Call::new("asks")).unwrap();
            seen.lock()
                .unwrap()
                .push((prepared.config(), resolved(&prepared)));
            context.executor().submit(&Call::new("asks")).unwrap();
            Ok(Produced::nothing())
        })
    };
    let executor = world.executor(vec![outer, asks()]);
    let one = world.files.load().unwrap().config().digest();
    let model = |name: &str| ModelIdentity::new("fake", name, "1");

    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    // The files were read once, for the run nothing called: one backend was made,
    // of the first config, and it answered the run and the run it called.
    let made = world.kind.made();
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].2.requests(), 2);
    assert_eq!(*seen.lock().unwrap(), [(one, Ok(model("one")))]);
    assert_eq!(world.record(&finished).asked().config, Some(one));

    // The next run is admitted under the file as it is now.
    let finished = executor.submit(&Call::new("outer")).unwrap();
    assert_eq!(finished.outcome().name(), "completed");
    let two = world.files.load().unwrap().config().digest();
    assert_ne!(one, two);
    let made = world.kind.made();
    assert_eq!(made.len(), 2);
    assert_eq!((made[0].2.requests(), made[1].2.requests()), (2, 2));
    assert_eq!(seen.lock().unwrap()[1], (two, Ok(model("two"))));
    assert_eq!(world.record(&finished).asked().config, Some(two));
    assert_eq!(world.runs(), 4);
}

// --- the secrets -----------------------------------------------------------------------------

/// `tells`: fails, saying the texts it was built with.
fn tells(texts: Vec<String>) -> Arc<dyn Action> {
    action("tells", Vec::new(), move |_, _| {
        Err(Failure::error("told", &texts.join(" and ")))
    })
}

fn message(finished: &Finished) -> String {
    let Outcome::Error { message, .. } = finished.outcome() else {
        panic!("not an error");
    };
    message.clone()
}

#[test]
fn the_snapshot_brings_every_credential_of_the_config_it_read() {
    let (key, token, other) = (made_up_key("-K"), made_up_key("-T"), made_up_key("-J"));
    let config = format!(
        "[backends.fill]\nkind = \"fake\"\nkey_env = \"FILL_KEY\"\ntoken_env = \"FILL_TOKEN\"\n\
         url = \"{API}/v1\"\n[backends.judge]\nkind = \"fake\"\nkey_env = \"JUDGE_KEY\"\n\
         url = \"{API}/v2\"\n"
    );
    let env = format!("FILL_KEY={key}\nFILL_TOKEN={token}\nJUDGE_KEY={other}\n");
    let world = World::new(&config, Some(&env));
    let all = vec![key.clone(), token.clone(), other.clone()];
    let executor = world.executor(vec![tells(all.clone())]);
    // Before the files are read the executor knows none of them.
    assert!(executor.request_id("request-1").is_ok());
    let finished = executor.submit(&Call::new("tells")).unwrap();
    assert_eq!(
        message(&finished),
        "[redacted] and [redacted] and [redacted]"
    );
    for one in &all {
        let refused = executor.request_id(one).unwrap_err();
        assert_eq!(refused, CallError::Refused(Refusal::SecretInRequestId));
    }
    // Each backend was handed its own, as the source read them.
    let carried: Vec<(String, Option<String>, Option<String>)> = world
        .kind
        .shown()
        .iter()
        .map(|shown| {
            let value = |credential: &Option<whizbang_core::egress::Credential>| {
                credential.as_ref().map(|one| one.expose().to_owned())
            };
            (shown.name.clone(), value(&shown.key), value(&shown.token))
        })
        .collect();
    assert_eq!(
        carried,
        [
            ("default".to_owned(), Some(key), Some(token)),
            ("default".to_owned(), Some(other), None),
        ]
    );
}

#[test]
fn a_variable_taken_out_of_the_env_file_after_a_call_was_prepared_refuses_the_call_under_the_same_digest()
 {
    // The digest covers nothing of the env file (ADR 0011, "The digest"): what the
    // env file changes for a call is whether its backend is ready, and that is
    // asked again at admission, of the files as they are then.
    let key = made_up_key("-gone");
    let config =
        format!("[backends.fill]\nkind = \"fake\"\nkey_env = \"FILL_KEY\"\nurl = \"{API}/v1\"\n");
    let world = World::new(&config, Some(&format!("FILL_KEY={key}\n")));
    let executor = world.executor(vec![asks()]);
    let call = Call::new("asks");
    let prepared = executor.prepare(&call).unwrap();
    assert_eq!(prepared.readiness(), &Readiness::Ready);
    world.env("");
    // What was prepared resolved a model, and nothing answers now: it is stale, by
    // its model and not by its config.
    let refused = executor
        .request(&Request::new(call.clone()).expecting(&prepared))
        .unwrap_err();
    assert_eq!(
        refused,
        CallError::Refused(Refusal::Stale {
            action: "asks".to_owned(),
            why: Stale::Model {
                capability: Capability::Fill
            },
        })
    );
    assert_eq!(executor.prepare(&call).unwrap().config(), prepared.config());
    // And the call with nothing expected of it is not ready, with the reason.
    let refused = executor.submit(&call).unwrap_err();
    let CallError::Refused(Refusal::NotReady { missing, .. }) = refused else {
        panic!("not refused as not ready: {refused:?}");
    };
    assert_eq!(
        missing[0].why(),
        "the default fill backend is not ready: the env file does not set FILL_KEY, the \
         variable its credential is in"
    );
    assert_eq!((world.runs(), world.requests("default")), (0, 0));
}

#[test]
fn a_config_that_holds_a_key_of_an_earlier_reading_is_not_used_and_a_kinds_sentence_that_holds_one_is_not_shown()
 {
    let (old, new) = (made_up_key("-old"), made_up_key("-new"));
    let config = |model: &str| {
        format!(
            "[backends.fill]\nkind = \"fake\"\nmodel = \"{model}\"\nkey_env = \"FILL_KEY\"\n\
             url = \"{API}/v1\"\n"
        )
    };
    let world = World::new(&config("lama"), Some(&format!("FILL_KEY={old}\n")));
    let executor = world.executor(vec![asks()]);
    executor.submit(&Call::new("asks")).unwrap();
    // The key is rotated, and the old one is pasted into the config as a model.
    // The env file as it is now does not hold it; the source read it before.
    world.env(&format!("FILL_KEY={new}\n"));
    world.config(&config(&old));
    let failed = executor.prepare(&Call::new("asks")).unwrap_err();
    assert_eq!(
        failed,
        config_failure(
            "the config holds a secret: a credential is named by its variable and is never \
             written in the file; nothing of the file is shown"
        )
    );
    assert_eq!(executor.submit(&Call::new("asks")), Err(failed));
    assert_eq!(
        world.files.load(),
        Err(LoadRefused::Config(ConfigRefused::HoldsSecret))
    );
    assert_eq!(world.runs(), 1);

    // A kind that says a key it was shown at an earlier reading: the sentence is
    // looked at with every secret the source has read, and is not shown.
    world.config(&config("lama"));
    let leaked: &'static str = Box::leak(format!("the key was {old}").into_boxed_str());
    world.kind.refuse_with(leaked);
    let prepared = executor.prepare(&Call::new("asks")).unwrap();
    assert_eq!(
        resolved(&prepared),
        Err(format!(
            "the default fill backend is not ready: its kind does not take it: \
             {KIND_SENTENCE_WITHHELD}"
        ))
    );
}

#[test]
fn a_reading_that_would_bring_more_keys_than_a_source_keeps_is_refused_and_none_is_let_go() {
    // A source that knows 4,000 secrets from the start, and a config whose two
    // backends name 96 and then 97 keys between them would be too slow to write:
    // the bound is on what readings add, so the source is told of all but two.
    assert_eq!(SECRETS_KEPT_LIMIT, 4096);
    let numbered = |from: usize, to: usize| (from..to).map(|at| format!("secret-value-{at:06}"));
    let config = format!(
        "[backends.fill]\nkind = \"fake\"\nkey_env = \"FILL_KEY\"\ntoken_env = \"FILL_TOKEN\"\n\
         url = \"{API}/v1\"\n"
    );
    let env = |key: usize, token: usize| {
        format!("FILL_KEY=secret-value-{key:06}\nFILL_TOKEN=secret-value-{token:06}\n")
    };
    let mut world = World::new(&config, Some(&env(4094, 4095)));
    let kinds = {
        let mut kinds = Kinds::new();
        kinds.register(world.kind.clone()).unwrap();
        kinds
    };
    let files = Files::new(&world.config_path(), Some(&world.env_path()), kinds)
        .knowing(Secrets::new(numbered(0, 4094)).unwrap());
    world.files = Arc::new(files);
    let executor = world.executor(vec![asks()]);
    // Two more are 4,096, and are taken.
    executor.submit(&Call::new("asks")).unwrap();
    // One rotated would be 4,097: the reading is refused, with no run.
    world.env(&env(4094, 4096));
    let failed = executor.submit(&Call::new("asks")).unwrap_err();
    assert_eq!(
        failed,
        config_failure(
            "the env file has given more keys, over every reading, than a source of the \
             config keeps, and none is let go"
        )
    );
    assert_eq!(world.runs(), 1);
    // A reading that adds none is answered, and none was let go: a config that
    // holds the first of them, or one of the first reading, is still refused.
    world.env(&env(4095, 4094));
    executor.submit(&Call::new("asks")).unwrap();
    assert_eq!(world.runs(), 2);
    for held in ["secret-value-000000", "secret-value-004095"] {
        world.config(&format!("{config}model = \"{held}\"\n"));
        assert_eq!(
            world.files.load(),
            Err(LoadRefused::Config(ConfigRefused::HoldsSecret))
        );
    }
    // The one that was refused was not kept.
    world.config(&format!("{config}model = \"secret-value-004096\"\n"));
    assert!(world.files.load().is_ok());

    // The bound is on what readings add. A source told of more than it keeps
    // answers a reading that adds none, and refuses one that adds any.
    world.config(&config);
    let kinds = {
        let mut kinds = Kinds::new();
        kinds.register(world.kind.clone()).unwrap();
        kinds
    };
    let over = Files::new(&world.config_path(), Some(&world.env_path()), kinds)
        .knowing(Secrets::new(numbered(0, 4100)).unwrap());
    assert!(over.snapshot().is_ok());
    world.env(&env(4094, 4100));
    assert_eq!(
        over.snapshot().unwrap_err().why(),
        "the env file has given more keys, over every reading, than a source of the config \
         keeps, and none is let go"
    );
}

#[test]
fn a_source_prints_no_path_and_no_key() {
    let key = made_up_key("-printed");
    let config =
        format!("[backends.fill]\nkind = \"fake\"\nkey_env = \"FILL_KEY\"\nurl = \"{API}/v1\"\n");
    let world = World::new(&config, Some(&format!("FILL_KEY={key}\n")));
    world.files.snapshot().unwrap();
    let printed = format!("{:?}", world.files);
    assert_eq!(printed, "Files { env_file: true, kinds: 1, secrets: 1 }");
    assert!(!world.names_a_path(&printed) && !printed.contains(&key));
    let secrets = Secrets::new([key.clone()]).unwrap();
    assert_eq!(world.files.load().unwrap().secrets(), &secrets);
}
