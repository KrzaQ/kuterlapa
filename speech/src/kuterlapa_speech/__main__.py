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
        self.recording = False
        self.frames = None  # audio callback -> segmenter thread
        self.pieces = None  # segmenter thread -> transcriber thread
        self.pre_roll = collections.deque(maxlen=PRE_ROLL_FRAMES)
        self.recorded = []
        self.texts = []
        self.wav = None
        self.last_text = ""
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
            closer = threading.Thread(target=lambda: (stream.abort(), stream.close()), daemon=True)
            closer.start()
            closer.join(CLOSE_TIMEOUT_S)
            if closer.is_alive():
                log("closing the microphone hung in CoreAudio; exiting so the core restarts us")
                os._exit(3)
            log("microphone closed after idle time")

    def _open_stream(self):
        def on_audio(indata, _frames, _time, status):
            if status:
                log(f"audio: {status}")
            frame = indata[:, 0].copy()
            if self.recording:
                self.frames.put(frame)
            else:
                self.pre_roll.append(frame)

        self.stream = sd.InputStream(
            samplerate=RATE, channels=1, dtype="float32", blocksize=FRAME,
            device=self.settings.microphone or None, callback=on_audio,
        )
        self.stream.start()
        log("microphone open")

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
        elif not self.stream.active:
            # The device went away (for example a headset), so open it again.
            log("microphone stream stopped; opening it again")
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

    def _transcribe(self):
        s = self.settings
        while (piece := self.pieces.get()) is not None:
            if self.cancelled:
                continue
            prompt = build_prompt(s.prompt_intro, self.dictionaries, self.last_text, self.context)
            t = time.time()
            try:
                raw = self.backend.transcribe(piece, s.language or None, prompt)
            except Exception as e:  # keep the provider alive on a bad piece
                log(f"transcription failed: {e}")
                play(s.sound_error)
                continue
            text = clean(raw, session_replacements(s), s.language)
            log(f"{len(piece) / RATE:.1f}s audio, {time.time() - t:.2f}s: {raw}")
            if text != raw:
                log(f"  cleaned: {text}")
            if not text or self.cancelled:
                continue
            self.texts.append(text)
            self.last_text = text
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
