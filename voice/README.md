# kuterlapa-voice

A voice provider for Kuterlapa (see `docs/protocol.md`): it reads text
aloud. It splits the text into pieces, starts with a short first piece so
that speech begins soon, and makes the next piece while the current one
plays.

Synthesis backends (`src/kuterlapa_voice/backends/`):

| Backend | What | Install |
|---|---|---|
| `say` | the macOS `say` command | nothing extra |
| `command` | any program: text on stdin, a WAV file on stdout | nothing extra |
| `mlx` | an mlx-audio model on Apple Silicon, optionally cloning a voice from a reference clip | `kuterlapa-voice[mlx]` |

The `command` backend lets you use a speech service you already run,
without code for it in this repository.

Settings: `~/.config/kuterlapa/voice.toml`. See `config/voice.toml.example`.
