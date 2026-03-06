use clap::{Parser, Subcommand};

use pullweights_cli::{
    api_key, auth, config, delete, deploy, inspect, logout, ls, pull, push, search, tags, update,
    utils, verify,
};

#[derive(Parser)]
#[command(name = "pullweights")]
#[command(
    about = "AI Registry CLI — push, pull, and manage models, datasets, and container images"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Login to PullWeights registry
    Login {
        /// API URL override
        #[arg(long, default_value = "https://api.pullweights.com")]
        api_url: String,
        /// Login method: "browser" (OAuth) or "password" (email/password).
        /// Omit for interactive menu.
        #[arg(long)]
        method: Option<String>,
    },
    /// Authenticate with an API key (for CI/CD or programmatic access)
    Auth {
        /// API key (or set PULLWEIGHTS_TOKEN env var, or omit for interactive prompt)
        #[arg(long)]
        token: Option<String>,
        /// API URL override
        #[arg(long)]
        api_url: Option<String>,
    },
    /// Log out and clear saved credentials
    Logout,
    /// List models in an org, or list your orgs
    Ls {
        /// Organization name (omit to list your orgs)
        org: Option<String>,
    },
    /// Push a model to the registry
    Push {
        /// Model reference (org/model:tag)
        model_ref: String,
        /// Files to upload
        #[arg(required = true)]
        files: Vec<String>,
        /// Make the model private (default: public)
        #[arg(long)]
        private: bool,
        /// Model version description
        #[arg(long, short)]
        description: Option<String>,
        /// Type: "model", "dataset", or "container_image"
        #[arg(long = "type", default_value = "model")]
        model_type: String,
    },
    /// Delete a model from the registry (requires org admin)
    Delete {
        /// Model reference (org/model)
        model_ref: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Pull a model from the registry
    Pull {
        /// Model reference (org/model:tag)
        model_ref: String,
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: String,
    },
    /// List tags for a model
    Tags {
        /// Model reference (org/model)
        model_ref: String,
    },
    /// Search models
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "20")]
        limit: u32,
        /// Filter by type: "model", "dataset", or "container_image"
        #[arg(long = "type")]
        model_type: Option<String>,
    },
    /// Inspect model manifest
    Inspect {
        /// Model reference (org/model:tag)
        model_ref: String,
    },
    /// Verify local files against remote checksums
    Verify {
        /// Model reference (org/model:tag)
        model_ref: String,
        /// Directory with model files
        #[arg(short, long, default_value = ".")]
        dir: String,
    },
    /// Update model metadata (description, etc.)
    Update {
        /// Model reference (org/model)
        model_ref: String,
        /// New description (or @path/to/file.md to read from file)
        #[arg(long, short)]
        description: String,
    },
    /// Deploy a model to GPU infrastructure
    Deploy {
        #[command(subcommand)]
        action: DeployAction,
    },
    /// Manage API keys
    #[command(name = "api-key")]
    ApiKey {
        #[command(subcommand)]
        action: ApiKeyAction,
    },
    /// Manage CLI configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ApiKeyAction {
    /// List all API keys
    List,
    /// Create a new API key
    Create {
        /// Key name
        #[arg(long)]
        name: String,
        /// Comma-separated scopes (e.g., model:read,model:push)
        #[arg(long)]
        scopes: String,
        /// Restrict to specific orgs (comma-separated)
        #[arg(long)]
        orgs: Option<String>,
        /// Restrict to specific models (comma-separated, org/model format)
        #[arg(long)]
        models: Option<String>,
        /// Restrict to specific IPs (comma-separated)
        #[arg(long)]
        ips: Option<String>,
        /// Expiration date (ISO 8601 or YYYY-MM-DD)
        #[arg(long)]
        expires: Option<String>,
    },
    /// Revoke an API key
    Revoke {
        /// API key ID (UUID)
        id: String,
    },
}

