use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::utils::{api_client, parse_model_ref, sanitize_error};

#[derive(Serialize)]
struct CreateDeployRequest {
    gpu_count: u32,
    memory: String,
    port: u16,
    ttl_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    vllm_args: Option<String>,
    tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct DeploymentResponse {
    id: String,
    status: String,
    url: Option<String>,
    message: Option<String>,
    progress: Option<DeployProgress>,
    provider_name: String,
}

#[derive(Deserialize)]
struct DeployProgress {
    current_step: Option<String>,
    percentage: Option<f64>,
}

pub struct DeployParams<'a> {
    pub api_url: &'a str,
    pub token: &'a str,
    pub model_ref: &'a str,
    pub gpu_count: u32,
    pub memory: &'a str,
    pub port: u16,
    pub ttl_seconds: u64,
    pub vllm_args: Option<&'a str>,
    pub provider: &'a str,
    pub api_key_for_model: Option<&'a str>,
    pub no_wait: bool,
}

pub async fn deploy(params: &DeployParams<'_>) -> Result<()> {
    let parsed = parse_model_ref(params.model_ref)?;
    let tag = parsed.tag.as_deref().unwrap_or("latest");

    let client = api_client(Some(params.token))?;
    let url = format!(
        "{}/v1/models/{}/{}/deploy/{}",
        params.api_url, parsed.org, parsed.model, params.provider
    );

    let body = CreateDeployRequest {
        gpu_count: params.gpu_count,
        memory: params.memory.to_string(),
        port: params.port,
        ttl_seconds: params.ttl_seconds,
        vllm_args: params.vllm_args.map(|s| s.to_string()),
        tag: tag.to_string(),
        api_key: params.api_key_for_model.map(|s| s.to_string()),
    };

    eprintln!(
        "Deploying {}/{}{} on {}...",
        parsed.org,
        parsed.model,
        parsed
            .tag
            .as_ref()
            .map(|t| format!(":{t}"))
            .unwrap_or_default(),
        params.provider
    );

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Deploy failed ({status}): {}", sanitize_error(&body));
    }

    let deployment: DeploymentResponse =
        resp.json().await.context("Invalid deployment response")?;

    eprintln!("Deployment created: {}", deployment.provider_name);

    if params.no_wait {
        eprintln!("Deployment ID: {}", deployment.id);
        eprintln!("Status: {}", deployment.status);
        eprintln!(
            "Use `pullweights deploy status {}` to check progress.",
            deployment.id
        );
        return Ok(());
    }

    // Poll until terminal state
    poll_deployment(params.api_url, params.token, &deployment.id).await
}

pub async fn status(api_url: &str, token: &str, deployment_id: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/deploy/deployments/{deployment_id}");

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "Failed to get deployment ({status}): {}",
            sanitize_error(&body)
        );
    }

    let d: DeploymentResponse = resp.json().await.context("Invalid deployment response")?;
    print_deployment_status(&d);
    Ok(())
}

pub async fn list(api_url: &str, token: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/deploy/deployments");

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "Failed to list deployments ({status}): {}",
            sanitize_error(&body)
        );
    }

    let deployments: Vec<DeploymentResponse> = resp.json().await.context("Invalid response")?;

    if deployments.is_empty() {
        eprintln!("No deployments found.");
        return Ok(());
    }

    for d in &deployments {
        let url_str = d.url.as_deref().unwrap_or("-");
        println!("{:<38} {:<10} {}", d.id, d.status, url_str);
    }

    Ok(())
}

pub async fn stop(api_url: &str, token: &str, deployment_id: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/deploy/deployments/{deployment_id}");

    let resp = client
        .delete(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!(
            "Failed to stop deployment ({status}): {}",
            sanitize_error(&body)
        );
    }

    let d: DeploymentResponse = resp.json().await.context("Invalid response")?;
    eprintln!("Deployment {} stopped.", d.id);
    Ok(())
}

pub async fn logs(api_url: &str, token: &str, deployment_id: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/deploy/deployments/{deployment_id}/logs");

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to API server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Failed to get logs ({status}): {}", sanitize_error(&body));
    }

    #[derive(Deserialize)]
    struct LogsResponse {
        logs: String,
    }

    let data: LogsResponse = resp.json().await.context("Invalid response")?;
    print!("{}", data.logs);
    Ok(())
}

async fn poll_deployment(api_url: &str, token: &str, deployment_id: &str) -> Result<()> {
    let client = api_client(Some(token))?;
    let url = format!("{api_url}/v1/deploy/deployments/{deployment_id}");

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.blue} {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Deploying...");

    loop {
        tokio::time::sleep(Duration::from_secs(4)).await;

        let resp = client
            .get(&url)
            .send()
            .await
            .context("Failed to poll deployment status")?;

        if !resp.status().is_success() {
            continue; // Retry on transient errors
        }

        let d: DeploymentResponse = match resp.json().await {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Update spinner message
        let msg = if let Some(ref progress) = d.progress {
            let step = progress.current_step.as_deref().unwrap_or("Deploying");
            if let Some(pct) = progress.percentage {
                format!("{step} ({:.0}%)", pct)
            } else {
                step.to_string()
            }
        } else {
            format!("Status: {}", d.status)
        };
        pb.set_message(msg);

        match d.status.as_str() {
            "running" => {
                pb.finish_and_clear();
                eprintln!("Deployment is live!");
                print_deployment_status(&d);
                return Ok(());
            }
            "failed" => {
                pb.finish_and_clear();
                let msg = d.message.unwrap_or_else(|| "Unknown error".into());
                bail!("Deployment failed: {msg}");
            }
            "stopped" => {
                pb.finish_and_clear();
                bail!("Deployment was stopped.");
            }
            _ => {} // Keep polling for "creating", etc.
        }
    }
}

fn print_deployment_status(d: &DeploymentResponse) {
    eprintln!("  ID:     {}", d.id);
    eprintln!("  Status: {}", d.status);
    if let Some(ref url) = d.url {
        eprintln!("  URL:    {url}");
    }
    if let Some(ref msg) = d.message {
        if !msg.is_empty() {
            eprintln!("  Info:   {msg}");
        }
    }
}
