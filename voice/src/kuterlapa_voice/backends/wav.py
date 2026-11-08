"""Read WAV data into mono float32."""

import io
import wave

import numpy as np


def decode(data):
    with wave.open(io.BytesIO(data)) as w:
        rate, channels, width = w.getframerate(), w.getnchannels(), w.getsampwidth()
        frames = w.readframes(w.getnframes())
    if width != 2:
        raise ValueError(f"only 16-bit WAV is supported, got {8 * width}-bit")
    audio = np.frombuffer(frames, dtype="<i2").astype(np.float32) / 32768
    if channels > 1:
        audio = audio.reshape(-1, channels).mean(axis=1)
    return audio, rate
