use clap::{Parser, Subcommand};

use pullweights_cli::{
    api_key, auth, config, delete, inspect, logout, ls, pull, push, search, tags, utils, verify,
};

#[derive(Parser)]
#[command(name = "pullweights")]
#[command(about = "AI Model Registry CLI — push, pull, and manage ML models and datasets")]
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
        /// Type: "model" or "dataset"
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
        /// Filter by type: "model" or "dataset"
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
