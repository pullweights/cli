use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::utils::{api_client, format_bytes, parse_model_ref};

#[derive(Deserialize)]
struct TagInfo {
    tag: String,
    total_size_bytes: u64,
    created_at: String,
}

pub async fn list_tags(api_url: &str, token: &str, model_ref: &str) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;

    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/models/{}/{}/tags", parsed.org, parsed.model);

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Failed to list tags ({status}): {body}");
    }

    let tags: Vec<TagInfo> = resp.json().await.context("Invalid tags response")?;

    if tags.is_empty() {
        println!("No tags found for {}/{}", parsed.org, parsed.model);
        return Ok(());
    }

    println!("Tags for {}/{}:\n", parsed.org, parsed.model);
    println!("{:<20} {:>12} CREATED", "TAG", "SIZE");
    println!("{}", "-".repeat(55));

    for tag in &tags {
        println!(
            "{:<20} {:>12} {}",
            tag.tag,
            format_bytes(tag.total_size_bytes),
            tag.created_at,
        );
    }

    Ok(())
}
