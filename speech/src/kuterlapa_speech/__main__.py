"""kuterlapa-speech: a speech provider for Kuterlapa, with local Whisper.

The recognition backend (MLX, faster-whisper) is a setting; see backends/.

It implements the speech protocol (docs/protocol.md). It keeps the model in
memory and listens on ~/.local/state/kuterlapa/speech.sock. Requests, one
line per connection:

    toggle   start recording, or stop it
    start-auto
             start recording that ends by itself after a pause (a chat
             message); the core gets "countdown <seconds>" when the pause
             begins, "countdown cancel" when speech returns, and
             "auto-finish" when the pause is long enough
    cancel   stop recording and drop the pieces not typed yet
    finish   stop recording and answer once the last piece is sent:
             "stopped", or "idle" if it was not recording
    status   "recording" or "idle"
    devices  the microphones, as JSON: {"system": <the system's default
             input>, "chosen": <the chosen one, "" to follow the system>,
             "inputs": [<names>]}
    microphone <name>
             record from this microphone from now on; without a name,
             follow the system's default input again. The choice is kept
             in ~/.local/state/kuterlapa/microphone.

While recording, it cuts the audio at pauses in speech. Each piece goes to
Whisper at once, and the text goes to the Kuterlapa core
(request "dictate <text>" on ~/.local/state/kuterlapa/core.sock). It also
tells the core when recording starts and stops ("state listening on",
"state listening off").

The core starts the provider with a pipe on stdin. When the pipe closes,
the provider exits.
"""

import collections
import datetime
import json
import os
import queue
import socket
import subprocess
import sys
import threading
import time
import wave
from pathlib import Path

import numpy as np
import sounddevice as sd

from . import backends, config
from .segmenter import FRAME, RATE, Segmenter
from .text import build_prompt, clean


def log(msg):
    print(f"{datetime.datetime.now():%H:%M:%S} {msg}", flush=True)


