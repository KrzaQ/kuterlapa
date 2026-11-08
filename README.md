# Kuterlapa

Control a Mac with your voice and a game controller, with as little typing
and clicking as possible.

Kuterlapa is a menu bar app. A controller button starts dictation; local
Whisper turns speech into text, sentence by sentence, and Kuterlapa types it
into the active window. Other buttons send keys (Enter, Escape, Page Up,
switching desktops) or run commands. The mapping is a JSON file.

Status: early, personal project. Planned next: an overlay that shows the
dictated text and the answers of a language model (Claude Code or pi with a
local model), drafts that the model edits on voice commands, and sessions
tied to projects.

## Parts

Three kinds of processes, so each part can be replaced on its own
(`docs/protocol.md`):

- `core/`: `kuterlapa`, the core (Rust). The same on every system. It maps
  buttons to actions and sends dictated text where it belongs. The same
  binary is also the command-line client (`kuterlapa press A`,
  `kuterlapa status`).
- `Sources/Kuterlapa/`: the macOS shell, `Kuterlapa.app` (Swift, AppKit):
  - `Input/`: game controller buttons,
  - `Output/`: keystrokes and typed text,
  - `Control/`: Unix sockets,
  - `App/`: start-up, the core process, the menu bar icon.
  Shells for Linux and Windows can follow; the core does not change.
- `speech/`: `kuterlapa-speech`, a speech provider (Python, MLX Whisper).
  Any program that follows the speech protocol can replace it.
- `config/`: example settings.

## Requirements

- macOS 14 or later, Apple Silicon (MLX).
- Xcode command-line tools (Swift 5.10 or later).
- Rust (cargo).
- [uv](https://docs.astral.sh/uv/).
- A game controller that macOS supports (for example a Nintendo Switch Pro
  Controller).

## Install

```bash
make install
make run
```

`make install` builds `~/Applications/Kuterlapa.app` (with the core
inside), links the `kuterlapa` command into `~/.local/bin`, installs
`kuterlapa-speech` with `uv tool`, and copies the example settings to
`~/.config/kuterlapa/` (only files that do not exist yet).

On first start macOS asks for two permissions:

- **Accessibility**, to type text and send keys.
- **Microphone**, for dictation.

macOS ties permissions to the app's code signature. With the default ad hoc
signature it forgets them after every rebuild. To keep them, sign with a
stable identity: see `Makefile.local.example`.

## Settings

All in `~/.config/kuterlapa/`:

- `config.toml`: which speech provider the core starts.
- `mapping.json`: button → action. An action is a key combination
  (`"Return"`, `"ctrl+Left"`), `"speech:toggle"`, `"erase-dictation|Escape"`
  (erase the text of the last dictation, or press Escape),
  `"finish-dictation|Return"` (end the dictation, wait for its last words,
  then press Enter), or `"cmd:<shell command>"`. The app reads the file again when it changes.
- `speech.toml`: Whisper model, language, context, microphone.
- `dictionary.txt`: words that Whisper should recognize (project and host
  names, tools). Most important first; Whisper reads about 200 tokens.
- `replacements.txt`: fixes after recognition, `wrong => right`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- MIT license ([LICENSE-MIT](LICENSE-MIT)),

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or
conditions.

## Credits

Written by krzaq together with Claude (Anthropic), through Claude Code. Claude designed and wrote most of the code from spoken instructions,
which is how this project is meant to be used.
