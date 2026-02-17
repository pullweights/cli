<p align="center">
  <h1 align="center">PullWeights</h1>
  <p align="center">
    <strong>Push, pull, and manage AI models & datasets from your terminal.</strong>
  </p>
  <p align="center">
    <a href="https://pullweights.com">Website</a> · <a href="https://pullweights.com/docs">Docs</a> · <a href="https://github.com/pullweights/mcp">MCP Server</a>
  </p>
  <p align="center">
    <a href="https://github.com/pullweights/cli/releases"><img src="https://img.shields.io/github/v/release/pullweights/cli?style=flat-square" alt="Release"></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License"></a>
    <a href="https://github.com/pullweights/cli"><img src="https://img.shields.io/badge/built%20with-Rust-orange?style=flat-square" alt="Rust"></a>
  </p>
</p>

---

**PullWeights** is a model & dataset registry built for automation. No rate limits. No download throttling. CLI-first.

```sh
brew install pullweights/tap/pullweights
pullweights pull meta-llama/Llama-3-8B:latest -o ./models
```

## Why PullWeights?

| | PullWeights | HuggingFace |
|---|:---:|:---:|
| Download limits | **None** | Rate limited |
| CLI-first | ✅ | Web-first |
| Native versioning | `org/model:tag` | Git-based |
| SHA-256 verification | Automatic | Manual |
| Dataset support | ✅ | Separate |
| MCP for AI agents | ✅ | ❌ |
| Written in | Rust | Python |

## Installation

### Homebrew (macOS and Linux)

```sh
brew install pullweights/tap/pullweights
```

### Cargo

```sh
cargo install pullweights-cli
```

### APT (Debian/Ubuntu)

```sh
# See https://github.com/pullweights/apt for setup
sudo apt install pullweights
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/pullweights/cli/releases) — Linux (x86_64, aarch64), macOS (Intel + Apple Silicon), Windows.

## Quick start

```sh
# Search for models
pullweights search "llama"

# Pull a model
pullweights pull myorg/my-model:v1.0 -o ./models

# Push a model
pullweights login
pullweights push myorg/my-model:v1.0 weights.safetensors config.json

# Inspect a model manifest
pullweights inspect myorg/my-model:v1.0
```

## Commands

| Command | Description |
|---------|-------------|
| `pullweights search <query>` | Search the model registry |
| `pullweights pull <org/model:tag>` | Download model files (SHA-256 verified) |
| `pullweights push <org/model:tag> <files...>` | Upload model files |
| `pullweights tags <org/model>` | List available tags |
| `pullweights inspect <org/model:tag>` | View manifest (files, sizes, checksums) |
| `pullweights verify <org/model:tag>` | Verify local files against remote |
| `pullweights login` | Log in with email/password |
| `pullweights auth` | Authenticate with API key |
| `pullweights api-key <list\|create\|revoke>` | Manage API keys |
| `pullweights config <show\|set\|get>` | CLI configuration |

### Push

```sh
pullweights push myorg/my-model:v1.0 weights.safetensors tokenizer.json --private
```

Models are public by default. Pass `--private` to restrict access. All files are checksummed (SHA-256) before upload and verified server-side.

### Pull

```sh
pullweights pull myorg/my-model:v1.0 -o ./output-dir
```

Downloads all files and verifies checksums automatically.

### Verify

```sh
pullweights verify myorg/my-model:v1.0 -d ./models
```

Compares local files against the remote manifest. Reports pass, fail, or missing.

### API Keys

```sh
pullweights api-key create --name "CI" --scopes model:read,model:push
```

Granular scopes: `model:read`, `model:push`, `model:delete`, `model:admin`, `org:read`, `org:admin`, `account:read`, `account:admin`. Optional restrictions by org, model, IP, or expiration.

## Authentication

```sh
# Interactive
pullweights login

# API key
pullweights auth
# or pipe it
echo "pw_..." | pullweights auth

# CI/CD (environment variable)
export PULLWEIGHTS_TOKEN="pw_..."
pullweights push myorg/my-model:v1.0 model.safetensors
```

## MCP Server (AI Agents)

PullWeights has a native [MCP server](https://github.com/pullweights/mcp) — your AI agents can search, pull, and push models programmatically from Claude Desktop, Cursor, Windsurf, and any MCP-compatible client.

```sh
npm install -g @pullweights/mcp
```

```json
{
  "mcpServers": {
    "pullweights": {
      "command": "npx",
      "args": ["-y", "@pullweights/mcp"],
      "env": { "PULLWEIGHTS_API_KEY": "pw_your_key" }
    }
  }
}
```

## Configuration

Stored at `~/.pullweights/config.toml` (permissions `0600`).

| Key | Description | Default |
|-----|-------------|---------|
| `api_url` | API server URL | `https://api.pullweights.com` |
| `token` | Auth token (session or API key) | — |
| `cache_dir` | Local cache | `~/.pullweights/cache` |

## Documentation

Full docs at [pullweights.com/docs](https://pullweights.com/docs).

## License

MIT
