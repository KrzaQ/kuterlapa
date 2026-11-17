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
BASE = Path(__file__).resolve().parent.parent / "docs" / "screenshots"
OUT = BASE


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
    print(path.relative_to(BASE.parent.parent))


def overlay(name, state, wait=0.5):
    """The overlay over an empty dark screen (the chat view as a backdrop):
    its translucent background needs something behind it to look as it
    does in use, and the backdrop keeps the user's screen out of the
    picture."""
    shell("overlay " + json.dumps({"visible": True, **state}))
    time.sleep(wait)
    path = OUT / f"{name}.png"
    subprocess.run(["screencapture", "-x", "-R", windows()["overlay-place"], str(path)], check=True)
    print(path.relative_to(BASE.parent.parent))


def states(pl):
    """The example overlays and chat view, in Polish or in English, with
    the labels the app shows in that language."""
    def tr(polish, english):
        return polish if pl else english

    return {
        "conversation": {
            "badge": {"text": tr("ROZMOWA", "CONVERSATION"), "tone": "conversation"},
            "title": "home · Opus medium",
            "text": tr(
                "› Jak sprawić, żeby laptop nie usnął w trakcie pobierania?\n\n"
                "Uruchom **caffeinate** w terminalu na czas pobierania:\n\n"
                "- `caffeinate -i` nie pozwala systemowi zasnąć\n"
                "- dodaj `-w <pid>`, żeby skończył razem z pobieraniem",
                "› How do I keep my laptop from sleeping while it downloads?\n\n"
                "Run **caffeinate** in a terminal while the download runs:\n\n"
                "- `caffeinate -i` keeps the system awake\n"
                "- add `-w <pid>` to stop when the download process ends"),
            "hint": tr("Y: dalej · Y Y: polecenie · B: zamknij", "Y: continue · Y Y: command · B: close"),
        },
        "command": {
            "badge": {"text": tr("POLECENIE · JEDNORAZOWE", "COMMAND · ONE-OFF"), "tone": "command"},
            "frame": "dashed",
            "title": "home · Qwen 96k",
            "text": tr("› Ile kart jest otwartych w Chrome?\n\nMasz **7 kart** w trzech oknach.",
                       "› How many tabs are open in Chrome?\n\nYou have **7 tabs** in three windows."),
            "hint": tr("Y Y: nowe polecenie · Y: rozmowa · B: zamknij", "Y Y: new command · Y: conversation · B: close"),
        },
        "listening": {
            "badge": {"text": tr("ROZMOWA", "CONVERSATION"), "tone": "conversation"},
            "title": "home · Opus medium",
            "status": tr("Słucham…", "Listening…"),
            "text": tr("Otwórz pogodę na jutro i przeczytaj mi ją", "Open the weather for tomorrow and read it to me"),
            "hint": tr("Y: wyślij · A: wyślij · B: anuluj", "Y: send · A: send · B: cancel"),
            "countdown": {"ends": 0, "total": 2.2},
        },
        "draft": {
            "title": tr("Szkic · Qwen 96k", "Draft · Qwen 96k"),
            "status": "",
            "text": tr("Cześć Ania, będę jutro o 10. Wezmę ze sobą dokumenty do podpisu.",
                       "Hi Anna, I'll be there tomorrow at 10. I'll bring the papers to sign."),
            "detail": tr("Polecenia:\n1. (pierwsze dyktowanie) cześć Ania będę jutro o dziesiątej\n"
                         "2. dopisz, że wezmę dokumenty do podpisu",
                         "Instructions:\n1. (first dictation) hi Anna I'll be there tomorrow at ten\n"
                         "2. add that I'll bring the papers to sign"),
            "hint": tr("X: popraw głosem · A: wpisz · B: odrzuć", "X: edit by voice · A: type · B: discard"),
        },
        "consent": {
            "title": tr("Zgoda", "Permission"),
            "status": tr("Czekam na Twoją decyzję", "Waiting for your decision"),
            "heading": "send message",
            "text": tr("Wyślij wiadomość do kontaktu.", "Send a chat message to a contact."),
            "detail": tr('~/bin/send-message --to Ania "Będę o 10"', '~/bin/send-message --to Alex "I\'ll be there at 10"'),
            "buttons": [{"key": "A", "label": tr("Tak", "Yes"), "tone": "yes"},
                        {"key": "B", "label": tr("Nie", "No"), "tone": "no"}],
            "attention": True,
        },
        "voice": {
            "title": tr("Lektor", "Narrator"),
            "status": tr("Czekam na Twoją decyzję", "Waiting for your decision"),
            "heading": tr("Którym głosem czytać?", "Which voice should read?"),
            "text": "",
            "choices": [{"text": "anna", "selected": False},
                        {"text": tr("lektor ✓", "narrator ✓"), "selected": True},
                        {"text": "tomasz", "selected": False}],
            "buttons": [{"key": "A", "label": tr("Wybierz", "Choose"), "tone": "yes"},
                        {"key": "B", "label": tr("Anuluj", "Cancel"), "tone": "no"}],
            "hint": tr("↑ ↓: wybór · A: wybierz · B: anuluj", "↑ ↓: move · A: choose · B: cancel"),
            "attention": True,
        },
        "help": {
            "title": tr("Pomoc · przyciski pada", "Help · controller buttons"),
            "help": [
                {"title": tr("Mówienie", "Speech"), "rows": [
                    {"keys": "X", "text": tr("dyktuj do pola", "dictate into a field")},
                    {"keys": "X ×2", "text": tr("nowy szkic", "new draft")},
                    {"keys": "Y", "text": tr("rozmowa", "conversation")},
                    {"keys": "Y ×2", "text": tr("jednorazowe polecenie", "one-off command")},
                    {"keys": "A", "text": tr("wyślij · zgoda: tak", "send · permission: yes")},
                    {"keys": "B", "text": tr("anuluj · zgoda: nie", "cancel · permission: no")},
                    {"keys": tr("− długo", "− hold"), "text": tr("mikrofon", "microphone")},
                ]},
                {"title": tr("Model i lektor", "Model and narrator"), "rows": [
                    {"keys": tr("Y długo", "Y hold"), "text": tr("tryb lektora", "narrator mode")},
                    {"keys": "L", "text": tr("następny model", "next model")},
                    {"keys": "ZL", "text": tr("poprzedni model", "previous model")},
                    {"keys": "ZR", "text": tr("ostatnia odpowiedź", "last answer")},
                    {"keys": "+", "text": tr("nowa rozmowa / polecenie", "new conversation / command")},
                    {"keys": "+ ×2 · Share ×2", "text": tr("ta pomoc", "this help")},
                    {"keys": tr("+ długo", "+ hold"), "text": tr("ostatnie sesje", "recent sessions")},
                    {"keys": "−", "text": tr("cisza", "silence")},
                    {"keys": "− ×2", "text": tr("głos lektora", "narrator voice")},
                ]},
                {"title": tr("Ekran i mysz", "Screen and mouse"), "rows": [
                    {"keys": "Share", "text": tr("widok czatu", "chat view")},
                    {"keys": "R · R3", "text": tr("klik", "click")},
                    {"keys": tr("R długo", "R hold"), "text": tr("prawy klik", "right click")},
                    {"keys": "↑ ↓", "text": tr("strona w górę / w dół", "page up / down")},
                    {"keys": "← →", "text": tr("pulpit w lewo / w prawo", "desktop to the left / right")},
                    {"keys": tr("lewa gałka", "left stick"), "text": tr("przewijanie", "scroll")},
                    {"keys": tr("prawa gałka", "right stick"), "text": tr("kursor", "pointer")},
                ]},
            ],
            "hint": tr("B: zamknij · zamyka się sama po 30 s · ×2: naciśnij dwa razy · długo: przytrzymaj",
                       "B: close · closes by itself after 30 s · ×2: press two times · hold: hold the button down"),
        },
        "sessions": {
            "title": tr("Sesje · bieżąca: home · lepka", "Sessions · current: home · sticky"),
            "help": [{"title": tr("Ostatnio używane", "Recently used"), "rows": [
                {"keys": "home", "text": tr("● teraz · Dom, rachunki i sprawy", "● now · The house, bills and errands")},
                {"keys": "thesis", "text": tr("2 godz. temu · Pisanie pracy, rozdział 3", "2 h ago · Writing the thesis, chapter 3")},
                {"keys": "garden", "text": tr("wczoraj · Plany ogrodu", "yesterday · Plans for the garden")},
                {"keys": "general", "text": tr("4 dni temu", "4 days ago")},
            ]}],
            "help_key_width": 120,
            "width": 560,
            "hint": tr("Powiedz: „przejdź do sesji …” · B: zamknij · zamyka się sama po 30 s",
                       "Say: “go to session …” · B: close · closes by itself after 30 s"),
        },
        "usage": {
            "title": tr("Zużycie modeli · dziś", "Model use · today"),
            "help": [
                {"title": tr("Odpowiedzi i polecenia", "Answers and commands"), "rows": [
                    {"keys": "claude-opus-5-5", "text": tr(
                        "w 4 zapytaniach · wejście 31 · z cache 128k · zapis do cache 42,0k · wyjście 2,1k (myślenie 640) · $0,74",
                        "in 4 requests · input 31 · from cache 128k · cache write 42.0k · output 2.1k (thinking 640) · $0.74")},
                    {"keys": "qwen3-8b", "text": tr(
                        "9 wywołań · wejście 38,2k · z cache 61,5k · wyjście 1,9k · 1 min 12 s",
                        "9 calls · input 38.2k · from cache 61.5k · output 1.9k · 1 min 12 s")},
                ]},
                {"title": tr("Poprawki szkicu", "Draft changes"), "rows": [
                    {"keys": "qwen3-8b", "text": tr("3 wywołania · wejście 2,4k · wyjście 310 · 6 s",
                                                    "3 calls · input 2.4k · output 310 · 6 s")},
                ]},
                {"title": tr("Klasyfikator pi-censora", "pi-censor classifier"), "rows": [
                    {"keys": "qwen3-8b", "text": tr("5 wywołań · wejście 5,6k · wyjście 820 · 11 s",
                                                    "5 calls · input 5.6k · output 820 · 11 s")},
                ]},
            ],
            "help_key_width": 210,
            "help_columns": 1,
            "width": 780,
            "hint": tr("Powiedz: „pokaż zużycie z tygodnia” · B: zamknij · zamyka się sama po 30 s",
                       "Say: “show the use for this week” · B: close · closes by itself after 30 s"),
        },
    }


