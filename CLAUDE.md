# Kuterlapa — notes for Claude

Kuterlapa lets a person control a Mac by voice and a game controller.
Claude wrote most of this code with its author, from spoken instructions.
Expect the author's messages to come from dictation, with words misheard.

## The repository will be public

- No private data in files, file names or commit messages: no host names,
  internal domains, IP addresses, home paths, people's names, recordings or
  personal settings.
- Private values belong in `~/.config/kuterlapa/`. The repository holds
  only examples (`config/*.example`) with generic values.
- `make hooks` installs a pre-commit hook (`scripts/check-private.sh`). It
  checks staged changes against private patterns kept outside the
  repository. Do not bypass it with `--no-verify`.

## Layout

- Three kinds of processes; `docs/protocol.md` defines how they talk.
  Keep logic in the core, so that shells for other systems stay thin.
- `core/`: the core `kuterlapa` (Rust): mapping, actions, dictation
  buffer, speech provider supervision, CLI. Shared by all systems.
- `Sources/Kuterlapa/`: the macOS shell (Swift, one target), grouped by
  role: `Input/` (controller), `Output/` (keys and text), `Control/` (Unix
  sockets), `App/` (start-up, core process, menu bar), `Support/`.
- `speech/`: the speech provider `kuterlapa-speech` (Python, uv). Whisper
  is one provider, not part of the core. Inside it, only
  `backends/` depends on the system (`mlx`, `faster-whisper`); keep the
  rest shared.
- `config/`: example settings. `docs/`: documentation.
- Files at run time: `~/.config/kuterlapa/` (settings),
  `~/.local/share/kuterlapa/` (recordings), `~/.local/state/kuterlapa/`
  (sockets `core.sock`, `shell.sock`, `speech.sock`).

## Build and run

- `make build`, `make install`, `make run`, `make stop`, `make logs`.
  `make help` lists all targets.
- Code signing: `SIGN_IDENTITY` in `Makefile.local`. A stable identity keeps
  the macOS permissions (Accessibility, Microphone) across rebuilds.

## Conventions

- Code, comments and documentation in English. User-facing text in the
  app may be Polish.
- Comments explain why, not what.
- Commits:
  - subject in imperative mood, under about 70 characters, with a module
    prefix: `core:`, `macos:`, `speech:`, `build:`, `docs:`;
  - the body explains why;
  - one commit per module or concern;
  - no AI attribution in commits (no `Co-Authored-By`, no tool names).
    Credit lives in `README.md`.
