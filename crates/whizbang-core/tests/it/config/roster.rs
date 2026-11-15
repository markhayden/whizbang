//! The kinds of backend a build has, and what a config offers with them (ADR 0011,
//! "Kinds" and "The roster of a config"; plan 1B, the part of task C4 that needs no
//! snapshot). Everything here is asked of the offers alone: no executor is built. The
//! kinds are the test kit's fake, the keys are made up and put together at run time.

use std::sync::Arc;

use whizbang_core::capability::{Declaration, Roster, Unresolved, feature};
use whizbang_core::config::{
    Config, Credentials, EnvFile, KIND_SENTENCE_LIMIT, KIND_SENTENCE_WITHHELD, KINDS_LIMIT, Kind,
    KindNotRegistered, KindRefused, Kinds, NotReady, Offers, Table, Unanswered,
};
use whizbang_core::contract::{
    Backends, Capability, Estimate, Made, ModelIdentity, Offered, Secrets,
};
use whizbang_core::egress::{Address, NotAllowed, Outgoing, Recipient};
use whizbang_testkit::capabilities::{FakeFiller, Paint};
use whizbang_testkit::kinds::{
    FAKE_KEY_HEADER, FAKE_NO_ADDRESS, FAKE_SETTING_NOT_TEXT, FAKE_TOKEN_HEADER,
    FAKE_UNKNOWN_SETTING, FakeKind,
};

use super::made_up_key;

/// A config and the credentials of its backends, read in the order the two files are:
/// the env file first, and the config told of its values.
fn read(config: &str, env: &str) -> (Config, Credentials) {
    let env = EnvFile::parse(env.as_bytes()).unwrap();
    let config = Config::parse(config.as_bytes(), &env.secrets()).unwrap();
    let credentials = env.credentials(&config).unwrap();
    (config, credentials)
}

/// The kinds of a build that has these fakes, and the fakes.
fn kinds(names: &[&str]) -> (Kinds, Vec<Arc<FakeKind>>) {
    let mut kinds = Kinds::new();
    let fakes: Vec<Arc<FakeKind>> = names
        .iter()
        .map(|name| Arc::new(FakeKind::new(name)))
        .collect();
    for fake in &fakes {
        kinds.register(fake.clone()).unwrap();
    }
    (kinds, fakes)
}

fn declared(kind: &str, model: &str, features: &[&str]) -> Declaration {
    Declaration::new(ModelIdentity::new(kind, model, "1"), features).unwrap()
}

fn needs(features: &[&str]) -> Vec<String> {
    features.iter().map(|one| (*one).to_owned()).collect()
}

/// A credential as a kind carried it: the header's name, the origin it is bound to, and
/// its value.
type Carried = (String, String, String);

/// One backend a kind was shown: its capability, its name, and what it carried.
type Seen = (Capability, String, Vec<Carried>);

/// Everything a value says of itself.
fn said<T: std::fmt::Debug + std::fmt::Display>(value: &T) -> String {
    format!("{value:?} {value}")
}

const SEVERAL: &str = r#"
[backends.fill]
default = "bria"

[backends.fill.bria]
kind = "fake"
model = "bria-erase"

[backends.fill.bria.settings]
features = "prompt"

[backends.fill.lama]
kind = "fake"
model = "lama"

[backends.fill.zeta]
kind = "fake"
model = "zeta"

[backends.judge.one]
kind = "fake"
model = "judge-one"

[backends.judge.two]
kind = "fake"
model = "judge-two"

[backends.segment]
kind = "fake"
model = "sam"

[backends.segment.settings]
features = "phrase,box"
version = "3"
"#;

