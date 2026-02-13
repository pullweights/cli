use anyhow::{bail, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};

/// Parsed model reference: org, model name, optional tag.
#[derive(Debug, Clone)]
pub struct ModelRef {
    pub org: String,
    pub model: String,
    pub tag: Option<String>,
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.org, self.model)?;
        if let Some(tag) = &self.tag {
            write!(f, ":{tag}")?;
        }
        Ok(())
    }
}

/// Parse "org/model:tag" or "org/model" format.
pub fn parse_model_ref(model_ref: &str) -> Result<ModelRef> {
    let (path, tag) = if let Some((path, tag)) = model_ref.split_once(':') {
        (path, Some(tag.to_string()))
    } else {
        (model_ref, None)
    };

    let (org, model) = path.split_once('/').ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid model reference '{model_ref}'. Expected format: org/model or org/model:tag"
        )
    })?;

    if org.is_empty() || model.is_empty() {
        bail!("Invalid model reference '{model_ref}'. Org and model name cannot be empty");
    }

    Ok(ModelRef {
        org: org.to_string(),
        model: model.to_string(),
        tag,
    })
}

/// Build an HTTP client with optional Bearer token in default headers.
pub fn api_client(token: Option<&str>) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("pullweights-cli"));

    if let Some(token) = token {
        let val = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| anyhow::anyhow!("Invalid token format"))?;
        headers.insert(AUTHORIZATION, val);
    }

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;

    Ok(client)
}

/// Require a token from env var, config, or bail with a helpful message.
pub fn require_token(config_token: Option<&str>) -> Result<String> {
    // PULLWEIGHTS_TOKEN env var takes priority (for CI/CD)
    if let Ok(env_token) = std::env::var("PULLWEIGHTS_TOKEN") {
        if !env_token.is_empty() {
            return Ok(env_token);
        }
    }
    config_token.map(|t| t.to_string()).ok_or_else(|| {
        anyhow::anyhow!(
            "Not logged in. Run `pullweights login` or `pullweights auth --token pw_...` first.\n\
             You can also set the PULLWEIGHTS_TOKEN environment variable."
        )
    })
}

/// Format byte count as human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_model_ref_with_tag() {
        let r = parse_model_ref("meta/llama-7b:v1.0").unwrap();
        assert_eq!(r.org, "meta");
        assert_eq!(r.model, "llama-7b");
        assert_eq!(r.tag.as_deref(), Some("v1.0"));
    }

    #[test]
    fn test_parse_model_ref_without_tag() {
        let r = parse_model_ref("openai/whisper").unwrap();
        assert_eq!(r.org, "openai");
        assert_eq!(r.model, "whisper");
        assert!(r.tag.is_none());
    }

    #[test]
    fn test_parse_model_ref_invalid() {
        assert!(parse_model_ref("no-slash").is_err());
        assert!(parse_model_ref("/model").is_err());
        assert!(parse_model_ref("org/").is_err());
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
    }
}
