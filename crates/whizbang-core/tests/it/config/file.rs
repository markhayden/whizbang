//! The config file and its strict reader (ADR 0011; plan 1B, task C1). A config is a
//! file a person writes, and is read as untrusted: its length before its bytes, every
//! key against a closed list, every value against its kind and its rule. A refusal names
//! a key, escaped and cut, and never shows a value or a path.

use whizbang_core::config::{
    BACKENDS_LIMIT, CONFIG_BYTES_LIMIT, Config, ConfigRefused, DataClass, KEY_SHOWN_LIMIT,
    RECIPIENTS_LIMIT, SETTINGS_LIMIT, Setting, UPSTREAMS_LIMIT,
};
use whizbang_core::contract::{Capability, Secrets};
use whizbang_testkit::scratch::Scratch;

use super::{made_up_key, the_specs_example};

fn read(text: &str) -> Config {
    match Config::parse(text.as_bytes(), &Secrets::none()) {
        Ok(config) => config,
        Err(refused) => panic!("refused: {refused}\n{text}"),
    }
}

fn refusal(text: &str) -> ConfigRefused {
    match Config::parse(text.as_bytes(), &Secrets::none()) {
        Ok(_) => panic!("read, and should be refused:\n{text}"),
        Err(refused) => refused,
    }
}

/// The key a refusal names, for a refusal that names one.
fn key_of(refused: &ConfigRefused) -> &str {
    match refused {
        ConfigRefused::Key { key, .. } | ConfigRefused::Twice { key, .. } => key,
        other => panic!("names no key: {other}"),
    }
}

/// Everything a refusal can say: its sentence and its `{:?}`.
fn said(refused: &ConfigRefused) -> String {
    format!("{refused}\n{refused:?}")
}

#[test]
fn the_specs_example_is_read_to_the_backends_it_shows() {
    let config = read(the_specs_example());
    // One backend for each of the two capabilities, each the default of its capability
    // and named `default`.
    let segment: Vec<_> = config.backends(Capability::Segment).collect();
    assert_eq!(segment.len(), 1);
    let (name, backend) = segment[0];
    assert_eq!(name, "default");
    assert_eq!(config.default_of(Capability::Segment), Some("default"));
    assert_eq!(backend.kind(), "remote");
    assert_eq!(backend.model(), Some("sam3"));
    assert_eq!(backend.url(), Some("https://example-host/whizbang-sam3"));
    assert_eq!(backend.provider(), None);
    assert_eq!(
        backend.credentials().collect::<Vec<_>>(),
        [("token_env", "SEGMENT_TOKEN")]
    );
    assert_eq!(backend.settings().count(), 0);
    let fill: Vec<_> = config.backends(Capability::Fill).collect();
    assert_eq!(fill.len(), 1);
    let (name, backend) = fill[0];
    assert_eq!(name, "default");
    assert_eq!(config.default_of(Capability::Fill), Some("default"));
    assert_eq!(backend.kind(), "service");
    assert_eq!(backend.provider(), Some("fal"));
    assert_eq!(backend.model(), Some("bria-erase"));
    assert_eq!(backend.url(), None);
    assert_eq!(
        backend.credentials().collect::<Vec<_>>(),
        [("key_env", "FAL_KEY")]
    );
    // Nothing else: no judge, no cap, no recipient.
    assert_eq!(config.backends(Capability::Judge).count(), 0);
    assert_eq!(config.default_of(Capability::Judge), None);
    assert_eq!(config.call_cap_micros(), None);
    assert_eq!(config.recipients().count(), 0);
    // The variables it names, and no value of either.
    assert_eq!(
        config.variables().into_iter().collect::<Vec<_>>(),
        ["FAL_KEY", "SEGMENT_TOKEN"]
    );
}

#[test]
fn several_backends_of_one_capability_are_written_by_name_with_a_default() {
    let config = read(
        r#"
[backends.fill]
default = "bria"

[backends.fill.bria]
kind = "service"
provider = "fal"
model = "bria-erase"
key_env = "FAL_KEY"

[backends.fill.local]
kind = "pack"
model = "lama"

[backends.fill.local.settings]
longest_side = 2048
tiled = true
rendering = "grey"
"#,
    );
    let fill: Vec<_> = config.backends(Capability::Fill).collect();
    assert_eq!(
        fill.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["bria", "local"]
    );
    assert_eq!(config.default_of(Capability::Fill), Some("bria"));
    let local = fill[1].1;
    assert_eq!(local.kind(), "pack");
    assert_eq!(local.credentials().count(), 0);
    assert_eq!(
        local.settings().collect::<Vec<_>>(),
        [
            ("longest_side", &Setting::Integer(2048)),
            ("rendering", &Setting::Text("grey".to_owned())),
            ("tiled", &Setting::Boolean(true)),
        ]
    );
    // Several backends and no default: none answers a call that names none (ADR 0006).
    let none = read("[backends.judge.one]\nkind = \"a\"\n[backends.judge.two]\nkind = \"b\"\n");
    assert_eq!(none.backends(Capability::Judge).count(), 2);
    assert_eq!(none.default_of(Capability::Judge), None);
    // A default names a backend that is written there.
    let stray =
        refusal("[backends.fill]\ndefault = \"other\"\n[backends.fill.one]\nkind = \"a\"\n");
    assert_eq!(key_of(&stray), "backends.fill.default");
    // The short form is one backend: a second beside it is not a setting of the first.
    let mixed = refusal("[backends.fill]\nkind = \"a\"\n[backends.fill.other]\nkind = \"b\"\n");
    assert_eq!(key_of(&mixed), "backends.fill.other");
    // In the long form `default` is the key that names the default, so no backend has
    // that name there, and `kind` is the key that makes a table the short form.
    let taken = refusal("[backends.fill.default]\nkind = \"a\"\n");
    assert_eq!(key_of(&taken), "backends.fill.default");
    let kind = refusal("[backends.fill.kind]\nkind = \"a\"\n");
    assert_eq!(key_of(&kind), "backends.fill.kind");
    // A backend's name keeps the rule of an action's name.
    for name in ["Upper", "9lives", "\"with space\"", "\"\""] {
        let bad = refusal(&format!("[backends.fill.{name}]\nkind = \"a\"\n"));
        assert!(key_of(&bad).starts_with("backends.fill."), "{bad}");
    }
    // One backend written without its kind is told so, by the key that is missing:
    // its keys are not taken for the names of backends. So none is named as one of
    // them is.
    for key in [
        "model",
        "provider",
        "url",
        "key_env",
        "token_env",
        "settings",
    ] {
        let missing = refusal(&format!("[backends.fill]\n{key} = \"zqxjkvalue\"\n"));
        assert_eq!(key_of(&missing), "backends.fill.kind", "{key}");
        let sentence = missing.to_string();
        assert!(sentence.contains("is missing") && !sentence.contains("zqxjk"));
        let named = refusal(&format!("[backends.fill.{key}]\nkind = \"a\"\n"));
        assert_eq!(key_of(&named), "backends.fill.kind", "{key}");
    }
    // A backend has a kind.
    let kindless = refusal("[backends.fill.one]\nmodel = \"m\"\n");
    assert_eq!(key_of(&kindless), "backends.fill.one.kind");
}

