"""Whisper through faster-whisper (CTranslate2). Linux and Windows, on an
NVIDIA GPU (CUDA) or the CPU.

Install with the extra: kuterlapa-speech[faster-whisper].
Not tested yet: the project so far runs on macOS with the MLX backend.
"""

DEFAULT_MODEL = "large-v3-turbo"


class Backend:
    def __init__(self, model):
        from faster_whisper import WhisperModel

        # "auto" picks CUDA when it is there, and a fitting number format.
        self.model = WhisperModel(model, device="auto", compute_type="auto")

    def transcribe(self, audio, language, prompt):
        segments, _info = self.model.transcribe(
            audio, language=language, initial_prompt=prompt,
            # The same protection against hallucinations as the MLX backend.
            condition_on_previous_text=False,
            word_timestamps=True, hallucination_silence_threshold=2.0,
        )
        return "".join(segment.text for segment in segments).strip()


def load(model):
    return Backend(model)
