//! Keys from an env file (ADR 0011, "The env file"; plan 1B, task C2). The file is read
//! first and whole, as untrusted input; every value in it that could be a secret is one
//! the config's reader is told of; and only the variables the config names become
//! credentials, each handed to the backend whose table names it. The keys here are made
//! up and put together at run time, and every file is written into the test's own
//! scratch folder.

use std::sync::Arc;

use whizbang_core::config::{
    Config, ConfigRefused, Credentials, ENV_BYTES_LIMIT, ENV_VALUE_LIMIT, ENV_VARIABLES_LIMIT,
    EnvFile, EnvRefused, LoadRefused, Loaded,
};
use whizbang_core::contract::{
    Call, Capability, Engine, Executor, NoBackends, Outcome, Registry, SECRET_MIN_BYTES, Secrets,
    Value,
};
use whizbang_core::identity::Digest;
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_testkit::echo::Echo;
use whizbang_testkit::scratch::Scratch;

use super::made_up_key;

fn read_env(text: &str) -> EnvFile {
    match EnvFile::parse(text.as_bytes()) {
        Ok(env) => env,
        Err(refused) => panic!("refused: {refused}"),
    }
}

fn refusal(text: &str) -> EnvRefused {
    match EnvFile::parse(text.as_bytes()) {
        Ok(_) => panic!("read, and should be refused"),
        Err(refused) => refused,
    }
}

/// A config with one fill backend whose key is in `FAL_KEY`, read as the order is: told
/// of every value of the env file.
fn fill_config(env: &EnvFile) -> Config {
    let text = "[backends.fill]\nkind = \"service\"\nkey_env = \"FAL_KEY\"\n";
    Config::parse(text.as_bytes(), &env.secrets()).unwrap()
}

/// Everything a value says of itself: its sentence, when it has one, and its `{:?}`.
fn said<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn a_quoted_value_a_line_break_and_an_export_prefix_each_give_the_bare_key() {
    let key = made_up_key("");
    for line in [
        format!("FAL_KEY={key}\n"),
        format!("FAL_KEY={key}"),
        format!("FAL_KEY={key}\r\n"),
        format!("FAL_KEY=\"{key}\"\n"),
        format!("FAL_KEY='{key}'\n"),
        format!("export FAL_KEY={key}\n"),
        format!("export\tFAL_KEY='{key}'\r\n"),
        format!("  export  FAL_KEY=  \"{key}\"  \n"),
        format!("# the key\n\nFAL_KEY={key}   \n\n# after\n"),
    ] {
        let env = read_env(&line);
        let credentials = env.credentials(&fill_config(&env)).unwrap();
        // The bare key is what is taken out of a body, with no quote left around the
        // marker and no part of the key left beside it.
        let body = format!("{{\"authorization\":\"Bearer {key}\"}}");
        assert_eq!(
            credentials.secrets().redact(&body),
            "{\"authorization\":\"Bearer [redacted]\"}",
            "{}",
            line.replace(&key, "<key>")
        );
        assert_eq!(
            env.secrets().redact(&body),
            credentials.secrets().redact(&body)
        );
        // And the bare key is what the backend is handed.
        let handed = credentials.of(Capability::Fill, "default");
        assert_eq!(handed.key().map(|key| key.expose()), Some(key.as_str()));
    }
    // A quote of one kind inside quotes of the other is part of the value.
    let odd = format!("{key}\"x");
    let env = read_env(&format!("FAL_KEY='{odd}'\n"));
    let credentials = env.credentials(&fill_config(&env)).unwrap();
    let handed = credentials.of(Capability::Fill, "default");
    assert_eq!(handed.key().map(|key| key.expose()), Some(odd.as_str()));
}

