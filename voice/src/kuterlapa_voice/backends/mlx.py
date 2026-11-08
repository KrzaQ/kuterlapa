"""An mlx-audio TTS model on Apple Silicon, in this process.

With ref_audio and ref_text, the model clones the voice of the reference
clip (models that support it, for example Fish Audio Drama 3 or Qwen3-TTS
Base). Install with the extra: kuterlapa-voice[mlx].
"""

import numpy as np


class Backend:
    def __init__(self, settings):
        from mlx_audio.tts.utils import load_model

        if not settings.model:
            raise SystemExit("voice backend 'mlx' needs a model in voice.toml")
        self.model = load_model(settings.model)
        self.kw = {}
        if settings.ref_audio:
            self.kw["ref_audio"] = settings.ref_audio
            self.kw["ref_text"] = _read_text(settings.ref_text)

    def synthesize(self, text):
        chunks = [np.asarray(r.audio, dtype=np.float32) for r in self.model.generate(text=text, **self.kw)]
        audio = np.concatenate(chunks) if chunks else np.zeros(0, np.float32)
        return audio, self.model.sample_rate


def _read_text(value):
    """ref_text is the text itself, or a path to a file with it."""
    try:
        with open(value, encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return value


def load(settings):
    return Backend(settings)
