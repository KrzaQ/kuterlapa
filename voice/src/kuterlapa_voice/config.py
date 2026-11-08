"""Settings in ~/.config/kuterlapa/voice.toml. Every key is optional."""

import os
import tomllib
from dataclasses import dataclass, field
from pathlib import Path


def _base(variable, fallback):
    root = os.environ.get(variable) or str(Path.home() / fallback)
    return Path(root) / "kuterlapa"


CONFIG = _base("XDG_CONFIG_HOME", ".config")
STATE = _base("XDG_STATE_HOME", ".local/state")
VOICE_SOCKET = STATE / "voice.sock"
CORE_SOCKET = STATE / "core.sock"


@dataclass
class Settings:
    # "say", "command" or "mlx".
    backend: str = "say"
    # say: the macOS voice name; empty means the system default.
    say_voice: str = ""
    # command: the program and its arguments. It gets the text on stdin and
    # writes a WAV file to stdout.
    command: list[str] = field(default_factory=list)
    # mlx: a model id or folder, and optionally a reference clip to clone
    # with the text spoken in it.
    model: str = ""
    ref_audio: str = ""
    ref_text: str = ""
    # The first piece is at most this long, so speech starts soon; later
    # pieces are made while earlier ones play, so they can be longer.
    first_piece_chars: int = 80
    piece_chars: int = 300


def load():
    """Read voice.toml. Unknown keys are an error, so a typo is visible."""
    path = CONFIG / "voice.toml"
    settings = Settings()
    if not path.exists():
        return settings
    with path.open("rb") as f:
        values = tomllib.load(f)
    for key, value in values.items():
        if not hasattr(settings, key):
            raise SystemExit(f"{path}: unknown setting {key!r}")
        if isinstance(value, str):
            value = os.path.expanduser(value)
        elif isinstance(value, list):
            value = [os.path.expanduser(v) if isinstance(v, str) else v for v in value]
        setattr(settings, key, value)
    return settings
