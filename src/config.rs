use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CliConfig {
    pub api_url: Option<String>,
    pub token: Option<String>,
    pub cache_dir: Option<String>,
}

impl CliConfig {
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
        Ok(home.join(".pullweights"))
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    #[allow(dead_code)]
    pub fn cache_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("cache"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path()?, content)?;
        Ok(())
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "api_url" => self.api_url = Some(value.to_string()),
            "token" => self.token = Some(value.to_string()),
            "cache_dir" => self.cache_dir = Some(value.to_string()),
            _ => anyhow::bail!("Unknown config key: {key}"),
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "api_url" => self.api_url.clone(),
            "token" => self.token.clone(),
            "cache_dir" => self.cache_dir.clone(),
            _ => None,
        }
    }

    pub fn api_url(&self) -> &str {
        self.api_url
            .as_deref()
            .unwrap_or("https://api.pullweights.com")
    }
}

impl fmt::Display for CliConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "api_url: {}", self.api_url())?;
        writeln!(
            f,
            "token: {}",
            if self.token.is_some() {
                "***"
            } else {
                "(not set)"
            }
        )?;
        writeln!(
            f,
            "cache_dir: {}",
            self.cache_dir.as_deref().unwrap_or("~/.pullweights/cache")
        )?;
        Ok(())
    }
}
