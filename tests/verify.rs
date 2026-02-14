use pullweights_cli::checksum::sha256_hex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_manifest(files: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "name": "model",
        "org": "org",
        "tag": "v1",
        "files": files,
        "metadata": {},
        "created_at": "2025-01-01T00:00:00Z"
    })
}

#[tokio::test]
async fn test_verify_all_pass() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();

    let content = b"verified content";
    let sha = sha256_hex(content);

    // Write local file
    std::fs::write(tmp.path().join("model.bin"), content).unwrap();

    // Mock manifest endpoint
    Mock::given(method("GET"))
        .and(path("/v1/models/org/model/manifests/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_manifest(vec![
            serde_json::json!({
                "filename": "model.bin",
                "size_bytes": content.len(),
                "sha256": sha,
                "content_type": "application/octet-stream"
            }),
        ])))
        .mount(&server)
        .await;

    let result = pullweights_cli::verify::verify(
        &server.uri(),
        "tok",
        "org/model:v1",
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_verify_checksum_mismatch() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();

    // Write file with different content
    std::fs::write(tmp.path().join("model.bin"), b"wrong content").unwrap();

    Mock::given(method("GET"))
        .and(path("/v1/models/org/model/manifests/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_manifest(vec![
            serde_json::json!({
                "filename": "model.bin",
                "size_bytes": 100,
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "content_type": null
            }),
        ])))
        .mount(&server)
        .await;

    let result = pullweights_cli::verify::verify(
        &server.uri(),
        "tok",
        "org/model:v1",
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_verify_missing_file() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    // Don't create any file — it should be missing

    Mock::given(method("GET"))
        .and(path("/v1/models/org/model/manifests/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_manifest(vec![
            serde_json::json!({
                "filename": "model.bin",
                "size_bytes": 100,
                "sha256": "abc123",
                "content_type": null
            }),
        ])))
        .mount(&server)
        .await;

    let result = pullweights_cli::verify::verify(
        &server.uri(),
        "tok",
        "org/model:v1",
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert!(result.is_err());
}
