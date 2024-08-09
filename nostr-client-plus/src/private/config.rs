use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::Path;
use toml::Value;

#[derive(Deserialize)]
pub struct AssignerConfig {
    pub metrics_socket_addr: String,
    pub whitelist: Vec<String>,
}

#[derive(Deserialize)]
pub struct AggregatorConfig {
    pub metrics_socket_addr: String,
    pub assigner_whitelist: Vec<String>,
}

pub fn load_config<P: AsRef<Path>>(path: P, section: &str) -> Result<Value> {
    use std::fs;

    let content = fs::read_to_string(path)?;
    let config: Value = toml::from_str::<Value>(&content)?
        .get(section)
        .ok_or_else(|| anyhow!("section not found"))?
        .clone();
    Ok(config)
}
