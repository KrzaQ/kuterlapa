# Protocol between the parts

Kuterlapa has three kinds of parts. Each runs as its own process, so each
can be replaced without touching the others:

- **Core** (`kuterlapa`, Rust): the same on every system. It decides what a
  button does and where dictated text goes.
- **Platform shell** (on macOS: `Kuterlapa.app`, Swift): reads the
  controller, types text, presses keys, holds the system permissions. It
  starts the core.
- **Speech provider** (default: `kuterlapa-speech`, Python with MLX
  Whisper): records speech and turns it into text. The core starts it. Any
  program that follows the speech part of this protocol works.

They talk over Unix sockets in `~/.local/state/kuterlapa/` (or
`$XDG_STATE_HOME/kuterlapa/`). Each connection carries one request: one line
of UTF-8 text. The server answers with one line and closes the connection.
An answer is `ok`, a status line, or `error: <message>`.

## Core: `core.sock`

| Request | From | Meaning |
|---|---|---|
| `button <name>` | shell | A controller button was pressed. Names: `A B X Y L R ZL ZR + - Share Up Down Left Right`. |
| `dictate <text>` | speech provider | Type the text, and remember it as dictation so that `erase-dictation` can remove it. |
| `speech <request>` | CLI | Pass the request to the speech provider. |
| `type <text>` | CLI | Type the text. |
| `key <combo>` | CLI | Press a key combination. |
| `status` | CLI | State of the parts. |

## Platform shell: `shell.sock`

| Request | Meaning |
|---|---|
| `type <text>` | Type the text into the active window. |
| `key <combo>` | Press a key combination, for example `Return` or `ctrl+Left`. |
| `erase <n>` | Press Backspace `n` times. |
| `status` | State of the shell, for example its permissions. |

Key combinations: modifiers `cmd`, `ctrl`, `alt` (`option`), `shift`, and
one key, joined with `+`. Key names: letters, digits, `Return`, `Tab`,
`Space`, `Backspace`, `Escape`, `Left`, `Right`, `Up`, `Down`, `PageUp`,
`PageDown`, `Home`, `End`, `F1`…`F20`. Case does not matter.

## Speech provider: `speech.sock`

| Request | Meaning |
|---|---|
| `toggle` | Start recording, or stop it. |
| `cancel` | Stop recording. Drop the text not sent yet. |
| `finish` | Stop recording and answer only after the last text is sent: `stopped`, or `idle` if there was no recording. |

The provider sends recognized text to the core as `dictate <text>`, piece
by piece, while the user speaks.

## Process lifetime

The parent starts the child with a pipe on its standard input and never
writes to it. When the parent exits, the pipe closes, and the child exits
too. So no part outlives the one that started it.
