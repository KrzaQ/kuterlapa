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


# Words from the context that are worth adding to the prompt: names with a
# capital letter, a digit or a hyphen inside, such as WhatsApp, iTerm or
# Hex-Rays. Plain words Whisper knows anyway.
NAME = re.compile(r"[\w-]{3,30}")


def is_name(word):
    """WhatsApp, iTerm2, Hex-Rays, IDA: a capital inside, a digit, a hyphen
    between letters, or all capitals. Not just a capital at the start."""
    inner = word[1:]
    return (
        any(c.isupper() for c in inner) and any(c.islower() for c in word)
        or any(c.isdigit() for c in word) and any(c.isalpha() for c in word)
        or "-" in word.strip("-") and word.replace("-", "").isalpha()
        or word.isupper() and word.isalpha() and len(word) >= 2
    )
CONTEXT_NAMES = 8


def context_names(context):
    """Unusual names in the context, at most CONTEXT_NAMES, first ones first."""
    names = []
    for match in NAME.finditer(context):
        word = match.group(0).strip("-")
        if len(word) >= 3 and is_name(word) and word not in names:
            names.append(word)
        if len(names) >= CONTEXT_NAMES:
            break
    return names


def build_prompt(intro, dictionaries, last_text, context=""):
    """Intro, then words, then the last text.

    The words: dictionary words that appear in the context (the window in
    front, the last messages, the session) first, then names from the
    context, then the other dictionary words in file order, as many as
    fit. Only the end of the last text goes in: a long one fills the
    context, pushes out the words and makes Whisper hallucinate.
    """
    if len(last_text) > LAST_TEXT_CHARS:
        last_text = last_text[-LAST_TEXT_CHARS:].split(" ", 1)[-1]
    tail = f" {last_text}" if last_text else ""
    budget = PROMPT_CHARS - len(intro) - len(tail) - 2
    known = []
    for path in dictionaries:
        for word in read_lines(path):
            if word not in known:
                known.append(word)
    lower = context.lower()
    in_context = [w for w in known if w.lower() in lower]
    names = [n for n in context_names(context) if n.lower() not in (w.lower() for w in known)]
    rest = [w for w in known if w not in in_context]
    words, used = [], 0
    for word in in_context + names + rest:
        if used + len(word) + 2 > budget:
            break
        words.append(word)
        used += len(word) + 2
    listed = f" Słowa: {', '.join(words)}." if words else ""
    return f"{intro}{listed}{tail}"


def read_replacements(path):
    """Lines "wrong => right". Whole words, case-insensitive, and the
    hyphens of the wrong form may stand anywhere or nowhere: Whisper
    writes the same misheard word as "ITR-ma" one day and "IT-RMA" the
    next."""
    rules = []
    for line in read_lines(path):
        if "=>" not in line:
            continue
        wrong, right = (s.strip() for s in line.split("=>", 1))
        letters = wrong.replace("-", "")
        if letters:
            loose = "-?".join(re.escape(c) for c in letters)
            rules.append((re.compile(rf"\b{loose}\b", re.IGNORECASE), right))
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
