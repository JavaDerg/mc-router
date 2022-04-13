use std::collections::HashMap;
use dashmap::DashMap;

pub struct Config {
    pub path: String,
    pub map: DashMap<String, String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config error: {0}")]
    InvalidConfig(#[from] serde_json::Error),
}

impl Config {
    pub async fn new(path: String) -> Result<Self, ConfigError> {
        let cfg = tokio::fs::read_to_string(&path).await?;
        let cfg: HashMap<String, String> = serde_json::from_str(&cfg)?;

        Ok(Self {
            path,
            map: DashMap::from_iter(cfg.into_iter()),
        })
    }
}
