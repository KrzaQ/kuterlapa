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

    def detect(self, audio, allowed):
        """The most likely of the allowed languages for this audio."""
        _language, _probability, probs = self.model.detect_language(audio)
        probs = dict(probs)
        return max(allowed, key=lambda language: probs.get(language, 0.0))

    def transcribe(self, audio, language, prompt):
        return self.transcribe_scored(audio, language, prompt)[0]

    def transcribe_scored(self, audio, language, prompt):
        """The text, and how sure Whisper is of it: the mean log
        probability of its tokens (higher is surer)."""
        segments, _info = self.model.transcribe(
            audio, language=language, initial_prompt=prompt or None,
            # The same protection against hallucinations as the MLX backend.
            condition_on_previous_text=False,
            word_timestamps=True, hallucination_silence_threshold=2.0,
        )
        segments = list(segments)
        scores = [segment.avg_logprob for segment in segments]
        text = "".join(segment.text for segment in segments).strip()
        return text, (sum(scores) / len(scores) if scores else -10.0)


def load(model):
    return Backend(model)
