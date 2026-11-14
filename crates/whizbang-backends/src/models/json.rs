//! JSON, read strictly (ADR 0017, "How a record is read").
//!
//! Stored bytes are not trusted. Their length is looked at before anything else, and
//! then the bytes whole for a secret. They are read into a value that refuses a field
//! given twice, which the usual reader takes in silence; every key and every text of
//! that value, as JSON decoded it, is looked at for a secret again, since an escape can
//! spell what the bytes do not hold. The walk over the value is by hand, so that every
//! refusal is made here and none quotes what was read.

use std::collections::BTreeMap;
use std::io::Read as _;

use serde::de::{Deserialize, Deserializer, Error as _, MapAccess, SeqAccess, Visitor};
use whizbang_core::contract::Secrets;

use super::name::holds_secret;
use super::{Refused, Rule, What};

/// What the reader's own refusal of a second field starts with. No other refusal of
/// the format's reader is about the value read, so this is how the two are told apart.
const TWICE: &str = "a field is given twice";

/// A JSON value. A number that is a whole number from 0 to what 64 bits hold is kept;
/// of any other, only that it was one.
pub(super) enum Json {
    Null,
    Bool,
    Whole(u64),
    OtherNumber,
    Text(String),
    List(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Json, D::Error> {
        struct Reading;

        impl<'de> Visitor<'de> for Reading {
            type Value = Json;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON value")
            }

            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }

            fn visit_bool<E>(self, _: bool) -> Result<Json, E> {
                Ok(Json::Bool)
            }

            fn visit_u64<E>(self, whole: u64) -> Result<Json, E> {
                Ok(Json::Whole(whole))
            }

            fn visit_i64<E>(self, _: i64) -> Result<Json, E> {
                Ok(Json::OtherNumber)
            }

            fn visit_f64<E>(self, _: f64) -> Result<Json, E> {
                Ok(Json::OtherNumber)
            }

            fn visit_str<E>(self, text: &str) -> Result<Json, E> {
                Ok(Json::Text(text.to_owned()))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut list: A) -> Result<Json, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = list.next_element()? {
                    items.push(item);
                }
                Ok(Json::List(items))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
                let mut fields = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if fields.contains_key(&key) {
                        return Err(A::Error::custom(TWICE));
                    }
                    let value = map.next_value()?;
                    fields.insert(key, value);
                }
                Ok(Json::Object(fields))
            }
        }

        deserializer.deserialize_any(Reading)
    }
}

impl Json {
    /// Whether any key or any text of the value holds a secret.
    fn holds_secret(&self, secrets: &Secrets) -> bool {
        let mut left = vec![self];
        while let Some(value) = left.pop() {
            match value {
                Json::Text(text) if holds_secret(text, secrets) => return true,
                Json::List(items) => left.extend(items),
                Json::Object(fields) => {
                    if fields.keys().any(|key| holds_secret(key, secrets)) {
                        return true;
                    }
                    left.extend(fields.values());
                }
                _ => {}
            }
        }
        false
    }
}

/// No more than `limit` bytes and one of what this gives, however much it has.
pub(super) fn read(what: What, from: impl std::io::Read, limit: u64) -> Result<Vec<u8>, Refused> {
    let mut bytes = Vec::new();
    from.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Refused::Unreadable {
            what,
            kind: error.kind(),
        })?;
    Ok(bytes)
}

/// These bytes as a JSON value: refused over `limit` before anything of them is looked
/// at, then when they hold a secret, when they are not JSON, when an object gives a
/// field twice, and when a key or a text holds a secret as it was decoded.
pub(super) fn parse(
    what: What,
    bytes: &[u8],
    limit: u64,
    secrets: &Secrets,
) -> Result<Json, Refused> {
    if bytes.len() as u64 > limit {
        return Err(Refused::TooLarge { what, limit });
    }
    if secrets.redact_bytes(bytes) != bytes {
        return Err(Refused::HoldsSecret);
    }
    let json: Json = serde_json::from_slice(bytes).map_err(|error| {
        let line = error.line();
        if error.to_string().starts_with(TWICE) {
            Refused::Twice { what, line }
        } else {
            Refused::NotJson { what, line }
        }
    })?;
    if json.holds_secret(secrets) {
        return Err(Refused::HoldsSecret);
    }
    Ok(json)
}

/// An object being taken apart: each field is taken once, by its name, and what is
/// left when the reader is done is a field it does not know.
pub(super) struct Object {
    at: String,
    fields: BTreeMap<String, Json>,
}

impl Object {
    /// The value at this place, which is an object or is refused.
    pub(super) fn of(json: Json, at: &str) -> Result<Object, Refused> {
        match json {
            Json::Object(fields) => Ok(Object {
                at: at.to_owned(),
                fields,
            }),
            _ => Err(Refused::at(at, Rule::Kind)),
        }
    }

    /// The place of one of this object's fields.
    pub(super) fn place(&self, key: &str) -> String {
        if self.at.is_empty() {
            key.to_owned()
        } else {
            format!("{}.{key}", self.at)
        }
    }

    fn take(&mut self, key: &str) -> Result<Json, Refused> {
        self.fields
            .remove(key)
            .ok_or_else(|| Refused::at(&self.place(key), Rule::Missing))
    }

    pub(super) fn text(&mut self, key: &str) -> Result<String, Refused> {
        match self.take(key)? {
            Json::Text(text) => Ok(text),
            _ => Err(Refused::at(&self.place(key), Rule::Kind)),
        }
    }

    pub(super) fn whole(&mut self, key: &str) -> Result<u64, Refused> {
        match self.take(key)? {
            Json::Whole(whole) => Ok(whole),
            _ => Err(Refused::at(&self.place(key), Rule::Kind)),
        }
    }

    /// A list of at most `most`: the count is looked at before any of its items is.
    pub(super) fn list(&mut self, key: &str, most: usize) -> Result<Vec<Json>, Refused> {
        match self.take(key)? {
            Json::List(items) if items.len() > most => {
                Err(Refused::at(&self.place(key), Rule::Count { most }))
            }
            Json::List(items) => Ok(items),
            _ => Err(Refused::at(&self.place(key), Rule::Kind)),
        }
    }

    /// Every field was taken, or the object holds one that is not its own.
    pub(super) fn done(self) -> Result<(), Refused> {
        if self.fields.is_empty() {
            Ok(())
        } else {
            Err(Refused::at(&self.at, Rule::Unknown))
        }
    }
}

/// A text at this place, or the refusal of a value of another kind.
pub(super) fn text(json: Json, at: &str) -> Result<String, Refused> {
    match json {
        Json::Text(text) => Ok(text),
        _ => Err(Refused::at(at, Rule::Kind)),
    }
}
