use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_inspect_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/meta/llama/manifests/v1.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "schema_version": 1,
            "name": "llama",
            "org": "meta",
            "tag": "v1.0",
            "description": "LLaMA model",
            "framework": "pytorch",
            "architecture": "transformer",
            "license": "MIT",
            "files": [
                {
                    "filename": "model.bin",
                    "size_bytes": 1024,
                    "sha256": "abc123def456",
                    "content_type": "application/octet-stream"
                }
            ],
            "metadata": {},
            "created_at": "2025-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let result =
        pullweights_cli::inspect::inspect(&server.uri(), "test-token", "meta/llama:v1.0").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_inspect_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/meta/llama/manifests/v1.0"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server)
        .await;

    let result =
        pullweights_cli::inspect::inspect(&server.uri(), "test-token", "meta/llama:v1.0").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_inspect_requires_tag() {
    // No mock server needed — should fail before HTTP
    let result =
        pullweights_cli::inspect::inspect("http://localhost:1234", "test-token", "meta/llama")
            .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Tag is required"),
        "Expected tag required error, got: {err}"
    );
}
