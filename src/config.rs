use anyhow::{Context, Result};
use config_rs::{Config as ConfigTree, Environment, File};
use serde::de::DeserializeOwned;
use std::{any::type_name, env};

/*
 * Constants
 */

pub const ENVIRONMENT_PREFIX: &str = "APP";
pub const CONFIG_SEPARATOR: &str = "__";

#[must_use]
pub fn environment() -> String {
    env::var(format!("{ENVIRONMENT_PREFIX}{CONFIG_SEPARATOR}ENVIRONMENT"))
        .unwrap_or_else(|_| "development".into())
}

/*
 * Configuration
 */

pub trait Configurable {
    fn key() -> &'static str;
}

#[derive(Debug, Clone)]
pub struct Config {
    config: ConfigTree,
}

impl Config {
    pub fn new(config_dir: &str) -> Result<Self> {
        let environment = environment();

        let default_config_path = format!("{}/settings", &config_dir);
        let env_config_path = format!("{}/settings.{}", &config_dir, &environment);
        let local_config_path = format!("{}/settings.local", &config_dir);

        ConfigTree::builder()
            .add_source(File::with_name(&default_config_path))
            .add_source(File::with_name(&env_config_path).required(false))
            .add_source(File::with_name(&local_config_path).required(false))
            .add_source(Environment::with_prefix(ENVIRONMENT_PREFIX).separator(CONFIG_SEPARATOR))
            .build()
            .map(|c| Config { config: c })
            .map_err(Into::into)
    }

    pub fn get<T>(&self) -> Result<T>
    where
        T: Configurable,
        T: DeserializeOwned,
    {
        self.config.get::<T>(T::key()).context(format!(
            "Error loading configuration for `{}` at `{}`",
            type_name::<T>(),
            T::key(),
        ))
    }

    pub fn get_by_key<T>(&self, key: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.config.get::<T>(key).context(format!(
            "Error loading configuration for `{}` at `{key}`",
            type_name::<T>(),
        ))
    }
}
