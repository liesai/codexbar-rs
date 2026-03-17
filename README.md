# codexbar-rs

CLI Rust asynchrone avec sortie JSON et providers interchangeables.

## Ce que fait le projet

`codexbar-rs` expose une petite interface en ligne de commande pour :

- vérifier que l'application répond ;
- lister les providers disponibles ;
- exécuter un prompt via un provider donné.

La sortie est toujours renvoyée en JSON, y compris en cas d'erreur.

## Providers disponibles

- `mock` : provider local de démonstration qui simule un appel modèle et renvoie un echo enrichi ;
- `ollama` : provider HTTP qui appelle une instance Ollama sur `/api/generate`.
- `openai` : provider HTTP qui appelle l'API OpenAI sur `/chat/completions`.

## Prérequis

- Rust / Cargo installés ;
- pour `ollama`, une instance Ollama accessible, par défaut sur `http://127.0.0.1:11434`.

## Lancer le projet

```bash
cargo run -- ping
```

## Commandes utiles

Lister les providers :

```bash
cargo run -- providers
```

Tester le provider `mock` :

```bash
cargo run -- run --provider mock --prompt "bonjour le monde"
```

Tester le provider `ollama` :

```bash
cargo run -- run --provider ollama --prompt "Explique Rust en une phrase"
```

Avec surcharge du modèle ou de l'URL :

```bash
cargo run -- run --provider ollama --model llama3.2 --base-url http://127.0.0.1:11434 --prompt "Salut"
```

## Variables d'environnement

Le provider `ollama` peut aussi être configuré via :

- `OLLAMA_MODEL`
- `OLLAMA_BASE_URL`

## OpenAI Provider

Pour l'activer :

```bash
export OPENAI_API_KEY=your_api_key
```

Variables optionnelles :

- `OPENAI_BASE_URL`
- `OPENAI_MODEL`

Exemple d'usage :

```bash
cargo run -- status --json
```

La sortie JSON de `status` inclut maintenant, selon le provider, les champs `prompt_tokens`, `completion_tokens`, `total_tokens` et `source`.

## Format de sortie

Exemple de réponse réussie :

```json
{
  "ok": true,
  "data": {
    "output": "[model=mock-v1] tokens=3 echo=bonjour le monde",
    "provider": "mock"
  }
}
```

Exemple de statut provider avec champs de tokens :

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

Exemple d'erreur :

```json
{
  "ok": false,
  "data": {},
  "error": {
    "message": "provider 'x' is not available"
  }
}
```