#[test]
fn an_unknown_table_or_key_is_refused_naming_the_key() {
    for (text, key) in [
        ("colour = 1\n", "colour"),
        ("[provider]\nmodel = \"m\"\n", "provider"),
        ("[backends.paint]\nkind = \"a\"\n", "backends.paint"),
        (
            "[backends.fill]\nkind = \"a\"\nmodle = \"m\"\n",
            "backends.fill.modle",
        ),
        (
            "[backends.fill.one]\nkind = \"a\"\nkey = \"K\"\n",
            "backends.fill.one.key",
        ),
        ("[spend]\nallowance_usd = 5\n", "spend.allowance_usd"),
        ("[egress]\nallow = true\n", "egress.allow"),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\ntoken = \"t\"\n",
            "egress.recipients.one.token",
        ),
        ("[models]\ndir = \"m\"\n", "models.dir"),
    ] {
        let refused = refusal(text);
        assert_eq!(key_of(&refused), key, "{refused}");
        let sentence = refused.to_string();
        assert!(sentence.contains("is not known"), "{sentence}");
        assert!(sentence.contains(&format!("`{key}`")), "{sentence}");
    }
}

#[test]
fn tools_is_refused_until_it_has_a_shape_and_models_holds_nothing_yet() {
    for text in ["[tools]\n", "[tools]\npaths = [\"a.py\"]\n", "tools = 1\n"] {
        let refused = refusal(text);
        assert_eq!(key_of(&refused), "tools");
        assert!(refused.to_string().contains("milestone 2"), "{refused}");
    }
    // A table with nothing in it is as no table: the config of such a file is the
    // empty one, with no capability that has no backend.
    for text in [
        "",
        "[backends]\n",
        "[backends.fill]\n",
        "[backends]\n[backends.fill]\n[backends.judge]\n[backends.segment]\n",
        "[spend]\n",
        "[egress]\n[egress.recipients]\n",
    ] {
        assert_eq!(read(text), Config::empty(), "{text}");
    }
    // `models` is a table of the file, and no key of it has a shape yet.
    assert_eq!(read("[models]\n"), Config::empty());
    assert_eq!(key_of(&refusal("models = 1\n")), "models");
}

