use anyhow::{bail, Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::checksum::sha256_hex;
use crate::utils::{api_client, format_bytes, parse_model_ref, sanitize_error};

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PushInitRequest {
    tag: String,
    visibility: Option<String>,
    files: Vec<FileEntry>,
}

#[derive(Serialize)]
struct FileEntry {
    filename: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct PushInitResponse {
    push_id: String,
    uploads: Vec<UploadTarget>,
}

#[derive(Deserialize)]
struct UploadTarget {
    filename: String,
    upload_url: String,
}

#[derive(Serialize)]
struct PushFinalizeRequest {
    push_id: String,
    tag: String,
}

#[derive(Deserialize)]
struct PushFinalizeResponse {
    tag: String,
    total_size_bytes: u64,
    sha256_digest: String,
    files: Vec<FinalizedFile>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct FinalizedFile {
    filename: String,
    size_bytes: u64,
    sha256: String,
}

// ---------------------------------------------------------------------------
// Push implementation
// ---------------------------------------------------------------------------

pub async fn push(
    api_url: &str,
    token: &str,
    model_ref: &str,
    files: &[String],
    visibility: &str,
) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;
    let tag = parsed
        .tag
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Tag is required for push. Use org/model:tag format"))?;

    // Validate files and compute checksums
    let mut file_entries: Vec<(String, String, u64, Vec<u8>)> = Vec::new(); // (filename, sha256, size, data)

    for file_path in files {
        let path = Path::new(file_path);
        if !path.exists() {
            bail!("File not found: {file_path}");
        }
        if !path.is_file() {
            bail!("Not a file: {file_path}");
        }
        let data =
            std::fs::read(path).with_context(|| format!("Failed to read file: {file_path}"))?;
        let sha = sha256_hex(&data);
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.clone());
        let size = data.len() as u64;
        file_entries.push((filename, sha, size, data));
    }

    let total_size: u64 = file_entries.iter().map(|(_, _, s, _)| s).sum();
    let vis_label = if visibility == "private" {
        " (private)"
    } else {
        ""
    };
    println!(
        "Pushing {}/{}{} ({} files, {})",
        parsed.org,
        parsed.model,
        vis_label,
        files.len(),
        format_bytes(total_size)
    );

    let client = api_client(Some(token))?;

    // Phase 1: Init push — send metadata, get presigned upload URLs
    let init_req = PushInitRequest {
        tag: tag.to_string(),
        visibility: Some(visibility.to_string()),
        files: file_entries
            .iter()
            .map(|(filename, sha, size, _)| FileEntry {
                filename: filename.clone(),
                sha256: sha.clone(),
                size_bytes: *size,
            })
            .collect(),
    };

    let init_url = format!(
        "{api_url}/v1/models/{}/{}/push/init",
        parsed.org, parsed.model
    );

    let resp = client
        .post(&init_url)
        .json(&init_req)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Push init failed ({status}): {}", sanitize_error(&body));
    }

    let init_resp: PushInitResponse = resp.json().await.context("Invalid push init response")?;

    // Phase 2: Upload files directly to S3 using presigned URLs
    let multi_progress = MultiProgress::new();
    let sty = ProgressStyle::with_template(
        "  {prefix:.bold} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap()
    .progress_chars("##-");

    let upload_client = reqwest::Client::new();

    for upload in &init_resp.uploads {
        let (_, _, size, data) = file_entries
            .iter()
            .find(|(f, _, _, _)| f == &upload.filename)
            .ok_or_else(|| {
                anyhow::anyhow!("Server returned unknown filename: {}", upload.filename)
            })?;

        let pb = multi_progress.add(ProgressBar::new(*size));
        pb.set_style(sty.clone());
        pb.set_prefix(upload.filename.clone());

        let resp = upload_client
            .put(&upload.upload_url)
            .header("content-type", "application/octet-stream")
            .body(data.clone())
            .send()
            .await
            .with_context(|| format!("Failed to upload {}", upload.filename))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "S3 upload failed for {} ({status}): {}",
                upload.filename,
                sanitize_error(&body)
            );
        }

        pb.set_position(*size);
        pb.finish();
    }

    // Phase 3: Finalize push — confirm uploads, record usage
    let finalize_url = format!(
        "{api_url}/v1/models/{}/{}/push/finalize",
        parsed.org, parsed.model
    );

    let finalize_req = PushFinalizeRequest {
        push_id: init_resp.push_id,
        tag: tag.to_string(),
    };

    let resp = client
        .post(&finalize_url)
        .json(&finalize_req)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Push finalize failed ({status}): {}", sanitize_error(&body));
    }

    let finalize_resp: PushFinalizeResponse = resp
        .json()
        .await
        .context("Invalid push finalize response")?;

    println!("\nPush successful!");
    println!("  Model:   {}/{}", parsed.org, parsed.model);
    println!("  Tag:     {}", finalize_resp.tag);
    println!("  Files:   {}", finalize_resp.files.len());
    println!(
        "  Size:    {}",
        format_bytes(finalize_resp.total_size_bytes)
    );
    println!("  Digest:  {}", &finalize_resp.sha256_digest[..12]);

    Ok(())
}
