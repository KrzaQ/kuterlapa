"""Whisper through MLX (mlx-audio). Apple Silicon Macs only.

Install with the extra: kuterlapa-speech[mlx].
"""

DEFAULT_MODEL = "mlx-community/whisper-large-v3-turbo"


class Backend:
    def __init__(self, model):
        from mlx_audio.stt.utils import load_model

        self.model = load_model(model)

    def transcribe(self, audio, language, prompt):
        result = self.model.generate(
            audio, language=language, initial_prompt=prompt,
            # Against hallucinations: do not feed the model its own earlier
            # output, and skip long silent gaps.
            condition_on_previous_text=False,
            word_timestamps=True, hallucination_silence_threshold=2.0,
        )
        return result.text.strip()


def load(model):
    return Backend(model)
