use pullweights_cli::checksum::sha256_hex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_push_full_flow() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("model.bin");
    let content = b"fake model data";
    std::fs::write(&file_path, content).unwrap();
    let sha = sha256_hex(content);

    Mock::given(method("POST")).and(path("/v1/models/testorg/testmodel/push/init"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "push_id": "push-123",
            "uploads": [{"filename": "model.bin", "upload_url": format!("{}/s3-upload/model.bin", server.uri())}]
        })))
        .mount(&server).await;
    Mock::given(method("PUT")).and(path("/s3-upload/model.bin"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/v1/models/testorg/testmodel/push/finalize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tag": "v1.0", "total_size_bytes": content.len(), "sha256_digest": sha,
            "files": [{"filename": "model.bin", "size_bytes": content.len(), "sha256": sha}]
        })))
        .mount(&server).await;

    let files = vec![file_path.to_string_lossy().to_string()];
    let result = pullweights_cli::push::push(&server.uri(), "test-token", "testorg/testmodel:v1.0", &files, "public").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_push_requires_tag() {
    let result = pullweights_cli::push::push("http://localhost:1234", "tok", "org/model", &["file.bin".to_string()], "public").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Tag is required"));
}

#[tokio::test]
async fn test_push_file_not_found() {
    let result = pullweights_cli::push::push("http://localhost:1234", "tok", "org/model:v1", &["/nonexistent/file.bin".to_string()], "public").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("File not found"));
}

#[tokio::test]
async fn test_push_init_failure() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("model.bin");
    std::fs::write(&file_path, b"data").unwrap();
    Mock::given(method("POST")).and(path("/v1/models/org/model/push/init"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .mount(&server).await;
    let files = vec![file_path.to_string_lossy().to_string()];
    let result = pullweights_cli::push::push(&server.uri(), "tok", "org/model:v1", &files, "public").await;
    assert!(result.is_err());
}