#[test]
fn a_short_value_is_refused_with_the_variables_name_and_nothing_of_the_value() {
    assert_eq!(SECRET_MIN_BYTES, 8);
    for short in ["zqxj", "z", "zqxjkvw", "'zqxjkvw'", "\"zq\""] {
        let env = read_env(&format!("FAL_KEY={short}\n"));
        let refused = env.credentials(&fill_config(&env)).unwrap_err();
        assert_eq!(
            refused,
            EnvRefused::TooShort {
                variable: "FAL_KEY".to_owned()
            }
        );
        assert_eq!(
            refused.to_string(),
            "the env file's value for FAL_KEY is shorter than 8 bytes, which is too short to \
             be a credential or to be kept out of what is written; the value is not shown"
        );
        // Nothing of the value, and not its length.
        let bare = short.trim_matches(['\'', '"']);
        assert!(!said(&refused).contains(bare), "{refused:?}");
        assert!(!said(&refused).contains(&bare.len().to_string()));
    }
    // Eight bytes is a credential.
    let env = read_env("FAL_KEY=zqxjkvwp\n");
    env.credentials(&fill_config(&env)).unwrap();
    // A short value of a variable no backend names is not a credential, and is not
    // refused: the file may hold what is not a key.
    let key = made_up_key("");
    let env = read_env(&format!("PORT=8080\nFAL_KEY={key}\n"));
    let credentials = env.credentials(&fill_config(&env)).unwrap();
    assert_eq!(credentials.secrets(), &Secrets::new([key]).unwrap());
}

