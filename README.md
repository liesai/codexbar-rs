# codexbar-rs

`codexbar-rs` is an asynchronous Rust CLI that emits JSON for every command and models provider state through a shared status snapshot format.

The project currently supports:

- provider discovery
- prompt execution for `mock`, `ollama`, and `openai`
- provider status collection through `auto`, `api`, and `cli` source strategies
- a real CLI-backed collector for `ollama`
- persisted config for status defaults
- disk-backed status caching
- local diagnostics through `doctor`

## Current Scope

This repository is not a full Linux port of the original CodexBar product yet. It is the core CLI layer that is converging toward that goal:

- a richer `status` model
- explicit source selection
- local-first collection for Linux-friendly providers
- config and cache plumbing
- diagnostics for config, cache, and provider support

## Commands

### Health Check

```bash
cargo run -- ping
```

### List Providers

```bash
cargo run -- providers
```

### Run a Prompt

Mock:

```bash
cargo run -- run --provider mock --prompt "hello world"
```

Ollama:

```bash
cargo run -- run --provider ollama --prompt "Explain Rust in one sentence"
```

Ollama with overrides:

```bash
cargo run -- run --provider ollama --model llama3.2 --base-url http://127.0.0.1:11434 --prompt "Hi"
```

OpenAI:

```bash
export OPENAI_API_KEY="your_api_key"
cargo run -- run --provider openai --prompt "Hello"
```

### Status

Default status:

```bash
cargo run -- status --json
```

Force a source strategy:

```bash
cargo run -- status --json --source auto
cargo run -- status --json --source api
cargo run -- status --json --source cli
```

Cache controls:

```bash
cargo run -- status --json --refresh
cargo run -- status --json --no-cache
```

### Config and Diagnostics

Resolved config path:

```bash
cargo run -- config path
```

Diagnostics:

```bash
cargo run -- doctor --json
cargo run -- doctor --source cli --json
```

## Providers

### mock

- local demo provider
- supports prompt execution
- reports local mock status snapshots

### ollama

- supports prompt execution through the Ollama HTTP API
- supports `status --source api`
- supports `status --source cli` through `ollama ps`
- enriches CLI status with installed model count from `ollama ls`
- `status --source auto` prefers the CLI collector, then falls back to API

### openai

- supports prompt execution through the OpenAI API
- supports `status --source api`
- supports `status --source auto`, which currently routes to the API path
- parses `prompt_tokens`, `completion_tokens`, and `total_tokens`
- does not yet have a real `status --source cli` collector

## Source Strategy Behavior

The `status` command supports three source modes:

- `auto`: use the provider default strategy
- `api`: force API-backed collection
- `cli`: force CLI-backed collection

Current behavior by provider:

- `mock`: local behavior only
- `ollama`: `auto` prefers CLI, `api` uses HTTP, `cli` uses the real local collector
- `openai`: `api` is real, `auto` routes to API, `cli` returns a degraded snapshot because no stable local CLI collector is implemented yet

## Status Snapshot Model

Each provider returns a JSON status snapshot with fields such as:

- `provider`
- `primary`
- `secondary`
- `source`
- `health`
- `stale`
- `error`
- `prompt_tokens`
- `completion_tokens`
- `total_tokens`

Example:

```json
{
  "ok": true,
  "data": {
    "providers": {
      "ollama": {
        "health": "ok",
        "primary": {
          "used": 0
        },
        "provider": "ollama",
        "secondary": {
          "used": 11
        },
        "source": "cli",
        "stale": false
      },
      "openai": {
        "error": "openai CLI status strategy is not implemented",
        "health": "degraded",
        "provider": "openai",
        "primary": {},
        "source": "cli",
        "stale": true
      }
    }
  }
}
```

## Configuration

Status behavior can be configured through a JSON file at:

- `~/.config/codexbar/config.json`
- or `$XDG_CONFIG_HOME/codexbar/config.json`

Minimal example:

```json
{
  "status": {
    "default_source": "auto",
    "cache_ttl_seconds": 30,
    "cache_enabled": true
  }
}
```

If the config file is missing or invalid, the CLI falls back to safe defaults.

## Cache

Status snapshots are cached on disk by source mode at:

- `~/.cache/codexbar/`
- or `$XDG_CACHE_HOME/codexbar/`

Examples:

- `status-cache-auto.json`
- `status-cache-api.json`
- `status-cache-cli.json`

The cache is used only for `status`, not for `run`.

Behavior:

- if cache is enabled and fresh, `status` can return cached data
- `--refresh` bypasses cache reads and forces live collection
- `--no-cache` disables cache reads and writes
- if live collection fails and cached data exists, cached snapshots can be returned as stale fallback data

## Diagnostics

`doctor` reports the local runtime state without performing heavy provider collection.

It currently checks:

- resolved config path
- config presence
- cache path
- cache freshness for the selected source
- cache policy
- `ollama` CLI availability
- presence of `OPENAI_API_KEY`
- provider capability summary
- explicit warning for `openai --source cli`

## Environment Variables

### Ollama

- `OLLAMA_MODEL`
- `OLLAMA_BASE_URL`

### OpenAI

- `OPENAI_API_KEY`
- `OPENAI_BASE_URL`
- `OPENAI_MODEL`

## Error Output

All failures are returned as JSON.

Example:

```json
{
  "ok": false,
  "data": {},
  "error": {
    "message": "provider 'x' is not available"
  }
}
```