#[test]
fn a_key_given_twice_is_refused_naming_the_key() {
    for (text, key, line) in [
        (
            "[spend]\ncall_cap_usd = 1\ncall_cap_usd = 2\n",
            "call_cap_usd",
            3,
        ),
        (
            "[backends.fill]\nkind = \"a\"\n\n[backends.fill]\nmodel = \"m\"\n",
            "fill",
            4,
        ),
        (
            "# twice\n[backends.fill.one]\nkind = \"a\"\n\"kind\" = \"b\"\n",
            "kind",
            4,
        ),
        // Under a heading that is some lines above, with a list between that begins
        // a line as a heading does; and at the top, under none. A dotted key is named
        // by its last key, which is where the format's reader points.
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nupstreams = [\n[\"x\"],\n]\norigin = \"https://i\"\n",
            "origin",
            6,
        ),
        ("a = 1\nb = 2\na = 3\n[spend]\n", "a", 3),
        (
            "spend.call_cap_usd = 1\nspend.call_cap_usd = 2\n",
            "call_cap_usd",
            2,
        ),
    ] {
        match refusal(text) {
            ConfigRefused::Twice {
                key: named,
                line: at,
            } => {
                assert_eq!(named, key, "{text}");
                assert_eq!(at, line, "{text}");
            }
            other => panic!("{other:?} for {text}"),
        }
    }
    // The key is named by its own keys alone, never by a line above it: a line that
    // begins as a heading does may be a value's text, inside a list written over lines
    // or a string of several lines. (The address is put together here from its parts.)
    let marker = "VALUEMARK9";
    let address = format!("https://user{}{marker}{}h", ":", "@");
    for above in [
        format!("upstreams = [\n[\"{address}\"]\n]\n"),
        format!("upstreams = [\n  [[\"{address}\"]]   # a comment\n]\n"),
        format!("account = \"\"\"\n[{marker}]\n\"\"\"\n"),
        format!("account = \'\'\'\n  [{marker}.{marker}]  \n\'\'\'\n"),
        format!("account = \"\"\"\r\n[{marker}]\r\n\"\"\"\r\n"),
    ] {
        let text = format!(
            "[egress.recipients.one]\n{above}origin = \"https://h\"\norigin = \"https://i\"\n"
        );
        let refused = refusal(&text);
        assert!(
            matches!(&refused, ConfigRefused::Twice { key, .. } if key == "origin"),
            "{refused:?}"
        );
        assert!(!said(&refused).contains(marker), "{refused:?}");
    }
    let sentence = refusal("a = 1\na = 2\n").to_string();
    assert_eq!(
        sentence,
        "the config gives the key `a` twice, the second time at line 2"
    );
}

#[test]
fn text_that_is_not_toml_is_refused_with_its_line_and_none_of_its_words() {
    for (text, line) in [
        ("[backends.fill\nkind = \"a\"\n", 1),
        ("[spend]\ncall_cap_usd = \n", 2),
        ("[spend]\n\n\ncall_cap_usd = \"unclosed\n", 4),
    ] {
        let refused = refusal(text);
        assert_eq!(refused, ConfigRefused::NotToml { line }, "{text}");
        let all = said(&refused);
        assert!(!all.contains("unclosed") && !all.contains("kind"), "{all}");
    }
    // Nesting past the reader's depth is not followed down: it is not TOML to this
    // reader, whatever it holds.
    let deep = format!("a = {}{}\n", "[".repeat(1000), "]".repeat(1000));
    assert!(matches!(refusal(&deep), ConfigRefused::NotToml { line: 1 }));
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_naming_the_key_and_never_the_value() {
    let marker = "zqxjkvalue";
    for (text, key) in [
        (format!("backends = \"{marker}\"\n"), "backends"),
        (
            format!("[backends]\nfill = \"{marker}\"\n"),
            "backends.fill",
        ),
        (
            format!("[backends.fill]\nkind = [\"{marker}\"]\n"),
            "backends.fill.kind",
        ),
        (
            format!("[backends.fill.one]\nkind = \"a\"\nmodel = {{ a = \"{marker}\" }}\n"),
            "backends.fill.one.model",
        ),
        (
            format!("[backends.fill]\ndefault = 7\n[backends.fill.one]\nkind = \"{marker}\"\n"),
            "backends.fill.default",
        ),
        (
            format!("[backends.fill]\nkind = \"a\"\nsettings = \"{marker}\"\n"),
            "backends.fill.settings",
        ),
        (
            format!(
                "[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nside = [\"{marker}\"]\n"
            ),
            "backends.fill.settings.side",
        ),
        (
            "[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nweight = 0.5\n".to_owned(),
            "backends.fill.settings.weight",
        ),
        (
            "[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nsince = 2026-01-01\n"
                .to_owned(),
            "backends.fill.settings.since",
        ),
        (
            format!("[spend]\ncall_cap_usd = \"{marker}\"\n"),
            "spend.call_cap_usd",
        ),
        (format!("spend = [\"{marker}\"]\n"), "spend"),
        (
            format!("[egress]\nrecipients = \"{marker}\"\n"),
            "egress.recipients",
        ),
        (
            format!("[egress.recipients.one]\norigin = 5\nclasses = [\"{marker}\"]\n"),
            "egress.recipients.one.origin",
        ),
        (
            format!("[egress.recipients.one]\norigin = \"https://h\"\nclasses = \"{marker}\"\n"),
            "egress.recipients.one.classes",
        ),
        (
            format!("[egress.recipients.one]\norigin = \"https://h\"\nclasses = [\"{marker}\"]\n"),
            "egress.recipients.one.classes",
        ),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\nprobe = \"yes\"\n"
                .to_owned(),
            "egress.recipients.one.probe",
        ),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\nupstreams = [1]\n"
                .to_owned(),
            "egress.recipients.one.upstreams",
        ),
    ] {
        let refused = refusal(&text);
        assert_eq!(key_of(&refused), key, "{refused}");
        let all = said(&refused);
        assert!(!all.contains(marker), "{all}");
        assert!(!all.contains("0.5") && !all.contains("2026"), "{all}");
    }
    // The sentence says what belongs there and what kind of thing was found.
    assert_eq!(
        refusal("[spend]\ncall_cap_usd = true\n").to_string(),
        "the config's key `spend.call_cap_usd` must be a number, and is true or false"
    );
}

#[test]
fn a_text_is_held_to_its_rule_and_its_length() {
    let long = "m".repeat(129);
    let wide = "s".repeat(1025);
    for (text, key) in [
        // A kind and a provider are names, as an action's name is.
        (
            "[backends.fill]\nkind = \"Remote\"\n".to_owned(),
            "backends.fill.kind",
        ),
        (
            "[backends.fill]\nkind = \"a\"\nprovider = \"f a l\"\n".to_owned(),
            "backends.fill.provider",
        ),
        // A model, an account and a setting's text: printable ASCII with no space at
        // either end, and not empty.
        (
            "[backends.fill]\nkind = \"a\"\nmodel = \"\"\n".to_owned(),
            "backends.fill.model",
        ),
        (
            "[backends.fill]\nkind = \"a\"\nmodel = \" m\"\n".to_owned(),
            "backends.fill.model",
        ),
        (
            "[backends.fill]\nkind = \"a\"\nmodel = \"m \"\n".to_owned(),
            "backends.fill.model",
        ),
        (
            "[backends.fill]\nkind = \"a\"\nmodel = \"m\\u0007\"\n".to_owned(),
            "backends.fill.model",
        ),
        (
            "[backends.fill]\nkind = \"a\"\nmodel = \"m\\nn\"\n".to_owned(),
            "backends.fill.model",
        ),
        (
            "[backends.fill]\nkind = \"a\"\nmodel = \"m\\u007f\"\n".to_owned(),
            "backends.fill.model",
        ),
        (
            "[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nnote = \"m\\u007f\"\n"
                .to_owned(),
            "backends.fill.settings.note",
        ),
        (
            "[backends.fill]\nkind = \"a\"\nmodel = \"mod\u{e8}le\"\n".to_owned(),
            "backends.fill.model",
        ),
        (
            format!("[backends.fill]\nkind = \"a\"\nmodel = \"{long}\"\n"),
            "backends.fill.model",
        ),
        (
            format!("[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nnote = \"{wide}\"\n"),
            "backends.fill.settings.note",
        ),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\naccount = \"a\\tb\"\n"
                .to_owned(),
            "egress.recipients.one.account",
        ),
        // A setting's key is a name without a hyphen.
        (
            "[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nlongest-side = 1\n"
                .to_owned(),
            "backends.fill.settings.longest-side",
        ),
    ] {
        let refused = refusal(&text);
        assert_eq!(key_of(&refused), key, "{refused}");
        let all = said(&refused);
        assert!(!all.contains(&long) && !all.contains(&wide), "{all}");
    }
    // At the bound each is read.
    let model = "m".repeat(128);
    let note = "s".repeat(1024);
    let config = read(&format!(
        "[backends.fill]\nkind = \"a\"\nmodel = \"{model}\"\n[backends.fill.settings]\nnote = \"{note}\"\n"
    ));
    let (_, backend) = config.backends(Capability::Fill).next().unwrap();
    assert_eq!(backend.model(), Some(model.as_str()));
    assert_eq!(
        backend.settings().next(),
        Some(("note", &Setting::Text(note)))
    );
}

