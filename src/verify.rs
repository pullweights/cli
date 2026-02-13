use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::checksum::sha256_reader;
use crate::manifest::PullWeightsManifest;
use crate::utils::{api_client, format_bytes, parse_model_ref};

pub async fn verify(api_url: &str, token: &str, model_ref: &str, dir: &str) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;
    let tag = parsed
        .tag
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Tag is required for verify. Use org/model:tag format"))?;

    let client = api_client(Some(token))?;
    let url = format!(
        "{api_url}/v1/models/{}/{}/manifests/{tag}",
        parsed.org, parsed.model
    );

    println!(
        "Fetching manifest for {}/{}:{tag}...",
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
        bail!("Failed to fetch manifest ({status}): {body}");
    }

    let manifest: PullWeightsManifest = resp.json().await.context("Invalid manifest response")?;

    let base_dir = Path::new(dir);
    let mut pass = 0u32;
    let mut fail = 0u32;
    let mut missing = 0u32;

    println!(
        "\nVerifying {} files in {}:\n",
        manifest.files.len(),
        base_dir.display()
    );

    for file in &manifest.files {
        let file_path = base_dir.join(&file.filename);

        if !file_path.exists() {
            println!("  ? {:<40} MISSING", file.filename);
            missing += 1;
            continue;
        }

        let f = std::fs::File::open(&file_path)
            .with_context(|| format!("Failed to open {}", file_path.display()))?;
        let computed =
            sha256_reader(f).with_context(|| format!("Failed to hash {}", file_path.display()))?;

        if computed == file.sha256 {
            println!(
                "  OK {:<40} {} ({})",
                file.filename,
                &file.sha256[..12],
                format_bytes(file.size_bytes)
            );
            pass += 1;
        } else {
            println!(
                "  FAIL {:<38} expected {} got {}",
                file.filename,
                &file.sha256[..12],
                &computed[..12]
            );
            fail += 1;
        }
    }

    println!();
    println!(
        "Results: {} passed, {} failed, {} missing",
        pass, fail, missing
    );

    if fail > 0 || missing > 0 {
        bail!("Verification failed");
    }

    println!("All files verified successfully.");
    Ok(())
}
