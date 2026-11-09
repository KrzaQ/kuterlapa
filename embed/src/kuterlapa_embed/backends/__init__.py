"""Embedding backends.

Each backend module has load(settings) -> Backend, and a Backend has
embed(texts) -> list of vectors (lists of floats, length 1 each).
"""

BACKENDS = ("mlx", "openai")


def load(settings):
    if settings.backend == "mlx":
        from . import mlx as module
    elif settings.backend == "openai":
        from . import openai as module
    else:
        raise SystemExit(f"unknown embed backend {settings.backend!r}; choose one of {', '.join(BACKENDS)}")
    return module.load(settings)
