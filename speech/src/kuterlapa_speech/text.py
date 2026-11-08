"""Whisper's context, and fixes to the recognized text."""

import re
from pathlib import Path

PROMPT_CHARS = 600  # Whisper reads about 224 tokens; this is about 200
LAST_TEXT_CHARS = 150
MAX_REPEATS = 4  # a word repeated more often in a row means a hallucination

# Phrases Whisper invents on silence or noise; it learned them from video
# subtitles. A piece that is only one of these is dropped.
HALLUCINATIONS = re.compile(
    r"^\W*(dzięki za oglądanie|dziękuję za uwagę|napisy|subskrybuj|"
    r"zapraszam na|thanks for watching|thank you)\b.*$",
    re.IGNORECASE,
)


def read_lines(path):
    """Lines of a text file without comments (#) and empty lines."""
    try:
        lines = Path(path).read_text(encoding="utf-8").splitlines()
    except FileNotFoundError:
        return []
    return [s.strip() for s in lines if s.strip() and not s.strip().startswith("#")]


def build_prompt(intro, dictionaries, last_text):
    """Intro, then dictionary words in file order, then the last text.

    Only the end of the last text goes in: a long one fills the context,
    pushes out the words and makes Whisper hallucinate.
    TODO: pick the words that best match each piece instead of file order.
    """
    if len(last_text) > LAST_TEXT_CHARS:
        last_text = last_text[-LAST_TEXT_CHARS:].split(" ", 1)[-1]
    tail = f" {last_text}" if last_text else ""
    budget = PROMPT_CHARS - len(intro) - len(tail) - 2
    words, used = [], 0
    for path in dictionaries:
        for word in read_lines(path):
            if word in words:
                continue
            if used + len(word) + 2 > budget:
                break
            words.append(word)
            used += len(word) + 2
    listed = f" Słowa: {', '.join(words)}." if words else ""
    return f"{intro}{listed}{tail}"


def read_replacements(path):
    """Lines "wrong => right". Whole words, case-insensitive."""
    rules = []
    for line in read_lines(path):
        if "=>" not in line:
            continue
        wrong, right = (s.strip() for s in line.split("=>", 1))
        if wrong:
            rules.append((re.compile(rf"\b{re.escape(wrong)}\b", re.IGNORECASE), right))
    return rules


def clean(text, replacements_path):
    """Apply replacements; return "" for hallucinations and repetition loops."""
    if HALLUCINATIONS.match(text) and len(text) < 60:
        return ""
    words = text.split()
    run = 1
    for a, b in zip(words, words[1:]):
        run = run + 1 if a.strip(",.").lower() == b.strip(",.").lower() else 1
        if run > MAX_REPEATS:
            return ""
    for pattern, right in read_replacements(replacements_path):
        text = pattern.sub(right, text)
    return text
