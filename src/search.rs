use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::utils::{api_client, sanitize_error};

#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
    total: u64,
}

fn default_model_type() -> String {
    "model".to_string()
}

#[derive(Deserialize)]
struct SearchResult {
    org_name: String,
    name: String,
    description: Option<String>,
    download_count: u64,
    #[serde(rename = "type", default = "default_model_type")]
    model_type: String,
}

pub async fn search(
    api_url: &str,
    token: Option<&str>,
    query: &str,
    limit: u32,
    model_type: Option<&str>,
) -> Result<()> {
    let client = api_client(token)?;
    let url = format!("{api_url}/v1/search");

    let mut params = vec![("q", query.to_string()), ("per_page", limit.to_string())];
    if let Some(t) = model_type {
        params.push(("type", t.to_string()));
    }

    let resp = client
        .get(&url)
        .query(&params)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Search failed ({status}): {}", sanitize_error(&body));
    }

    let search_resp: SearchResponse = resp.json().await.context("Invalid search response")?;

    if search_resp.results.is_empty() {
        println!("No models found for '{query}'");
        return Ok(());
    }

    println!("Found {} result(s) for '{query}':\n", search_resp.total);

    for result in &search_resp.results {
        let desc = result.description.as_deref().unwrap_or("");
        let badge = if result.model_type == "dataset" {
            " [dataset]"
        } else {
            ""
        };

        println!(
            "  {}/{}{}  {} downloads",
            result.org_name, result.name, badge, result.download_count
        );
        if !desc.is_empty() {
            println!("    {desc}");
        }
    }

    Ok(())
}
