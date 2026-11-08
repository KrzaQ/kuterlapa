"""Speech synthesis backends.

Each backend module has load(settings) -> Backend, and a Backend has
synthesize(text) -> (audio, rate): mono float32 numpy data and its sample
rate.
"""

BACKENDS = ("say", "command", "mlx")


def load(settings):
    name = settings.backend
    if name == "say":
        from . import say as module
    elif name == "command":
        from . import command as module
    elif name == "mlx":
        from . import mlx as module
    else:
        raise SystemExit(f"unknown voice backend {name!r}; choose one of {', '.join(BACKENDS)}")
    return module.load(settings)
