//! The settings a run records (ADR 0011, "Which settings a run records"; ADR 0013, "A
//! setting"): from a config and the backends a call resolved, names and values in the
//! order of the names' bytes.
//!
//! Each pair is one a record takes, made as a record makes a setting
//! (`contract::record::Setting::new`): a text has the secrets taken out while it is
//! whole and is then cut. What a record would not take makes no pair and is not left
//! out in silence: the whole is refused, naming the setting and never its value.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::record::{ArgumentValue, SETTINGS_LIMIT, Setting as Recorded};
use crate::contract::{Capability, NotJudged, Secrets, Settings, Value};

use super::{BACKENDS_LIMIT, CAPABILITIES, Config, KEY_SHOWN_LIMIT, Setting};

/// Why the settings of a run were not made. A setting is named by its name in the
/// record, which is made of the config's keys; its value is never shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRefused {
    /// More backends were named than a config can hold, or the settings come to more
    /// than a record lists.
    TooMany { limit: usize },
    /// A backend that was named is not one the config gives the capability. The name
    /// is not shown: it did not come from the config.
    NotOffered { capability: Capability },
    /// The name a setting would be recorded under holds a secret. It is not shown.
    HoldsSecret,
    /// The name a setting would be recorded under is longer than a record takes: its
    /// first characters. No config that was read gives one: the reader refuses a
    /// backend's name and a setting's key that are too long together. It is kept so
    /// that such a name, if the two ever part, is refused and not left out.
    Name { name: String },
    /// The setting is a whole number whose digits, as they are written, hold a secret:
    /// a record writes a number as it is, so it writes none.
    Value { name: String },
}

impl std::fmt::Display for SettingsRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsRefused::TooMany { limit } => write!(
                f,
                "there are more than {limit} backends or settings to record for one run"
            ),
            SettingsRefused::NotOffered { capability } => write!(
                f,
                "a {} backend whose settings were asked for is not one the config gives; \
                 its name is not shown",
                capability.name()
            ),
            SettingsRefused::HoldsSecret => f.write_str(
                "the name a setting would be recorded under holds a secret, and is not shown",
            ),
            SettingsRefused::Name { name } => write!(
                f,
                "the setting {name} has a name longer than a record takes"
            ),
            SettingsRefused::Value { name } => write!(
                f,
                "the setting {name} is a number whose digits hold a secret, and a record \
                 writes a number as it is; the value is not shown"
            ),
        }
    }
}

impl std::error::Error for SettingsRefused {}

/// A name as a refusal shows it: its first [`KEY_SHOWN_LIMIT`] characters, and three
/// dots when there were more. A name here is made of the config's keys, which are
/// ASCII names, and holds no secret: that is looked at before this is called.
fn shown(name: &str) -> String {
    match name.char_indices().nth(KEY_SHOWN_LIMIT) {
        Some((cut, _)) => format!("{}...", &name[..cut]),
        None => name.to_owned(),
    }
}

/// The name a backend's setting is recorded under:
/// `backends.<capability>.<name>.settings.<key>`. The config's reader holds it to the
/// length a record takes ([`SETTING_NAME_LIMIT`](crate::contract::record::SETTING_NAME_LIMIT)),
/// with this same function, so the name that is checked is the name that is made.
pub(super) fn setting_name(capability: &str, backend: &str, key: &str) -> String {
    format!("backends.{capability}.{backend}.settings.{key}")
}

