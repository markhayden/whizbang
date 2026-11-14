//! Both files, read again each time they are asked for (ADR 0011, "Both files, each
//! time"; plan 1B, the part of task C4 that needs no snapshot). Every file is written
//! into the test's own scratch folder, and every key is made up at run time.

use whizbang_core::config::{ConfigRefused, EnvRefused, LoadRefused, Loaded};
use whizbang_core::contract::{Capability, Secrets};
use whizbang_testkit::scratch::Scratch;

use super::made_up_key;

const NAMES_A_KEY: &str = "[backends.fill]\nkind = \"fake\"\nkey_env = \"FAL_KEY\"\n";

#[test]
fn both_files_are_read_again_each_time_and_a_file_put_right_is_read() {
    let (first, second) = (made_up_key("-first"), made_up_key("-second"));
    let scratch = Scratch::new("config-files");
    let config = scratch.path().join("whizbang.toml");
    let env = scratch.path().join("keys.env");
    std::fs::write(&config, NAMES_A_KEY).unwrap();
    std::fs::write(&env, format!("FAL_KEY={first}\n")).unwrap();
    let read = || Loaded::of_files(&config, Some(&env));
    let key_of = |loaded: &Loaded| {
        let handed = loaded.credentials().of(Capability::Fill, "default");
        handed.key().map(|key| key.expose().to_owned())
    };

    let loaded = read().unwrap();
    assert_eq!(loaded, Loaded::read(&config, Some(&env)).unwrap());
    assert_eq!(key_of(&loaded), Some(first.clone()));
    assert_eq!(loaded.secrets(), &Secrets::new([first.clone()]).unwrap());
    let digest = loaded.config().digest();

    // The config changes: the next ask answers what it says now.
    std::fs::write(&config, format!("{NAMES_A_KEY}model = \"lama\"\n")).unwrap();
    let changed = read().unwrap();
    assert_ne!(changed.config().digest(), digest);
    let model = |loaded: &Loaded| {
        let mut backends = loaded.config().backends(Capability::Fill);
        backends
            .next()
            .and_then(|(_, backend)| backend.model().map(str::to_owned))
    };
    assert_eq!(model(&changed), Some("lama".to_owned()));

    // The key is rotated in the env file: the next ask hands the new one, and the
    // secrets of that reading are the new one's. The config's digest is as it was.
    std::fs::write(&env, format!("FAL_KEY={second}\n")).unwrap();
    let rotated = read().unwrap();
    assert_eq!(key_of(&rotated), Some(second.clone()));
    assert_eq!(rotated.secrets(), &Secrets::new([second.clone()]).unwrap());
    assert_eq!(rotated.config().digest(), changed.config().digest());

    // A file that does not parse is the refusal, with no path and none of its words;
    // put right, it is read.
    std::fs::write(&config, "[backends.fill\nkind = \"fake\"\n").unwrap();
    let refused = read().unwrap_err();
    assert_eq!(
        refused,
        LoadRefused::Config(ConfigRefused::NotToml { line: 1 })
    );
    let folder = scratch.path().display().to_string();
    let said = |refused: &LoadRefused| format!("{refused:?} {refused}");
    assert!(!said(&refused).contains(&folder));
    assert!(!said(&refused).contains("whizbang.toml"));
    std::fs::write(&config, NAMES_A_KEY).unwrap();
    assert_eq!(read().unwrap().config().digest(), digest);

    // The same of the env file: one that is not an env file, and one that is gone.
    std::fs::write(&env, format!("{second}\n")).unwrap();
    let refused = read().unwrap_err();
    assert_eq!(
        refused,
        LoadRefused::Env(EnvRefused::NotAssignment { line: 1 })
    );
    assert!(!said(&refused).contains(&second) && !said(&refused).contains(&folder));
    std::fs::remove_file(&env).unwrap();
    let refused = read().unwrap_err();
    assert_eq!(
        refused,
        LoadRefused::Env(EnvRefused::Unreadable(std::io::ErrorKind::NotFound))
    );
    assert!(!said(&refused).contains(&folder) && !said(&refused).contains("keys.env"));
    std::fs::write(&env, format!("FAL_KEY={first}\n")).unwrap();
    assert_eq!(key_of(&read().unwrap()), Some(first));
}

#[test]
fn with_no_env_file_a_config_that_names_a_variable_is_refused_and_one_that_names_none_is_read() {
    let scratch = Scratch::new("config-no-env");
    let config = scratch.path().join("whizbang.toml");
    for names in [
        NAMES_A_KEY,
        "[backends.judge.a]\nkind = \"fake\"\n[backends.judge.b]\nkind = \"fake\"\n\
         token_env = \"JUDGE_TOKEN\"\n",
    ] {
        std::fs::write(&config, names).unwrap();
        let refused = Loaded::of_files(&config, None).unwrap_err();
        assert_eq!(refused, LoadRefused::NoEnvFile);
        assert_eq!(
            refused.to_string(),
            "the config names a variable a credential is in, and no env file was given: \
             without one nothing is known to keep out of what is written, so the config is \
             not used; name the env file"
        );
        let said = format!("{refused:?} {refused}");
        assert!(!said.contains("FAL_KEY") && !said.contains("JUDGE_TOKEN"));
        assert!(!said.contains(&scratch.path().display().to_string()));
        // The reader of ADR 0011's three steps still reads it, with every variable
        // missing: the refusal is this reader's.
        let read = Loaded::read(&config, None).unwrap();
        assert_eq!(read.credentials().missing().count(), 1);
    }
    // A config that names no variable is read with no env file.
    let plain = "[backends.fill]\nkind = \"fake\"\nmodel = \"lama\"\n[spend]\ncall_cap_usd = 1\n";
    std::fs::write(&config, plain).unwrap();
    let loaded = Loaded::of_files(&config, None).unwrap();
    assert_eq!(loaded, Loaded::read(&config, None).unwrap());
    assert_eq!(loaded.config().call_cap_micros(), Some(1_000_000));
    assert_eq!(loaded.secrets(), &Secrets::none());
    // A config that is refused is its own refusal, with or without an env file.
    std::fs::write(&config, "[tools]\n").unwrap();
    assert!(matches!(
        Loaded::of_files(&config, None),
        Err(LoadRefused::Config(_))
    ));
    // An env file that is given and names nothing the config uses is no refusal.
    let env = scratch.path().join("keys.env");
    std::fs::write(&env, "").unwrap();
    std::fs::write(&config, NAMES_A_KEY).unwrap();
    let loaded = Loaded::of_files(&config, Some(&env)).unwrap();
    assert_eq!(
        loaded.credentials().missing().collect::<Vec<_>>(),
        ["FAL_KEY"]
    );
}
