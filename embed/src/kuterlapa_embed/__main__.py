"""kuterlapa-embed: the embedding provider for Kuterlapa.

It listens on ~/.local/state/kuterlapa/embed.sock. Requests, one line per
connection:

    embed <json>   {"kind": "query" | "document", "texts": ["...", ...]}
                   answers {"vectors": [[...], ...]} on one line
    status         the backend's name

The backend (mlx: a local model; openai: any OpenAI-compatible server) is
set in embed.toml. The core starts the provider with a pipe on stdin;
when the pipe closes, the provider exits.
"""

import datetime
import json
import os
import socket
import sys
import threading

from . import backends, config


def log(msg):
    print(f"{datetime.datetime.now():%H:%M:%S} embed: {msg}", flush=True)


def serve(backend, settings):
    sock = config.EMBED_SOCKET
    sock.parent.mkdir(parents=True, exist_ok=True)
    if sock.exists():
        sock.unlink()
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(str(sock))
    os.chmod(sock, 0o600)
    srv.listen(4)
    log(f"listening on {sock}")
    # One request at a time: the model is not shared between threads.
    while True:
        conn, _ = srv.accept()
        with conn:
            data = b""
            while not data.endswith(b"\n"):
                chunk = conn.recv(1 << 20)
                if not chunk:
                    break
                data += chunk
            line = data.decode("utf-8").rstrip("\n")
            verb, _, rest = line.partition(" ")
            try:
                if verb == "embed":
                    request = json.loads(rest)
                    texts = [str(t) for t in request.get("texts", [])]
                    if request.get("kind") == "query":
                        texts = [f"{settings.query_prefix}{t}" for t in texts]
                    answer = json.dumps({"vectors": backend.embed(texts) if texts else []})
                elif verb == "status":
                    answer = settings.backend
                else:
                    answer = f"error: unknown request {verb}"
            except Exception as e:  # keep the provider alive on a bad request
                log(f"failed: {e!r}")
                answer = f"error: {e}"
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
    log(f"loading backend {settings.backend}, model {settings.model or '(none)'}")
    backend = backends.load(settings)
    log("ready")
    serve(backend, settings)


if __name__ == "__main__":
    sys.exit(main())
