# codexbar-rs

Asynchronous Rust CLI with JSON output and interchangeable providers.

## What The Project Does

`codexbar-rs` exposes a small command-line interface to:

- verify that the application responds;
- list available providers;
- execute a prompt through a selected provider.

Output is always returned as JSON, including errors.

## Available Providers

- `mock`: local demo provider that simulates a model call and returns an enriched echo;
- `ollama`: HTTP provider that calls an Ollama instance on `/api/generate`;
- `openai`: HTTP provider that calls the OpenAI API on `/chat/completions`.

## Prerequisites

- Rust / Cargo installed;
- for `ollama`, an accessible Ollama instance, defaulting to `http://127.0.0.1:11434`.

## Run The Project

```bash
cargo run -- ping
```

## Useful Commands

List providers:

```bash
cargo run -- providers
```

Test the `mock` provider:

```bash
cargo run -- run --provider mock --prompt "bonjour le monde"
```

Test the `ollama` provider:

```bash
cargo run -- run --provider ollama --prompt "Explique Rust en une phrase"
```

Override the model or base URL:

```bash
cargo run -- run --provider ollama --model llama3.2 --base-url http://127.0.0.1:11434 --prompt "Salut"
```

## Environment Variables

The `ollama` provider can also be configured with:

- `OLLAMA_MODEL`
- `OLLAMA_BASE_URL`

## OpenAI Provider

To enable it:

```bash
export OPENAI_API_KEY=your_api_key
```

Optional variables:

- `OPENAI_BASE_URL`
- `OPENAI_MODEL`

Usage example:

```bash
cargo run -- status --json
```

Select a source strategy explicitly:

```bash
cargo run -- status --json --source auto
cargo run -- status --json --source api
cargo run -- status --json --source cli
```

`auto` currently uses the provider default strategy. `ollama` now has a real CLI-backed collector that reads `ollama ps` and enriches the snapshot with `ollama ls`. `openai --source cli` still reports a degraded snapshot until a real CLI collector is added.

By default, `status` also uses:

- a user config file at `~/.config/codexbar/config.json` (or `$XDG_CONFIG_HOME/codexbar/config.json`)
- a disk cache at `~/.cache/codexbar/` (or `$XDG_CACHE_HOME/codexbar/`)

Minimal config example:

```json
{
  "status": {
    "default_source": "auto",
    "cache_ttl_seconds": 30,
    "cache_enabled": true
  }
}
```

Useful cache controls:

```bash
cargo run -- status --json --refresh
cargo run -- status --json --no-cache
```

Diagnostics:

```bash
cargo run -- config path
cargo run -- doctor --json
cargo run -- doctor --source cli --json
```

`doctor` reports resolved config/cache paths, cache state, `ollama` CLI availability, and the effective support level of each provider source.

The `status` JSON output now exposes a richer usage snapshot for each provider, including `primary`, `health`, `source`, `stale`, and, when available, `prompt_tokens`, `completion_tokens`, and `total_tokens`.

## Output Format

Example successful response:

```json
{
  "ok": true,
  "data": {
    "output": "[model=mock-v1] tokens=3 echo=bonjour le monde",
    "provider": "mock"
  }
}
```

Example provider status with usage snapshot fields:

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
