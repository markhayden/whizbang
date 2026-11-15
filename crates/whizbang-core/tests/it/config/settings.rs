//! The settings a run records (ADR 0011, "Which settings a run records"; ADR 0013, "A
//! setting"; plan 1B, the function of task C5 that needs no snapshot): from a config
//! and the backends a call resolved, names and values in the order of the names.

use whizbang_core::config::{Config, ConfigRefused, SettingsRefused};
use whizbang_core::contract::record::{SETTING_NAME_LIMIT, Setting};
use whizbang_core::contract::{Capability, Secrets, Value};

use super::made_up_key;

fn config(text: &str) -> Config {
    Config::parse(text.as_bytes(), &Secrets::none()).unwrap()
}

fn text(name: &str, value: &str) -> (String, Value) {
    (name.to_owned(), Value::text(value))
}

/// Everything a refusal says of itself.
fn said(refused: &SettingsRefused) -> String {
    format!("{refused:?} {refused}")
}

const FULL: &str = r#"
[backends.fill]
default = "bria"

[backends.fill.bria]
kind = "service"
provider = "fal"
model = "bria-erase"
url = "HTTPS://Fal.Example:443/v1/Erase"
key_env = "FAL_KEY"
token_env = "FAL_TOKEN"

[backends.fill.bria.settings]
strength = 7
upscale = false
mode = "fast"

[backends.fill.local]
kind = "pack"
model = "lama"

[backends.segment]
kind = "remote"
url = "https://segment.example"
token_env = "SEGMENT_TOKEN"

[spend]
call_cap_usd = 2.5

[egress.recipients.fal]
origin = "https://fal.example"
account = "team-1234"
classes = ["prompts"]
"#;

#[test]
fn the_settings_of_a_run_are_the_resolved_backends_and_the_cap_sorted_by_name() {
    let config = config(FULL);
    let none = Secrets::none();
    let of_bria = config.settings_of(&[(Capability::Fill, "bria")], &none);
    assert_eq!(
        of_bria,
        Ok(vec![
            text("backends.fill.bria.kind", "service"),
            text("backends.fill.bria.model", "bria-erase"),
            text("backends.fill.bria.provider", "fal"),
            text("backends.fill.bria.settings.mode", "fast"),
            (
                "backends.fill.bria.settings.strength".to_owned(),
                Value::Integer(7)
            ),
            (
                "backends.fill.bria.settings.upscale".to_owned(),
                Value::Boolean(false)
            ),
            // The address as the config keeps it: the origin as an origin writes
            // itself, and the path as the file gave it.
            text("backends.fill.bria.url", "https://fal.example/v1/Erase"),
            (
                "spend.call_cap_micros".to_owned(),
                Value::Integer(2_500_000)
            ),
        ])
    );
    // Two backends, of two capabilities, given in any order and one of them twice:
    // one list, in the order of the names' bytes, each name once.
    let both = [
        (Capability::Segment, "default"),
        (Capability::Fill, "local"),
        (Capability::Segment, "default"),
    ];
    let pairs = config.settings_of(&both, &none).unwrap();
    assert_eq!(
        pairs,
        vec![
            text("backends.fill.local.kind", "pack"),
            text("backends.fill.local.model", "lama"),
            text("backends.segment.default.kind", "remote"),
            text("backends.segment.default.url", "https://segment.example"),
            (
                "spend.call_cap_micros".to_owned(),
                Value::Integer(2_500_000)
            ),
        ]
    );
    let mut turned = both;
    turned.reverse();
    assert_eq!(config.settings_of(&turned, &none).unwrap(), pairs);
    let names: Vec<&str> = pairs.iter().map(|(name, _)| name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted);

    // No variable's name, no backend the call did not resolve, nothing of `egress`.
    let printed = format!(
        "{:?}",
        config.settings_of(&[(Capability::Fill, "bria")], &none)
    );
    for absent in [
        "FAL_KEY",
        "FAL_TOKEN",
        "key_env",
        "token_env",
        "lama",
        "team-1234",
        "egress",
    ] {
        assert!(!printed.contains(absent), "{absent}");
    }
    // A call that resolved nothing records the cap alone; no config, nothing.
    assert_eq!(
        config.settings_of(&[], &none).unwrap(),
        [(
            "spend.call_cap_micros".to_owned(),
            Value::Integer(2_500_000)
        )]
    );
    assert_eq!(Config::empty().settings_of(&[], &none), Ok(Vec::new()));
}