def play(sound):
    if sound:
        subprocess.Popen(["afplay", sound], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def core_request(line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(10)
        s.connect(str(config.CORE_SOCKET))
        s.sendall((line.replace("\n", " ") + "\n").encode())
        return s.recv(1024).decode().strip()


_states = queue.Queue()


def _send_states():
    while True:
        line = _states.get()
        try:
            core_request(line)
        except OSError as e:
            log(f"core: {e}")


threading.Thread(target=_send_states, daemon=True).start()


def tell_core(line):
    """Send a request to the core from the reporting thread, in order."""
    _states.put(line)


# The user's choice of microphone: a name, or empty to follow the system.
MICROPHONE_FILE = config.STATE / "microphone"


def inputs():
    """The names of the input devices, and the system's default input."""
    names = [d["name"] for d in sd.query_devices() if d["max_input_channels"] > 0]
    try:
        system = sd.query_devices(kind="input")["name"]
    except Exception:  # noqa: BLE001 - no default input at all
        system = ""
    return names, system


# Below this probability of the main language, a piece is also read in the
# next likeliest one. Polish pieces scored 0.71 to 1.0; an English one that
# Whisper took for Polish, 0.45.
SURE_OF_MAIN = 0.65


def report(listening):
    """Tell the core that recording started or stopped (it runs hooks).

    One thread sends the reports in order, so recording never waits for the
    core, and "off" never overtakes "on".
    """
    _states.put(f"state listening {'on' if listening else 'off'}")


def session_dictionaries(settings):
    """The current session's word list first, then the global ones.

    The core keeps the current session's name in the state folder and picks
    it before a recording starts.
    """
    try:
        session = (config.STATE / "session").read_text(encoding="utf-8").strip()
    except OSError:
        session = ""
    own = config.CONFIG / "sessions" / session / "dictionary.txt" if session else None
    if own is not None and own.is_file():
        return [str(own)] + [d for d in settings.dictionaries if d != str(own)]
    return list(settings.dictionaries)


def session_replacements(settings):
    """The current session's fixes first, then the global ones."""
    try:
        session = (config.STATE / "session").read_text(encoding="utf-8").strip()
    except OSError:
        session = ""
    own = config.CONFIG / "sessions" / session / "replacements.txt" if session else None
    if own is not None and own.is_file():
        return [str(own), settings.replacements]
    return [settings.replacements]


def speech_context():
    """What the user is likely to talk about: the core writes the window in
    front, the last messages and the session's description to the state
    folder before a recording starts."""
    try:
        return (config.STATE / "speech-context.txt").read_text(encoding="utf-8")
    except OSError:
        return ""


PRE_ROLL_FRAMES = 10  # 0.3 s of audio from before the button press


CLOSE_TIMEOUT_S = 3.0

# Auto-finish (start-auto): ignore this much audio at the start, where the
# start sound plays, and count a pause only after this much speech.
START_SOUND_S = 0.5
ARM_SPEECH_S = 0.6


class Dictation:
    """Records while the user dictates.

    The microphone stream stays open between recordings, and a recording
    only decides whether its frames are kept. An open stream keeps the
    first words: frames from just before the press go into the recording.

    After `microphone_idle_close` seconds without a recording, the stream
    closes, so macOS stops showing its microphone indicator. Closing a
    stream on macOS can deadlock inside CoreAudio (seen 2026-11-08:
    Pa_StopStream waited on a HAL mutex that the audio thread held). So
    the stream never closes while a request waits for it: a separate
    thread closes it, and if that hangs, the provider exits and the core
    starts a fresh one.
    """

    def __init__(self, backend, settings):
        self.backend = backend
        self.settings = settings
        self.lock = threading.Lock()
        # Closing a stream and opening one never run at the same time.
        # Opening restarts PortAudio, and a new stream can get the old one's
        # address: a late close of the old stream then closed the new one,
        # and every recording after it failed ("Invalid stream pointer").
        self.audio_lock = threading.Lock()
        # Streams given to a closing thread and not closed yet. PortAudio
        # restarts only when there are none.
        self.closing = 0
        self.closing_lock = threading.Lock()
        self.recording = False
        self.frames = None  # audio callback -> segmenter thread
        self.pieces = None  # segmenter thread -> transcriber thread
        self.pre_roll = collections.deque(maxlen=PRE_ROLL_FRAMES)
        self.recorded = []
        self.texts = []
        self.wav = None
        self.last_text = ""
        self.last_language = None
        self.cancelled = False
        self.dictionaries = list(settings.dictionaries)
        self.context = ""
        self.auto = False
        self.workers = []
        self.stream = None
        self.last_used = time.monotonic()
        if settings.microphone_idle_close > 0:
            threading.Thread(target=self._close_when_idle, daemon=True).start()
        else:
            self._open_stream()

    def _close_when_idle(self):
        while True:
            time.sleep(1)
            with self.lock:
                idle = time.monotonic() - self.last_used
                if self.recording or self.stream is None or idle < self.settings.microphone_idle_close:
                    continue
                stream, self.stream = self.stream, None
                self.pre_roll.clear()
            self._closing(1)
            closer = threading.Thread(target=self._close, args=(stream,), daemon=True)
            closer.start()
            closer.join(CLOSE_TIMEOUT_S)
            if closer.is_alive():
                log("closing the microphone hung in CoreAudio; exiting so the core restarts us")
                os._exit(3)
            log("microphone closed after idle time")

    def _closing(self, step):
        with self.closing_lock:
            self.closing += step

    def _close(self, stream):
        """Close a stream; the caller counted it with _closing(1) first."""
        try:
            with self.audio_lock:
                stream.abort()
                stream.close()
        except sd.PortAudioError as e:
            log(f"audio: closing the microphone: {e}")
        finally:
            self._closing(-1)

    def _open_stream(self):
        def on_audio(indata, _frames, _time, status):
            if status:
                log(f"audio: {status}")
            frame = indata[:, 0].copy()
            if self.recording:
                self.frames.put(frame)
            else:
                self.pre_roll.append(frame)

        # PortAudio learns the devices, and which is the default, once at
        # start. Headphones that connect or leave later change the system's
        # default input, and the provider kept recording from the old one,
        # which gave silence. With no stream open, start PortAudio again so
        # it sees the devices as they are now.
        # A just-opened stream is not idle: the idle closer must not take it.
        self.last_used = time.monotonic()
        with self.closing_lock:
            idle = self.closing == 0
        if self.stream is None and idle:
            try:
                with self.audio_lock:
                    sd._terminate()
                    sd._initialize()
            except Exception as e:  # noqa: BLE001 - keep the old list then
                log(f"audio: cannot reload the device list: {e}")
        device = self._device()
        self.stream = sd.InputStream(
            samplerate=RATE, channels=1, dtype="float32", blocksize=FRAME,
            device=device, callback=on_audio,
        )
        self.stream.start()
        try:
            name = sd.query_devices(self.stream.device, "input")["name"]
        except Exception:  # noqa: BLE001 - the name is only for the log
            name = "?"
        log(f"microphone open: {name}")

    def chosen_microphone(self):
        """The chosen microphone's name; empty follows the system."""
        try:
            return MICROPHONE_FILE.read_text(encoding="utf-8").strip()
        except OSError:
            return self.settings.microphone or ""

    def _device(self):
        """The chosen microphone, or None (the system's) when it is not
        there, with a note for the user."""
        chosen = self.chosen_microphone()
        if not chosen:
            return None
        # By index: sounddevice takes a name as a part of one, and "Mic"
        # would fit several devices.
        for index, d in enumerate(sd.query_devices()):
            if d["name"] == chosen and d["max_input_channels"] > 0:
                return index
        _names, system = inputs()
        log(f"microphone {chosen!r} is not there; recording from the system's: {system}")
        if config.ui_polish():
            tell_core(f"note Mikrofon „{chosen}” niedostępny — nagrywam z systemowego ({system})")
        else:
            tell_core(f"note Microphone “{chosen}” is not there — recording from the system's ({system})")
        return None

    def devices(self):
        names, system = inputs()
        return json.dumps({"system": system, "chosen": self.chosen_microphone(), "inputs": names}, ensure_ascii=False)

    def set_microphone(self, name):
        """Record from `name` from the next recording on ("" follows the
        system). An open stream closes now, unless it records."""
        MICROPHONE_FILE.parent.mkdir(parents=True, exist_ok=True)
        MICROPHONE_FILE.write_text(name + "\n", encoding="utf-8")
        log(f"microphone chosen: {name or 'the system default'}")
        with self.lock:
            if self.recording or self.stream is None:
                return
            stream, self.stream = self.stream, None
            self.pre_roll.clear()
        self._closing(1)
        threading.Thread(target=self._close, args=(stream,), daemon=True).start()

    def _alive(self):
        """Whether the open stream still records. A stream PortAudio no
        longer knows counts as dead, so it is opened again."""
        try:
            return self.stream.active
        except sd.PortAudioError:
            return False

    def toggle(self):
        with self.lock:
            if not self.recording:
                self.auto = False
                self._start()
            else:
                self._stop()

    def start_auto(self):
        with self.lock:
            if not self.recording:
                self.auto = True
                self._start()

    def cancel(self):
        with self.lock:
            if self.recording:
                self._stop(cancelled=True)

    def finish(self):
        """Stop recording, if it runs, and wait until the last piece is sent.

        Returns "stopped" or "idle". The core uses it before it presses
        Enter, so the Enter comes after the whole text.
        """
        with self.lock:
            if not self.recording:
                return "idle"
            self._stop()
            workers = list(self.workers)
        for w in workers:
            w.join()
        return "stopped"

    def _start(self):
        # The previous recording's workers read self.frames and self.pieces;
        # let them finish before those point to new queues.
        for w in self.workers:
            w.join()
        day = Path(self.settings.recordings) / f"{datetime.date.today():%Y-%m-%d}"
        day.mkdir(parents=True, exist_ok=True)
        self.wav = day / f"{datetime.datetime.now():%H%M%S}.wav"
        self.frames, self.pieces = queue.Queue(), queue.Queue()
        self.recorded, self.texts = [], []
        self.cancelled = False
        self.dictionaries = session_dictionaries(self.settings)
        self.context = speech_context()
        if self.stream is None:
            self._open_stream()
        elif not self._alive():
            # The device went away (for example a headset), so open it again.
            log("microphone stream stopped; opening it again")
            try:
                self.stream.close()
            except Exception:  # noqa: BLE001 - it is gone already
                pass
            self.stream = None
            self._open_stream()
        for frame in list(self.pre_roll):
            self.frames.put(frame)
        self.recording = True
        report(True)
        self.workers = [
            threading.Thread(target=self._segment, daemon=True),
            threading.Thread(target=self._transcribe, daemon=True),
        ]
        for w in self.workers:
            w.start()
        play(self.settings.sound_start)
        log(f"recording {self.wav.name}")

    def _stop(self, cancelled=False):
        self.cancelled = cancelled
        self.recording = False
        report(False)
        self.last_used = time.monotonic()
        self.frames.put(None)  # ends the segmenter, which ends the transcriber
        play(self.settings.sound_error if cancelled else self.settings.sound_stop)
        log("cancelled" if cancelled else "stopped")
        threading.Thread(target=self._finish, daemon=True).start()

    def _segment(self):
        seg = Segmenter()
        s = self.settings
        frames_per_s = RATE / FRAME
        # The pause counts only once the user has really spoken: the start
        # sound and a click are short, and the first half second holds the
        # start sound.
        skip = int(START_SOUND_S * frames_per_s) + PRE_ROLL_FRAMES
        speech = 0  # frames of speech so far
        quiet = 0  # frames of silence since the last speech
        counting = False
        frame_number = 0
        while (frame := self.frames.get()) is not None:
            self.recorded.append(frame)
            if (piece := seg.push(frame)) is not None:
                self.pieces.put(piece)
            frame_number += 1
            if not self.auto or frame_number <= skip:
                continue
            heard = speech >= ARM_SPEECH_S * frames_per_s
            if seg.last_was_speech:
                speech += 1
                quiet = 0
                if counting:
                    counting = False
                    tell_core("countdown cancel")
            elif heard:
                quiet += 1
                elapsed = frame_number / frames_per_s
                if not counting and quiet >= s.countdown_after * frames_per_s:
                    counting = True
                    # The countdown ends when both the pause is long enough
                    # and the recording is old enough.
                    left = max(s.auto_finish - s.countdown_after, s.auto_finish_min - elapsed)
                    tell_core(f"countdown {left:.2f}")
                if quiet >= s.auto_finish * frames_per_s and elapsed >= s.auto_finish_min:
                    # The core finishes the message as if the user pressed
                    # the button; that stops this recording.
                    self.auto = False
                    tell_core("auto-finish")
        if not self.cancelled and (piece := seg.cut()) is not None:
            self.pieces.put(piece)
        self.pieces.put(None)

    def _prompt(self, language):
        """Whisper's context for a piece in this language. The intro is
        written in the main language, and the last text in its own: in
        another language, either pulls Whisper into translating."""
        s = self.settings
        main = s.languages[0] if s.languages else (s.language or None)
        intro = s.prompt_intro if language == main or not s.languages else ""
        last = self.last_text if language == self.last_language or not s.languages else ""
        return build_prompt(intro, self.dictionaries, last, self.context)

    def _recognize(self, piece):
        """The language and the text of a piece.

        With several languages, Whisper's guess decides only when it is the
        main language and Whisper is sure of it. Otherwise the piece is
        recognized in the two likeliest languages, and the surer text wins:
        speech read in the wrong language scores much lower. Whisper took
        Polish speech for English with 92% certainty, and English for Polish
        at 45% against 44%.
        """
        s = self.settings
        if len(s.languages) > 1:
            probs = self.backend.detect(piece, s.languages)
            ranked = sorted(s.languages, key=lambda language: -probs[language])
            main = s.languages[0]
            if ranked[0] == main and probs[main] >= SURE_OF_MAIN:
                candidates = [main]
            else:
                candidates = ranked[:2]
            best = None
            for language in candidates:
                text, score = self.backend.transcribe_scored(piece, language, self._prompt(language))
                if best is None or score > best[2]:
                    best = (language, text, score)
            return best[0], best[1]
        language = s.languages[0] if s.languages else (s.language or None)
        return language, self.backend.transcribe(piece, language, self._prompt(language))

    def _transcribe(self):
        s = self.settings
        while (piece := self.pieces.get()) is not None:
            if self.cancelled:
                continue
            t = time.time()
            try:
                language, raw = self._recognize(piece)
            except Exception as e:  # keep the provider alive on a bad piece
                log(f"transcription failed: {e}")
                play(s.sound_error)
                continue
            text = clean(raw, session_replacements(s), language or "")
            log(f"{len(piece) / RATE:.1f}s audio, {time.time() - t:.2f}s, {language or 'auto'}: {raw}")
            if text != raw:
                log(f"  cleaned: {text}")
            if not text or self.cancelled:
                continue
            self.texts.append(text)
            self.last_text = text
            self.last_language = language
            try:
                # Leading space: the text often follows text already in the field.
                reply = core_request(f"dictate  {text}")
            except OSError as e:
                reply = f"error: {e}"
            if reply != "ok":
                log(f"core: {reply}")
                play(s.sound_error)

    def _finish(self):
        """After the last piece: save the recording and its text."""
        for w in self.workers:
            w.join()
        if not self.recorded:
            return
        audio = np.concatenate(self.recorded)
        with wave.open(str(self.wav), "wb") as f:
            f.setnchannels(1)
            f.setsampwidth(2)
            f.setframerate(RATE)
            f.writeframes((np.clip(audio, -1, 1) * 32767).astype(np.int16).tobytes())
        note = "\n\n# cancelled" if self.cancelled else ""
        self.wav.with_suffix(".txt").write_text(" ".join(self.texts) + note + "\n", encoding="utf-8")


def serve(dictation):
    sock = config.SPEECH_SOCKET
    sock.parent.mkdir(parents=True, exist_ok=True)
    if sock.exists():
        sock.unlink()
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(str(sock))
    os.chmod(sock, 0o600)
    srv.listen(4)
    log(f"listening on {sock}")
    while True:
        conn, _ = srv.accept()
        cmd = conn.recv(256).decode().strip()
        if cmd == "status":
            with conn:
                state = "recording" if dictation.recording else "idle"
                conn.sendall(f"{state}\n".encode())
            continue
        if cmd == "devices":
            with conn:
                try:
                    conn.sendall((dictation.devices() + "\n").encode())
                except Exception as e:  # noqa: BLE001 - answer with the error
                    conn.sendall(f"error: {e}\n".encode())
            continue
        if cmd == "finish":
            # Answered only when done; its own thread, so other requests
            # (and the pieces still on their way) are not blocked.
            def answer(conn=conn):
                with conn:
                    conn.sendall((dictation.finish() + "\n").encode())

            threading.Thread(target=answer, daemon=True).start()
            continue
        with conn:
            conn.sendall(b"ok\n")
        if cmd == "toggle":
            threading.Thread(target=dictation.toggle, daemon=True).start()
        elif cmd == "start-auto":
            threading.Thread(target=dictation.start_auto, daemon=True).start()
        elif cmd == "cancel":
            threading.Thread(target=dictation.cancel, daemon=True).start()
        elif cmd == "microphone" or cmd.startswith("microphone "):
            name = cmd[len("microphone"):].strip()
            threading.Thread(target=dictation.set_microphone, args=(name,), daemon=True).start()
        else:
            log(f"unknown request: {cmd!r}")


def exit_when_stdin_closes():
    """Exit with the parent: the core keeps a pipe open on our stdin.

    Started by hand from a terminal, stdin is the terminal, and Ctrl+D
    ends the provider.
    """
    def wait():
        while sys.stdin.buffer.read(64):
            pass
        log("stdin closed, exiting")
        os._exit(0)

    threading.Thread(target=wait, daemon=True).start()


def main():
    exit_when_stdin_closes()
    settings = config.load()
    log(f"loading backend {settings.backend or backends.default_backend()}, "
        f"model {settings.model or '(default)'}")
    name, backend = backends.load(settings.backend, settings.model)
    log(f"backend {name} ready")
    serve(Dictation(backend, settings))


if __name__ == "__main__":
    sys.exit(main())