#[test]
fn a_key_in_a_refusal_is_escaped_and_cut() {
    assert_eq!(KEY_SHOWN_LIMIT, 64);
    // A key of 300 characters: its first 64, and three dots.
    let long = "k".repeat(300);
    let refused_long = refusal(&format!("{long} = 1\n"));
    assert_eq!(key_of(&refused_long), format!("{}...", "k".repeat(64)));
    // A key with a line break, an escape character and a letter outside ASCII, as a
    // quoted key can hold them: each is shown escaped, so the sentence is one line of
    // ASCII, and the cut counts what is shown.
    let odd = refusal("\"a\\nb\\u001b[31m\u{e9}\" = 1\n");
    assert_eq!(key_of(&odd), "a\\nb\\u{1b}[31m\\u{e9}");
    let sentence = odd.to_string();
    assert!(sentence.is_ascii() && !sentence.contains('\n') && !sentence.contains('\u{1b}'));
    // The same inside a table: the part that came from the file is the part that is cut.
    let inner = refusal(&format!("[spend]\n{long} = 1\n"));
    assert_eq!(key_of(&inner), format!("spend.{}...", "k".repeat(64)));
    let named = refusal(&format!("[backends.fill.{long}]\nkind = \"a\"\n"));
    assert_eq!(
        key_of(&named),
        format!("backends.fill.{}...", "k".repeat(64))
    );
    // A key given twice is shown the same way.
    match refusal(&format!("{long} = 1\n{long} = 2\n")) {
        ConfigRefused::Twice { key, line } => {
            assert_eq!(key, format!("{}...", "k".repeat(64)));
            assert_eq!(line, 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn bytes_that_are_not_text_and_control_characters_are_refused() {
    let none = Secrets::none();
    // Not UTF-8.
    let mut bytes = b"[spend]\ncall_cap_usd = 1\n# ".to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe, b'\n']);
    assert_eq!(
        Config::parse(&bytes, &none).unwrap_err(),
        ConfigRefused::NotText { line: 3 }
    );
    // A control character, also in a comment, where the format's own reader need not
    // look. A tab, a line feed and a carriage return before a line feed are spacing.
    for control in [0x00u8, 0x07, 0x08, 0x0b, 0x0c, 0x1b, 0x7f] {
        let mut bytes = b"[spend]\n# a comment ".to_vec();
        bytes.push(control);
        bytes.extend_from_slice(b"\ncall_cap_usd = 1\n");
        assert_eq!(
            Config::parse(&bytes, &none).unwrap_err(),
            ConfigRefused::NotText { line: 2 },
            "{control:#x}"
        );
    }
    assert_eq!(
        read("[spend]\r\n\tcall_cap_usd = 1 # a tab\tin a comment\r\n").call_cap_micros(),
        Some(1_000_000)
    );
    // Letters outside ASCII are text, in a comment.
    read("# caf\u{e9}\n[spend]\ncall_cap_usd = 1\n");
}

#[test]
fn a_file_over_the_bound_is_refused_from_its_length() {
    assert_eq!(CONFIG_BYTES_LIMIT, 256 * 1024);
    let limit = usize::try_from(CONFIG_BYTES_LIMIT).unwrap();
    // As bytes: a comment that fills the file to the bound is read, one byte more is not.
    let mut text = "[spend]\ncall_cap_usd = 1\n#".to_owned();
    text.push_str(&"c".repeat(limit - text.len()));
    assert_eq!(read(&text).call_cap_micros(), Some(1_000_000));
    text.push('c');
    assert_eq!(
        refusal(&text),
        ConfigRefused::TooLarge {
            limit: CONFIG_BYTES_LIMIT
        }
    );
    // As a file: the same two, read through their path.
    let scratch = Scratch::new("config-bound");
    let path = scratch.path().join("whizbang.toml");
    std::fs::write(&path, &text[..limit]).unwrap();
    assert_eq!(
        Config::read(&path, &Secrets::none())
            .unwrap()
            .call_cap_micros(),
        Some(1_000_000)
    );
    std::fs::write(&path, &text).unwrap();
    assert_eq!(
        Config::read(&path, &Secrets::none()).unwrap_err(),
        ConfigRefused::TooLarge {
            limit: CONFIG_BYTES_LIMIT
        }
    );
}

#[cfg(unix)]
#[test]
fn a_file_over_the_bound_is_refused_before_it_is_opened_to_read() {
    use std::os::unix::fs::PermissionsExt;
    // A file nobody may read: opening it fails. One that is over the bound is refused
    // for its length all the same, so its length was looked at first, and one that is
    // not is refused as unreadable. (A process that may read anything proves nothing
    // here, and is told apart by the second file.)
    let scratch = Scratch::new("config-unopened");
    let limit = usize::try_from(CONFIG_BYTES_LIMIT).unwrap();
    let over = scratch.path().join("over.toml");
    std::fs::write(&over, vec![b'#'; limit + 1]).unwrap();
    std::fs::set_permissions(&over, std::fs::Permissions::from_mode(0o000)).unwrap();
    let small = scratch.path().join("small.toml");
    std::fs::write(&small, "# nothing\n").unwrap();
    std::fs::set_permissions(&small, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(
        Config::read(&over, &Secrets::none()).unwrap_err(),
        ConfigRefused::TooLarge {
            limit: CONFIG_BYTES_LIMIT
        }
    );
    match Config::read(&small, &Secrets::none()) {
        Err(ConfigRefused::Unreadable(std::io::ErrorKind::PermissionDenied)) => {}
        // Read by a process that file permissions do not hold.
        Ok(config) => assert_eq!(config, Config::empty()),
        Err(other) => panic!("{other:?}"),
    }
}

#[test]
fn no_more_than_the_bound_and_one_byte_is_asked_of_what_is_read() {
    // What an opened file is read through, given something that never ends and counts
    // what was asked of it: a file that grew after its length was looked at.
    struct Endless(usize);
    impl std::io::Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            buffer.fill(b'#');
            self.0 += buffer.len();
            Ok(buffer.len())
        }
    }
    let mut endless = Endless(0);
    assert_eq!(
        Config::read_from(&mut endless, &Secrets::none()).unwrap_err(),
        ConfigRefused::TooLarge {
            limit: CONFIG_BYTES_LIMIT
        }
    );
    assert_eq!(endless.0, usize::try_from(CONFIG_BYTES_LIMIT).unwrap() + 1);
    // What fails while it is read is unreadable, with the kind of failure.
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("zqxjk"))
        }
    }
    let refused = Config::read_from(Broken, &Secrets::none()).unwrap_err();
    assert_eq!(
        refused,
        ConfigRefused::Unreadable(std::io::ErrorKind::Other)
    );
    assert!(!said(&refused).contains("zqxjk"));
    // And what is short is read.
    assert_eq!(
        Config::read_from(&b"[spend]\ncall_cap_usd = 1\n"[..], &Secrets::none())
            .unwrap()
            .call_cap_micros(),
        Some(1_000_000)
    );
}

