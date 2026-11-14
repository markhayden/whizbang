//! The strict reader of a config file (ADR 0011, "How the file is read").
//!
//! A config is a file a person writes, and its bytes are not trusted: the length is
//! looked at before anything is read, the text is looked at whole for a secret before
//! it is parsed, every key is held to a closed list and every value to its kind and its
//! rule. The format's own reader refuses a key given twice and nesting past its depth.
//! The walk below is by hand, so that every sentence a refusal can hold is written here
//! and none can quote a value.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::Path;

use toml::{Table, Value};

use super::refused::shown;
use super::{
    ADDRESS_LIMIT, BACKENDS_LIMIT, Backend, CALL_CAP_LIMIT_MICROS, CAPABILITIES,
    CONFIG_BYTES_LIMIT, Config, ConfigRefused, DataClass, Offered, RECIPIENTS_LIMIT, Recipient,
    SETTING_TEXT_LIMIT, SETTINGS_LIMIT, Setting, TEXT_LIMIT, UPSTREAMS_LIMIT, VARIABLE_LIMIT,
};
use crate::contract::{Secrets, text};

type Read<T> = Result<T, ConfigRefused>;

const NAME_RULE: &str =
    "a lower-case letter, then lower-case letters, digits, _ or -, 64 characters at most";

