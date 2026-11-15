//! The digest of a config (ADR 0011, "The digest"; plan 1B, task C1): of what the file
//! says, and not of its bytes. The three fixed digests were worked out from the words of
//! the record alone, with a few lines of a scripting language's standard library, and
//! not from this code.

use std::collections::BTreeMap;

use whizbang_core::config::Config;
use whizbang_core::contract::Secrets;
use whizbang_core::identity::Digest;

use super::the_specs_example;

fn digest(text: &str) -> Digest {
    match Config::parse(text.as_bytes(), &Secrets::none()) {
        Ok(config) => config.digest(),
        Err(refused) => panic!("refused: {refused}\n{text}"),
    }
}

/// A config with something in every place a setting can be.
const FULL: &str = r#"
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
offset = -3
rendering = "grey"
tiled = true

[backends.judge.claude]
kind = "service"
url = "https://judge.example/v1"
key_env = "JUDGE_KEY"
token_env = "JUDGE_TOKEN"

[spend]
call_cap_usd = 2.5

[egress.recipients.fal]
origin = "https://fal.example"
account = "team-1234"
upstreams = ["https://storage.fal.example", "https://cdn.fal.example:8443"]
classes = ["prompts", "source_images", "derived_images"]
probe = true

[egress.recipients.judge-host]
origin = "http://127.0.0.1:8080"
classes = []
"#;

#[test]
fn the_digests_of_three_fixed_configs_are_the_records() {
    for (text, expected) in [
        (
            "",
            "sha256:9d60a543ea39be1ca1bc2c4e1a90094a59ded6bdcfdf1c21436f4026f207c4a6",
        ),
        (
            the_specs_example(),
            "sha256:e0f7ca76571f20c077084f4dcb18a57c0adf857b37d59928546508937fa31c7f",
        ),
        (
            FULL,
            "sha256:c151bf69e75bfec361a2018365535e49731847d0dbf6d13f4bb71b06356d6399",
        ),
    ] {
        assert_eq!(digest(text).to_string(), expected, "{text}");
    }
    assert_eq!(Config::empty().digest(), digest(""));
}

#[test]
fn two_files_that_differ_in_comments_spacing_and_order_have_one_digest() {
    // The same settings: the tables in another order, the keys of each in another
    // order, other spacing, comments, and line breaks of the other kind.
    let again = "# A config.\r\n\
        [egress.recipients.judge-host]\r\n\
        classes=[]\r\n\
        origin   =   \"http://127.0.0.1:8080\"   # this machine\r\n\
        \r\n\
        [spend]\r\n\
        \tcall_cap_usd = 2.5\r\n\
        [backends.judge.claude]\r\n\
        token_env = \"JUDGE_TOKEN\"\r\n\
        key_env = \"JUDGE_KEY\"\r\n\
        url = \"https://judge.example/v1\"\r\n\
        kind = \"service\"\r\n\
        [egress.recipients.fal]\r\n\
        probe = true\r\n\
        classes = [\r\n\
            \"derived_images\", # what a fill is of\r\n\
            \"source_images\",\r\n\
            \"prompts\",\r\n\
        ]\r\n\
        upstreams = [\"https://cdn.fal.example:8443\", \"https://storage.fal.example\"]\r\n\
        account = \"team-1234\"\r\n\
        origin = \"https://fal.example\"\r\n\
        [backends.fill.local.settings]\r\n\
        tiled = true\r\n\
        rendering = \"grey\"\r\n\
        offset = -3\r\n\
        longest_side = 2048\r\n\
        [backends.fill.local]\r\n\
        model = \"lama\"\r\n\
        kind = \"pack\"\r\n\
        [backends.fill.bria]\r\n\
        key_env = \"FAL_KEY\"\r\n\
        model = \"bria-erase\"\r\n\
        provider = \"fal\"\r\n\
        kind = \"service\"\r\n\
        [backends.fill]\r\n\
        default = \"bria\"\r\n";
    assert_eq!(digest(again), digest(FULL));
    // And they are equal as configs.
    let none = Secrets::none();
    assert_eq!(
        Config::parse(again.as_bytes(), &none).unwrap(),
        Config::parse(FULL.as_bytes(), &none).unwrap()
    );
}