#[test]
fn what_is_not_a_file_is_refused_before_it_is_opened() {
    // A folder, and on a system that has it the device of endless zeros, which says
    // its length is nothing and would never end: what a path names is asked first,
    // and only a file is opened.
    let scratch = Scratch::new("config-not-a-file");
    let mut paths = vec![scratch.path().to_path_buf()];
    if cfg!(unix) {
        paths.push(std::path::PathBuf::from("/dev/zero"));
    }
    for path in paths {
        let refused = Config::read(&path, &Secrets::none()).unwrap_err();
        assert_eq!(refused, ConfigRefused::NotAFile);
        assert_eq!(
            refused.to_string(),
            "the config file cannot be read: it is not a file"
        );
    }
}

#[test]
fn a_refusal_shows_no_path() {
    let scratch = Scratch::new("config-no-path");
    let folder = scratch.path().file_name().unwrap().to_str().unwrap();
    let none = Secrets::none();
    // A file that is not there, a folder, a file that does not parse and one with an
    // unknown key.
    let missing = scratch.path().join("missing-zqxjk.toml");
    let absent = Config::read(&missing, &none).unwrap_err();
    assert_eq!(
        absent,
        ConfigRefused::Unreadable(std::io::ErrorKind::NotFound)
    );
    assert_eq!(
        absent.to_string(),
        "the config file cannot be read: it is not there"
    );
    let broken = scratch.path().join("broken-zqxjk.toml");
    std::fs::write(&broken, "[spend\n").unwrap();
    let unknown = scratch.path().join("unknown-zqxjk.toml");
    std::fs::write(&unknown, "colour = 1\n").unwrap();
    for refused in [
        absent,
        Config::read(scratch.path(), &none).unwrap_err(),
        Config::read(&broken, &none).unwrap_err(),
        Config::read(&unknown, &none).unwrap_err(),
    ] {
        let all = said(&refused);
        assert!(!all.contains(folder) && !all.contains("zqxjk"), "{all}");
        assert!(!all.contains('/') && !all.contains(".toml"), "{all}");
    }
}

#[test]
fn a_value_where_a_variables_name_belongs_is_refused_without_being_shown() {
    // What is written where a variable's name belongs may be the key itself. It is
    // never shown: not when it has the shape of a key, and not when it is only wrong.
    let key = made_up_key("");
    for (which, value) in [
        ("key_env", key.as_str()),
        ("token_env", key.as_str()),
        ("key_env", "lower_case"),
        ("key_env", "9STARTS_WITH_A_DIGIT"),
        ("key_env", "HAS SPACE"),
        ("key_env", "${FAL_KEY}"),
        ("key_env", "$FAL_KEY"),
        ("key_env", ""),
        ("token_env", "Bearer abcdefghijklmnop"),
        ("key_env", &"K".repeat(65)),
    ] {
        let text = format!("[backends.fill]\nkind = \"a\"\n{which} = \"{value}\"\n");
        let refused = refusal(&text);
        assert_eq!(key_of(&refused), format!("backends.fill.{which}"));
        let all = said(&refused);
        assert!(all.contains("is not shown"), "{all}");
        if value.len() >= 4 {
            assert!(!all.contains(value), "{all}");
            assert!(!all.contains(&value[..4]), "{all}");
        }
    }
    // Not a text at all: the same sentence, and nothing of the value.
    let number = refusal("[backends.fill]\nkind = \"a\"\nkey_env = 123456789\n");
    assert!(!said(&number).contains("1234"), "{number}");
    // A variable's name is read: capital letters, digits and `_`, 64 at most.
    let name = format!("K_{}", "9".repeat(62));
    let config = read(&format!(
        "[backends.fill]\nkind = \"a\"\nkey_env = \"{name}\"\ntoken_env = \"_T\"\n"
    ));
    let (_, backend) = config.backends(Capability::Fill).next().unwrap();
    assert_eq!(
        backend.credentials().collect::<Vec<_>>(),
        [("key_env", name.as_str()), ("token_env", "_T")]
    );
}

