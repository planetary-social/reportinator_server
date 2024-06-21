use crate::config::Configurable;
use nostr_sdk::Keys;
use serde::{de, Deserialize, Deserializer};
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "parse_keys")]
    pub keys: Keys,
}

impl Configurable for Config {
    fn key() -> &'static str {
        "reportinator"
    }
}

fn parse_keys<'de, D>(deserializer: D) -> Result<Keys, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Keys::parse(s).map_err(de::Error::custom)
}

/*
 * This is hopefully temporary. Generally its better to provide config
 * via dependency injection, instead of having global state. Based on
 * the current architecture though, there were a couple places where
 * it was non-trivial to pass configuration to.
 */
static CONFIG: OnceLock<Config> = OnceLock::new();

/// This will panic if config was not set.
pub fn config<'a>() -> &'a Config {
    CONFIG.get().unwrap()
}

pub fn set_config(config: Config) {
    CONFIG.set(config).expect("Failed to set  config");
}
