"""Cut a stream of audio frames into pieces of speech."""

import numpy as np

RATE = 16000
FRAME = 480  # 30 ms

# Short pieces lose context and split quiet words, and Whisper then
# hallucinates. So a pause ends a piece only once the piece is long enough;
# a one-sentence dictation becomes one piece, transcribed when the user
# stops recording.
PAUSE_S = 1.0  # this much silence after speech can end a piece
MIN_PIECE_S = 8.0  # a pause does not end a shorter piece
MIN_SPEECH_S = 0.3  # pieces with less speech are noise
MAX_PIECE_S = 25.0  # cut a piece even without a pause
NOISE_MARGIN_DB = 10.0  # speech is this much louder than the noise floor
MIN_SPEECH_DBFS = -50.0
LEAD_IN_FRAMES = 10  # audio kept from before the first speech frame


def dbfs(frame):
    rms = float(np.sqrt(np.mean(frame * frame))) + 1e-9
    return 20 * np.log10(rms)


class Segmenter:
    """Splits 30 ms frames into pieces of speech.

    The noise floor follows the quiet frames, so steady background sound,
    such as a TV, raises the floor instead of counting as speech.
    """

    def __init__(self):
        self.floor = None
        self.piece = []
        self.speech_frames = 0
        self.silence_frames = 0
        self.last_was_speech = False

    def push(self, frame):
        """Add one frame. Return a finished piece, or None."""
        level = dbfs(frame)
        if self.floor is None:
            self.floor = level
        speech = level > max(self.floor + NOISE_MARGIN_DB, MIN_SPEECH_DBFS)
        self.last_was_speech = speech
        if not speech:
            # Follow the floor down fast and up slowly.
            rate = 0.3 if level < self.floor else 0.02
            self.floor += rate * (level - self.floor)

        if speech:
            self.piece.append(frame)
            self.speech_frames += 1
            self.silence_frames = 0
        elif self.speech_frames:
            self.piece.append(frame)
            self.silence_frames += 1
        else:
            self.piece.append(frame)
            del self.piece[:-LEAD_IN_FRAMES]

        frames_per_s = RATE / FRAME
        if (self.speech_frames and self.silence_frames >= PAUSE_S * frames_per_s
                and len(self.piece) >= MIN_PIECE_S * frames_per_s):
            return self.cut()
        if len(self.piece) >= MAX_PIECE_S * frames_per_s:
            return self.cut()
        return None

    def cut(self):
        """Return the current piece if it holds enough speech, and reset."""
        piece, speech = self.piece, self.speech_frames
        self.piece, self.speech_frames, self.silence_frames = [], 0, 0
        if speech * FRAME / RATE < MIN_SPEECH_S:
            return None
        return np.concatenate(piece)