impl Config {
    /// The settings a run records (ADR 0011): for each backend named here, which are
    /// those the run's call resolved, `backends.<capability>.<name>.kind`, `.model`,
    /// `.provider` and `.url` where the file gives them, and each of its settings as
    /// `backends.<capability>.<name>.settings.<key>`; and `spend.call_cap_micros`
    /// when the file caps a call. In the order of the names' bytes, each once.
    ///
    /// No variable's name, no credential, no backend that is not named here and
    /// nothing of the `egress` table is in them. A text is as a record keeps one (ADR
    /// 0013): the secrets are taken out of it whole, and it is then cut. Anything a
    /// record would not take is a refusal of the whole, so that nothing is left out
    /// without a word.
    pub fn settings_of(
        &self,
        resolved: &[(Capability, &str)],
        secrets: &Secrets,
    ) -> Result<Vec<(String, Value)>, SettingsRefused> {
        let most = CAPABILITIES.len() * BACKENDS_LIMIT;
        if resolved.len() > most {
            return Err(SettingsRefused::TooMany { limit: most });
        }
        let resolved: BTreeSet<(&'static str, &str)> = resolved
            .iter()
            .map(|(capability, name)| (capability.name(), *name))
            .collect();
        let mut pairs: BTreeMap<String, Value> = BTreeMap::new();
        for capability in CAPABILITIES {
            let named = resolved
                .iter()
                .filter(|(of, _)| *of == capability.name())
                .map(|(_, name)| *name);
            for name in named {
                let mut backends = self.backends(capability);
                let Some((_, backend)) = backends.find(|(offered, _)| *offered == name) else {
                    return Err(SettingsRefused::NotOffered { capability });
                };
                let under = format!("backends.{}.{name}", capability.name());
                pairs.insert(format!("{under}.kind"), Value::text(backend.kind()));
                let url = backend.url();
                for (key, text) in [
                    ("model", backend.model()),
                    ("provider", backend.provider()),
                    ("url", url.as_deref()),
                ] {
                    if let Some(text) = text {
                        pairs.insert(format!("{under}.{key}"), Value::text(text));
                    }
                }
                for (key, setting) in backend.settings() {
                    let value = match setting {
                        Setting::Text(text) => Value::text(text),
                        Setting::Integer(number) => Value::Integer(*number),
                        Setting::Boolean(boolean) => Value::Boolean(*boolean),
                    };
                    pairs.insert(setting_name(capability.name(), name, key), value);
                }
            }
        }
        if let Some(micros) = self.call_cap_micros() {
            let name = "spend.call_cap_micros";
            // A cap is at most a million dollars in millionths, which a record's whole
            // number holds; one that did not fit would be refused and not cut.
            let whole = i64::try_from(micros).map_err(|_| SettingsRefused::Value {
                name: name.to_owned(),
            })?;
            pairs.insert(name.to_owned(), Value::Integer(whole));
        }
        if pairs.len() > SETTINGS_LIMIT {
            return Err(SettingsRefused::TooMany {
                limit: SETTINGS_LIMIT,
            });
        }
        pairs
            .into_iter()
            .map(|(name, value)| {
                let Some(recorded) = Recorded::new(&name, &value, secrets) else {
                    // Why a record takes none: the name holds a secret, which is
                    // looked for first and in the whole name; or the name is longer
                    // than a record's; or the value is a number it would write whole.
                    return Err(if secrets.redact(&name) != name {
                        SettingsRefused::HoldsSecret
                    } else if Recorded::new(&name, &Value::Boolean(true), secrets).is_none() {
                        SettingsRefused::Name { name: shown(&name) }
                    } else {
                        SettingsRefused::Value { name: shown(&name) }
                    });
                };
                let value = match recorded.value() {
                    ArgumentValue::Number(number) => Value::Number(*number),
                    ArgumentValue::Integer(number) => Value::Integer(*number),
                    ArgumentValue::Boolean(boolean) => Value::Boolean(*boolean),
                    ArgumentValue::Text(text) => Value::text(text),
                };
                Ok((name, value))
            })
            .collect()
    }
}

/// A config makes the settings of the runs admitted under a snapshot of it (ADR 0015,
/// "One snapshot of the config"). What [`Config::settings_of`] refuses is a failure
/// in that refusal's words, which name a setting and never a value.
impl Settings for Config {
    fn of(
        &self,
        resolved: &[(Capability, &str)],
        secrets: &Secrets,
    ) -> Result<Vec<(String, Value)>, NotJudged> {
        self.settings_of(resolved, secrets)
            .map_err(|refused| NotJudged::saying(&refused.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Config, Setting, SettingsRefused};
    use crate::config::{Backend, Offered};
    use crate::contract::{Capability, Secrets};

    /// What `settings::what_a_record_would_not_take_is_refused_naming_the_setting_and_never_its_value`
    /// held of a name too long for a record, until the config's reader refused such
    /// a config. No file gives this config any more, so it is made here by hand:
    /// were the reader's bound and the record's ever to part, the setting is still
    /// refused, shown cut, and not left out.
    #[test]
    fn a_name_longer_than_a_record_takes_is_refused_and_shown_cut_should_a_config_ever_hold_one() {
        let (backend, key) = ("b".repeat(64), "k".repeat(64));
        let settings = BTreeMap::from([
            (key, Setting::Integer(1)),
            ("short".to_owned(), Setting::Integer(2)),
        ]);
        let table = Backend {
            kind: "fake".to_owned(),
            model: None,
            provider: None,
            url: None,
            key_env: None,
            token_env: None,
            settings,
        };
        let offered = Offered {
            default: None,
            backends: BTreeMap::from([(backend.clone(), table)]),
        };
        let long = Config {
            backends: BTreeMap::from([("segment", offered)]),
            call_cap_micros: None,
            recipients: BTreeMap::new(),
        };
        let refused = long
            .settings_of(&[(Capability::Segment, backend.as_str())], &Secrets::none())
            .unwrap_err();
        let shown = format!("backends.segment.{}...", "b".repeat(64 - 17));
        assert_eq!(
            refused,
            SettingsRefused::Name {
                name: shown.clone()
            }
        );
        assert_eq!(
            refused.to_string(),
            format!("the setting {shown} has a name longer than a record takes")
        );
    }
}
