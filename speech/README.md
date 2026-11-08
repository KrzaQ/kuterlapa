# kuterlapa-speech

A speech provider for Kuterlapa (see `docs/protocol.md`). It keeps a
Whisper model in memory, records the microphone, cuts the speech at pauses
and transcribes each piece. The Kuterlapa core types the text.

Recognition backends (`src/kuterlapa_speech/backends/`), one extra each:

| Backend | Systems | Install |
|---|---|---|
| `mlx` | Apple Silicon Macs | `kuterlapa-speech[mlx]` |
| `faster-whisper` | Linux, Windows (NVIDIA GPU or CPU); not tested yet | `kuterlapa-speech[faster-whisper]` |

The core starts the provider as a child process. To run it alone:

```bash
uv run --extra mlx kuterlapa-speech
```

Settings: `~/.config/kuterlapa/speech.toml`. See `config/speech.toml.example`
in the repository.
