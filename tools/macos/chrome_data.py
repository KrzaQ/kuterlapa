#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Search Google Chrome's bookmarks and history, for Kuterlapa's models.

    chrome_data.py bookmarks <query> [profile] [limit]
    chrome_data.py history <query> [profile] [limit]

The query comes from speech recognition, so it may be misheard: each word
matches a word of a title or address that contains it, or one that sounds
close enough ("alegro" finds "allegro"). Results list the profile, so the
model can open the page in it (chrome_open with that profile). Without a
profile, all profiles are searched.

Chrome keeps the history in SQLite and holds a lock on it while it runs;
the file is opened read-only as immutable, which needs no lock.
"""

import json
import re
import sqlite3
import sys
import unicodedata
from datetime import datetime, timedelta
from difflib import SequenceMatcher
from pathlib import Path

CHROME = Path.home() / "Library/Application Support/Google/Chrome"
# History rows read, newest first: enough for months, still fast.
HISTORY_ROWS = 50000


def norm(text: str) -> str:
    text = (text or "").lower().replace("ł", "l")
    text = unicodedata.normalize("NFD", text)
    text = "".join(c for c in text if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9]+", " ", text).strip()


def profiles() -> dict[str, str]:
    """Folder → the profile's name, as Chrome shows it."""
    try:
        state = json.loads((CHROME / "Local State").read_text())
        return {d: info.get("name", d) for d, info in state["profile"]["info_cache"].items()}
    except (OSError, KeyError, ValueError):
        return {"Default": "Default"}


def chosen(profile: str) -> dict[str, str]:
    all_ = profiles()
    if not profile:
        return all_
    wanted = norm(profile)
    picked = {d: n for d, n in all_.items() if norm(n) == wanted or norm(d) == wanted}
    if not picked:
        raise SystemExit(f"error: no Chrome profile {profile!r}; profiles: {', '.join(all_.values())}")
    return picked


def score(query: list[str], text: str) -> float:
    """How well the words of the query fit the text, from 0 to 1."""
    words = set(text.split())
    if not words:
        return 0.0
    total = 0.0
    for q in query:
        if q in text:
            total += 1.0
            continue
        best = 0.0
        for w in words:
            # Cheap checks first: most words are far off.
            if abs(len(w) - len(q)) > max(2, len(q) // 3):
                continue
            m = SequenceMatcher(None, q, w)
            if m.real_quick_ratio() >= 0.75 and m.quick_ratio() >= 0.75:
                best = max(best, m.ratio())
        total += best if best >= 0.75 else 0.0
    return total / len(query)


def bookmarks(folder: str):
    try:
        data = json.loads((CHROME / folder / "Bookmarks").read_text())
    except (OSError, ValueError):
        return
    def walk(node, path):
        if node.get("type") == "url":
            yield node.get("name", ""), node.get("url", ""), " / ".join(path)
        for child in node.get("children", []):
            yield from walk(child, path + ([node["name"]] if node.get("type") == "folder" and node.get("name") else []))
    for root in data.get("roots", {}).values():
        if isinstance(root, dict):
            yield from walk(root, [])


def history(folder: str):
    path = CHROME / folder / "History"
    if not path.exists():
        return
    db = sqlite3.connect(f"file:{path}?immutable=1", uri=True)
    try:
        rows = db.execute(
            "SELECT title, url, visit_count, last_visit_time FROM urls ORDER BY last_visit_time DESC LIMIT ?",
            (HISTORY_ROWS,),
        ).fetchall()
    except sqlite3.Error:
        return
    finally:
        db.close()
    epoch = datetime(1601, 1, 1)
    for title, url, count, last in rows:
        when = (epoch + timedelta(microseconds=last)).strftime("%Y-%m-%d") if last else ""
        yield title or "", url, count, when


def search(kind: str, query: str, profile: str, limit: int) -> str:
    words = norm(query).split()
    if not words:
        return "error: give words to look for"
    found = []
    for folder, name in chosen(profile).items():
        items = bookmarks(folder) if kind == "bookmarks" else history(folder)
        for item in items:
            title, url = item[0], item[1]
            s = score(words, norm(f"{title} {url}"))
            if s >= 0.5:
                found.append((s, name, item))
    # The best fit first; in history, among equal ones, the most visited.
    found.sort(key=lambda f: (-f[0], -(f[2][2] if kind == "history" else 0)))
    if not found:
        return f"No {kind} like {query!r}."
    lines = []
    for s, name, item in found[:limit]:
        if kind == "bookmarks":
            title, url, path = item
            where = f" [{path}]" if path else ""
            lines.append(f"{title} | {url} | profile {name}{where}")
        else:
            title, url, count, when = item
            lines.append(f"{title} | {url} | profile {name} | {count} visits, last {when}")
    more = f"\n({len(found) - limit} more; give more words)" if len(found) > limit else ""
    return "\n".join(lines) + more


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:]]
    if len(args) < 2 or args[0] not in ("bookmarks", "history"):
        print(__doc__.strip())
        return 2
    kind, query = args[0], args[1]
    profile = args[2] if len(args) > 2 else ""
    try:
        limit = max(1, min(int(args[3]), 100)) if len(args) > 3 and args[3] else 20
    except ValueError:
        limit = 20
    print(search(kind, query, profile, limit))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
