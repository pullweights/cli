use anyhow::{Context, Result};
use serde::Deserialize;

use crate::utils::{api_client, sanitize_error};

#[derive(Deserialize)]
struct ModelItem {
    name: String,
    description: Option<String>,
    visibility: String,
    tags: Vec<String>,
    download_count: i64,
}

#[derive(Deserialize)]
struct OrgItem {
    name: String,
}

/// List models for an org, or list all orgs if no argument given.
pub async fn ls(api_url: &str, token: &str, org: Option<&str>) -> Result<()> {
    match org {
        Some(org) => list_models(api_url, token, org).await,
        None => list_orgs(api_url, token).await,
    }
}

async fn list_orgs(api_url: &str, token: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let resp = client
        .get(format!("{api_url}/v1/orgs"))
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to list orgs ({status}): {}", sanitize_error(&body));
    }

    let orgs: Vec<OrgItem> = resp.json().await.context("Invalid response")?;

    if orgs.is_empty() {
        println!("No organizations found.");
        return Ok(());
    }

    println!("Your organizations:");
    for org in &orgs {
        println!("  {}", org.name);
    }
    println!("\nRun `pullweights ls <org>` to list models in an organization.");
    Ok(())
}

async fn list_models(api_url: &str, token: &str, org: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let resp = client
        .get(format!("{api_url}/v1/models/{org}"))
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Failed to list models ({status}): {}",
            sanitize_error(&body)
        );
    }

    let models: Vec<ModelItem> = resp.json().await.context("Invalid response")?;

    if models.is_empty() {
        println!("No models found in '{org}'.");
        return Ok(());
    }

    // Header
    println!(
        "{:<30} {:<10} {:<8} {:<10} DESCRIPTION",
        "MODEL", "VISIBILITY", "TAGS", "PULLS"
    );

    for m in &models {
        let desc = m
            .description
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(40)
            .collect::<String>();
        println!(
            "{:<30} {:<10} {:<8} {:<10} {}",
            format!("{org}/{}", m.name),
            m.visibility,
            m.tags.len(),
            m.download_count,
            desc,
        );
    }

    Ok(())
}