#[test]
fn the_ways_toml_writes_one_value_have_one_digest() {
    // The digest is of what was read, so every way the format has of writing one
    // value is one config: the two kinds of string, an escape, an inline table, dotted
    // keys, a quoted key, and the ways of writing a number.
    let plain = "[backends.fill]\nkind = \"service\"\nmodel = \"bria-erase\"\n\
        [backends.fill.settings]\nside = 2048\n[spend]\ncall_cap_usd = 2.5\n";
    for other in [
        "[backends.fill]\nkind = 'service'\nmodel = '''bria-erase'''\n\
         [backends.fill.settings]\nside = 2_048\n[spend]\ncall_cap_usd = 2.50\n",
        "backends = { fill = { kind = \"service\", model = \"bria\\u002derase\", settings = { side = 0x800 } } }\n\
         spend = { call_cap_usd = 25e-1 }\n",
        "backends.fill.kind = \"service\"\nbackends.fill.model = \"bria-erase\"\n\
         backends.fill.settings.side = 0o4000\nspend.call_cap_usd = 2.5\n",
        "[\"backends\".'fill']\n\"kind\" = \"service\"\n\"mo\\u0064el\" = \"bria-erase\"\n\
         [backends.fill.settings]\nside = +2048\n[spend]\ncall_cap_usd = +2.5\n",
    ] {
        assert_eq!(digest(other), digest(plain), "{other}");
    }
    // A whole number of dollars and the same amount with a fraction are one cap.
    assert_eq!(
        digest("[spend]\ncall_cap_usd = 2\n"),
        digest("[spend]\ncall_cap_usd = 2.0\n")
    );
}

#[test]
fn a_table_with_nothing_in_it_is_as_no_table() {
    let empty = digest("");
    for text in [
        "# nothing\n",
        "[backends]\n",
        "[backends.fill]\n",
        "[backends]\n[backends.fill]\n[backends.judge]\n[backends.segment]\n",
        "[spend]\n",
        "[egress]\n",
        "[egress.recipients]\n",
        "[models]\n",
        "[backends]\n[spend]\n[egress]\n[models]\n",
    ] {
        assert_eq!(digest(text), empty, "{text}");
    }
    // A backend's empty table of settings is as none.
    assert_eq!(
        digest("[backends.fill]\nkind = \"a\"\n[backends.fill.settings]\n"),
        digest("[backends.fill]\nkind = \"a\"\n")
    );
    // And saying what is so unless said otherwise is as not saying it.
    let recipient = "[egress.recipients.one]\norigin = \"https://h\"\nclasses = []\n";
    assert_eq!(
        digest(&format!("{recipient}probe = false\nupstreams = []\n")),
        digest(recipient)
    );
}

