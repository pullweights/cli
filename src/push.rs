use anyhow::{bail, Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Body;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use tokio_util::io::ReaderStream;

use crate::checksum::sha256_reader;
use crate::utils::{api_client, format_bytes, parse_model_ref, sanitize_error};

/// Threshold for CDC chunking: files above 100 MB are chunked.
const CHUNKING_THRESHOLD: u64 = 100 * 1024 * 1024;

/// FastCDC parameters
const CDC_MIN_SIZE: u32 = 1024 * 1024; // 1 MB
const CDC_AVG_SIZE: u32 = 8 * 1024 * 1024; // 8 MB
const CDC_MAX_SIZE: u32 = 32 * 1024 * 1024; // 32 MB

// ---------------------------------------------------------------------------
// API types (regular push)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PushInitRequest {
    tag: String,
    visibility: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    model_type: Option<String>,
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
    #[serde(default)]
    deduplicated: Vec<DeduplicatedFile>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct DeduplicatedFile {
    filename: String,
    size_bytes: u64,
    sha256: String,
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
// API types (chunked push)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChunkedPushInitRequest {
    tag: String,
    visibility: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    model_type: Option<String>,
    files: Vec<ChunkedFileEntry>,
}

#[derive(Serialize)]
struct ChunkedFileEntry {
    filename: String,
    size_bytes: u64,
    sha256: String,
    chunking_version: u32,
    chunks: Vec<ChunkEntryReq>,
}

#[derive(Serialize)]
struct ChunkEntryReq {
    sha256: String,
    size_bytes: u64,
    offset_bytes: u64,
}

#[derive(Deserialize)]
struct ChunkedPushInitResponse {
    push_id: String,
    files: Vec<ChunkedFileUploadInfo>,
}

#[derive(Deserialize)]
struct ChunkedFileUploadInfo {
    filename: String,
    uploads: Vec<ChunkUploadTarget>,
    deduplicated_chunks: usize,
    deduplicated_bytes: u64,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct ChunkUploadTarget {
    chunk_index: usize,
    sha256: String,
    upload_url: String,
}

// ---------------------------------------------------------------------------
// Local chunk info computed by CDC
// ---------------------------------------------------------------------------

struct LocalChunkInfo {
    sha256: String,
    offset: u64,
    length: u64,
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
    description: Option<&str>,
    model_type: &str,
) -> Result<()> {
    if model_type != "model" && model_type != "dataset" {
        bail!("Invalid --type '{model_type}'. Must be 'model' or 'dataset'");
    }

    let parsed = parse_model_ref(model_ref)?;
    let tag = parsed
        .tag
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Tag is required for push. Use org/model:tag format"))?;

    // Validate files and compute checksums (streaming -- no full file in memory)
    // (filename, sha256, size, path)
    let mut file_entries: Vec<(String, String, u64, String)> = Vec::new();

    for file_path in files {
        let path = Path::new(file_path);
        if !path.exists() {
            bail!("File not found: {file_path}");
        }
        if !path.is_file() {
            bail!("Not a file: {file_path}");
        }
        let metadata =
            std::fs::metadata(path).with_context(|| format!("Failed to stat file: {file_path}"))?;
        let size = metadata.len();
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open file: {file_path}"))?;
        let sha =
            sha256_reader(file).with_context(|| format!("Failed to hash file: {file_path}"))?;
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.clone());
        file_entries.push((filename, sha, size, file_path.clone()));
    }

    let total_size: u64 = file_entries.iter().map(|(_, _, s, _)| s).sum();
    let vis_label = if visibility == "private" {
        " (private)"
    } else {
        ""
    };
    let type_label = if model_type == "dataset" {
        "dataset"
    } else {
        "model"
    };
    println!(
        "Pushing {} {}/{}{} ({} files, {})",
        type_label,
        parsed.org,
        parsed.model,
        vis_label,
        files.len(),
        format_bytes(total_size)
    );

    // Check if any files need chunking
    let has_large_files = file_entries
        .iter()
        .any(|(_, _, size, _)| *size > CHUNKING_THRESHOLD);

    if has_large_files {
        push_chunked(
            api_url,
            token,
            &parsed,
            tag,
            &file_entries,
            visibility,
            description,
            model_type,
        )
        .await
    } else {
        push_regular(
            api_url,
            token,
            &parsed,
            tag,
            &file_entries,
            visibility,
            description,
            model_type,
        )
        .await
    }
}

/// Regular push flow (no chunking) -- unchanged from original.
#[allow(clippy::too_many_arguments)]
async fn push_regular(
    api_url: &str,
    token: &str,
    parsed: &crate::utils::ModelRef,
    tag: &str,
    file_entries: &[(String, String, u64, String)],
    visibility: &str,
    description: Option<&str>,
    model_type: &str,
) -> Result<()> {
    let client = api_client(Some(token))?;

    let init_req = PushInitRequest {
        tag: tag.to_string(),
        visibility: Some(visibility.to_string()),
        description: description.map(|s| s.to_string()),
        model_type: Some(model_type.to_string()),
        files: file_entries
            .iter()
            .map(|(filename, sha, size, _path)| FileEntry {
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

    if !init_resp.deduplicated.is_empty() {
        let dedup_size: u64 = init_resp.deduplicated.iter().map(|f| f.size_bytes).sum();
        println!(
            "  Skipping {} deduplicated files ({})",
            init_resp.deduplicated.len(),
            format_bytes(dedup_size)
        );
    }

    // Upload files
    let multi_progress = MultiProgress::new();
    let sty = ProgressStyle::with_template(
        "  {prefix:.bold} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap()
    .progress_chars("##-");

    let upload_client = reqwest::Client::new();

    for upload in &init_resp.uploads {
        let (_, _, size, file_path) = file_entries
            .iter()
            .find(|(f, _, _, _)| f == &upload.filename)
            .ok_or_else(|| {
                anyhow::anyhow!("Server returned unknown filename: {}", upload.filename)
            })?;

        let pb = multi_progress.add(ProgressBar::new(*size));
        pb.set_style(sty.clone());
        pb.set_prefix(upload.filename.clone());

        let file = tokio::fs::File::open(file_path)
            .await
            .with_context(|| format!("Failed to open {} for upload", upload.filename))?;
        let stream = ReaderStream::new(file);
        let body = Body::wrap_stream(stream);

        let resp = upload_client
            .put(&upload.upload_url)
            .header("content-type", "application/octet-stream")
            .header("content-length", size.to_string())
            .body(body)
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

    // Finalize
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

    print_push_summary(parsed, &finalize_resp, &init_resp.deduplicated, &[]);

    Ok(())
}

/// Chunked push flow for files >100MB using content-defined chunking (FastCDC).
#[allow(clippy::too_many_arguments)]
async fn push_chunked(
    api_url: &str,
    token: &str,
    parsed: &crate::utils::ModelRef,
    tag: &str,
    file_entries: &[(String, String, u64, String)],
    visibility: &str,
    description: Option<&str>,
    model_type: &str,
) -> Result<()> {
    let client = api_client(Some(token))?;

    // Compute CDC chunks for large files
    println!("  Computing chunks for large files...");
    let mut chunked_files: Vec<ChunkedFileEntry> = Vec::new();
    // Track chunk data: filename -> Vec<LocalChunkInfo>
    let mut file_chunk_map: Vec<(String, Vec<LocalChunkInfo>)> = Vec::new();

    for (filename, sha, size, file_path) in file_entries {
        if *size > CHUNKING_THRESHOLD {
            let chunks = compute_chunks(file_path)?;
            println!(
                "  {} -> {} chunks (avg {})",
                filename,
                chunks.len(),
                format_bytes(*size / chunks.len() as u64)
            );

            let chunk_entries: Vec<ChunkEntryReq> = chunks
                .iter()
                .map(|c| ChunkEntryReq {
                    sha256: c.sha256.clone(),
                    size_bytes: c.length,
                    offset_bytes: c.offset,
                })
                .collect();

            chunked_files.push(ChunkedFileEntry {
                filename: filename.clone(),
                size_bytes: *size,
                sha256: sha.clone(),
                chunking_version: 1,
                chunks: chunk_entries,
            });
            file_chunk_map.push((filename.clone(), chunks));
        } else {
            // Small files: still include in chunked request with a single "chunk" = the whole file
            chunked_files.push(ChunkedFileEntry {
                filename: filename.clone(),
                size_bytes: *size,
                sha256: sha.clone(),
                chunking_version: 1,
                chunks: vec![ChunkEntryReq {
                    sha256: sha.clone(),
                    size_bytes: *size,
                    offset_bytes: 0,
                }],
            });
            file_chunk_map.push((
                filename.clone(),
                vec![LocalChunkInfo {
                    sha256: sha.clone(),
                    offset: 0,
                    length: *size,
                }],
            ));
        }
    }

    // Try chunked-init endpoint
    let init_url = format!(
        "{api_url}/v1/models/{}/{}/push/chunked-init",
        parsed.org, parsed.model
    );

    let init_req = ChunkedPushInitRequest {
        tag: tag.to_string(),
        visibility: Some(visibility.to_string()),
        description: description.map(|s| s.to_string()),
        model_type: Some(model_type.to_string()),
        files: chunked_files,
    };

    let resp = client
        .post(&init_url)
        .json(&init_req)
        .send()
        .await
        .context("Failed to connect to API server")?;

    // If server returns 404, fall back to regular push
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        println!("  Server does not support chunked push, falling back to regular push...");
        return push_regular(
            api_url,
            token,
            parsed,
            tag,
            file_entries,
            visibility,
            description,
            model_type,
        )
        .await;
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "Chunked push init failed ({status}): {}",
            sanitize_error(&body)
        );
    }

    let init_resp: ChunkedPushInitResponse = resp
        .json()
        .await
        .context("Invalid chunked push init response")?;

    // Report dedup summary
    let mut total_dedup_chunks: usize = 0;
    let mut total_dedup_bytes: u64 = 0;
    for fi in &init_resp.files {
        total_dedup_chunks += fi.deduplicated_chunks;
        total_dedup_bytes += fi.deduplicated_bytes;
    }
    if total_dedup_chunks > 0 {
        println!(
            "  Skipping {} deduplicated chunks ({})",
            total_dedup_chunks,
            format_bytes(total_dedup_bytes)
        );
    }

    // Upload new chunks
    let multi_progress = MultiProgress::new();
    let sty = ProgressStyle::with_template(
        "  {prefix:.bold} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap()
    .progress_chars("##-");

    let upload_client = reqwest::Client::new();

    for fi in &init_resp.files {
        if fi.uploads.is_empty() {
            continue;
        }

        // Find the file path and chunk map for this file
        let (_, _, _, file_path) = file_entries
            .iter()
            .find(|(f, _, _, _)| f == &fi.filename)
            .ok_or_else(|| anyhow::anyhow!("Server returned unknown filename: {}", fi.filename))?;

        let (_, local_chunks) = file_chunk_map
            .iter()
            .find(|(f, _)| f == &fi.filename)
            .ok_or_else(|| anyhow::anyhow!("Missing chunk map for: {}", fi.filename))?;

        for chunk_upload in &fi.uploads {
            let local_chunk = local_chunks.get(chunk_upload.chunk_index).ok_or_else(|| {
                anyhow::anyhow!(
                    "Chunk index {} out of range for {}",
                    chunk_upload.chunk_index,
                    fi.filename
                )
            })?;

            let pb = multi_progress.add(ProgressBar::new(local_chunk.length));
            pb.set_style(sty.clone());
            pb.set_prefix(format!(
                "{} [chunk {}/{}]",
                fi.filename,
                chunk_upload.chunk_index + 1,
                local_chunks.len()
            ));

            // Read chunk data from file at the correct offset
            let chunk_data =
                read_chunk_from_file(file_path, local_chunk.offset, local_chunk.length)?;

            let chunk_size = chunk_data.len() as u64;
            let resp = upload_client
                .put(&chunk_upload.upload_url)
                .header("content-type", "application/octet-stream")
                .header("content-length", chunk_size.to_string())
                .body(chunk_data)
                .send()
                .await
                .with_context(|| {
                    format!(
                        "Failed to upload chunk {} of {}",
                        chunk_upload.chunk_index, fi.filename
                    )
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                bail!(
                    "S3 upload failed for chunk {} of {} ({status}): {}",
                    chunk_upload.chunk_index,
                    fi.filename,
                    sanitize_error(&body)
                );
            }

            pb.set_position(chunk_size);
            pb.finish();
        }
    }

    // Finalize
    let finalize_url = format!(
        "{api_url}/v1/models/{}/{}/push/chunked-finalize",
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
        bail!(
            "Chunked push finalize failed ({status}): {}",
            sanitize_error(&body)
        );
    }

    let finalize_resp: PushFinalizeResponse = resp
        .json()
        .await
        .context("Invalid push finalize response")?;

    print_push_summary(parsed, &finalize_resp, &[], &init_resp.files);

    Ok(())
}

/// Compute FastCDC chunks for a file. Returns chunk metadata with SHA256 hashes.
fn compute_chunks(file_path: &str) -> Result<Vec<LocalChunkInfo>> {
    let file_data = std::fs::read(file_path)
        .with_context(|| format!("Failed to read file for chunking: {file_path}"))?;

    let chunker =
        fastcdc::v2020::FastCDC::new(&file_data, CDC_MIN_SIZE, CDC_AVG_SIZE, CDC_MAX_SIZE);

    let mut chunks = Vec::new();
    for chunk in chunker {
        let mut hasher = Sha256::new();
        hasher.update(&file_data[chunk.offset..chunk.offset + chunk.length]);
        let sha256 = hex::encode(hasher.finalize());

        chunks.push(LocalChunkInfo {
            sha256,
            offset: chunk.offset as u64,
            length: chunk.length as u64,
        });
    }

    Ok(chunks)
}

/// Read a specific chunk from a file at the given offset and length.
fn read_chunk_from_file(file_path: &str, offset: u64, length: u64) -> Result<Vec<u8>> {
    use std::io::Seek;
    let mut file = std::fs::File::open(file_path)
        .with_context(|| format!("Failed to open file for chunk read: {file_path}"))?;
    file.seek(std::io::SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; length as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

fn print_push_summary(
    parsed: &crate::utils::ModelRef,
    finalize_resp: &PushFinalizeResponse,
    deduplicated_files: &[DeduplicatedFile],
    chunked_files: &[ChunkedFileUploadInfo],
) {
    println!("\nPush successful!");
    println!("  Model:   {}/{}", parsed.org, parsed.model);
    println!("  Tag:     {}", finalize_resp.tag);
    println!("  Files:   {}", finalize_resp.files.len());
    println!(
        "  Size:    {}",
        format_bytes(finalize_resp.total_size_bytes)
    );
    println!("  Digest:  {}", &finalize_resp.sha256_digest[..12]);
    if !deduplicated_files.is_empty() {
        let dedup_size: u64 = deduplicated_files.iter().map(|f| f.size_bytes).sum();
        println!(
            "  Dedup:   {} files saved ({})",
            deduplicated_files.len(),
            format_bytes(dedup_size)
        );
    }
    if !chunked_files.is_empty() {
        let total_dedup_chunks: usize = chunked_files.iter().map(|f| f.deduplicated_chunks).sum();
        let total_dedup_bytes: u64 = chunked_files.iter().map(|f| f.deduplicated_bytes).sum();
        if total_dedup_chunks > 0 {
            println!(
                "  Chunks:  {} deduplicated ({})",
                total_dedup_chunks,
                format_bytes(total_dedup_bytes)
            );
        }
    }
}
