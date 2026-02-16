use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

use crate::config::CliConfig;
use crate::oauth_server;
use crate::utils::{api_client, sanitize_error};

/// Login method selected by user or --method flag.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    Browser,
    Password,
}

impl LoginMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "browser" => Some(Self::Browser),
            "password" => Some(Self::Password),
            _ => None,
        }
    }
}

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

#[derive(Deserialize)]
struct ProvidersResponse {
    providers: Vec<String>,
}

#[derive(Deserialize)]
struct AccountResponse {
    username: String,
}

/// Main login entry point. Shows interactive menu or uses the specified method.
pub async fn login(api_url: &str, method: Option<LoginMethod>) -> Result<()> {
    let method = match method {
        Some(m) => m,
        None => prompt_login_method(api_url).await?,
    };

    match method {
        LoginMethod::Browser => login_browser(api_url).await,
        LoginMethod::Password => login_email_password(api_url).await,
    }
}

/// Prompt user to choose login method interactively.
async fn prompt_login_method(api_url: &str) -> Result<LoginMethod> {
    // Check if OAuth providers are available
    let has_oauth = check_oauth_providers(api_url).await;

    if !has_oauth {
        // No OAuth configured — go straight to email/password
        return Ok(LoginMethod::Password);
    }

    eprintln!("How would you like to log in?");
    eprintln!("  [1] Browser (Google/GitHub)");
    eprintln!("  [2] Email & password");
    eprint!("Choice [1]: ");
    io::stderr().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    match input {
        "" | "1" => Ok(LoginMethod::Browser),
        "2" => Ok(LoginMethod::Password),
        _ => {
            eprintln!("Invalid choice, defaulting to browser login");
            Ok(LoginMethod::Browser)
        }
    }
}

/// Check if the API has any OAuth providers configured.
async fn check_oauth_providers(api_url: &str) -> bool {
    let client = match api_client(None) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let resp = client
        .get(format!("{api_url}/v1/auth/providers"))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => r
            .json::<ProvidersResponse>()
            .await
            .map(|p| !p.providers.is_empty())
            .unwrap_or(false),
        _ => false,
    }
}

/// Login via browser OAuth flow.
async fn login_browser(api_url: &str) -> Result<()> {
    // Start local callback server
    let (port, rx) = oauth_server::start_callback_server().await?;

    // Build the OAuth URL (default to github, user can pick on the web page)
    let url = format!("{api_url}/v1/auth/oauth/github?cli_port={port}");

    eprintln!("Opening browser for login...");

    // Try to open the browser
    if open::that(&url).is_err() {
        eprintln!("Could not open browser automatically.");
        eprintln!("Open this URL manually:");
        eprintln!("  {url}");
    }

    eprintln!("Waiting for login (timeout: 120s)...");

    // Wait for the callback
    let token = oauth_server::wait_for_callback(rx).await?;

    // Validate the token by fetching account info
    let client = api_client(Some(&token))?;
    let resp = client
        .get(format!("{api_url}/v1/account"))
        .send()
        .await
        .context("Failed to validate login token")?;

    if !resp.status().is_success() {
        bail!("Login token validation failed");
    }

    let account: AccountResponse = resp.json().await.context("Invalid account response")?;

    // Save config
    let mut cfg = CliConfig::load()?;
    cfg.token = Some(token);
    cfg.api_url = Some(api_url.to_string());
    cfg.save()?;

    println!("Logged in as {}", account.username);
    Ok(())
}

/// Login via email + password.
async fn login_email_password(api_url: &str) -> Result<()> {
    eprint!("Email: ");
    io::stderr().flush()?;
    let mut email = String::new();
    io::stdin().read_line(&mut email)?;
    let email = email.trim().to_string();
    if email.is_empty() {
        bail!("Email cannot be empty");
    }

    let password = rpassword::prompt_password("Password: ").context("Failed to read password")?;
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
        bail!("Login failed ({status}): {}", sanitize_error(&body));
    }

    let login_resp: LoginResponse = resp.json().await.context("Invalid login response")?;

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
