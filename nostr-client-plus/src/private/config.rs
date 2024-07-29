use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
pub struct Config {
    pub whitelist: Vec<String>,
}

pub fn get_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    use std::fs;

    let content = fs::read_to_string(path)?;
    let config: Config = toml::from_str::<toml::Value>(&content)
        .expect("error parsing content")
        .get("assigner")
        .expect("no assigner section")
        .clone()
        .try_into()?;
    Ok(config)
}
