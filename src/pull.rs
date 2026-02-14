use anyhow::{bail, Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::path::{Component, Path};
use tokio::io::AsyncWriteExt;

use crate::checksum::StreamingHasher;
use crate::utils::{api_client, format_bytes, parse_model_ref, sanitize_error};

#[derive(Deserialize)]
struct PullResponse {
    files: Vec<FileDownload>,
    total_size_bytes: u64,
}

#[derive(Deserialize)]
struct FileDownload {
    filename: String,
    download_url: String,
    size_bytes: u64,
    sha256: String,
}

pub async fn pull(api_url: &str, token: &str, model_ref: &str, output: &str) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;
    let tag = parsed
        .tag
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Tag is required for pull. Use org/model:tag format"))?;

    // Create output directory
    let out_dir = Path::new(output);
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create output directory: {output}"))?;

    let client = api_client(Some(token))?;
    let url = format!(
        "{api_url}/v1/models/{}/{}/pull/{tag}",
        parsed.org, parsed.model
    );

    println!("Pulling {}/{}:{tag}...", parsed.org, parsed.model);

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Pull failed ({status}): {}", sanitize_error(&body));
    }

    let pull_resp: PullResponse = resp.json().await.context("Invalid pull response")?;

    println!(
        "Downloading {} files ({})",
        pull_resp.files.len(),
        format_bytes(pull_resp.total_size_bytes)
    );

    let multi_progress = MultiProgress::new();
    let sty = ProgressStyle::with_template(
        "  {prefix:.bold} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap()
    .progress_chars("##-");

    let download_client = reqwest::Client::new();
    let mut checksum_errors: Vec<String> = Vec::new();

    for download in &pull_resp.files {
        // Block path traversal: reject filenames with parent components
        if Path::new(&download.filename)
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            bail!(
                "Refusing to download '{}': path traversal detected",
                download.filename
            );
        }

        let pb = multi_progress.add(ProgressBar::new(download.size_bytes));
        pb.set_style(sty.clone());
        pb.set_prefix(download.filename.clone());

        let file_path = out_dir.join(&download.filename);

        // Download with streaming
        let resp = download_client
            .get(&download.download_url)
            .send()
            .await
            .with_context(|| format!("Failed to download {}", download.filename))?;

        if !resp.status().is_success() {
            let status = resp.status();
            bail!("Failed to download {} ({status})", download.filename);
        }

        let bytes = resp.bytes().await?;
        let mut hasher = StreamingHasher::new();
        hasher.update(&bytes);

        let mut file = tokio::fs::File::create(&file_path)
            .await
            .with_context(|| format!("Failed to create file: {}", file_path.display()))?;
        file.write_all(&bytes).await?;
        file.flush().await?;

        pb.set_position(download.size_bytes);
        pb.finish();

        // Verify checksum
        let computed = hasher.finalize();
        if computed != download.sha256 {
            checksum_errors.push(format!(
                "{}: expected {}, got {computed}",
                download.filename, download.sha256
            ));
        }
    }

    println!();
    if checksum_errors.is_empty() {
        println!(
            "Pull complete! {} files downloaded to {}",
            pull_resp.files.len(),
            out_dir.display()
        );
        println!("All checksums verified.");
    } else {
        println!("Pull complete with checksum errors:");
        for err in &checksum_errors {
            println!("  MISMATCH: {err}");
        }
        bail!(
            "{} file(s) failed checksum verification",
            checksum_errors.len()
        );
    }

    Ok(())
}
