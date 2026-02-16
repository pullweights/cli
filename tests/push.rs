use pullweights_cli::checksum::sha256_hex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_push_full_flow() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();

    // Create a temp file to push
    let file_path = tmp.path().join("model.bin");
    let content = b"fake model data";
    std::fs::write(&file_path, content).unwrap();
    let sha = sha256_hex(content);

    // Phase 1: Init
    Mock::given(method("POST"))
        .and(path("/v1/models/testorg/testmodel/push/init"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "push_id": "push-123",
            "uploads": [
                {
                    "filename": "model.bin",
                    "upload_url": format!("{}/s3-upload/model.bin", server.uri())
                }
            ]
        })))
        .mount(&server)
        .await;

    // Phase 2: S3 upload (mock the presigned URL)
    Mock::given(method("PUT"))
        .and(path("/s3-upload/model.bin"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    // Phase 3: Finalize
    Mock::given(method("POST"))
        .and(path("/v1/models/testorg/testmodel/push/finalize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tag": "v1.0",
            "total_size_bytes": content.len(),
            "sha256_digest": sha,
            "files": [
                {
                    "filename": "model.bin",
                    "size_bytes": content.len(),
                    "sha256": sha
                }
            ]
        })))
        .mount(&server)
        .await;

    let files = vec![file_path.to_string_lossy().to_string()];
    let result = pullweights_cli::push::push(
        &server.uri(),
        "test-token",
        "testorg/testmodel:v1.0",
        &files,
        "public",
        None,
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_push_requires_tag() {
    let result = pullweights_cli::push::push(
        "http://localhost:1234",
        "tok",
        "org/model",
        &["file.bin".to_string()],
        "public",
        None,
    )
    .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Tag is required"),
        "Expected tag required error, got: {err}"
    );
}

#[tokio::test]
async fn test_push_file_not_found() {
    let result = pullweights_cli::push::push(
        "http://localhost:1234",
        "tok",
        "org/model:v1",
        &["/nonexistent/file.bin".to_string()],
        "public",
        None,
    )
    .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("File not found"),
        "Expected file not found error, got: {err}"
    );
}

#[tokio::test]
async fn test_push_init_failure() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("model.bin");
    std::fs::write(&file_path, b"data").unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/models/org/model/push/init"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .mount(&server)
        .await;

    let files = vec![file_path.to_string_lossy().to_string()];
    let result =
        pullweights_cli::push::push(&server.uri(), "tok", "org/model:v1", &files, "public", None)
            .await;
    assert!(result.is_err());
}
