# codexbar-rs

`codexbar-rs` is an asynchronous Rust CLI focused on provider status and usage collection, with JSON output, source-aware collection, persisted status configuration, and disk-backed caching.

The project currently provides:

- provider status snapshots through `status`
- source selection with `auto`, `api`, and `cli`
- a real CLI-backed `ollama` status collector
- a real CLI-backed `codex` status collector
- OpenAI organization usage collection
- persisted config and disk cache for status
- local diagnostics through `doctor`
- an experimental native GUI shell

## Current Scope

This repository is not a full Linux port of the upstream macOS product yet. At the moment it is a CLI-first foundation focused on:

- provider abstraction
- structured JSON output
- status snapshot modeling
- source routing and fallback behavior
- local observability and diagnostics

## Architecture

The CLI is now a thin façade over a reusable backend layer.

Current backend entry points are centered around:

- status collection
- diagnostics
- config path resolution
- cache-aware status retrieval

This is intended to make a future Linux GUI reuse the same provider, cache, and doctor logic instead of reimplementing it.

## GUI (Experimental)

A first native GUI shell is available with `egui/eframe`.

Run it with:

```bash
cargo run --bin codexbar-gui
```

Current GUI scope:

- provider selector
- source selector
- manual refresh
- `no cache` toggle
- status cards
- doctor panel
- config path display

## Requirements

- Rust and Cargo
- for `ollama` API usage: a reachable Ollama instance, defaulting to `http://127.0.0.1:11434`
- for `ollama` CLI status collection: a working `ollama` binary in `PATH`
- for `codex` CLI status collection: a working `codex` binary in `PATH`
- for `openai` organization usage: `OPENAI_ADMIN_KEY` or `OPENAI_API_KEY`

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
- `codex`
- `ollama`
- `openai`

### Status

Collect provider status snapshots.

```bash
cargo run -- status --json
```

Restrict status collection to a single provider:

```bash
cargo run -- status --json --provider ollama
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
- `codex`: real CLI collection via local Codex auth and app-server account status
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
- `codex` CLI availability
- `ollama` CLI availability
- whether OpenAI credentials are set, with an admin-key warning when relevant
- provider capability summary
- explicit warning that `codex --source api` is not implemented
- explicit warning that `openai --source cli` is not implemented

## Backend Layer

The project now includes a reusable Rust backend layer under [src/backend/mod.rs](/home/marc/codexbar-rs/src/backend/mod.rs) intended to be the future integration point for a Linux GUI.

Current backend entry points:

- `get_status(...)`
- `get_doctor(...)`
- `get_config_path()`
- `get_provider_names()`

The CLI is meant to stay as a thin adapter over this backend instead of being the primary home for product logic.

## Provider Details

### Mock

`mock` is a local demo provider. It returns predictable status data useful for development.

### Codex

`codex` supports:

- CLI-backed account status collection
- CLI-backed rate limit collection when exposed by the local Codex app-server

CLI status collection uses:

- `codex app-server --listen stdio://`
- the local auth file at `$CODEX_HOME/auth.json`
- fallback auth path: `~/.codex/auth.json`

With `--source cli` or `--source auto`, the `codex` snapshot currently reports:

- local Codex authentication availability
- account email when exposed by `account/read`
- plan type when exposed by `account/read`
- auth mode from the local auth file
- `primary` and `secondary` quota windows when `account/rateLimits/read` is available
- `updated_at` from the local auth file when available

`codex --source api` is not implemented. The command returns a degraded snapshot intentionally rather than pretending API support exists.

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

- API-backed organization usage collection over the last 24 hours

Environment variables:

- `OPENAI_ADMIN_KEY` (preferred for the organization usage endpoint)
- `OPENAI_API_KEY` (accepted, but organization usage endpoints may reject non-admin keys)
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

When `--provider <name>` is used, a provider-specific cache file is used for that request.

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
      "codex": {
        "account": "user@example.com",
        "auth_mode": "chatgpt",
        "plan": "plus",
        "provider": "codex",
        "primary": {},
        "source": "cli",
        "health": "ok",
        "updated_at": "2026-03-11T15:16:38.974908875Z",
        "stale": false
      },
      "openai": {
        "provider": "openai",
        "primary": {},
        "source": "unknown",
        "health": "missing_credentials",
        "stale": true,
        "error": "OPENAI_ADMIN_KEY or OPENAI_API_KEY is not set"
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
