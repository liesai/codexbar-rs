# codexbar-rs

`codexbar-rs` is an asynchronous Rust CLI focused on provider status and usage collection, with JSON output, source-aware collection, persisted status configuration, and disk-backed caching.

The project currently provides:

- provider status snapshots through `status`
- source selection with `auto`, `api`, and `cli`
- a real CLI-backed `ollama` status collector
- OpenAI API-backed status probing
- persisted config and disk cache for status
- local diagnostics through `doctor`

## Current Scope

This repository is not a full Linux port of the upstream macOS product yet. At the moment it is a CLI-first foundation focused on:

- provider abstraction
- structured JSON output
- status snapshot modeling
- source routing and fallback behavior
- local observability and diagnostics

## Requirements

- Rust and Cargo
- for `ollama` API usage: a reachable Ollama instance, defaulting to `http://127.0.0.1:11434`
- for `ollama` CLI status collection: a working `ollama` binary in `PATH`
- for `openai` API usage: `OPENAI_API_KEY`

## Build And Run

```bash
cargo run -- ping
```

## Commands

### Ping

```bash
cargo run -- ping
cargo run -- ping --message "ok"
```

### Providers

```bash
cargo run -- providers
```

Current providers:

- `mock`
- `ollama`
- `openai`

### Status

Collect provider status snapshots.

```bash
cargo run -- status --json
```

Source selection:

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

Behavior by source:

- `auto`: provider default strategy
- `api`: force API-backed status collection
- `cli`: force CLI-backed status collection where available

Current source support:

- `mock`: local/mock status behavior
- `ollama`: API and real CLI collection
- `openai`: API-backed status only; `--source cli` returns a degraded snapshot by design

### Config

Print the resolved config file path:

```bash
cargo run -- config path
```

### Doctor

Run local diagnostics for config, cache, and provider support:

```bash
cargo run -- doctor --json
```

```bash
cargo run -- doctor --source cli --json
```

`doctor` currently reports:

- resolved config path
- whether config exists
- resolved cache path
- cache policy and selected source
- cache presence and freshness
- `ollama` CLI availability
- whether `OPENAI_API_KEY` is set
- provider capability summary
- explicit warning that `openai --source cli` is not implemented

## Provider Details

### Mock

`mock` is a local demo provider. It returns predictable status data useful for development.

### Ollama

`ollama` supports:

- API-backed status collection
- CLI-backed status collection

CLI status collection uses:

- `ollama ps` for active models
- `ollama ls` for installed models

With `--source cli`, the `ollama` snapshot currently maps:

- `primary.used` to the number of active models
- `secondary.used` to the number of installed models

With `--source auto`, `ollama` tries the CLI collector first and falls back to API status collection if needed.

Environment variables:

- `OLLAMA_BASE_URL`

### OpenAI

`openai` supports:

- API-backed status probing

Environment variables:

- `OPENAI_API_KEY`
- `OPENAI_BASE_URL`
- `OPENAI_MODEL`

`openai --source cli` is not implemented. The command returns a degraded snapshot intentionally rather than pretending CLI support exists.

## Persisted Config

Resolved config path:

- `$XDG_CONFIG_HOME/codexbar/config.json`
- fallback: `~/.config/codexbar/config.json`

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

Config behavior:

- missing config falls back to safe defaults
- invalid config also falls back to defaults
- current defaults are:
  - `default_source = auto`
  - `cache_ttl_seconds = 30`
  - `cache_enabled = true`

## Disk Cache

Status cache is stored under:

- `$XDG_CACHE_HOME/codexbar/`
- fallback: `~/.cache/codexbar/`

One cache file is created per source mode:

- `status-cache-auto.json`
- `status-cache-api.json`
- `status-cache-cli.json`

Cache behavior:

- fresh cache is returned when allowed
- `--refresh` forces live collection
- `--no-cache` disables cache reads and writes
- if live status collection fails and a cached snapshot exists, stale cached provider snapshots can be returned

## JSON Output

All commands return JSON.

Example status response:

```json
{
  "ok": true,
  "data": {
    "providers": {
      "ollama": {
        "provider": "ollama",
        "primary": {
          "used": 0
        },
        "secondary": {
          "used": 11
        },
        "source": "cli",
        "health": "ok",
        "stale": false
      },
      "openai": {
        "provider": "openai",
        "primary": {
          "used": 0
        },
        "source": "unknown",
        "health": "missing_credentials",
        "stale": true,
        "error": "OPENAI_API_KEY is not set"
      }
    }
  }
}
```

Example error response:

```json
{
  "ok": false,
  "data": {},
  "error": {
    "message": "provider 'x' is not available"
  }
}
```
