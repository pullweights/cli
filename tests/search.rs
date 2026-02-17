use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_search_returns_results() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/search"))
        .and(query_param("q", "llama"))
        .and(query_param("per_page", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "org_name": "meta",
                    "name": "llama-7b",
                    "description": "LLaMA 7B",
                    "download_count": 1000
                }
            ],
            "total": 1
        })))
        .mount(&server)
        .await;

    let result =
        pullweights_cli::search::search(&server.uri(), Some("test-token"), "llama", 10, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_no_results() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [],
            "total": 0
        })))
        .mount(&server)
        .await;

    let result =
        pullweights_cli::search::search(&server.uri(), Some("tok"), "nonexistent", 20, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_without_auth() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [],
            "total": 0
        })))
        .mount(&server)
        .await;

    let result = pullweights_cli::search::search(&server.uri(), None, "test", 20, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/search"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;

    let result = pullweights_cli::search::search(&server.uri(), Some("tok"), "test", 20, None).await;
    assert!(result.is_err());
}