#[test]
fn a_change_to_any_setting_changes_the_digest() {
    // Each line is the full config with one thing changed: a value, a name, a thing
    // added or a thing taken away. Every one has a digest of its own.
    let changes: &[(&str, &str)] = &[
        ("default = \"bria\"", "default = \"local\""),
        ("default = \"bria\"\n", ""),
        ("[backends.judge.claude]", "[backends.judge.claudf]"),
        (
            "default = \"bria\"\n\n[backends.fill.bria]",
            "[backends.segment.bria]",
        ),
        (
            "kind = \"service\"\nprovider",
            "kind = \"services\"\nprovider",
        ),
        ("provider = \"fal\"", "provider = \"fal2\""),
        ("provider = \"fal\"\n", ""),
        ("model = \"bria-erase\"", "model = \"bria-erase \\u0032\""),
        ("model = \"bria-erase\"\n", ""),
        ("key_env = \"FAL_KEY\"", "key_env = \"FAL_KEY2\""),
        ("key_env = \"FAL_KEY\"", "token_env = \"FAL_KEY\""),
        ("key_env = \"FAL_KEY\"\n", ""),
        (
            "url = \"https://judge.example/v1\"",
            "model = \"https://judge.example/v1\"",
        ),
        ("model = \"lama\"", "provider = \"lama\""),
        (
            "kind = \"pack\"\nmodel = \"lama\"",
            "kind = \"lama\"\nmodel = \"pack\"",
        ),
        ("longest_side = 2048", "longest_side = 2049"),
        ("longest_side = 2048", "longest_side = \"2048\""),
        ("longest_side = 2048", "longest_sides = 2048"),
        ("longest_side = 2048\n", ""),
        ("offset = -3", "offset = 3"),
        ("rendering = \"grey\"", "rendering = \"gray\""),
        ("tiled = true", "tiled = false"),
        ("tiled = true", "tiled = 1"),
        ("tiled = true", "tiled = \"true\""),
        (
            "[backends.fill.local.settings]",
            "[backends.fill.bria.settings]",
        ),
        ("[backends.judge.claude]", "[backends.segment.claude]"),
        (
            "[backends.judge.claude]",
            "[backends.judge]\ndefault = \"claude\"\n[backends.judge.claude]",
        ),
        (
            "url = \"https://judge.example/v1\"",
            "url = \"https://judge.example/v2\"",
        ),
        ("url = \"https://judge.example/v1\"\n", ""),
        (
            "token_env = \"JUDGE_TOKEN\"",
            "token_env = \"JUDGE_TOKEN_2\"",
        ),
        ("token_env = \"JUDGE_TOKEN\"\n", ""),
        (
            "key_env = \"JUDGE_KEY\"\ntoken_env = \"JUDGE_TOKEN\"",
            "key_env = \"JUDGE_TOKEN\"\ntoken_env = \"JUDGE_KEY\"",
        ),
        ("call_cap_usd = 2.5", "call_cap_usd = 2.500001"),
        ("call_cap_usd = 2.5", "call_cap_usd = 1"),
        ("call_cap_usd = 2.5\n", ""),
        ("[egress.recipients.fal]", "[egress.recipients.fal2]"),
        (
            "origin = \"https://fal.example\"",
            "origin = \"http://fal.example\"",
        ),
        (
            "origin = \"https://fal.example\"",
            "origin = \"https://fal.example:444\"",
        ),
        ("account = \"team-1234\"", "account = \"team-1235\""),
        ("account = \"team-1234\"\n", ""),
        ("\"https://storage.fal.example\", ", ""),
        ("https://cdn.fal.example:8443", "https://cdn.fal.example"),
        (
            "upstreams = [\"https://storage.fal.example\", \"https://cdn.fal.example:8443\"]\n",
            "",
        ),
        ("\"prompts\", ", ""),
        ("\"prompts\", ", "\"prompts\", \"evidence\", "),
        ("\"source_images\"", "\"evidence\""),
        ("probe = true", "probe = false"),
        (
            "origin = \"http://127.0.0.1:8080\"",
            "origin = \"http://127.0.0.1:8081\"",
        ),
        ("classes = []\n", "classes = [\"evidence\"]\n"),
        ("classes = []\n", "classes = []\nprobe = true\n"),
        ("classes = []\n", "classes = []\naccount = \"team-1234\"\n"),
        (
            "classes = []\n",
            "classes = []\nupstreams = [\"https://fal.example\"]\n",
        ),
        (
            "[egress.recipients.judge-host]\norigin = \"http://127.0.0.1:8080\"\nclasses = []\n",
            "",
        ),
    ];
    let mut seen: BTreeMap<Digest, String> = BTreeMap::new();
    seen.insert(digest(FULL), "the full config".to_owned());
    for (from, to) in changes {
        assert_eq!(
            FULL.matches(from).count(),
            1,
            "{from} is in the config once"
        );
        let changed = FULL.replacen(from, to, 1);
        let what = format!("{from:?} -> {to:?}");
        if let Some(other) = seen.insert(digest(&changed), what.clone()) {
            panic!("one digest for two configs: {what}, and {other}");
        }
    }
    assert_eq!(seen.len(), changes.len() + 1);
}

