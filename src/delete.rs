use anyhow::{bail, Context, Result};

use crate::utils::{api_client, parse_model_ref, sanitize_error};

pub async fn delete(api_url: &str, token: &str, model_ref: &str) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;

    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/models/{}/{}", parsed.org, parsed.model);

    let resp = client
        .delete(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "Failed to delete {}/{} ({status}): {}",
            parsed.org,
            parsed.model,
            sanitize_error(&body)
        );
    }

    println!("Deleted {}/{}", parsed.org, parsed.model);
    Ok(())
}
