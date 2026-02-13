use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

use crate::config::CliConfig;
use crate::utils::api_client;

#[derive(Serialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
    #[allow(dead_code)]
    user: LoginUser,
}

#[derive(Deserialize)]
struct LoginUser {
    #[allow(dead_code)]
    id: String,
    username: String,
    #[allow(dead_code)]
    email: String,
}

pub async fn login(api_url: &str) -> Result<()> {
    // Prompt for email
    eprint!("Email: ");
    io::stderr().flush()?;
    let mut email = String::new();
    io::stdin().read_line(&mut email)?;
    let email = email.trim().to_string();
    if email.is_empty() {
        bail!("Email cannot be empty");
    }

    // Prompt for password (no echo suppression in basic stdin, but functional)
    eprint!("Password: ");
    io::stderr().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim().to_string();
    if password.is_empty() {
        bail!("Password cannot be empty");
    }

    let client = api_client(None)?;
    let url = format!("{api_url}/v1/auth/login");

    let resp = client
        .post(&url)
        .json(&LoginRequest { email, password })
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Login failed ({status}): {body}");
    }

    let login_resp: LoginResponse = resp.json().await.context("Invalid login response")?;

    // Save token and API URL to config
    let mut cfg = CliConfig::load()?;
    cfg.token = Some(login_resp.token);
    cfg.api_url = Some(api_url.to_string());
    cfg.save()?;

    println!("Logged in as {}", login_resp.user.username);
    Ok(())
}

/// Authenticate using an API key instead of email/password.
/// Validates the key against the API before saving.
pub async fn auth_with_key(token: Option<&str>, api_url: Option<&str>) -> Result<()> {
    let key = if let Some(t) = token {
        t.to_string()
    } else {
        // Read from stdin (supports piping: echo "pw_..." | pullweights auth)
        eprint!("API Key: ");
        io::stderr().flush()?;
        let mut key = String::new();
        io::stdin().lock().read_line(&mut key)?;
        key.trim().to_string()
    };

    if key.is_empty() {
        bail!("API key cannot be empty");
    }

    if !key.starts_with("pw_") {
        bail!("Invalid API key format. Keys start with 'pw_'");
    }

    let mut cfg = CliConfig::load()?;
    let base_url = api_url.unwrap_or(cfg.api_url());

    // Validate key by calling /v1/account
    let client = api_client(Some(&key))?;
    let resp = client
        .get(format!("{base_url}/v1/account"))
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        bail!("Invalid API key — authentication failed");
    }

    #[derive(Deserialize)]
    struct Account {
        username: String,
    }
    let account: Account = resp.json().await.context("Invalid response")?;

    cfg.token = Some(key);
    if let Some(url) = api_url {
        cfg.api_url = Some(url.to_string());
    }
    cfg.save()?;

    println!("Authenticated as {} (via API key)", account.username);
    Ok(())
}