#[test]
fn where_one_text_ends_and_the_next_begins_is_in_the_digest() {
    // Two configs whose texts run together to the same letters.
    assert_ne!(
        digest("[backends.fill.ab]\nkind = \"c\"\n"),
        digest("[backends.fill.a]\nkind = \"bc\"\n")
    );
    assert_ne!(
        digest("[backends.fill.a]\nkind = \"b\"\nmodel = \"cd\"\n"),
        digest("[backends.fill.a]\nkind = \"b\"\nprovider = \"cd\"\n")
    );
    // A backend of one capability and the same backend of another.
    assert_ne!(
        digest("[backends.fill]\nkind = \"a\"\n"),
        digest("[backends.judge]\nkind = \"a\"\n")
    );
    // The short form is a backend named `default` that is the default, and that is all
    // it is: it is not the same config as a backend of another name.
    assert_ne!(
        digest("[backends.fill]\nkind = \"a\"\n"),
        digest("[backends.fill]\ndefault = \"one\"\n[backends.fill.one]\nkind = \"a\"\n")
    );
}

#[test]
fn the_ways_of_writing_one_origin_have_one_digest() {
    // An origin is hashed as an origin writes itself (ADR 0012): the scheme and the
    // host in lower case, and no port when it is the scheme's own. The same for a
    // backend's address, whose path is hashed as it was given.
    let recipient = |origin: &str, upstream: &str| {
        digest(&format!(
            "[egress.recipients.one]\norigin = \"{origin}\"\nclasses = []\nupstreams = [\"{upstream}\"]\n"
        ))
    };
    let plain = recipient("https://h.example", "http://u.example");
    for (origin, upstream) in [
        ("https://h.example:443", "http://u.example:80"),
        ("HTTPS://H.Example", "HTTP://U.EXAMPLE"),
        ("https://H.example:443", "http://u.example"),
    ] {
        assert_eq!(recipient(origin, upstream), plain, "{origin} {upstream}");
    }
    assert_ne!(
        recipient("https://h.example:8443", "http://u.example"),
        plain
    );
    assert_ne!(recipient("http://h.example", "http://u.example"), plain);
    let backend = |url: &str| digest(&format!("[backends.fill]\nkind = \"a\"\nurl = \"{url}\"\n"));
    assert_eq!(
        backend("HTTPS://H.Example:443/v1/Fill"),
        backend("https://h.example/v1/Fill")
    );
    // The path is not folded: its letters' case is the path's.
    assert_ne!(
        backend("https://h.example/v1/fill"),
        backend("https://h.example/v1/Fill")
    );
    assert_ne!(backend("https://h.example"), backend("https://h.example/"));
}

#[test]
fn a_variable_is_in_the_digest_by_its_name_and_a_value_by_nothing() {
    // A config holds the name of a variable and never its value, so the digest of a
    // config cannot depend on a key: there is none to read. The name is in it.
    let one = digest("[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\n");
    let other = digest("[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY_2\"\n");
    assert_ne!(one, other);
    // The secrets a reader is told of only refuse; they change nothing that is read.
    let secrets = Secrets::new(["zqxjkvwpyh0123456789".to_owned()]).unwrap();
    let text = "[backends.fill]\nkind = \"a\"\nkey_env = \"FAL_KEY\"\n";
    assert_eq!(
        Config::parse(text.as_bytes(), &secrets).unwrap().digest(),
        one
    );
}
