#!/usr/bin/env python3
"""tmux and Claude Code for Kuterlapa's model tools, on this computer
("local") or on a host reached by SSH with a key.

    tmux.py panes <host>                  the panes: target, program, title;
                                          Claude Code panes that wait for an
                                          answer are marked
    tmux.py read <host> <pane> [lines]    the last lines of a pane
    tmux.py send <host> <pane> <text>     type text into a pane and press
                                          Enter (a message to Claude Code)
    tmux.py question <host> <pane>        the question Claude Code waits on:
                                          its options, numbered
    tmux.py answer <host> <pane> <choice> answer it: "2", "1,3" for several,
                                          "submit" or "cancel" on the review
    tmux.py ask <host> <pane>             show Claude Code's questions in
                                          Kuterlapa's overlay, one by one;
                                          the user picks with the controller

Claude Code (2.1) draws its questions like this: numbered options, a
number key picks one (or ticks it, when several may be picked), the
right arrow goes to the next question, and a review asks "Ready to submit
your answers?". A few menus have no numbers: arrows and Enter.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import socket
import subprocess
import sys
import time

STATE = os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state"), "kuterlapa")
FOOTERS = ("Esc to cancel", "Enter to select", "Enter to confirm")
RULE = re.compile(r"^\s*[─╌━-]{8,}\s*$")
NUMBERED = re.compile(r"^\s*(❯\s*)?(\d+)\.\s+(.*\S)\s*$")
BOX = re.compile(r"^\[(.)\]\s*(.*)$")


class Fail(Exception):
    pass


def tmux(host: str, *args: str, script: str | None = None) -> str:
    """Runs tmux here or on the host; or a shell script on the host."""
    if host in ("", "local", "localhost"):
        command = ["/bin/sh", "-c", script] if script else ["tmux", *args]
    else:
        remote = script if script else " ".join(shlex.quote(a) for a in ["tmux", *args])
        command = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", host, remote]
    done = subprocess.run(command, capture_output=True, text=True, timeout=30)
    if done.returncode != 0:
        raise Fail((done.stderr or done.stdout).strip() or f"tmux failed on {host}")
    return done.stdout


def screen(host: str, pane: str, lines: int = 80) -> list[str]:
    text = tmux(host, "capture-pane", "-p", "-J", "-t", pane, "-S", f"-{lines}")
    return [line.rstrip() for line in text.split("\n")]


def panes(host: str) -> str:
    # tmux may turn tabs in its output into "_": a printable separator.
    sep = " |~| "
    listing = tmux(host, "list-panes", "-a", "-F",
                   sep.join(["#{session_name}:#{window_index}.#{pane_index}", "#{pane_current_command}", "#{pane_title}"]))
    rows = [line.split(sep) for line in listing.splitlines() if line.count(sep) == 2]
    if not rows:
        return "No tmux panes."
    # Claude Code shows as "claude", or as its version number (2.1.319).
    claude = [r[0] for r in rows if r[1] == "claude" or re.fullmatch(r"\d+\.\d+\.\d+", r[1])]
    waiting = set()
    if claude:
        # One round trip: which of them wait for an answer.
        footer = "|".join(FOOTERS)
        script = "; ".join(
            f"tmux capture-pane -p -J -t {shlex.quote(t)} -S -40 | grep -v '^[[:space:]]*$' | tail -n 3 "
            f"| grep -qE '{footer}|Submit answers' && echo {shlex.quote(t)}"
            for t in claude) + "; true"
        waiting = set(tmux(host, script=script).split())
    out = []
    for target, command, title in rows:
        kind = "Claude Code" if target in claude else command
        # Claude Code puts a status sign before its title ("✳", or "_" once
        # tmux has cleaned it).
        title = re.sub(r"^[_✳⠂⠐*·]\s+", "", title)
        mark = " | WAITS FOR AN ANSWER" if target in waiting else ""
        out.append(f"{target} | {kind} | {title}{mark}")
    return "\n".join(out)


def question(lines: list[str]) -> dict | None:
    """The dialog at the bottom of a Claude Code screen, or None."""
    filled = [i for i, line in enumerate(lines) if line.strip()]
    if not filled:
        return None
    last = filled[-1]
    if any(f in lines[last] for f in FOOTERS):
        # The dialog ends with its footer, the last thing on the screen.
        end = last
    elif NUMBERED.match(lines[last]) and any(lines[i].lstrip().startswith("❯") and NUMBERED.match(lines[i]) for i in filled[-8:]):
        # The review of the answers has no footer: its options end it.
        end = last + 1
    else:
        return None
    # Up to the question tabs, Claude's own output (⏺ ⎿ ✻) or the user's
    # last message (a "❯ " line that is not a numbered option).
    def top(line: str) -> bool:
        t = line.lstrip()
        return t.startswith(("←", "⏺", "⎿", "✻")) or (line.startswith("❯ ") and not NUMBERED.match(line))
    start = end
    while start > 0 and not top(lines[start - 1]) and end - start < 60:
        start -= 1
    # The tabs of several questions belong to the dialog.
    if start > 0 and lines[start - 1].lstrip().startswith("←"):
        start -= 1
    block = lines[start:end]
    options, current = [], None
    for line in block:
        m = NUMBERED.match(line)
        if m:
            label = m.group(3)
            box = BOX.match(label)
            option = {"n": int(m.group(2)), "label": box.group(2) if box else label}
            if box:
                option["ticked"] = box.group(1) not in (" ", "")
            if m.group(1):
                current = option["n"]
            options.append(option)
        elif options and line.startswith("      ") and line.strip() and not RULE.match(line) and line.strip() != "Submit":
            # A description under an option.
            options[-1].setdefault("about", line.strip())
    # The options that count: from the last "1." on.
    ones = [i for i, o in enumerate(options) if o["n"] == 1]
    if ones:
        options = options[ones[-1]:]
    text = [l.strip() for l in block if l.strip() and not RULE.match(l) and not NUMBERED.match(l)]
    tabs_line = next((l.strip() for l in block if l.strip().startswith("←") and l.strip().endswith("→")), "")
    tabs = [t.strip() for t in re.split(r"\s{2,}", tabs_line.strip("←→ ")) if t.strip()] if tabs_line else []
    review = any("Ready to submit your answers?" in l for l in block)
    if options:
        kind = "review" if review else "several" if any("ticked" in o for o in options) else "one"
        if any("Do you want to" in l for l in block):
            kind = "permission"
        # The question: the text line just before the options.
        first = next(i for i, l in enumerate(block) if NUMBERED.match(l))
        before = [l.strip() for l in block[:first] if l.strip() and not RULE.match(l) and not l.strip().startswith("←")]
        ask = before[-1] if before else ""
        details = before[:-1]
        for o in options:
            low = o["label"].lower().rstrip(".")
            if low in ("type something", "chat about this"):
                o["special"] = "text" if low == "type something" else "chat"
        return {"kind": kind, "question": ask, "details": details, "tabs": tabs, "options": options, "current": current}
    # A menu without numbers: the marked line and its neighbours.
    marked = [i for i, l in enumerate(block) if l.lstrip().startswith("❯ ")]
    if not marked:
        return None
    m = marked[-1]
    lo = m
    while lo > 0 and block[lo - 1].strip() and block[lo - 1].startswith(" ") and len(block[lo - 1]) - len(block[lo - 1].lstrip()) <= 4 and not RULE.match(block[lo - 1]):
        lo -= 1
    hi = m
    while hi + 1 < len(block) and block[hi + 1].strip() and not RULE.match(block[hi + 1]):
        hi += 1
    items = [block[i].replace("❯", "").strip() for i in range(lo, hi + 1)]
    # The line before the menu that says what it asks.
    before = [l.strip() for l in block[:lo] if l.strip() and not RULE.match(l)]
    options = [{"n": i + 1, "label": item} for i, item in enumerate(items)]
    return {"kind": "menu", "question": before[-1] if before else "", "details": before[:-1][-3:], "tabs": [],
            "options": options, "current": m - lo + 1}


def describe(q: dict | None) -> str:
    if not q:
        return "No question waits in this pane."
    kinds = {
        "one": "a question, pick one", "several": "a question, pick any number",
        "permission": "a permission prompt", "review": "the review of the answers", "menu": "a menu without numbers",
    }
    lines = [f"Claude Code asks ({kinds[q['kind']]}): {q['question']}"]
    if q["tabs"]:
        lines.append("Questions: " + " · ".join(q["tabs"]))
    for d in q["details"][-6:]:
        lines.append(f"  {d}")
    for o in q["options"]:
        tick = "" if "ticked" not in o else ("[x] " if o["ticked"] else "[ ] ")
        about = f" ({o['about']})" if o.get("about") and o["about"] != o["label"] else ""
        special = {"text": " — then type your own text", "chat": " — leaves the question to talk"}.get(o.get("special"), "")
        lines.append(f"  {o['n']}. {tick}{o['label']}{about}{special}")
    return "\n".join(lines)


def keys(host: str, pane: str, *names: str):
    for name in names:
        tmux(host, "send-keys", "-t", pane, name)
        time.sleep(0.25)


def answer(host: str, pane: str, choice: str) -> str:
    q = question(screen(host, pane))
    if not q:
        raise Fail("No question waits in this pane.")
    choice = choice.strip().lower()
    numbers = {o["n"] for o in q["options"]}
    if q["kind"] == "review" and choice in ("submit", "cancel"):
        wanted = next((o["n"] for o in q["options"] if o["label"].lower().startswith(choice)), None)
        if wanted is None:
            raise Fail(f"No {choice} option in the review.")
        picks = [wanted]
    else:
        try:
            picks = [int(x) for x in re.split(r"[,\s]+", choice) if x]
        except ValueError:
            raise Fail(f"Not option numbers: {choice}")
        if not picks or any(p not in numbers for p in picks):
            raise Fail(f"No option {choice}. {describe(q)}")
    if q["kind"] == "several":
        # Tick exactly the chosen ones, then go on to the next question.
        for o in q["options"]:
            if "ticked" in o and o["ticked"] != (o["n"] in picks):
                keys(host, pane, str(o["n"]))
        keys(host, pane, "Right")
    elif len(picks) != 1:
        raise Fail("This question takes one option.")
    elif q["kind"] == "menu":
        step = picks[0] - (q["current"] or 1)
        keys(host, pane, *(["Down"] * step if step > 0 else ["Up"] * -step), "Enter")
    else:
        keys(host, pane, str(picks[0]))
    time.sleep(0.8)
    chosen = ", ".join(o["label"] for o in q["options"] if o["n"] in picks)
    after = question(screen(host, pane))
    return f"Answered: {chosen}.\n" + (describe(after) if after else "No question waits any more.")


def kuterlapa(line: str, wait: float) -> str:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(wait)
        s.connect(os.path.join(STATE, "core.sock"))
        s.sendall((line.replace("\n", " ") + "\n").encode())
        data = b""
        while chunk := s.recv(4096):
            data += chunk
        return data.decode().strip()


def ask(host: str, pane: str) -> str:
    """Each question of the pane in the overlay, until none waits."""
    done = []
    for _ in range(12):
        q = question(screen(host, pane))
        if not q:
            break
        where = f"Claude Code · {host} {pane}"
        if q["kind"] == "review":
            # "● Which color?" and "→ Green" lines: "Which color? → Green".
            pairs, asked = [], None
            for d in q["details"]:
                if d.startswith("●"):
                    asked = d.lstrip("● ").strip()
                elif d.startswith("→") and asked:
                    pairs.append(f"{asked} → {d.lstrip('→ ').strip()}")
                    asked = None
            summary = "\n".join(pairs)
            request = {"kind": "question", "title": where, "tool": "", "command": summary,
                       "description": "Wysłać odpowiedzi do Claude Code?"}
            reply = kuterlapa("approve " + json.dumps(request), 150)
            done.append(answer(host, pane, "submit" if reply == "yes" else "cancel").split("\n")[0])
            continue
        shown = [o for o in q["options"] if not o.get("special")]
        labels = [o["label"] for o in shown]
        request = {"kind": "question", "title": where, "tool": "", "command": "\n".join(q["details"][-6:]),
                   "description": q["question"] or "Claude Code pyta", "options": labels,
                   "multiple": q["kind"] == "several"}
        if len(labels) > 6:
            return "\n".join(done + [f"Too many options for the overlay. {describe(q)}"])
        reply = kuterlapa("approve " + json.dumps(request), 150)
        if not reply.startswith("chose "):
            return "\n".join(done + ["The user cancelled; the question still waits in Claude Code."])
        # "chose 1,3: …": numbers in the overlay's list, which leaves out
        # the special options; back to Claude Code's numbers.
        picked = [i for i in reply[len("chose "):].split(":")[0].split(",") if i.strip()]
        numbers = [str(shown[int(i) - 1]["n"]) for i in picked]
        if q["kind"] == "several" and not numbers:
            for o in q["options"]:
                if o.get("ticked"):
                    keys(host, pane, str(o["n"]))
            keys(host, pane, "Right")
            done.append("Answered: none ticked.")
            continue
        done.append(answer(host, pane, ",".join(numbers)).split("\n")[0])
    return "\n".join(done) if done else "No question waits in this pane."


def send(host: str, pane: str, text: str) -> str:
    tmux(host, "send-keys", "-t", pane, "-l", "--", text)
    time.sleep(0.2)
    tmux(host, "send-keys", "-t", pane, "Enter")
    return f"Typed into {host} {pane} and pressed Enter."


def main(argv: list[str]) -> int:
    try:
        if len(argv) >= 2 and argv[0] == "panes":
            print(panes(argv[1]))
        elif len(argv) >= 3 and argv[0] == "read":
            count = int(argv[3]) if len(argv) > 3 and argv[3] else 40
            print("\n".join(screen(argv[1], argv[2], max(count, 1))[-count:]).strip("\n"))
        elif len(argv) >= 4 and argv[0] == "send":
            print(send(argv[1], argv[2], argv[3]))
        elif len(argv) >= 3 and argv[0] == "question":
            print(describe(question(screen(argv[1], argv[2]))))
        elif len(argv) >= 4 and argv[0] == "answer":
            print(answer(argv[1], argv[2], argv[3]))
        elif len(argv) >= 3 and argv[0] == "ask":
            print(ask(argv[1], argv[2]))
        else:
            print(__doc__.strip())
            return 2
        return 0
    except (Fail, subprocess.TimeoutExpired, OSError) as e:
        print(f"error: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