def view_state(pl):
    """The chat view with a made-up conversation."""
    def tr(polish, english):
        return polish if pl else english

    return {
        "visible": True,
        "language": "pl" if pl else "en",
        "scale": 1.15,
        "session": "thesis",
        "label": "thesis",
        "automatic": False,
        "other_session": False,
        "conversation": {"kind": "Rozmowa", "index": 4, "count": 6, "created": "2026-11-09 10:12"},
        "sessions": [
            {"label": "thesis", "current": True, "shown": True, "when": tr("przed chwilą", "just now")},
            {"label": "home", "current": False, "shown": False, "when": tr("2 godz. temu", "2 h ago")},
            {"label": "garden", "current": False, "shown": False, "when": tr("wczoraj", "yesterday")},
        ],
        "items": [
            {"kind": "Rozmowa", "title": tr("Wyjaśnij resztę we wzorze 3.2", "Explain the error term in equation 3.2"),
             "created": "2026-11-09 10:12", "current": True},
            {"kind": "Polecenie", "title": tr("Otwórz szkic w edytorze", "Open the draft in the editor"),
             "created": "2026-11-09 09:40", "current": False},
            {"kind": "Rozmowa", "title": tr("Plan rozdziału 3", "Outline for chapter 3"),
             "created": "2026-11-08 18:05", "current": False},
        ],
        "messages": [
            {"role": "user", "profile": None, "created": "2026-11-09 10:12", "text": tr(
                "Wyjaśnij resztę we wzorze 3.2 i pokaż, jak ją policzyć.",
                "Explain the error term in equation 3.2 and show how to compute it.")},
            {"role": "assistant", "profile": "Opus medium", "created": "2026-11-09 10:12", "text": tr(
                "Reszta to różnica między prawdziwą wartością a przybliżeniem:",
                "The error term is the difference between the true value and the approximation:") +
             "\n\n$$\\varepsilon_n = f(x) - \\sum_{k=0}^{n} \\frac{f^{(k)}(a)}{k!}(x-a)^k$$\n\n" + tr(
                "Maleje wraz ze wzrostem **n**, gdy $f$ jest gładka w pobliżu $a$. W Pythonie:",
                "It shrinks as **n** grows when $f$ is smooth near $a$. In Python:") +
             "\n\n```python\nfrom math import exp, factorial\n\n"
             "def error(x, a=0.0, n=4):\n    approx = sum(exp(a) * (x - a) ** k / factorial(k) for k in range(n + 1))\n"
             "    return exp(x) - approx\n```"},
            {"role": "user", "profile": None, "created": "2026-11-09 10:14", "text": tr("A dla n = 6?", "And for n = 6?")},
            {"role": "assistant", "profile": "Opus medium", "created": "2026-11-09 10:14", "text": tr(
                "Dla $x = 1$ spada z około `9.9e-3` do `2.3e-4`: każdy kolejny wyraz dzieli ją mniej więcej przez $n+1$.",
                "For $x = 1$ it drops from about `9.9e-3` to `2.3e-4`: each extra term divides it by roughly $n+1$.")},
        ],
        "focus": None,
        "follow": False,
    }


