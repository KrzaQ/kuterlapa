"""A local Qwen3-Embedding model with MLX: the hidden state of the last
token (the end-of-text token), normalized to length 1."""

import json
from pathlib import Path

import mlx.core as mx
from mlx_lm.models import qwen3
from transformers import AutoTokenizer

END_OF_TEXT = 151643
MAX_TOKENS = 8192


class Backend:
    def __init__(self, folder):
        folder = Path(folder)
        args = qwen3.ModelArgs.from_dict(json.loads((folder / "config.json").read_text()))
        self.model = qwen3.Model(args)
        weights = mx.load(str(folder / "model.safetensors"))
        # The embedding checkpoint has no "model." prefix; mlx-lm expects it.
        weights = {k if k.startswith("model.") else f"model.{k}": v for k, v in weights.items()}
        self.model.load_weights(list(weights.items()), strict=False)
        mx.eval(self.model.parameters())
        self.tokenizer = AutoTokenizer.from_pretrained(str(folder))

    def embed(self, texts):
        vectors = []
        for text in texts:
            ids = self.tokenizer.encode(text)[: MAX_TOKENS - 1]
            if not ids or ids[-1] != END_OF_TEXT:
                ids.append(END_OF_TEXT)
            hidden = self.model.model(mx.array([ids]))[0, -1].astype(mx.float32)
            hidden = hidden / mx.linalg.norm(hidden)
            vectors.append(hidden.tolist())
        return vectors


def load(settings):
    if not settings.model:
        raise SystemExit("embed.toml: set model to the folder of the embedding model")
    return Backend(settings.model)