#[test]
fn a_name_or_a_key_that_holds_a_secret_is_refused_whole_and_not_shown() {
    // The secret is one the reader was told of, as the values of an env file are
    // (task C2). It is lower-case, so that it could stand in a name.
    let secret = format!("zq{}xjkvwpyh0123456789", "-");
    let secrets = Secrets::new([secret.clone()]).unwrap();
    // Where the cut of a refusal would fall inside it: 60 characters, then the secret.
    let across = format!("{}{secret}", "k".repeat(60));
    // Written with an escape, so that the file's bytes do not hold it and the key does.
    let escaped = format!("\"zq\\u002d{}\"", &secret[3..]);
    for text in [
        // A backend's name, read and then unknown or wrongly formed.
        format!("[backends.fill.{secret}]\nkind = \"a\"\n"),
        format!("[backends.fill.{across}]\nkind = \"a\"\n"),
        format!("[backends.fill.{escaped}]\nkind = \"a\"\n"),
        // A setting's key.
        format!("[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\n{secret} = 1\n"),
        format!("[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\n{across} = 1\n"),
        format!("[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\n{escaped} = 1\n"),
        // A recipient's name, an unknown key, and a key given twice.
        format!("[egress.recipients.{secret}]\norigin = \"https://h\"\nclasses = []\n"),
        format!("{across} = 1\n"),
        format!("{escaped} = 1\n"),
        format!("{across} = 1\n{across} = 2\n"),
        format!("{escaped} = 1\n{escaped} = 2\n"),
        // A value, whatever its key and whether or not it would have been read.
        format!("[backends.fill]\nkind = \"a\"\nmodel = \"{secret}\"\n"),
        format!(
            "[backends.fill]\nkind = \"a\"\nmodel = \"zq\\u002d{}\"\n",
            &secret[3..]
        ),
        format!("[backends.fill]\nkind = \"a\"\nurl = \"https://h/{secret}\"\n"),
        format!("[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nnote = \"x {secret}\"\n"),
        format!("unknown = [[\"{secret}\"]]\n"),
        // A comment, and text that is not TOML at all.
        format!("# {secret}\n[spend]\ncall_cap_usd = 1\n"),
        format!("[spend\n{secret}\n"),
    ] {
        let refused = Config::parse(text.as_bytes(), &secrets).unwrap_err();
        assert_eq!(refused, ConfigRefused::HoldsSecret, "{text}");
        let all = said(&refused);
        for start in 0..secret.len() - 4 {
            assert!(!all.contains(&secret[start..start + 4]), "{all}");
        }
        assert!(!all.contains("kkkk"), "{all}");
    }
    // A secret that stands across two keys, or across a key and one of the reader's
    // own words: no key alone holds it, and the path a refusal would show does. The
    // path is looked at whole, as it would be shown, before any refusal is made.
    for (secret, text) in [
        (
            "abc12345.XYZsecret99",
            "[backends.fill.abc12345]\nkind = \"k\"\nXYZsecret99 = 1\n",
        ),
        (
            "abc12345.XYZsecret99",
            "[egress.recipients.abc12345]\norigin = \"https://h\"\nclasses = []\nXYZsecret99 = 1\n",
        ),
        (
            "fill.zqxjkvwpyh",
            "[backends.\"fill\".zqxjkvwpyh]\nkind = 7\n",
        ),
        (
            "zqxjkvwpyh.kind",
            "[backends.fill.zqxjkvwpyh]\nmodel = \"m\"\n",
        ),
        (
            "recipients.zqxjkvwpyh.origin",
            "[egress.recipients.zqxjkvwpyh]\nclasses = []\n",
        ),
        (
            "abc12345.settings.xyzsecret99",
            "[backends.fill.abc12345]\nkind = \"k\"\n[backends.fill.abc12345.settings]\nxyzsecret99 = 1.5\n",
        ),
        // Where the key is cut when it is shown, and the secret reaches past the cut:
        // the look is at the path as it was read, not as it is shown.
        (
            "recipients.kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
            "[egress.recipients.\"kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk\"]\norigin = \"https://h\"\nclasses = []\n",
        ),
    ] {
        let secrets = Secrets::new([secret.to_owned()]).unwrap();
        let refused = Config::parse(text.as_bytes(), &secrets).unwrap_err();
        assert_eq!(refused, ConfigRefused::HoldsSecret, "{text}");
        // The same file with no secret known is refused for what it is, by a key.
        let untold = Config::parse(text.as_bytes(), &Secrets::none()).unwrap_err();
        assert!(
            matches!(
                untold,
                ConfigRefused::Key { .. } | ConfigRefused::Twice { .. }
            ),
            "{untold:?}"
        );
    }
    // A secret written with an escape inside a list, where only the decoded text
    // holds it.
    for text in [
        format!("unknown = [\"zq\\u002d{}\"]\n", &secret[3..]),
        format!(
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = [[\"zq\\u002d{}\"]]\n",
            &secret[3..]
        ),
    ] {
        assert_eq!(
            Config::parse(text.as_bytes(), &secrets).unwrap_err(),
            ConfigRefused::HoldsSecret,
            "{text}"
        );
    }
    assert_eq!(
        ConfigRefused::HoldsSecret.to_string(),
        "the config file holds a secret: a credential is named by its variable and is \
         never written in the file; nothing of the file is shown"
    );
    // The same texts with no secret known are refused, or read, for what they are.
    let config = Config::parse(
        format!("[backends.fill.{secret}]\nkind = \"a\"\n").as_bytes(),
        &Secrets::none(),
    )
    .unwrap();
    assert_eq!(config.backends(Capability::Fill).count(), 1);
}

#[test]
fn the_cap_of_one_call_is_an_amount_in_dollars_read_to_millionths() {
    for (written, micros) in [
        ("2", 2_000_000),
        ("2.5", 2_500_000),
        ("2.50", 2_500_000),
        ("25e-1", 2_500_000),
        ("0.000001", 1),
        ("0.1", 100_000),
        ("0.29", 290_000),
        ("1.1", 1_100_000),
        ("4.35", 4_350_000),
        ("0.999999", 999_999),
        // Two whose product with a million is not a whole number as doubles multiply:
        // one falls just under, one just over, and each is rounded to the nearest.
        ("0.000498", 498),
        ("0.000246", 246),
        ("1_000", 1_000_000_000),
        ("1000000", 1_000_000_000_000),
        ("1000000.0", 1_000_000_000_000),
        ("0x10", 16_000_000),
    ] {
        let config = read(&format!("[spend]\ncall_cap_usd = {written}\n"));
        assert_eq!(config.call_cap_micros(), Some(micros), "{written}");
    }
    // Zero is no cap, and neither is less; an amount finer than a millionth is not
    // rounded; over a million dollars is not a cap anyone meant.
    for written in [
        "0",
        "0.0",
        "-0.0",
        "-1",
        "-0.5",
        "0.0000001",
        "0.0000015",
        "1.2345678",
        "1.00000001",
        "1.0000001",
        "1000001",
        "1000000.000001",
        "1e300",
        "inf",
        "-inf",
        "nan",
    ] {
        let refused = refusal(&format!("[spend]\ncall_cap_usd = {written}\n"));
        assert_eq!(key_of(&refused), "spend.call_cap_usd", "{written}");
        let said = refused.to_string();
        assert!(said.contains("more than 0"), "{said}");
        assert!(said.contains("six decimal places"), "{said}");
    }
    assert_eq!(read("[spend]\n").call_cap_micros(), None);
}

#[test]
fn a_recipient_is_an_origin_with_the_classes_of_data_it_may_receive() {
    let config = read(
        r#"
[egress.recipients.fal]
origin = "https://fal.example"
account = "team-1234"
upstreams = ["https://storage.fal.example", "https://cdn.fal.example:8443"]
classes = ["prompts", "source_images", "derived_images"]
probe = true

[egress.recipients.judge-host]
origin = "http://127.0.0.1:8080"
classes = []
"#,
    );
    let recipients: Vec<_> = config.recipients().collect();
    assert_eq!(
        recipients.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["fal", "judge-host"]
    );
    let fal = recipients[0].1;
    assert_eq!(fal.origin(), "https://fal.example");
    assert_eq!(fal.account(), Some("team-1234"));
    assert_eq!(
        fal.upstreams().collect::<Vec<_>>(),
        [
            "https://cdn.fal.example:8443",
            "https://storage.fal.example"
        ]
    );
    assert_eq!(
        fal.classes().collect::<Vec<_>>(),
        [
            DataClass::DerivedImages,
            DataClass::Prompts,
            DataClass::SourceImages
        ]
    );
    assert!(fal.may_be_probed());
    // Approved for nothing, and not to be probed, unless the file says so.
    let host = recipients[1].1;
    assert_eq!(host.origin(), "http://127.0.0.1:8080");
    assert_eq!(host.account(), None);
    assert_eq!(host.upstreams().count(), 0);
    assert_eq!(host.classes().count(), 0);
    assert!(!host.may_be_probed());
    // The four classes, by the names the file gives them.
    assert_eq!(
        [
            DataClass::DerivedImages,
            DataClass::Evidence,
            DataClass::Prompts,
            DataClass::SourceImages
        ]
        .map(DataClass::name),
        ["derived_images", "evidence", "prompts", "source_images"]
    );
    // An origin and the classes are not left to a default; a class or an upstream
    // given twice is refused, not folded.
    for (text, key) in [
        (
            "[egress.recipients.one]\nclasses = []\n",
            "egress.recipients.one.origin",
        ),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\n",
            "egress.recipients.one.classes",
        ),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = [\"prompts\", \"prompts\"]\n",
            "egress.recipients.one.classes",
        ),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\nupstreams = [\"https://u\", \"https://u\"]\n",
            "egress.recipients.one.upstreams",
        ),
        (
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\nupstreams = [\"https://h\"]\n",
            "egress.recipients.one.upstreams",
        ),
        (
            "[egress.recipients.Fal]\norigin = \"https://h\"\nclasses = []\n",
            "egress.recipients.Fal",
        ),
    ] {
        assert_eq!(key_of(&refusal(text)), key, "{text}");
    }
}

#[test]
fn an_address_that_could_hold_a_credential_is_refused_and_not_shown() {
    let marker = "zqxjkvalue";
    // An origin is a scheme, a host and perhaps a port, and nothing else.
    for origin in [
        format!("https://user:{marker}@h.example"),
        format!("https://{marker}@h.example"),
        format!("https://h.example/?key={marker}"),
        format!("https://h.example/{marker}"),
        "https://h.example/".to_owned(),
        format!("https://h.example#{marker}"),
        format!("ftp://{marker}.example"),
        format!("{marker}.example"),
        format!("https://{marker}.Example"),
        "https://".to_owned(),
        "https://h.example:".to_owned(),
        "https://h.example:0".to_owned(),
        "https://h.example:65536".to_owned(),
        "https://h.example:080".to_owned(),
        "https://h..example".to_owned(),
        "https://-h.example".to_owned(),
        "https://h.example.".to_owned(),
        format!("https://{}", "h".repeat(254)),
        "https://[::1]:8080".to_owned(),
    ] {
        let text = format!("[egress.recipients.one]\norigin = \"{origin}\"\nclasses = []\n");
        let refused = refusal(&text);
        assert_eq!(key_of(&refused), "egress.recipients.one.origin", "{origin}");
        assert!(!said(&refused).contains(marker), "{refused}");
        let text = format!(
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\nupstreams = [\"{origin}\"]\n"
        );
        let upstream = refusal(&text);
        assert_eq!(key_of(&upstream), "egress.recipients.one.upstreams");
    }
    // A backend's address is an origin and perhaps a path: no user, no password, no
    // query and no fragment, each of which is where a credential is written.
    for url in [
        format!("https://user:{marker}@h.example/v1"),
        format!("https://h.example/v1?token={marker}"),
        format!("https://h.example/v1#{marker}"),
        format!("https://h.example/{marker} v1"),
        format!("https://h.example/v1\\\\{marker}"),
        marker.to_owned(),
        format!("https://h.example/{}", "p".repeat(2048)),
    ] {
        let text = format!("[backends.fill]\nkind = \"a\"\nurl = \"{url}\"\n");
        let refused = refusal(&text);
        assert_eq!(key_of(&refused), "backends.fill.url", "{url}");
        assert!(!said(&refused).contains(marker), "{refused}");
    }
    for url in [
        "https://h.example",
        "https://h.example/",
        "http://127.0.0.1:8188/v1/fill",
        "https://h.example:8443/a/b-c_d.e~f/%20",
    ] {
        let config = read(&format!("[backends.fill]\nkind = \"a\"\nurl = \"{url}\"\n"));
        let (_, backend) = config.backends(Capability::Fill).next().unwrap();
        assert_eq!(backend.url(), Some(url));
    }
}

#[test]
fn more_than_the_bound_of_a_list_is_refused() {
    assert_eq!(
        (
            BACKENDS_LIMIT,
            SETTINGS_LIMIT,
            RECIPIENTS_LIMIT,
            UPSTREAMS_LIMIT
        ),
        (16, 32, 32, 16)
    );
    let backends = |count: usize| {
        (0..count)
            .map(|n| format!("[backends.fill.b{n}]\nkind = \"a\"\n"))
            .collect::<String>()
    };
    assert_eq!(read(&backends(16)).backends(Capability::Fill).count(), 16);
    assert_eq!(key_of(&refusal(&backends(17))), "backends.fill");
    let settings = |count: usize| {
        let keys: String = (0..count).map(|n| format!("s{n} = {n}\n")).collect();
        format!("[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\n{keys}")
    };
    read(&settings(32));
    assert_eq!(key_of(&refusal(&settings(33))), "backends.fill.settings");
    let recipients = |count: usize| {
        (0..count)
            .map(|n| format!("[egress.recipients.r{n}]\norigin = \"https://h{n}\"\nclasses = []\n"))
            .collect::<String>()
    };
    assert_eq!(read(&recipients(32)).recipients().count(), 32);
    assert_eq!(key_of(&refusal(&recipients(33))), "egress.recipients");
    let upstreams = |count: usize| {
        let list: Vec<String> = (0..count).map(|n| format!("\"https://u{n}\"")).collect();
        format!(
            "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\nupstreams = [{}]\n",
            list.join(", ")
        )
    };
    read(&upstreams(16));
    assert_eq!(
        key_of(&refusal(&upstreams(17))),
        "egress.recipients.one.upstreams"
    );
}

#[test]
fn a_config_prints_its_shape_and_none_of_its_values() {
    let marker = "zqxjkvalue";
    let config = read(&format!(
        "[backends.fill.{marker}]\nkind = \"{marker}\"\nmodel = \"{marker}\"\n\
         url = \"https://{marker}\"\nkey_env = \"ZQXJKVALUE\"\n\
         [backends.fill.{marker}.settings]\n{marker} = \"{marker}\"\n\
         [spend]\ncall_cap_usd = 7\n\
         [egress.recipients.{marker}]\norigin = \"https://{marker}\"\naccount = \"{marker}\"\n\
         classes = [\"prompts\"]\n"
    ));
    let shown = format!("{config:?}");
    assert_eq!(
        shown,
        "Config { backends: 1, call_cap: true, recipients: 1 }"
    );
    let (_, backend) = config.backends(Capability::Fill).next().unwrap();
    let (_, recipient) = config.recipients().next().unwrap();
    let parts = format!(
        "{backend:?} {recipient:?} {:?}",
        backend.settings().next().unwrap().1
    );
    assert_eq!(
        parts,
        "Backend { model: true, provider: false, url: true, credentials: 1, settings: 1 } \
         Recipient { account: true, upstreams: 0, classes: 1, probe: false } Setting(text)"
    );
    for text in [shown, parts] {
        assert!(!text.to_lowercase().contains(marker), "{text}");
    }
    assert_eq!(
        format!("{:?}", Config::empty()),
        "Config { backends: 0, call_cap: false, recipients: 0 }"
    );
}

#[test]
fn what_the_reader_does_not_find_or_tell_apart_is_known() {
    // ADR 0011, "What the reader does not do". Each of these is so, and is pinned so
    // that the list in the record stays true.
    let none = Secrets::none();
    // A key the reader was not told of is not known by its shape: written into a
    // setting's text, or as a model, it is read. (Where a variable's name belongs it is
    // refused, because it is no variable's name.)
    let key = made_up_key("");
    let text = format!(
        "[backends.fill]\nkind = \"a\"\nmodel = \"{key}\"\n[backends.fill.settings]\nnote = \"{key}\"\n"
    );
    let config = Config::parse(text.as_bytes(), &none).unwrap();
    let (_, backend) = config.backends(Capability::Fill).next().unwrap();
    assert_eq!(backend.model(), Some(key.as_str()));
    // A key made of capital letters and digits alone is a variable's name to the
    // reader, and is read as one.
    let capitals = format!("ZQXJ{}", "ABCDEFGHIJ012345");
    let text = format!("[backends.fill]\nkind = \"a\"\nkey_env = \"{capitals}\"\n");
    let config = Config::parse(text.as_bytes(), &none).unwrap();
    assert!(config.variables().contains(capitals.as_str()));
    // Two origins that a client would reach as one are two texts, and two configs.
    let origin = |origin: &str| {
        let text = format!("[egress.recipients.one]\norigin = \"{origin}\"\nclasses = []\n");
        Config::parse(text.as_bytes(), &none).unwrap()
    };
    assert_ne!(origin("https://h.example"), origin("https://h.example:443"));
    // A key that is not known is shown, its first 64 characters, whatever it looks
    // like: a key the reader was not told of, pasted as a KEY of the file, is said back.
    let pasted = Config::parse(format!("\"{key}\" = 1\n").as_bytes(), &none).unwrap_err();
    assert_eq!(key_of(&pasted), key);
    // The reader looks for a secret in the bytes, in the keys and in the texts. A
    // number is none of these: a secret of digits, written as a number with a mark
    // between its digits, is read as that number.
    let digits = Secrets::new(["12345678".to_owned()]).unwrap();
    let text = "[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\nside = 1234_5678\n";
    let config = Config::parse(text.as_bytes(), &digits).unwrap();
    let (_, backend) = config.backends(Capability::Fill).next().unwrap();
    assert_eq!(
        backend.settings().next(),
        Some(("side", &Setting::Integer(12_345_678)))
    );
    // An amount is a double before it is millionths: digits past what a double holds
    // are not seen, and are not refused as finer than a millionth.
    let cap = "[spend]\ncall_cap_usd = 2.5000000000000000001\n";
    assert_eq!(
        Config::parse(cap.as_bytes(), &none)
            .unwrap()
            .call_cap_micros(),
        Some(2_500_000)
    );
    // A key given twice is named without its table. A secret that stands across the
    // table's name and the key is not whole in what is shown, and its last part is
    // shown: the format's reader does not say which table the key is in.
    let across = Secrets::new(["abc12345.XYZsecret99".to_owned()]).unwrap();
    let text = "[backends.fill.abc12345]\nkind = \"k\"\nXYZsecret99 = 1\nXYZsecret99 = 2\n";
    assert_eq!(
        Config::parse(text.as_bytes(), &across).unwrap_err(),
        ConfigRefused::Twice {
            key: "XYZsecret99".to_owned(),
            line: 4
        }
    );
    // A secret the reader was told of, written in a form redaction does not search
    // for, is not found: here, with its letters in the other case.
    let secret = "zqxjkvwpyh0123456789".to_owned();
    let secrets = Secrets::new([secret.clone()]).unwrap();
    let text = format!("# {}\n", secret.to_uppercase());
    assert_eq!(
        Config::parse(text.as_bytes(), &secrets).unwrap(),
        Config::empty()
    );
}