#[test]
fn a_backend_the_config_does_not_give_is_refused_and_its_name_is_not_shown() {
    let config = config(FULL);
    let key = made_up_key("");
    for (capability, name) in [
        (Capability::Judge, "bria"),
        (Capability::Fill, "default"),
        (Capability::Segment, "bria"),
        (Capability::Fill, key.as_str()),
    ] {
        let refused = config
            .settings_of(
                &[(Capability::Fill, "bria"), (capability, name)],
                &Secrets::none(),
            )
            .unwrap_err();
        assert_eq!(refused, SettingsRefused::NotOffered { capability });
        assert!(!said(&refused).contains(name), "{name}");
    }
    assert_eq!(
        SettingsRefused::NotOffered {
            capability: Capability::Judge
        }
        .to_string(),
        "a judge backend whose settings were asked for is not one the config gives; its \
         name is not shown"
    );
}

#[test]
fn a_settings_text_has_the_secrets_taken_out_whole_and_is_then_cut_as_a_record_keeps_one() {
    // A config read before the executor's secrets were known, as one read with no env
    // file is: a key written into a setting's text, a model and a provider.
    let key = made_up_key("");
    let filler = "x".repeat(250);
    let text_of = format!(
        r#"
[backends.fill.a]
kind = "fake"
model = "m-{key}"

[backends.fill.a.settings]
note = "Bearer {key} and more"
long = "{filler}{key}{filler}"
plain = "{long}"
"#,
        long = "y".repeat(1024)
    );
    let config = config(&text_of);
    let secrets = Secrets::new([key.clone()]).unwrap();
    let pairs = config
        .settings_of(&[(Capability::Fill, "a")], &secrets)
        .unwrap();
    assert_eq!(
        pairs,
        vec![
            text("backends.fill.a.kind", "fake"),
            text("backends.fill.a.model", "m-[redacted]"),
            // Taken out while whole, then cut at 256 characters: the key stood across
            // the cut, and nothing of it is on either side.
            text("backends.fill.a.settings.long", &format!("{filler}[redac")),
            text(
                "backends.fill.a.settings.note",
                "Bearer [redacted] and more"
            ),
            text("backends.fill.a.settings.plain", &"y".repeat(256)),
        ]
    );
    let printed = format!("{pairs:?}");
    for start in 0..key.len() - 5 {
        assert!(!printed.contains(&key[start..start + 5]));
    }
    // Each pair is one a record takes as it is: a setting made of it is the pair.
    for (name, value) in &pairs {
        let recorded = Setting::new(name, value, &secrets).unwrap();
        assert_eq!(
            serde_json::to_value(recorded.value()).unwrap(),
            serde_json::to_value(Setting::new(name, value, &Secrets::none()).unwrap().value())
                .unwrap(),
            "{name}"
        );
    }
    // With no secret told, the texts are as the file gave them, cut.
    let untold = config
        .settings_of(&[(Capability::Fill, "a")], &Secrets::none())
        .unwrap();
    assert_eq!(
        untold[1],
        text("backends.fill.a.model", &format!("m-{key}"))
    );
}