#[test]
fn a_kind_is_registered_once_by_a_name_and_a_build_has_no_more_than_its_bound() {
    let mut kinds = Kinds::new();
    assert!(kinds.is_empty());
    assert!(kinds.get("fake").is_none());
    kinds.register(Arc::new(FakeKind::new("fake"))).unwrap();
    assert_eq!(kinds.get("fake").map(|kind| kind.name()), Some("fake"));
    assert!(kinds.get("fak").is_none());
    // A second of the same name is refused, and the kinds are as they were.
    let twice = kinds.register(Arc::new(FakeKind::new("fake")));
    assert_eq!(
        twice,
        Err(KindNotRegistered::Duplicate {
            name: "fake".to_owned()
        })
    );
    assert_eq!(
        twice.unwrap_err().to_string(),
        "a kind named fake is registered already"
    );
    // What is no name is refused and not shown: a key could be pasted there.
    let key = made_up_key("");
    for name in ["", "Fake", "a kind", "1st", key.as_str(), &"k".repeat(65)] {
        let refused = kinds.register(Arc::new(FakeKind::new(name))).unwrap_err();
        assert_eq!(refused, KindNotRegistered::Name);
        assert!(!said(&refused).contains(&key));
    }
    assert_eq!(kinds.len(), 1);
    assert_eq!(format!("{kinds:?}"), "Kinds(1)");
    // As many as the bound, and not one more.
    assert_eq!(KINDS_LIMIT, 64);
    for n in 1..64 {
        kinds
            .register(Arc::new(FakeKind::new(&format!("k{n}"))))
            .unwrap();
    }
    assert_eq!(kinds.len(), 64);
    assert_eq!(
        kinds.register(Arc::new(FakeKind::new("one-more"))),
        Err(KindNotRegistered::TooMany { limit: 64 })
    );
    assert_eq!(kinds.len(), 64);
    assert!(kinds.get("one-more").is_none());
    assert_eq!(kinds.get("k63").map(|kind| kind.name()), Some("k63"));
}

