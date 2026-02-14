# pullweights

<!-- TODO: Add logo. <p align="center"><img src="logo.png" width="200" alt="PullWeights"></p> -->

The command-line interface for [PullWeights](https://pullweights.com) -- push, pull, and manage AI models from your terminal.

## Installation

### Homebrew (macOS and Linux)

```sh
brew install pullweights/tap/pullweights
```

### Cargo

```sh
cargo install pullweights-cli
```

### Pre-built binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/pullweights/cli/releases).

Binaries are available for:
- Linux (x86_64, aarch64)
- macOS (x86_64, Apple Silicon)
- Windows (x86_64)

## Quick start

Log in to your PullWeights account:

```sh
pullweights login
```

Push a model:

```sh
pullweights push myorg/my-model:v1.0 model.safetensors config.json
```

Pull a model:

```sh
pullweights pull myorg/my-model:v1.0 -o ./models
```

Search for models:

```sh
pullweights search "llama"
```

## Commands

| Command | Description |
|---------|-------------|
| `pullweights login` | Log in with email and password |
| `pullweights auth` | Authenticate with an API key |
| `pullweights push <org/model:tag> <files...>` | Upload model files to the registry |
| `pullweights pull <org/model:tag>` | Download model files |
| `pullweights tags <org/model>` | List available tags for a model |
| `pullweights search <query>` | Search the model registry |
| `pullweights inspect <org/model:tag>` | View the model manifest (files, sizes, checksums) |
| `pullweights verify <org/model:tag>` | Verify local files against remote checksums |
| `pullweights api-key <list\|create\|revoke>` | Manage API keys |
| `pullweights config <show\|set\|get>` | View or modify CLI configuration |

### push

```sh
pullweights push myorg/my-model:v1.0 weights.safetensors tokenizer.json --private
```

Models are public by default. Pass `--private` to restrict access.

All files are checksummed (SHA-256) before upload and verified server-side.

### pull

```sh
pullweights pull myorg/my-model:v1.0 -o ./output-dir
```

Downloads all files for the given tag into the output directory (defaults to `.`). Checksums are automatically verified after download.

### inspect

```sh
pullweights inspect myorg/my-model:v1.0
```

Prints the full manifest as JSON, including every file's name, size, and SHA-256 checksum.

### verify

```sh
pullweights verify myorg/my-model:v1.0 -d ./models
```

Compares local files against the remote manifest. Reports which files pass, fail, or are missing.

### api-key

```sh
pullweights api-key list
pullweights api-key create --name "CI" --scopes model:read,model:push
pullweights api-key revoke <key-id>
```

Keys support granular scopes (`model:read`, `model:push`, `model:delete`, `model:admin`, `org:read`, `org:admin`, `account:read`, `account:admin`) and optional restrictions by org, model, IP, or expiration date.

The full key is shown only once at creation. Store it securely.

## Authentication

The CLI supports three authentication methods:

### Interactive login

```sh
pullweights login
```

Prompts for email and password. Stores a session token in `~/.pullweights/config.toml`.

### API key

```sh
pullweights auth
```

Prompts for an API key (prefix `pw_`). The key is validated against the API before being saved. You can also pipe it in:

```sh
echo "pw_..." | pullweights auth
```

### Environment variable (CI/CD)

Set `PULLWEIGHTS_TOKEN` to a session token or API key. This takes priority over the config file, so it works in CI without modifying local state:

```sh
export PULLWEIGHTS_TOKEN="pw_..."
pullweights push myorg/my-model:v1.0 model.safetensors
```

## Configuration

Configuration is stored at `~/.pullweights/config.toml` with permissions restricted to the current user (`0600`).

```toml
api_url = "https://api.pullweights.com"
token = "..."
cache_dir = "~/.pullweights/cache"
```

| Key | Description | Default |
|-----|-------------|---------|
| `api_url` | API server URL | `https://api.pullweights.com` |
| `token` | Authentication token (session or API key) | -- |
| `cache_dir` | Local cache directory for downloads | `~/.pullweights/cache` |

Manage configuration with:

```sh
pullweights config show           # print current config (token is masked)
pullweights config get api_url    # get a single value
pullweights config set api_url https://api.pullweights.com
```

## Documentation

Full documentation is available at [pullweights.com/docs](https://pullweights.com/docs).

## License

MIT
