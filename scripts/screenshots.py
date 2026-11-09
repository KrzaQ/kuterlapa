#!/usr/bin/env python3
"""Takes the screenshots in docs/screenshots/ for docs/tour.md.

Kuterlapa must be running. The script shows made-up example content in
the overlay and the chat view (nothing from the user's own sessions or
conversations), takes a picture of that one window each time, and hides
it again. Run it after a change to how the panels look:

    make screenshots
"""

import json
import os
import socket
import subprocess
import time
from pathlib import Path

STATE = Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state") / "kuterlapa"
OUT = Path(__file__).resolve().parent.parent / "docs" / "screenshots"


def shell(line):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(10)
        s.connect(str(STATE / "shell.sock"))
        s.sendall((line + "\n").encode())
        return s.recv(4096).decode().strip()


def windows():
    return dict(part.split("=") for part in shell("panel-windows").split())


def capture(window, name, width=None):
    path = OUT / f"{name}.png"
    subprocess.run(["screencapture", "-x", "-o", "-l", str(window), str(path)], check=True)
    if width:
        subprocess.run(["sips", "-Z", str(width), str(path)], check=True, capture_output=True)
    print(path.relative_to(OUT.parent.parent))


def overlay(name, state, wait=0.5):
    """The overlay over an empty dark screen (the chat view as a backdrop):
    its translucent background needs something behind it to look as it
    does in use, and the backdrop keeps the user's screen out of the
    picture."""
    shell("overlay " + json.dumps({"visible": True, **state}))
    time.sleep(wait)
    path = OUT / f"{name}.png"
    subprocess.run(["screencapture", "-x", "-R", windows()["overlay-place"], str(path)], check=True)
    print(path.relative_to(OUT.parent.parent))


CONVERSATION = {
    "badge": {"text": "ROZMOWA", "tone": "conversation"},
    "title": "home · Opus medium",
    "text": "› How do I keep my laptop from sleeping while it downloads?\n\n"
    "Run **caffeinate** in a terminal while the download runs:\n\n"
    "- `caffeinate -i` keeps the system awake\n"
    "- add `-w <pid>` to stop when the download process ends",
    "hint": "Y: dalej · Y Y: polecenie · B: zamknij",
}

COMMAND = {
    "badge": {"text": "POLECENIE · JEDNORAZOWE", "tone": "command"},
    "frame": "dashed",
    "title": "home · Qwen 96k",
    "text": "› How many tabs are open in Chrome?\n\nYou have **7 tabs** in three windows.",
    "hint": "Y Y: dopytaj · +: nowe polecenie · Y: rozmowa · B: zamknij",
}

LISTENING = {
    "badge": {"text": "ROZMOWA", "tone": "conversation"},
    "title": "home · Opus medium",
    "status": "Słucham…",
    "text": "Open the weather for tomorrow and read it to me",
    "hint": "Y: wyślij · A: wyślij · B: anuluj",
    "countdown": {"ends": time.time() + 2.2, "total": 2.2},
}

CONSENT = {
    "title": "Zgoda",
    "status": "Czekam na Twoją decyzję",
    "heading": "send message",
    "text": "Send a chat message to a contact.",
    "detail": "~/bin/send-message --to Alex \"I'll be there at 10\"",
    "buttons": [{"key": "A", "label": "Tak", "tone": "yes"}, {"key": "B", "label": "Nie", "tone": "no"}],
    "attention": True,
}

HELP = {
    "title": "Pomoc · przyciski pada",
    "help": [
        {"title": "Mówienie", "rows": [
            {"keys": "X", "text": "dyktuj do pola"},
            {"keys": "X ×2", "text": "nowy szkic"},
            {"keys": "Y", "text": "rozmowa"},
            {"keys": "Y ×2", "text": "jednorazowe polecenie"},
            {"keys": "A", "text": "wyślij · zgoda: tak"},
            {"keys": "B", "text": "anuluj · zgoda: nie"},
        ]},
        {"title": "Model i lektor", "rows": [
            {"keys": "Y długo", "text": "tryb lektora"},
            {"keys": "L", "text": "następny model"},
            {"keys": "ZL", "text": "poprzedni model"},
            {"keys": "ZR", "text": "ostatnia odpowiedź"},
            {"keys": "+", "text": "nowa rozmowa / polecenie"},
            {"keys": "+ ×2 · Share ×2", "text": "ta pomoc"},
            {"keys": "+ długo", "text": "ostatnie sesje"},
            {"keys": "−", "text": "cisza"},
        ]},
        {"title": "Ekran i mysz", "rows": [
            {"keys": "Share", "text": "widok czatu"},
            {"keys": "R · R3", "text": "klik"},
            {"keys": "↑", "text": "strona w górę"},
            {"keys": "↓", "text": "strona w dół"},
            {"keys": "←", "text": "pulpit w lewo"},
            {"keys": "→", "text": "pulpit w prawo"},
            {"keys": "lewa gałka", "text": "przewijanie"},
            {"keys": "prawa gałka", "text": "kursor"},
        ]},
    ],
    "hint": "B: zamknij · zamyka się sama po 30 s · ×2: naciśnij dwa razy · długo: przytrzymaj",
}

