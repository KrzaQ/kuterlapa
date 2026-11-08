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
| `press <name>` | shell | A controller button went down. Names: `A B X Y L R ZL ZR + - Share Up Down Left Right L3 R3`. |
| `release <name>` | shell | The button went up. The core turns presses and releases into gestures: a single press, a double press (`X double` in mapping.json) or a hold (`Y hold`). |
| `button <key>` | CLI | Run the action for a key directly, without gestures: `button A`, `button X double`. |
| `dictate <text>` | speech provider | Type the text, and remember it as dictation so that `erase-dictation` can remove it. |
| `state <what> on\|off` | providers | The speech provider reports `listening`, the voice provider `speaking`. The core runs its hooks (config.toml `[hooks]`, see `core/src/hooks.rs`). |
| `speech <request>` | CLI | Pass the request to the speech provider. |
| `say <text>` | CLI | Read the text aloud with the voice provider. |
| `voice <request>` | CLI | Pass the request (`stop`, `status`) to the voice provider. |
| `type <text>` | CLI | Type the text. |
| `key <combo>` | CLI | Press a key combination. |
| `status` | CLI | State of the parts. |

## Platform shell: `shell.sock`

| Request | Meaning |
|---|---|
| `type <text>` | Type the text into the active window. |
| `key <combo>` | Press a key combination, for example `Return` or `ctrl+Left`. |
| `erase <n>` | Press Backspace `n` times. |
| `mouse <what>` | `click`, `double-click` or `right-click` at the pointer. |
| `sticks <json>` | What the analog sticks do, for example `{"leftstick": "scroll", "rightstick": "mouse"}`. The shell moves the pointer and scrolls by itself. |
| `overlay <json>` | Show the overlay with this state, or hide it (`{"visible": false}`). |
| `frontmost` | The identifier of the application in front (a bundle id on macOS). |
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
by piece, while the user speaks. It sends `state listening on` when
recording starts and `state listening off` when it stops.

## Voice provider: `voice.sock`

Reads text aloud. Default: `kuterlapa-voice`, with the backends `say`,
`command` and `mlx`.

| Request | Meaning |
|---|---|
| `say <text>` | Read the text aloud, after anything still being read. Answers at once. |
| `stop` | Stop reading and drop what is waiting. |
| `status` | `speaking` or `idle`. |

The provider sends `state speaking on` to the core when it starts to read,
and `state speaking off` when nothing is left to read.

## Process lifetime

The parent starts the child with a pipe on its standard input and never
writes to it. When the parent exits, the pipe closes, and the child exits
too. So no part outlives the one that started it.