def take(pl):
    """All the pictures in one language: English in docs/screenshots/,
    Polish in docs/screenshots/pl/."""
    global OUT
    OUT = BASE / "pl" if pl else BASE
    OUT.mkdir(parents=True, exist_ok=True)
    shown = states(pl)
    shown["listening"]["countdown"]["ends"] = time.time() + 2.2
    for name, state in shown.items():
        overlay(name, state, wait=1.2 if name == "usage" else 0.9 if name == "listening" else 0.5)
    shell('overlay {"visible": false}')
    shell("view " + json.dumps({**view_state(pl), "backdrop": False}))
    time.sleep(1.5)
    capture(windows()["view"], "chat-view", width=1600)
    shell("view " + json.dumps({"visible": True, "backdrop": True, "language": "pl" if pl else "en"}))
    time.sleep(0.8)


def main():
    scale_file = STATE / "overlay-scale"
    scale = scale_file.read_text().strip() if scale_file.exists() else "1"
    shell("overlay-scale 1")
    try:
        shell("view " + json.dumps({"visible": True, "backdrop": True}))
        time.sleep(1.0)
        take(pl=False)
        take(pl=True)
    finally:
        shell('view {"visible": false}')
        shell('overlay {"visible": false}')
        shell(f"overlay-scale {scale}")


if __name__ == "__main__":
    main()
