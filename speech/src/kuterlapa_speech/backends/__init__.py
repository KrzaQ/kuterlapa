"""Speech recognition backends.

A backend turns a piece of audio into text. It is the only part of the
speech provider that depends on the system: recording, cutting at pauses,
dictionaries and replacements are shared.

Each backend module has:
    DEFAULT_MODEL           the model to use when speech.toml names none
    load(model) -> Backend  load the model once
and a Backend has:
    transcribe(audio, language, prompt) -> str
where audio is mono float32 numpy data at 16 kHz.
"""

import platform
import sys

BACKENDS = ("mlx", "faster-whisper")


def default_backend():
    """MLX on Apple Silicon Macs, faster-whisper anywhere else."""
    if sys.platform == "darwin" and platform.machine() == "arm64":
        return "mlx"
    return "faster-whisper"


def load(name, model):
    """Load the backend `name` with `model` (empty: the backend's default)."""
    name = name or default_backend()
    if name == "mlx":
        from . import mlx as module
    elif name == "faster-whisper":
        from . import faster_whisper as module
    else:
        raise SystemExit(f"unknown speech backend {name!r}; choose one of {', '.join(BACKENDS)}")
    return name, module.load(model or module.DEFAULT_MODEL)