SESSIONS = {
    "title": "Sesje · bieżąca: home · lepka",
    "help": [{"title": "Ostatnio używane", "rows": [
        {"keys": "home", "text": "● teraz · The house, bills and errands"},
        {"keys": "thesis", "text": "2 godz. temu · Writing the thesis, chapter 3"},
        {"keys": "garden", "text": "wczoraj · Plans for the garden"},
        {"keys": "general", "text": "4 dni temu"},
    ]}],
    "help_key_width": 120,
    "width": 560,
    "hint": "Powiedz: „przejdź do sesji …” · B: zamknij · zamyka się sama po 30 s",
}

VIEW = {
    "visible": True,
    "scale": 1.15,
    "session": "thesis",
    "label": "thesis",
    "automatic": False,
    "other_session": False,
    "conversation": {"kind": "Rozmowa", "index": 4, "count": 6, "created": "2026-11-09 10:12"},
    "sessions": [
        {"label": "thesis", "current": True, "shown": True, "when": "przed chwilą"},
        {"label": "home", "current": False, "shown": False, "when": "2 godz. temu"},
        {"label": "garden", "current": False, "shown": False, "when": "wczoraj"},
    ],
    "items": [
        {"kind": "Rozmowa", "title": "Explain the error term in equation 3.2", "created": "2026-11-09 10:12", "current": True},
        {"kind": "Polecenie", "title": "Open the draft in the editor", "created": "2026-11-09 09:40", "current": False},
        {"kind": "Rozmowa", "title": "Outline for chapter 3", "created": "2026-11-08 18:05", "current": False},
    ],
    "messages": [
        {"role": "user", "text": "Explain the error term in equation 3.2 and show how to compute it.",
         "profile": None, "created": "2026-11-09 10:12"},
        {"role": "assistant", "profile": "Opus medium", "created": "2026-11-09 10:12", "text":
         "The error term is the difference between the true value and the approximation:\n\n"
         "$$\\varepsilon_n = f(x) - \\sum_{k=0}^{n} \\frac{f^{(k)}(a)}{k!}(x-a)^k$$\n\n"
         "It shrinks as **n** grows when $f$ is smooth near $a$. In Python:\n\n"
         "```python\nfrom math import exp, factorial\n\n"
         "def error(x, a=0.0, n=4):\n    approx = sum(exp(a) * (x - a) ** k / factorial(k) for k in range(n + 1))\n"
         "    return exp(x) - approx\n```"},
        {"role": "user", "text": "And for n = 6?", "profile": None, "created": "2026-11-09 10:14"},
        {"role": "assistant", "profile": "Opus medium", "created": "2026-11-09 10:14", "text":
         "For $x = 1$ it drops from about `9.9e-3` to `2.3e-4`: each extra term divides it by roughly $n+1$."},
    ],
    "focus": None,
    "follow": False,
}


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    scale_file = STATE / "overlay-scale"
    scale = scale_file.read_text().strip() if scale_file.exists() else "1"
    shell("overlay-scale 1")
    try:
        shell("view " + json.dumps({"visible": True, "backdrop": True}))
        time.sleep(1.0)
        overlay("conversation", CONVERSATION)
        overlay("command", COMMAND)
        LISTENING["countdown"]["ends"] = time.time() + 2.2
        overlay("listening", LISTENING, wait=0.9)
        overlay("consent", CONSENT)
        overlay("help", HELP)
        overlay("sessions", SESSIONS)
        shell('overlay {"visible": false}')
        shell("view " + json.dumps({**VIEW, "backdrop": False}))
        time.sleep(1.5)
        capture(windows()["view"], "chat-view", width=1600)
    finally:
        shell('view {"visible": false}')
        shell('overlay {"visible": false}')
        shell(f"overlay-scale {scale}")


if __name__ == "__main__":
    main()
