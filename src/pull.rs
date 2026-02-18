use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::path::{Component, Path};
use tokio::io::AsyncWriteExt;

use crate::checksum::StreamingHasher;
use crate::utils::{api_client, format_bytes, parse_model_ref, sanitize_error};

// ---------------------------------------------------------------------------
// Regular pull types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Chunked pull types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChunkedPullResponse {
    files: Vec<ChunkedPullFile>,
    total_size_bytes: u64,
}

#[derive(Deserialize)]
struct ChunkedPullFile {
    filename: String,
    sha256: String,
    size_bytes: u64,
    chunked: bool,
    #[serde(default)]
    chunks: Option<Vec<ChunkDownload>>,
    download_url: Option<String>,
}

#[derive(Deserialize)]
struct ChunkDownload {
    chunk_index: i32,
    sha256: String,
    size_bytes: u64,
    download_url: String,
}

// ---------------------------------------------------------------------------
// Pull implementation
// ---------------------------------------------------------------------------

pub async fn pull(api_url: &str, token: &str, model_ref: &str, output: &str) -> Result<()> {
    let parsed = parse_model_ref(model_ref)?;
    let tag = parsed
        .tag
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Tag is required for pull. Use org/model:tag format"))?;

    let out_dir = Path::new(output);
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create output directory: {output}"))?;

    let client = api_client(Some(token))?;

    println!("Pulling {}/{}:{tag}...", parsed.org, parsed.model);

    // Try chunked pull first
    let chunked_url = format!(
        "{api_url}/v1/models/{}/{}/pull/{tag}/chunked",
        parsed.org, parsed.model
    );

    let resp = client
        .get(&chunked_url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if resp.status().is_success() {
        let chunked_resp: ChunkedPullResponse =
            resp.json().await.context("Invalid chunked pull response")?;

        // Only proceed with chunked pull if there are actually chunked files
        let has_chunked = chunked_resp.files.iter().any(|f| f.chunked);
        if has_chunked {
            return pull_chunked(&chunked_resp, out_dir, &parsed, tag).await;
        }
    }

    // Fall back to regular pull (either server doesn't support chunked, or no chunked files)
    let url = format!(
        "{api_url}/v1/models/{}/{}/pull/{tag}",
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

        let resp = download_client
            .get(&download.download_url)
            .send()
            .await
            .with_context(|| format!("Failed to download {}", download.filename))?;

        if !resp.status().is_success() {
            let status = resp.status();
            bail!("Failed to download {} ({status})", download.filename);
        }

        let mut file = tokio::fs::File::create(&file_path)
            .await
            .with_context(|| format!("Failed to create file: {}", file_path.display()))?;

        let mut hasher = StreamingHasher::new();
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.with_context(|| format!("Download interrupted: {}", download.filename))?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }

        file.flush().await?;
        pb.finish();

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

/// Pull files using the chunked endpoint. Downloads chunks in parallel and reassembles.
async fn pull_chunked(
    resp: &ChunkedPullResponse,
    out_dir: &Path,
    parsed: &crate::utils::ModelRef,
    tag: &str,
) -> Result<()> {
    println!(
        "Downloading {} files ({}) [chunked]",
        resp.files.len(),
        format_bytes(resp.total_size_bytes)
    );

    // Resolve chunk cache dir
    let chunk_cache_dir = resolve_chunk_cache_dir()?;
    std::fs::create_dir_all(&chunk_cache_dir).with_context(|| {
        format!(
            "Failed to create chunk cache: {}",
            chunk_cache_dir.display()
        )
    })?;

    let multi_progress = MultiProgress::new();
    let sty = ProgressStyle::with_template(
        "  {prefix:.bold} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap()
    .progress_chars("##-");

    let download_client = reqwest::Client::new();
    let mut checksum_errors: Vec<String> = Vec::new();

    for pull_file in &resp.files {
        if Path::new(&pull_file.filename)
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            bail!(
                "Refusing to download '{}': path traversal detected",
                pull_file.filename
            );
        }

        let file_path = out_dir.join(&pull_file.filename);

        if pull_file.chunked {
            let chunks = pull_file.chunks.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Chunked file '{}' missing chunk data", pull_file.filename)
            })?;

            let total_chunk_bytes: u64 = chunks.iter().map(|c| c.size_bytes).sum();
            let pb = multi_progress.add(ProgressBar::new(total_chunk_bytes));
            pb.set_style(sty.clone());
            pb.set_prefix(format!("{} (chunked)", pull_file.filename));

            // Download chunks with concurrency limit (4 parallel)
            let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
            let mut chunk_results: Vec<(i32, std::path::PathBuf)> = Vec::new();

            // Download all chunks (some may be cached)
            let mut handles = Vec::new();
            for chunk in chunks {
                let sem = semaphore.clone();
                let client = download_client.clone();
                let cache_path = chunk_cache_dir.join(&chunk.sha256);
                let chunk_sha = chunk.sha256.clone();
                let chunk_size = chunk.size_bytes;
                let download_url = chunk.download_url.clone();
                let chunk_index = chunk.chunk_index;
                let pb_clone = pb.clone();

                let handle = tokio::spawn(async move {
                    let _permit = sem
                        .acquire()
                        .await
                        .map_err(|e| anyhow::anyhow!("Semaphore error: {e}"))?;

                    // Check chunk cache
                    if cache_path.exists() {
                        let metadata = tokio::fs::metadata(&cache_path).await?;
                        if metadata.len() == chunk_size {
                            pb_clone.inc(chunk_size);
                            return Ok::<(i32, std::path::PathBuf), anyhow::Error>((
                                chunk_index,
                                cache_path,
                            ));
                        }
                    }

                    // Download chunk
                    let resp = client
                        .get(&download_url)
                        .send()
                        .await
                        .with_context(|| format!("Failed to download chunk {chunk_index}"))?;

                    if !resp.status().is_success() {
                        bail!("Failed to download chunk {chunk_index} ({})", resp.status());
                    }

                    let mut file = tokio::fs::File::create(&cache_path).await?;
                    let mut stream = resp.bytes_stream();
                    let mut hasher = StreamingHasher::new();

                    while let Some(data) = stream.next().await {
                        let data = data.context("Chunk download interrupted")?;
                        hasher.update(&data);
                        file.write_all(&data).await?;
                        pb_clone.inc(data.len() as u64);
                    }
                    file.flush().await?;

                    // Verify chunk hash
                    let computed = hasher.finalize();
                    if computed != chunk_sha {
                        // Remove bad chunk from cache
                        let _ = tokio::fs::remove_file(&cache_path).await;
                        bail!(
                            "Chunk {chunk_index} hash mismatch: expected {chunk_sha}, got {computed}"
                        );
                    }

                    Ok((chunk_index, cache_path))
                });

                handles.push(handle);
            }

            // Collect results
            for handle in handles {
                let result = handle.await.context("Chunk download task panicked")??;
                chunk_results.push(result);
            }

            pb.finish();

            // Sort chunks by index and reassemble
            chunk_results.sort_by_key(|(idx, _)| *idx);

            let mut output_file = tokio::fs::File::create(&file_path)
                .await
                .with_context(|| format!("Failed to create file: {}", file_path.display()))?;

            let mut full_hasher = StreamingHasher::new();

            for (_idx, chunk_path) in &chunk_results {
                let chunk_data = tokio::fs::read(chunk_path).await.with_context(|| {
                    format!("Failed to read cached chunk: {}", chunk_path.display())
                })?;
                full_hasher.update(&chunk_data);
                output_file.write_all(&chunk_data).await?;
            }

            output_file.flush().await?;

            // Verify full file SHA256
            let computed = full_hasher.finalize();
            if computed != pull_file.sha256 {
                checksum_errors.push(format!(
                    "{}: expected {}, got {computed}",
                    pull_file.filename, pull_file.sha256
                ));
            }
        } else {
            // Non-chunked file: regular download
            let download_url = pull_file.download_url.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Non-chunked file '{}' missing download URL",
                    pull_file.filename
                )
            })?;

            let pb = multi_progress.add(ProgressBar::new(pull_file.size_bytes));
            pb.set_style(sty.clone());
            pb.set_prefix(pull_file.filename.clone());

            let resp = download_client
                .get(download_url)
                .send()
                .await
                .with_context(|| format!("Failed to download {}", pull_file.filename))?;

            if !resp.status().is_success() {
                let status = resp.status();
                bail!("Failed to download {} ({status})", pull_file.filename);
            }

            let mut file = tokio::fs::File::create(&file_path)
                .await
                .with_context(|| format!("Failed to create file: {}", file_path.display()))?;

            let mut hasher = StreamingHasher::new();
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk
                    .with_context(|| format!("Download interrupted: {}", pull_file.filename))?;
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                pb.set_position(downloaded);
            }

            file.flush().await?;
            pb.finish();

            let computed = hasher.finalize();
            if computed != pull_file.sha256 {
                checksum_errors.push(format!(
                    "{}: expected {}, got {computed}",
                    pull_file.filename, pull_file.sha256
                ));
            }
        }
    }

    println!();
    if checksum_errors.is_empty() {
        println!(
            "Pull complete! {} files downloaded to {} [chunked, {}/{}:{tag}]",
            resp.files.len(),
            out_dir.display(),
            parsed.org,
            parsed.model
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

/// Resolve the chunk cache directory (~/.pullweights/cache/chunks/).
fn resolve_chunk_cache_dir() -> Result<std::path::PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(home.join(".pullweights").join("cache").join("chunks"))
}
