"""Settings in ~/.config/kuterlapa/embed.toml. Every key is optional."""

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path


def _base(variable, fallback):
    root = os.environ.get(variable) or str(Path.home() / fallback)
    return Path(root) / "kuterlapa"


CONFIG = _base("XDG_CONFIG_HOME", ".config")
STATE = _base("XDG_STATE_HOME", ".local/state")
EMBED_SOCKET = STATE / "embed.sock"


@dataclass
class Settings:
    # "mlx" (a local model) or "openai" (an OpenAI-compatible server).
    backend: str = "mlx"
    # mlx: the model folder (Hugging Face format), for example
    # Qwen3-Embedding-0.6B. openai: the model name on the server.
    model: str = ""
    # openai: the server's base URL, for example https://api.openai.com/v1
    # or http://localhost:11434/v1, and the environment variable that holds
    # the API key (empty: no key).
    url: str = ""
    api_key_env: str = ""
    # Put before a search query (Qwen3-Embedding wants an instruction);
    # documents get no prefix.
    query_prefix: str = (
        "Instruct: Given a search query, retrieve notes and messages that answer it\nQuery:"
    )


def load():
    """Read embed.toml. Unknown keys are an error, so a typo is visible."""
    path = CONFIG / "embed.toml"
    settings = Settings()
    if not path.exists():
        return settings
    with path.open("rb") as f:
        values = tomllib.load(f)
    for key, value in values.items():
        if not hasattr(settings, key):
            raise SystemExit(f"{path}: unknown setting {key!r}")
        if isinstance(value, str) and key in ("model",):
            value = os.path.expanduser(value)
        setattr(settings, key, value)
    return settings
