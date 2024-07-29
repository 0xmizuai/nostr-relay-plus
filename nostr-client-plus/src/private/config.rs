use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::path::Path;
use toml::Value;

#[derive(Deserialize)]
pub struct Config {
    pub whitelist: Vec<String>,
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
