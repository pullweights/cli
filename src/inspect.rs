use anyhow::{bail, Context, Result};

use crate::manifest::PullWeightsManifest;
use crate::utils::{api_client, parse_model_ref};

pub async fn inspect(api_url: &str, token: &str, model_ref: &str) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;
    let tag = parsed
        .tag
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Tag is required for inspect. Use org/model:tag format"))?;

    let client = api_client(Some(token))?;
    let url = format!(
        "{api_url}/v1/models/{}/{}/manifests/{tag}",
        parsed.org, parsed.model
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Inspect failed ({status}): {body}");
    }

    let manifest: PullWeightsManifest = resp.json().await.context("Invalid manifest response")?;

    let json = serde_json::to_string_pretty(&manifest).context("Failed to serialize manifest")?;
    println!("{json}");

    Ok(())
}
