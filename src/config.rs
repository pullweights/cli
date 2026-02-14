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
        Self::load_from(&path)
    }

    pub fn load_from(path: &PathBuf) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let dir = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid config path"))?;
        std::fs::create_dir_all(dir)?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms)?;
        }
        Ok(())
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "api_url" => {
                // Enforce HTTPS unless targeting localhost for development
                if value.starts_with("http://") {
                    let host = value
                        .trim_start_matches("http://")
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .split(':')
                        .next()
                        .unwrap_or("");
                    if host != "localhost" && host != "127.0.0.1" && host != "::1" {
                        anyhow::bail!(
                            "Refusing plaintext HTTP for non-localhost URL. Use https:// instead."
                        );
                    }
                }
                self.api_url = Some(value.to_string());
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = CliConfig::default();
        assert!(cfg.api_url.is_none());
        assert!(cfg.token.is_none());
        assert!(cfg.cache_dir.is_none());
    }

    #[test]
    fn test_api_url_default() {
        let cfg = CliConfig::default();
        assert_eq!(cfg.api_url(), "https://api.pullweights.com");
    }

    #[test]
    fn test_api_url_custom() {
        let cfg = CliConfig {
            api_url: Some("http://localhost:3000".to_string()),
            ..Default::default()
        };
        assert_eq!(cfg.api_url(), "http://localhost:3000");
    }

    #[test]
    fn test_set_get_known_keys() {
        let mut cfg = CliConfig::default();
        cfg.set("api_url", "http://localhost").unwrap();
        cfg.set("token", "tok123").unwrap();
        cfg.set("cache_dir", "/tmp/cache").unwrap();
        assert_eq!(cfg.get("api_url").as_deref(), Some("http://localhost"));
        assert_eq!(cfg.get("token").as_deref(), Some("tok123"));
        assert_eq!(cfg.get("cache_dir").as_deref(), Some("/tmp/cache"));
    }

    #[test]
    fn test_set_api_url_https_allowed() {
        let mut cfg = CliConfig::default();
        cfg.set("api_url", "https://api.pullweights.com").unwrap();
        assert_eq!(
            cfg.get("api_url").as_deref(),
            Some("https://api.pullweights.com")
        );
    }

    #[test]
    fn test_set_api_url_http_localhost_allowed() {
        let mut cfg = CliConfig::default();
        cfg.set("api_url", "http://localhost:8080").unwrap();
        assert_eq!(cfg.get("api_url").as_deref(), Some("http://localhost:8080"));
        let mut cfg2 = CliConfig::default();
        cfg2.set("api_url", "http://127.0.0.1:8080").unwrap();
        assert_eq!(
            cfg2.get("api_url").as_deref(),
            Some("http://127.0.0.1:8080")
        );
    }

    #[test]
    fn test_set_api_url_http_remote_rejected() {
        let mut cfg = CliConfig::default();
        let result = cfg.set("api_url", "http://evil.com");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Refusing plaintext HTTP"));
    }

    #[test]
    fn test_set_unknown_key() {
        let mut cfg = CliConfig::default();
        assert!(cfg.set("nonexistent", "val").is_err());
    }

    #[test]
    fn test_get_unknown_key() {
        let cfg = CliConfig::default();
        assert!(cfg.get("nonexistent").is_none());
    }

    #[test]
    fn test_display_with_token() {
        let cfg = CliConfig {
            token: Some("secret".to_string()),
            ..Default::default()
        };
        let display = format!("{cfg}");
        assert!(display.contains("***"));
        assert!(!display.contains("secret"));
    }

    #[test]
    fn test_display_without_token() {
        let cfg = CliConfig::default();
        let display = format!("{cfg}");
        assert!(display.contains("(not set)"));
    }

    #[test]
    fn test_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let cfg = CliConfig {
            api_url: Some("http://test.com".to_string()),
            token: Some("tok".to_string()),
            cache_dir: Some("/tmp/cache".to_string()),
        };
        cfg.save_to(&path).unwrap();

        let loaded = CliConfig::load_from(&path).unwrap();
        assert_eq!(loaded.api_url.as_deref(), Some("http://test.com"));
        assert_eq!(loaded.token.as_deref(), Some("tok"));
        assert_eq!(loaded.cache_dir.as_deref(), Some("/tmp/cache"));
    }

    #[test]
    fn test_load_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let cfg = CliConfig::load_from(&path).unwrap();
        assert!(cfg.api_url.is_none());
        assert!(cfg.token.is_none());
    }

    #[test]
    fn test_load_malformed_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "not valid { toml [[[").unwrap();
        assert!(CliConfig::load_from(&path).is_err());
    }
}
