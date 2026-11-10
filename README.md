# Kuterlapa

Control a Mac with your voice and a game controller, with as little typing
and clicking as possible.

Kuterlapa is a menu bar app. A controller button starts dictation; local
Whisper turns speech into text, sentence by sentence, and Kuterlapa types it
into the active window. Other buttons talk to a language model (Claude
through Claude Code, or a local model through pi), edit a draft by voice
before it is sent, read answers aloud, or send keys and run commands. The
mapping is a JSON file.

The models get tools: they can see and use the computer (screenshots, the
accessibility tree, clicks, keys, windows and desktops), call your own
programs described by small TOML files, keep notes for later and search
notes and past conversations by meaning. A tool can ask you first, and
you answer with a button.

**[See the tour with screenshots](docs/tour.md).**

Status: a personal project, used every day by its first user, on macOS.

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
  - `Control/`: Unix sockets, and what the models see and do on screen,
  - `Overlay/`, `View/`: the overlay and the chat view,
  - `App/`: start-up, the core process, the menu bar icon.
  Shells for Linux and Windows can follow; the core does not change.
- `speech/`: `kuterlapa-speech`, a speech provider (Python, MLX Whisper).
  Any program that follows the speech protocol can replace it.
- `voice/`: `kuterlapa-voice`, a voice provider that reads text aloud
  (`kuterlapa say "…"`). Backends: macOS `say`, any command, or a local
  mlx-audio model that can clone a voice from a reference clip.
- `embed/`: `kuterlapa-embed`, an embedding provider for search by
  meaning. Backends: a local Qwen3-Embedding model (MLX) or any server
  with the OpenAI embeddings API.
- `view/`: the chat view, a web page inside the app (marked, KaTeX and
  highlight.js are included, so it works offline).
- `tools/macos/`: tools for the models on macOS: Chrome tabs and profiles,
  iTerm tabs.
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

- `config.toml`: which speech and voice providers the core starts.
- `profiles.json`: model profiles (engine, model, effort) that L and ZL
  switch between. `"local": true` marks a model that runs on your own
  machines. Only such profiles get the tools marked `private = true`.
  The engine does not decide it: `pi` can also reach hosted models.
  `"shell": false` takes the shell away from a profile's model; by default
  it may run commands with its engine's own tool (Claude Code's `Bash`,
  pi's `bash`), which the engine's permission classifier judges.
- `tools/*.toml`: your own tools for the models. A session's own tools are
  in its `tools/` folder.
- `sessions/<name>/`: a session's `context.md` (what the model should
  know) and `session.toml` (`folder`: the project folder the model runs in).
- History of conversations and one-off commands:
  `~/.local/share/kuterlapa/kuterlapa.db` (SQLite). The same file keeps
  the token statistics (table `usage`): one row for each model in each
  request, with pi-censor's classifier calls apart. `kuterlapa stats
  [today|week|month|all]` prints them.
- `mapping.json`: button → action. A button can also have a double press
  (`"X double"`) and a hold (`"Y hold"`); only buttons that have them wait
  to tell the gestures apart. `"LeftStick"` and `"RightStick"` take
  `"scroll"`, `"scroll-inverted"` or `"mouse"`. Use `"scroll-inverted"`
  when a tool such as Scroll Reverser flips mouse wheel events: it takes
  the stick for a mouse. An action is `"dictate"`, `"draft"`,
  `"chat"`, `"command"`, `"voice-toggle"`, `"recall"`, `"next-profile"`,
  `"help"`, `"sessions"`, `"view"`, `"usage"` (today's token statistics),
  `"claude-ask"` (answer the question of the Claude Code in iTerm's front
  tab, in the overlay, without a model),
  `"previous-profile"`, `"click"`, `"double-click"`, `"right-click"`, a key combination
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
