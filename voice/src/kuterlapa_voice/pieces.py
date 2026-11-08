"""Split text into pieces to synthesize one after another."""

import re

SENTENCE_END = re.compile(r"(?<=[.!?…])\s+")


def split(text, first_chars, piece_chars):
    """Whole sentences, joined into pieces. The first piece is short, so
    that speech starts soon; the rest can be longer, because each one is
    made while the previous one plays. A sentence longer than the limit
    stays whole: cutting it would break the intonation.
    """
    sentences = [s.strip() for s in SENTENCE_END.split(text.strip()) if s.strip()]
    pieces, current = [], ""
    for sentence in sentences:
        limit = first_chars if not pieces else piece_chars
        if current and len(current) + 1 + len(sentence) > limit:
            pieces.append(current)
            current = sentence
        else:
            current = f"{current} {sentence}".strip()
    if current:
        pieces.append(current)
    return pieces
