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

The `status` JSON output now includes, depending on the provider, the `prompt_tokens`, `completion_tokens`, `total_tokens`, and `source` fields.

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

Example provider status with token fields:

```json
{
  "ok": true,
  "data": {
    "providers": {
      "openai": {
        "used": 42,
        "limit": 0,
        "prompt_tokens": 12,
        "completion_tokens": 30,
        "total_tokens": 42,
        "source": "openai/chat_completions"
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
