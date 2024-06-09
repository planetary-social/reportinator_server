use crate::config::Configurable;
use nostr_sdk::Keys;
use serde::{de, Deserialize, Deserializer};

#[derive(Deserialize)]
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
