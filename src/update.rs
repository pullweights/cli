use anyhow::{bail, Context, Result};

use crate::utils::{api_client, parse_model_ref, sanitize_error};

pub async fn run(api_url: &str, token: &str, model_ref: &str, description: &str) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;

    // If description starts with @, read from file
    let description = if let Some(path) = description.strip_prefix('@') {
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read description file: {path}"))?
    } else {
        description.to_string()
    };

    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/models/{}/{}", parsed.org, parsed.model);

    let resp = client
        .patch(&url)
        .json(&serde_json::json!({ "description": description }))
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "Failed to update {}/{} ({status}): {}",
            parsed.org,
            parsed.model,
            sanitize_error(&body)
        );
    }

    println!("Updated {}/{}", parsed.org, parsed.model);
    Ok(())
}