#[test]
fn two_capabilities_with_several_named_backends_resolve_a_choice_and_the_default() {
    let (config, credentials) = read(SEVERAL, "");
    let (kinds, _) = kinds(&["fake"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    assert_eq!(offers.every_not_ready().count(), 0);

    // The roster is the one a hand would build of what each kind declares: every
    // backend under its name, and the default the file gives, for each capability.
    let by_hand = Roster::new()
        .offer(
            Capability::Fill,
            "bria",
            declared("fake", "bria-erase", &["prompt"]),
        )
        .and_then(|r| r.offer(Capability::Fill, "lama", declared("fake", "lama", &[])))
        .and_then(|r| r.offer(Capability::Fill, "zeta", declared("fake", "zeta", &[])))
        .and_then(|r| r.default_for(Capability::Fill, "bria"))
        .and_then(|r| r.offer(Capability::Judge, "one", declared("fake", "judge-one", &[])))
        .and_then(|r| r.offer(Capability::Judge, "two", declared("fake", "judge-two", &[])))
        .and_then(|r| {
            let sam = Declaration::new(ModelIdentity::new("fake", "sam", "3"), &["phrase", "box"]);
            r.offer(Capability::Segment, "default", sam.unwrap())
        })
        .and_then(|r| r.default_for(Capability::Segment, "default"))
        .unwrap();
    assert_eq!(offers.roster(), &by_hand);

    let model = |capability, choice: Option<&str>, features: &[&str]| {
        offers
            .resolve(capability, choice, &needs(features))
            .map(|resolved| {
                (
                    resolved.name().to_owned(),
                    resolved.declaration().model().model().to_owned(),
                )
            })
    };
    let picked = |name: &str, model: &str| Ok((name.to_owned(), model.to_owned()));
    // The default, when the call chose none; the choice, when it made one.
    assert_eq!(
        model(Capability::Fill, None, &[]),
        picked("bria", "bria-erase")
    );
    assert_eq!(
        model(Capability::Fill, None, &[feature::PROMPT]),
        picked("bria", "bria-erase")
    );
    assert_eq!(
        model(Capability::Fill, Some("lama"), &[]),
        picked("lama", "lama")
    );
    assert_eq!(
        model(Capability::Fill, Some("zeta"), &[]),
        picked("zeta", "zeta")
    );
    // A choice that lacks a feature is not answered by the default, which has it.
    assert_eq!(
        model(Capability::Fill, Some("lama"), &[feature::PROMPT]),
        Err(Unanswered::Unresolved(Unresolved::MissingFeature {
            capability: Capability::Fill,
            name: "lama".to_owned(),
            feature: "prompt".to_owned(),
        }))
    );
    // A choice that is not offered is not answered by the default either.
    assert_eq!(
        model(Capability::Fill, Some("sam"), &[]),
        Err(Unanswered::Unresolved(Unresolved::UnknownChoice {
            capability: Capability::Fill,
            choice: "sam".to_owned(),
        }))
    );
    // Two backends and no default: a call that chose none is answered by neither.
    assert_eq!(offers.default_of(Capability::Judge), None);
    assert_eq!(
        model(Capability::Judge, None, &[]),
        Err(Unanswered::Unresolved(Unresolved::NoDefault {
            capability: Capability::Judge
        }))
    );
    assert_eq!(
        model(Capability::Judge, Some("two"), &[]),
        picked("two", "judge-two")
    );
    // The short form: one backend, named `default`, which is the default.
    assert_eq!(offers.default_of(Capability::Segment), Some("default"));
    assert_eq!(offers.default_of(Capability::Fill), Some("bria"));
    for choice in [None, Some("default")] {
        assert_eq!(
            model(
                Capability::Segment,
                choice,
                &[feature::PHRASE, feature::BOX]
            ),
            picked("default", "sam")
        );
    }
    assert!(model(Capability::Segment, None, &[feature::POINT]).is_err());

    // The same files and the same kinds give the same offers.
    assert_eq!(Offers::of(&config, &credentials, &kinds).unwrap(), offers);
    assert_eq!(format!("{offers:?}"), "Offers { ready: 6, not_ready: 0 }");
    // No config offers nothing.
    let none = Offers::of(&Config::empty(), &Credentials::none(), &kinds).unwrap();
    assert_eq!(none.roster(), &Roster::new());
    assert_eq!(format!("{none:?}"), "Offers { ready: 0, not_ready: 0 }");
}

#[test]
fn a_backend_whose_kind_is_not_registered_is_offered_as_not_ready_and_never_as_another() {
    let text = r#"
[backends.fill]
default = "gone"

[backends.fill.gone]
kind = "service"
model = "bria-erase"

[backends.fill.here]
kind = "fake"

[backends.segment]
kind = "pack"
"#;
    let (config, credentials) = read(text, "");
    let (kinds, fakes) = kinds(&["fake"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    let not_ready = |chosen| {
        Err(Unanswered::NotReady {
            capability: Capability::Fill,
            chosen,
            why: NotReady::UnknownKind,
        })
    };
    // The default is the one that is not ready: a call that chose none is told so,
    // and is not answered by the backend beside it, whatever it needs.
    assert_eq!(
        offers.resolve(Capability::Fill, None, &[]),
        not_ready(false)
    );
    assert_eq!(
        offers.resolve(Capability::Fill, None, &needs(&["prompt"])),
        not_ready(false)
    );
    assert_eq!(
        offers.resolve(Capability::Fill, Some("gone"), &[]),
        not_ready(true)
    );
    let here = offers.resolve(Capability::Fill, Some("here"), &[]).unwrap();
    assert_eq!(here.declaration().model().backend(), "fake");
    // It is offered: listed with its reason, and still the capability's default.
    assert_eq!(offers.default_of(Capability::Fill), Some("gone"));
    assert_eq!(
        offers.not_ready(Capability::Fill, "gone"),
        Some(&NotReady::UnknownKind)
    );
    assert_eq!(offers.not_ready(Capability::Fill, "here"), None);
    assert_eq!(offers.not_ready(Capability::Judge, "gone"), None);
    assert_eq!(
        offers.every_not_ready().collect::<Vec<_>>(),
        [
            ("fill", "gone", &NotReady::UnknownKind),
            ("segment", "default", &NotReady::UnknownKind)
        ]
    );
    assert_eq!(format!("{offers:?}"), "Offers { ready: 1, not_ready: 2 }");
    // The roster holds the ready ones and no default that is not: asked alone it
    // answers a call that chose none with no backend, never with `here`.
    let by_hand = Roster::new()
        .offer(Capability::Fill, "here", declared("fake", "m", &[]))
        .unwrap();
    assert_eq!(offers.roster(), &by_hand);
    assert_eq!(
        offers.roster().resolve(Capability::Fill, None, &[]),
        Err(Unresolved::NoDefault {
            capability: Capability::Fill
        })
    );
    // The sentences are fixed words and the capability. Prepare is told the same.
    let asked: &dyn Backends = &offers;
    assert_eq!(
        asked
            .resolve(Capability::Fill, None, &[])
            .unwrap_err()
            .why(),
        "the default fill backend is not ready: its kind is not one this build has"
    );
    assert_eq!(
        asked
            .resolve(Capability::Fill, Some("gone"), &[])
            .unwrap_err()
            .why(),
        "the fill backend the call chose is not ready: its kind is not one this build has"
    );
    assert_eq!(
        asked
            .resolve(Capability::Segment, Some("default"), &[])
            .unwrap_err()
            .why(),
        "the segment backend the call chose is not ready: its kind is not one this build has"
    );
    assert_eq!(
        asked
            .resolve(Capability::Judge, None, &[])
            .unwrap_err()
            .why(),
        "no default backend is configured for judge, and the call chose none"
    );
    assert_eq!(
        asked.resolve(Capability::Fill, Some("here"), &[]),
        Ok(ModelIdentity::new("fake", "m", "1"))
    );
    // A kind is shown the tables that name it, and no other's.
    assert_eq!(fakes[0].shown().len(), 1);
    assert_eq!(fakes[0].shown()[0].name, "here");
    // With no kind at all, every backend is offered and none is ready.
    let none = Offers::of(&config, &credentials, &Kinds::new()).unwrap();
    assert_eq!(none.every_not_ready().count(), 3);
    assert_eq!(none.roster(), &Roster::new());
}

#[test]
fn a_backend_whose_kind_does_not_take_its_table_is_not_ready_with_the_kinds_own_words() {
    let marker = "setting-marker-9876";
    let text = format!(
        r#"
[backends.fill]
default = "odd"

[backends.fill.odd]
kind = "fake"

[backends.fill.odd.settings]
strength = "{marker}"

[backends.fill.whole]
kind = "fake"

[backends.fill.whole.settings]
features = 3

[backends.fill.spaced]
kind = "fake"

[backends.fill.spaced.settings]
features = "not a name"

[backends.fill.fine]
kind = "fake"
"#
    );
    let (config, credentials) = read(&text, "");
    let (kinds, _) = kinds(&["fake"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    let refused = |name: &str| match offers.resolve(Capability::Fill, Some(name), &[]) {
        Err(Unanswered::NotReady {
            why: NotReady::Refused(KindRefused::Table(why)),
            chosen: true,
            capability: Capability::Fill,
        }) => why,
        other => panic!("{other:?}"),
    };
    assert_eq!(refused("odd"), FAKE_UNKNOWN_SETTING);
    assert_eq!(refused("whole"), FAKE_SETTING_NOT_TEXT);
    assert_eq!(
        refused("spaced"),
        "a fake backend cannot declare that model, that version or those features"
    );
    assert!(offers.resolve(Capability::Fill, Some("fine"), &[]).is_ok());
    // The default is the one refused: the call that chose none is not handed `fine`.
    let unanswered = offers.resolve(Capability::Fill, None, &[]).unwrap_err();
    assert_eq!(
        unanswered.to_string(),
        format!(
            "the default fill backend is not ready: its kind does not take it: \
             {FAKE_UNKNOWN_SETTING}"
        )
    );
    // Nothing of the setting's value is in what is said or printed.
    let every: String = offers
        .every_not_ready()
        .map(|(_, _, why)| said(why))
        .collect();
    assert!(!format!("{every} {} {offers:?}", said(&unanswered)).contains(marker));
}

#[test]
fn a_variable_the_env_file_does_not_set_makes_its_backend_not_ready_naming_the_variable() {
    let key = made_up_key("");
    let text = r#"
[backends.fill]
default = "both"

[backends.fill.both]
kind = "fake"
url = "https://a.example/v1"
key_env = "A_KEY"
token_env = "B_TOKEN"

[backends.fill.neither]
kind = "fake"
url = "https://a.example/v1"
key_env = "C_KEY"
token_env = "B_TOKEN"

[backends.fill.token]
kind = "fake"
url = "https://a.example/v1"
token_env = "B_TOKEN"

[backends.fill.set]
kind = "fake"
url = "https://a.example/v1"
key_env = "A_KEY"

[backends.fill.plain]
kind = "fake"

[backends.judge]
kind = "service"
token_env = "B_TOKEN"
"#;
    let (config, credentials) = read(text, &format!("A_KEY={key}\nC_KEY=\n"));
    assert_eq!(
        credentials.missing().collect::<Vec<_>>(),
        ["B_TOKEN", "C_KEY"]
    );
    let (kinds, fakes) = kinds(&["fake"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    let missing = |variable: &str| NotReady::Missing {
        variable: variable.to_owned(),
    };
    assert_eq!(
        offers.every_not_ready().collect::<Vec<_>>(),
        [
            ("fill", "both", &missing("B_TOKEN")),
            // The key's variable first, when neither is set.
            ("fill", "neither", &missing("C_KEY")),
            ("fill", "token", &missing("B_TOKEN")),
            // Of a kind the build has not, too: the variable is said first, since it
            // is the one a person can put right in a file.
            ("judge", "default", &missing("B_TOKEN")),
        ]
    );
    // The reason names the variable, and nothing of a value: the default is not ready,
    // though its key is set, and is not answered by the backend that has all it names.
    let unanswered = offers.resolve(Capability::Fill, None, &[]).unwrap_err();
    assert_eq!(
        unanswered.to_string(),
        "the default fill backend is not ready: the env file does not set B_TOKEN, the \
         variable its credential is in"
    );
    assert!(!said(&unanswered).contains(&key));
    assert!(!format!("{offers:?}").contains(&key));
    // A kind is not asked about a backend with a missing variable, and is handed no
    // credential of one.
    let shown = fakes[0].shown();
    assert_eq!(
        shown.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["plain", "set"]
    );
    assert!(shown[0].key.is_none() && shown[0].token.is_none());
    assert_eq!(
        shown[1].key.as_ref().map(|k| k.expose()),
        Some(key.as_str())
    );
    assert!(offers.resolve(Capability::Fill, Some("set"), &[]).is_ok());
    // With the credentials of no env file, every backend that names a variable is not
    // ready, and none is handed anything.
    let none = Offers::of(&config, &Credentials::none(), &kinds).unwrap();
    assert_eq!(
        none.every_not_ready()
            .map(|(_, name, why)| (name, why.clone()))
            .collect::<Vec<_>>(),
        [
            ("both", missing("A_KEY")),
            ("neither", missing("C_KEY")),
            ("set", missing("A_KEY")),
            ("token", missing("B_TOKEN")),
            ("default", missing("B_TOKEN")),
        ]
    );
}

#[test]
fn a_kind_is_handed_its_own_backends_credential_bound_to_its_address_and_no_others() {
    let (key_a, key_b, token_b, key_c) = (
        made_up_key("-a"),
        made_up_key("-b"),
        made_up_key("-token"),
        made_up_key("-c"),
    );
    let text = r#"
[backends.fill.a]
kind = "one"
url = "https://a.example/v1/fill"
key_env = "A_KEY"

[backends.fill.b]
kind = "two"
url = "https://b.example:8443/v1"
key_env = "B_KEY"
token_env = "B_TOKEN"

[backends.judge.a]
kind = "one"
url = "https://c.example"
key_env = "C_KEY"
"#;
    let env = format!("A_KEY={key_a}\nB_KEY={key_b}\nB_TOKEN={token_b}\nC_KEY={key_c}\n");
    let (config, credentials) = read(text, &env);
    let (kinds, fakes) = kinds(&["one", "two"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    assert_eq!(offers.every_not_ready().count(), 0);

    // Each kind was shown the tables that name it, each with the credentials that
    // table names: the value, the kind's own header, and the origin of its address.
    let seen = |fake: &FakeKind| -> Vec<Seen> {
        fake.shown()
            .into_iter()
            .map(|shown| {
                let carried = [shown.key, shown.token]
                    .into_iter()
                    .flatten()
                    .map(|credential| {
                        (
                            credential.header().to_owned(),
                            credential.origin().to_string(),
                            credential.expose().to_owned(),
                        )
                    })
                    .collect();
                (shown.capability, shown.name, carried)
            })
            .collect()
    };
    let carried = |header: &str, origin: &str, value: &str| {
        (header.to_owned(), origin.to_owned(), value.to_owned())
    };
    assert_eq!(
        seen(&fakes[0]),
        [
            (
                Capability::Fill,
                "a".to_owned(),
                vec![carried(FAKE_KEY_HEADER, "https://a.example", &key_a)]
            ),
            (
                Capability::Judge,
                "a".to_owned(),
                vec![carried(FAKE_KEY_HEADER, "https://c.example", &key_c)]
            ),
        ]
    );
    assert_eq!(
        seen(&fakes[1]),
        [(
            Capability::Fill,
            "b".to_owned(),
            vec![
                carried(FAKE_KEY_HEADER, "https://b.example:8443", &key_b),
                carried(FAKE_TOKEN_HEADER, "https://b.example:8443", &token_b),
            ]
        )]
    );

    // A credential made for the backend at one origin is refused for a request to
    // another backend's address, and taken for one to its own.
    let for_a = fakes[0].shown()[0].key.clone().unwrap();
    let address = |capability, name: &str| -> Address {
        let mut backends = config.backends(capability);
        let (_, backend) = backends.find(|(offered, _)| *offered == name).unwrap();
        backend.address().unwrap().clone()
    };
    let to = |address: Address| Outgoing::new(address, Vec::new()).unwrap();
    assert_eq!(
        to(address(Capability::Fill, "b")).with_credential(for_a.clone()),
        Err(NotAllowed::Elsewhere)
    );
    assert_eq!(
        to(address(Capability::Judge, "a")).with_credential(for_a.clone()),
        Err(NotAllowed::Elsewhere)
    );
    let sent = to(address(Capability::Fill, "a")).with_credential(for_a.clone());
    assert_eq!(sent.unwrap().credential(), Some(&for_a));
    // The same host on another port is another origin.
    let elsewhere = Address::of("https://a.example:8443/v1/fill", &Secrets::none()).unwrap();
    assert_eq!(
        to(elsewhere).with_credential(for_a.clone()),
        Err(NotAllowed::Elsewhere)
    );

    // The backend declares who receives its requests: the origin of its address.
    let resolved = offers.resolve(Capability::Fill, Some("b"), &[]).unwrap();
    let recipient = Recipient::at("https://b.example:8443", &Secrets::none()).unwrap();
    assert_eq!(resolved.declaration().recipient(), Some(&recipient));
    // A backend that names a recipient and states no price has no bound (ADR 0012).
    let asked: &dyn Backends = &offers;
    assert_eq!(
        asked.bound(Capability::Fill, resolved.declaration().model()),
        Estimate::unbounded()
    );

    // Nothing that is printed holds a key: the offers, a table as its kind sees it, a
    // credential, the credentials.
    let printed = format!(
        "{offers:?} {credentials:?} {:?} {:?} {:?}",
        fakes[0].shown(),
        fakes[1].shown(),
        offers.roster()
    );
    for key in [&key_a, &key_b, &token_b, &key_c] {
        assert!(!printed.contains(key.as_str()));
    }
    assert!(printed.contains("Table { capability: Fill, backend: Backend {"));
}

#[test]
fn a_credential_that_cannot_be_carried_makes_its_backend_not_ready_and_shows_nothing() {
    // A value an env file may hold and a header may not: a letter outside ASCII. And a
    // backend that names a key and gives no address to bind it to.
    let odd = format!("{}\u{e9}", made_up_key(""));
    let text = r#"
[backends.fill.odd]
kind = "fake"
url = "https://a.example"
key_env = "ODD_KEY"

[backends.fill.nowhere]
kind = "fake"
key_env = "ODD_KEY"
"#;
    let (config, credentials) = read(text, &format!("ODD_KEY={odd}\n"));
    let (kinds, fakes) = kinds(&["fake"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    assert_eq!(
        offers.not_ready(Capability::Fill, "odd"),
        Some(&NotReady::Refused(KindRefused::Credential(
            NotAllowed::Character
        )))
    );
    assert_eq!(
        offers.not_ready(Capability::Fill, "nowhere"),
        Some(&NotReady::Refused(KindRefused::Table(FAKE_NO_ADDRESS)))
    );
    let unanswered = offers
        .resolve(Capability::Fill, Some("odd"), &[])
        .unwrap_err();
    assert_eq!(
        unanswered.to_string(),
        "the fill backend the call chose is not ready: its kind does not take it: its \
         credential cannot be carried: the text holds a character that is not printable \
         ASCII, or a space or a backslash"
    );
    assert!(!format!("{} {offers:?}", said(&unanswered)).contains(&odd[..8]));
    assert!(fakes[0].shown().is_empty());

    // A header's name is printed, so the egress type looks at it for a secret, with
    // the executor's secrets, which the kind is handed for that. A key that is, to the
    // letter, the name of the header the kind carries its key in: every credential of
    // that kind is refused, the other backend's too, and nothing is carried.
    let text = r#"
[backends.fill.named]
kind = "fake"
url = "https://a.example"
key_env = "HEADER_KEY"

[backends.judge]
kind = "fake"
url = "https://b.example"
key_env = "OTHER_KEY"
"#;
    let env = format!(
        "HEADER_KEY={FAKE_KEY_HEADER}\nOTHER_KEY={}\n",
        made_up_key("")
    );
    let (config, credentials) = read(text, &env);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    let secret = NotReady::Refused(KindRefused::Credential(NotAllowed::Secret));
    assert_eq!(
        offers.every_not_ready().collect::<Vec<_>>(),
        [("fill", "named", &secret), ("judge", "default", &secret)]
    );
    assert!(fakes[0].shown().is_empty());
}

#[test]
fn two_backends_of_one_capability_that_declare_one_model_are_both_offered() {
    // Known, and the snapshot's to settle (ADR 0011, "What a config offers"): the two
    // are told apart by their names and by nothing they declare, so an executor that
    // picks a backend by the model a call resolved would not tell them apart.
    let text = r#"
[backends.fill.first]
kind = "fake"
model = "lama"
url = "https://a.example"

[backends.fill.second]
kind = "fake"
model = "lama"
url = "https://b.example"
"#;
    let (config, credentials) = read(text, "");
    let (kinds, _) = kinds(&["fake"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    let resolved = |name| offers.resolve(Capability::Fill, Some(name), &[]).unwrap();
    let (first, second) = (resolved("first"), resolved("second"));
    assert_eq!(first.declaration().model(), second.declaration().model());
    assert_ne!(
        first.declaration().recipient(),
        second.declaration().recipient()
    );
    assert_eq!((first.name(), second.name()), ("first", "second"));
}

// --- a kind makes the backend (ADR 0015, "One snapshot of the config") -----------------------

const KEYED: &str = r#"
[backends.fill]
kind = "fake"
key_env = "FILL_KEY"
url = "https://a.example/v1"
"#;

/// Why the one backend of `KEYED` is not ready, with a fake kind that refuses every
/// table in these words, and everything the offers say and print of it.
fn kept_of(sentence: String, key: &str) -> (NotReady, String) {
    let (config, credentials) = read(KEYED, &format!("FILL_KEY={key}\n"));
    let (kinds, fakes) = kinds(&["fake"]);
    // The type takes fixed words, and a kind can leak a made text all the same.
    fakes[0].refuse_with(Box::leak(sentence.into_boxed_str()));
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    let why = offers
        .not_ready(Capability::Fill, "default")
        .unwrap()
        .clone();
    let unanswered = offers.resolve(Capability::Fill, None, &[]).unwrap_err();
    let all = format!("{} {} {offers:?}", said(&why), said(&unanswered));
    (why, all)
}

#[test]
fn a_kinds_sentence_that_holds_its_backends_key_is_replaced_with_fixed_words_and_a_long_one_is_cut()
{
    let key = made_up_key("K");
    let table = |why: &NotReady| match why {
        NotReady::Refused(KindRefused::Table(sentence)) => *sentence,
        other => panic!("{other:?}"),
    };
    // The key, whole, at the head of the sentence, and past where a sentence is cut:
    // the look is over the whole sentence, before the cut.
    for sentence in [
        format!("the key {key} was not taken"),
        format!("{}{key}", "w".repeat(300)),
        format!("{}{key}", "w".repeat(KIND_SENTENCE_LIMIT - 10)),
    ] {
        let (why, all) = kept_of(sentence, &key);
        assert_eq!(table(&why), KIND_SENTENCE_WITHHELD);
        assert!(!all.contains(&key) && !all.contains(&key[..12]), "{all}");
    }
    // A sentence that holds no secret is kept, its first 256 characters: counted in
    // characters, so the cut is never inside one.
    assert_eq!(KIND_SENTENCE_LIMIT, 256);
    for (sentence, kept) in [
        ("w".repeat(256), "w".repeat(256)),
        ("w".repeat(257), "w".repeat(256)),
        ("é".repeat(300), "é".repeat(256)),
        ("short words".to_owned(), "short words".to_owned()),
    ] {
        let (why, _) = kept_of(sentence, &key);
        assert_eq!(table(&why), kept);
    }
    // The egress types' refusal of a credential is fixed words of theirs, and is kept
    // as it is.
    assert!(KIND_SENTENCE_WITHHELD.chars().count() <= KIND_SENTENCE_LIMIT);
}

/// A kind that makes a filler whatever capability its table is under.
struct Crossed;

impl Kind for Crossed {
    fn name(&self) -> &str {
        "crossed"
    }

    fn make(&self, _: &Table<'_>) -> Result<Made, KindRefused> {
        Ok(Made::Fill(Arc::new(FakeFiller::new(Paint::Inverse))))
    }
}

#[test]
fn a_kind_that_makes_a_backend_of_another_capability_makes_its_backend_not_ready() {
    let text = "[backends.judge]\nkind = \"crossed\"\n\n[backends.fill]\nkind = \"crossed\"\n";
    let (config, credentials) = read(text, "");
    let mut kinds = Kinds::new();
    kinds.register(Arc::new(Crossed)).unwrap();
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    // Under `fill` the filler is what the table asked for.
    assert!(offers.resolve(Capability::Fill, None, &[]).is_ok());
    assert!(Offered::made(&offers, Capability::Fill, "default").is_some());
    // Under `judge` it is not: the backend is not ready, nothing of it is in the
    // roster, and no run could be handed it.
    assert_eq!(
        offers.not_ready(Capability::Judge, "default"),
        Some(&NotReady::OtherCapability)
    );
    let unanswered = offers.resolve(Capability::Judge, None, &[]).unwrap_err();
    assert_eq!(
        unanswered.to_string(),
        "the default judge backend is not ready: its kind made a backend of another capability"
    );
    assert!(Offered::made(&offers, Capability::Judge, "default").is_none());
    assert!(
        offers
            .roster()
            .resolve(Capability::Judge, None, &[])
            .is_err()
    );
}

#[test]
fn what_a_config_offers_holds_each_ready_backend_made_under_its_name_and_no_other() {
    let (config, credentials) = read(SEVERAL, "");
    let (kinds, fakes) = kinds(&["fake"]);
    let offers = Offers::of(&config, &credentials, &kinds).unwrap();
    // One backend was made for each table, in the one walk that offered it, and the
    // roster's entry is what that backend declares.
    assert_eq!(fakes[0].made().len(), 6);
    for (capability, name, model) in [
        (Capability::Fill, "bria", "bria-erase"),
        (Capability::Fill, "lama", "lama"),
        (Capability::Judge, "two", "judge-two"),
        (Capability::Segment, "default", "sam"),
    ] {
        let made = Offered::made(&offers, capability, name).unwrap();
        assert_eq!(made.capability(), capability);
        assert_eq!(made.declaration().model().model(), model);
        let resolved = offers.resolve(capability, Some(name), &[]).unwrap();
        assert_eq!(resolved.declaration(), made.declaration());
        assert_eq!(format!("{made:?}"), format!("Made({})", capability.name()));
    }
    // A name that is not offered, and a name of another capability, are held by none.
    assert!(Offered::made(&offers, Capability::Fill, "nobody").is_none());
    assert!(Offered::made(&offers, Capability::Judge, "bria").is_none());
    assert!(Offered::made(&offers, Capability::Segment, "lama").is_none());
    // The default is the file's.
    assert_eq!(Offered::default_of(&offers, Capability::Fill), Some("bria"));
    assert_eq!(Offered::default_of(&offers, Capability::Judge), None);
}

#[test]
fn two_offers_are_equal_when_they_offer_the_same_whatever_backends_they_made() {
    // Equality is of what is offered: the roster, each backend that is not ready
    // with its reason, and the defaults. Each walk makes backends of its own, which
    // have no equality and are never the same ones, so they are left out: two
    // readings of one config are equal, as two readings of one file should be.
    let (config, credentials) = read(SEVERAL, "");
    let (kinds, _) = kinds(&["fake"]);
    let one = Offers::of(&config, &credentials, &kinds).unwrap();
    let two = Offers::of(&config, &credentials, &kinds).unwrap();
    assert_eq!(one, two);
    let fill = |offers: &Offers| match Offered::made(offers, Capability::Fill, "lama") {
        Some(Made::Fill(filler)) => filler,
        other => panic!("{other:?}"),
    };
    assert!(!Arc::ptr_eq(&fill(&one), &fill(&two)));
    // A clone holds the same backends.
    assert!(Arc::ptr_eq(&fill(&one), &fill(&one.clone())));
    // Another default, another model, and a backend that is not ready for another
    // reason are each another thing offered.
    for other in [
        SEVERAL.replace("default = \"bria\"", "default = \"lama\""),
        SEVERAL.replace("model = \"zeta\"", "model = \"zeta-2\""),
        SEVERAL.replace(
            "model = \"zeta\"",
            "model = \"zeta\"\nkey_env = \"ZETA_KEY\"",
        ),
    ] {
        let (config, credentials) = read(&other, "");
        assert_ne!(one, Offers::of(&config, &credentials, &kinds).unwrap());
    }
    let missing = SEVERAL.replace(
        "model = \"zeta\"",
        "model = \"zeta\"\nkey_env = \"ZETA_KEY\"",
    );
    let unknown = SEVERAL.replace(
        "[backends.fill.zeta]\nkind = \"fake\"",
        "[backends.fill.zeta]\nkind = \"other\"",
    );
    let (missing, none) = read(&missing, "");
    let (unknown, nothing) = read(&unknown, "");
    assert_ne!(
        Offers::of(&missing, &none, &kinds).unwrap(),
        Offers::of(&unknown, &nothing, &kinds).unwrap()
    );
}