#[test]
fn a_line_that_is_not_an_assignment_and_a_name_given_twice_are_refused() {
    let marker = "zqxjkvalue";
    for (text, expected) in [
        (format!("{marker}\n"), EnvRefused::NotAssignment { line: 1 }),
        (
            format!("# ok\nA_KEY=abcdefgh1\n{marker} {marker}\n"),
            EnvRefused::NotAssignment { line: 3 },
        ),
        (
            format!("={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("lower={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("9KEY={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("A KEY={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("A_KEY ={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("A-KEY={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("exportA_KEY={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("set A_KEY={marker}\n"),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("{}={marker}\n", "K".repeat(65)),
            EnvRefused::NotAssignment { line: 1 },
        ),
        (
            format!("A_KEY={marker}1\n\nB_KEY={marker}2\nA_KEY={marker}3\n"),
            EnvRefused::Twice { line: 4 },
        ),
        (
            format!("A_KEY=\nexport A_KEY={marker}\n"),
            EnvRefused::Twice { line: 2 },
        ),
        // A value the reader cannot take for one thing.
        (
            format!("A_KEY={marker} # mine\n"),
            EnvRefused::Value { line: 1 },
        ),
        (
            format!("A_KEY={marker} {marker}\n"),
            EnvRefused::Value { line: 1 },
        ),
        (format!("A_KEY=\"{marker}\n"), EnvRefused::Value { line: 1 }),
        (
            format!("A_KEY='{marker}'{marker}\n"),
            EnvRefused::Value { line: 1 },
        ),
        (
            format!("A_KEY='{marker}'{marker}'\n"),
            EnvRefused::Value { line: 1 },
        ),
        (
            format!("A_KEY=\"{marker}\\n\"\n"),
            EnvRefused::Value { line: 1 },
        ),
        (format!("A_KEY={marker}'\n"), EnvRefused::Value { line: 1 }),
        (
            format!("B_KEY=x\nA_KEY={}\n", "v".repeat(4097)),
            EnvRefused::Value { line: 2 },
        ),
    ] {
        let refused = refusal(&text);
        assert_eq!(refused, expected);
        let all = format!("{refused}\n{refused:?}");
        assert!(!all.contains(marker) && !all.contains("KKKK"), "{all}");
    }
    assert_eq!(
        EnvRefused::NotAssignment { line: 3 }.to_string(),
        "line 3 of the env file is not an assignment: NAME=value, with a name of capital \
         letters, digits and _, not a digit first, 64 characters at most; the line is not shown"
    );
    assert_eq!(
        EnvRefused::Twice { line: 4 }.to_string(),
        "line 4 of the env file gives a name the file has already given; the line is not shown"
    );
    // At the bounds each is read.
    assert_eq!((ENV_VALUE_LIMIT, ENV_VARIABLES_LIMIT), (4096, 256));
    read_env(&format!("A_KEY={}\n", "v".repeat(4096)));
    let many = |count: usize| (0..count).map(|n| format!("K{n}=v\n")).collect::<String>();
    read_env(&many(256));
    assert_eq!(refusal(&many(257)), EnvRefused::TooMany { limit: 256 });
}

#[test]
fn a_malformed_line_whose_name_holds_a_value_of_another_line_is_refused_without_showing_it() {
    // The key is pasted a second time, where a name belongs: with a colon, with a
    // space, and alone. The refusal has the line's number and nothing of the line.
    let key = made_up_key("");
    for bad in [
        format!("{key}: yes\n"),
        format!("{key} = yes\n"),
        format!("{key}=yes\n"),
        format!("{key}\n"),
        format!("export {key}\n"),
    ] {
        for text in [
            format!("FAL_KEY={key}\n{bad}"),
            format!("{bad}FAL_KEY={key}\n"),
        ] {
            let refused = refusal(&text);
            assert!(
                matches!(refused, EnvRefused::NotAssignment { .. }),
                "{refused:?}"
            );
            let all = format!("{refused}\n{refused:?}");
            for start in 0..key.len() - 4 {
                assert!(!all.contains(&key[start..start + 4]), "{all}");
            }
        }
    }
    // A value that is itself a well-formed name, given twice as a name: the line's
    // number, and not the name.
    let capitals = format!("ZQXJ{}", "ABCDEFGHIJ012345");
    let refused = refusal(&format!("FAL_KEY={capitals}\n{capitals}=1\n{capitals}=2\n"));
    assert_eq!(refused, EnvRefused::Twice { line: 3 });
    assert!(!format!("{refused}\n{refused:?}").contains("ZQXJ"));
    // And where a config names such a variable, nothing is made of it and the name is
    // not said back: by the config's reader when it was told of the file's values, and
    // by the env file when it was not.
    let env = read_env(&format!("FAL_KEY={capitals}\n{capitals}=abcdefgh1\n"));
    let text = format!("[backends.fill]\nkind = \"a\"\nkey_env = \"{capitals}\"\n");
    assert_eq!(
        Config::parse(text.as_bytes(), &env.secrets()).unwrap_err(),
        ConfigRefused::HoldsSecret
    );
    let untold = Config::parse(text.as_bytes(), &Secrets::none()).unwrap();
    let refused = env.credentials(&untold).unwrap_err();
    assert_eq!(refused, EnvRefused::NameIsAValue);
    assert!(!format!("{refused}\n{refused:?}").contains("ZQXJ"));
}

#[test]
fn a_file_over_its_bound_is_refused_from_its_length_and_no_refusal_has_a_path() {
    assert_eq!(ENV_BYTES_LIMIT, 64 * 1024);
    let limit = usize::try_from(ENV_BYTES_LIMIT).unwrap();
    let mut text = "A_KEY=abcdefgh1\n#".to_owned();
    text.push_str(&"c".repeat(limit - text.len()));
    read_env(&text);
    text.push('c');
    let too_large = EnvRefused::TooLarge {
        limit: ENV_BYTES_LIMIT,
    };
    assert_eq!(refusal(&text), too_large);
    let scratch = Scratch::new("env-bound");
    let folder = scratch.path().file_name().unwrap().to_str().unwrap();
    let path = scratch.path().join("zqxjk.env");
    std::fs::write(&path, &text[..limit]).unwrap();
    EnvFile::read(&path).unwrap();
    std::fs::write(&path, &text).unwrap();
    assert_eq!(EnvFile::read(&path).unwrap_err(), too_large);
    // Refused before it is opened to read: a file nobody may read, over the bound, is
    // refused for its length. (A process that file permissions do not hold shows
    // nothing here.)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(EnvFile::read(&path).unwrap_err(), too_large);
        // What is not a file is refused before it is opened: the device of endless
        // zeros, which says its length is nothing.
        let endless = std::path::Path::new("/dev/zero");
        assert_eq!(EnvFile::read(endless).unwrap_err(), EnvRefused::NotAFile);
    }
    // A file that is not there, a folder, and a line that is not an assignment: no
    // refusal has a path or a name of a file.
    let broken = scratch.path().join("broken-zqxjk.env");
    std::fs::write(&broken, "not an assignment\n").unwrap();
    let missing = EnvFile::read(&scratch.path().join("missing-zqxjk.env")).unwrap_err();
    assert_eq!(
        missing,
        EnvRefused::Unreadable(std::io::ErrorKind::NotFound)
    );
    assert_eq!(
        missing.to_string(),
        "the env file cannot be read: it is not there"
    );
    assert_eq!(
        EnvFile::read(scratch.path()).unwrap_err(),
        EnvRefused::NotAFile
    );
    assert_eq!(
        EnvRefused::NotAFile.to_string(),
        "the env file cannot be read: it is not a file"
    );
    for refused in [
        missing,
        EnvFile::read(scratch.path()).unwrap_err(),
        EnvFile::read(&broken).unwrap_err(),
    ] {
        let all = format!("{refused}\n{refused:?}");
        assert!(!all.contains(folder) && !all.contains("zqxjk"), "{all}");
        assert!(!all.contains('/') && !all.contains(".env"), "{all}");
    }
    // Bytes that are not text.
    let mut bytes = b"A_KEY=abcdefgh1\nB_KEY=".to_vec();
    bytes.extend_from_slice(&[0xff, b'\n']);
    assert_eq!(
        EnvFile::parse(&bytes).unwrap_err(),
        EnvRefused::NotText { line: 2 }
    );
    for control in [0x00u8, 0x07, 0x0b, 0x1b, 0x7f] {
        let mut bytes = b"# c\n\nA_KEY=abcdefgh".to_vec();
        bytes.push(control);
        assert_eq!(
            EnvFile::parse(&bytes).unwrap_err(),
            EnvRefused::NotText { line: 3 }
        );
    }
}

#[test]
fn the_env_file_is_read_first_and_a_config_that_holds_any_of_its_values_is_refused() {
    // Every value of the file that could be a secret is one the config's reader is
    // told of, whether or not the config names its variable: a value in the env file
    // is never said back from the config.
    let (named, other) = (made_up_key("-named"), made_up_key("-other"));
    let env = read_env(&format!(
        "FAL_KEY={named}\nexport OTHER_TOKEN='{other}'\nPORT=8080\n"
    ));
    assert_eq!(
        env.secrets(),
        Secrets::new([named.clone(), other.clone()]).unwrap()
    );
    for held in [&named, &other] {
        for text in [
            format!("[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\nmodel = \"{held}\"\n"),
            format!("[backends.fill]\nkind = \"a\"\nkey_env = \"{held}\"\n"),
            format!("# {held}\n"),
            format!("\"{held}\" = 1\n"),
        ] {
            assert_eq!(
                Config::parse(text.as_bytes(), &env.secrets()).unwrap_err(),
                ConfigRefused::HoldsSecret
            );
        }
    }
    // A key of the shape of a host and a path. A backend's address that only folding
    // turns into it, by lowering its host or dropping its scheme's own port, is
    // refused like any other place the key stands.
    let shaped = format!("zqxjkvwpyh{}Path9-0123", "/");
    let env_shaped = read_env(&format!("FAL_KEY='{shaped}'\n"));
    for url in [
        format!("https://{shaped}"),
        "https://ZQXJKVWPYH/Path9-0123".to_owned(),
        "https://zqxjkvwpyh:443/Path9-0123".to_owned(),
    ] {
        let text =
            format!("[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\nurl = \"{url}\"\n");
        assert_eq!(
            Config::parse(text.as_bytes(), &env_shaped.secrets()).unwrap_err(),
            ConfigRefused::HoldsSecret,
            "{url}"
        );
    }
    // A value too short to be a secret is not looked for: `8080` may stand in a config.
    let text = "[backends.fill]\nkind = \"a\"\nurl = \"http://127.0.0.1:8080\"\n";
    Config::parse(text.as_bytes(), &env.secrets()).unwrap();
    // The executor's secrets are the credentials the config names that have a value,
    // and not the rest of the file.
    let credentials = env.credentials(&fill_config(&env)).unwrap();
    assert_eq!(credentials.secrets(), &Secrets::new([named]).unwrap());
}

#[test]
fn a_backend_is_handed_its_own_credential_and_cannot_ask_for_anothers() {
    let (fal, judge_key, judge_token) = (
        made_up_key("-fal"),
        made_up_key("-judge-key"),
        made_up_key("-judge-token"),
    );
    let env = read_env(&format!(
        "FAL_KEY={fal}\nJUDGE_KEY={judge_key}\nJUDGE_TOKEN={judge_token}\nUNNAMED={}\n",
        made_up_key("-unnamed")
    ));
    let text = "[backends.fill.bria]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\n\
        [backends.fill.local]\nkind = \"b\"\n\
        [backends.fill.unset]\nkind = \"a\"\nkey_env = \"NOT_SET\"\ntoken_env = \"FAL_KEY\"\n\
        [backends.judge.bria]\nkind = \"c\"\nkey_env = \"JUDGE_KEY\"\ntoken_env = \"JUDGE_TOKEN\"\n";
    let config = Config::parse(text.as_bytes(), &env.secrets()).unwrap();
    let credentials = env.credentials(&config).unwrap();
    let expose = |capability, name| {
        let handed = credentials.of(capability, name);
        (
            handed.key().map(|key| key.expose().to_owned()),
            handed.token().map(|token| token.expose().to_owned()),
        )
    };
    // Each backend is handed what its own table names, by the place it has in the
    // config. There is no question by a variable's name, and none that answers all.
    assert_eq!(expose(Capability::Fill, "bria"), (Some(fal.clone()), None));
    assert_eq!(expose(Capability::Fill, "local"), (None, None));
    assert_eq!(expose(Capability::Fill, "unset"), (None, Some(fal.clone())));
    assert_eq!(
        expose(Capability::Judge, "bria"),
        (Some(judge_key.clone()), Some(judge_token.clone()))
    );
    // A backend the config does not have, and one of the same name under a capability
    // that has none, are handed nothing.
    assert_eq!(expose(Capability::Fill, "other"), (None, None));
    assert_eq!(expose(Capability::Segment, "bria"), (None, None));
    // A variable the config names and the file does not set is said by its name, and
    // is no refusal here: whether a backend can do without is the backend's.
    assert_eq!(credentials.missing().collect::<Vec<_>>(), ["NOT_SET"]);
    // The secrets are the three that are named and set; the unnamed one is not there.
    assert_eq!(
        credentials.secrets(),
        &Secrets::new([fal, judge_key, judge_token]).unwrap()
    );
    // A variable set to nothing is as one not set.
    let env = read_env("FAL_KEY=\n");
    let credentials = env.credentials(&fill_config(&env)).unwrap();
    assert_eq!(
        expose_none(&credentials),
        (true, vec!["FAL_KEY".to_owned()])
    );
    // No file, and no config.
    let none = EnvFile::empty().credentials(&Config::empty()).unwrap();
    assert_eq!(none.secrets(), &Secrets::none());
    assert_eq!(said(&none), said(&Credentials::none()));
}

fn expose_none(credentials: &Credentials) -> (bool, Vec<String>) {
    let handed = credentials.of(Capability::Fill, "default");
    (
        handed.key().is_none() && handed.token().is_none(),
        credentials.missing().map(str::to_owned).collect(),
    )
}

#[test]
fn a_marker_key_is_in_no_error_no_debug_no_record_and_no_journal_of_a_run_that_used_it() {
    // The key holds a dot, as a key of several parts does.
    let key = made_up_key(".marker.0123");
    let scratch = Scratch::new("env-marker");
    let path = scratch.path().join("keys.env");
    std::fs::write(&path, format!("export FAL_KEY=\"{key}\"\nPORT=8080\n")).unwrap();
    let env = EnvFile::read(&path).unwrap();
    let config = fill_config(&env);
    let credentials = env.credentials(&config).unwrap();
    let handed = credentials.of(Capability::Fill, "default");
    // No `{:?}` of anything that holds it shows it.
    let shown = format!("{env:?} {credentials:?} {handed:?} {:?}", handed.key());
    assert_eq!(
        shown,
        "EnvFile(2) Credentials { backends: 1, missing: 0, secrets: Secrets(1) } \
         Handed { key: true, token: false } Some(KeyValue(..))"
    );
    // No refusal that the file or the config can be brought to shows it.
    let refusals = [
        format!("{:?}", refusal(&format!("FAL_KEY={key}\nFAL_KEY={key}\n"))),
        format!("{}", refusal(&format!("FAL_KEY={key} {key}\n"))),
        format!("{}", refusal(&format!("FAL_KEY={key}\n{key}\n"))),
        format!(
            "{:?}",
            Config::parse(format!("# {key}\n").as_bytes(), &env.secrets()).unwrap_err()
        ),
    ];
    for text in refusals.iter().chain([&shown]) {
        assert!(!text.contains(&key) && !text.contains(&key[..6]), "{text}");
    }
    // A run that used it: the executor is given the credentials' secrets, and the
    // key is planted in an argument, which the journal quotes, and in the error's
    // message. It is in no file of the store.
    let store = scratch.path().join("store");
    let mut registry = Registry::new();
    registry.register(Arc::new(Echo::new())).unwrap();
    let engine = Engine::over(
        registry,
        Arc::new(NoBackends),
        Arc::new(Store::open(&store, Arc::new(RealFs)).unwrap()),
        Digest::of_bytes(b""),
    )
    .with_secrets(credentials.secrets().clone());
    let call = Call::new("echo")
        .argument(
            "text",
            Value::text(&format!("Bearer {}", handed.key().unwrap().expose())),
        )
        .argument("fail", Value::text(&format!("refused with {key}")));
    let finished = engine.submit(&call).unwrap();
    let Outcome::Error { message, .. } = finished.outcome() else {
        panic!("{finished:?}");
    };
    assert_eq!(message, "refused with [redacted]");
    assert!(!format!("{finished:?}").contains(&key));
    drop(engine);
    let mut files = 0;
    let mut dirs = vec![store];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                assert!(
                    !holds(&bytes, key.as_bytes()),
                    "a file of the store holds the key"
                );
                files += 1;
            }
        }
    }
    assert!(files >= 4, "the store's files were looked at: {files}");
}

/// Env files in which one value has white space at an end inside its quotes. Such a
/// value is 8 bytes as it stands and fewer once trimmed, which is how a set of secrets
/// counts it; the set was then refused whole, and the refusal was swallowed.
fn files_with_a_padded_value(key: &str) -> Vec<String> {
    vec![
        format!("FAL_KEY={key}\nNOTE=' abcdefg'\n"),
        format!("FAL_KEY={key}\nNOTE=\"abcdefg \"\n"),
        format!("NOTE=\"\tabcdefg\"\nFAL_KEY={key}\n"),
        format!("NOTE=' abcdefgh '\nFAL_KEY={key}\n"),
    ]
}

#[test]
fn a_padded_value_does_not_empty_the_secrets_a_config_is_read_with() {
    // The first face: with the set emptied, a config that held another key of the
    // file word for word was read. Whatever the reader makes of such a file, that
    // must not be: it refuses the file, or the key is still a secret.
    let key = made_up_key("");
    for text in files_with_a_padded_value(&key) {
        let Ok(env) = EnvFile::parse(text.as_bytes()) else {
            continue;
        };
        assert_ne!(env.secrets(), Secrets::none(), "the set is empty");
        let config = format!("[backends.fill]\nkind = \"a\"\nmodel = \"{key}\"\n");
        assert_eq!(
            Config::parse(config.as_bytes(), &env.secrets()).unwrap_err(),
            ConfigRefused::HoldsSecret
        );
    }
}

#[test]
fn a_padded_value_does_not_empty_the_secrets_the_executor_takes_out() {
    // The second face: `credentials` succeeded with an empty set, so a backend was
    // handed its key and nothing would take that key out of what is written.
    let key = made_up_key("");
    let text = "[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\n\
        [backends.judge]\nkind = \"b\"\nkey_env = \"NOTE\"\n";
    let config = Config::parse(text.as_bytes(), &Secrets::none()).unwrap();
    for text in files_with_a_padded_value(&key) {
        let Ok(env) = EnvFile::parse(text.as_bytes()) else {
            continue;
        };
        let Ok(credentials) = env.credentials(&config) else {
            continue;
        };
        let handed = credentials.of(Capability::Fill, "default");
        let exposed = handed.key().expect("the key is handed over").expose();
        assert_eq!(
            credentials.secrets().redact(exposed),
            "[redacted]",
            "a key that was handed over is not taken out of what is written"
        );
    }
}

#[test]
fn a_padded_value_does_not_let_a_pasted_key_be_said_as_a_variables_name() {
    // The third face: with the set emptied, a key of capitals pasted where a
    // variable's name belongs was read by the config, and listed by name as missing.
    let key = made_up_key("");
    let capitals = format!("ZQXJ{}", "ABCDEFGHIJ012345");
    for text in files_with_a_padded_value(&key) {
        let text = format!("{text}OTHER={capitals}\n");
        let Ok(env) = EnvFile::parse(text.as_bytes()) else {
            continue;
        };
        let config = format!("[backends.fill]\nkind = \"a\"\nkey_env = \"{capitals}\"\n");
        match Config::parse(config.as_bytes(), &env.secrets()) {
            Err(refused) => assert_eq!(refused, ConfigRefused::HoldsSecret),
            Ok(config) => {
                let said = match env.credentials(&config) {
                    Ok(credentials) => credentials.missing().collect::<Vec<_>>().join(" "),
                    Err(refused) => format!("{refused} {refused:?}"),
                };
                assert!(!said.contains("ZQXJ"), "the pasted key is said: {said}");
            }
        }
    }
}

#[test]
fn white_space_at_an_end_inside_quotes_and_quotes_around_nothing_are_refused() {
    // A key never has white space at an end, and taking it off would hand over
    // something other than what was written. So it is refused, with the line's number
    // and nothing of the line; and so are quotes around nothing, or around white space
    // alone. White space outside the quotes is spacing, as before.
    let marker = "zqxjkvalue";
    for (text, line) in [
        (format!("A_KEY=' {marker}'\n"), 1),
        (format!("A_KEY='{marker} '\n"), 1),
        (format!("B_KEY=x\nA_KEY=\"{marker}\t\"\n"), 2),
        (format!("A_KEY=\"\t{marker}\"\n"), 1),
        (format!("export A_KEY='  {marker}  '\n"), 1),
        ("A_KEY=''\n".to_owned(), 1),
        ("A_KEY=\"\"\n".to_owned(), 1),
        ("A_KEY=' '\n".to_owned(), 1),
        ("A_KEY=\" \t \"\n".to_owned(), 1),
    ] {
        let refused = refusal(&text);
        assert_eq!(refused, EnvRefused::Value { line }, "{text:?}");
        assert!(!format!("{refused}\n{refused:?}").contains(marker));
    }
    let env = read_env(&format!("A_KEY=   '{marker} {marker}'   \nB_KEY=\n"));
    assert_eq!(
        env.secrets(),
        Secrets::new([format!("{marker} {marker}")]).unwrap()
    );
}

#[test]
fn no_more_than_the_bound_and_one_byte_is_asked_of_what_an_env_file_is_read_from() {
    // It gives without end as far as the reader under test may ask, and fails once it
    // has given a mebibyte: a reader that asked without a bound would otherwise fill
    // the machine's memory before anything stopped it.
    struct Endless(usize);
    impl std::io::Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.0 > 1024 * 1024 {
                return Err(std::io::Error::other("asked for more than a mebibyte"));
            }
            buffer.fill(b'#');
            self.0 += buffer.len();
            Ok(buffer.len())
        }
    }
    let mut endless = Endless(0);
    assert_eq!(
        EnvFile::read_from(&mut endless).unwrap_err(),
        EnvRefused::TooLarge {
            limit: ENV_BYTES_LIMIT
        }
    );
    assert_eq!(endless.0, usize::try_from(ENV_BYTES_LIMIT).unwrap() + 1);
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("zqxjk"))
        }
    }
    let refused = EnvFile::read_from(Broken).unwrap_err();
    assert_eq!(refused, EnvRefused::Unreadable(std::io::ErrorKind::Other));
    assert!(!format!("{refused} {refused:?}").contains("zqxjk"));
    assert_eq!(
        format!(
            "{:?}",
            EnvFile::read_from(&b"A_KEY=abcdefgh\n"[..]).unwrap()
        ),
        "EnvFile(1)"
    );
}

