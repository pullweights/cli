use pullweights_cli::checksum::sha256_hex;

/// Build a minimal valid manifest JSON for testing.
pub fn sample_manifest(
    org: &str,
    name: &str,
    tag: &str,
    files: Vec<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "name": name,
        "org": org,
        "tag": tag,
        "files": files,
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z"
    })
}

/// Build a manifest file entry with correct SHA-256 for the given content.
pub fn manifest_file_entry(filename: &str, content: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "filename": filename,
        "size_bytes": content.len(),
        "sha256": sha256_hex(content),
        "content_type": "application/octet-stream"
    })
}

/// Build an API error response body.
pub fn error_response(message: &str) -> serde_json::Value {
    serde_json::json!({
        "error": {
            "message": message
        }
    })
}
