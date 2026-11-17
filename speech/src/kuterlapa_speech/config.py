"""Paths and settings.

Files follow the XDG base directory layout:
    ~/.config/kuterlapa/speech.toml        settings (all optional)
    ~/.config/kuterlapa/dictionary.txt     words for Whisper's context
    ~/.config/kuterlapa/replacements.txt   fixes applied after recognition
    ~/.config/kuterlapa/sessions/<name>/   a session's own dictionary.txt and
                                           replacements.txt, used first
    ~/.config/kuterlapa/hallucinations/    <language>.txt: more phrases that
                                           Whisper invents on silence
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
    # The language of the speech (ISO 639-1, such as "pl"). Empty: Whisper
    # detects it in each piece.
    language: str = ""
    # The languages the user speaks, the main one first, for example
    # ["pl", "en"]. Whisper then detects the language of each piece and
    # takes the most likely of these, so it never writes another language
    # (Chinese letters in noise), and English speech is not translated into
    # Polish, as a fixed `language` does. It replaces `language`.
    languages: list[str] = field(default_factory=list)
    # The first sentence of Whisper's context: the language and style of
    # the speech, written in that language. Empty: none.
    prompt_intro: str = ""
    # Input device name; empty means the system default.
    microphone: str = ""
    # Close the microphone after this many seconds without a recording, so
    # macOS stops showing its microphone indicator. 0 keeps it open. While
    # it is open, the first words before a button press are kept too.
    microphone_idle_close: float = 30.0
    # A recording started with "start-auto" (a chat message) ends by itself
    # after this much silence following speech. After `countdown_after`
    # seconds of silence the core shows a countdown.
    auto_finish: float = 3.0
    countdown_after: float = 1.0
    # ...but never before this many seconds from the start of the recording.
    auto_finish_min: float = 10.0
    # Word lists for Whisper's context, most important first.
    dictionaries: list[str] = field(default_factory=lambda: [str(CONFIG / "dictionary.txt")])
    replacements: str = str(CONFIG / "replacements.txt")
    recordings: str = str(DATA / "recordings")
    # Sounds at start, stop and on errors. Empty disables a sound.
    sound_start: str = "/System/Library/Sounds/Tink.aiff"
    sound_stop: str = "/System/Library/Sounds/Pop.aiff"
    sound_error: str = "/System/Library/Sounds/Basso.aiff"


def ui_polish():
    """Whether Kuterlapa speaks Polish to the user: `language = "pl"` in
    the core's config.toml. English otherwise."""
    try:
        with (CONFIG / "config.toml").open("rb") as f:
            return str(tomllib.load(f).get("language", "")).strip() == "pl"
    except (OSError, tomllib.TOMLDecodeError):
        return False


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