impl Config {
    /// Reads the config file at this path. `secrets` are the values the reader was told
    /// must never be written, as the values of an env file are: a file that holds one
    /// anywhere is refused, and nothing of it is shown.
    ///
    /// The file's length is asked for before the file is opened, and a file over
    /// [`CONFIG_BYTES_LIMIT`] is refused from it. No more than the limit and one byte is
    /// read of a file that is longer than it said. The refusal holds no path.
    pub fn read(path: &Path, secrets: &Secrets) -> Result<Config, ConfigRefused> {
        let unreadable = |error: std::io::Error| ConfigRefused::Unreadable(error.kind());
        let length = std::fs::metadata(path).map_err(unreadable)?.len();
        if length > CONFIG_BYTES_LIMIT {
            return Err(ConfigRefused::TooLarge {
                limit: CONFIG_BYTES_LIMIT,
            });
        }
        let file = std::fs::File::open(path).map_err(unreadable)?;
        let mut bytes = Vec::new();
        file.take(CONFIG_BYTES_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(unreadable)?;
        Config::parse(&bytes, secrets)
    }

    /// Reads these bytes as a config file, as [`Config::read`] reads a file's.
    pub fn parse(bytes: &[u8], secrets: &Secrets) -> Result<Config, ConfigRefused> {
        if bytes.len() as u64 > CONFIG_BYTES_LIMIT {
            return Err(ConfigRefused::TooLarge {
                limit: CONFIG_BYTES_LIMIT,
            });
        }
        // The whole file, as it is, before it is taken for text or parsed: a comment
        // and a line that is not TOML are looked at like any other.
        if secrets.redact_bytes(bytes) != bytes {
            return Err(ConfigRefused::HoldsSecret);
        }
        let text = std::str::from_utf8(bytes).map_err(|error| ConfigRefused::NotText {
            line: line_of(bytes, error.valid_up_to()),
        })?;
        let control = |byte: &u8| (*byte < 0x20 && !b"\t\n\r".contains(byte)) || *byte == 0x7f;
        if let Some(at) = bytes.iter().position(control) {
            return Err(ConfigRefused::NotText {
                line: line_of(bytes, at),
            });
        }
        let table = text
            .parse::<Table>()
            .map_err(|error| not_toml(text, &error, secrets))?;
        // Every key and every text as the format decoded it: an escape can spell what
        // the file's bytes do not hold. From here on a key that is shown holds no
        // secret, so no cut of it can hold part of one.
        if holds_secret(&table, secrets) {
            return Err(ConfigRefused::HoldsSecret);
        }
        config(table)
    }
}

/// The line a byte is on, counted from 1.
fn line_of(bytes: &[u8], at: usize) -> usize {
    1 + bytes[..at.min(bytes.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

/// Whether a key or a text anywhere in the table holds a secret.
fn holds_secret(table: &Table, secrets: &Secrets) -> bool {
    let held = |text: &str| secrets.redact(text) != text;
    let mut values: Vec<&Value> = Vec::new();
    let mut tables: Vec<&Table> = vec![table];
    loop {
        while let Some(table) = tables.pop() {
            for (key, value) in table {
                if held(key) {
                    return true;
                }
                values.push(value);
            }
        }
        match values.pop() {
            None => return false,
            Some(Value::String(text)) if held(text) => return true,
            Some(Value::Array(items)) => values.extend(items),
            Some(Value::Table(table)) => tables.push(table),
            Some(_) => {}
        }
    }
}

/// The refusal of text the format's reader did not take. Its own sentence is not passed
/// on, since it is not written here; the line is. A key given twice is named, decoded
/// as the format decodes a key and looked at for a secret before it is shown.
fn not_toml(text: &str, error: &toml::de::Error, secrets: &Secrets) -> ConfigRefused {
    let span = error.span();
    let line = line_of(text.as_bytes(), span.as_ref().map_or(0, |span| span.start));
    if !error.message().contains("duplicate key") {
        return ConfigRefused::NotToml { line };
    }
    let written = span.and_then(|span| text.get(span)).unwrap_or_default();
    // What stands where the second key stands, read as a key and nothing else.
    let Ok(read) = format!("{written} = 0").parse::<Table>() else {
        return ConfigRefused::NotToml { line };
    };
    let mut keys: Vec<&str> = Vec::new();
    let mut table = &read;
    while let Some((key, value)) = table.iter().next() {
        keys.push(key);
        match value {
            Value::Table(inner) if table.len() == 1 => table = inner,
            _ => break,
        }
    }
    let key = keys.join(".");
    if secrets.redact(&key) != key {
        return ConfigRefused::HoldsSecret;
    }
    ConfigRefused::Twice {
        key: shown(&key),
        line,
    }
}

/// Where the reader is: the keys from the top of the file, with dots. A key that came
/// from the file is escaped and cut where it is added.
#[derive(Clone)]
struct At(String);

impl At {
    fn top(key: &str) -> At {
        At(shown(key))
    }

    fn then(&self, key: &str) -> At {
        At(format!("{}.{}", self.0, shown(key)))
    }

    fn refuse(&self, why: impl Into<String>) -> ConfigRefused {
        ConfigRefused::Key {
            key: self.0.clone(),
            why: why.into(),
        }
    }

    /// The refusal of a value of the wrong kind. It says the kind, never the value.
    fn wrong(&self, wanted: &str, found: &Value) -> ConfigRefused {
        let found = match found {
            Value::String(_) => "a text",
            Value::Integer(_) => "a whole number",
            Value::Float(_) => "a number with a fraction",
            Value::Boolean(_) => "true or false",
            Value::Datetime(_) => "a date or a time",
            Value::Array(_) => "a list",
            Value::Table(_) => "a table",
        };
        self.refuse(format!("must be {wanted}, and is {found}"))
    }

    fn table(&self, value: Value) -> Read<Table> {
        match value {
            Value::Table(table) => Ok(table),
            other => Err(self.wrong("a table", &other)),
        }
    }

    fn text(&self, value: Value) -> Read<String> {
        match value {
            Value::String(text) => Ok(text),
            other => Err(self.wrong("a text", &other)),
        }
    }

    /// A text of printable ASCII, not empty, with no space at either end.
    fn plain(&self, value: Value, limit: usize) -> Read<String> {
        let text = self.text(value)?;
        let printable = text.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
        if text.is_empty() || !printable || text.starts_with(' ') || text.ends_with(' ') {
            return Err(self.refuse(
                "must be a text of ASCII letters, digits, marks and spaces, not empty, \
                 with no space at either end",
            ));
        }
        if text.len() > limit {
            return Err(self.refuse(format!("is longer than {limit} characters")));
        }
        Ok(text)
    }

    /// A name, as an action's name is one.
    fn name(&self, value: Value) -> Read<String> {
        let text = self.text(value)?;
        if text::action_name(&text).is_err() {
            return Err(self.refuse(format!("must be a name: {NAME_RULE}")));
        }
        Ok(text)
    }

    /// The name of a variable. Whatever stands there may be the key itself, written
    /// where its variable's name belongs: it is never shown, and neither is its kind.
    fn variable(&self, value: Value) -> Read<String> {
        let name = match value {
            Value::String(name) => name,
            _ => String::new(),
        };
        let first = name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_');
        let rest = name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
        if !first || !rest || name.len() > VARIABLE_LIMIT {
            return Err(self.refuse(format!(
                "must name a variable: capital letters, digits and _, not a digit first, \
                 {VARIABLE_LIMIT} characters at most; what is written there is not shown"
            )));
        }
        Ok(name)
    }

    /// An origin: `http` or `https`, a host and perhaps a port.
    fn origin(&self, value: Value) -> Read<String> {
        let text = self.text(value)?;
        if !is_origin(&text) {
            return Err(self.refuse(
                "must be an origin: http:// or https://, a host in lower case, perhaps a \
                 port, and nothing else; what is written there is not shown",
            ));
        }
        Ok(text)
    }

    /// The refusal of the first key left in a table once the known ones were taken.
    fn unknown(&self, table: &Table, known: &str) -> Read<()> {
        match table.keys().next() {
            Some(key) => Err(self
                .then(key)
                .refuse(format!("is not known; the keys here are {known}"))),
            None => Ok(()),
        }
    }
}

/// Whether this is a scheme, a host and perhaps a port, and nothing else. A host is
/// lower-case letters, digits and hyphens in parts with dots between them, 253
/// characters at most; a port is 1 to 65535 with no leading zero.
fn is_origin(text: &str) -> bool {
    let Some(rest) = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
    else {
        return false;
    };
    let (host, port) = match rest.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (rest, None),
    };
    let part = |part: &str| {
        !part.is_empty()
            && !part.starts_with('-')
            && !part.ends_with('-')
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    };
    let port = port.is_none_or(|port| {
        port.len() <= 5
            && !port.starts_with('0')
            && port.bytes().all(|byte| byte.is_ascii_digit())
            && port.parse::<u32>().is_ok_and(|port| port <= 65535)
    });
    host.len() <= 253 && host.split('.').all(part) && port
}

/// Whether this is an origin and perhaps a path, with nothing a credential is written
/// in: no user, no password, no query and no fragment.
fn is_address(text: &str) -> bool {
    let after_scheme = text.find("://").map_or(0, |at| at + 3);
    let (origin, path) = match text[after_scheme..].find('/') {
        Some(at) => text.split_at(after_scheme + at),
        None => (text, ""),
    };
    let path = path
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && !b"?#\\".contains(&byte));
    is_origin(origin) && path && text.len() <= ADDRESS_LIMIT
}

fn config(mut table: Table) -> Read<Config> {
    let mut config = Config::empty();
    if let Some(value) = table.remove("backends") {
        config.backends = backends(&At::top("backends"), value)?;
    }
    if let Some(value) = table.remove("spend") {
        let at = At::top("spend");
        let mut spend = at.table(value)?;
        if let Some(value) = spend.remove("call_cap_usd") {
            config.call_cap_micros = Some(call_cap(&at.then("call_cap_usd"), &value)?);
        }
        at.unknown(&spend, "call_cap_usd")?;
    }
    if let Some(value) = table.remove("egress") {
        let at = At::top("egress");
        let mut egress = at.table(value)?;
        if let Some(value) = egress.remove("recipients") {
            config.recipients = recipients(&at.then("recipients"), value)?;
        }
        at.unknown(&egress, "recipients")?;
    }
    if let Some(value) = table.remove("models") {
        let at = At::top("models");
        if let Some(key) = at.table(value)?.keys().next() {
            return Err(at
                .then(key)
                .refuse("is not known; the table `models` has no key yet"));
        }
    }
    if table.contains_key("tools") {
        return Err(At::top("tools")
            .refuse("is not read yet: the table of tools is given its shape at milestone 2"));
    }
    match table.keys().next() {
        Some(key) => {
            Err(At::top(key)
                .refuse("is not known; the tables are backends, spend, egress and models"))
        }
        None => Ok(config),
    }
}

fn backends(at: &At, value: Value) -> Read<BTreeMap<&'static str, Offered>> {
    let mut read = BTreeMap::new();
    for (key, value) in at.table(value)? {
        let at = at.then(&key);
        let Some(capability) = CAPABILITIES.iter().find(|one| one.name() == key) else {
            return Err(at.refuse("is not known; the capabilities are fill, judge and segment"));
        };
        let mut table = at.table(value)?;
        let mut offered = Offered::default();
        if table.contains_key("kind") {
            // The short form: one backend, named `default`, which is the default.
            offered.default = Some("default".to_owned());
            offered
                .backends
                .insert("default".to_owned(), backend(&at, table)?);
        } else {
            let default = match table.remove("default") {
                Some(value) => Some(at.then("default").name(value)?),
                None => None,
            };
            if table.len() > BACKENDS_LIMIT {
                return Err(at.refuse(format!("holds more than {BACKENDS_LIMIT} backends")));
            }
            for (name, value) in table {
                let at = at.then(&name);
                if text::action_name(&name).is_err() {
                    return Err(at.refuse(format!("is not a backend's name: {NAME_RULE}")));
                }
                offered
                    .backends
                    .insert(name, backend(&at, at.table(value)?)?);
            }
            if let Some(default) = &default
                && !offered.backends.contains_key(default)
            {
                return Err(at
                    .then("default")
                    .refuse("names a backend that is not written in this table"));
            }
            offered.default = default;
        }
        if !offered.backends.is_empty() {
            read.insert(capability.name(), offered);
        }
    }
    Ok(read)
}

fn backend(at: &At, mut table: Table) -> Read<Backend> {
    let Some(kind) = table.remove("kind") else {
        return Err(at.then("kind").refuse("is missing: a backend has a kind"));
    };
    let kind = at.then("kind").name(kind)?;
    let mut take = |key: &str| table.remove(key).map(|value| (at.then(key), value));
    let model = match take("model") {
        Some((at, value)) => Some(at.plain(value, TEXT_LIMIT)?),
        None => None,
    };
    let provider = match take("provider") {
        Some((at, value)) => Some(at.name(value)?),
        None => None,
    };
    let url = match take("url") {
        Some((at, value)) => {
            let text = at.text(value)?;
            if !is_address(&text) {
                return Err(at.refuse(format!(
                    "must be an address: http:// or https://, a host in lower case, perhaps \
                     a port and a path, with no user, password, query or fragment, \
                     {ADDRESS_LIMIT} characters at most; what is written there is not shown"
                )));
            }
            Some(text)
        }
        None => None,
    };
    let key_env = match take("key_env") {
        Some((at, value)) => Some(at.variable(value)?),
        None => None,
    };
    let token_env = match take("token_env") {
        Some((at, value)) => Some(at.variable(value)?),
        None => None,
    };
    let mut settings = BTreeMap::new();
    if let Some((at, value)) = take("settings") {
        let table = at.table(value)?;
        if table.len() > SETTINGS_LIMIT {
            return Err(at.refuse(format!("holds more than {SETTINGS_LIMIT} settings")));
        }
        for (key, value) in table {
            let at = at.then(&key);
            if text::field_name(&key).is_err() {
                return Err(at.refuse(
                    "is not a setting's key: a lower-case letter, then lower-case letters, \
                     digits or _, 64 characters at most",
                ));
            }
            let setting = match value {
                Value::Integer(number) => Setting::Integer(number),
                Value::Boolean(boolean) => Setting::Boolean(boolean),
                Value::String(_) => Setting::Text(at.plain(value, SETTING_TEXT_LIMIT)?),
                other => {
                    return Err(at.wrong("a text, a whole number, or true or false", &other));
                }
            };
            settings.insert(key, setting);
        }
    }
    at.unknown(
        &table,
        "kind, model, provider, url, key_env, token_env and settings",
    )?;
    Ok(Backend {
        kind,
        model,
        provider,
        url,
        key_env,
        token_env,
        settings,
    })
}

/// The cap of one call, in millionths of a US dollar, from an amount in dollars.
///
/// A whole number is that many dollars. A number with a fraction is taken times
/// 1,000,000 and rounded to the nearest whole number, and that number over 1,000,000
/// must be the number that was written: so an amount finer than a millionth is refused,
/// not rounded. Multiplying, rounding and dividing doubles is the same on every machine.
fn call_cap(at: &At, value: &Value) -> Read<u64> {
    let dollars_limit = CALL_CAP_LIMIT_MICROS / 1_000_000;
    let micros = match value {
        Value::Integer(dollars) => u64::try_from(*dollars)
            .ok()
            .filter(|dollars| *dollars <= dollars_limit)
            .map(|dollars| dollars * 1_000_000),
        Value::Float(dollars) => {
            let scaled = (dollars * 1_000_000.0).round();
            #[allow(clippy::cast_precision_loss)]
            let within = scaled >= 1.0 && scaled <= CALL_CAP_LIMIT_MICROS as f64;
            #[allow(clippy::float_cmp)]
            let exact = scaled / 1_000_000.0 == *dollars;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            (within && exact).then_some(scaled as u64)
        }
        other => return Err(at.wrong("a number", other)),
    };
    micros.filter(|micros| *micros > 0).ok_or_else(|| {
        at.refuse(format!(
            "must be an amount in US dollars of more than 0 and at most {dollars_limit}, \
             with at most six decimal places"
        ))
    })
}

fn recipients(at: &At, value: Value) -> Read<BTreeMap<String, Recipient>> {
    let table = at.table(value)?;
    if table.len() > RECIPIENTS_LIMIT {
        return Err(at.refuse(format!("holds more than {RECIPIENTS_LIMIT} recipients")));
    }
    let mut read = BTreeMap::new();
    for (name, value) in table {
        let at = at.then(&name);
        if text::action_name(&name).is_err() {
            return Err(at.refuse(format!("is not a recipient's name: {NAME_RULE}")));
        }
        read.insert(name, recipient(&at, at.table(value)?)?);
    }
    Ok(read)
}

fn recipient(at: &At, mut table: Table) -> Read<Recipient> {
    let Some(origin) = table.remove("origin") else {
        return Err(at
            .then("origin")
            .refuse("is missing: a recipient has an origin"));
    };
    let origin = at.then("origin").origin(origin)?;
    let account = match table.remove("account") {
        Some(value) => Some(at.then("account").plain(value, TEXT_LIMIT)?),
        None => None,
    };
    let mut upstreams = BTreeSet::new();
    if let Some(value) = table.remove("upstreams") {
        let at = at.then("upstreams");
        let Value::Array(items) = value else {
            return Err(at.wrong("a list of origins", &value));
        };
        if items.len() > UPSTREAMS_LIMIT {
            return Err(at.refuse(format!("holds more than {UPSTREAMS_LIMIT} origins")));
        }
        for item in items {
            let upstream = at.origin(item)?;
            if upstream == origin {
                return Err(at.refuse("holds the recipient's own origin"));
            }
            if !upstreams.insert(upstream) {
                return Err(at.refuse("holds one origin twice"));
            }
        }
    }
    let Some(value) = table.remove("classes") else {
        return Err(at.then("classes").refuse(
            "is missing: a recipient lists the classes of data it may receive, or none, as []",
        ));
    };
    let mut classes = BTreeSet::new();
    {
        let at = at.then("classes");
        let Value::Array(items) = value else {
            return Err(at.wrong("a list of classes", &value));
        };
        for item in items {
            let name = at.text(item)?;
            let Some(class) = DataClass::ALL.iter().find(|class| class.name() == name) else {
                return Err(at.refuse(
                    "holds a class that is not known; the classes are derived_images, \
                     evidence, prompts and source_images",
                ));
            };
            if !classes.insert(*class) {
                return Err(at.refuse("holds one class twice"));
            }
        }
    }
    let probe = match table.remove("probe") {
        Some(Value::Boolean(probe)) => probe,
        Some(other) => return Err(at.then("probe").wrong("true or false", &other)),
        None => false,
    };
    at.unknown(&table, "origin, account, upstreams, classes and probe")?;
    Ok(Recipient {
        origin,
        account,
        upstreams,
        classes,
        probe,
    })
}
