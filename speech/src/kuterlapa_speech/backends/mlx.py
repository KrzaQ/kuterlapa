"""Whisper through MLX (mlx-audio). Apple Silicon Macs only.

Install with the extra: kuterlapa-speech[mlx].
"""

DEFAULT_MODEL = "mlx-community/whisper-large-v3-turbo"


class Backend:
    def __init__(self, model):
        from mlx_audio.stt.utils import load_model

        self.model = load_model(model)

    def detect(self, audio, allowed):
        """The most likely of the allowed languages for this audio."""
        import mlx.core as mx
        from mlx_audio.stt.models.whisper import audio as whisper_audio

        mel = whisper_audio.log_mel_spectrogram(
            whisper_audio.pad_or_trim(mx.array(audio)), n_mels=self.model.dims.n_mels)
        _tokens, probs = self.model.detect_language(mel)
        probs = probs[0] if isinstance(probs, list) else probs
        return max(allowed, key=lambda language: probs.get(language, 0.0))

    def transcribe(self, audio, language, prompt):
        return self.transcribe_scored(audio, language, prompt)[0]

    def transcribe_scored(self, audio, language, prompt):
        """The text, and how sure Whisper is of it: the mean log
        probability of its tokens (higher is surer)."""
        result = self.model.generate(
            audio, language=language, initial_prompt=prompt or None,
            # Against hallucinations: do not feed the model its own earlier
            # output, and skip long silent gaps.
            condition_on_previous_text=False,
            word_timestamps=True, hallucination_silence_threshold=2.0,
        )
        segments = result.segments or []
        scores = [s.get("avg_logprob", 0.0) for s in segments]
        return result.text.strip(), (sum(scores) / len(scores) if scores else -10.0)


def load(model):
    return Backend(model)
