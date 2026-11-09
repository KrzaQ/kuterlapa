# kuterlapa-embed

The embedding provider for Kuterlapa. It turns text into vectors, so the
models can search notes and conversations by meaning (`memory_search`).

Backends, set in `~/.config/kuterlapa/embed.toml`:

- `mlx`: a local Qwen3-Embedding model on Apple Silicon (install with the
  `mlx` extra).
- `openai`: any server with the OpenAI embeddings API, such as a hosted
  provider, Ollama or llama.cpp.

See `config/embed.toml.example` in the repository.
