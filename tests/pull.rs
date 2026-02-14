use pullweights_cli::checksum::sha256_hex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_pull_full_flow() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();

    let content = b"model file content";
    let sha = sha256_hex(content);

    // Mock pull endpoint
    Mock::given(method("GET"))
        .and(path("/v1/models/meta/llama/pull/v1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "files": [
                {
                    "filename": "model.bin",
                    "download_url": format!("{}/downloads/model.bin", server.uri()),
                    "size_bytes": content.len(),
                    "sha256": sha
                }
            ],
            "total_size_bytes": content.len()
        })))
        .mount(&server)
        .await;

    // Mock file download
    Mock::given(method("GET"))
        .and(path("/downloads/model.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content.to_vec()))
        .mount(&server)
        .await;

    let output = tmp.path().join("output");
    let result = pullweights_cli::pull::pull(
        &server.uri(),
        "test-token",
        "meta/llama:v1.0",
        output.to_str().unwrap(),
    )
    .await;
    assert!(result.is_ok());

    // Verify file was written
    let written = std::fs::read(output.join("model.bin")).unwrap();
    assert_eq!(written, content);
}

#[tokio::test]
async fn test_pull_requires_tag() {
    let result =
        pullweights_cli::pull::pull("http://localhost:1234", "tok", "org/model", ".").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Tag is required"),
        "Expected tag required error, got: {err}"
    );
}

#[tokio::test]
async fn test_pull_checksum_mismatch() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();

    // Mock pull endpoint with wrong sha256
    Mock::given(method("GET"))
        .and(path("/v1/models/org/model/pull/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "files": [
                {
                    "filename": "model.bin",
                    "download_url": format!("{}/downloads/model.bin", server.uri()),
                    "size_bytes": 5,
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
                }
            ],
            "total_size_bytes": 5
        })))
        .mount(&server)
        .await;

    // Return different content than what sha256 expects
    Mock::given(method("GET"))
        .and(path("/downloads/model.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
        .mount(&server)
        .await;

    let output = tmp.path().join("output");
    let result = pullweights_cli::pull::pull(
        &server.uri(),
        "tok",
        "org/model:v1",
        output.to_str().unwrap(),
    )
    .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("checksum"),
        "Expected checksum error, got: {err}"
    );
}

#[tokio::test]
async fn test_pull_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/org/model/pull/v1"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let result = pullweights_cli::pull::pull(
        &server.uri(),
        "tok",
        "org/model:v1",
        tmp.path().to_str().unwrap(),
    )
    .await;
    assert!(result.is_err());
}
