use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::utils::api_client;

#[derive(Deserialize)]
struct KeyListItem {
    id: Uuid,
    name: String,
    prefix: String,
    scopes: Vec<String>,
    allowed_orgs: Option<Vec<String>>,
    allowed_models: Option<Vec<String>>,
    allowed_ips: Option<Vec<String>>,
    last_used_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct CreateKeyRequest {
    name: String,
    scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_orgs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_models: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_ips: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct CreateKeyResponse {
    id: Uuid,
    key: String,
    prefix: String,
    name: String,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
struct MessageResponse {
    message: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: String,
}

pub async fn list(api_url: &str, token: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let resp = client.get(format!("{api_url}/v1/api-keys")).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
            bail!("API error ({}): {}", status, err.error.message);
        }
        bail!("API error ({}): {}", status, body);
    }

    let keys: Vec<KeyListItem> = resp.json().await?;

    if keys.is_empty() {
        println!("No API keys found. Create one with: pullweights api-key create --name \"My Key\" --scopes model:read");
        return Ok(());
    }

    println!(
        "{:<38} {:<20} {:<14} {:<30} CREATED",
        "ID", "NAME", "PREFIX", "SCOPES"
    );
    println!("{}", "-".repeat(110));

    for key in &keys {
        let scopes = key.scopes.join(", ");
        let created = key.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
        println!(
            "{:<38} {:<20} {:<14} {:<30} {}",
            key.id,
            key.name,
            format!("{}...", key.prefix),
            scopes,
            created
        );
    }

    // Print restriction details for keys that have them
    for key in &keys {
        let mut restrictions = Vec::new();
        if let Some(ref orgs) = key.allowed_orgs {
            if !orgs.is_empty() {
                restrictions.push(format!("orgs: {}", orgs.join(", ")));
            }
        }
        if let Some(ref models) = key.allowed_models {
            if !models.is_empty() {
                restrictions.push(format!("models: {}", models.join(", ")));
            }
        }
        if let Some(ref ips) = key.allowed_ips {
            if !ips.is_empty() {
                restrictions.push(format!("ips: {}", ips.join(", ")));
            }
        }
        if let Some(expires) = key.expires_at {
            restrictions.push(format!("expires: {}", expires.format("%Y-%m-%d %H:%M UTC")));
        }
        if let Some(last) = key.last_used_at {
            restrictions.push(format!("last used: {}", last.format("%Y-%m-%d %H:%M UTC")));
        }
        if !restrictions.is_empty() {
            println!("  {} [{}]", key.name, restrictions.join(" | "));
        }
    }

    Ok(())
}

fn parse_comma_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_expires(value: &str) -> Result<DateTime<Utc>> {
    // Try ISO 8601 first (e.g. 2026-12-31T00:00:00Z)
    if let Ok(dt) = value.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    // Try plain date (e.g. 2026-12-31)
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let dt = date
            .and_hms_opt(23, 59, 59)
            .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
        return Ok(DateTime::from_naive_utc_and_offset(dt, Utc));
    }
    bail!("Invalid expiration date '{value}'. Use ISO 8601 (2026-12-31T00:00:00Z) or YYYY-MM-DD format.");
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    api_url: &str,
    token: &str,
    name: &str,
    scopes: &str,
    orgs: Option<&str>,
    models: Option<&str>,
    ips: Option<&str>,
    expires: Option<&str>,
) -> Result<()> {
    let scope_list = parse_comma_list(scopes);
    if scope_list.is_empty() {
        bail!("At least one scope is required. Example: model:read,model:push");
    }

    let expires_at = match expires {
        Some(v) => Some(parse_expires(v)?),
        None => None,
    };

    let body = CreateKeyRequest {
        name: name.to_string(),
        scopes: scope_list,
        allowed_orgs: orgs.map(parse_comma_list).filter(|v| !v.is_empty()),
        allowed_models: models.map(parse_comma_list).filter(|v| !v.is_empty()),
        allowed_ips: ips.map(parse_comma_list).filter(|v| !v.is_empty()),
        expires_at,
    };

    let client = api_client(Some(token))?;
    let resp = client
        .post(format!("{api_url}/v1/api-keys"))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(&text) {
            bail!("API error ({}): {}", status, err.error.message);
        }
        bail!("API error ({}): {}", status, text);
    }

    let result: CreateKeyResponse = resp.json().await?;

    println!("API key created successfully!\n");
    println!("  Name:   {}", result.name);
    println!("  ID:     {}", result.id);
    println!("  Prefix: {}", result.prefix);
    println!("  Scopes: {}", result.scopes.join(", "));
    println!("\n  Key: {}\n", result.key);
    println!("Save this key now — it will not be shown again.");

    Ok(())
}

pub async fn revoke(api_url: &str, token: &str, id: &str) -> Result<()> {
    // Validate UUID format
    let _uuid: Uuid = id
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid key ID '{id}'. Expected a UUID."))?;

    let client = api_client(Some(token))?;
    let resp = client
        .delete(format!("{api_url}/v1/api-keys/{id}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(&text) {
            bail!("API error ({}): {}", status, err.error.message);
        }
        bail!("API error ({}): {}", status, text);
    }

    let result: MessageResponse = resp.json().await?;
    println!("{}", result.message);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_parse_comma_list_basic() {
        let result = parse_comma_list("a,b,c");
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_comma_list_with_spaces() {
        let result = parse_comma_list("a , b , c");
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_comma_list_empty_segments() {
        let result = parse_comma_list("a,,b,,,c");
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_comma_list_single() {
        let result = parse_comma_list("model:read");
        assert_eq!(result, vec!["model:read"]);
    }

    #[test]
    fn test_parse_comma_list_empty_string() {
        let result = parse_comma_list("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_expires_iso8601() {
        let dt = parse_expires("2026-12-31T00:00:00Z").unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 12);
        assert_eq!(dt.day(), 31);
    }

    #[test]
    fn test_parse_expires_date_only() {
        let dt = parse_expires("2026-12-31").unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 12);
        assert_eq!(dt.day(), 31);
        // Date-only should be end of day
        assert_eq!(dt.hour(), 23);
        assert_eq!(dt.minute(), 59);
        assert_eq!(dt.second(), 59);
    }

    #[test]
    fn test_parse_expires_invalid() {
        assert!(parse_expires("not-a-date").is_err());
        assert!(parse_expires("").is_err());
    }
}