#[test]
fn one_call_reads_the_env_file_first_and_then_the_config_with_its_secrets() {
    // The way in: both paths to one function, which keeps the order. A config that
    // holds a value of the env file is refused though nothing told its reader of one
    // by hand; and what comes back is the config, each backend's credentials and the
    // executor's secrets, of one reading.
    let key = made_up_key("");
    let scratch = Scratch::new("env-loaded");
    let env_path = scratch.path().join("keys.env");
    let config_path = scratch.path().join("whizbang.toml");
    std::fs::write(
        &env_path,
        format!("FAL_KEY='{key}'\nOTHER={}\n", made_up_key("-other")),
    )
    .unwrap();
    std::fs::write(
        &config_path,
        "[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\n",
    )
    .unwrap();
    let loaded = Loaded::read(&config_path, Some(&env_path)).unwrap();
    assert_eq!(loaded.config().backends(Capability::Fill).count(), 1);
    let handed = loaded.credentials().of(Capability::Fill, "default");
    assert_eq!(handed.key().map(|key| key.expose()), Some(key.as_str()));
    assert_eq!(loaded.secrets(), &Secrets::new([key.clone()]).unwrap());
    assert_eq!(
        format!("{loaded:?}"),
        format!(
            "Loaded {{ config: {:?}, credentials: {:?} }}",
            loaded.config(),
            loaded.credentials()
        )
    );
    // A config that holds either value of the env file, the one no backend names too.
    // In a comment, and as a KEY of the file: read with no secrets told, the second
    // is a key that is not known and would be said back.
    for held in [key.clone(), made_up_key("-other")] {
        for text in [format!("# {held}\n"), format!("\"{held}\" = 1\n")] {
            std::fs::write(&config_path, text).unwrap();
            assert_eq!(
                Loaded::read(&config_path, Some(&env_path)).unwrap_err(),
                LoadRefused::Config(ConfigRefused::HoldsSecret)
            );
        }
    }
    // An env file that is refused is the refusal, and the config is not read: here it
    // is not there at all.
    std::fs::write(&env_path, "not an assignment\n").unwrap();
    assert_eq!(
        Loaded::read(&scratch.path().join("missing.toml"), Some(&env_path)).unwrap_err(),
        LoadRefused::Env(EnvRefused::NotAssignment { line: 1 })
    );
    let said = Loaded::read(&config_path, Some(&env_path))
        .unwrap_err()
        .to_string();
    assert!(said.starts_with("line 1 of the env file"), "{said}");
    // With no env file there is no credential, and a variable the config names is
    // missing.
    std::fs::write(
        &config_path,
        "[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\n",
    )
    .unwrap();
    let loaded = Loaded::read(&config_path, None).unwrap();
    assert_eq!(
        loaded.credentials().missing().collect::<Vec<_>>(),
        ["FAL_KEY"]
    );
    assert_eq!(loaded.secrets(), &Secrets::none());
}

