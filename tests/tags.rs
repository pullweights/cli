use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_list_tags_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/models/meta/llama/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"tag": "v1.0", "total_size_bytes": 1048576, "created_at": "2025-01-01T00:00:00Z"},
            {"tag": "v2.0", "total_size_bytes": 2097152, "created_at": "2025-06-01T00:00:00Z"}
        ])))
        .mount(&server).await;
    let result = pullweights_cli::tags::list_tags(&server.uri(), "test-token", "meta/llama").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_list_tags_empty() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/models/meta/llama/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server).await;
    let result = pullweights_cli::tags::list_tags(&server.uri(), "test-token", "meta/llama").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_list_tags_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/models/meta/nonexistent/tags"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
        .mount(&server).await;
    let result = pullweights_cli::tags::list_tags(&server.uri(), "test-token", "meta/nonexistent").await;
    assert!(result.is_err());
}