#[test]
fn what_a_record_would_not_take_is_refused_naming_the_setting_and_never_its_value() {
    // A whole number whose digits hold a secret: a record writes a number as it is.
    let digits = "1234567890";
    let secrets = Secrets::new([digits.to_owned()]).unwrap();
    let numbered = config(&format!(
        "[backends.fill.a]\nkind = \"fake\"\n[backends.fill.a.settings]\nseed = 9{digits}\n"
    ));
    let refused = numbered
        .settings_of(&[(Capability::Fill, "a")], &secrets)
        .unwrap_err();
    assert_eq!(
        refused,
        SettingsRefused::Value {
            name: "backends.fill.a.settings.seed".to_owned()
        }
    );
    assert_eq!(
        refused.to_string(),
        "the setting backends.fill.a.settings.seed is a number whose digits hold a secret, \
         and a record writes a number as it is; the value is not shown"
    );
    assert!(!said(&refused).contains(digits));
    // The cap is a number too.
    let capped = config("[spend]\ncall_cap_usd = 1234.56789\n");
    let cap_secret = Secrets::new(["1234567890".to_owned()]).unwrap();
    assert_eq!(
        capped.settings_of(&[], &cap_secret),
        Err(SettingsRefused::Value {
            name: "spend.call_cap_micros".to_owned()
        })
    );
    assert!(capped.settings_of(&[], &Secrets::none()).is_ok());

    // A name that holds a secret: a backend's name, or a setting's key. Not shown.
    let name = "abcdefgh-0123";
    let named = config(&format!(
        "[backends.fill.{name}]\nkind = \"fake\"\n[backends.judge.j]\nkind = \"fake\"\n\
         [backends.judge.j.settings]\nabcdefgh_0123 = true\n"
    ));
    for (resolved, secret) in [
        ((Capability::Fill, name), name),
        ((Capability::Judge, "j"), "abcdefgh_0123"),
        // Across a word the name is made with and a key of the file.
        ((Capability::Judge, "j"), "settings.abcdefgh"),
        ((Capability::Fill, name), "fill.abcdefgh"),
    ] {
        let secrets = Secrets::new([secret.to_owned()]).unwrap();
        let refused = named.settings_of(&[resolved], &secrets).unwrap_err();
        assert_eq!(refused, SettingsRefused::HoldsSecret, "{secret}");
        assert!(!said(&refused).contains("abcdefgh"));
    }
}