#[derive(Subcommand)]
enum DeployAction {
    /// Deploy a model (one-click)
    Run {
        /// Model reference (org/model or org/model:tag)
        model_ref: String,
        /// Number of GPUs
        #[arg(long, default_value = "1")]
        gpu_count: u32,
        /// Container memory (e.g. 16Gi, 32Gi)
        #[arg(long, default_value = "32Gi")]
        memory: String,
        /// Port for the inference server
        #[arg(long, default_value = "8000")]
        port: u16,
        /// Time-to-live in seconds (auto-shutdown timer)
        #[arg(long, default_value = "3600")]
        ttl: u64,
        /// Extra vLLM arguments (e.g. "--max-model-len 4096")
        #[arg(long)]
        vllm_args: Option<String>,
        /// Deployment provider
        #[arg(long, default_value = "basilica")]
        provider: String,
        /// API key for private model access (inside the container)
        #[arg(long)]
        api_key: Option<String>,
        /// Don't wait for deployment to become ready
        #[arg(long)]
        no_wait: bool,
    },
    /// List your deployments
    #[command(name = "ls")]
    List,
    /// Show deployment status
    Status {
        /// Deployment ID
        id: String,
    },
    /// Stop a running deployment
    Stop {
        /// Deployment ID
        id: String,
    },
    /// View deployment logs
    Logs {
        /// Deployment ID
        id: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current config
    Show,
    /// Set a config value
    Set { key: String, value: String },
    /// Get a config value
    Get { key: String },
}

/// Load config and extract the API URL (prefer command-line override, else config, else default).
fn resolve_api_url(cfg: &config::CliConfig) -> String {
    cfg.api_url().to_string()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Login { api_url, method } => {
            let method = match method.as_deref() {
                Some(m) => Some(auth::LoginMethod::parse(m).ok_or_else(|| {
                    anyhow::anyhow!("Invalid --method: {m}. Use 'browser' or 'password'")
                })?),
                None => None,
            };
            auth::login(&api_url, method).await?;
        }
        Commands::Auth { token, api_url } => {
            let token = token.or_else(|| std::env::var("PULLWEIGHTS_TOKEN").ok());
            auth::auth_with_key(token.as_deref(), api_url.as_deref()).await?;
        }
        Commands::Logout => {
            logout::logout()?;
        }
        Commands::Ls { org } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            ls::ls(&api_url, &token, org.as_deref()).await?;
        }
        Commands::Push {
            model_ref,
            files,
            private,
            description,
            model_type,
        } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            let visibility = if private { "private" } else { "public" };
            push::push(
                &api_url,
                &token,
                &model_ref,
                &files,
                visibility,
                description.as_deref(),
                &model_type,
            )
            .await?;
        }
        Commands::Delete { model_ref, yes } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            if !yes {
                eprint!(
                    "Are you sure you want to delete '{model_ref}'? This cannot be undone. [y/N] "
                );
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            delete::delete(&api_url, &token, &model_ref).await?;
        }
        Commands::Pull { model_ref, output } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            pull::pull(&api_url, &token, &model_ref, &output).await?;
        }
        Commands::Tags { model_ref } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            tags::list_tags(&api_url, &token, &model_ref).await?;
        }
        Commands::Search {
            query,
            limit,
            model_type,
        } => {
            let cfg = config::CliConfig::load()?;
            let api_url = resolve_api_url(&cfg);
            let token = utils::require_token(cfg.token.as_deref()).ok();
            search::search(
                &api_url,
                token.as_deref(),
                &query,
                limit,
                model_type.as_deref(),
            )
            .await?;
        }
        Commands::Inspect { model_ref } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            inspect::inspect(&api_url, &token, &model_ref).await?;
        }
        Commands::Verify { model_ref, dir } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            verify::verify(&api_url, &token, &model_ref, &dir).await?;
        }
        Commands::Update {
            model_ref,
            description,
        } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            update::run(&api_url, &token, &model_ref, &description).await?;
        }
        Commands::Deploy { action } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            match action {
                DeployAction::Run {
                    model_ref,
                    gpu_count,
                    memory,
                    port,
                    ttl,
                    vllm_args,
                    provider,
                    api_key,
                    no_wait,
                } => {
                    deploy::deploy(&deploy::DeployParams {
                        api_url: &api_url,
                        token: &token,
                        model_ref: &model_ref,
                        gpu_count,
                        memory: &memory,
                        port,
                        ttl_seconds: ttl,
                        vllm_args: vllm_args.as_deref(),
                        provider: &provider,
                        api_key_for_model: api_key.as_deref(),
                        no_wait,
                    })
                    .await?;
                }
                DeployAction::List => {
                    deploy::list(&api_url, &token).await?;
                }
                DeployAction::Status { id } => {
                    deploy::status(&api_url, &token, &id).await?;
                }
                DeployAction::Stop { id } => {
                    deploy::stop(&api_url, &token, &id).await?;
                }
                DeployAction::Logs { id } => {
                    deploy::logs(&api_url, &token, &id).await?;
                }
            }
        }
        Commands::ApiKey { action } => {
            let cfg = config::CliConfig::load()?;
            let token = utils::require_token(cfg.token.as_deref())?;
            let api_url = resolve_api_url(&cfg);
            match action {
                ApiKeyAction::List => {
                    api_key::list(&api_url, &token).await?;
                }
                ApiKeyAction::Create {
                    name,
                    scopes,
                    orgs,
                    models,
                    ips,
                    expires,
                } => {
                    api_key::create(
                        &api_url,
                        &token,
                        &name,
                        &scopes,
                        orgs.as_deref(),
                        models.as_deref(),
                        ips.as_deref(),
                        expires.as_deref(),
                    )
                    .await?;
                }
                ApiKeyAction::Revoke { id } => {
                    api_key::revoke(&api_url, &token, &id).await?;
                }
            }
        }
        Commands::Config { action } => match action {
            ConfigAction::Show => {
                let cfg = config::CliConfig::load()?;
                println!("{cfg}");
            }
            ConfigAction::Set { key, value } => {
                let mut cfg = config::CliConfig::load()?;
                cfg.set(&key, &value)?;
                cfg.save()?;
                println!("Set {key} = {value}");
            }
            ConfigAction::Get { key } => {
                let cfg = config::CliConfig::load()?;
                match cfg.get(&key) {
                    Some(v) => println!("{v}"),
                    None => println!("Key '{key}' not set"),
                }
            }
        },
    }

    Ok(())
}
