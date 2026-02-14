#[tokio::test]
async fn test_auth_with_key_validates_format() {
    // Non-pw_ key should fail before any HTTP call
    let result = pullweights_cli::auth::auth_with_key(Some("invalid-key"), None).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("pw_"), "Expected pw_ format error, got: {err}");
}

#[tokio::test]
async fn test_auth_with_key_rejects_empty() {
    let result = pullweights_cli::auth::auth_with_key(Some(""), None).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("empty") || err.contains("pw_"),
        "Expected empty/format error, got: {err}"
    );
}
