"""kuterlapa-voice: a voice provider for Kuterlapa. It reads text aloud.

It implements the voice protocol (docs/protocol.md) and listens on
~/.local/state/kuterlapa/voice.sock. Requests, one line per connection:

    say <text>   read the text aloud, after anything still being read
    stop         stop reading and drop what is waiting
    status       "speaking" or "idle"

The text is split into pieces (pieces.py). One thread makes pieces with
the backend, another plays them, so the next piece is ready when the
current one ends.

The core starts the provider with a pipe on stdin. When the pipe closes,
the provider exits.
"""

import datetime
import os
import queue
import socket
import sys
import threading

import sounddevice as sd

from . import backends, config
from .pieces import split


def log(msg):
    print(f"{datetime.datetime.now():%H:%M:%S} voice: {msg}", flush=True)


class Reader:
    def __init__(self, backend, settings):
        self.backend = backend
        self.settings = settings
        self.texts = queue.Queue()  # pieces of text to make
        self.sounds = queue.Queue(maxsize=2)  # made pieces to play
        # Each "stop" starts a new generation; work from an older one is
        # dropped instead of played.
        self.generation = 0
        self.busy = 0
        self.lock = threading.Lock()
        threading.Thread(target=self._make, daemon=True).start()
        threading.Thread(target=self._play, daemon=True).start()

    def say(self, text):
        s = self.settings
        with self.lock:
            generation = self.generation
            for piece in split(text, s.first_piece_chars, s.piece_chars):
                self.busy += 1
                self.texts.put((generation, piece))

    def stop(self):
        with self.lock:
            self.generation += 1
        sd.stop()

    def speaking(self):
        return self.busy > 0

    def _done(self):
        with self.lock:
            self.busy = max(0, self.busy - 1)

    def _make(self):
        while True:
            generation, text = self.texts.get()
            if generation != self.generation:
                self._done()
                continue
            try:
                audio, rate = self.backend.synthesize(text)
            except Exception as e:  # keep the provider alive on a bad piece
                log(f"synthesis failed: {e}")
                self._done()
                continue
            self.sounds.put((generation, audio, rate))

    def _play(self):
        while True:
            generation, audio, rate = self.sounds.get()
            if generation == self.generation and len(audio):
                sd.play(audio, rate)
                sd.wait()
            self._done()


def serve(reader):
    sock = config.VOICE_SOCKET
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
        with conn:
            data = b""
            while not data.endswith(b"\n"):
                chunk = conn.recv(65536)
                if not chunk:
                    break
                data += chunk
            line = data.decode("utf-8").rstrip("\n")
            verb, _, text = line.partition(" ")
            if verb == "say":
                reader.say(text)
                answer = "ok"
            elif verb == "stop":
                reader.stop()
                answer = "ok"
            elif verb == "status":
                answer = "speaking" if reader.speaking() else "idle"
            else:
                answer = f"error: unknown request {verb}"
            conn.sendall((answer + "\n").encode())


def exit_when_stdin_closes():
    def wait():
        while sys.stdin.buffer.read(64):
            pass
        log("stdin closed, exiting")
        os._exit(0)

    threading.Thread(target=wait, daemon=True).start()


def main():
    exit_when_stdin_closes()
    settings = config.load()
    log(f"loading backend {settings.backend}")
    backend = backends.load(settings)
    log("ready")
    serve(Reader(backend, settings))


if __name__ == "__main__":
    sys.exit(main())