#[test]
fn the_longest_names_a_config_can_hold_are_recorded_and_one_character_more_is_refused_where_the_config_is_read()
 {
    // A setting is recorded as `backends.<capability>.<name>.settings.<key>`, and a
    // record takes a name of 128 characters. A backend's name and a setting's key
    // may each be 64, which with the rest would come to 155 for `segment`: so the
    // two together are held to what is left of the 128, where the config is read.
    assert_eq!(SETTING_NAME_LIMIT, 128);
    let table = |capability: &str, name: &str, key: &str| {
        format!(
            "[backends.{capability}.{name}]\nkind = \"fake\"\n\
             [backends.{capability}.{name}.settings]\n{key} = 1\nshort = 2\n"
        )
    };
    for capability in [Capability::Fill, Capability::Judge, Capability::Segment] {
        let of = capability.name();
        let around = format!("backends.{of}..settings.").len();
        let room = SETTING_NAME_LIMIT - around;
        // The longest name with the longest key that still fits, and the longest
        // key with the longest name that still fits.
        for name_len in [64, room - 64] {
            let (name, key) = ("b".repeat(name_len), "k".repeat(room - name_len));
            let longest = config(&table(of, &name, &key));
            let pairs = longest
                .settings_of(&[(capability, name.as_str())], &Secrets::none())
                .unwrap();
            let recorded = format!("backends.{of}.{name}.settings.{key}");
            assert_eq!(recorded.len(), SETTING_NAME_LIMIT);
            assert_eq!(pairs[1], (recorded.clone(), Value::Integer(1)));
            // And a record takes each pair.
            for (name, value) in &pairs {
                assert!(Setting::new(name, value, &Secrets::none()).is_some());
            }

            // One character more, in the key or in the name, is refused by the
            // reader, which names the key and shows no value.
            for (name, key) in [
                (name.clone(), format!("{key}k")),
                (format!("{name}b"), key.clone()),
            ] {
                if name.len() > 64 || key.len() > 64 {
                    continue;
                }
                let refused = Config::parse(table(of, &name, &key).as_bytes(), &Secrets::none());
                assert_eq!(
                    refused,
                    Err(ConfigRefused::Key {
                        key: format!("backends.{of}.{name}.settings.{key}"),
                        why: "would be recorded under a name of 129 characters, and a run's \
                              record takes a setting's name of 128 at most: shorten the \
                              backend's name or the key by 1"
                            .to_owned(),
                    })
                );
            }
        }
        // The longest of both, which the reader took until the two bounds agreed.
        let (name, key) = ("b".repeat(64), "k".repeat(64));
        let refused = Config::parse(table(of, &name, &key).as_bytes(), &Secrets::none());
        let Err(ConfigRefused::Key { why, .. }) = refused else {
            panic!("the longest name and the longest key were read");
        };
        let over = around + 128 - SETTING_NAME_LIMIT;
        assert!(why.ends_with(&format!("or the key by {over}")), "{why}");
        // The short form's backend is named `default`, and every key fits it.
        let short =
            format!("[backends.{of}]\nkind = \"fake\"\n[backends.{of}.settings]\n{key} = 1\n");
        let pairs = config(&short)
            .settings_of(&[(capability, "default")], &Secrets::none())
            .unwrap();
        assert_eq!(pairs[1].0, format!("backends.{of}.default.settings.{key}"));
    }
}

#[test]
fn more_settings_than_a_record_lists_and_more_backends_than_a_config_holds_are_refused() {
    // Three backends of 31 settings each, and their kinds: 96, which is as many as a
    // record lists. One more setting, or the cap, is one too many.
    let table = |capability: &str, settings: usize| {
        let mut text =
            format!("[backends.{capability}]\nkind = \"fake\"\n[backends.{capability}.settings]\n");
        for n in 0..settings {
            text.push_str(&format!("s{n:02} = {n}\n"));
        }
        text
    };
    let all = [
        (Capability::Fill, "default"),
        (Capability::Judge, "default"),
        (Capability::Segment, "default"),
    ];
    let full = config(&format!(
        "{}{}{}",
        table("fill", 31),
        table("judge", 31),
        table("segment", 31)
    ));
    assert_eq!(full.settings_of(&all, &Secrets::none()).unwrap().len(), 96);
    let over = config(&format!(
        "{}{}{}",
        table("fill", 32),
        table("judge", 31),
        table("segment", 31)
    ));
    let refused = over.settings_of(&all, &Secrets::none()).unwrap_err();
    assert_eq!(refused, SettingsRefused::TooMany { limit: 96 });
    assert_eq!(
        refused.to_string(),
        "there are more than 96 backends or settings to record for one run"
    );
    assert_eq!(
        over.settings_of(&all[1..], &Secrets::none()).unwrap().len(),
        64
    );
    let capped = config(&format!(
        "{}{}{}[spend]\ncall_cap_usd = 1\n",
        table("fill", 31),
        table("judge", 31),
        table("segment", 31)
    ));
    assert_eq!(
        capped.settings_of(&all, &Secrets::none()),
        Err(SettingsRefused::TooMany { limit: 96 })
    );
    // More backends than a config can hold are refused before any is looked for.
    let named = vec![(Capability::Fill, "default"); 48];
    assert!(full.settings_of(&named, &Secrets::none()).is_ok());
    let named = vec![(Capability::Fill, "nowhere"); 49];
    assert_eq!(
        full.settings_of(&named, &Secrets::none()),
        Err(SettingsRefused::TooMany { limit: 48 })
    );
}
