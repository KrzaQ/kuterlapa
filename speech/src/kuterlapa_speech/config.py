"""Paths and settings.

Files follow the XDG base directory layout:
    ~/.config/kuterlapa/speech.toml        settings (all optional)
    ~/.config/kuterlapa/dictionary.txt     words for Whisper's context
    ~/.config/kuterlapa/replacements.txt   fixes applied after recognition
    ~/.local/share/kuterlapa/recordings/   recordings and their text
    ~/.local/state/kuterlapa/              sockets
"""

import os
import tomllib
from dataclasses import dataclass, field
from pathlib import Path


def _base(variable, fallback):
    root = os.environ.get(variable) or str(Path.home() / fallback)
    return Path(root) / "kuterlapa"


CONFIG = _base("XDG_CONFIG_HOME", ".config")
DATA = _base("XDG_DATA_HOME", ".local/share")
STATE = _base("XDG_STATE_HOME", ".local/state")

SPEECH_SOCKET = STATE / "speech.sock"
CORE_SOCKET = STATE / "core.sock"


@dataclass
class Settings:
    # Recognition backend: "mlx" or "faster-whisper". Empty: MLX on Apple
    # Silicon Macs, faster-whisper elsewhere.
    backend: str = ""
    # A model id or a local folder with a model for the backend. Empty:
    # the backend's default (Whisper large-v3-turbo).
    model: str = ""
    language: str = "pl"
    # The first sentence of Whisper's context. It tells Whisper the
    # language and style of the speech.
    prompt_intro: str = "Rozmawiamy po polsku z angielskimi terminami IT."
    # Input device name; empty means the system default.
    microphone: str = ""
    # Close the microphone after this many seconds without a recording, so
    # macOS stops showing its microphone indicator. 0 keeps it open. While
    # it is open, the first words before a button press are kept too.
    microphone_idle_close: float = 30.0
    # Word lists for Whisper's context, most important first.
    dictionaries: list[str] = field(default_factory=lambda: [str(CONFIG / "dictionary.txt")])
    replacements: str = str(CONFIG / "replacements.txt")
    recordings: str = str(DATA / "recordings")
    # Sounds at start, stop and on errors. Empty disables a sound.
    sound_start: str = "/System/Library/Sounds/Tink.aiff"
    sound_stop: str = "/System/Library/Sounds/Pop.aiff"
    sound_error: str = "/System/Library/Sounds/Basso.aiff"


def load():
    """Read speech.toml. Unknown keys are an error, so a typo is visible."""
    path = CONFIG / "speech.toml"
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
