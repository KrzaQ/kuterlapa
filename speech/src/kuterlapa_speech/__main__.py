"""kuterlapa-speech: a speech provider for Kuterlapa, with local Whisper.

The recognition backend (MLX, faster-whisper) is a setting; see backends/.

It implements the speech protocol (docs/protocol.md). It keeps the model in
memory and listens on ~/.local/state/kuterlapa/speech.sock. Requests, one
line per connection:

    toggle   start recording, or stop it
    cancel   stop recording and drop the pieces not typed yet
    finish   stop recording and answer once the last piece is sent:
             "stopped", or "idle" if it was not recording
    status   "recording" or "idle"

While recording, it cuts the audio at pauses in speech. Each piece goes to
Whisper at once, and the text goes to the Kuterlapa core
(request "dictate <text>" on ~/.local/state/kuterlapa/core.sock).

The core starts the provider with a pipe on stdin. When the pipe closes,
the provider exits.
"""

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


class Dictation:
    def __init__(self, backend, settings):
        self.backend = backend
        self.settings = settings
        self.lock = threading.Lock()
        self.stream = None
        self.frames = None  # audio callback -> segmenter thread
        self.pieces = None  # segmenter thread -> transcriber thread
        self.recorded = []
        self.texts = []
        self.wav = None
        self.last_text = ""
        self.cancelled = False
        self.workers = []

    def toggle(self):
        with self.lock:
            if self.stream is None:
                self._start()
            else:
                self._stop()

    def cancel(self):
        with self.lock:
            if self.stream is not None:
                self._stop(cancelled=True)

    def finish(self):
        """Stop recording, if it runs, and wait until the last piece is sent.

        Returns "stopped" or "idle". The core uses it before it presses
        Enter, so the Enter comes after the whole text.
        """
        with self.lock:
            if self.stream is None:
                return "idle"
            self._stop()
            workers = list(self.workers)
        for w in workers:
            w.join()
        return "stopped"

    def _start(self):
        day = Path(self.settings.recordings) / f"{datetime.date.today():%Y-%m-%d}"
        day.mkdir(parents=True, exist_ok=True)
        self.wav = day / f"{datetime.datetime.now():%H%M%S}.wav"
        self.frames, self.pieces = queue.Queue(), queue.Queue()
        self.recorded, self.texts = [], []
        self.cancelled = False

        def on_audio(indata, _frames, _time, status):
            if status:
                log(f"audio: {status}")
            self.frames.put(indata[:, 0].copy())

        self.stream = sd.InputStream(
            samplerate=RATE, channels=1, dtype="float32", blocksize=FRAME,
            device=self.settings.microphone or None, callback=on_audio,
        )
        self.stream.start()
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
        self.stream.stop()
        self.stream.close()
        self.stream = None
        self.frames.put(None)  # ends the segmenter, which ends the transcriber
        play(self.settings.sound_error if cancelled else self.settings.sound_stop)
        log("cancelled" if cancelled else "stopped")
        threading.Thread(target=self._finish, daemon=True).start()

    def _segment(self):
        seg = Segmenter()
        while (frame := self.frames.get()) is not None:
            self.recorded.append(frame)
            if (piece := seg.push(frame)) is not None:
                self.pieces.put(piece)
        if not self.cancelled and (piece := seg.cut()) is not None:
            self.pieces.put(piece)
        self.pieces.put(None)

    def _transcribe(self):
        s = self.settings
        while (piece := self.pieces.get()) is not None:
            if self.cancelled:
                continue
            prompt = build_prompt(s.prompt_intro, s.dictionaries, self.last_text)
            t = time.time()
            try:
                raw = self.backend.transcribe(piece, s.language, prompt)
            except Exception as e:  # keep the provider alive on a bad piece
                log(f"transcription failed: {e}")
                play(s.sound_error)
                continue
            text = clean(raw, s.replacements)
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
                state = "recording" if dictation.stream is not None else "idle"
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
