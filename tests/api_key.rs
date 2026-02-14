use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_api_key_list_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/api-keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "name": "My Key",
                "prefix": "pw_abc",
                "scopes": ["model:read", "model:push"],
                "allowed_orgs": null,
                "allowed_models": null,
                "allowed_ips": null,
                "last_used_at": null,
                "expires_at": null,
                "created_at": "2025-01-01T00:00:00Z"
            }
        ])))
        .mount(&server)
        .await;

    let result = pullweights_cli::api_key::list(&server.uri(), "test-token").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_api_key_list_empty() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/api-keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let result = pullweights_cli::api_key::list(&server.uri(), "test-token").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_api_key_create_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/api-keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "key": "pw_live_abcdef123456",
            "prefix": "pw_live_abc",
            "name": "CI Key",
            "scopes": ["model:read", "model:push"]
        })))
        .mount(&server)
        .await;

    let result = pullweights_cli::api_key::create(
        &server.uri(),
        "test-token",
        "CI Key",
        "model:read,model:push",
        None,
        None,
        None,
        None,
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_api_key_create_empty_scopes() {
    // Should fail before HTTP — empty scopes bail
    let result = pullweights_cli::api_key::create(
        "http://localhost:1234",
        "tok",
        "Key",
        "",
        None,
        None,
        None,
        None,
    )
    .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("scope"), "Expected scope error, got: {err}");
}

#[tokio::test]
async fn test_api_key_revoke_success() {
    let server = MockServer::start().await;
    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/api-keys/{uuid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": "API key revoked"
        })))
        .mount(&server)
        .await;

    let result = pullweights_cli::api_key::revoke(&server.uri(), "test-token", uuid).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_api_key_revoke_invalid_uuid() {
    // Should fail before HTTP — bad UUID
    let result =
        pullweights_cli::api_key::revoke("http://localhost:1234", "tok", "not-a-uuid").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("UUID") || err.contains("Invalid"),
        "Expected UUID error, got: {err}"
    );
}

#[tokio::test]
async fn test_api_key_revoke_not_found() {
    let server = MockServer::start().await;
    let uuid = "550e8400-e29b-41d4-a716-446655440001";
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/api-keys/{uuid}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {"message": "API key not found"}
        })))
        .mount(&server)
        .await;

    let result = pullweights_cli::api_key::revoke(&server.uri(), "test-token", uuid).await;
    assert!(result.is_err());
}