#[test]
fn what_the_env_reader_does_not_do_is_known() {
    // ADR 0011, "What the env reader does not do". Each is so, and is pinned.
    let key = made_up_key("");
    // Nothing is expanded: a `$NAME` in a value is those letters.
    let env = read_env(&format!("OTHER={key}\nFAL_KEY=$OTHER-and-more\n"));
    let credentials = env.credentials(&fill_config(&env)).unwrap();
    let handed = credentials.of(Capability::Fill, "default");
    assert_eq!(
        handed.key().map(|key| key.expose()),
        Some("$OTHER-and-more")
    );
    // A `#` in a value with no space before it is part of the value, not a comment.
    let env = read_env(&format!("FAL_KEY={key}#1\n"));
    let credentials = env.credentials(&fill_config(&env)).unwrap();
    let handed = credentials.of(Capability::Fill, "default");
    assert_eq!(
        handed.key().map(|key| key.expose()),
        Some(format!("{key}#1").as_str())
    );
    // The process's own environment is not read: a variable set there and not in the
    // file is missing. (`PATH` is set wherever a test runs.)
    assert!(std::env::var_os("PATH").is_some());
    let text = "[backends.fill]\nkind = \"a\"\nkey_env = \"PATH\"\n";
    let config = Config::parse(text.as_bytes(), &Secrets::none()).unwrap();
    let credentials = EnvFile::empty().credentials(&config).unwrap();
    assert_eq!(credentials.missing().collect::<Vec<_>>(), ["PATH"]);
    // A byte-order mark at the head of the file is no part of a name, and is refused
    // with the first line.
    assert_eq!(
        EnvFile::parse("\u{feff}A_KEY=abcdefgh\n".as_bytes()).unwrap_err(),
        EnvRefused::NotAssignment { line: 1 }
    );
    // A link to a file is followed: what it leads to is read.
    #[cfg(unix)]
    {
        let scratch = Scratch::new("env-link");
        let file = scratch.path().join("keys.env");
        let link = scratch.path().join("link.env");
        std::fs::write(&file, "A_KEY=abcdefgh\n").unwrap();
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert_eq!(format!("{:?}", EnvFile::read(&link).unwrap()), "EnvFile(1)");
    }
    // A value of the file shorter than 8 bytes is not a secret to anyone: it is not
    // looked for in a config, and not taken out of what is written.
    let env = read_env("PIN=1234567\n");
    assert_eq!(env.secrets(), Secrets::none());
    // At 8 bytes it is one.
    let env = read_env("PIN=12345678\n");
    assert_eq!(
        env.secrets(),
        Secrets::new(["12345678".to_owned()]).unwrap()
    );
}
